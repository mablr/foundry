//! Native Ethereum EVM construction.

use alloy_consensus::transaction::Recovered;
use evm2::{
    BaseEvmTypes, Evm, ExecutionConfig, Inspector, Precompiles, TxResult,
    ethereum::{TxEnvelope, ethereum_tx_registry},
    evm::{Db, DynDatabase, registry::HandlerResult},
};
use foundry_evm_core::native::{EthereumEnv, LocalState};

mod inspector;
pub use inspector::EthereumInspectorStack;

/// Constructs the Ethereum execution host used by Foundry.
#[derive(Clone, Copy, Debug, Default)]
pub struct EthereumFactory;

impl EthereumFactory {
    /// Creates an EVM with Ethereum transaction handlers and precompiles for `env.spec`.
    pub fn create<'db>(
        self,
        env: EthereumEnv,
        database: impl DynDatabase + 'db,
    ) -> Evm<'db, BaseEvmTypes> {
        Evm::new_with_execution_config(
            ExecutionConfig::for_spec_and_version(env.spec, env.version),
            env.spec,
            env.block,
            ethereum_tx_registry(env.spec),
            database,
            Precompiles::base(env.spec),
        )
    }
}

/// Ethereum execution with copy-on-write local state.
#[derive(Clone, Debug)]
pub struct EthereumExecutor {
    env: EthereumEnv,
    state: LocalState,
}

impl EthereumExecutor {
    /// Creates an executor over local Ethereum state.
    pub const fn new(env: EthereumEnv, state: LocalState) -> Self {
        Self { env, state }
    }

    /// Returns the accepted state.
    pub const fn state(&self) -> &LocalState {
        &self.state
    }

    /// Returns mutable accepted state, cloning it if shared by another executor.
    pub const fn state_mut(&mut self) -> &mut LocalState {
        &mut self.state
    }

    /// Executes a transaction without accepting its state changes.
    pub fn call(&self, tx: &Recovered<TxEnvelope>) -> HandlerResult<TxResult> {
        let mut evm = EthereumFactory.create(self.env, Db::new(&self.state));
        Ok(evm.transact(tx)?.discard())
    }

    /// Executes an inspected transaction without accepting its state changes.
    pub fn inspect_call<I: Inspector<BaseEvmTypes>>(
        &self,
        tx: &Recovered<TxEnvelope>,
        inspector: &mut I,
    ) -> HandlerResult<TxResult> {
        let mut evm = EthereumFactory.create(self.env, Db::new(&self.state));
        evm.set_inspector(inspector);
        Ok(evm.transact(tx)?.discard())
    }

    /// Executes and accepts a transaction's state changes.
    pub fn transact(&mut self, tx: &Recovered<TxEnvelope>) -> HandlerResult<TxResult> {
        let outcome = {
            let mut evm = EthereumFactory.create(self.env, Db::new(&self.state));
            evm.transact(tx)?.detach()
        };
        self.state.commit(&outcome.pending_state);
        Ok(outcome.result)
    }

