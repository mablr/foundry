//! Inspectors for native Ethereum execution.

use alloy_primitives::{Address, Log, U256};
use alloy_sol_types::{SolEvent, SolInterface, SolValue};
use evm2::{
    EvmTypesHost, Inspector,
    evm::{Database, EmptyDB},
    interpreter::{GasTracker, InstrStop, Interpreter, Message, MessageResult, MessageResultExt},
};
use foundry_cheatcodes::native::NativeCheatcodes;
use foundry_common::{ErrorExt, fmt::ConsoleFmt};
use foundry_evm_core::{
    abi::console,
    constants::HARDHAT_CONSOLE_ADDRESS,
    native::{FoundryEvmTypes, LocalState, NativeInspector},
};
use foundry_evm_traces::native::{CallTraceArena, TracingInspector, TracingInspectorConfig};

/// Native Ethereum inspectors and their per-test observations.
#[derive(Clone, Debug)]
pub struct EthereumInspectorStack<D: Database + Clone = EmptyDB> {
    cheatcodes: NativeCheatcodes<D>,
    logs: Vec<Log>,
    tracing: Option<TracingInspector>,
    traces: Vec<CallTraceArena>,
}

impl<D: Database + Clone + 'static> EthereumInspectorStack<D> {
    /// Creates an inspector stack over the executor's accepted state.
    pub const fn new(backend: LocalState<D>) -> Self {
        Self {
            cheatcodes: NativeCheatcodes::new(backend),
            logs: Vec::new(),
            tracing: None,
            traces: Vec::new(),
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
        if let Some(tracing) = &mut self.tracing {
            let _ = tracing.call(interp, message);
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
        self.cheatcodes.call(interp, message)
    }

    fn call_end(
        &mut self,
        interp: &mut Interpreter<'_, '_, FoundryEvmTypes>,
        message: &Message<FoundryEvmTypes>,
        result: &mut MessageResult<FoundryEvmTypes>,
    ) {
        self.cheatcodes.call_end(interp, message, result);
        if let Some(tracing) = &mut self.tracing {
            tracing.call_end(interp, message, result);
        }
    }

    fn create(
        &mut self,
        interp: &mut Interpreter<'_, '_, FoundryEvmTypes>,
        message: &mut Message<FoundryEvmTypes>,
    ) -> Option<MessageResult<FoundryEvmTypes>> {
        if let Some(tracing) = &mut self.tracing {
            let _ = tracing.create(interp, message);
        }
        None
    }

    fn create_end(
        &mut self,
        interp: &mut Interpreter<'_, '_, FoundryEvmTypes>,
        message: &Message<FoundryEvmTypes>,
        result: &mut MessageResult<FoundryEvmTypes>,
    ) {
        self.cheatcodes.create_end(interp, message, result);
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
        self.cheatcodes.set_backend(backend);
    }

    fn take_backend_reset(&mut self) -> Option<LocalState<D>> {
        self.cheatcodes.take_backend_reset()
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
    use alloy_consensus::{TxLegacy, transaction::Recovered};
    use alloy_primitives::{Bytes, TxKind, keccak256};
    use evm2::{
        SpecId, bytecode::Bytecode, env::BlockEnvExt, ethereum::TxEnvelope, evm::AccountInfo,
    };
    use foundry_evm_core::{decode::decode_console_log, native::EthereumEnv};

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
            assert!(!trace.nodes()[1].trace.steps.is_empty());
        }
        assert!(executor.inspector_mut().take_traces().is_empty());
    }
}
