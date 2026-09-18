//! Cheatcodes whose execution-state contract has been migrated to native evm2.
//!
//! Unsupported calls are distinct from Solidity reverts: the executor must reject
//! the test even if Solidity catches the call. No REVM context or journal is built.

use crate::{
    Cheatcode, CheatcodeDef, CheatsConfig, Error, Result, Vm,
    evm::{
        mock::{MockCallDataContext, MockCallReturnData},
        prank::Prank,
    },
    test::{
        expect::{
            CreateScheme, ExpectedCallTracker, ExpectedCreate, ExpectedEmitTracker, ExpectedRevert,
            ExpectedRevertKind,
        },
        revert_handlers,
    },
};
use alloy_primitives::{Address, Bytes, Log, U256, map::HashMap};
use alloy_sol_types::SolValue;
use evm2::{
    BaseEvmTypes, Evm, SpecId,
    bytecode::Bytecode,
    interpreter::{
        GasTracker, InstrStop, Message, MessageKind, MessageResult, MessageResultExt,
        derive_create_destination,
    },
};
use foundry_evm_core::{
    backend::GLOBAL_FAIL_SLOT,
    constants::{CHEATCODE_ADDRESS, HARDHAT_CONSOLE_ADDRESS},
    eip2935::HISTORY_STORAGE_ADDRESS,
};
use foundry_evm_traces::identifier::SignaturesIdentifier;
use revm::interpreter::{CallScheme, InstructionResult};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::OnceLock,
};

