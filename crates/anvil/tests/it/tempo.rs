//! Tests for Tempo-specific features in Anvil.
//!
//! This module tests Tempo's payment-native protocol features including:
//! - TIP20 fee tokens (PathUSD, AlphaUSD, BetaUSD, ThetaUSD)
//! - Tempo precompiles initialization (sentinel bytecode)
//! - Native value transfer rejection
//! - Basic transaction behavior in Tempo mode

use alloy_eips::eip2718::Encodable2718;
use alloy_network::{ReceiptResponse, TransactionBuilder};
use alloy_primitives::{Address, Bytes, TxKind, U256};
use alloy_provider::Provider;
use alloy_rpc_types::{BlockNumberOrTag, TransactionRequest};
use alloy_serde::WithOtherFields;
use alloy_signer::Signer;
use alloy_signer_local::PrivateKeySigner;
use alloy_sol_types::sol;
use anvil::{NodeConfig, spawn};
use foundry_evm::core::tempo::{
    ALPHA_USD_ADDRESS, BETA_USD_ADDRESS, PATH_USD_ADDRESS, TEMPO_PRECOMPILE_ADDRESSES,
    TEMPO_TIP20_TOKENS, THETA_USD_ADDRESS,
};
use tempo_alloy::primitives::TempoTxEnvelope;
use tempo_primitives::{
    AASigned, TempoSignature, TempoTransaction,
    transaction::{Call, PrimitiveSignature},
};

const PATH_USD: Address = PATH_USD_ADDRESS;
const ALPHA_USD: Address = ALPHA_USD_ADDRESS;
const BETA_USD: Address = BETA_USD_ADDRESS;
const THETA_USD: Address = THETA_USD_ADDRESS;

/// Gas limit for TIP20 transfer calls (precompile interactions need more gas).
const TIP20_TRANSFER_GAS: u64 = 300_000;

sol! {
    #[sol(rpc)]
    interface IERC20 {
        function name() external view returns (string memory);
        function symbol() external view returns (string memory);
        function decimals() external view returns (uint8);
        function totalSupply() external view returns (uint256);
        function balanceOf(address account) external view returns (uint256);
        function transfer(address to, uint256 amount) external returns (bool);
        function allowance(address owner, address spender) external view returns (uint256);
        function approve(address spender, uint256 amount) external returns (bool);
        function transferFrom(address from, address to, uint256 amount) external returns (bool);
    }
}

// ============================================================================
// Tempo Genesis: Precompile Initialization
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_tempo_precompiles_have_code() {
    let (api, _handle) = spawn(NodeConfig::test_tempo()).await;

    // Tempo precompiles should have sentinel bytecode (0xef)
    for addr in TEMPO_PRECOMPILE_ADDRESSES {
        let code = api.get_code(*addr, None).await.unwrap();
        assert!(!code.is_empty(), "Precompile {addr} should have code");
    }

    // All TIP20 token addresses should also have code
    for addr in TEMPO_TIP20_TOKENS {
        let code = api.get_code(*addr, None).await.unwrap();
        assert!(!code.is_empty(), "Token {addr} should have code deployed");
    }
}

// ============================================================================
// Tempo Genesis: Fee Token Metadata
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_tip20_token_metadata() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let tokens = [
        (PATH_USD, "PathUSD", "PathUSD"),
        (ALPHA_USD, "AlphaUSD", "AlphaUSD"),
        (BETA_USD, "BetaUSD", "BetaUSD"),
        (THETA_USD, "ThetaUSD", "ThetaUSD"),
    ];

    for (addr, expected_name, expected_symbol) in tokens {
        let token = IERC20::new(addr, &provider);
        let name = token.name().call().await.unwrap();
        let symbol = token.symbol().call().await.unwrap();
        let decimals = token.decimals().call().await.unwrap();

        assert_eq!(name, expected_name, "Token at {addr} name mismatch");
        assert_eq!(symbol, expected_symbol, "Token at {addr} symbol mismatch");
        assert_eq!(decimals, 6, "All TIP20 tokens should use 6 decimals");
    }
}

// ============================================================================
// Tempo Genesis: Fee Token Balances
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_fee_token_balances_minted_to_dev_accounts() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let dev_accounts: Vec<Address> = handle.dev_accounts().collect();
    assert!(!dev_accounts.is_empty());

    for account in dev_accounts.iter().take(3) {
        for token_addr in [PATH_USD, ALPHA_USD, BETA_USD, THETA_USD] {
            let token = IERC20::new(token_addr, &provider);
            let balance = token.balanceOf(*account).call().await.unwrap();
            assert!(
                balance > U256::ZERO,
                "Account {account} should have {token_addr} balance, got 0"
            );
        }
    }
}

