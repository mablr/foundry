//! Inspectors for native Ethereum execution.

use alloy_consensus::{TxLegacy, transaction::Recovered};
use alloy_primitives::{Address, Bytes, Log, TxKind, U256};
use alloy_sol_types::{SolEvent, SolInterface, SolValue};
use evm2::{
    EvmFeatures, EvmTypesHost, Inspector,
    bytecode::Bytecode,
    ethereum::{TxEnvelope, intrinsic_gas},
    evm::{Database, Db, EmptyDB, State},
    interpreter::{
        GasTracker, Host, InstrStop, Interpreter, Message, MessageExt, MessageKind, MessageResult,
        MessageResultExt, derive_create_destination,
    },
};
use foundry_cheatcodes::{
    CheatsConfig, Error,
    native::{NativeCheatcodes, NativeDeployCodeRequest},
};
use foundry_common::{ErrorExt, fmt::ConsoleFmt};
use foundry_evm_core::{
    abi::console,
    constants::{CHEATCODE_ADDRESS, HARDHAT_CONSOLE_ADDRESS},
    native::{EthereumEnv, FoundryEvmTypes, LocalState, NativeInspector},
};
use foundry_evm_coverage::{HitMaps, NativeLineCoverageCollector};
use foundry_evm_traces::native::{CallTraceArena, TracingInspector, TracingInspectorConfig};

use super::EthereumFactory;

/// Native Ethereum inspectors and their per-test observations.
#[derive(Clone, Debug)]
pub struct EthereumInspectorStack<D: Database + Clone = EmptyDB> {
    cheatcodes: NativeCheatcodes<D>,
    backend: LocalState<D>,
    backend_reset: Option<LocalState<D>>,
    isolate: bool,
    in_isolated_transaction: bool,
    root_state: Option<State<'static>>,
    logs: Vec<Log>,
    tracing: Option<TracingInspector>,
    traces: Vec<CallTraceArena>,
    coverage: Option<NativeLineCoverageCollector>,
}

