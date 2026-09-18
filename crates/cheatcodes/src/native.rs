//! Cheatcodes whose execution-state contract has been migrated to native evm2.
//!
//! Unsupported calls are distinct from Solidity reverts: the executor must reject
//! the test even if Solidity catches the call. No REVM context or journal is built.

use crate::{
    Cheatcode, CheatcodeDef, CheatsConfig, Error, Result, Vm,
    evm::prank::Prank,
    test::{
        expect::{ExpectedRevert, ExpectedRevertKind},
        revert_handlers,
    },
};
use alloy_primitives::{Address, Bytes, U256};
use alloy_sol_types::SolValue;
use evm2::{
    BaseEvmTypes, Evm, SpecId,
    bytecode::Bytecode,
    interpreter::{Message, MessageKind, derive_create_destination},
};
use foundry_evm_core::{
    backend::GLOBAL_FAIL_SLOT,
    constants::{CHEATCODE_ADDRESS, HARDHAT_CONSOLE_ADDRESS},
    eip2935::HISTORY_STORAGE_ADDRESS,
};
use revm::interpreter::InstructionResult;
use std::collections::BTreeMap;

/// Engine-independent state retained between native executions.
#[derive(Clone, Debug, Default)]
pub struct Session {
    /// Revert expectations.
    pub expectations: Expectations,
    /// Depth-scoped caller overrides.
    pub pranks: BTreeMap<usize, Prank>,
    /// Diagnostics emitted by the current cheatcode.
    pub diagnostics: Vec<String>,
}

/// The calling frame's identity for cheatcode dispatch.
pub struct CallContext<'a> {
    /// Intercepted cheatcode message.
    pub message: &'a Message,
    /// Current transaction origin.
    pub origin: Address,
}

impl Session {
    /// Applies sender overrides before native CALL/CREATE preparation.
    pub fn prepare(
        &mut self,
        host: &mut Evm<'_, BaseEvmTypes>,
        message: &mut Message,
    ) -> Result<()> {
        let depth = usize::from(message.depth);
        let Some((_, prank)) = self.pranks.range_mut(..=depth).next_back() else {
            return Ok(());
        };
        ensure!(prank.new_origin.is_none(), "evm2 prank origin override is not migrated");
        if depth == prank.depth && prank.delegate_call && message.kind == MessageKind::DelegateCall
        {
            message.destination = prank.new_caller;
            message.caller = prank.new_caller;
        }
        if depth == prank.depth && message.caller == prank.prank_caller {
            let mut account = host
                .state_mut()
                .account(&prank.new_caller, false)
                .map_err(|error| fmt_err!("native state access failed: {error:?}"))?;
            account.warm();
            account.touch();
            let nonce = account.nonce();
            message.caller = prank.new_caller;
            if message.kind.is_create() {
                message.destination = derive_create_destination(
                    message.kind,
                    &message.caller,
                    &message.salt,
                    &message.input,
                    nonce,
                );
                message.code_address = message.destination;
            }
            prank.used = true;
        }
        Ok(())
    }

    /// Consumes a one-shot override when its non-cheatcode call returns.
    pub fn finish(&mut self, message: &Message) {
        let depth = usize::from(message.depth);
        if self.pranks.get(&depth).is_some_and(|prank| prank.single_call) {
            if message.kind.is_create() {
                self.pranks.clear();
            } else {
                self.pranks.remove(&depth);
            }
        }
    }

    fn dispatch_prank(
        &mut self,
        call: &Vm::VmCalls,
        host: &mut Evm<'_, BaseEvmTypes>,
        context: &CallContext<'_>,
    ) -> Option<Result> {
        let depth = usize::from(context.message.depth);
        let (caller, single, delegate) = match call {
            Vm::VmCalls::prank_0(c) => (c.msgSender, true, false),
            Vm::VmCalls::startPrank_0(c) => (c.msgSender, false, false),
            Vm::VmCalls::prank_2(c) => (c.msgSender, true, c.delegateCall),
            Vm::VmCalls::startPrank_2(c) => (c.msgSender, false, c.delegateCall),
            Vm::VmCalls::readCallers(_) => {
                let (mode, sender) = self.pranks.range(..=depth).next_back().map_or(
                    (Vm::CallerMode::None, context.origin),
                    |(_, prank)| {
                        (
                            if prank.single_call {
                                Vm::CallerMode::Prank
                            } else {
                                Vm::CallerMode::RecurrentPrank
                            },
                            prank.new_caller,
                        )
                    },
                );
                return Some(Ok((mode, sender, context.origin).abi_encode_params()));
            }
            Vm::VmCalls::stopPrank(_) => {
                self.pranks.remove(&depth);
                return Some(Ok(Vec::new()));
            }
            _ => return None,
        };
        Some((|| -> Result {
            let mut account = host
                .state_mut()
                .account(&caller, false)
                .map_err(|error| fmt_err!("native state access failed: {error:?}"))?;
            account.warm();
            account.touch();
            if delegate {
                ensure!(
                    !account
                        .load_code()
                        .map_err(|error| fmt_err!("native state access failed: {error:?}"))?
                        .is_empty(),
                    "cannot `prank` delegate call from an EOA"
                );
            }
            if let Some((_, current)) = self.pranks.range(..=depth).next_back() {
                ensure!(current.used, "cannot overwrite a prank until it is applied at least once");
                ensure!(
                    single == current.single_call,
                    "cannot override an ongoing prank with a single vm.prank; use vm.startPrank to override the current prank"
                );
            }
            self.pranks.insert(
                depth,
                Prank::new(
                    context.message.caller,
                    context.origin,
                    caller,
                    None,
                    depth,
                    single,
                    delegate,
                ),
            );
            Ok(Vec::new())
        })())
    }
}