// ============================================================================
// Tempo Genesis: Dev Account Balance
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_dev_accounts_have_balance() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let genesis_balance = handle.genesis_balance();

    for account in handle.dev_accounts() {
        let balance = provider.get_balance(account).await.unwrap();
        assert_eq!(balance, genesis_balance, "Dev account {account} should have genesis balance");
    }
}

// ============================================================================
// TIP20 Token Operations: Transfer
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_tip20_transfer() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();
    let sender = accounts[0];
    let recipient = accounts[1];

    let token = IERC20::new(PATH_USD, &provider);

    let sender_balance_before = token.balanceOf(sender).call().await.unwrap();
    let recipient_balance_before = token.balanceOf(recipient).call().await.unwrap();

    let transfer_amount = U256::from(1_000_000);
    let transfer_call = token.transfer(recipient, transfer_amount);
    let calldata: Bytes = transfer_call.calldata().clone();

    let tx = TransactionRequest::default()
        .from(sender)
        .to(PATH_USD)
        .with_input(calldata)
        .with_gas_limit(TIP20_TRANSFER_GAS);

    let tx = WithOtherFields::new(tx);
    let receipt = provider.send_transaction(tx).await.unwrap().get_receipt().await.unwrap();
    assert!(receipt.status());

    let sender_balance_after = token.balanceOf(sender).call().await.unwrap();
    let recipient_balance_after = token.balanceOf(recipient).call().await.unwrap();

    assert_eq!(
        sender_balance_before - transfer_amount,
        sender_balance_after,
        "Sender balance should decrease by transfer amount"
    );
    assert_eq!(
        recipient_balance_before + transfer_amount,
        recipient_balance_after,
        "Recipient balance should increase by transfer amount"
    );
}

// ============================================================================
// TIP20 Token Operations: Approve and TransferFrom
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_tip20_approve_and_transfer_from() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();
    let owner = accounts[0];
    let spender = accounts[1];
    let recipient = accounts[2];

    let token = IERC20::new(BETA_USD, &provider);

    // Owner approves spender
    let approve_amount = U256::from(5_000_000);
    let approve_call = token.approve(spender, approve_amount);
    let calldata: Bytes = approve_call.calldata().clone();

    let tx = TransactionRequest::default()
        .from(owner)
        .to(BETA_USD)
        .with_input(calldata)
        .with_gas_limit(TIP20_TRANSFER_GAS);

    let tx = WithOtherFields::new(tx);
    provider.send_transaction(tx).await.unwrap().get_receipt().await.unwrap();

    let allowance = token.allowance(owner, spender).call().await.unwrap();
    assert_eq!(allowance, approve_amount);

    // Spender transfers from owner to recipient
    let transfer_amount = U256::from(2_000_000);
    let transfer_from_call = token.transferFrom(owner, recipient, transfer_amount);
    let calldata: Bytes = transfer_from_call.calldata().clone();

    let recipient_balance_before = token.balanceOf(recipient).call().await.unwrap();

    let tx = TransactionRequest::default()
        .from(spender)
        .to(BETA_USD)
        .with_input(calldata)
        .with_gas_limit(TIP20_TRANSFER_GAS);

    let tx = WithOtherFields::new(tx);
    let receipt = provider.send_transaction(tx).await.unwrap().get_receipt().await.unwrap();
    assert!(receipt.status());

    let recipient_balance_after = token.balanceOf(recipient).call().await.unwrap();
    assert_eq!(recipient_balance_before + transfer_amount, recipient_balance_after);

    let allowance_after = token.allowance(owner, spender).call().await.unwrap();
    assert_eq!(allowance_after, approve_amount - transfer_amount);
}

// ============================================================================
// TIP20 Token Operations: Total Supply
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_tip20_total_supply() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let token = IERC20::new(PATH_USD, &provider);
    let total_supply = token.totalSupply().call().await.unwrap();

    assert!(total_supply > U256::ZERO, "Total supply should be non-zero");
}

// ============================================================================
// TIP20 Token Operations: Transfer Between Different Fee Tokens
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_transfer_between_different_fee_tokens() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();
    let sender = accounts[0];
    let recipient = accounts[1];

    for token_addr in [PATH_USD, ALPHA_USD, BETA_USD, THETA_USD] {
        let token = IERC20::new(token_addr, &provider);
        let balance_before = token.balanceOf(recipient).call().await.unwrap();

        let transfer_amount = U256::from(100_000);
        let transfer_call = token.transfer(recipient, transfer_amount);
        let calldata: Bytes = transfer_call.calldata().clone();

        let tx = TransactionRequest::default()
            .from(sender)
            .to(token_addr)
            .with_input(calldata)
            .with_gas_limit(TIP20_TRANSFER_GAS);

        let tx = WithOtherFields::new(tx);
        let receipt = provider.send_transaction(tx).await.unwrap().get_receipt().await.unwrap();
        assert!(receipt.status(), "Transfer for {token_addr} failed");

        let balance_after = token.balanceOf(recipient).call().await.unwrap();
        assert_eq!(balance_after, balance_before + transfer_amount);
    }
}