impl<D: Database + Clone + 'static> EthereumInspectorStack<D> {
    /// Creates an inspector stack over the executor's accepted state.
    pub fn new(backend: LocalState<D>) -> Self {
        Self {
            cheatcodes: NativeCheatcodes::new(backend.clone()),
            backend,
            backend_reset: None,
            isolate: false,
            in_isolated_transaction: false,
            root_state: None,
            logs: Vec::new(),
            tracing: None,
            traces: Vec::new(),
            coverage: None,
        }
    }

    /// Installs contracts required by the enabled inspectors.
    pub fn install(&self, state: &mut LocalState<D>) {
        self.cheatcodes.install(state);
    }

    /// Drains logs in observation order, including Hardhat console calls.
    pub fn take_logs(&mut self) -> Vec<Log> {
        std::mem::take(&mut self.logs)
    }

    /// Enables call and opcode tracing for subsequent executions.
    pub fn enable_tracing(&mut self, config: TracingInspectorConfig) {
        self.tracing = Some(TracingInspector::new(config));
        self.traces.clear();
    }

    /// Enables bytecode hit collection for subsequent executions.
    pub fn enable_coverage(&mut self) {
        self.coverage = Some(NativeLineCoverageCollector::default());
    }

    /// Drains collected bytecode hits, if coverage is enabled.
    pub fn take_coverage(&mut self) -> Option<HitMaps> {
        self.coverage.as_mut().map(NativeLineCoverageCollector::take_maps)
    }

    /// Installs the running test contract's cheatcode configuration.
    pub fn set_cheatcode_config(&mut self, config: CheatsConfig) {
        self.cheatcodes.set_config(config);
    }

    fn deploy_code(
        &mut self,
        interp: &mut Interpreter<'_, '_, FoundryEvmTypes>,
        message: &Message<FoundryEvmTypes>,
        request: NativeDeployCodeRequest,
    ) -> MessageResult<FoundryEvmTypes> {
        let gas = GasTracker::new(message.gas_limit);
        if interp.is_static() {
            return MessageResultExt { stop: InstrStop::Revert, gas, ..Default::default() };
        }
        let code = match self.cheatcodes.artifact_code(&request.path, false) {
            Ok(code) => code,
            Err(output) => {
                return MessageResultExt {
                    stop: InstrStop::Revert,
                    gas,
                    output,
                    ..Default::default()
                };
            }
        };
        let mut input = code.to_vec();
        input.extend_from_slice(&request.args);
        let input = Bytes::from(input);
        let kind = if request.salt.is_some() { MessageKind::Create2 } else { MessageKind::Create };
        let salt = request.salt.unwrap_or_default();
        let depth = message.depth.saturating_sub(1);
        let caller = match self.cheatcodes.synthetic_create_caller(interp, message.caller, depth) {
            Ok(caller) => caller,
            Err(stop) => return MessageResultExt { stop, gas, ..Default::default() },
        };
        let nonce =
            interp.host().state_mut().account(&caller, false).map(|account| account.nonce());
        let nonce = match nonce {
            Ok(nonce) => nonce,
            Err(_) => {
                self.cheatcodes.finish_synthetic_create_prank(interp, depth);
                return MessageResultExt { stop: InstrStop::Revert, gas, ..Default::default() };
            }
        };
        let destination = derive_create_destination(kind, &caller, &salt, &input, nonce);
        let mut create = MessageExt {
            kind,
            depth: message.depth,
            gas_limit: message.gas_limit,
            reservoir: message.reservoir,
            destination,
            call_target: destination,
            caller,
            code: Bytecode::new_legacy(input.clone()),
            input,
            value: request.value,
            code_address: caller,
            disable_precompiles: false,
            caller_is_static: false,
            salt,
            ext: Default::default(),
            _non_exhaustive: (),
        };
        let env = EthereumEnv {
            spec: interp.spec(),
            version: *interp.version(),
            block: *interp.host().block(),
        };
        let origin = interp.host().ext().origin_override.unwrap_or(interp.tx_env().origin);
        let tx_env = interp.tx_env().clone();
        let mut backend = self.backend.clone();
        let accepted = self.backend.clone();
        let state = interp.host().state().clone_with(Db::new(backend.clone()));
        let (result, mut state, block) = {
            let mut evm = EthereumFactory.create(env, Db::new(&mut backend));
            *evm.state_mut() = state;
            evm.ext_mut().origin_override = Some(origin);
            evm.set_inspector(&mut *self);
            let result = Host::execute_message(&mut evm, &tx_env, &mut create);
            let state = evm.state().clone_with(Db::new(accepted));
            (result, state, *evm.block())
        };
        self.cheatcodes.finish_synthetic_create_prank(interp, depth);
        if let Some((_, backend)) = self.cheatcodes.take_restored_state() {
            state = state.clone_with(Db::new(backend.clone()));
            self.backend = backend.clone();
            self.backend_reset = Some(backend);
        } else if let Some(backend) = &self.backend_reset {
            state = state.clone_with(Db::new(backend.clone()));
        }
        *interp.host().state_mut() = state;
        interp.host().set_block(block);
        let (stop, output) = if result.stop.is_success() {
            match result.created_address {
                Some(address) => (InstrStop::Return, Bytes::from(address.abi_encode())),
                None => (InstrStop::Revert, Error::encode("contract creation failed")),
            }
        } else {
            (InstrStop::Revert, result.output)
        };
        MessageResultExt { stop, gas, output, ..Default::default() }
    }

    /// Runs depth-one CALLs as separate transactions while retaining the surrounding frame.
    pub const fn enable_isolation(&mut self) {
        self.isolate = true;
    }

    fn capture_root_state(
        &mut self,
        interp: &mut Interpreter<'_, '_, FoundryEvmTypes>,
        depth: u16,
    ) {
        if self.isolate && !self.in_isolated_transaction && depth == 0 {
            self.root_state = Some(interp.host().state().clone_with(Db::new(self.backend.clone())));
        }
    }

    fn finish_root_state(
        &mut self,
        interp: &mut Interpreter<'_, '_, FoundryEvmTypes>,
        depth: u16,
        stop: InstrStop,
    ) {
        if self.isolate
            && !self.in_isolated_transaction
            && depth == 0
            && let Some(root_state) = self.root_state.take()
            && !stop.is_success()
        {
            *interp.host().state_mut() = root_state.clone_with(Db::new(self.backend.clone()));
            self.backend_reset = Some(self.backend.clone());
        }
    }

    fn isolate_call(
        &mut self,
        interp: &mut Interpreter<'_, '_, FoundryEvmTypes>,
        message: &Message<FoundryEvmTypes>,
    ) -> MessageResult<FoundryEvmTypes> {
        let spec = interp.spec();
        let mut version = *interp.version();
        // A contract is the sender of the synthetic transaction.
        version.features.remove(EvmFeatures::EIP3607);
        let mut block = *interp.host().block();
        let basefee = block.basefee;
        block.basefee = U256::ZERO;
        let origin = interp.host().ext().origin_override.unwrap_or(interp.tx_env().origin);
        let precharged_state = if version.feature(EvmFeatures::EIP8037) && !message.value.is_zero()
        {
            match interp
                .host()
                .target_is_empty_for_new_account_gas(&message.destination, version.features)
            {
                Ok(true) => version.gas_params.new_account_state_gas(),
                Ok(false) => 0,
                Err(stop) => {
                    return MessageResultExt {
                        stop,
                        gas: GasTracker::new(message.gas_limit),
                        ..Default::default()
                    };
                }
            }
        } else {
            0
        };
        let pending = interp.host().state().prepare_isolated_state();
        let nonce = match interp.host().state_mut().account(&message.caller, false) {
            Ok(account) => account.nonce(),
            Err(_) => {
                return MessageResultExt {
                    stop: InstrStop::Revert,
                    gas: GasTracker::new(message.gas_limit),
                    ..Default::default()
                };
            }
        };
        let mut backend = self.backend.clone();
        let stipend = intrinsic_gas(
            &version,
            message.caller,
            TxKind::Call(message.destination),
            &message.input,
            0,
            0,
            message.value,
        );
        let regular_limit = message.gas_limit.saturating_add(stipend);
        if version.feature(EvmFeatures::EIP8037) {
            version.tx_gas_limit_cap = regular_limit;
        }
        let mut tx_gas_limit =
            regular_limit.saturating_add(message.reservoir).saturating_add(precharged_state);
        if version.feature(EvmFeatures::BLOCK_GAS_LIMIT_CHECK) {
            tx_gas_limit = tx_gas_limit.min(u64::try_from(block.gas_limit).unwrap_or(u64::MAX));
        }
        let tx = Recovered::new_unchecked(
            TxEnvelope::Legacy(TxLegacy {
                nonce,
                gas_limit: tx_gas_limit,
                to: TxKind::Call(message.destination),
                input: message.input.clone(),
                value: message.value,
                ..Default::default()
            }),
            message.caller,
        );
        self.in_isolated_transaction = true;
        let (outcome, mut child_block) = {
            let mut evm =
                EthereumFactory.create(EthereumEnv { spec, version, block }, Db::new(&mut backend));
            evm.state_mut().set_pending_state(pending);
            evm.ext_mut().origin_override = Some(origin);
            evm.set_inspector(&mut *self);
            let outcome = evm.transact(&tx).map(|executed| executed.detach());
            (outcome, *evm.block())
        };
        self.in_isolated_transaction = false;
        let Ok(outcome) = outcome else {
            return MessageResultExt {
                stop: InstrStop::Revert,
                gas: GasTracker::new(message.gas_limit),
                ..Default::default()
            };
        };
        let state_gas_spent = outcome.result.state_gas_spent().saturating_sub(precharged_state);
        let execution_gas_spent = outcome.result.execution_gas_spent().saturating_sub(stipend);
        let mut gas =
            GasTracker::new_with_execution_gas_and_reservoir(message.gas_limit, message.reservoir);
        if gas.spend_state(state_gas_spent).and_then(|()| gas.spend(execution_gas_spent)).is_err() {
            return MessageResultExt {
                stop: InstrStop::OutOfGas,
                gas: GasTracker::new_spent_with_reservoir(message.gas_limit, message.reservoir),
                ..Default::default()
            };
        }
        child_block.basefee = basefee;
        interp.host().set_block(child_block);
        if let Some((restored, backend)) = self.cheatcodes.take_restored_state() {
            *interp.host().state_mut() = restored.clone_with(Db::new(backend.clone()));
            self.backend = backend.clone();
            self.backend_reset = Some(backend);
        }
        interp.host().state_mut().merge_isolated_state(outcome.pending_state);
        if outcome.result.status {
            interp.host().state_mut().logs_mut().extend(outcome.result.logs);
        }
        if outcome.result.status {
            gas.set_refunded(outcome.result.refunded as i64);
        }
        MessageResultExt {
            stop: outcome.result.stop,
            gas,
            output: outcome.result.output,
            ..Default::default()
        }
    }

    /// Drains transaction traces in execution order.
    pub fn take_traces(&mut self) -> Vec<CallTraceArena> {
        std::mem::take(&mut self.traces)
    }
}

