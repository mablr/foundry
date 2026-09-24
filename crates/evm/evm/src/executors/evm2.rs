//! Initial Ethereum execution path for the evm2 migration.
//!
//! Execution and nested frames use native evm2 state. The database adapter loads on demand;
//! the result sink translates only transaction writes for existing Foundry consumers. Neither
//! adapter reconstructs a REVM journal or runs a REVM interpreter. This boundary is temporary
//! until the backend and result consumers migrate to native state-change interfaces.

use super::{EvmExecutionCancellation, Executor, RawCallResult, sancov::SancovGuard};
use crate::inspectors::{InspectorStack, LogCollector};
use alloy_consensus::{TxLegacy, transaction::Recovered};
use alloy_primitives::{Address, B256, Bytes, Log};
use evm2::{
    BaseEvmTypes, Evm, ExecutionConfig, Inspector, Precompiles, Version,
    env::BlockEnvExt,
    ethereum::{TxEnvelope, ethereum_tx_registry, intrinsic_gas},
    evm::Db,
    interpreter::{GasTracker, InstrStop, Interpreter, Message, MessageResult, MessageResultExt},
};
use foundry_cheatcodes::CheatsConfig;
use foundry_common::ErrorExt;
use foundry_evm_core::{
    FoundryBlock, FoundryTransaction,
    constants::{CHEATCODE_ADDRESS, HARDHAT_CONSOLE_ADDRESS},
    evm::{EthEvmNetwork, EvmEnvFor, FoundryEvmNetwork, TxEnvFor},
    native::StateChangesetCollector,
    state_changes::ExecutionOutput,
    utils::StateChangeset,
};
use std::any::TypeId;

#[derive(Default)]
struct MigrationInspector<'a> {
    config: Option<&'a CheatsConfig>,
    native: foundry_cheatcodes::native::Session,
    unsupported: Option<String>,
    log_collector: Option<LogCollector>,
    cancellation: Option<EvmExecutionCancellation>,
    cancelled: bool,
    script_address: Option<Address>,
    cancellation_poll_counter: u8,
    #[cfg(test)]
    early_exit_test_gate: Option<crate::inspectors::EarlyExitTestGate>,
}