// ============================================================================
// TIP20 Token Operations: All Fee Tokens Have Correct Metadata
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_all_fee_tokens_have_correct_metadata() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let tokens = [
        (PATH_USD, "PathUSD"),
        (ALPHA_USD, "AlphaUSD"),
        (BETA_USD, "BetaUSD"),
        (THETA_USD, "ThetaUSD"),
    ];

    for (addr, expected_name) in tokens {
        let token = IERC20::new(addr, &provider);
        let name = token.name().call().await.unwrap();
        let decimals = token.decimals().call().await.unwrap();

        assert_eq!(name, expected_name, "Token at {addr} should be named {expected_name}");
        assert_eq!(decimals, 6, "All TIP20 tokens use 6 decimals");
    }
}

// ============================================================================
// TIP20 Token Operations: Transfer Emits Event
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_transfer_emits_event() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();
    let from = accounts[0];
    let to = accounts[1];

    let token = IERC20::new(ALPHA_USD, &provider);
    let transfer_amount = U256::from(1_000_000);
    let transfer_call = token.transfer(to, transfer_amount);
    let calldata: Bytes = transfer_call.calldata().clone();

    let tx = TransactionRequest::default()
        .from(from)
        .to(ALPHA_USD)
        .with_input(calldata)
        .with_gas_limit(TIP20_TRANSFER_GAS);

    let tx = WithOtherFields::new(tx);
    let receipt = provider.send_transaction(tx).await.unwrap().get_receipt().await.unwrap();

    assert!(!receipt.inner.logs().is_empty(), "Transfer should emit event");

    let log = &receipt.inner.logs()[0];
    assert_eq!(log.address(), ALPHA_USD);

    let transfer_topic =
        alloy_primitives::keccak256("Transfer(address,address,uint256)".as_bytes());
    assert_eq!(log.topics()[0], transfer_topic);
}

// ============================================================================
// Tempo Transactions: Native Value Transfer Rejected
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_native_value_transfer_rejected() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();
    let from = accounts[0];
    let to = accounts[1];

    let tx = TransactionRequest::default()
        .from(from)
        .to(to)
        .value(U256::from(1_000_000_000_000_000_000u64)); // 1 ETH

    let tx = WithOtherFields::new(tx);
    let result = provider.send_transaction(tx).await;
    assert!(result.is_err(), "Native ETH transfers should be rejected in Tempo mode");

    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("native value transfer not allowed"),
        "Expected 'native value transfer not allowed' error, got: {err}"
    );
}

// ============================================================================
// Tempo Transactions: Zero-Value EIP-1559 Tx Succeeds
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_zero_value_tx_succeeds() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();
    let sender = accounts[0];
    let recipient = accounts[1];

    // TIP20 transfer (value=0, only calldata)
    let token = IERC20::new(PATH_USD, &provider);
    let transfer_call = token.transfer(recipient, U256::from(1_000_000));
    let calldata: Bytes = transfer_call.calldata().clone();

    let tx = TransactionRequest::default()
        .from(sender)
        .to(PATH_USD)
        .with_input(calldata)
        .with_gas_limit(TIP20_TRANSFER_GAS);

    let tx = WithOtherFields::new(tx);
    let receipt = provider.send_transaction(tx).await.unwrap().get_receipt().await.unwrap();
    assert!(receipt.status());
}

// ============================================================================
// Tempo Transactions: Contract Deployment
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_contract_deployment() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();
    let sender = accounts[0];

    // Minimal contract: PUSH1 0x00 PUSH1 0x00 RETURN (returns empty)
    let bytecode = Bytes::from(vec![0x60, 0x00, 0x60, 0x00, 0xf3]);

    let tx =
        TransactionRequest::default().from(sender).with_input(bytecode).with_gas_limit(100_000);

    let tx = WithOtherFields::new(tx);
    let receipt = provider.send_transaction(tx).await.unwrap().get_receipt().await.unwrap();
    assert!(receipt.status());
    assert!(receipt.contract_address.is_some(), "Should have deployed a contract");
}