impl<D: Database + Clone + 'static> Inspector<FoundryEvmTypes> for EthereumInspectorStack<D> {
    fn initialize_interp(&mut self, interp: &mut Interpreter<'_, '_, FoundryEvmTypes>) {
        if let Some(tracing) = &mut self.tracing {
            tracing.initialize_interp(interp);
        }
    }

    fn step(&mut self, interp: &mut Interpreter<'_, '_, FoundryEvmTypes>) {
        if let Some(coverage) = &mut self.coverage {
            coverage.step(interp);
        }
        if let Some(tracing) = &mut self.tracing {
            tracing.step(interp);
        }
    }

    fn step_end(&mut self, interp: &mut Interpreter<'_, '_, FoundryEvmTypes>) {
        if let Some(tracing) = &mut self.tracing {
            tracing.step_end(interp);
        }
    }

    fn log(&mut self, log: &Log, host: &mut <FoundryEvmTypes as EvmTypesHost>::Host<'_>) {
        if let Some(tracing) = &mut self.tracing {
            <TracingInspector as Inspector<FoundryEvmTypes>>::log(tracing, log, host);
        }
        self.logs.push(log.clone());
    }

    fn call(
        &mut self,
        interp: &mut Interpreter<'_, '_, FoundryEvmTypes>,
        message: &mut Message<FoundryEvmTypes>,
    ) -> Option<MessageResult<FoundryEvmTypes>> {
        self.capture_root_state(interp, message.depth);
        if let Some(coverage) = &mut self.coverage {
            let _ = coverage.call(interp, message);
        }
        if let Some(tracing) = &mut self.tracing
            && !(self.in_isolated_transaction && message.depth == 0)
        {
            let mut trace_message = message.clone();
            trace_message.depth += u16::from(self.in_isolated_transaction);
            let _ = tracing.call(interp, &mut trace_message);
        }
        if message.call_target == HARDHAT_CONSOLE_ADDRESS {
            let (stop, output) = match console::hh::ConsoleCalls::abi_decode(&message.input) {
                Ok(call) => {
                    for line in call.fmt(Default::default()).lines() {
                        self.logs.push(Log::new_unchecked(
                            HARDHAT_CONSOLE_ADDRESS,
                            vec![console::ds::log::SIGNATURE_HASH],
                            line.abi_encode().into(),
                        ));
                    }
                    (InstrStop::Return, Default::default())
                }
                Err(error) => (InstrStop::Revert, error.abi_encode_revert()),
            };
            return Some(MessageResultExt {
                stop,
                gas: GasTracker::new(message.gas_limit),
                output,
                ..Default::default()
            });
        }
        if message.call_target == CHEATCODE_ADDRESS
            && let Some(request) = NativeDeployCodeRequest::decode(&message.input)
        {
            return Some(self.deploy_code(interp, message, request));
        }
        if let Some(result) = self.cheatcodes.call(interp, message) {
            return Some(result);
        }
        if self.isolate
            && !self.in_isolated_transaction
            && message.depth == 1
            && message.kind == MessageKind::Call
        {
            return Some(self.isolate_call(interp, message));
        }
        None
    }

    fn call_end(
        &mut self,
        interp: &mut Interpreter<'_, '_, FoundryEvmTypes>,
        message: &Message<FoundryEvmTypes>,
        result: &mut MessageResult<FoundryEvmTypes>,
    ) {
        self.cheatcodes.call_end(interp, message, result);
        self.finish_root_state(interp, message.depth, result.stop);
        if let Some(tracing) = &mut self.tracing
            && !(self.in_isolated_transaction && message.depth == 0)
        {
            tracing.call_end(interp, message, result);
        }
    }

    fn create(
        &mut self,
        interp: &mut Interpreter<'_, '_, FoundryEvmTypes>,
        message: &mut Message<FoundryEvmTypes>,
    ) -> Option<MessageResult<FoundryEvmTypes>> {
        self.capture_root_state(interp, message.depth);
        if let Some(tracing) = &mut self.tracing {
            let mut trace_message = message.clone();
            trace_message.depth += u16::from(self.in_isolated_transaction);
            let _ = tracing.create(interp, &mut trace_message);
        }
        None
    }

    fn create_end(
        &mut self,
        interp: &mut Interpreter<'_, '_, FoundryEvmTypes>,
        message: &Message<FoundryEvmTypes>,
        result: &mut MessageResult<FoundryEvmTypes>,
    ) {
        if let Some(coverage) = &mut self.coverage {
            coverage.create_end(interp, message, result);
        }
        self.cheatcodes.create_end(interp, message, result);
        self.finish_root_state(interp, message.depth, result.stop);
        if let Some(tracing) = &mut self.tracing {
            tracing.create_end(interp, message, result);
        }
    }

    fn selfdestruct(
        &mut self,
        contract: &Address,
        target: &Address,
        value: &U256,
        host: &mut <FoundryEvmTypes as EvmTypesHost>::Host<'_>,
    ) {
        if let Some(tracing) = &mut self.tracing {
            <TracingInspector as Inspector<FoundryEvmTypes>>::selfdestruct(
                tracing, contract, target, value, host,
            );
        }
    }
}