impl Inspector<BaseEvmTypes> for MigrationInspector<'_> {
    fn step(&mut self, interp: &mut Interpreter<'_, '_, BaseEvmTypes>) {
        #[cfg(test)]
        if let Some(gate) = &self.early_exit_test_gate {
            gate.check_step(interp.pc());
        }
        if self.script_address == Some(interp.message().destination)
            && interp.message().destination == interp.message().code_address
            && interp.opcode() == evm2::interpreter::op::ADDRESS
        {
            self.unsupported = Some("Usage of `address(this)` detected in script contract. Script contracts are ephemeral and their addresses should not be relied upon.".into());
            interp.set_stop(InstrStop::Revert);
            return;
        }

        if let Some(cancellation) = &self.cancellation {
            let poll_deadline = self.cancellation_poll_counter == 0;
            self.cancellation_poll_counter = self.cancellation_poll_counter.wrapping_add(1);
            if cancellation.should_stop(poll_deadline) {
                self.cancelled = true;
                interp.set_stop(InstrStop::Stop);
            }
        }
    }

    fn log(&mut self, log: &Log, _host: &mut Evm<'_, BaseEvmTypes>) {
        if let Some(collector) = &mut self.log_collector {
            collector.push_raw_log(log.clone());
        }
    }

    fn call(
        &mut self,
        interp: &mut Interpreter<'_, '_, BaseEvmTypes>,
        message: &mut Message<BaseEvmTypes>,
    ) -> Option<MessageResult<BaseEvmTypes>> {
        if message.code_address != CHEATCODE_ADDRESS
            && message.code_address != HARDHAT_CONSOLE_ADDRESS
        {
            return None;
        }
        if message.code_address == HARDHAT_CONSOLE_ADDRESS {
            let result =
                self.log_collector.as_mut().map(|collector| collector.hardhat_log(&message.input));
            let (stop, output) = match result {
                Some(Err(error)) => (InstrStop::Revert, error.abi_encode_revert()),
                _ => (InstrStop::Return, Bytes::new()),
            };
            return Some(MessageResultExt {
                stop,
                output,
                gas: GasTracker::new(message.gas_limit),
                ..Default::default()
            });
        }
        if message.code_address == CHEATCODE_ADDRESS
            && let Some(config) = self.config
        {
            let result = match foundry_cheatcodes::decode_cheatcode(&message.input) {
                Ok(foundry_cheatcodes::Vm::VmCalls::setEvmVersion(_)) => {
                    // TODO(evm2): Restore live hardfork changes after compiled-entry guards are
                    // complete.
                    self.unsupported = Some("evm2: setEvmVersion is temporarily unsupported; select --evm-version before execution".into());
                    None
                }
                Ok(call) => foundry_cheatcodes::native::dispatch(
                    &call,
                    interp.host(),
                    config,
                    &mut self.native,
                ),
                Err(error) => Some(Err(error)),
            };
            for diagnostic in self.native.diagnostics.drain(..) {
                if let Some(collector) = &mut self.log_collector {
                    collector.push_msg(&diagnostic);
                }
            }
            if let Some(result) = result {
                let (stop, output) = match result {
                    Ok(output) => (InstrStop::Return, output.into()),
                    Err(error) => (InstrStop::Revert, foundry_cheatcodes::Error::encode(error)),
                };
                return Some(MessageResultExt {
                    stop,
                    output,
                    gas: GasTracker::new(message.gas_limit),
                    ..Default::default()
                });
            }
        }
        self.unsupported.get_or_insert_with(|| {
            format!(
                "evm2: unsupported cheatcode (target {}, input {})",
                message.code_address, message.input,
            )
        });
        Some(MessageResultExt {
            stop: InstrStop::Revert,
            gas: GasTracker::new(message.gas_limit),
            ..Default::default()
        })
    }

    fn call_end(
        &mut self,
        interp: &mut Interpreter<'_, '_, BaseEvmTypes>,
        message: &Message<BaseEvmTypes>,
        result: &mut MessageResult<BaseEvmTypes>,
    ) {
        if message.depth == 0 && !result.stop.is_success() {
            // TODO(evm2): Coordinate cleanup with expectRevert and isolated execution when ported.
            while let Some((address, balance)) = self.native.deals.pop() {
                match interp.host().state_mut().account(&address, false) {
                    Ok(mut account) => account.override_balance(balance),
                    Err(error) => {
                        self.unsupported = Some(format!("evm2: deal cleanup failed: {error:?}"));
                    }
                }
            }
        }
    }

    fn create_end(
        &mut self,
        interp: &mut Interpreter<'_, '_, BaseEvmTypes>,
        message: &Message<BaseEvmTypes>,
        result: &mut MessageResult<BaseEvmTypes>,
    ) {
        self.call_end(interp, message, result);
    }
}

impl<FEN: FoundryEvmNetwork> Executor<FEN> {
    pub(super) fn execute_evm2_system_call(
        &self,
        caller: Address,
        destination: Address,
        input: Bytes,
    ) -> eyre::Result<StateChangeset> {
        let (spec, version, block) = native_environment(&self.evm_env, self.inspector())?;
        let mut evm = Evm::<BaseEvmTypes>::new_with_execution_config(
            ExecutionConfig::for_spec_and_version(spec, version),
            spec,
            block,
            ethereum_tx_registry(spec),
            Db::new(self.backend()),
            Precompiles::base(spec),
        );
        let mut writes = StateChangesetCollector::default();
        let _result = evm
            .system_call(evm2::evm::SystemTx::new(destination, input).with_caller(caller))?
            .discard_with(&mut writes)
            .unwrap();
        writes.0.retain(|address, _| *address == destination);
        Ok(writes.0)
    }