/// Revert expectations retained independently of the native journal.
#[derive(Clone, Debug, Default)]
pub struct Expectations {
    /// Pending expectation, shared with the existing Foundry session boundary.
    pub revert: Option<ExpectedRevert>,
}

impl Expectations {
    /// Records entry to a non-cheatcode execution frame.
    pub fn enter(&mut self, depth: usize) {
        if let Some(expected) = &mut self.revert {
            expected.max_depth = expected.max_depth.max(depth);
        }
    }

    /// Processes an outcome after native frame settlement.
    pub fn finish(
        &mut self,
        message: &evm2::interpreter::Message,
        status: InstructionResult,
        output: &Bytes,
        config: &CheatsConfig,
    ) -> Option<Result<(Option<Address>, Bytes)>> {
        let expected = self.revert.as_mut()?;
        let cheatcode = message.code_address == CHEATCODE_ADDRESS
            || message.code_address == HARDHAT_CONSOLE_ADDRESS;
        let failed = !status.is_ok();
        if failed
            && expected.reverter.is_some()
            && (expected.reverted_by.is_none() || expected.count > 1)
        {
            expected.reverted_by = Some(message.destination);
        }
        if usize::from(message.depth) > expected.depth {
            return None;
        }
        let process = match &mut expected.kind {
            ExpectedRevertKind::Default => {
                !cheatcode && (failed || !config.internal_expect_revert || message.depth == 0)
            }
            ExpectedRevertKind::Cheatcode { pending_processing } => {
                let process = cheatcode && !*pending_processing;
                *pending_processing = false;
                process
            }
        };
        if !process {
            return None;
        }
        let mut expected = self.revert.take().unwrap();
        let result = revert_handlers::handle_expect_revert(
            cheatcode,
            message.kind.is_create(),
            config.internal_expect_revert,
            &expected,
            status,
            output.clone(),
            &config.available_artifacts,
        );
        if result.is_ok() {
            expected.actual_count += 1;
            if expected.actual_count < expected.count {
                self.revert = Some(expected);
            }
        }
        Some(result)
    }
}

/// Applies a migrated call, or returns `None` when its semantics are not migrated.
pub fn dispatch(
    call: &Vm::VmCalls,
    host: &mut Evm<'_, BaseEvmTypes>,
    config: &CheatsConfig,
    session: &mut Session,
    context: CallContext<'_>,
) -> Option<Result> {
    macro_rules! metadata {
        ($($variant:ident),*) => { match call {
            $(Vm::VmCalls::$variant(c) => definition(c),)*
        }};
    }
    macro_rules! assertion {
        ($($variant:ident),*) => { match call {
            $(Vm::VmCalls::$variant(c) => c.assertion_result(),)*
        }};
    }
    macro_rules! expectation {
        ($($variant:ident),*) => { match call {
            $(Vm::VmCalls::$variant(c) => c.apply_expectation(&mut session.expectations.revert, usize::from(context.message.depth)),)*
        }};
    }
    let definition = vm_calls!(metadata);
    let name = definition.func.signature.split('(').next().unwrap();
    let mut result = if config.blocked_cheatcodes.contains(&definition.func.selector_bytes) {
        Err(Error::display("disabled during restricted execution"))
    } else if let Some(result) = vm_calls!(assertion) {
        if !config.assertions_revert
            && let Err(error) = result
        {
            session.diagnostics.push(error.to_string());
            mark_assertion_failure(host)
        } else {
            result
        }
    } else if let Some(result) = vm_calls!(expectation) {
        result
    } else if let Some(result) = session.dispatch_prank(call, host, &context) {
        result
    } else {
        apply(call, host)?
    };
    if let Err(error) = &mut result
        && error.is_str()
        && !name.contains("assert")
    {
        *error = Error::display(format_args!("vm.{name}: {error}"));
    }
    Some(result)
}

const fn definition<C: CheatcodeDef>(_: &C) -> &'static crate::spec::Cheatcode<'static> {
    C::CHEATCODE
}

