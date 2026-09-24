//! Ethereum cheatcodes executed through evm2 inspection hooks.

use crate::Vm;
use alloy_primitives::Bytes;
use alloy_sol_types::SolInterface;
use evm2::{
    BaseEvmTypes, Inspector,
    interpreter::{GasTracker, InstrStop, Interpreter, Message, MessageResult, MessageResultExt},
};
use foundry_evm_core::constants::CHEATCODE_ADDRESS;

/// Cheatcode dispatch for native Ethereum execution.
#[derive(Clone, Debug, Default)]
pub struct NativeCheatcodes;

impl Inspector<BaseEvmTypes> for NativeCheatcodes {
    fn call(
        &mut self,
        interp: &mut Interpreter<'_, '_, BaseEvmTypes>,
        message: &mut Message<BaseEvmTypes>,
    ) -> Option<MessageResult<BaseEvmTypes>> {
        if message.call_target != CHEATCODE_ADDRESS {
            return None;
        }

        let stop = match Vm::VmCalls::abi_decode(&message.input) {
            Ok(Vm::VmCalls::deal(call)) => {
                match interp.host().state_mut().account(&call.account, false) {
                    Ok(mut account) => {
                        account.set_balance(call.newBalance);
                        InstrStop::Return
                    }
                    Err(_) => InstrStop::Revert,
                }
            }
            _ => InstrStop::Revert,
        };
        Some(MessageResultExt {
            stop,
            gas: GasTracker::new(message.gas_limit),
            output: Bytes::new(),
            ..Default::default()
        })
    }
}
