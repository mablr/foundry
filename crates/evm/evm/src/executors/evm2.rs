//! Initial Ethereum execution path for the evm2 migration.
//!
//! Execution and nested frames use native evm2 state. The database adapter loads on demand;
//! the result sink translates only transaction writes for existing Foundry consumers. Neither
//! adapter reconstructs a REVM journal or runs a REVM interpreter. This boundary is temporary
//! until the backend and result consumers migrate to native state-change interfaces.

use super::{EvmExecutionCancellation, Executor, RawCallResult};
use alloy_consensus::{TxLegacy, transaction::Recovered};
use alloy_primitives::{Address, B256, Bytes, Log, U256};
use evm2::{
    BaseEvmTypes, Evm, EvmFeatures, ExecutionConfig, Inspector, Precompiles, Version,
    bytecode::Bytecode as NativeBytecode,
    env::BlockEnvExt,
    ethereum::{TxEnvelope, ethereum_tx_registry, intrinsic_gas},
    evm::{AccountChangeRef, AccountInfo as NativeAccount, Db, StateChangeSink, StorageChange},
    interpreter::{GasTracker, InstrStop, Interpreter, Message, MessageResult, MessageResultExt},
};
use foundry_evm_core::{
    backend::{Backend, DatabaseError},
    constants::{CHEATCODE_ADDRESS, HARDHAT_CONSOLE_ADDRESS},
    evm::{EthEvmNetwork, EvmEnvFor, FoundryEvmNetwork, TxEnvFor},
    utils::StateChangeset,
};
use revm::{
    bytecode::Bytecode,
    context::{Block, Cfg, Transaction},
    context_interface::result::Output,
    database::DatabaseRef,
    interpreter::InstructionResult,
    primitives::hardfork::SpecId,
    state::{AccountInfo, EvmStorageSlot, TransactionId},
};
use std::{any::TypeId, convert::Infallible};

struct BackendReads<'a, FEN: FoundryEvmNetwork>(&'a Backend<FEN>);

impl<FEN: FoundryEvmNetwork> evm2::evm::Database for BackendReads<'_, FEN> {
    type Error = DatabaseError;

    fn get_account(&mut self, address: &Address) -> Result<Option<NativeAccount>, Self::Error> {
        Ok(self.0.basic_ref(*address)?.map(|info| NativeAccount {
            balance: info.balance,
            nonce: info.nonce,
            code_hash: info.code_hash,
            code: info.code.map(|code| NativeBytecode::new_raw(code.original_bytes())),
            ..Default::default()
        }))
    }

    fn get_code_by_hash(&mut self, hash: &B256) -> Result<NativeBytecode, Self::Error> {
        self.0.code_by_hash_ref(*hash).map(|code| NativeBytecode::new_raw(code.original_bytes()))
    }

    fn get_storage(&mut self, address: &Address, key: &U256) -> Result<U256, Self::Error> {
        self.0.storage_ref(*address, *key)
    }

    fn get_block_hash(&mut self, number: &U256) -> Result<B256, Self::Error> {
        self.0.block_hash_ref(number.saturating_to())
    }
}

#[derive(Default)]
struct Writes(StateChangeset);

impl StateChangeSink for Writes {
    type Error = Infallible;

    fn account(&mut self, change: AccountChangeRef<'_>) -> Result<(), Self::Error> {
        let account = self.0.entry(change.address).or_default();
        if let Some(info) = change.current {
            account.info = AccountInfo {
                balance: info.balance,
                nonce: info.nonce,
                code_hash: info.code_hash,
                code: info.code.as_ref().map(|code| Bytecode::new_raw(code.original_bytes())),
                ..Default::default()
            };
        }
        account.mark_touch();
        if change.created {
            account.mark_created();
            account.mark_created_locally();
        }
        if change.selfdestructed || change.current.is_none() {
            account.mark_selfdestruct();
        }
        Ok(())
    }

    fn storage(&mut self, change: StorageChange) -> Result<(), Self::Error> {
        let account = self.0.entry(change.address).or_default();
        account.mark_touch();
        account.storage.insert(
            change.key,
            EvmStorageSlot::new_changed(change.original, change.current, TransactionId::ZERO),
        );
        Ok(())
    }

