//! Ethereum cheatcodes executed through evm2 inspection hooks.

use crate::{BroadcastableTransaction, BroadcastableTransactions, CheatsConfig, Error, Vm};
use alloy_network::{Ethereum, TransactionBuilder};
use alloy_primitives::{Address, B256, Bytes, TxKind, U256, keccak256};
use alloy_rpc_types::TransactionRequest;
use alloy_sol_types::{SolError, SolInterface, SolValue};
use evm2::{
    Inspector,
    bytecode::Bytecode,
    evm::{AccountInfo, Database, Db, EmptyDB, State},
    interpreter::{
        GasTracker, InstrStop, Interpreter, Message, MessageKind, MessageResult, MessageResultExt,
        derive_create_destination,
    },
};
use foundry_common::TransactionMaybeSigned;
use foundry_evm_core::{
    constants::{
        CHEATCODE_ADDRESS, CHEATCODE_CONTRACT_HASH, HARDHAT_CONSOLE_ADDRESS, MAGIC_ASSUME,
    },
    eip2935::{HISTORY_STORAGE_ADDRESS, HISTORY_STORAGE_CODE},
    native::{FoundryEvmTypes, LocalState, NativeInspector},
};
use std::{collections::BTreeMap, sync::Arc};

/// Cheatcode dispatch for native Ethereum execution.
#[derive(Clone, Debug)]
pub struct NativeCheatcodes<D: Database + Clone = EmptyDB> {
    backend: LocalState<D>,
    config: Arc<CheatsConfig>,
    pranks: BTreeMap<u16, NativePrank>,
    active_origins: BTreeMap<u16, Option<Address>>,
    synthetic_origins: BTreeMap<u16, Option<Address>>,
    broadcast: Option<NativeBroadcast>,
    broadcast_transactions: BroadcastableTransactions<Ethereum>,
    expected_revert: Option<NativeExpectedRevert>,
    snapshots: BTreeMap<U256, Arc<NativeSnapshot<D>>>,
    next_snapshot_id: U256,
    backend_reset: Option<LocalState<D>>,
    restored_state: Option<State<'static>>,
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

#[derive(Clone, Copy, Debug)]
struct NativeBroadcast {
    caller: Address,
    sender: Address,
    depth: u16,
    single_call: bool,
    used: bool,
}

#[derive(Clone, Debug)]
struct NativeExpectedRevert {
    depth: u16,
    reason: Option<Bytes>,
    partial_match: bool,
}

/// A `vm.deployCode` request that must execute a nested CREATE message.
pub struct NativeDeployCodeRequest {
    pub path: String,
    pub args: Bytes,
    pub value: U256,
    pub salt: Option<B256>,
}

impl NativeDeployCodeRequest {
    /// Decodes a `vm.deployCode` call from cheatcode calldata.
    pub fn decode(input: &[u8]) -> Option<Self> {
        let (path, args, value, salt) = match Vm::VmCalls::abi_decode(input).ok()? {
            Vm::VmCalls::deployCode_0(call) => (call.artifactPath, Bytes::new(), U256::ZERO, None),
            Vm::VmCalls::deployCode_1(call) => {
                (call.artifactPath, call.constructorArgs, U256::ZERO, None)
            }
            Vm::VmCalls::deployCode_2(call) => (call.artifactPath, Bytes::new(), call.value, None),
            Vm::VmCalls::deployCode_3(call) => {
                (call.artifactPath, call.constructorArgs, call.value, None)
            }
            Vm::VmCalls::deployCode_4(call) => {
                (call.artifactPath, Bytes::new(), U256::ZERO, Some(call.salt))
            }
            Vm::VmCalls::deployCode_5(call) => {
                (call.artifactPath, call.constructorArgs, U256::ZERO, Some(call.salt))
            }
            Vm::VmCalls::deployCode_6(call) => {
                (call.artifactPath, Bytes::new(), call.value, Some(call.salt))
            }
            Vm::VmCalls::deployCode_7(call) => {
                (call.artifactPath, call.constructorArgs, call.value, Some(call.salt))
            }
            _ => return None,
        };
        Some(Self { path, args, value, salt })
    }
}

impl Default for NativeCheatcodes<EmptyDB> {
    fn default() -> Self {
        Self::new(LocalState::default())
    }
}

impl<D: Database + Clone + 'static> NativeCheatcodes<D> {
    /// Creates native cheatcodes over the same accepted state as the executor.
    pub fn new(backend: LocalState<D>) -> Self {
        Self {
            backend,
            config: Arc::new(CheatsConfig::default()),
            pranks: BTreeMap::new(),
            active_origins: BTreeMap::new(),
            synthetic_origins: BTreeMap::new(),
            broadcast: None,
            broadcast_transactions: BroadcastableTransactions::default(),
            expected_revert: None,
            snapshots: BTreeMap::new(),
            next_snapshot_id: U256::ONE,
            backend_reset: None,
            restored_state: None,
        }
    }