    pub(super) fn execute_evm2(
        &self,
        mut evm_env: EvmEnvFor<FEN>,
        tx_env: TxEnvFor<FEN>,
    ) -> eyre::Result<RawCallResult<FEN>> {
        eyre::ensure!(
            TypeId::of::<FEN>() == TypeId::of::<EthEvmNetwork>(),
            "evm2 milestone 1 supports Ethereum only"
        );
        eyre::ensure!(!self.backend().is_in_forking_mode(), "evm2 fork execution is not migrated");
        let stack = self.inspector();
        // TODO(evm2): Port CREATE-to-CREATE2 rewriting through native create hooks.
        eyre::ensure!(
            stack.cheatcodes.as_ref().is_none_or(|cheats| !cheats.config.batch_rewrite_creates),
            "evm2 batch CREATE rewriting is not migrated"
        );
        eyre::ensure!(!stack.enable_isolation, "evm2 isolation is not migrated; use --no-isolate");
        eyre::ensure!(
            stack.tracer.is_none() && stack.printer.is_none(),
            "evm2 trace/debug production is not migrated; use verbosity below 3"
        );
        eyre::ensure!(
            stack.fuzzer.is_none()
                && stack.line_coverage.is_none()
                && stack.edge_coverage.is_none()
                && stack.chisel_state.is_none(),
            "evm2 fuzz/invariant/coverage/Chisel inspection is not migrated"
        );
        eyre::ensure!(
            tx_env.tx_type() == 0 || tx_env.tx_type() == 2,
            "evm2 milestone 1 requires an ordinary synthetic test transaction"
        );
        eyre::ensure!(
            tx_env.gas_price() == 0 && tx_env.max_priority_fee_per_gas().is_none(),
            "evm2 synthetic gas-price overrides are not migrated"
        );
        eyre::ensure!(
            tx_env.access_list_is_empty()
                && tx_env.authorization_list_len() == 0
                && tx_env.blob_versioned_hashes().is_empty(),
            "evm2 access-list, authorization and blob transactions are not migrated"
        );
        let (spec, version, block) = native_environment(&evm_env, stack)?;
        let caller = tx_env.caller();
        let tx = TxLegacy {
            chain_id: tx_env.chain_id(),
            nonce: self.get_nonce(caller)?,
            gas_limit: tx_env.gas_limit(),
            gas_price: 0,
            to: tx_env.kind(),
            value: tx_env.value(),
            input: tx_env.input().clone(),
        };
        let stipend = intrinsic_gas(&version, caller, tx.to, &tx.input, 0, 0, tx.value);
        let mut inspector = MigrationInspector {
            log_collector: stack.log_collector.as_deref().cloned(),
            config: stack.cheatcodes.as_ref().map(|cheats| cheats.config.as_ref()),
            cancellation: stack.execution_cancellation().cloned(),
            #[cfg(test)]
            early_exit_test_gate: stack.early_exit_test_gate(),
            script_address: stack
                .script_execution_inspector
                .as_ref()
                .map(|inspector| inspector.script_address),
            ..Default::default()
        };
        let mut evm = Evm::<BaseEvmTypes>::new_with_execution_config(
            ExecutionConfig::for_spec_and_version(spec, version),
            spec,
            block,
            ethereum_tx_registry(spec),
            Db::new(self.backend()),
            Precompiles::base(spec),
        );
        evm.set_inspector(&mut inspector);
        let _coverage = (stack.sancov_edges || stack.sancov_trace_cmp)
            .then(|| SancovGuard::new(stack.sancov_edges, stack.sancov_trace_cmp));
        let mut writes = StateChangesetCollector::default();
        let result = evm
            .transact(&Recovered::new_unchecked(TxEnvelope::Legacy(tx), caller))?
            .discard_with(&mut writes)
            .unwrap();
        let final_block = *evm.block();
        drop(evm);
        evm_env.block_env.set_timestamp(final_block.timestamp);
        evm_env.block_env.set_number(final_block.number);
        evm_env.block_env.set_beneficiary(final_block.beneficiary);
        evm_env.block_env.set_basefee(final_block.basefee.to());
        evm_env.block_env.set_prevrandao(Some(B256::from(final_block.prevrandao)));

        let mut cheatcodes = stack.cheatcodes.clone();
        if let Some(cheats) = &mut cheatcodes {
            cheats.block = Some(evm_env.block_env.clone());
            cheats.deprecated.extend(inspector.native.deprecated);
        }
        if let Some(error) = inspector.unsupported {
            eyre::bail!(error);
        }
        let exit_reason = result.stop;
        let gas_used = result.tx_gas_used();
        let out = if result.status && tx_env.kind().is_create() {
            Some(ExecutionOutput::Create(result.output.clone(), result.created_address))
        } else if result.status || result.stop.is_revert() {
            Some(ExecutionOutput::Call(result.output.clone()))
        } else {
            None
        };
        tracing::debug!(target: "foundry_evm::evm2", ?spec, ?exit_reason, gas_used, "executed with evm2");
        let mut result = RawCallResult {
            exit_reason: Some(exit_reason),
            reverted: !result.status,
            result: if tx_env.kind().is_create() && result.status {
                Bytes::new()
            } else {
                result.output
            },
            gas_used,
            gas_refunded: result.refunded,
            stipend,
            logs: inspector
                .log_collector
                .and_then(LogCollector::into_captured_logs)
                .unwrap_or_default(),
            state_changeset: writes.0,
            evm_env,
            tx_env,
            cheatcodes,
            execution_cancelled: inspector.cancelled,
            out,
            ..Default::default()
        };
        if stack.sancov_edges {
            SancovGuard::append_edges_into(&mut result);
        }
        if stack.sancov_trace_cmp {
            SancovGuard::drain_cmp_into(&mut result);
        }
        Ok(result)
    }
}