// ============================================================================
// Tempo Transactions: Nonce Increments
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_nonce_increments() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();
    let from = accounts[0];
    let to = accounts[1];

    let nonce_before = provider.get_transaction_count(from).await.unwrap();
    assert_eq!(nonce_before, 0);

    // Send a TIP20 transfer
    let token = IERC20::new(ALPHA_USD, &provider);
    let transfer_call = token.transfer(to, U256::from(1000));
    let calldata: Bytes = transfer_call.calldata().clone();

    let tx = TransactionRequest::default()
        .from(from)
        .to(ALPHA_USD)
        .with_input(calldata)
        .with_gas_limit(TIP20_TRANSFER_GAS);

    let tx = WithOtherFields::new(tx);
    provider.send_transaction(tx).await.unwrap().get_receipt().await.unwrap();

    let nonce_after = provider.get_transaction_count(from).await.unwrap();
    assert_eq!(nonce_after, 1);
}

// ============================================================================
// Tempo AA Transaction Tests (Type 0x76)
// ============================================================================

/// Helper to get the private key for a dev account.
fn dev_key(index: u32) -> PrivateKeySigner {
    let mnemonic = "test test test test test test test test test test test junk";
    alloy_signer_local::MnemonicBuilder::<alloy_signer_local::coins_bip39::English>::default()
        .phrase(mnemonic)
        .index(index)
        .expect("valid mnemonic")
        .build()
        .expect("valid key")
}

