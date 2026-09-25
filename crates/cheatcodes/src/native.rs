//! Ethereum cheatcodes executed through evm2 inspection hooks.

use crate::Vm;
use alloy_primitives::{Address, Bytes, U256};
use alloy_sol_types::SolInterface;
use evm2::{
    Inspector,
    bytecode::Bytecode,
    evm::{AccountInfo, Database},
    interpreter::{GasTracker, InstrStop, Interpreter, Message, MessageResult, MessageResultExt},
};
use foundry_evm_core::{
    constants::{CHEATCODE_ADDRESS, CHEATCODE_CONTRACT_HASH, HARDHAT_CONSOLE_ADDRESS},
    native::{FoundryEvmTypes, LocalState},
};
use std::collections::BTreeMap;

/// Cheatcode dispatch for native Ethereum execution.
#[derive(Clone, Debug, Default)]
pub struct NativeCheatcodes {
    pranks: BTreeMap<u16, NativePrank>,
    active_origins: BTreeMap<u16, Option<Address>>,
}

#[derive(Clone, Copy, Debug)]
struct NativePrank {
    caller: Address,
    new_caller: Address,
    new_origin: Option<Address>,
    single_call: bool,
    used: bool,
}

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

    fn start_prank(
        &mut self,
        interp: &mut Interpreter<'_, '_, FoundryEvmTypes>,
        message: &Message<FoundryEvmTypes>,
        new_caller: Address,
        new_origin: Option<Address>,
        single_call: bool,
    ) -> (InstrStop, Bytes) {
        if interp.host().state_mut().account(&new_caller, false).is_err() {
            return (InstrStop::Revert, Bytes::new());
        }
        let depth = message.depth.saturating_sub(1);
        if let Some((_, prank)) = self.pranks.range(..=depth).next_back()
            && (!prank.used || (single_call && !prank.single_call))
        {
            return (InstrStop::Revert, Bytes::new());
        }
        self.pranks.insert(
            depth,
            NativePrank {
                caller: message.caller,
                new_caller,
                new_origin,
                single_call,
                used: false,
            },
        );
        (InstrStop::Return, Bytes::new())
    }
}

impl Inspector<FoundryEvmTypes> for NativeCheatcodes {
    fn call(
        &mut self,
        interp: &mut Interpreter<'_, '_, FoundryEvmTypes>,
        message: &mut Message<FoundryEvmTypes>,
    ) -> Option<MessageResult<FoundryEvmTypes>> {
        if message.call_target != CHEATCODE_ADDRESS {
            let depth = message.depth.saturating_sub(1);
            if let Some((prank_depth, prank)) = self.pranks.range_mut(..=depth).next_back()
                && depth == *prank_depth
                && message.caller == prank.caller
            {
                if interp.host().state_mut().account(&prank.new_caller, false).is_err() {
                    return Some(MessageResultExt {
                        stop: InstrStop::Revert,
                        gas: GasTracker::new(message.gas_limit),
                        ..Default::default()
                    });
                }
                message.caller = prank.new_caller;
                if let Some(new_origin) = prank.new_origin {
                    let context = interp.host().ext_mut();
                    self.active_origins.insert(depth, context.origin_override);
                    context.origin_override = Some(new_origin);
                }
                prank.used = true;
            }
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
            Ok(Vm::VmCalls::warp(call)) => {
                let host = interp.host();
                let mut block = *host.block();
                block.timestamp = call.newTimestamp;
                host.set_block(block);
                (InstrStop::Return, Bytes::new())
            }
            Ok(Vm::VmCalls::prank_0(call)) => {
                self.start_prank(interp, message, call.msgSender, None, true)
            }
            Ok(Vm::VmCalls::prank_1(call)) => {
                self.start_prank(interp, message, call.msgSender, Some(call.txOrigin), true)
            }
            Ok(Vm::VmCalls::startPrank_0(call)) => {
                self.start_prank(interp, message, call.msgSender, None, false)
            }
            Ok(Vm::VmCalls::startPrank_1(call)) => {
                self.start_prank(interp, message, call.msgSender, Some(call.txOrigin), false)
            }
            Ok(Vm::VmCalls::stopPrank(_)) => {
                self.pranks.remove(&message.depth.saturating_sub(1));
                (InstrStop::Return, Bytes::new())
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

    fn call_end(
        &mut self,
        interp: &mut Interpreter<'_, '_, FoundryEvmTypes>,
        message: &Message<FoundryEvmTypes>,
        _result: &mut MessageResult<FoundryEvmTypes>,
    ) {
        if message.call_target == CHEATCODE_ADDRESS
            || message.call_target == HARDHAT_CONSOLE_ADDRESS
        {
            return;
        }
        let depth = message.depth.saturating_sub(1);
        if let Some(previous_origin) = self.active_origins.remove(&depth) {
            interp.host().ext_mut().origin_override = previous_origin;
        }
        if self.pranks.get(&depth).is_some_and(|prank| prank.single_call && prank.used) {
            self.pranks.remove(&depth);
        }
    }
}
