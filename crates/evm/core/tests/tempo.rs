use alloy_evm::{Evm, EvmEnv, FromRecoveredTx};
use alloy_primitives::{Address, B256, Bytes, TxKind, U256, bytes};
use alloy_signer::Signer;
use alloy_signer_local::PrivateKeySigner;
use alloy_sol_types::SolCall;
use foundry_evm_core::{
    backend::Backend,
    evm::{FoundryEvmFactory, TempoEvmNetwork},
    fork::MultiFork,
};
use revm::{
    DatabaseRef, Inspector,
    context::result::{EVMError, ExecutionResult, HaltReason, InvalidTransaction},
    inspector::NoOpInspector,
    state::{AccountInfo, Bytecode},
};
use tempo_alloy::primitives::TempoTxEnvelope;
use tempo_evm::{TempoBlockEnv, TempoEvmFactory};
use tempo_hardfork::TempoHardfork;
use tempo_precompiles::{
    ACCOUNT_KEYCHAIN_ADDRESS, DEFAULT_FEE_TOKEN,
    account_keychain::{
        KeyRestrictions, SignatureType as KeychainSignatureType, TokenLimit as KeychainTokenLimit,
        authorizeKeyCall,
    },
    storage::{StorageActions, StorageCtx},
    storage_credits::StorageCredits,
    tip20::{ITIP20, TIP20Token},
};
use tempo_primitives::{
    AASigned, TempoSignature, TempoTransaction,
    transaction::{Call, KeychainSignature, PrimitiveSignature, calc_gas_balance_spending},
};
use tempo_revm::{TempoInvalidTransaction, TempoTxEnv, gas_params::tempo_gas_params};

const GAS_LIMIT: u64 = 500_000;
const GAS_PRICE: u128 = 1_000_000_000_000;
const TRANSFER_AMOUNT: u64 = 1_234;

#[tokio::test]
async fn nested_tempo_execution_preserves_halt_reason() {
    let account = PrivateKeySigner::random();
    let target = Address::repeat_byte(0xee);

    for (opcode, expected) in
        [(0x0c, HaltReason::OpcodeNotFound), (0x50, HaltReason::StackUnderflow)]
    {
        let mut db = in_memory_tempo_backend();
        seed_fee_token_balances(&mut db, account.address(), target);
        db.insert_account_info(
            target,
            AccountInfo::default().with_code(Bytecode::new_raw(Bytes::from(vec![opcode]))),
        );
        let evm_env = EvmEnv::new(
            revm::context::CfgEnv::<TempoHardfork>::default()
                .with_spec_and_gas_params(TempoHardfork::T11, tempo_gas_params(TempoHardfork::T11)),
            TempoBlockEnv::default(),
        );
        let mut evm = TempoEvmFactory::default().create_nested_evm(&mut db, evm_env);
        let tx = primitive_tx(
            &account,
            TempoTransaction {
                chain_id: 1,
                fee_token: Some(DEFAULT_FEE_TOKEN),
                gas_limit: GAS_LIMIT,
                calls: vec![Call {
                    to: TxKind::Call(target),
                    value: U256::ZERO,
                    input: Bytes::new(),
                }],
                ..Default::default()
            },
        )
        .await;

        let result = evm.transact_raw(tx).expect("nested transaction executes");
        assert!(
            matches!(result.result, ExecutionResult::Halt { reason, .. } if reason == expected)
        );
    }
}

fn in_memory_tempo_backend() -> Backend<TempoEvmNetwork> {
    let (forks, _fork_handler) = MultiFork::new();
    Backend::new(forks, None).unwrap()
}

fn seed_fee_token_balances(
    db: &mut Backend<TempoEvmNetwork>,
    account: Address,
    recipient: Address,
) {
    db.insert_account_info(
        account,
        AccountInfo { balance: U256::from(1_000_000_000_000_000_000u128), ..Default::default() },
    );
    let initial_balance =
        calc_gas_balance_spending(GAS_LIMIT, GAS_PRICE) + U256::from(TRANSFER_AMOUNT);
    let token = TIP20Token::from_address_unchecked(DEFAULT_FEE_TOKEN);
    db.insert_account_storage(DEFAULT_FEE_TOKEN, token.balances[account].slot(), initial_balance)
        .unwrap();
    db.insert_account_storage(DEFAULT_FEE_TOKEN, token.balances[recipient].slot(), U256::ONE)
        .unwrap();
}

fn storage_credit_balance<DB, I>(evm: &mut tempo_evm::evm::TempoEvm<DB, I>, owner: Address) -> u64
where
    DB: alloy_evm::Database,
    I: Inspector<tempo_revm::evm::TempoContext<DB>>,
{
    let ctx = evm.ctx_mut();
    StorageCtx::enter_evm(
        &mut ctx.journaled_state,
        &ctx.block,
        &ctx.cfg,
        &ctx.tx,
        StorageActions::disabled(),
        || StorageCredits::new().balance_of(owner).unwrap(),
    )
}