    /// Executes an inspected transaction and accepts its state changes.
    pub fn inspect_transact<I: Inspector<BaseEvmTypes>>(
        &mut self,
        tx: &Recovered<TxEnvelope>,
        inspector: &mut I,
    ) -> HandlerResult<TxResult> {
        let outcome = {
            let mut evm = EthereumFactory.create(self.env, Db::new(&self.state));
            evm.set_inspector(inspector);
            evm.transact(tx)?.detach()
        };
        self.state.commit(&outcome.pending_state);
        Ok(outcome.result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_consensus::TxLegacy;
    use alloy_primitives::{Address, Bytes, TxKind, U256};
    use alloy_sol_types::SolCall;
    use evm2::{
        SpecId,
        bytecode::Bytecode,
        env::BlockEnvExt,
        evm::AccountInfo,
        interpreter::{
            GasTracker, InstrStop, Interpreter, Message, MessageResult, MessageResultExt,
        },
    };
    use foundry_cheatcodes::{Vm, native::NativeCheatcodes};
    use foundry_compilers::artifacts::EvmVersion;
    use foundry_evm_core::{constants::CHEATCODE_ADDRESS, opts::EvmOpts};

    #[test]
    fn executor_discards_calls_and_commits_copy_on_write_transactions() {
        let config =
            foundry_config::Config { evm_version: EvmVersion::Cancun, ..Default::default() };
        let caller = Address::with_last_byte(0xa);
        let recipient = Address::with_last_byte(0xb);
        let mut state = LocalState::default();
        state.database_mut().insert_account_info(
            &recipient,
            AccountInfo::default().with_code(Bytecode::new_legacy(Bytes::from_static(&[
                0x60, 0x01, 0x5f, 0x55, 0x46, 0x5f, 0x52, 0x60, 0x20, 0x5f, 0xf3,
            ]))),
        );
        let mut opts = EvmOpts::default();
        opts.env.chain_id = Some(31_337);
        opts.env.gas_limit = foundry_config::GasLimit(30_000_000);
        opts.memory_limit = 1_000_000;
        let env = EthereumEnv::local_from_config(&config, &opts).unwrap();
        assert_eq!(env.spec, SpecId::CANCUN);
        let mut executor = EthereumExecutor::new(env, state);
        let tx = Recovered::new_unchecked(
            TxEnvelope::Legacy(TxLegacy {
                gas_limit: 100_000,
                to: TxKind::Call(recipient),
                ..Default::default()
            }),
            caller,
        );

        let call = executor.call(&tx).unwrap();
        assert!(call.status);
        assert_eq!(U256::from_be_slice(&call.output), U256::from(31_337));
        assert!(!executor.state().database().cache.accounts.contains_key(&caller));
        assert!(!executor.state().database().cache.storage.contains_key(&recipient));

        let transaction = executor.transact(&tx).unwrap();
        assert!(transaction.status);
        assert_eq!(
            executor.state().database().cache.storage[&recipient].slots[&U256::ZERO],
            U256::ONE
        );
        let snapshot = executor.clone();
        assert!(executor.transact(&tx).unwrap().status);
        assert_eq!(executor.state().database().cache.accounts[&caller].as_ref().unwrap().nonce, 2);
        assert_eq!(snapshot.state().database().cache.accounts[&caller].as_ref().unwrap().nonce, 1);
    }

    #[test]
    fn reverted_transaction_commits_nonce_without_storage() {
        let caller = Address::with_last_byte(0xa);
        let recipient = Address::with_last_byte(0xb);
        let mut state = LocalState::default();
        state.database_mut().insert_account_info(
            &recipient,
            AccountInfo::default().with_code(Bytecode::new_legacy(Bytes::from_static(&[
                0x60, 0x01, 0x5f, 0x55, 0x5f, 0x5f, 0xfd,
            ]))),
        );
        let env = EthereumEnv::new(
            SpecId::CANCUN,
            BlockEnvExt { gas_limit: U256::from(30_000_000), ..Default::default() },
        );
        let mut executor = EthereumExecutor::new(env, state);
        let tx = Recovered::new_unchecked(
            TxEnvelope::Legacy(TxLegacy {
                gas_limit: 100_000,
                to: TxKind::Call(recipient),
                ..Default::default()
            }),
            caller,
        );

        assert!(!executor.transact(&tx).unwrap().status);
        assert_eq!(executor.state().database().cache.accounts[&caller].as_ref().unwrap().nonce, 1);
        assert!(!executor.state().database().cache.storage.contains_key(&recipient));
    }

    #[test]
    fn inspector_mutations_follow_call_and_transaction_boundaries() {
        struct BalanceInspector {
            target: Address,
            calls: usize,
        }

        impl Inspector<BaseEvmTypes> for BalanceInspector {
            fn call(
                &mut self,
                interp: &mut Interpreter<'_, '_, BaseEvmTypes>,
                message: &mut Message<BaseEvmTypes>,
            ) -> Option<MessageResult<BaseEvmTypes>> {
                self.calls += 1;
                interp
                    .host()
                    .state_mut()
                    .account(&self.target, false)
                    .unwrap()
                    .set_balance(U256::from(7));
                Some(MessageResultExt {
                    stop: InstrStop::Return,
                    gas: GasTracker::new(message.gas_limit),
                    ..Default::default()
                })
            }
        }

        let caller = Address::with_last_byte(0xa);
        let target = Address::with_last_byte(0xb);
        let env = EthereumEnv::new(
            SpecId::CANCUN,
            BlockEnvExt { gas_limit: U256::from(30_000_000), ..Default::default() },
        );
        let mut executor = EthereumExecutor::new(env, LocalState::default());
        let tx = Recovered::new_unchecked(
            TxEnvelope::Legacy(TxLegacy {
                gas_limit: 100_000,
                to: TxKind::Call(target),
                ..Default::default()
            }),
            caller,
        );
        let mut inspector = BalanceInspector { target, calls: 0 };

        assert!(executor.inspect_call(&tx, &mut inspector).unwrap().status);
        assert_eq!(inspector.calls, 1);
        assert!(!executor.state().database().cache.accounts.contains_key(&target));

        assert!(executor.inspect_transact(&tx, &mut inspector).unwrap().status);
        assert_eq!(inspector.calls, 2);
        assert_eq!(
            executor.state().database().cache.accounts[&target].as_ref().unwrap().balance,
            U256::from(7)
        );
    }

    #[test]
    fn native_deal_cheatcode_obeys_execution_boundaries() {
        let caller = Address::with_last_byte(0xa);
        let target = Address::with_last_byte(0xb);
        let env = EthereumEnv::new(
            SpecId::CANCUN,
            BlockEnvExt { gas_limit: U256::from(30_000_000), ..Default::default() },
        );
        let mut executor = EthereumExecutor::new(env, LocalState::default());
        let tx = Recovered::new_unchecked(
            TxEnvelope::Legacy(TxLegacy {
                gas_limit: 100_000,
                to: TxKind::Call(CHEATCODE_ADDRESS),
                input: Vm::dealCall { account: target, newBalance: U256::from(7) }
                    .abi_encode()
                    .into(),
                ..Default::default()
            }),
            caller,
        );
        let mut inspector = NativeCheatcodes;

        assert!(executor.inspect_call(&tx, &mut inspector).unwrap().status);
        assert!(!executor.state().database().cache.accounts.contains_key(&target));

        assert!(executor.inspect_transact(&tx, &mut inspector).unwrap().status);
        assert_eq!(
            executor.state().database().cache.accounts[&target].as_ref().unwrap().balance,
            U256::from(7)
        );
    }

    #[test]
    fn deployed_code_executes_from_accepted_state() {
        let caller = Address::with_last_byte(0xa);
        let mut state = LocalState::default();
        state.set_balance(caller, U256::MAX);
        state.set_nonce(caller, 1);
        let env = EthereumEnv::new(
            SpecId::CANCUN,
            BlockEnvExt { gas_limit: U256::from(30_000_000), ..Default::default() },
        );
        let mut executor = EthereumExecutor::new(env, state);
        let creation_code = Bytes::from_static(&[
            0x60, 0x0a, 0x60, 0x0c, 0x60, 0x00, 0x39, 0x60, 0x0a, 0x60, 0x00, 0xf3, 0x60, 0x2a,
            0x60, 0x00, 0x52, 0x60, 0x20, 0x60, 0x00, 0xf3,
        ]);
        let deploy = Recovered::new_unchecked(
            TxEnvelope::Legacy(TxLegacy {
                nonce: 1,
                gas_limit: 100_000,
                to: TxKind::Create,
                input: creation_code,
                ..Default::default()
            }),
            caller,
        );

        let deployment = executor.transact(&deploy).unwrap();
        assert!(deployment.status);
        let address = deployment.created_address.unwrap();
        assert_eq!(address, caller.create(1));
        assert_eq!(executor.state().database().account_info(&caller).unwrap().nonce, 2);

        let call = Recovered::new_unchecked(
            TxEnvelope::Legacy(TxLegacy {
                nonce: 2,
                gas_limit: 100_000,
                to: TxKind::Call(address),
                ..Default::default()
            }),
            caller,
        );
        let result = executor.call(&call).unwrap();
        assert!(result.status);
        assert_eq!(U256::from_be_slice(&result.output), U256::from(42));
        assert_eq!(executor.state().database().account_info(&caller).unwrap().nonce, 2);
    }

    #[test]
    fn nested_native_deal_respects_parent_outcome() {
        let caller = Address::with_last_byte(0xa);
        let contract = Address::with_last_byte(0xb);
        let target = Address::with_last_byte(0xc);
        let calldata = Vm::dealCall { account: target, newBalance: U256::from(7) }.abi_encode();
        let mut code = vec![
            0x60,
            calldata.len() as u8,
            0x60,
            0,
            0x60,
            0,
            0x39, // Copy cheatcode calldata.
            0x60,
            0,
            0x60,
            0,
            0x60,
            calldata.len() as u8,
            0x60,
            0,
            0x60,
            0,    // CALL arguments.
            0x73, // PUSH20 cheatcode address.
        ];
        code.extend_from_slice(CHEATCODE_ADDRESS.as_slice());
        code.extend_from_slice(&[0x61, 0x27, 0x10, 0xf1, 0x50, 0x60, 0, 0x60, 0, 0xfd]);
        code[3] = code.len() as u8;
        code.extend_from_slice(&calldata);
        let mut success_code = code.clone();
        success_code[code[3] as usize - 1] = 0xf3;

        let mut state = LocalState::default();
        state.database_mut().insert_account_info(
            &contract,
            AccountInfo::default().with_code(Bytecode::new_legacy(code.into())),
        );
        let env = EthereumEnv::new(
            SpecId::CANCUN,
            BlockEnvExt { gas_limit: U256::from(30_000_000), ..Default::default() },
        );
        let mut executor = EthereumExecutor::new(env, state);
        let tx = Recovered::new_unchecked(
            TxEnvelope::Legacy(TxLegacy {
                gas_limit: 100_000,
                to: TxKind::Call(contract),
                ..Default::default()
            }),
            caller,
        );

        assert!(!executor.inspect_transact(&tx, &mut NativeCheatcodes).unwrap().status);
        assert!(!executor.state().database().cache.accounts.contains_key(&target));

        let mut success_executor = executor.clone();
        success_executor.state_mut().database_mut().insert_account_info(
            &contract,
            AccountInfo::default().with_code(Bytecode::new_legacy(success_code.into())),
        );
        let success_tx = Recovered::new_unchecked(
            TxEnvelope::Legacy(TxLegacy {
                nonce: 1,
                gas_limit: 100_000,
                to: TxKind::Call(contract),
                ..Default::default()
            }),
            caller,
        );
        assert!(
            success_executor.inspect_transact(&success_tx, &mut NativeCheatcodes).unwrap().status
        );
        assert_eq!(
            success_executor.state().database().account_info(&target).unwrap().balance,
            U256::from(7)
        );
    }
}