    fn account_read(
        &mut self,
        address: Address,
        info: Option<&NativeAccount>,
    ) -> Result<(), Self::Error> {
        // Native storage changes precede account callbacks. A storage-only write still needs
        // the unchanged account metadata when committed through Foundry's current backend.
        if let Some(account) = self.0.get_mut(&address)
            && let Some(info) = info
        {
            account.info = AccountInfo {
                balance: info.balance,
                nonce: info.nonce,
                code_hash: info.code_hash,
                code: info.code.as_ref().map(|code| Bytecode::new_raw(code.original_bytes())),
                ..Default::default()
            };
        }
        Ok(())
    }
}

#[derive(Default)]
struct MigrationInspector {
    unsupported: Option<String>,
    logs: Vec<Log>,
    cancellation: Option<EvmExecutionCancellation>,
    cancelled: bool,
    cancellation_poll_counter: u8,
}

impl Inspector<BaseEvmTypes> for MigrationInspector {
    fn step(&mut self, interp: &mut Interpreter<'_, '_, BaseEvmTypes>) {
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
        self.logs.push(log.clone());
    }

    fn call(
        &mut self,
        _interp: &mut Interpreter<'_, '_, BaseEvmTypes>,
        message: &mut Message<BaseEvmTypes>,
    ) -> Option<MessageResult<BaseEvmTypes>> {
        if message.code_address != CHEATCODE_ADDRESS
            && message.code_address != HARDHAT_CONSOLE_ADDRESS
        {
            return None;
        }
        self.unsupported.get_or_insert_with(|| {
            format!(
                "evm2 milestone 1: cheatcode/console calls are not migrated (target {}, input {})",
                message.code_address, message.input,
            )
        });
        Some(MessageResultExt {
            stop: InstrStop::Revert,
            gas: GasTracker::new(message.gas_limit),
            ..Default::default()
        })
    }
}

impl<FEN: FoundryEvmNetwork> Executor<FEN> {
    pub(super) fn execute_evm2(
        &self,
        evm_env: EvmEnvFor<FEN>,
        tx_env: TxEnvFor<FEN>,
    ) -> eyre::Result<RawCallResult<FEN>> {
        eyre::ensure!(
            TypeId::of::<FEN>() == TypeId::of::<EthEvmNetwork>(),
            "evm2 milestone 1 supports Ethereum only"
        );
        eyre::ensure!(!self.backend().is_in_forking_mode(), "evm2 fork execution is not migrated");
        let stack = self.inspector();
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
            stack.script_execution_inspector.is_none()
                && !stack.sancov_edges
                && !stack.sancov_trace_cmp,
            "evm2 script and native coverage inspection is not migrated"
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
            tx_env.access_list().is_none_or(|mut list| list.next().is_none())
                && tx_env.authorization_list_len() == 0
                && tx_env.blob_versioned_hashes().is_empty(),
            "evm2 access-list, authorization and blob transactions are not migrated"
        );
        let cfg = &evm_env.cfg_env;
        let spec = match cfg.spec.into() {
            SpecId::MERGE => evm2::SpecId::MERGE,
            SpecId::SHANGHAI => evm2::SpecId::SHANGHAI,
            SpecId::CANCUN => evm2::SpecId::CANCUN,
            SpecId::PRAGUE => evm2::SpecId::PRAGUE,
            SpecId::OSAKA => evm2::SpecId::OSAKA,
            other => eyre::bail!("evm2 milestone 1 hardfork not yet wired: {other:?}"),
        };
        let mut version = Version::new(spec);
        version.chain_id = cfg.chain_id();
        version.tx_gas_limit_cap = cfg.tx_gas_limit_cap();
        version.memory_limit = cfg.memory_limit();
        version.max_code_size = cfg.max_code_size();
        version.max_initcode_size = cfg.max_initcode_size();
        for (feature, enabled) in [
            (EvmFeatures::NONCE_CHECK, !cfg.is_nonce_check_disabled()),
            (EvmFeatures::BALANCE_CHECK, !cfg.is_balance_check_disabled()),
            (EvmFeatures::BLOCK_GAS_LIMIT_CHECK, !cfg.is_block_gas_limit_disabled()),
            (EvmFeatures::BASE_FEE_CHECK, !cfg.is_base_fee_check_disabled()),
            (EvmFeatures::EIP3607, !cfg.is_eip3607_disabled()),
            (EvmFeatures::FEE_CHARGE, !cfg.is_fee_charge_disabled()),
            (EvmFeatures::TX_CHAIN_ID_CHECK, cfg.tx_chain_id_check()),
        ] {
            version.features.set(feature, enabled);
        }
        if cfg.is_eip7623_disabled() {
            version.features.remove(EvmFeatures::EIP7623);
        }
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
        let block = BlockEnvExt {
            number: block.number(),
            beneficiary: block.beneficiary(),
            timestamp: block.timestamp(),
            gas_limit: U256::from(block.gas_limit()),
            basefee: U256::from(block.basefee()),
            difficulty: block.difficulty(),
            prevrandao: U256::from_be_bytes(block.prevrandao().unwrap_or_default().0),
            blob_basefee: U256::from(block.blob_gasprice().unwrap_or_default()),
            slot_num: U256::from(block.slot_num()),
            ..Default::default()
        };
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
            cancellation: stack.execution_cancellation().cloned(),
            ..Default::default()
        };
        let mut evm = Evm::<BaseEvmTypes>::new_with_execution_config(
            ExecutionConfig::for_spec_and_version(spec, version),
            spec,
            block,
            ethereum_tx_registry(spec),
            Db::new(BackendReads(self.backend())),
            Precompiles::base(spec),
        );
        evm.set_inspector(&mut inspector);
        let mut writes = Writes::default();
        let result = evm
            .transact(&Recovered::new_unchecked(TxEnvelope::Legacy(tx), caller))?
            .discard_with(&mut writes)
            .unwrap();
        drop(evm);
        if let Some(error) = inspector.unsupported {
            eyre::bail!(error);
        }
        let exit_reason = instruction_result(result.stop)?;
        let gas_used = result.tx_gas_used();
        let out = if result.status && tx_env.kind().is_create() {
            Some(Output::Create(result.output.clone(), result.created_address))
        } else if result.status || result.stop.is_revert() {
            Some(Output::Call(result.output.clone()))
        } else {
            None
        };
        tracing::debug!(target: "foundry_evm::evm2", ?spec, ?exit_reason, gas_used, "executed with evm2");
        Ok(RawCallResult {
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
            logs: inspector.logs,
            state_changeset: writes.0,
            evm_env,
            tx_env,
            cheatcodes: stack.cheatcodes.clone(),
            execution_cancelled: inspector.cancelled,
            out,
            ..Default::default()
        })
    }
}