async fn primitive_tx(account: &PrivateKeySigner, tx: TempoTransaction) -> TempoTxEnv {
    let signature = account.sign_hash(&tx.signature_hash()).await.unwrap();
    let envelope = TempoTxEnvelope::AA(AASigned::new_unhashed(
        tx,
        TempoSignature::Primitive(PrimitiveSignature::Secp256k1(signature)),
    ));
    TempoTxEnv::from_recovered_tx(&envelope, account.address())
}

async fn keychain_spend_setup() -> (Address, Address, TempoTxEnv, TempoTxEnv) {
    let root = PrivateKeySigner::random();
    let access_key = PrivateKeySigner::random();
    let recipient = Address::repeat_byte(0xee);
    let spending_limit =
        calc_gas_balance_spending(GAS_LIMIT, GAS_PRICE) + U256::from(TRANSFER_AMOUNT);

    let authorize = authorizeKeyCall {
        keyId: access_key.address(),
        signatureType: KeychainSignatureType::Secp256k1,
        config: KeyRestrictions {
            expiry: u64::MAX,
            enforceLimits: true,
            limits: vec![KeychainTokenLimit {
                token: DEFAULT_FEE_TOKEN,
                amount: spending_limit,
                period: 0,
            }],
            allowAnyCalls: true,
            allowedCalls: vec![],
        },
    };
    let auth_tx = primitive_tx(
        &root,
        TempoTransaction {
            chain_id: 1,
            fee_token: Some(DEFAULT_FEE_TOKEN),
            max_priority_fee_per_gas: 0,
            max_fee_per_gas: 0,
            gas_limit: 2_000_000,
            calls: vec![Call {
                to: TxKind::Call(ACCOUNT_KEYCHAIN_ADDRESS),
                value: U256::ZERO,
                input: authorize.abi_encode().into(),
            }],
            access_list: Default::default(),
            nonce_key: U256::ZERO,
            nonce: 0,
            fee_payer_signature: None,
            valid_before: None,
            valid_after: None,
            key_authorization: None,
            tempo_authorization_list: vec![],
        },
    )
    .await;

    let spend_tx = TempoTransaction {
        chain_id: 1,
        fee_token: Some(DEFAULT_FEE_TOKEN),
        max_priority_fee_per_gas: GAS_PRICE,
        max_fee_per_gas: GAS_PRICE,
        gas_limit: GAS_LIMIT,
        calls: vec![Call {
            to: TxKind::Call(DEFAULT_FEE_TOKEN),
            value: U256::ZERO,
            input: ITIP20::transferCall { to: recipient, amount: U256::from(TRANSFER_AMOUNT) }
                .abi_encode()
                .into(),
        }],
        access_list: Default::default(),
        nonce_key: U256::ZERO,
        nonce: 1,
        fee_payer_signature: None,
        valid_before: None,
        valid_after: None,
        key_authorization: None,
        tempo_authorization_list: vec![],
    };
    let keychain_hash = KeychainSignature::signing_hash(spend_tx.signature_hash(), root.address());
    let access_signature = access_key.sign_hash(&keychain_hash).await.unwrap();
    let envelope = TempoTxEnvelope::AA(AASigned::new_unhashed(
        spend_tx,
        TempoSignature::Keychain(KeychainSignature::new(
            root.address(),
            PrimitiveSignature::Secp256k1(access_signature),
        )),
    ));
    let spend_env = TempoTxEnv::from_recovered_tx(&envelope, root.address());
    (spend_env.inner.caller, recipient, auth_tx, spend_env)
}

#[tokio::test]
async fn foundry_factory_keychain_limit_refund_does_not_leak_storage_credit() {
    let (account, recipient, auth_tx, spend_tx) = keychain_spend_setup().await;
    let mut db = in_memory_tempo_backend();
    seed_fee_token_balances(&mut db, account, recipient);

    let evm_env = EvmEnv::new(
        revm::context::CfgEnv::<TempoHardfork>::default()
            .with_spec_and_gas_params(TempoHardfork::T7, tempo_gas_params(TempoHardfork::T7)),
        TempoBlockEnv::default(),
    );
    let mut evm = TempoEvmFactory::default().create_foundry_evm_with_inspector(
        &mut db,
        evm_env,
        NoOpInspector,
    );

    let auth_result = evm.transact_commit(auth_tx).expect("auth transaction executes");
    assert!(auth_result.is_success(), "auth transaction should succeed: {auth_result:?}");

    let spend_result = evm.transact_commit(spend_tx).expect("spend transaction executes");
    assert!(spend_result.is_success(), "keychain spend should succeed: {spend_result:?}");
    assert_eq!(
        storage_credit_balance(&mut evm, ACCOUNT_KEYCHAIN_ADDRESS),
        0,
        "post-tx fee refund recreates the keychain spending-limit slot, so the same-tx clear credit must be canceled",
    );
}