#[tokio::test(flavor = "multi_thread")]
async fn test_tempo_aa_transaction_basic() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();
    let recipient = accounts[1];
    let signer = dev_key(0);

    let token = IERC20::new(PATH_USD, &provider);
    let recipient_balance_before = token.balanceOf(recipient).call().await.unwrap();

    let transfer_amount = U256::from(100_000);
    let transfer_call = token.transfer(recipient, transfer_amount);
    let calldata: Bytes = transfer_call.calldata().clone();

    let chain_id = provider.get_chain_id().await.unwrap();
    let base_fee = provider.get_gas_price().await.unwrap();

    let tempo_tx = TempoTransaction {
        chain_id,
        fee_token: Some(ALPHA_USD),
        max_priority_fee_per_gas: base_fee / 10,
        max_fee_per_gas: base_fee * 2,
        gas_limit: TIP20_TRANSFER_GAS,
        calls: vec![Call { to: TxKind::Call(PATH_USD), value: U256::ZERO, input: calldata }],
        access_list: Default::default(),
        nonce_key: U256::ZERO,
        nonce: 0,
        fee_payer_signature: None,
        valid_before: None,
        valid_after: None,
        key_authorization: None,
        tempo_authorization_list: vec![],
    };

    let sig_hash = tempo_tx.signature_hash();
    let signature = signer.sign_hash(&sig_hash).await.unwrap();
    let tempo_sig = TempoSignature::Primitive(PrimitiveSignature::Secp256k1(signature));
    let signed_tx = AASigned::new_unhashed(tempo_tx, tempo_sig);
    let envelope = TempoTxEnvelope::AA(signed_tx);

    let mut encoded = Vec::new();
    envelope.encode_2718(&mut encoded);
    let tx_hash = provider.send_raw_transaction(&encoded).await.unwrap();
    let receipt = tx_hash.get_receipt().await.unwrap();

    assert!(receipt.status(), "Tempo AA transaction should succeed");

    let recipient_balance_after = token.balanceOf(recipient).call().await.unwrap();
    assert_eq!(
        recipient_balance_after,
        recipient_balance_before + transfer_amount,
        "Recipient should receive transfer amount"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_tempo_aa_transaction_with_2d_nonce() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();
    let recipient = accounts[1];
    let signer = dev_key(0);

    let token = IERC20::new(PATH_USD, &provider);
    let chain_id = provider.get_chain_id().await.unwrap();
    let base_fee = provider.get_gas_price().await.unwrap();

    // Send two transactions with different nonce keys (can be parallelized)
    let nonce_keys = [U256::from(1), U256::from(2)];

    for (i, nonce_key) in nonce_keys.iter().enumerate() {
        let transfer_amount = U256::from(50_000 * (i + 1) as u64);
        let transfer_call = token.transfer(recipient, transfer_amount);
        let calldata: Bytes = transfer_call.calldata().clone();

        let tempo_tx = TempoTransaction {
            chain_id,
            fee_token: Some(ALPHA_USD),
            max_priority_fee_per_gas: base_fee / 10,
            max_fee_per_gas: base_fee * 2,
            gas_limit: TIP20_TRANSFER_GAS,
            calls: vec![Call { to: TxKind::Call(PATH_USD), value: U256::ZERO, input: calldata }],
            access_list: Default::default(),
            nonce_key: *nonce_key,
            nonce: 0,
            fee_payer_signature: None,
            valid_before: None,
            valid_after: None,
            key_authorization: None,
            tempo_authorization_list: vec![],
        };

        let sig_hash = tempo_tx.signature_hash();
        let signature = signer.sign_hash(&sig_hash).await.unwrap();
        let tempo_sig = TempoSignature::Primitive(PrimitiveSignature::Secp256k1(signature));
        let signed_tx = AASigned::new_unhashed(tempo_tx, tempo_sig);
        let envelope = TempoTxEnvelope::AA(signed_tx);

        let mut encoded = Vec::new();
        envelope.encode_2718(&mut encoded);
        let tx_hash = provider.send_raw_transaction(&encoded).await.unwrap();
        let receipt = tx_hash.get_receipt().await.unwrap();

        assert!(receipt.status(), "Tempo AA transaction with nonce_key {nonce_key} should succeed");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_tempo_aa_transaction_with_valid_before() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();
    let recipient = accounts[1];
    let signer = dev_key(0);

    let token = IERC20::new(PATH_USD, &provider);
    let chain_id = provider.get_chain_id().await.unwrap();
    let base_fee = provider.get_gas_price().await.unwrap();

    let block = provider.get_block(BlockNumberOrTag::Latest.into()).await.unwrap().unwrap();
    let current_time = block.header.timestamp;
    let valid_before = current_time + 30;

    let transfer_call = token.transfer(recipient, U256::from(75_000));
    let calldata: Bytes = transfer_call.calldata().clone();

    let tempo_tx = TempoTransaction {
        chain_id,
        fee_token: Some(ALPHA_USD),
        max_priority_fee_per_gas: base_fee / 10,
        max_fee_per_gas: base_fee * 2,
        gas_limit: TIP20_TRANSFER_GAS,
        calls: vec![Call { to: TxKind::Call(PATH_USD), value: U256::ZERO, input: calldata }],
        access_list: Default::default(),
        nonce_key: U256::from(3),
        nonce: 0,
        fee_payer_signature: None,
        valid_before: Some(valid_before),
        valid_after: None,
        key_authorization: None,
        tempo_authorization_list: vec![],
    };

    let sig_hash = tempo_tx.signature_hash();
    let signature = signer.sign_hash(&sig_hash).await.unwrap();
    let tempo_sig = TempoSignature::Primitive(PrimitiveSignature::Secp256k1(signature));
    let signed_tx = AASigned::new_unhashed(tempo_tx, tempo_sig);
    let envelope = TempoTxEnvelope::AA(signed_tx);

    let mut encoded = Vec::new();
    envelope.encode_2718(&mut encoded);
    let tx_hash = provider.send_raw_transaction(&encoded).await.unwrap();
    let receipt = tx_hash.get_receipt().await.unwrap();

    assert!(receipt.status(), "Tempo AA transaction with valid_before should succeed");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_tempo_aa_transaction_with_valid_after() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();
    let recipient = accounts[1];
    let signer = dev_key(0);

    let token = IERC20::new(PATH_USD, &provider);
    let chain_id = provider.get_chain_id().await.unwrap();
    let base_fee = provider.get_gas_price().await.unwrap();

    let block = provider.get_block(BlockNumberOrTag::Latest.into()).await.unwrap().unwrap();
    let current_time = block.header.timestamp;
    let valid_after = current_time;
    let valid_before = current_time + 30;

    let transfer_call = token.transfer(recipient, U256::from(60_000));
    let calldata: Bytes = transfer_call.calldata().clone();

    let tempo_tx = TempoTransaction {
        chain_id,
        fee_token: Some(ALPHA_USD),
        max_priority_fee_per_gas: base_fee / 10,
        max_fee_per_gas: base_fee * 2,
        gas_limit: TIP20_TRANSFER_GAS,
        calls: vec![Call { to: TxKind::Call(PATH_USD), value: U256::ZERO, input: calldata }],
        access_list: Default::default(),
        nonce_key: U256::from(4),
        nonce: 0,
        fee_payer_signature: None,
        valid_before: Some(valid_before),
        valid_after: Some(valid_after),
        key_authorization: None,
        tempo_authorization_list: vec![],
    };

    let sig_hash = tempo_tx.signature_hash();
    let signature = signer.sign_hash(&sig_hash).await.unwrap();
    let tempo_sig = TempoSignature::Primitive(PrimitiveSignature::Secp256k1(signature));
    let signed_tx = AASigned::new_unhashed(tempo_tx, tempo_sig);
    let envelope = TempoTxEnvelope::AA(signed_tx);

    let mut encoded = Vec::new();
    envelope.encode_2718(&mut encoded);
    let tx_hash = provider.send_raw_transaction(&encoded).await.unwrap();
    let receipt = tx_hash.get_receipt().await.unwrap();

    assert!(
        receipt.status(),
        "Tempo AA transaction with valid_after (already valid) should succeed"
    );
}

// ============================================================================
// Tempo AA Transaction Error Cases
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_tempo_aa_expired_valid_before() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();
    let recipient = accounts[1];
    let signer = dev_key(0);

    let token = IERC20::new(PATH_USD, &provider);
    let chain_id = provider.get_chain_id().await.unwrap();
    let base_fee = provider.get_gas_price().await.unwrap();

    let block = provider.get_block(BlockNumberOrTag::Latest.into()).await.unwrap().unwrap();
    let current_time = block.header.timestamp;
    let valid_before = current_time.saturating_sub(10); // 10 seconds ago

    let transfer_call = token.transfer(recipient, U256::from(50_000));
    let calldata: Bytes = transfer_call.calldata().clone();

    let tempo_tx = TempoTransaction {
        chain_id,
        fee_token: Some(ALPHA_USD),
        max_priority_fee_per_gas: base_fee / 10,
        max_fee_per_gas: base_fee * 2,
        gas_limit: TIP20_TRANSFER_GAS,
        calls: vec![Call { to: TxKind::Call(PATH_USD), value: U256::ZERO, input: calldata }],
        access_list: Default::default(),
        nonce_key: U256::from(100),
        nonce: 0,
        fee_payer_signature: None,
        valid_before: Some(valid_before),
        valid_after: None,
        key_authorization: None,
        tempo_authorization_list: vec![],
    };

    let sig_hash = tempo_tx.signature_hash();
    let signature = signer.sign_hash(&sig_hash).await.unwrap();
    let tempo_sig = TempoSignature::Primitive(PrimitiveSignature::Secp256k1(signature));
    let signed_tx = AASigned::new_unhashed(tempo_tx, tempo_sig);
    let envelope = TempoTxEnvelope::AA(signed_tx);

    let mut encoded = Vec::new();
    envelope.encode_2718(&mut encoded);

    let result = provider.send_raw_transaction(&encoded).await;
    assert!(result.is_err(), "Transaction with expired valid_before should be rejected");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_tempo_aa_valid_after_future() {
    let (api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();
    let recipient = accounts[1];
    let signer = dev_key(0);

    let token = IERC20::new(PATH_USD, &provider);
    let chain_id = provider.get_chain_id().await.unwrap();
    let base_fee = provider.get_gas_price().await.unwrap();

    let block = provider.get_block(BlockNumberOrTag::Latest.into()).await.unwrap().unwrap();
    let current_time = block.header.timestamp;
    let valid_after = current_time + 5;
    let valid_before = current_time + 60;

    let transfer_call = token.transfer(recipient, U256::from(50_000));
    let calldata: Bytes = transfer_call.calldata().clone();

    let tempo_tx = TempoTransaction {
        chain_id,
        fee_token: Some(ALPHA_USD),
        max_priority_fee_per_gas: base_fee / 10,
        max_fee_per_gas: base_fee * 2,
        gas_limit: TIP20_TRANSFER_GAS,
        calls: vec![Call { to: TxKind::Call(PATH_USD), value: U256::ZERO, input: calldata }],
        access_list: Default::default(),
        nonce_key: U256::from(101),
        nonce: 0,
        fee_payer_signature: None,
        valid_before: Some(valid_before),
        valid_after: Some(valid_after),
        key_authorization: None,
        tempo_authorization_list: vec![],
    };

    let sig_hash = tempo_tx.signature_hash();
    let signature = signer.sign_hash(&sig_hash).await.unwrap();
    let tempo_sig = TempoSignature::Primitive(PrimitiveSignature::Secp256k1(signature));
    let signed_tx = AASigned::new_unhashed(tempo_tx, tempo_sig);
    let envelope = TempoTxEnvelope::AA(signed_tx);

    let mut encoded = Vec::new();
    envelope.encode_2718(&mut encoded);

    // Transaction enters pool but is not yet valid
    let tx_hash = provider.send_raw_transaction(&encoded).await.unwrap();

    // Advance time past valid_after
    api.evm_set_next_block_timestamp(valid_after + 1).unwrap();
    api.mine_one().await;

    let receipt = tx_hash.get_receipt().await.unwrap();
    assert!(receipt.status(), "Transaction should succeed after valid_after time");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_tempo_aa_nonce_replay_same_key() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();
    let recipient = accounts[1];
    let signer = dev_key(0);

    let token = IERC20::new(PATH_USD, &provider);
    let chain_id = provider.get_chain_id().await.unwrap();
    let base_fee = provider.get_gas_price().await.unwrap();

    let nonce_key = U256::from(200);

    // First transaction with nonce 0
    let transfer_call = token.transfer(recipient, U256::from(50_000));
    let calldata: Bytes = transfer_call.calldata().clone();

    let tempo_tx1 = TempoTransaction {
        chain_id,
        fee_token: Some(ALPHA_USD),
        max_priority_fee_per_gas: base_fee / 10,
        max_fee_per_gas: base_fee * 2,
        gas_limit: TIP20_TRANSFER_GAS,
        calls: vec![Call {
            to: TxKind::Call(PATH_USD),
            value: U256::ZERO,
            input: calldata.clone(),
        }],
        access_list: Default::default(),
        nonce_key,
        nonce: 0,
        fee_payer_signature: None,
        valid_before: None,
        valid_after: None,
        key_authorization: None,
        tempo_authorization_list: vec![],
    };

    let sig_hash = tempo_tx1.signature_hash();
    let signature = signer.sign_hash(&sig_hash).await.unwrap();
    let tempo_sig = TempoSignature::Primitive(PrimitiveSignature::Secp256k1(signature));
    let signed_tx = AASigned::new_unhashed(tempo_tx1, tempo_sig);
    let envelope = TempoTxEnvelope::AA(signed_tx);

    let mut encoded = Vec::new();
    envelope.encode_2718(&mut encoded);

    let tx_hash = provider.send_raw_transaction(&encoded).await.unwrap();
    let receipt = tx_hash.get_receipt().await.unwrap();
    assert!(receipt.status(), "First transaction should succeed");

    // Second transaction with nonce=1 on the same key should succeed
    let transfer_call2 = token.transfer(recipient, U256::from(60_000));
    let calldata2: Bytes = transfer_call2.calldata().clone();

    let tempo_tx2 = TempoTransaction {
        chain_id,
        fee_token: Some(ALPHA_USD),
        max_priority_fee_per_gas: base_fee / 10,
        max_fee_per_gas: base_fee * 2,
        gas_limit: TIP20_TRANSFER_GAS,
        calls: vec![Call { to: TxKind::Call(PATH_USD), value: U256::ZERO, input: calldata2 }],
        access_list: Default::default(),
        nonce_key,
        nonce: 1,
        fee_payer_signature: None,
        valid_before: None,
        valid_after: None,
        key_authorization: None,
        tempo_authorization_list: vec![],
    };

    let sig_hash = tempo_tx2.signature_hash();
    let signature = signer.sign_hash(&sig_hash).await.unwrap();
    let tempo_sig = TempoSignature::Primitive(PrimitiveSignature::Secp256k1(signature));
    let signed_tx = AASigned::new_unhashed(tempo_tx2, tempo_sig);
    let envelope = TempoTxEnvelope::AA(signed_tx);

    let mut encoded = Vec::new();
    envelope.encode_2718(&mut encoded);

    let tx_hash2 = provider.send_raw_transaction(&encoded).await.unwrap();
    let receipt2 = tx_hash2.get_receipt().await.unwrap();
    assert!(receipt2.status(), "Second transaction with nonce=1 should succeed");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_tempo_aa_parallel_nonces_different_keys() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();
    let recipient = accounts[1];
    let signer = dev_key(0);

    let token = IERC20::new(PATH_USD, &provider);
    let chain_id = provider.get_chain_id().await.unwrap();
    let base_fee = provider.get_gas_price().await.unwrap();

    let recipient_balance_before = token.balanceOf(recipient).call().await.unwrap();

    // Send two transactions with the SAME nonce (0) but DIFFERENT nonce keys
    let mut tx_hashes = vec![];

    for nonce_key_val in [300u64, 301u64] {
        let transfer_call = token.transfer(recipient, U256::from(10_000));
        let calldata: Bytes = transfer_call.calldata().clone();

        let tempo_tx = TempoTransaction {
            chain_id,
            fee_token: Some(ALPHA_USD),
            max_priority_fee_per_gas: base_fee / 10,
            max_fee_per_gas: base_fee * 2,
            gas_limit: TIP20_TRANSFER_GAS,
            calls: vec![Call { to: TxKind::Call(PATH_USD), value: U256::ZERO, input: calldata }],
            access_list: Default::default(),
            nonce_key: U256::from(nonce_key_val),
            nonce: 0,
            fee_payer_signature: None,
            valid_before: None,
            valid_after: None,
            key_authorization: None,
            tempo_authorization_list: vec![],
        };

        let sig_hash = tempo_tx.signature_hash();
        let signature = signer.sign_hash(&sig_hash).await.unwrap();
        let tempo_sig = TempoSignature::Primitive(PrimitiveSignature::Secp256k1(signature));
        let signed_tx = AASigned::new_unhashed(tempo_tx, tempo_sig);
        let envelope = TempoTxEnvelope::AA(signed_tx);

        let mut encoded = Vec::new();
        envelope.encode_2718(&mut encoded);

        let tx_hash = provider.send_raw_transaction(&encoded).await.unwrap();
        tx_hashes.push(tx_hash);
    }

    for tx_hash in tx_hashes {
        let receipt = tx_hash.get_receipt().await.unwrap();
        assert!(receipt.status(), "Parallel transactions with different nonce keys should succeed");
    }

    let recipient_balance_after = token.balanceOf(recipient).call().await.unwrap();
    assert_eq!(
        recipient_balance_after,
        recipient_balance_before + U256::from(20_000),
        "Recipient should receive both transfers"
    );
}

// ============================================================================
// Gas Estimation
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_gas_estimation() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();

    let token = IERC20::new(ALPHA_USD, &provider);
    let transfer_call = token.transfer(accounts[1], U256::from(1000));
    let calldata: Bytes = transfer_call.calldata().clone();

    let tx = TransactionRequest::default().from(accounts[0]).to(ALPHA_USD).with_input(calldata);

    let gas_estimate = provider.estimate_gas(tx.into()).await.unwrap();

    // TIP20 transfer should use more than 21000 gas
    assert!(gas_estimate > 21000, "TIP20 transfer should use more than 21000 gas");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_gas_estimation_for_contract_call() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();

    let token = IERC20::new(ALPHA_USD, &provider);
    let transfer_call = token.transfer(accounts[1], U256::from(1000));
    let calldata: Bytes = transfer_call.calldata().clone();

    let tx = TransactionRequest::default().from(accounts[0]).to(ALPHA_USD).with_input(calldata);

    let gas_estimate = provider.estimate_gas(tx.into()).await.unwrap();

    // Contract call should use more gas than simple transfer
    assert!(gas_estimate > 21000, "Contract call should use more than 21000 gas");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_gas_estimation_with_value_fails() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();

    // Gas estimation with native value should fail in Tempo mode
    let tx = TransactionRequest::default()
        .from(accounts[0])
        .to(accounts[1])
        .value(U256::from(1_000_000_000_000_000_000u64));

    let result = provider.estimate_gas(tx.into()).await;
    assert!(result.is_err(), "Gas estimation with native value should fail in Tempo mode");
}

// ============================================================================
// Gas Price & Base Fee
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_gas_price() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let gas_price = provider.get_gas_price().await.unwrap();

    assert!(gas_price > 0, "Gas price should be non-zero");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_base_fee() {
    let (api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    api.mine_one().await;

    let block = provider.get_block(BlockNumberOrTag::Latest.into()).await.unwrap().unwrap();

    assert!(block.header.base_fee_per_gas.is_some());
}

// ============================================================================
// Fee Token Deduction
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_eip1559_fee_token_deduction() {
    let (_api, handle) = spawn(NodeConfig::test_tempo()).await;
    let provider = handle.http_provider();

    let accounts: Vec<Address> = handle.dev_accounts().collect();
    let sender = accounts[0];
    let recipient = accounts[1];

    // Check fee token balance before (ALPHA_USD is the default fee token)
    let fee_token = IERC20::new(ALPHA_USD, &provider);
    let fee_balance_before = fee_token.balanceOf(sender).call().await.unwrap();

    // Transfer PATH_USD so balance change is only from gas fees, not the transfer itself
    let token = IERC20::new(PATH_USD, &provider);
    let transfer_call = token.transfer(recipient, U256::from(100_000));
    let calldata: Bytes = transfer_call.calldata().clone();

    let base_fee = provider.get_gas_price().await.unwrap();

    let tx = TransactionRequest::default()
        .from(sender)
        .to(PATH_USD)
        .with_input(calldata)
        .with_gas_limit(TIP20_TRANSFER_GAS)
        .max_fee_per_gas(base_fee * 2)
        .max_priority_fee_per_gas(base_fee / 10);

    let tx = WithOtherFields::new(tx);
    let receipt = provider.send_transaction(tx).await.unwrap().get_receipt().await.unwrap();

    assert!(receipt.status(), "Transaction should succeed");

    // Fee token balance should have decreased (gas fees paid in ALPHA_USD)
    let fee_balance_after = fee_token.balanceOf(sender).call().await.unwrap();
    assert!(
        fee_balance_after < fee_balance_before,
        "Fee token balance should decrease after paying gas (before: {fee_balance_before}, after: {fee_balance_after})"
    );
}
