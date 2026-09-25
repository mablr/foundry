//! Native Ethereum EVM construction.

use alloy_consensus::transaction::Recovered;
use evm2::{
    Evm, ExecutionConfig, NoopInspector, Precompiles, TxResult,
    ethereum::{TxEnvelope, ethereum_tx_registry},
    evm::{Database, Db, DynDatabase, EmptyDB, registry::HandlerResult},
};
use foundry_evm_core::native::{EthereumEnv, FoundryEvmTypes, LocalState, NativeInspector};

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
    ) -> Evm<'db, FoundryEvmTypes> {
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

/// Ethereum execution with copy-on-write local state and an owned inspector.
#[derive(Clone, Debug)]
pub struct EthereumExecutor<D: Database + Clone = EmptyDB, I = NoopInspector> {
    env: EthereumEnv,
    state: LocalState<D>,
    inspector: I,
}

impl<D: Database + Clone> EthereumExecutor<D, NoopInspector> {
    /// Creates an executor without Foundry inspectors.
    pub fn new(env: EthereumEnv, state: LocalState<D>) -> Self {
        Self { env, state, inspector: NoopInspector::default() }
    }
}

impl<D: Database + Clone + 'static> EthereumExecutor<D, EthereumInspectorStack<D>> {
    /// Creates an executor with Foundry's native inspector stack installed.
    pub fn new_foundry(env: EthereumEnv, mut state: LocalState<D>) -> Self {
        let inspector = EthereumInspectorStack::new(state.clone());
        inspector.install(&mut state);
        Self { env, state, inspector }
    }
}