#[tokio::test]
async fn tempo_batch_revert_settles_fees_and_nonce_without_publishing_call_state() {
    let signer = PrivateKeySigner::from_bytes(&B256::repeat_byte(1)).unwrap();
    let writer = Address::repeat_byte(0xdd);
    let reverter = Address::repeat_byte(0xee);
    for &spec in TempoHardfork::VARIANTS {
        let mut db = in_memory_tempo_backend();
        seed_fee_token_balances(&mut db, signer.address(), reverter);
        db.insert_account_info(
            writer,
            AccountInfo::default().with_code(Bytecode::new_raw(bytes!("600160005500"))),
        );
        db.insert_account_info(
            reverter,
            AccountInfo::default()
                .with_code(Bytecode::new_raw(bytes!("63deadbeef6000526004601cfd"))),
        );
        let slot =
            TIP20Token::from_address_unchecked(DEFAULT_FEE_TOKEN).balances[signer.address()].slot();
        db.insert_account_storage(DEFAULT_FEE_TOKEN, slot, U256::from(6_000_000)).unwrap();
        let initial_tokens = db.storage_ref(DEFAULT_FEE_TOKEN, slot).unwrap();
        let initial_native = db.basic_ref(signer.address()).unwrap().unwrap().balance;
        let env = EvmEnv::new(
            revm::context::CfgEnv::<TempoHardfork>::default()
                .with_spec_and_gas_params(spec, tempo_gas_params(spec)),
            TempoBlockEnv::default(),
        );
        let mut evm = TempoEvmFactory::default().create_foundry_evm_with_inspector(
            &mut db,
            env,
            NoOpInspector,
        );
        let call =
            |target| Call { to: TxKind::Call(target), value: U256::ZERO, input: Bytes::new() };
        let mut tx = TempoTransaction {
            chain_id: 1,
            fee_token: Some(DEFAULT_FEE_TOKEN),
            gas_limit: 2_000_000,
            max_fee_per_gas: GAS_PRICE,
            max_priority_fee_per_gas: GAS_PRICE,
            calls: vec![call(writer), call(reverter)],
            ..Default::default()
        };
        let result = evm.transact_commit(primitive_tx(&signer, tx.clone()).await).unwrap();
        assert!(
            matches!(&result, ExecutionResult::Revert { output, .. } if *output == bytes!("deadbeef")),
            "{spec:?}: {result:?}"
        );
        // GAS_PRICE is 10^12 attodollars: one charged gas equals one token base unit.
        let fee = U256::from(result.tx_gas_used());
        assert!(fee > U256::ZERO);
        let db = &mut evm.ctx_mut().journaled_state.database;
        assert_eq!(db.storage(writer, U256::ZERO).unwrap(), U256::ZERO, "{spec:?}");
        assert_eq!(db.basic(signer.address()).unwrap().unwrap().nonce, 1, "{spec:?}");
        assert_eq!(db.basic(signer.address()).unwrap().unwrap().balance, initial_native);
        assert_eq!(db.storage(DEFAULT_FEE_TOKEN, slot).unwrap(), initial_tokens - fee, "{spec:?}");

        // Admission failure must not consume another nonce, fee, or application write.
        tx.nonce = 7;
        tx.calls = vec![call(writer)];
        let error = evm.transact_commit(primitive_tx(&signer, tx.clone()).await).unwrap_err();
        assert!(
            matches!(
                error,
                EVMError::Transaction(TempoInvalidTransaction::EthInvalidTransaction(
                    InvalidTransaction::NonceTooHigh { tx: 7, state: 1 }
                ))
            ),
            "{spec:?}: {error:?}"
        );
        let db = &mut evm.ctx_mut().journaled_state.database;
        assert_eq!(db.basic(signer.address()).unwrap().unwrap().nonce, 1);
        assert_eq!(db.storage(DEFAULT_FEE_TOKEN, slot).unwrap(), initial_tokens - fee);
        assert_eq!(db.storage(writer, U256::ZERO).unwrap(), U256::ZERO);

        // The same engine must remain usable after both EVM revert and admission error.
        tx.nonce = 1;
        let next = evm.transact_commit(primitive_tx(&signer, tx).await).unwrap();
        assert!(next.is_success(), "{spec:?}: {next:?}");
        let next_fee = U256::from(next.tx_gas_used());
        let db = &mut evm.ctx_mut().journaled_state.database;
        assert_eq!(db.basic(signer.address()).unwrap().unwrap().nonce, 2);
        assert_eq!(db.storage(writer, U256::ZERO).unwrap(), U256::ONE);
        assert_eq!(db.storage(DEFAULT_FEE_TOKEN, slot).unwrap(), initial_tokens - fee - next_fee);
    }
}