    /// Installs the configuration for the running test contract.
    pub fn set_config(&mut self, config: CheatsConfig) {
        self.config = Arc::new(config);
    }

    /// Resolves artifact bytecode with the same rules as the other cheatcodes.
    pub fn artifact_code(&self, path: &str, deployed: bool) -> Result<Bytes, Bytes> {
        crate::artifact::get_artifact_code(&self.config, path, deployed).map_err(Error::encode)
    }

    /// Drains transactions captured by native broadcast cheatcodes.
    pub fn take_broadcast_transactions(&mut self) -> BroadcastableTransactions<Ethereum> {
        std::mem::take(&mut self.broadcast_transactions)
    }

    fn start_broadcast(
        &mut self,
        interp: &mut Interpreter<'_, '_, FoundryEvmTypes>,
        message: &Message<FoundryEvmTypes>,
        sender: Address,
        single_call: bool,
    ) -> (InstrStop, Bytes) {
        let depth = message.depth.saturating_sub(1);
        if self.pranks.range(..=depth).next_back().is_some() {
            return (
                InstrStop::Revert,
                Error::encode(
                    "you have an active prank; broadcasting and pranks are not compatible",
                ),
            );
        }
        if self.broadcast.is_some() {
            return (InstrStop::Revert, Error::encode("a broadcast is active already"));
        }
        if interp.host().state_mut().account(&sender, false).is_err() {
            return (InstrStop::Revert, Bytes::new());
        }
        self.broadcast = Some(NativeBroadcast {
            caller: message.caller,
            sender,
            depth,
            single_call,
            used: false,
        });
        (InstrStop::Return, Bytes::new())
    }

    fn default_broadcast_sender(&self, interp: &Interpreter<'_, '_, FoundryEvmTypes>) -> Address {
        let sender = self.config.evm_opts.sender;
        if sender == foundry_config::Config::DEFAULT_SENDER {
            interp.tx_env().origin
        } else {
            sender
        }
    }

    fn broadcast_error(gas_limit: u64, error: impl Into<Error>) -> MessageResult<FoundryEvmTypes> {
        MessageResultExt {
            stop: InstrStop::Revert,
            gas: GasTracker::new(gas_limit),
            output: Error::encode(error),
            ..Default::default()
        }
    }

    fn broadcast_call(
        &mut self,
        interp: &mut Interpreter<'_, '_, FoundryEvmTypes>,
        message: &mut Message<FoundryEvmTypes>,
    ) -> Option<MessageResult<FoundryEvmTypes>> {
        let depth = message.depth.saturating_sub(1);
        let broadcast = self.broadcast.as_mut()?;
        if depth != broadcast.depth || message.caller != broadcast.caller {
            return None;
        }
        if message.kind == MessageKind::StaticCall {
            if broadcast.single_call {
                return Some(Self::broadcast_error(
                    message.gas_limit,
                    "`staticcall`s are not allowed after `broadcast`; use `startBroadcast` instead",
                ));
            }
            message.caller = broadcast.sender;
            let context = interp.host().ext_mut();
            self.active_origins.insert(depth, context.origin_override);
            context.origin_override = Some(broadcast.sender);
            return None;
        }
        if message.kind != MessageKind::Call {
            return Some(Self::broadcast_error(
                message.gas_limit,
                "native broadcast does not support this call kind",
            ));
        }
        let chain_id = match u64::try_from(interp.tx_env().chain_id) {
            Ok(chain_id) => chain_id,
            Err(_) => return Some(Self::broadcast_error(message.gas_limit, "invalid chain ID")),
        };
        let nonce = match interp.host().state_mut().account(&broadcast.sender, false) {
            Ok(mut account) => {
                let nonce = account.nonce();
                if !account.bump_nonce() {
                    return Some(Self::broadcast_error(
                        message.gas_limit,
                        "broadcast nonce overflow",
                    ));
                }
                nonce
            }
            Err(_) => {
                return Some(Self::broadcast_error(
                    message.gas_limit,
                    "broadcast account unavailable",
                ));
            }
        };
        let request = TransactionRequest::default()
            .with_from(broadcast.sender)
            .with_to(message.call_target)
            .with_value(message.value)
            .with_input(message.input.clone())
            .with_nonce(nonce)
            .with_chain_id(chain_id);
        self.broadcast_transactions.push_back(BroadcastableTransaction {
            rpc: self.config.evm_opts.fork_url.clone(),
            transaction: TransactionMaybeSigned::new(request),
        });
        message.caller = broadcast.sender;
        let context = interp.host().ext_mut();
        self.active_origins.insert(depth, context.origin_override);
        context.origin_override = Some(broadcast.sender);
        broadcast.used = true;
        None
    }