fn apply(call: &Vm::VmCalls, host: &mut Evm<'_, BaseEvmTypes>) -> Option<Result> {
    let supported = match call {
        Vm::VmCalls::warp(_)
        | Vm::VmCalls::coinbase(_)
        | Vm::VmCalls::fee(_)
        | Vm::VmCalls::prevrandao_0(_)
        | Vm::VmCalls::prevrandao_1(_)
        | Vm::VmCalls::getBlockTimestamp(_)
        | Vm::VmCalls::getBlockNumber(_)
        | Vm::VmCalls::getChainId(_)
        | Vm::VmCalls::load(_)
        | Vm::VmCalls::store(_)
        | Vm::VmCalls::getNonce_0(_)
        | Vm::VmCalls::getNonce_1(_) => true,
        Vm::VmCalls::roll(_) => host.spec_id() < SpecId::PRAGUE,
        Vm::VmCalls::etch(c) => c.target != HISTORY_STORAGE_ADDRESS,
        _ => false,
    };
    if !supported {
        return None;
    }
    let mut block = *host.block();
    Some((|| -> Result {
        match call {
            Vm::VmCalls::warp(c) => block.timestamp = c.newTimestamp,
            Vm::VmCalls::coinbase(c) => block.beneficiary = c.newCoinbase,
            Vm::VmCalls::fee(c) => {
                ensure!(c.newBasefee <= U256::from(u64::MAX), "base fee must be less than 2^64");
                block.basefee = c.newBasefee;
            }
            Vm::VmCalls::roll(c) if host.spec_id() < SpecId::PRAGUE => block.number = c.newHeight,
            Vm::VmCalls::prevrandao_0(c) => block.prevrandao = c.newPrevrandao.into(),
            Vm::VmCalls::prevrandao_1(c) => block.prevrandao = c.newPrevrandao,
            Vm::VmCalls::getBlockTimestamp(_) => return Ok(block.timestamp.abi_encode()),
            Vm::VmCalls::getBlockNumber(_) => return Ok(block.number.abi_encode()),
            Vm::VmCalls::getChainId(_) => {
                return Ok(U256::from(host.version().chain_id).abi_encode());
            }
            Vm::VmCalls::load(c) => {
                let mut account = host
                    .state_mut()
                    .account(&c.target, false)
                    .map_err(|error| fmt_err!("native state access failed: {error:?}"))?;
                account.warm();
                drop(account);
                let mut slot = host
                    .state_mut()
                    .storage_slot(&c.target, c.slot.into(), false)
                    .map_err(|error| fmt_err!("native state access failed: {error:?}"))?;
                slot.warm();
                return Ok(slot.current().abi_encode());
            }
            Vm::VmCalls::store(c) => {
                ensure_not_precompile(host, &c.target)?;
                let mut account = host
                    .state_mut()
                    .account(&c.target, false)
                    .map_err(|error| fmt_err!("native state access failed: {error:?}"))?;
                account.warm();
                account.touch();
                drop(account);
                let mut slot = host
                    .state_mut()
                    .storage_slot(&c.target, c.slot.into(), false)
                    .map_err(|error| fmt_err!("native state access failed: {error:?}"))?;
                slot.warm();
                slot.set(c.value.into());
            }
            Vm::VmCalls::etch(c) if c.target != HISTORY_STORAGE_ADDRESS => {
                ensure_not_precompile(host, &c.target)?;
                let mut account = host
                    .state_mut()
                    .account(&c.target, false)
                    .map_err(|error| fmt_err!("native state access failed: {error:?}"))?;
                account.warm();
                let code = Bytecode::new_raw_checked(c.newRuntimeBytecode.clone())
                    .map_err(|error| fmt_err!("failed to create bytecode: {error}"))?;
                account.touch();
                account.set_code_slow(code);
            }
            Vm::VmCalls::getNonce_0(c) => {
                let mut account = host
                    .state_mut()
                    .account(&c.account, false)
                    .map_err(|error| fmt_err!("native state access failed: {error:?}"))?;
                account.warm();
                return Ok(account.nonce().abi_encode());
            }
            Vm::VmCalls::getNonce_1(c) => {
                let mut account = host
                    .state_mut()
                    .account(&c.wallet.addr, false)
                    .map_err(|error| fmt_err!("native state access failed: {error:?}"))?;
                account.warm();
                return Ok(account.nonce().abi_encode());
            }
            _ => unreachable!("capability checked before dispatch"),
        }
        host.set_block(block);
        Ok(Vec::new())
    })())
}

fn ensure_not_precompile(host: &Evm<'_, BaseEvmTypes>, address: &Address) -> Result<()> {
    ensure!(
        !host.precompiles().contains(address),
        "cannot use precompile {address} as an argument"
    );
    Ok(())
}

fn mark_assertion_failure(host: &mut Evm<'_, BaseEvmTypes>) -> Result {
    let mut account = host
        .state_mut()
        .account(&CHEATCODE_ADDRESS, false)
        .map_err(|error| fmt_err!("native state access failed: {error:?}"))?;
    account.warm();
    account.touch();
    drop(account);
    let mut slot = host
        .state_mut()
        .storage_slot(&CHEATCODE_ADDRESS, GLOBAL_FAIL_SLOT, false)
        .map_err(|error| fmt_err!("native state access failed: {error:?}"))?;
    slot.warm();
    slot.set(U256::from(1));
    Ok(Vec::new())
}