/// Engine-independent state retained between native executions.
#[derive(Clone, Debug, Default)]
pub struct Session {
    /// Revert expectations.
    pub expectations: Expectations,
    /// Pending event expectations, including partially filled templates.
    pub emits: ExpectedEmitTracker,
    /// Lazily initialized event decoder.
    pub signatures: OnceLock<Option<SignaturesIdentifier>>,
    /// Pending runtime-bytecode creation expectations.
    pub creates: Vec<ExpectedCreate>,
    /// Calls observed independently of state rollback.
    pub calls: ExpectedCallTracker,
    /// Depth-scoped caller overrides.
    pub pranks: BTreeMap<usize, Prank>,
    /// Mock responses ordered by calldata specificity and value.
    pub mocks: HashMap<Address, BTreeMap<MockCallDataContext, VecDeque<MockCallReturnData>>>,
    /// Replacement code addresses keyed by exact calldata or a four-byte selector.
    pub mocked_functions: HashMap<Address, HashMap<Bytes, Address>>,
    /// Diagnostic log recording, independent of frame rollback.
    pub recorded_logs: Option<Vec<Vm::Log>>,
    /// Deprecated calls observed across executions.
    pub deprecated: HashMap<&'static str, Option<&'static str>>,
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
    /// Observes a log independently of journal rollback.
    pub fn observe_emit(&mut self, log: &Log) -> Option<&'static str> {
        crate::test::expect::observe_emit(&mut self.emits, log)
    }

    /// Verifies event matching at ordinary call completion.
    pub fn verify_emits(
        &mut self,
        message: &Message,
        success: bool,
        config: &CheatsConfig,
    ) -> std::result::Result<(), Bytes> {
        let clear = crate::test::expect::verify_emits(
            &self.emits,
            usize::from(message.depth),
            message.caller_is_static || message.kind == MessageKind::StaticCall,
            success,
            || {
                self.signatures
                    .get_or_init(|| {
                        if let Some(artifacts) = &config.available_artifacts {
                            return SignaturesIdentifier::new_offline_with_abis(
                                artifacts.values().map(|contract| &contract.abi),
                            )
                            .ok();
                        }
                        SignaturesIdentifier::new(true).ok()
                    })
                    .as_ref()
            },
        )?;
        if clear {
            self.emits.clear();
        }
        Ok(())
    }

    /// Checks leftover root event expectations after call expectations.
    pub fn verify_root_emits(&mut self, success: bool) -> Result<()> {
        crate::test::expect::verify_root_emits(&mut self.emits, success)
    }

    /// Verifies creation expectations after call expectations at root completion.
    pub fn verify_creates(&self) -> Result<()> {
        crate::test::expect::verify_creates(&self.creates)
    }

    /// Matches the account code published by native create settlement.
    pub fn observe_create(
        &mut self,
        host: &mut Evm<'_, BaseEvmTypes>,
        message: &Message,
        result: &MessageResult,
        entered: bool,
    ) -> Result<()> {
        if self.creates.is_empty() {
            return Ok(());
        }
        let address = result.created_address.or_else(|| entered.then_some(message.destination));
        let Some(address) = address else {
            // Pre-frame rejection has no creation to match, unlike constructor failure.
            return Ok(());
        };
        let mut account = host
            .state_mut()
            .account(&address, false)
            .map_err(|error| fmt_err!("native created account access failed: {error:?}"))?;
        let code = account
            .load_code()
            .map_err(|error| fmt_err!("native created code access failed: {error:?}"))?;
        crate::test::expect::match_create(
            &mut self.creates,
            message.caller,
            if message.kind == MessageKind::Create2 {
                CreateScheme::Create2
            } else {
                CreateScheme::Create
            },
            code.original_byte_slice(),
        );
        Ok(())
    }

    /// Records EVM logs, including logs from subsequently reverted frames.
    pub fn record_log(&mut self, log: &Log) {
        if let Some(logs) = &mut self.recorded_logs {
            logs.push(Vm::Log {
                topics: log.data.topics().to_vec(),
                data: log.data.data.clone(),
                emitter: log.address,
            });
        }
    }

    /// Redirects code before call expectations, pranks and return-data mocks observe the call.
    pub fn redirect_mock_function(
        &self,
        host: &mut Evm<'_, BaseEvmTypes>,
        message: &mut Message,
    ) -> Result<()> {
        if message.kind.is_create() {
            return Ok(());
        }
        let Some(mocks) = self.mocked_functions.get(&message.code_address) else {
            return Ok(());
        };
        let Some(target) = mocks
            .get(&message.input)
            .or_else(|| message.input.get(..4).and_then(|selector| mocks.get(selector)))
        else {
            return Ok(());
        };
        let mut account = host
            .state_mut()
            .account(target, false)
            .map_err(|error| fmt_err!("native mock function account access failed: {error:?}"))?;
        account.warm();
        let code = account
            .load_code()
            .map_err(|error| fmt_err!("native mock function code access failed: {error:?}"))?;
        message.code_address = *target;
        message.code = code;
        Ok(())
    }

    /// Intercepts a registered mock after expectations and prank application.
    pub fn mock_call(
        &mut self,
        host: &mut Evm<'_, BaseEvmTypes>,
        message: &Message,
    ) -> Result<Option<MessageResult>> {
        let Some(mocks) = self.mocks.get_mut(&message.code_address) else {
            return Ok(None);
        };
        let value = (message.kind != MessageKind::DelegateCall).then_some(message.value);
        let key = MockCallDataContext { calldata: message.input.clone(), value };
        let queue = if mocks.contains_key(&key) {
            mocks.get_mut(&key)
        } else {
            mocks
                .iter_mut()
                .find(|(key, _)| {
                    message.input.starts_with(&key.calldata)
                        && key.value.is_none_or(|v| Some(v) == value)
                })
                .map(|(_, queue)| queue)
        };
        let Some(queue) = queue else {
            return Ok(None);
        };
        let Some(response) = queue.front().cloned() else {
            return Ok(None);
        };
        let gas =
            GasTracker::new_with_execution_gas_and_reservoir(message.gas_limit, message.reservoir);
        if let Some(value) = value {
            if message.caller != message.destination && !value.is_zero() {
                let balance = host
                    .state_mut()
                    .account(&message.destination, false)
                    .map_err(|error| fmt_err!("native mock balance access failed: {error:?}"))?
                    .balance();
                ensure!(
                    balance.checked_add(value).is_some(),
                    "evm2 mocked recipient balance overflow is not migrated"
                );
            }
            let checkpoint = host.state().checkpoint();
            let features = host.version().features;
            let transferred = host
                .state_mut()
                .transfer(&message.caller, &message.destination, &value)
                .map_err(|error| fmt_err!("native mock transfer failed: {error:?}"))?;
            if !transferred || !response.ret_type.is_ok() {
                host.state_mut().rollback(checkpoint, features);
            }
            if !transferred {
                return Ok(Some(MessageResultExt {
                    stop: InstrStop::OutOfFunds,
                    gas,
                    ..Default::default()
                }));
            }
        }
        if queue.len() > 1 {
            queue.pop_front();
        }
        Ok(Some(MessageResultExt {
            stop: if response.ret_type.is_ok() { InstrStop::Return } else { InstrStop::Revert },
            output: response.data,
            gas,
            ..Default::default()
        }))
    }

    fn dispatch_mock(
        &mut self,
        call: &Vm::VmCalls,
        host: &mut Evm<'_, BaseEvmTypes>,
    ) -> Option<Result> {
        let (callee, data, value, output, revert, inject) = match call {
            Vm::VmCalls::mockFunction(c) => {
                self.mocked_functions.entry(c.callee).or_default().insert(c.data.clone(), c.target);
                return Some(Ok(Vec::new()));
            }
            Vm::VmCalls::expectCreate(c) => {
                return Some(crate::test::expect::expect_create(
                    &mut self.creates,
                    c.bytecode.clone(),
                    c.deployer,
                    CreateScheme::Create,
                ));
            }
            Vm::VmCalls::expectCreate2(c) => {
                return Some(crate::test::expect::expect_create(
                    &mut self.creates,
                    c.bytecode.clone(),
                    c.deployer,
                    CreateScheme::Create2,
                ));
            }
            Vm::VmCalls::mockCall_0(c) => {
                (c.callee, c.data.clone(), None, vec![c.returnData.clone()], false, true)
            }
            Vm::VmCalls::mockCall_1(c) => (
                c.callee,
                c.data.clone(),
                Some(c.msgValue),
                vec![c.returnData.clone()],
                false,
                true,
            ),
            Vm::VmCalls::mockCall_2(c) => (
                c.callee,
                Bytes::copy_from_slice(c.data.as_slice()),
                None,
                vec![c.returnData.clone()],
                false,
                true,
            ),
            Vm::VmCalls::mockCall_3(c) => (
                c.callee,
                Bytes::copy_from_slice(c.data.as_slice()),
                Some(c.msgValue),
                vec![c.returnData.clone()],
                false,
                true,
            ),
            Vm::VmCalls::mockCall_4(c) => {
                (c.callee, c.data.clone(), None, vec![c.returnData.clone()], false, c.injectCode)
            }
            Vm::VmCalls::mockCalls_0(c) => {
                (c.callee, c.data.clone(), None, c.returnData.clone(), false, true)
            }
            Vm::VmCalls::mockCalls_1(c) => {
                (c.callee, c.data.clone(), Some(c.msgValue), c.returnData.clone(), false, true)
            }
            Vm::VmCalls::mockCallRevert_0(c) => {
                (c.callee, c.data.clone(), None, vec![c.revertData.clone()], true, true)
            }
            Vm::VmCalls::mockCallRevert_1(c) => {
                (c.callee, c.data.clone(), Some(c.msgValue), vec![c.revertData.clone()], true, true)
            }
            Vm::VmCalls::mockCallRevert_2(c) => (
                c.callee,
                Bytes::copy_from_slice(c.data.as_slice()),
                None,
                vec![c.revertData.clone()],
                true,
                true,
            ),
            Vm::VmCalls::mockCallRevert_3(c) => (
                c.callee,
                Bytes::copy_from_slice(c.data.as_slice()),
                Some(c.msgValue),
                vec![c.revertData.clone()],
                true,
                true,
            ),
            Vm::VmCalls::clearMockedCalls(_) => {
                self.mocks.clear();
                return Some(Ok(Vec::new()));
            }
            Vm::VmCalls::recordLogs(_) => {
                self.recorded_logs = Some(Vec::new());
                return Some(Ok(Vec::new()));
            }
            Vm::VmCalls::getRecordedLogs(_) => {
                return Some(Ok(self
                    .recorded_logs
                    .replace(Vec::new())
                    .unwrap_or_default()
                    .abi_encode()));
            }
            _ => return None,
        };
        Some((|| -> Result {
            if inject {
                let mut account = host
                    .state_mut()
                    .account(&callee, false)
                    .map_err(|error| fmt_err!("native state access failed: {error:?}"))?;
                account.warm();
                if account
                    .load_code()
                    .map_err(|error| fmt_err!("native state access failed: {error:?}"))?
                    .is_empty()
                {
                    account.set_code_slow(Bytecode::new_legacy(Bytes::from_static(&[0])));
                }
            }
            self.mocks.entry(callee).or_default().insert(
                MockCallDataContext { calldata: data, value },
                output
                    .into_iter()
                    .map(|data| MockCallReturnData {
                        ret_type: if revert {
                            InstructionResult::Revert
                        } else {
                            InstructionResult::Return
                        },
                        data,
                    })
                    .collect(),
            );
            Ok(Vec::new())
        })())
    }

    /// Observes non-cheatcode calls before prank application and mock interception.
    pub fn observe_call(&mut self, message: &Message) {
        let Some(calls) = self.calls.get_mut(&message.code_address) else {
            return;
        };
        let scheme = match message.kind {
            MessageKind::Call => CallScheme::Call,
            MessageKind::StaticCall => CallScheme::StaticCall,
            MessageKind::CallCode => CallScheme::CallCode,
            MessageKind::DelegateCall => CallScheme::DelegateCall,
            _ => return,
        };
        let value = (message.kind != MessageKind::DelegateCall).then_some(message.value);
        for ((data, expected_scheme), (expected, actual)) in calls {
            if message.input.starts_with(data)
                && expected.value.is_none_or(|v| Some(v) == value)
                && expected.gas.is_none_or(|v| v == message.gas_limit)
                && expected.min_gas.is_none_or(|v| v <= message.gas_limit)
                && expected_scheme.is_none_or(|v| v == scheme)
            {
                *actual += 1;
            }
        }
    }

    /// Verifies registered calls when the root frame terminates.
    pub fn verify_calls(&self, success: bool) -> Result<()> {
        crate::test::expect::verify_calls(&self.calls, success)
    }

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
        creation_address: Option<Address>,
    ) -> Option<Result<(Option<Address>, Bytes)>> {
        let expected = self.revert.as_mut()?;
        let cheatcode = message.code_address == CHEATCODE_ADDRESS
            || message.code_address == HARDHAT_CONSOLE_ADDRESS;
        let failed = !status.is_ok();
        if message.kind.is_create() {
            if status.is_revert()
                && expected.reverter.is_some()
                && expected.reverted_by.is_none()
                && let Some(address) = creation_address
            {
                expected.reverted_by = Some(address);
            }
        } else if failed
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
                !cheatcode
                    && (message.kind.is_create()
                        || failed
                        || !config.internal_expect_revert
                        || message.depth == 0)
            }
            ExpectedRevertKind::Cheatcode { pending_processing } => {
                let process = cheatcode && !*pending_processing;
                if !message.kind.is_create() {
                    *pending_processing = false;
                }
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
                if message.kind.is_create() {
                    expected.reverted_by = None;
                }
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
    macro_rules! stateless {
        ($($variant:ident),*) => { match call {
            $(Vm::VmCalls::$variant(c) => c.apply_stateless(),)*
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
    macro_rules! emits {
        ($($variant:ident),*) => { match call {
            $(Vm::VmCalls::$variant(c) => c.apply_emit_expectation(&mut session.emits, usize::from(context.message.depth)),)*
        }};
    }
    macro_rules! calls {
        ($($variant:ident),*) => { match call {
            $(Vm::VmCalls::$variant(c) => c.apply_call_expectation(&mut session.calls),)*
        }};
    }
    let definition = vm_calls!(metadata);
    if let crate::spec::Status::Deprecated(replacement) = definition.status {
        session.deprecated.insert(definition.func.signature, replacement);
    }
    let name = definition.func.signature.split('(').next().unwrap();
    let mut result = if config.blocked_cheatcodes.contains(&definition.func.selector_bytes) {
        Err(Error::display("disabled during restricted execution"))
    } else if let Some(result) = vm_calls!(stateless) {
        result
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
    } else if let Some(result) = vm_calls!(emits) {
        result
    } else if let Some(result) = vm_calls!(calls) {
        result
    } else if let Some(result) = session.dispatch_mock(call, host) {
        result
    } else if let Some(result) = session.dispatch_prank(call, host, &context) {
        result
    } else {
        apply(call, host)?
    };
    if let Err(error) = &mut result
        && error.is_str()
        && !name.contains("assert")
        && name != "rpcUrl"
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

#[cfg(test)]
mod tests {
    use super::*;
    use evm2::{Precompiles, ethereum::ethereum_tx_registry, evm::InMemoryDB};

    #[test]
    fn overflowing_mock_transfer_is_rejected_before_mutation() {
        let mut host = Evm::<BaseEvmTypes>::new(
            SpecId::CANCUN,
            Default::default(),
            ethereum_tx_registry(SpecId::CANCUN),
            InMemoryDB::default(),
            Precompiles::base(SpecId::CANCUN),
        );
        let caller = Address::with_last_byte(0x40);
        let destination = Address::with_last_byte(0x41);
        host.state_mut().account(&caller, false).unwrap().set_balance(U256::from(1));
        host.state_mut().account(&destination, false).unwrap().set_balance(U256::MAX);
        let mut session = Session::default();
        session.mocks.entry(destination).or_default().insert(
            MockCallDataContext::default(),
            VecDeque::from([MockCallReturnData {
                ret_type: InstructionResult::Return,
                data: Bytes::new(),
            }]),
        );
        let message = Message::<BaseEvmTypes> {
            caller,
            destination,
            code_address: destination,
            value: U256::from(1),
            gas_limit: 1000,
            ..Default::default()
        };
        let error = session.mock_call(&mut host, &message).unwrap_err();
        assert_eq!(error.to_string(), "evm2 mocked recipient balance overflow is not migrated");
        assert_eq!(host.state_mut().account(&caller, false).unwrap().balance(), U256::from(1));
        assert_eq!(host.state_mut().account(&destination, false).unwrap().balance(), U256::MAX);
        assert_eq!(session.mocks[&destination].values().next().unwrap().len(), 1);
    }
}