    fn broadcast_create(
        &mut self,
        interp: &mut Interpreter<'_, '_, FoundryEvmTypes>,
        message: &mut Message<FoundryEvmTypes>,
    ) -> Option<MessageResult<FoundryEvmTypes>> {
        let depth = message.depth.saturating_sub(1);
        let broadcast = self.broadcast.as_mut()?;
        if depth != broadcast.depth || message.caller != broadcast.caller {
            return None;
        }
        if message.kind != MessageKind::Create {
            return Some(Self::broadcast_error(
                message.gas_limit,
                "native CREATE2 broadcast requires the CREATE2 factory",
            ));
        }
        let nonce = match interp.host().state_mut().account(&broadcast.sender, false) {
            Ok(account) => account.nonce(),
            Err(_) => {
                return Some(Self::broadcast_error(
                    message.gas_limit,
                    "broadcast account unavailable",
                ));
            }
        };
        let request = TransactionRequest::default()
            .with_from(broadcast.sender)
            .with_kind(TxKind::Create)
            .with_value(message.value)
            .with_input(message.input.clone())
            .with_nonce(nonce);
        self.broadcast_transactions.push_back(BroadcastableTransaction {
            rpc: self.config.evm_opts.fork_url.clone(),
            transaction: TransactionMaybeSigned::new(request),
        });
        message.caller = broadcast.sender;
        message.code_address = broadcast.sender;
        message.destination = derive_create_destination(
            message.kind,
            &broadcast.sender,
            &message.salt,
            &message.input,
            nonce,
        );
        message.call_target = message.destination;
        let context = interp.host().ext_mut();
        self.active_origins.insert(depth, context.origin_override);
        context.origin_override = Some(broadcast.sender);
        broadcast.used = true;
        None
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

    /// Applies a prank to a synthetic creation initiated by a cheatcode call.
    pub fn synthetic_create_caller(
        &mut self,
        interp: &mut Interpreter<'_, '_, FoundryEvmTypes>,
        caller: Address,
        depth: u16,
    ) -> Result<Address, InstrStop> {
        let Some((prank_depth, prank)) = self.pranks.range_mut(..=depth).next_back() else {
            return Ok(caller);
        };
        if depth != *prank_depth || caller != prank.caller {
            return Ok(caller);
        }
        interp
            .host()
            .state_mut()
            .account(&prank.new_caller, false)
            .map_err(|_| InstrStop::Revert)?;
        if let Some(new_origin) = prank.new_origin {
            let context = interp.host().ext_mut();
            self.synthetic_origins.insert(depth, context.origin_override);
            context.origin_override = Some(new_origin);
        }
        prank.used = true;
        Ok(prank.new_caller)
    }

    /// Clears a one-shot prank after a synthetic creation completes.
    pub fn finish_synthetic_create_prank(
        &mut self,
        interp: &mut Interpreter<'_, '_, FoundryEvmTypes>,
        depth: u16,
    ) {
        if let Some(previous_origin) = self.synthetic_origins.remove(&depth) {
            interp.host().ext_mut().origin_override = previous_origin;
        }
        if self.pranks.get(&depth).is_some_and(|prank| prank.single_call && prank.used) {
            self.pranks.remove(&depth);
        }
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
        self.restored_state = Some(host.state().clone_with(Db::new(snapshot.backend.clone())));
        host.set_block(snapshot.block);
        self.backend = snapshot.backend.clone();
        self.backend_reset = Some(snapshot.backend.clone());
        true.abi_encode().into()
    }

    /// Returns state restored while an isolated child transaction was executing.
    pub fn take_restored_state(&mut self) -> Option<(State<'static>, LocalState<D>)> {
        let state = self.restored_state.take()?;
        let backend = self.backend_reset.take()?;
        Some((state, backend))
    }

    fn expect_revert(
        &mut self,
        depth: u16,
        reason: Option<Bytes>,
        partial_match: bool,
    ) -> (InstrStop, Bytes) {
        if self.expected_revert.is_some() {
            return (
                InstrStop::Revert,
                Error::encode("you must call another function prior to expecting a second revert"),
            );
        }
        self.expected_revert = Some(NativeExpectedRevert { depth, reason, partial_match });
        (InstrStop::Return, Bytes::new())
    }

    fn finish_expected_revert(
        &mut self,
        message: &Message<FoundryEvmTypes>,
        result: &mut MessageResult<FoundryEvmTypes>,
    ) {
        if !self
            .expected_revert
            .as_ref()
            .is_some_and(|expected| message.depth <= expected.depth.saturating_add(1))
        {
            return;
        }
        let expected = self.expected_revert.take().unwrap();
        let call_failed = !result.stop.is_success();
        let matched = call_failed
            && expected.reason.as_ref().is_none_or(|reason| {
                if expected.partial_match {
                    reason.get(..4).is_some_and(|expected| result.output.get(..4) == Some(expected))
                } else if result.output == *reason {
                    true
                } else if result.output.starts_with(&alloy_sol_types::Revert::SELECTOR) {
                    String::abi_decode(&result.output[4..])
                        .is_ok_and(|actual| actual.as_bytes() == reason.as_ref())
                } else {
                    false
                }
            });
        if matched && message.depth > expected.depth {
            result.stop = InstrStop::Return;
            if message.kind.is_create() {
                result.created_address = Some(Address::with_last_byte(1));
                result.output = Bytes::new();
            } else {
                result.output = Bytes::from_static(&[0; 8192]);
            }
        } else {
            result.stop = InstrStop::Revert;
            result.created_address = None;
            result.output = Error::encode(if message.depth <= expected.depth {
                "call didn't revert at a lower depth than cheatcode call depth"
            } else if !call_failed {
                "next call did not revert as expected"
            } else {
                "revert data did not match the expected reason"
            });
        }
    }
}

impl<D: Database + Clone + 'static> NativeInspector<D> for NativeCheatcodes<D> {
    fn set_backend(&mut self, backend: LocalState<D>) {
        self.backend = backend;
        self.backend_reset = None;
        self.restored_state = None;
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
            if let Some(result) = self.broadcast_call(interp, message) {
                return Some(result);
            }
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
            Ok(Vm::VmCalls::assume(call)) => {
                if call.condition {
                    (InstrStop::Return, Bytes::new())
                } else {
                    (InstrStop::Revert, Bytes::from_static(MAGIC_ASSUME))
                }
            }
            Ok(Vm::VmCalls::deal(call)) => {
                match interp.host().state_mut().account(&call.account, false) {
                    Ok(mut account) => {
                        account.set_balance(call.newBalance);
                        (InstrStop::Return, Bytes::new())
                    }
                    Err(_) => (InstrStop::Revert, Bytes::new()),
                }
            }
            Ok(Vm::VmCalls::etch(call)) => {
                if interp.host().precompiles().contains(&call.target) {
                    (
                        InstrStop::Revert,
                        Error::encode(format!(
                            "cannot use precompile {} as an argument",
                            call.target
                        )),
                    )
                } else {
                    match Bytecode::new_raw_checked(call.newRuntimeBytecode) {
                        Ok(code) => {
                            let state = interp.host().state_mut();
                            let replaced_history_contract =
                                state.account(&call.target, false).map(|mut account| {
                                    // Replacing the history contract also needs a journaled storage
                                    // wipe.
                                    let replace = call.target == HISTORY_STORAGE_ADDRESS
                                        && account.code_hash() == keccak256(&HISTORY_STORAGE_CODE)
                                        && code.hash_slow() != keccak256(&HISTORY_STORAGE_CODE);
                                    account.set_code_slow(code);
                                    replace
                                });
                            match replaced_history_contract {
                                Ok(replace) => {
                                    if replace {
                                        state.storage(&call.target).wipe_journaled();
                                    }
                                    (InstrStop::Return, Bytes::new())
                                }
                                Err(_) => (InstrStop::Revert, Bytes::new()),
                            }
                        }
                        Err(error) => (
                            InstrStop::Revert,
                            Error::encode(format!("failed to create bytecode: {error}")),
                        ),
                    }
                }
            }
            Ok(Vm::VmCalls::getCode(call)) => match self.artifact_code(&call.artifactPath, false) {
                Ok(code) => (InstrStop::Return, code.abi_encode().into()),
                Err(error) => (InstrStop::Revert, error),
            },
            Ok(Vm::VmCalls::getDeployedCode(call)) => {
                match self.artifact_code(&call.artifactPath, true) {
                    Ok(code) => (InstrStop::Return, code.abi_encode().into()),
                    Err(error) => (InstrStop::Revert, error),
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
            Ok(Vm::VmCalls::broadcast_0(_)) => {
                let sender = self.default_broadcast_sender(interp);
                self.start_broadcast(interp, message, sender, true)
            }
            Ok(Vm::VmCalls::broadcast_1(call)) => {
                self.start_broadcast(interp, message, call.signer, true)
            }
            Ok(Vm::VmCalls::startBroadcast_0(_)) => {
                let sender = self.default_broadcast_sender(interp);
                self.start_broadcast(interp, message, sender, false)
            }
            Ok(Vm::VmCalls::startBroadcast_1(call)) => {
                self.start_broadcast(interp, message, call.signer, false)
            }
            Ok(Vm::VmCalls::stopBroadcast(_)) => {
                if self.broadcast.take().is_some() {
                    (InstrStop::Return, Bytes::new())
                } else {
                    (InstrStop::Revert, Error::encode("no broadcast in progress to stop"))
                }
            }
            Ok(Vm::VmCalls::expectRevert_0(_)) => {
                self.expect_revert(message.depth.saturating_sub(1), None, false)
            }
            Ok(Vm::VmCalls::expectRevert_1(call)) => self.expect_revert(
                message.depth.saturating_sub(1),
                Some(Bytes::copy_from_slice(call.revertData.as_slice())),
                true,
            ),
            Ok(Vm::VmCalls::expectRevert_2(call)) => {
                self.expect_revert(message.depth.saturating_sub(1), Some(call.revertData), false)
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
        result: &mut MessageResult<FoundryEvmTypes>,
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
        if self.broadcast.is_some_and(|broadcast| {
            broadcast.single_call && broadcast.used && broadcast.depth == depth
        }) {
            self.broadcast = None;
        }
        self.finish_expected_revert(message, result);
    }

    fn create(
        &mut self,
        interp: &mut Interpreter<'_, '_, FoundryEvmTypes>,
        message: &mut Message<FoundryEvmTypes>,
    ) -> Option<MessageResult<FoundryEvmTypes>> {
        if let Some(result) = self.broadcast_create(interp, message) {
            return Some(result);
        }
        let depth = message.depth.saturating_sub(1);
        if let Some((prank_depth, prank)) = self.pranks.range_mut(..=depth).next_back()
            && depth == *prank_depth
            && message.caller == prank.caller
        {
            let nonce = match interp.host().state_mut().account(&prank.new_caller, false) {
                Ok(account) => account.nonce(),
                Err(_) => {
                    return Some(MessageResultExt {
                        stop: InstrStop::Revert,
                        gas: GasTracker::new(message.gas_limit),
                        ..Default::default()
                    });
                }
            };
            message.caller = prank.new_caller;
            message.code_address = prank.new_caller;
            message.destination = derive_create_destination(
                message.kind,
                &prank.new_caller,
                &message.salt,
                &message.input,
                nonce,
            );
            message.call_target = message.destination;
            if let Some(new_origin) = prank.new_origin {
                let context = interp.host().ext_mut();
                self.active_origins.insert(depth, context.origin_override);
                context.origin_override = Some(new_origin);
            }
            prank.used = true;
        }
        None
    }

    fn create_end(
        &mut self,
        interp: &mut Interpreter<'_, '_, FoundryEvmTypes>,
        message: &Message<FoundryEvmTypes>,
        result: &mut MessageResult<FoundryEvmTypes>,
    ) {
        let depth = message.depth.saturating_sub(1);
        if let Some(previous_origin) = self.active_origins.remove(&depth) {
            interp.host().ext_mut().origin_override = previous_origin;
        }
        if self.pranks.get(&depth).is_some_and(|prank| prank.single_call && prank.used) {
            self.pranks.remove(&depth);
        }
        if self.broadcast.is_some_and(|broadcast| {
            broadcast.single_call && broadcast.used && broadcast.depth == depth
        }) {
            self.broadcast = None;
        }
        self.finish_expected_revert(message, result);
    }
}