impl<D: Database + Clone + 'static> NativeInspector<D> for EthereumInspectorStack<D> {
    fn set_backend(&mut self, backend: LocalState<D>) {
        self.cheatcodes.set_backend(backend.clone());
        self.backend = backend;
        self.backend_reset = None;
    }

    fn take_backend_reset(&mut self) -> Option<LocalState<D>> {
        self.backend_reset.take().or_else(|| self.cheatcodes.take_backend_reset())
    }

    fn finish_transaction(&mut self, gas_used: u64) {
        if let Some(tracing) = &mut self.tracing {
            tracing.set_transaction_gas_used(gas_used);
            let config = *tracing.config();
            let traces = std::mem::replace(tracing, TracingInspector::new(config)).into_traces();
            self.traces.push(traces);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native::EthereumExecutor;
    use alloy_primitives::keccak256;
    use evm2::{SpecId, env::BlockEnvExt, evm::AccountInfo};
    use foundry_evm_core::decode::decode_console_log;

    #[test]
    fn collects_console_calls_and_opcode_logs_in_order() {
        let sender = Address::with_last_byte(1);
        let target = Address::with_last_byte(0x42);
        let mut state = LocalState::default();
        state.database_mut().insert_account_info(
            &target,
            AccountInfo::default().with_code(Bytecode::new_legacy(Bytes::from_static(&[
                0x60, 0x2a, 0x60, 0x00, 0x53, 0x60, 0x01, 0x60, 0x00, 0xa0, 0x00,
            ]))),
        );
        let env = EthereumEnv::new(
            SpecId::CANCUN,
            BlockEnvExt { gas_limit: U256::from(30_000_000), ..Default::default() },
        );
        let mut executor = EthereumExecutor::new_foundry(env, state);
        let mut console_input = keccak256("log(string)").as_slice()[..4].to_vec();
        console_input.extend(("hello".to_string(),).abi_encode_params());

        for (nonce, to, input) in
            [(0, HARDHAT_CONSOLE_ADDRESS, Bytes::from(console_input)), (1, target, Bytes::new())]
        {
            let tx = Recovered::new_unchecked(
                TxEnvelope::Legacy(TxLegacy {
                    nonce,
                    gas_limit: 100_000,
                    to: TxKind::Call(to),
                    input,
                    ..Default::default()
                }),
                sender,
            );
            let result = executor.transact(&tx).unwrap();
            assert!(result.status);
            if nonce == 1 {
                assert_eq!(result.logs.len(), 1, "{result:?}");
            }
        }

        let logs = executor.inspector_mut().take_logs();
        assert_eq!(logs.len(), 2, "{logs:?}");
        assert_eq!(decode_console_log(&logs[0]).as_deref(), Some("hello"));
        assert_eq!(logs[1].data.data.as_ref(), &[0x2a]);
        assert!(executor.inspector_mut().take_logs().is_empty());
    }

    #[test]
    fn traces_nested_calls_and_opcode_steps() {
        let sender = Address::with_last_byte(1);
        let parent = Address::with_last_byte(0x41);
        let child = Address::with_last_byte(0x42);
        let mut state = LocalState::default();
        state.set_balance(sender, U256::MAX).unwrap();
        state.database_mut().insert_account_info(
            &parent,
            AccountInfo::default().with_code(Bytecode::new_legacy(Bytes::from_static(&[
                0x60, 0x00, 0x60, 0x00, 0x60, 0x00, 0x60, 0x00, 0x60, 0x00, 0x60, 0x42, 0x61, 0xff,
                0xff, 0xf1, 0x00,
            ]))),
        );
        state.database_mut().insert_account_info(
            &child,
            AccountInfo::default().with_code(Bytecode::new_legacy(Bytes::from_static(&[0x00]))),
        );
        let env = EthereumEnv::new(
            SpecId::CANCUN,
            BlockEnvExt { gas_limit: U256::from(30_000_000), ..Default::default() },
        );
        let mut executor = EthereumExecutor::new_foundry(env, state);
        executor
            .inspector_mut()
            .enable_tracing(TracingInspectorConfig { record_steps: true, ..Default::default() });
        let tx = Recovered::new_unchecked(
            TxEnvelope::Legacy(TxLegacy {
                gas_limit: 100_000,
                to: TxKind::Call(parent),
                ..Default::default()
            }),
            sender,
        );

        assert!(executor.transact(&tx).unwrap().status);
        executor.inspector_mut().enable_isolation();
        let second = Recovered::new_unchecked(
            TxEnvelope::Legacy(TxLegacy {
                nonce: 1,
                gas_limit: 100_000,
                to: TxKind::Call(parent),
                ..Default::default()
            }),
            sender,
        );
        assert!(executor.transact(&second).unwrap().status);
        let traces = executor.inspector_mut().take_traces();
        assert_eq!(traces.len(), 2);
        for trace in traces {
            assert_eq!(trace.nodes().len(), 2);
            assert_eq!(trace.nodes()[0].trace.address, parent);
            assert_eq!(trace.nodes()[1].trace.address, child);
            assert!(!trace.nodes()[0].trace.steps.is_empty());
            assert!(
                !trace.nodes()[1].trace.steps.is_empty(),
                "{:?}",
                trace
                    .nodes()
                    .iter()
                    .map(|node| (node.trace.address, node.trace.steps.len()))
                    .collect::<Vec<_>>()
            );
        }
        assert!(executor.inspector_mut().take_traces().is_empty());
    }

    #[test]
    fn isolated_child_write_is_undone_when_parent_reverts() {
        let sender = Address::with_last_byte(1);
        let parent = Address::with_last_byte(0x41);
        let child = Address::with_last_byte(0x42);
        let mut state = LocalState::default();
        state.set_balance(sender, U256::MAX).unwrap();
        state.database_mut().insert_account_info(
            &parent,
            AccountInfo::default().with_code(Bytecode::new_legacy(Bytes::from_static(&[
                0x60, 0x00, 0x60, 0x00, 0x60, 0x00, 0x60, 0x00, 0x60, 0x00, 0x60, 0x42, 0x61, 0xff,
                0xff, 0xf1, 0x5f, 0x5f, 0xfd,
            ]))),
        );
        state.database_mut().insert_account_info(
            &child,
            AccountInfo::default().with_code(Bytecode::new_legacy(Bytes::from_static(&[
                0x60, 0x01, 0x5f, 0x55, 0x00,
            ]))),
        );
        let env = EthereumEnv::new(
            SpecId::CANCUN,
            BlockEnvExt { gas_limit: U256::from(30_000_000), ..Default::default() },
        );
        let mut executor = EthereumExecutor::new_foundry(env, state);
        executor.inspector_mut().enable_isolation();
        let tx = Recovered::new_unchecked(
            TxEnvelope::Legacy(TxLegacy {
                gas_limit: 200_000,
                to: TxKind::Call(parent),
                ..Default::default()
            }),
            sender,
        );

        assert!(!executor.transact(&tx).unwrap().status);
        assert_eq!(
            executor
                .state()
                .database()
                .cache
                .storage
                .get(&child)
                .and_then(|storage| storage.slots.get(&U256::ZERO))
                .copied()
                .unwrap_or_default(),
            U256::ZERO
        );
        assert_eq!(executor.state().database().cache.accounts[&parent].as_ref().unwrap().nonce, 0);
    }

    #[test]
    fn isolated_child_keeps_amsterdam_state_gas_separate() {
        let sender = Address::with_last_byte(1);
        let parent = Address::with_last_byte(0x41);
        let child = Address::with_last_byte(0x42);
        let mut state = LocalState::default();
        state.set_balance(sender, U256::MAX).unwrap();
        state.database_mut().insert_account_info(
            &parent,
            AccountInfo::default().with_code(Bytecode::new_legacy(Bytes::from_static(&[
                0x60, 0x00, 0x60, 0x00, 0x60, 0x00, 0x60, 0x00, 0x60, 0x00, 0x60, 0x42, 0x61, 0xff,
                0xff, 0xf1, 0x00,
            ]))),
        );
        state.database_mut().insert_account_info(
            &child,
            AccountInfo::default().with_code(Bytecode::new_legacy(Bytes::from_static(&[
                0x60, 0x01, 0x5f, 0x55, 0x00,
            ]))),
        );
        let mut env = EthereumEnv::new(
            SpecId::AMSTERDAM,
            BlockEnvExt { gas_limit: U256::from(30_000_000), ..Default::default() },
        );
        env.version.tx_gas_limit_cap = 200_000;
        let mut executor = EthereumExecutor::new_foundry(env, state);
        executor.inspector_mut().enable_isolation();
        let tx = Recovered::new_unchecked(
            TxEnvelope::Legacy(TxLegacy {
                gas_limit: 400_000,
                to: TxKind::Call(parent),
                ..Default::default()
            }),
            sender,
        );

        let result = executor.transact(&tx).unwrap();
        assert!(result.status, "{result:?}");
        assert!(result.state_gas_spent() >= 64 * 1530, "{result:?}");
        assert_eq!(executor.state().database().cache.storage[&child].slots[&U256::ZERO], U256::ONE);
    }

    #[test]
    fn isolated_value_transfer_does_not_charge_new_account_state_twice() {
        let sender = Address::with_last_byte(1);
        let parent = Address::with_last_byte(0x41);
        let target = Address::with_last_byte(0x43);
        let transact = |isolate| {
            let mut state = LocalState::default();
            state.set_balance(sender, U256::MAX).unwrap();
            state.database_mut().insert_account_info(
                &parent,
                AccountInfo { balance: U256::ONE, ..Default::default() }.with_code(
                    Bytecode::new_legacy(Bytes::from_static(&[
                        0x5f, 0x5f, 0x5f, 0x5f, 0x60, 0x01, 0x60, 0x43, 0x61, 0xff, 0xff, 0xf1,
                        0x00,
                    ])),
                ),
            );
            let mut env = EthereumEnv::new(
                SpecId::AMSTERDAM,
                BlockEnvExt { gas_limit: U256::from(30_000_000), ..Default::default() },
            );
            env.version.tx_gas_limit_cap = 200_000;
            let mut executor = EthereumExecutor::new_foundry(env, state);
            if isolate {
                executor.inspector_mut().enable_isolation();
            }
            let tx = Recovered::new_unchecked(
                TxEnvelope::Legacy(TxLegacy {
                    gas_limit: 400_000,
                    to: TxKind::Call(parent),
                    ..Default::default()
                }),
                sender,
            );
            let result = executor.transact(&tx).unwrap();
            assert!(result.status, "{result:?}");
            assert_eq!(
                executor.state().database().cache.accounts[&target].as_ref().unwrap().balance,
                U256::ONE
            );
            result.state_gas_spent()
        };

        assert_eq!(transact(true), transact(false));
    }

    #[test]
    fn isolated_revert_restores_amsterdam_state_gas_reservoir() {
        let sender = Address::with_last_byte(1);
        let parent = Address::with_last_byte(0x41);
        let child = Address::with_last_byte(0x42);
        let transact = |isolate| {
            let mut state = LocalState::default();
            state.set_balance(sender, U256::MAX).unwrap();
            state.database_mut().insert_account_info(
                &parent,
                AccountInfo::default().with_code(Bytecode::new_legacy(Bytes::from_static(&[
                    0x5f, 0x5f, 0x5f, 0x5f, 0x5f, 0x60, 0x42, 0x61, 0xff, 0xff, 0xf1, 0x50, 0x60,
                    0x01, 0x60, 0x01, 0x55, 0x00,
                ]))),
            );
            state.database_mut().insert_account_info(
                &child,
                AccountInfo::default().with_code(Bytecode::new_legacy(Bytes::from_static(&[
                    0x60, 0x01, 0x5f, 0x55, 0x5f, 0x5f, 0xfd,
                ]))),
            );
            let mut env = EthereumEnv::new(
                SpecId::AMSTERDAM,
                BlockEnvExt { gas_limit: U256::from(30_000_000), ..Default::default() },
            );
            env.version.tx_gas_limit_cap = 200_000;
            let mut executor = EthereumExecutor::new_foundry(env, state);
            if isolate {
                executor.inspector_mut().enable_isolation();
            }
            let tx = Recovered::new_unchecked(
                TxEnvelope::Legacy(TxLegacy {
                    gas_limit: 400_000,
                    to: TxKind::Call(parent),
                    ..Default::default()
                }),
                sender,
            );
            let result = executor.transact(&tx).unwrap();
            assert!(result.status, "{result:?}");
            assert_eq!(
                executor.state().database().cache.storage[&parent].slots[&U256::ONE],
                U256::ONE
            );
            assert_eq!(result.state_gas_spent(), 64 * 1530);
            (result.state_gas_spent(), result.execution_gas_spent())
        };

        assert_eq!(transact(true), transact(false));
    }
}