impl<D: Database + Clone + 'static, I: NativeInspector<D>> EthereumExecutor<D, I> {
    /// Creates an executor with an inspector retained across accepted transactions.
    pub const fn with_inspector(env: EthereumEnv, state: LocalState<D>, inspector: I) -> Self {
        Self { env, state, inspector }
    }

    /// Returns the inspector and its accumulated observations.
    pub const fn inspector(&self) -> &I {
        &self.inspector
    }

    /// Returns the mutable inspector and its accumulated observations.
    pub const fn inspector_mut(&mut self) -> &mut I {
        &mut self.inspector
    }

    /// Returns the execution environment used for subsequent transactions.
    pub const fn env(&self) -> &EthereumEnv {
        &self.env
    }

    /// Returns the accepted state.
    pub const fn state(&self) -> &LocalState<D> {
        &self.state
    }

    /// Returns mutable accepted state, cloning it if shared by another executor.
    pub const fn state_mut(&mut self) -> &mut LocalState<D> {
        &mut self.state
    }

    /// Executes a transaction without accepting its state or inspector changes.
    pub fn call(&self, tx: &Recovered<TxEnvelope>) -> HandlerResult<TxResult> {
        self.inspect(tx).map(|(result, _)| result)
    }

    /// Executes without accepting state, returning the transaction's inspector observations.
    pub fn inspect(&self, tx: &Recovered<TxEnvelope>) -> HandlerResult<(TxResult, I)> {
        let mut state = self.state.clone();
        let mut inspector = self.inspector.clone();
        inspector.set_backend(state.clone());
        let result = {
            let mut evm = EthereumFactory.create(self.env, Db::new(&mut state));
            evm.set_inspector(&mut inspector);
            evm.transact(tx)?.discard()
        };
        inspector.finish_transaction(result.tx_gas_used());
        Ok((result, inspector))
    }

    /// Executes and accepts a transaction's state changes.
    pub fn transact(&mut self, tx: &Recovered<TxEnvelope>) -> HandlerResult<TxResult> {
        let mut inspector = self.inspector.clone();
        inspector.set_backend(self.state.clone());
        let (outcome, block) = {
            let mut evm = EthereumFactory.create(self.env, Db::new(&mut self.state));
            evm.set_inspector(&mut inspector);
            let outcome = evm.transact(tx)?.detach();
            (outcome, *evm.block())
        };
        if let Some(state) = inspector.take_backend_reset() {
            self.state = state;
        }
        inspector.finish_transaction(outcome.result.tx_gas_used());
        self.state.commit(&outcome.pending_state);
        self.env.block = block;
        self.inspector = inspector;
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
        Inspector, SpecId,
        bytecode::Bytecode,
        env::BlockEnvExt,
        evm::{AccountInfo, InMemoryDB},
        interpreter::{
            GasTracker, InstrStop, Interpreter, Message, MessageResult, MessageResultExt,
        },
    };
    use foundry_cheatcodes::{Vm, native::NativeCheatcodes};
    use foundry_compilers::artifacts::EvmVersion;
    use foundry_evm_core::{
        constants::CHEATCODE_ADDRESS,
        native::{ForkState, fork_db},
        opts::EvmOpts,
    };
    use std::{
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        time::Duration,
    };
    use tiny_http::{Response, Server};

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
        assert_eq!(
            Database::get_storage(&mut executor.state_mut(), &recipient, &U256::ZERO).unwrap(),
            U256::ZERO
        );
    }

    #[test]
    fn owned_inspector_observations_follow_execution_boundaries() {
        #[derive(Clone)]
        struct BalanceInspector {
            target: Address,
            calls: usize,
        }

        impl Inspector<FoundryEvmTypes> for BalanceInspector {
            fn call(
                &mut self,
                interp: &mut Interpreter<'_, '_, FoundryEvmTypes>,
                message: &mut Message<FoundryEvmTypes>,
            ) -> Option<MessageResult<FoundryEvmTypes>> {
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

        impl NativeInspector<EmptyDB> for BalanceInspector {}

        let caller = Address::with_last_byte(0xa);
        let target = Address::with_last_byte(0xb);
        let env = EthereumEnv::new(
            SpecId::CANCUN,
            BlockEnvExt { gas_limit: U256::from(30_000_000), ..Default::default() },
        );
        let mut executor = EthereumExecutor::with_inspector(
            env,
            LocalState::default(),
            BalanceInspector { target, calls: 0 },
        );
        let tx = Recovered::new_unchecked(
            TxEnvelope::Legacy(TxLegacy {
                gas_limit: 100_000,
                to: TxKind::Call(target),
                ..Default::default()
            }),
            caller,
        );
        let (observed_result, observed_inspector) = executor.inspect(&tx).unwrap();
        assert!(observed_result.status);
        assert_eq!(observed_inspector.calls, 1);
        assert_eq!(executor.inspector().calls, 0);
        assert!(!executor.state().database().cache.accounts.contains_key(&target));

        assert!(executor.transact(&tx).unwrap().status);
        assert_eq!(executor.inspector().calls, 1);
        assert_eq!(
            executor.state().database().cache.accounts[&target].as_ref().unwrap().balance,
            U256::from(7)
        );

        let mut clone = executor.clone();
        let next_tx = Recovered::new_unchecked(
            TxEnvelope::Legacy(TxLegacy {
                nonce: 1,
                gas_limit: 100_000,
                to: TxKind::Call(target),
                ..Default::default()
            }),
            caller,
        );
        assert!(clone.transact(&next_tx).unwrap().status);
        assert_eq!(clone.inspector().calls, 2);
        assert_eq!(executor.inspector().calls, 1);
    }

    #[test]
    fn native_deal_cheatcode_obeys_execution_boundaries() {
        let caller = Address::with_last_byte(0xa);
        let target = Address::with_last_byte(0xb);
        let env = EthereumEnv::new(
            SpecId::CANCUN,
            BlockEnvExt { gas_limit: U256::from(30_000_000), ..Default::default() },
        );
        let mut executor = EthereumExecutor::with_inspector(
            env,
            LocalState::default(),
            NativeCheatcodes::default(),
        );
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
        assert!(executor.call(&tx).unwrap().status);
        assert!(!executor.state().database().cache.accounts.contains_key(&target));

        assert!(executor.transact(&tx).unwrap().status);
        assert_eq!(
            executor.state().database().cache.accounts[&target].as_ref().unwrap().balance,
            U256::from(7)
        );
    }

    #[test]
    fn native_broadcast_collects_sender_nonce_and_call_data() {
        let caller = Address::with_last_byte(0xa);
        let sender = Address::with_last_byte(0xb);
        let target = Address::with_last_byte(0xc);
        let mut state = LocalState::default();
        state.set_balance(caller, U256::from(10)).unwrap();
        state.set_balance(sender, U256::from(10)).unwrap();
        let env = EthereumEnv::new(
            SpecId::CANCUN,
            BlockEnvExt { gas_limit: U256::from(30_000_000), ..Default::default() },
        );
        let mut executor = EthereumExecutor::new_foundry(env, state);
        let broadcast = Recovered::new_unchecked(
            TxEnvelope::Legacy(TxLegacy {
                gas_limit: 100_000,
                to: TxKind::Call(CHEATCODE_ADDRESS),
                input: Vm::broadcast_1Call { signer: sender }.abi_encode().into(),
                ..Default::default()
            }),
            caller,
        );
        assert!(executor.transact(&broadcast).unwrap().status);

        let input = Bytes::from_static(&[1, 2, 3]);
        let call = Recovered::new_unchecked(
            TxEnvelope::Legacy(TxLegacy {
                nonce: 1,
                gas_limit: 100_000,
                to: TxKind::Call(target),
                value: U256::from(3),
                input: input.clone(),
                ..Default::default()
            }),
            caller,
        );
        assert!(executor.transact(&call).unwrap().status);
        let mut transactions = executor.inspector_mut().take_broadcast_transactions();
        let transaction = transactions.pop_front().unwrap().transaction;
        assert!(transactions.is_empty());
        assert_eq!(transaction.from(), Some(sender));
        assert_eq!(transaction.to(), Some(target));
        assert_eq!(transaction.value(), Some(U256::from(3)));
        assert_eq!(transaction.input(), Some(&input));
        assert_eq!(transaction.nonce(), Some(0));
        assert_eq!(executor.state().database().account_info(&sender).unwrap().nonce, 1);
    }

    #[test]
    fn deployed_code_executes_from_accepted_state() {
        let caller = Address::with_last_byte(0xa);
        let mut state = LocalState::default();
        state.set_balance(caller, U256::MAX).unwrap();
        state.set_nonce(caller, 1).unwrap();
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
        let mut executor =
            EthereumExecutor::with_inspector(env, state, NativeCheatcodes::default());
        let tx = Recovered::new_unchecked(
            TxEnvelope::Legacy(TxLegacy {
                gas_limit: 100_000,
                to: TxKind::Call(contract),
                ..Default::default()
            }),
            caller,
        );

        assert!(!executor.transact(&tx).unwrap().status);
        assert!(executor.state().database().account_info(&target).is_none());

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
        assert!(success_executor.transact(&success_tx).unwrap().status);
        assert_eq!(
            success_executor.state().database().account_info(&target).unwrap().balance,
            U256::from(7)
        );
    }

    #[test]
    fn executor_reads_backing_storage_and_commits_only_to_overlay() {
        let caller = Address::with_last_byte(0xa);
        let contract = Address::with_last_byte(0xb);
        let mut backing = InMemoryDB::default();
        backing.insert_account_info(
            &contract,
            AccountInfo::default().with_code(Bytecode::new_legacy(Bytes::from_static(&[
                0x5f, 0x54, 0x60, 0x01, 0x01, 0x5f, 0x55, 0x5f, 0x54, 0x5f, 0x52, 0x60, 0x20, 0x5f,
                0xf3,
            ]))),
        );
        backing.insert_account_storage(&contract, &U256::ZERO, &U256::from(3));
        let env = EthereumEnv::new(
            SpecId::CANCUN,
            BlockEnvExt { gas_limit: U256::from(30_000_000), ..Default::default() },
        );
        let mut executor = EthereumExecutor::new(env, LocalState::new(backing));
        let tx = Recovered::new_unchecked(
            TxEnvelope::Legacy(TxLegacy {
                gas_limit: 100_000,
                to: TxKind::Call(contract),
                ..Default::default()
            }),
            caller,
        );

        assert_eq!(U256::from_be_slice(&executor.call(&tx).unwrap().output), U256::from(4));
        assert!(!executor.state().database().cache.storage.contains_key(&contract));
        assert_eq!(U256::from_be_slice(&executor.transact(&tx).unwrap().output), U256::from(4));
        assert_eq!(
            executor.state().database().cache.storage[&contract].slots[&U256::ZERO],
            U256::from(4)
        );
        assert_eq!(
            executor.state().database().db.inner().cache.storage[&contract].slots[&U256::ZERO],
            U256::from(3)
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn executor_reads_rpc_fork_and_keeps_commits_local() {
        let server = Server::http("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}", server.server_addr());
        let stopped = Arc::new(AtomicBool::new(false));
        let stop = Arc::clone(&stopped);
        let handle = std::thread::spawn(move || {
            let mut methods = Vec::new();
            while !stop.load(Ordering::Relaxed) {
                if let Some(mut request) = server.recv_timeout(Duration::from_millis(20)).unwrap() {
                    let rpc: serde_json::Value =
                        serde_json::from_reader(request.as_reader()).unwrap();
                    let method = rpc["method"].as_str().unwrap().to_owned();
                    let result = match method.as_str() {
                        "eth_getAccountInfo" => serde_json::json!({
                            "balance": "0x0",
                            "nonce": "0x0",
                            "code": "0x5f546001015f555f545f5260205ff3"
                        }),
                        "eth_getStorageAt" => serde_json::json!("0x3"),
                        other => panic!("unexpected RPC method: {other}"),
                    };
                    let response =
                        serde_json::json!({ "jsonrpc": "2.0", "id": rpc["id"], "result": result });
                    methods.push(method);
                    request.respond(Response::from_string(response.to_string())).unwrap();
                }
            }
            methods
        });

        let caller = Address::with_last_byte(0xa);
        let contract = Address::with_last_byte(0xb);
        let meta = fork_db::cache::BlockchainDbMeta::new(serde_json::Value::Null, endpoint.clone())
            .with_account_fetch_policy(fork_db::AccountFetchPolicy::RequireAccountInfo);
        let db = fork_db::BlockchainDb::new(meta, None);
        db.accounts().write().insert(caller, AccountInfo::default());
        let provider = EvmOpts::default().fork_provider_with_url(&endpoint).unwrap();
        let backend: fork_db::SharedBackend =
            fork_db::SharedBackend::spawn_backend(Arc::new(provider), db.clone(), None).await;
        let env = EthereumEnv::new(
            SpecId::CANCUN,
            BlockEnvExt { gas_limit: U256::from(30_000_000), ..Default::default() },
        );
        let mut executor = EthereumExecutor::new(env, ForkState::new(backend));
        let tx = Recovered::new_unchecked(
            TxEnvelope::Legacy(TxLegacy {
                gas_limit: 100_000,
                to: TxKind::Call(contract),
                ..Default::default()
            }),
            caller,
        );

        assert_eq!(U256::from_be_slice(&executor.call(&tx).unwrap().output), U256::from(4));
        assert_eq!(U256::from_be_slice(&executor.transact(&tx).unwrap().output), U256::from(4));
        assert_eq!(
            executor.state().database().cache.storage[&contract].slots[&U256::ZERO],
            U256::from(4)
        );
        assert_eq!(db.storage().read()[&contract][&U256::ZERO], U256::from(3));

        stopped.store(true, Ordering::Relaxed);
        let methods = handle.join().unwrap();
        assert!(methods.contains(&"eth_getAccountInfo".to_owned()));
        assert!(methods.contains(&"eth_getStorageAt".to_owned()));
    }
}