fn native_environment<FEN: FoundryEvmNetwork>(
    evm_env: &EvmEnvFor<FEN>,
    stack: &InspectorStack<FEN>,
) -> eyre::Result<(evm2::SpecId, Version, BlockEnvExt)> {
    if let Some(cheats) = &stack.cheatcodes {
        eyre::ensure!(
            cheats.gas_price.is_none_or(|price| price == 0),
            "evm2 synthetic gas-price overrides are not migrated"
        );
    }
    let block = stack
        .cheatcodes
        .as_ref()
        .and_then(|cheats| cheats.block.as_ref())
        .unwrap_or(&evm_env.block_env);
    foundry_evm_core::native::environment(&evm_env.cfg_env, block)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executors::{EarlyExit, ExecutorBuilder};
    use alloy_primitives::{U256, bytes};
    use alloy_sol_types::SolCall;
    use evm2::{
        bytecode::Bytecode,
        env::TxEnvExt,
        evm::{AccountInfo as NativeAccount, InMemoryDB},
        interpreter::{Host, MessageExt},
    };
    use foundry_evm_core::{backend::Backend, constants::CALLER, decode::RevertDecoder};
    use foundry_evm_networks::NetworkConfigs;
    use revm::database::DatabaseRef;
    use std::{sync::mpsc, thread, time::Duration};

    fn executor() -> Executor<EthEvmNetwork> {
        let mut executor =
            ExecutorBuilder::default().spec_id(evm2::SpecId::CANCUN).gas_limit(1_000_000).build(
                EvmEnvFor::<EthEvmNetwork>::default(),
                TxEnvFor::<EthEvmNetwork>::default(),
                Backend::spawn(None).unwrap(),
                NetworkConfigs::default(),
            );
        executor.set_balance(CALLER, U256::MAX).unwrap();
        executor
    }

    #[test]
    fn native_call_discards_and_transact_commits_storage_without_losing_account() {
        let mut executor = executor();
        let target = Address::repeat_byte(0x11);
        // Store calldata word at slot zero, then return it.
        let code = Bytecode::new_raw(bytes!("60003560005560005460005260206000f3"));
        executor.set_code(target, code.clone()).unwrap();
        executor.set_nonce(target, 7).unwrap();
        executor.set_balance(target, U256::from(123)).unwrap();
        let input = Bytes::copy_from_slice(&U256::from(42).to_be_bytes::<32>());
        let nonce = executor.get_nonce(CALLER).unwrap();

        let call = executor.call_raw(CALLER, target, input.clone(), U256::ZERO).unwrap();
        assert!(!call.reverted);
        assert_eq!(call.result, input);
        assert_eq!(executor.backend().storage_ref(target, U256::ZERO).unwrap(), U256::ZERO);
        assert_eq!(executor.get_nonce(CALLER).unwrap(), nonce);

        let committed = executor.transact_raw(CALLER, target, input.clone(), U256::ZERO).unwrap();
        assert_eq!(committed.result, input);
        assert_eq!(committed.gas_used, call.gas_used);
        assert_eq!(executor.backend().storage_ref(target, U256::ZERO).unwrap(), U256::from(42));
        assert_eq!(executor.get_nonce(CALLER).unwrap(), nonce + 1);
        let account = executor.backend().native_account(target).unwrap().unwrap();
        assert_eq!(account.balance, U256::from(123));
        assert_eq!(account.nonce, 7);
        assert_eq!(account.code_hash, code.hash_slow());
        assert_eq!(
            executor.backend().mem_db().code(account.code_hash).original_bytes(),
            code.original_bytes()
        );

        // Setup mutations must preserve code and committed storage in the native cache.
        executor.set_balance(target, U256::from(456)).unwrap();
        executor.set_account_nonce(target, 8).unwrap();
        assert_eq!(executor.get_balance(target).unwrap(), U256::from(456));
        assert_eq!(executor.get_nonce(target).unwrap(), 8);
        assert!(!executor.is_empty_code(target).unwrap());
        assert_eq!(executor.backend().mem_db().slot(target, U256::ZERO), U256::from(42));
        assert_eq!(executor.backend().mem_db().code(code.hash_slow()), code);
    }

    #[test]
    fn native_top_level_revert_rolls_back_storage_and_preserves_revert_bytes() {
        let mut executor = executor();
        let target = Address::repeat_byte(0x11);
        executor
            .set_code(target, Bytecode::new_raw(bytes!("602a60005560ab60005360016000fd")))
            .unwrap();
        let nonce = executor.get_nonce(CALLER).unwrap();
        let result = executor.transact_raw(CALLER, target, Bytes::new(), U256::ZERO).unwrap();
        assert!(result.reverted);
        assert_eq!(result.exit_reason, Some(InstrStop::Revert));
        assert_eq!(result.result, bytes!("ab"));
        assert_eq!(executor.backend().storage_ref(target, U256::ZERO).unwrap(), U256::ZERO);
        assert_eq!(executor.get_nonce(CALLER).unwrap(), nonce + 1);
    }

    #[test]
    fn native_cancellation_interrupts_before_storage_write() {
        let mut executor = executor();
        let target = Address::repeat_byte(0x11);
        executor.set_code(target, Bytecode::new_raw(bytes!("602a60005500"))).unwrap();
        let signal = EarlyExit::new(false);
        signal.record_ctrl_c();
        executor.inspector_mut().set_early_exit(signal);
        let result = executor.call_raw(CALLER, target, Bytes::new(), U256::ZERO).unwrap();
        assert!(result.execution_cancelled);
        assert_eq!(result.exit_reason, Some(InstrStop::Stop));
        assert!(
            result.state_changeset.get(&target).is_none_or(|account| account.storage.is_empty())
        );
        assert!(result.gas_used < 1_000_000);
    }

    #[test]
    fn native_top_level_revert_restores_deal_balance() {
        let target = Address::repeat_byte(0x11);
        let mut db = InMemoryDB::default();
        db.insert_account_info(&target, NativeAccount::default().with_balance(U256::from(100)));
        let config = CheatsConfig::default();
        let mut inspector = MigrationInspector { config: Some(&config), ..Default::default() };
        let mut evm = Evm::<BaseEvmTypes>::new(
            evm2::SpecId::CANCUN,
            Default::default(),
            ethereum_tx_registry(evm2::SpecId::CANCUN),
            db,
            Precompiles::base(evm2::SpecId::CANCUN),
        );
        evm.set_inspector(&mut inspector);
        // Forward calldata to the cheatcode address, then revert the root frame.
        let mut code = bytes!("365f5f375f5f365f5f73").to_vec();
        code.extend_from_slice(CHEATCODE_ADDRESS.as_slice());
        code.extend_from_slice(&[0x5a, 0xf1, 0x50, 0x5f, 0x5f, 0xfd]);
        let mut message = MessageExt {
            destination: Address::repeat_byte(0x22),
            gas_limit: 100_000,
            input: foundry_cheatcodes::Vm::dealCall {
                account: target,
                newBalance: U256::from(109),
            }
            .abi_encode()
            .into(),
            code: Bytecode::new_raw(code.into()),
            ..Default::default()
        };
        let result = evm.execute_message(&TxEnvExt::default(), &mut message);
        assert_eq!(result.stop, InstrStop::Revert);
        assert_eq!(evm.state_mut().account(&target, false).unwrap().balance(), U256::from(100));
    }

    #[test]
    fn native_system_call_commits_without_bumping_sender_nonce() {
        let mut executor = executor();
        let address = alloy_eips::eip4788::BEACON_ROOTS_ADDRESS;
        executor.set_code(address, Bytecode::new_raw(bytes!("60003560005500"))).unwrap();
        executor.apply_beacon_root(B256::from(U256::from(42))).unwrap();
        assert_eq!(executor.backend().storage_ref(address, U256::ZERO).unwrap(), U256::from(42));
        assert_eq!(executor.get_nonce(alloy_eips::eip4788::SYSTEM_ADDRESS).unwrap(), 0);
    }
    #[test]
    fn native_unknown_opcode_preserves_halt_status() {
        let mut executor = executor();
        let target = Address::repeat_byte(0x11);
        executor.set_code(target, Bytecode::new_raw(bytes!("0c"))).unwrap();
        let result = executor.call_raw(CALLER, target, Bytes::new(), U256::ZERO).unwrap();
        assert!(result.reverted);
        assert_eq!(result.exit_reason, Some(InstrStop::OpcodeNotFound));
        assert_eq!(
            RevertDecoder::default().decode_native(&result.result, result.exit_reason),
            "EvmError: OpcodeNotFound"
        );
    }
    #[test]
    fn native_cancellation_interrupts_an_active_opcode_hook() {
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let signal = EarlyExit::new(false);
        let execution_signal = signal.clone();
        let worker = thread::spawn(move || {
            let mut executor = executor();
            let target = Address::repeat_byte(0x11);
            executor.set_code(target, Bytecode::new_raw(bytes!("602a60005500"))).unwrap();
            executor.inspector_mut().set_early_exit(execution_signal);
            executor.inspector_mut().set_early_exit_test_gate(entered_tx, release_rx, 0);
            let result = executor.call_raw(CALLER, target, Bytes::new(), U256::ZERO).unwrap();
            assert!(result.execution_cancelled);
            assert!(
                result
                    .state_changeset
                    .get(&target)
                    .is_none_or(|account| account.storage.is_empty())
            );
        });
        entered_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        signal.record_ctrl_c();
        release_tx.send(()).unwrap();
        worker.join().unwrap();
    }

    #[test]
    fn native_block_number_advances_for_sequential_simulation() {
        let mut executor = executor();
        let target = Address::with_last_byte(0x42);
        executor.set_code(target, Bytecode::new_raw(bytes!("4360005260206000f3"))).unwrap();
        executor.evm_env_mut().block_env.number = U256::from(41);
        let before = executor.call_raw(CALLER, target, Bytes::new(), U256::ZERO).unwrap();
        assert_eq!(U256::from_be_slice(&before.result), U256::from(41));
        executor.increment_block_number();
        let after = executor.call_raw(CALLER, target, Bytes::new(), U256::ZERO).unwrap();
        assert_eq!(U256::from_be_slice(&after.result), U256::from(42));
    }
}
