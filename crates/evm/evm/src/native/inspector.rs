//! Inspectors for native Ethereum execution.

use alloy_primitives::Log;
use alloy_sol_types::{SolEvent, SolInterface, SolValue};
use evm2::{
    BaseEvmTypes, EvmTypesHost, Inspector,
    interpreter::{GasTracker, InstrStop, Interpreter, Message, MessageResult, MessageResultExt},
};
use foundry_cheatcodes::native::NativeCheatcodes;
use foundry_common::{ErrorExt, fmt::ConsoleFmt};
use foundry_evm_core::{abi::console, constants::HARDHAT_CONSOLE_ADDRESS, native::LocalState};

/// Native Ethereum inspectors and their per-test observations.
#[derive(Clone, Debug, Default)]
pub struct EthereumInspectorStack {
    cheatcodes: NativeCheatcodes,
    logs: Vec<Log>,
}

impl EthereumInspectorStack {
    /// Installs contracts required by the enabled inspectors.
    pub fn install(&self, state: &mut LocalState) {
        self.cheatcodes.install(state);
    }

    /// Drains logs in observation order, including Hardhat console calls.
    pub fn take_logs(&mut self) -> Vec<Log> {
        std::mem::take(&mut self.logs)
    }
}

impl Inspector<BaseEvmTypes> for EthereumInspectorStack {
    fn log(&mut self, log: &Log, _host: &mut <BaseEvmTypes as EvmTypesHost>::Host<'_>) {
        self.logs.push(log.clone());
    }

    fn call(
        &mut self,
        interp: &mut Interpreter<'_, '_, BaseEvmTypes>,
        message: &mut Message<BaseEvmTypes>,
    ) -> Option<MessageResult<BaseEvmTypes>> {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native::EthereumExecutor;
    use alloy_consensus::{TxLegacy, transaction::Recovered};
    use alloy_primitives::{Address, Bytes, TxKind, U256, keccak256};
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
        let mut executor = EthereumExecutor::new(env, state);
        let mut inspector = EthereumInspectorStack::default();
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
            let result = executor.inspect_transact(&tx, &mut inspector).unwrap();
            assert!(result.status);
            if nonce == 1 {
                assert_eq!(result.logs.len(), 1, "{result:?}");
            }
        }

        let logs = inspector.take_logs();
        assert_eq!(logs.len(), 2, "{logs:?}");
        assert_eq!(decode_console_log(&logs[0]).as_deref(), Some("hello"));
        assert_eq!(logs[1].data.data.as_ref(), &[0x2a]);
        assert!(inspector.take_logs().is_empty());
    }
}