fn instruction_result(stop: InstrStop) -> eyre::Result<InstructionResult> {
    Ok(match stop {
        InstrStop::Stop => InstructionResult::Stop,
        InstrStop::Return => InstructionResult::Return,
        InstrStop::SelfDestruct => InstructionResult::SelfDestruct,
        InstrStop::Revert => InstructionResult::Revert,
        InstrStop::OutOfGas => InstructionResult::OutOfGas,
        InstrStop::MemoryOOG => InstructionResult::MemoryOOG,
        InstrStop::MemoryLimitOOG => InstructionResult::MemoryLimitOOG,
        InstrStop::InvalidFEOpcode => InstructionResult::InvalidFEOpcode,
        InstrStop::InvalidJump => InstructionResult::InvalidJump,
        InstrStop::StackUnderflow => InstructionResult::StackUnderflow,
        InstrStop::StackOverflow => InstructionResult::StackOverflow,
        InstrStop::PrecompileOOG => InstructionResult::PrecompileOOG,
        InstrStop::PrecompileError => InstructionResult::PrecompileError,
        InstrStop::CreateContractSizeLimit => InstructionResult::CreateContractSizeLimit,
        InstrStop::CreateContractStartingWithEF => InstructionResult::CreateContractStartingWithEF,
        InstrStop::CreateInitCodeSizeLimit => InstructionResult::CreateInitCodeSizeLimit,
        other => eyre::bail!("evm2 outcome not yet mapped: {other:?}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executors::{EarlyExit, ExecutorBuilder};
    use alloy_primitives::bytes;
    use foundry_evm_core::constants::CALLER;
    use foundry_evm_networks::NetworkConfigs;

    fn executor() -> Executor<EthEvmNetwork> {
        let mut executor =
            ExecutorBuilder::default().spec_id(SpecId::CANCUN).gas_limit(1_000_000).build(
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
        let account = executor.backend().basic_ref(target).unwrap().unwrap();
        assert_eq!(account.balance, U256::from(123));
        assert_eq!(account.nonce, 7);
        assert_eq!(account.code_hash, code.hash_slow());
        assert_eq!(
            executor.backend().code_by_hash_ref(account.code_hash).unwrap().original_bytes(),
            code.original_bytes()
        );
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
        assert_eq!(result.exit_reason, Some(InstructionResult::Revert));
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
        assert_eq!(result.exit_reason, Some(InstructionResult::Stop));
        assert!(
            result.state_changeset.get(&target).is_none_or(|account| account.storage.is_empty())
        );
        assert!(result.gas_used < 1_000_000);
    }
}
