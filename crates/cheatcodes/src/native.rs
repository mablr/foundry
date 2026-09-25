//! Ethereum cheatcodes executed through evm2 inspection hooks.

use crate::Vm;
use alloy_primitives::{Bytes, U256};
use alloy_sol_types::SolInterface;
use evm2::{
    BaseEvmTypes, Inspector,
    bytecode::Bytecode,
    evm::{AccountInfo, Database},
    interpreter::{GasTracker, InstrStop, Interpreter, Message, MessageResult, MessageResultExt},
};
use foundry_evm_core::{
    constants::{CHEATCODE_ADDRESS, CHEATCODE_CONTRACT_HASH},
    native::LocalState,
};

/// Cheatcode dispatch for native Ethereum execution.
#[derive(Clone, Debug, Default)]
pub struct NativeCheatcodes;

impl NativeCheatcodes {
    /// Installs code at the cheatcode address for Solidity `EXTCODESIZE` checks.
    pub fn install<D: Database + Clone>(&self, state: &mut LocalState<D>) {
        state.database_mut().insert_account_info(
            &CHEATCODE_ADDRESS,
            AccountInfo {
                code_hash: CHEATCODE_CONTRACT_HASH,
                code: Some(Bytecode::new_legacy(Bytes::from_static(&[0]))),
                ..Default::default()
            },
        );
    }
}

impl Inspector<BaseEvmTypes> for NativeCheatcodes {
    fn call(
        &mut self,
        interp: &mut Interpreter<'_, '_, BaseEvmTypes>,
        message: &mut Message<BaseEvmTypes>,
    ) -> Option<MessageResult<BaseEvmTypes>> {
        if message.call_target != CHEATCODE_ADDRESS {
            return None;
        }

        let (stop, output) = match Vm::VmCalls::abi_decode(&message.input) {
            Ok(Vm::VmCalls::deal(call)) => {
                match interp.host().state_mut().account(&call.account, false) {
                    Ok(mut account) => {
                        account.set_balance(call.newBalance);
                        (InstrStop::Return, Bytes::new())
                    }
                    Err(_) => (InstrStop::Revert, Bytes::new()),
                }
            }
            Ok(Vm::VmCalls::load(call)) => {
                let state = interp.host().state_mut();
                let account_loaded = state.account(&call.target, false).is_ok();
                let value = account_loaded.then(|| {
                    state
                        .storage_slot(&call.target, U256::from_be_bytes(call.slot.0), false)
                        .map(|slot| slot.current())
                });
                match value {
                    Some(Ok(value)) => (InstrStop::Return, value.to_be_bytes::<32>().into()),
                    _ => (InstrStop::Revert, Bytes::new()),
                }
            }
            Ok(Vm::VmCalls::store(call)) => {
                if interp.host().precompiles().contains(&call.target) {
                    (InstrStop::Revert, Bytes::new())
                } else {
                    let state = interp.host().state_mut();
                    let account_loaded = state.account(&call.target, false).is_ok();
                    let stored = account_loaded
                        && state
                            .storage_slot(&call.target, U256::from_be_bytes(call.slot.0), false)
                            .map(|mut slot| slot.set(U256::from_be_bytes(call.value.0)))
                            .is_ok();
                    if stored {
                        (InstrStop::Return, Bytes::new())
                    } else {
                        (InstrStop::Revert, Bytes::new())
                    }
                }
            }
            _ => (InstrStop::Revert, Bytes::new()),
        };
        Some(MessageResultExt {
            stop,
            gas: GasTracker::new(message.gas_limit),
            output,
            ..Default::default()
        })
    }
}
