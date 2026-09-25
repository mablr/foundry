//! Ethereum cheatcodes executed through evm2 inspection hooks.

use crate::Vm;
use alloy_primitives::{Address, Bytes, U256};
use alloy_sol_types::{SolInterface, SolValue};
use evm2::{
    Inspector,
    bytecode::Bytecode,
    evm::{AccountInfo, Database, Db, EmptyDB, State},
    interpreter::{GasTracker, InstrStop, Interpreter, Message, MessageResult, MessageResultExt},
};
use foundry_evm_core::{
    constants::{CHEATCODE_ADDRESS, CHEATCODE_CONTRACT_HASH, HARDHAT_CONSOLE_ADDRESS},
    native::{FoundryEvmTypes, LocalState, NativeInspector},
};
use std::{collections::BTreeMap, sync::Arc};

/// Cheatcode dispatch for native Ethereum execution.
#[derive(Clone, Debug)]
pub struct NativeCheatcodes<D: Database + Clone = EmptyDB> {
    backend: LocalState<D>,
    pranks: BTreeMap<u16, NativePrank>,
    active_origins: BTreeMap<u16, Option<Address>>,
    snapshots: BTreeMap<U256, Arc<NativeSnapshot<D>>>,
    next_snapshot_id: U256,
    backend_reset: Option<LocalState<D>>,
}

#[derive(Debug)]
struct NativeSnapshot<D: Database + Clone> {
    state: State<'static>,
    block: evm2::env::BlockEnv<FoundryEvmTypes>,
    backend: LocalState<D>,
}

#[derive(Clone, Copy, Debug)]
struct NativePrank {
    caller: Address,
    new_caller: Address,
    new_origin: Option<Address>,
    single_call: bool,
    used: bool,
}

impl Default for NativeCheatcodes<EmptyDB> {
    fn default() -> Self {
        Self::new(LocalState::default())
    }
}

impl<D: Database + Clone + 'static> NativeCheatcodes<D> {
    /// Creates native cheatcodes over the same accepted state as the executor.
    pub const fn new(backend: LocalState<D>) -> Self {
        Self {
            backend,
            pranks: BTreeMap::new(),
            active_origins: BTreeMap::new(),
            snapshots: BTreeMap::new(),
            next_snapshot_id: U256::ONE,
            backend_reset: None,
        }
    }

    /// Installs code at the cheatcode address for Solidity `EXTCODESIZE` checks.
    pub fn install(&self, state: &mut LocalState<D>) {
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

    fn snapshot(&mut self, interp: &mut Interpreter<'_, '_, FoundryEvmTypes>) -> Bytes {
        let host = interp.host();
        let snapshot = NativeSnapshot {
            state: host.state().clone_with(Db::new(self.backend.clone())),
            block: *host.block(),
            backend: self.backend.clone(),
        };
        let id = self.next_snapshot_id;
        self.next_snapshot_id += U256::ONE;
        self.snapshots.insert(id, Arc::new(snapshot));
        id.abi_encode().into()
    }

    fn restore(
        &mut self,
        interp: &mut Interpreter<'_, '_, FoundryEvmTypes>,
        id: U256,
        delete: bool,
    ) -> Bytes {
        let snapshot =
            if delete { self.snapshots.remove(&id) } else { self.snapshots.get(&id).cloned() };
        let Some(snapshot) = snapshot else { return false.abi_encode().into() };
        let host = interp.host();
        let logs = host.state().logs().to_vec();
        *host.state_mut() = snapshot.state.clone_with(Db::new(snapshot.backend.clone()));
        host.state_mut().logs_mut().clone_from(&logs);
        host.set_block(snapshot.block);
        self.backend = snapshot.backend.clone();
        self.backend_reset = Some(snapshot.backend.clone());
        true.abi_encode().into()
    }
}

impl<D: Database + Clone + 'static> NativeInspector<D> for NativeCheatcodes<D> {
    fn set_backend(&mut self, backend: LocalState<D>) {
        self.backend = backend;
        self.backend_reset = None;
    }

    fn take_backend_reset(&mut self) -> Option<LocalState<D>> {
        self.backend_reset.take()
    }
}

impl<D: Database + Clone + 'static> Inspector<FoundryEvmTypes> for NativeCheatcodes<D> {
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
            Ok(Vm::VmCalls::snapshotState(_) | Vm::VmCalls::snapshot(_)) => {
                (InstrStop::Return, self.snapshot(interp))
            }
            Ok(Vm::VmCalls::revertToState(call)) => {
                (InstrStop::Return, self.restore(interp, call.snapshotId, false))
            }
            Ok(Vm::VmCalls::revertTo(call)) => {
                (InstrStop::Return, self.restore(interp, call.snapshotId, false))
            }
            Ok(Vm::VmCalls::revertToStateAndDelete(call)) => {
                (InstrStop::Return, self.restore(interp, call.snapshotId, true))
            }
            Ok(Vm::VmCalls::revertToAndDelete(call)) => {
                (InstrStop::Return, self.restore(interp, call.snapshotId, true))
            }
            Ok(Vm::VmCalls::deleteStateSnapshot(call)) => (
                InstrStop::Return,
                self.snapshots.remove(&call.snapshotId).is_some().abi_encode().into(),
            ),
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
