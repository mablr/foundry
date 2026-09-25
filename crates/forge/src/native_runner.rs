//! Native Ethereum execution of a linked Forge test contract.

use crate::{
    TestContract, TestFilter,
    result::{
        InvariantFailure, InvariantOutcome, SuiteResult, TestKind, TestResult, TestStatus,
        invariant_kind,
    },
    test_contract::{LibraryDeployment, PreparedTestArtifacts, analyze_compiled_sources},
    test_matcher::{
        FuzzFailureReplayConfig, TestFunctionMatcher, is_generated_symbolic_regression_contract,
    },
};
use alloy_consensus::{TxLegacy, transaction::Recovered};
use alloy_dyn_abi::{DynSolValue, FunctionExt, JsonAbiExt};
use alloy_json_abi::{Function, JsonAbi, StateMutability};
use alloy_primitives::{Address, Bytes, Log, Selector, TxKind, U256, map::HashMap};
use evm2::{
    TxResult,
    ethereum::{TxEnvelope, intrinsic_gas},
    evm::{Database, EmptyDB},
};
use eyre::{Result, ensure};
use foundry_common::{
    ContractsByArtifact, LIBRARY_DEPLOYER, TestFunctionExt, TestFunctionKind,
    fmt::{format_tokens, format_tokens_raw},
};
use foundry_compilers::ProjectCompileOutput;
use foundry_config::{Config, InlineConfig, InvariantDepthMode};
use foundry_evm::{
    core::{
        constants::{
            CALLER, DEFAULT_CREATE2_DEPLOYER, DEFAULT_CREATE2_DEPLOYER_CODE,
            DEFAULT_CREATE2_DEPLOYER_DEPLOYER, MAGIC_ASSUME,
        },
        native::{EthereumEnv, LocalState},
    },
    coverage::HitMaps,
    fuzz::{
        BaseCounterExample, CounterExample, FuzzCase, FuzzFixtures, FuzzTestResult, fixture_name,
        strategies::{EnumBounds, fuzz_calldata, fuzz_msg_value},
    },
    native::{EthereumExecutor, EthereumInspectorStack},
    opts::EvmOpts,
    traces::native::{CallTraceArena as NativeCallTraceArena, TracingInspectorConfig},
};
use itertools::Itertools;
use proptest::{
    strategy::{Strategy, ValueTree},
    test_runner::{RngAlgorithm, TestRng, TestRunner},
};
use rand::Rng;
use std::{collections::BTreeMap, sync::Arc, time::Instant};

/// Linked local tests and native Ethereum execution options.
pub(crate) struct NativeMultiContractRunner {
    pub prepared: PreparedTestArtifacts,
    config: Arc<Config>,
    inline_config: Arc<InlineConfig>,
    evm_opts: EvmOpts,
    sender: Address,
    fuzz_input: Option<FuzzFailureReplayConfig>,
    enum_bounds: EnumBounds,
    coverage: bool,
}

/// A deployed test contract with state shared by its individual test runs.
#[derive(Clone, Debug)]
pub struct NativeContractRunner<D: Database + Clone = EmptyDB> {
    executor: EthereumExecutor<D, EthereumInspectorStack<D>>,
    address: Address,
    gas_limit: u64,
    gas_price: u128,
    setup_coverage: Option<HitMaps>,
}

type NativeCallOutput = (TxResult, Vec<Log>, Vec<NativeCallTraceArena>, Option<HitMaps>);

/// The result of deploying and setting up one native test contract.
pub(crate) enum NativeContractSetup<D: Database + Clone> {
    Ready(Box<NativeContractRunner<D>>),
    Failed {
        stage: &'static str,
        result: Box<TxResult>,
        logs: Vec<Log>,
        traces: Vec<NativeCallTraceArena>,
        coverage: Option<HitMaps>,
    },
}

enum NativeInvariantFailure {
    Predicate {
        reason: String,
        sequence: Vec<BaseCounterExample>,
    },
    Handler {
        name: String,
        target: Address,
        selector: Selector,
        reason: String,
        sequence: Vec<BaseCounterExample>,
    },
}

/// Libraries linked into a native test contract.
pub(crate) struct NativeLibraries<'a> {
    code: &'a [Bytes],
    deployment: LibraryDeployment,
}

/// Inputs shared by test contract deployment and execution.
pub(crate) struct NativeTestSetup<'a> {
    sender: Address,
    initial_balance: U256,
    gas_limit: u64,
    gas_price: u128,
    libraries: NativeLibraries<'a>,
    tracing: Option<TracingInspectorConfig>,
    coverage: bool,
    isolation: bool,
    artifacts: Option<&'a ContractsByArtifact>,
}

impl<D: Database + Clone + 'static> NativeContractRunner<D> {
    /// Deploys a linked test contract and executes its optional `setUp()` function.
    pub(crate) fn prepare(
        contract: &TestContract,
        mut env: EthereumEnv,
        mut state: LocalState<D>,
        setup: NativeTestSetup<'_>,
    ) -> Result<NativeContractSetup<D>> {
        let NativeTestSetup {
            sender,
            initial_balance,
            gas_limit,
            gas_price,
            libraries,
            tracing,
            coverage,
            isolation,
            artifacts,
        } = setup;
        env.block.gas_limit = U256::from(gas_limit);
        state.set_balance(sender, U256::MAX)?;
        state.set_nonce(sender, 1)?;
        state.set_balance(CALLER, U256::MAX)?;
        state.set_balance(LIBRARY_DEPLOYER, U256::MAX)?;
        let expected_address = sender.create(1);
        state.set_balance(expected_address, initial_balance)?;
        let mut executor = EthereumExecutor::new_foundry(env, state);
        if let Some(artifacts) = artifacts {
            executor.inspector_mut().set_artifacts(artifacts.clone());
        }
        if let Some(tracing) = tracing {
            executor.inspector_mut().enable_tracing(tracing);
        }
        if coverage {
            executor.inspector_mut().enable_coverage();
        }
        if isolation {
            executor.inspector_mut().enable_isolation();
        }
        if let LibraryDeployment::Create2 { deployer, .. } = libraries.deployment
            && !libraries.code.is_empty()
        {
            ensure!(
                deployer == DEFAULT_CREATE2_DEPLOYER,
                "native custom CREATE2 deployer is not implemented"
            );
            Self::deploy_create2_factory(&mut executor, gas_limit, gas_price)?;
        }
        for (index, code) in libraries.code.iter().enumerate() {
            match libraries.deployment {
                LibraryDeployment::Nonce => {
                    let address = Self::deploy_code(
                        &mut executor,
                        LIBRARY_DEPLOYER,
                        index as u64,
                        code.clone(),
                        gas_limit,
                        gas_price,
                    )?;
                    ensure!(
                        address == LIBRARY_DEPLOYER.create(index as u64),
                        "native library deployment returned an unexpected address"
                    );
                }
                LibraryDeployment::Create2 { deployer, salt } => {
                    let expected = deployer.create2_from_code(salt, code);
                    let mut input = Vec::with_capacity(32 + code.len());
                    input.extend_from_slice(salt.as_slice());
                    input.extend_from_slice(code);
                    let tx = Self::transaction(
                        LIBRARY_DEPLOYER,
                        index as u64,
                        TxKind::Call(deployer),
                        input.into(),
                        gas_limit,
                        gas_price,
                        U256::ZERO,
                    );
                    let result = executor.transact(&tx)?;
                    ensure!(
                        result.status,
                        "native CREATE2 library deployment failed: {:?}",
                        result.stop
                    );
                    ensure!(
                        executor.state().database().account_info(&expected).is_some_and(|info| {
                            executor
                                .state()
                                .database()
                                .cache
                                .contracts
                                .get(&info.code_hash)
                                .is_some_and(|code| !code.is_empty())
                        }),
                        "native CREATE2 library deployment produced no code at {expected}"
                    );
                }
            }
        }
        let tx = Self::transaction(
            sender,
            1,
            TxKind::Create,
            contract.bytecode.clone(),
            gas_limit,
            gas_price,
            U256::ZERO,
        );
        let result = executor.transact(&tx)?;
        if !result.status {
            return Ok(NativeContractSetup::Failed {
                stage: "constructor()",
                result: Box::new(result),
                logs: executor.inspector_mut().take_logs(),
                traces: executor.inspector_mut().take_traces(),
                coverage: executor.inspector_mut().take_coverage(),
            });
        }
        let address = result
            .created_address
            .ok_or_else(|| eyre::eyre!("native deployment returned no address"))?;
        ensure!(address == expected_address, "native deployment returned an unexpected address");

        executor.state_mut().set_balance(sender, initial_balance)?;
        executor.state_mut().set_balance(CALLER, initial_balance)?;
        executor.state_mut().set_balance(LIBRARY_DEPLOYER, initial_balance)?;

        if matches!(libraries.deployment, LibraryDeployment::Nonce) {
            Self::deploy_create2_factory(&mut executor, gas_limit, gas_price)?;
        }

        let mut runner = Self { executor, address, gas_limit, gas_price, setup_coverage: None };
        if let Some(setup) = contract
            .abi
            .functions
            .get("setUp")
            .and_then(|functions| functions.iter().find(|function| function.inputs.is_empty()))
        {
            let result = runner.execute(setup.selector().into())?;
            if !result.status {
                return Ok(NativeContractSetup::Failed {
                    stage: "setUp()",
                    result: Box::new(result),
                    logs: runner.executor.inspector_mut().take_logs(),
                    traces: runner.executor.inspector_mut().take_traces(),
                    coverage: runner.executor.inspector_mut().take_coverage(),
                });
            }
        }
        runner.setup_coverage = runner.executor.inspector_mut().take_coverage();
        Ok(NativeContractSetup::Ready(Box::new(runner)))
    }

    /// Returns the deployed test contract address.
    pub const fn address(&self) -> Address {
        self.address
    }

    /// Executes one no-argument unit test against an isolated copy of setup state.
    pub fn run_unit(&self, function: &Function) -> Result<NativeCallOutput> {
        ensure!(function.inputs.is_empty(), "native unit execution requires no arguments");
        self.run_input(function.selector().into(), U256::ZERO)
    }

    /// Replays one concrete call against an isolated copy of setup state.
    pub fn run_input(&self, input: Bytes, value: U256) -> Result<NativeCallOutput> {
        let mut runner = self.clone();
        let result = runner.execute_with_value(input, value)?;
        let logs = runner.executor.inspector_mut().take_logs();
        let traces = runner.executor.inspector_mut().take_traces();
        let coverage = runner.executor.inspector_mut().take_coverage();
        Ok((result, logs, traces, coverage))
    }

    fn merge_setup_coverage(&self, coverage: Option<HitMaps>) -> Option<HitMaps> {
        let mut total = self.setup_coverage.clone();
        HitMaps::merge_opt(&mut total, coverage);
        total
    }

    /// Reads declared fuzz fixtures from the state after `setUp()`.
    pub fn fuzz_fixtures(&self, abi: &JsonAbi) -> FuzzFixtures {
        let mut fixtures = HashMap::default();
        for function in abi.functions().filter(|function| function.is_fixture()) {
            let value = if function.inputs.is_empty() {
                self.read_fixture(function, &[])
            } else {
                let values = (0..)
                    .map(|index| {
                        self.read_fixture(function, &[DynSolValue::Uint(U256::from(index), 256)])
                    })
                    .take_while(Option::is_some)
                    .flatten()
                    .collect();
                Some(DynSolValue::Array(values))
            };
            if let Some(value) = value {
                fixtures.insert(fixture_name(function.name.clone()), value);
            }
        }
        FuzzFixtures::new(fixtures)
    }

    fn read_fixture(&self, function: &Function, args: &[DynSolValue]) -> Option<DynSolValue> {
        let input = function.abi_encode_input(args).ok()?;
        let (result, _, _, _) = self.run_input(input.into(), U256::ZERO).ok()?;
        if !result.status {
            return None;
        }
        let mut values = function.abi_decode_output(&result.output).ok()?;
        Some(if values.len() == 1 { values.pop()? } else { DynSolValue::Tuple(values) })
    }

    fn read_target_list(&self, abi: &JsonAbi, name: &str) -> Result<Vec<DynSolValue>> {
        let Some(function) = abi
            .functions
            .get(name)
            .and_then(|functions| functions.iter().find(|function| function.inputs.is_empty()))
        else {
            return Ok(Vec::new());
        };
        let (result, _, _, _) = self.run_unit(function)?;
        if !result.status {
            return Ok(Vec::new());
        }
        let Some(DynSolValue::Array(values)) =
            function.abi_decode_output(&result.output)?.into_iter().next()
        else {
            return Ok(Vec::new());
        };
        Ok(values)
    }

    fn read_target_addresses(&self, abi: &JsonAbi, name: &str) -> Result<Vec<Address>> {
        self.read_target_list(abi, name)?
            .into_iter()
            .map(|value| value.as_address().ok_or_else(|| eyre::eyre!("invalid {name} address")))
            .collect()
    }

    fn read_target_selectors(
        &self,
        abi: &JsonAbi,
        name: &str,
    ) -> Result<BTreeMap<Address, Vec<Selector>>> {
        let mut targeted = BTreeMap::<Address, Vec<Selector>>::new();
        for value in self.read_target_list(abi, name)? {
            let fields = match value {
                DynSolValue::Tuple(fields) => fields,
                DynSolValue::CustomStruct { tuple, .. } => tuple,
                value => eyre::bail!("invalid {name} entry: {value:?}"),
            };
            let [DynSolValue::Address(address), DynSolValue::Array(selectors)] = fields.as_slice()
            else {
                eyre::bail!("invalid {name} entry");
            };
            for selector in selectors {
                let DynSolValue::FixedBytes(bytes, 4) = selector else {
                    eyre::bail!("invalid {name} selector");
                };
                targeted.entry(*address).or_default().push(Selector::from_slice(&bytes[..4]));
            }
        }
        Ok(targeted)
    }

    fn invariant_targets(
        &self,
        abi: &JsonAbi,
        known_contracts: &ContractsByArtifact,
    ) -> Result<Vec<(Address, String, Function)>> {
        for name in [
            "targetArtifacts",
            "excludeArtifacts",
            "targetArtifactSelectors",
            "targetInterfaces",
            "targetSenders",
            "excludeSenders",
        ] {
            ensure!(
                self.read_target_list(abi, name)?.is_empty(),
                "native invariant {name} selection is not implemented"
            );
        }
        let selected = self.read_target_addresses(abi, "targetContracts")?;
        let excluded = self.read_target_addresses(abi, "excludeContracts")?;
        let targeted_selectors = self.read_target_selectors(abi, "targetSelectors")?;
        let excluded_selectors = self.read_target_selectors(abi, "excludeSelectors")?;
        let explicit_targets = !selected.is_empty();
        let mut state = self.executor.state().clone();
        let mut addresses = if explicit_targets {
            selected
        } else {
            state.database().cache.accounts.keys().copied().collect::<Vec<_>>()
        };
        addresses.extend(targeted_selectors.keys().copied());
        addresses.extend(excluded_selectors.keys().copied());
        addresses.sort_unstable();
        addresses.dedup();
        let mut targets = Vec::new();
        for address in addresses {
            if (address == self.address
                && !explicit_targets
                && !targeted_selectors.contains_key(&address))
                || excluded.contains(&address)
            {
                continue;
            }
            let Some(info) = Database::get_account(&mut state, &address)? else {
                ensure!(
                    !targeted_selectors.contains_key(&address)
                        && !excluded_selectors.contains_key(&address),
                    "invariant selector address does not have an associated contract: {address}"
                );
                continue;
            };
            let code = Database::get_code_by_hash(&mut state, &info.code_hash)?;
            let Some((id, contract)) =
                known_contracts.find_by_deployed_code(code.original_byte_slice())
            else {
                ensure!(
                    !targeted_selectors.contains_key(&address)
                        && !excluded_selectors.contains_key(&address),
                    "invariant selector address does not have an associated contract: {address}"
                );
                continue;
            };
            for (name, selectors) in [
                ("targetSelectors", targeted_selectors.get(&address)),
                ("excludeSelectors", excluded_selectors.get(&address)),
            ] {
                if let Some(selectors) = selectors {
                    for selector in selectors {
                        ensure!(
                            contract
                                .abi
                                .functions()
                                .any(|function| function.selector() == *selector),
                            "{name}: {} does not have the selector {selector}",
                            id.name
                        );
                    }
                }
            }
            targets.extend(contract.abi.functions().filter_map(|function| {
                let selector = function.selector();
                let selected = targeted_selectors
                    .get(&address)
                    .is_some_and(|selectors| selectors.contains(&selector));
                let default = !targeted_selectors.contains_key(&address)
                    && !matches!(
                        function.state_mutability,
                        StateMutability::Pure | StateMutability::View
                    );
                let excluded = excluded_selectors
                    .get(&address)
                    .is_some_and(|selectors| selectors.contains(&selector));
                ((selected || default) && !excluded).then_some((
                    address,
                    id.name.clone(),
                    function.clone(),
                ))
            }));
        }
        ensure!(!targets.is_empty(), "No functions to fuzz.");
        Ok(targets)
    }

    fn execute(&mut self, input: Bytes) -> Result<TxResult> {
        self.execute_with_value(input, U256::ZERO)
    }

    fn execute_with_value(&mut self, input: Bytes, value: U256) -> Result<TxResult> {
        self.execute_call(CALLER, self.address, input, value)
    }

    fn execute_call(
        &mut self,
        caller: Address,
        target: Address,
        input: Bytes,
        value: U256,
    ) -> Result<TxResult> {
        let nonce =
            self.executor.state().database().account_info(&caller).map_or(0, |info| info.nonce);
        let tx = Self::transaction(
            caller,
            nonce,
            TxKind::Call(target),
            input,
            self.gas_limit,
            self.gas_price,
            value,
        );
        Ok(self.executor.transact(&tx)?)
    }

    fn deploy_code(
        executor: &mut EthereumExecutor<D, EthereumInspectorStack<D>>,
        caller: Address,
        nonce: u64,
        code: Bytes,
        gas_limit: u64,
        gas_price: u128,
    ) -> Result<Address> {
        let tx = Self::transaction(
            caller,
            nonce,
            TxKind::Create,
            code,
            gas_limit,
            gas_price,
            U256::ZERO,
        );
        let result = executor.transact(&tx)?;
        ensure!(
            result.status,
            "native contract deployment by {caller} at nonce {nonce} failed: {:?}",
            result.stop
        );
        result.created_address.ok_or_else(|| eyre::eyre!("native deployment returned no address"))
    }

    fn deploy_create2_factory(
        executor: &mut EthereumExecutor<D, EthereumInspectorStack<D>>,
        gas_limit: u64,
        gas_price: u128,
    ) -> Result<()> {
        if let Some(info) =
            Database::get_account(&mut executor.state_mut(), &DEFAULT_CREATE2_DEPLOYER)?
            && !Database::get_code_by_hash(&mut executor.state_mut(), &info.code_hash)?.is_empty()
        {
            return Ok(());
        }
        let creator = DEFAULT_CREATE2_DEPLOYER_DEPLOYER;
        let balance = Database::get_account(&mut executor.state_mut(), &creator)?
            .map_or(U256::ZERO, |info| info.balance);
        executor.state_mut().set_balance(creator, U256::MAX)?;
        let nonce = executor.state().database().account_info(&creator).map_or(0, |info| info.nonce);
        let address = Self::deploy_code(
            executor,
            creator,
            nonce,
            DEFAULT_CREATE2_DEPLOYER_CODE.into(),
            gas_limit,
            gas_price,
        )?;
        ensure!(address == DEFAULT_CREATE2_DEPLOYER, "native CREATE2 factory address mismatch");
        executor.state_mut().set_balance(creator, balance)?;
        Ok(())
    }

    fn transaction(
        caller: Address,
        nonce: u64,
        to: TxKind,
        input: Bytes,
        gas_limit: u64,
        gas_price: u128,
        value: U256,
    ) -> Recovered<TxEnvelope> {
        Recovered::new_unchecked(
            TxEnvelope::Legacy(TxLegacy {
                nonce,
                gas_limit,
                gas_price,
                value,
                to,
                input,
                ..Default::default()
            }),
            caller,
        )
    }
}

impl NativeMultiContractRunner {
    /// Prepares linked test artifacts without constructing a REVM executor.
    pub fn new(
        config: Arc<Config>,
        inline_config: Arc<InlineConfig>,
        output: &ProjectCompileOutput,
        evm_opts: EvmOpts,
        sender: Address,
        create2_deployer_available: bool,
        fuzz_input: Option<FuzzFailureReplayConfig>,
    ) -> Result<Self> {
        let prepared = PreparedTestArtifacts::new(
            &config,
            &inline_config,
            None,
            output,
            &evm_opts,
            create2_deployer_available,
        )?;
        let enum_bounds = EnumBounds::collect(&analyze_compiled_sources(&config, output)?);
        Ok(Self {
            prepared,
            config,
            inline_config,
            evm_opts,
            sender,
            fuzz_input,
            enum_bounds,
            coverage: false,
        })
    }

    /// Enables native coverage collection for a coverage command.
    pub const fn with_coverage(mut self, coverage: bool) -> Self {
        self.coverage = coverage;
        self
    }

    /// Executes selected local unit tests through evm2 and collects their results.
    pub fn test_collect(&self, filter: &dyn TestFilter) -> Result<BTreeMap<String, SuiteResult>> {
        let env = EthereumEnv::local_from_config(&self.config, &self.evm_opts)?;
        self.test_collect_with_state(filter, env, LocalState::default())
    }

    /// Executes selected unit tests over fresh snapshots of one native state.
    pub fn test_collect_with_state<D: Database + Clone + 'static>(
        &self,
        filter: &dyn TestFilter,
        env: EthereumEnv,
        state: LocalState<D>,
    ) -> Result<BTreeMap<String, SuiteResult>> {
        let gas_price = u128::try_from(env.block.basefee)?;
        let matcher = TestFunctionMatcher::new(&self.config, &self.inline_config, None);
        let mut suites = BTreeMap::new();
        for (id, contract) in &self.prepared.contracts {
            if !matcher.matches_contract(filter, id, &contract.abi) {
                continue;
            }
            let timer = Instant::now();
            let runner = NativeContractRunner::prepare(
                contract,
                env,
                state.clone(),
                NativeTestSetup {
                    sender: self.sender,
                    initial_balance: self.evm_opts.initial_balance,
                    gas_limit: self.evm_opts.gas_limit(),
                    gas_price,
                    libraries: NativeLibraries {
                        code: &self.prepared.libs_to_deploy,
                        deployment: self.prepared.library_deployment,
                    },
                    tracing: (self.config.tracing.verbosity >= 3).then_some(
                        TracingInspectorConfig {
                            record_steps: self.config.tracing.verbosity >= 5,
                            record_bytecode: true,
                            record_logs: true,
                            ..Default::default()
                        },
                    ),
                    coverage: self.coverage,
                    isolation: self.config.isolate,
                    artifacts: Some(&self.prepared.known_contracts),
                },
            )?;
            let runner = match runner {
                NativeContractSetup::Ready(runner) => runner,
                NativeContractSetup::Failed { stage, result, logs, traces, coverage } => {
                    let mut failure = TestResult::fail(self.failure_reason(&result));
                    failure.logs = logs;
                    failure.native_traces = traces;
                    failure.line_coverage = coverage;
                    suites.insert(
                        id.identifier(),
                        SuiteResult::new(
                            timer.elapsed(),
                            [(stage.to_string(), failure)].into(),
                            Vec::new(),
                        ),
                    );
                    continue;
                }
            };
            let mut tests = BTreeMap::new();
            let mut fixtures = None;
            for function in matcher.matching_test_functions(filter, id, &contract.abi) {
                let kind = matcher.test_function_kind(
                    &id.identifier(),
                    function,
                    is_generated_symbolic_regression_contract(&contract.abi),
                );
                if let Some(replay) = &self.fuzz_input {
                    let test = if replay.contract == id.identifier()
                        && replay.test == function.signature()
                    {
                        ensure!(
                            matches!(kind, TestFunctionKind::FuzzTest { should_fail: false }),
                            "native fuzz replay requires a stateless fuzz test"
                        );
                        self.run_fuzz_replay(&runner, function, replay, &env)?
                    } else {
                        let mut test = TestResult {
                            status: TestStatus::Skipped,
                            reason: Some("not runnable in replay mode".to_string()),
                            ..Default::default()
                        };
                        if matches!(kind, TestFunctionKind::FuzzTest { .. }) {
                            test.kind = TestKind::Fuzz {
                                first_case: FuzzCase::default(),
                                runs: 0,
                                mean_gas: 0,
                                median_gas: 0,
                                failed_corpus_replays: 0,
                            };
                        }
                        test
                    };
                    tests.insert(function.signature(), test);
                    continue;
                }
                if matches!(kind, TestFunctionKind::FuzzTest { should_fail: false }) {
                    let fixtures = fixtures.get_or_insert_with(|| {
                        runner
                            .fuzz_fixtures(&contract.abi)
                            .with_enum_bounds(self.enum_bounds.clone())
                    });
                    tests.insert(
                        function.signature(),
                        self.run_fuzz_campaign(&runner, function, fixtures, &env)?,
                    );
                    continue;
                }
                if matches!(kind, TestFunctionKind::TableTest) {
                    let fixtures =
                        fixtures.get_or_insert_with(|| runner.fuzz_fixtures(&contract.abi));
                    tests.insert(
                        function.signature(),
                        self.run_table_test(&runner, function, fixtures, &env)?,
                    );
                    continue;
                }
                if matches!(kind, TestFunctionKind::InvariantTest) {
                    let fixtures = fixtures.get_or_insert_with(|| {
                        runner
                            .fuzz_fixtures(&contract.abi)
                            .with_enum_bounds(self.enum_bounds.clone())
                    });
                    let test = match self.run_invariant_campaign(
                        &runner,
                        function,
                        &contract.abi,
                        fixtures,
                    ) {
                        Ok(test) => test,
                        Err(error) => {
                            let mut test = TestResult::default();
                            test.invariant_setup_fail(error);
                            test
                        }
                    };
                    tests.insert(function.signature(), test);
                    continue;
                }
                ensure!(
                    matches!(kind, TestFunctionKind::UnitTest { should_fail: false }),
                    "native execution does not yet support {} tests",
                    kind.name()
                );
                let (result, logs, traces, coverage) = runner.run_unit(function)?;
                let passed = result.status;
                let reason = (!passed).then(|| self.failure_reason(&result));
                let input = Bytes::copy_from_slice(function.selector().as_slice());
                let stipend = intrinsic_gas(
                    &env.version,
                    CALLER,
                    TxKind::Call(runner.address()),
                    &input,
                    0,
                    0,
                    U256::ZERO,
                );
                tests.insert(
                    function.signature(),
                    TestResult {
                        status: if passed { TestStatus::Success } else { TestStatus::Failure },
                        reason,
                        kind: TestKind::Unit { gas: result.tx_gas_used().saturating_sub(stipend) },
                        logs,
                        native_traces: traces,
                        line_coverage: runner.merge_setup_coverage(coverage),
                        ..Default::default()
                    },
                );
            }
            suites.insert(id.identifier(), SuiteResult::new(timer.elapsed(), tests, Vec::new()));
        }
        Ok(suites)
    }

    fn run_fuzz_replay<D: Database + Clone + 'static>(
        &self,
        runner: &NativeContractRunner<D>,
        function: &Function,
        replay: &FuzzFailureReplayConfig,
        env: &EthereumEnv,
    ) -> Result<TestResult> {
        ensure!(self.config.fuzz.fail_on_revert, "native fuzz replay requires fail_on_revert");
        let balance = runner
            .executor
            .state()
            .database()
            .account_info(&CALLER)
            .map_or(U256::ZERO, |info| info.balance);
        let value = replay.failure.value.unwrap_or_default().min(balance);
        let (result, logs, traces, coverage) =
            runner.run_input(replay.failure.calldata.clone(), value)?;
        if result.output.as_ref() == MAGIC_ASSUME {
            let mut test = TestResult::default();
            test.fuzz_result(FuzzTestResult {
                skipped: true,
                reason: Some("persisted fuzz failure rejected by `vm.assume`".to_string()),
                ..Default::default()
            });
            test.native_traces = traces;
            test.line_coverage = runner.merge_setup_coverage(coverage);
            return Ok(test);
        }
        let stipend = intrinsic_gas(
            &env.version,
            CALLER,
            TxKind::Call(runner.address()),
            &replay.failure.calldata,
            0,
            0,
            value,
        );
        let gas_used = result.tx_gas_used();
        let passed = result.status;
        let counterexample = (!passed).then(|| {
            let args = function.abi_decode_input(&replay.failure.calldata[4..]).unwrap_or_default();
            let mut counterexample = (*replay.failure).clone();
            counterexample.sender = Some(CALLER);
            counterexample.addr = Some(runner.address());
            counterexample.value = (!value.is_zero()).then_some(value);
            counterexample.warp = None;
            counterexample.roll = None;
            counterexample.args = Some(format_tokens(&args).format(", ").to_string());
            counterexample.raw_args = Some(format_tokens_raw(&args).format(", ").to_string());
            CounterExample::Single(counterexample)
        });
        let mut test = TestResult::default();
        test.fuzz_result(FuzzTestResult {
            first_case: if passed {
                FuzzCase { gas: gas_used, stipend }
            } else {
                FuzzCase::default()
            },
            gas_by_case: passed.then_some((gas_used, stipend)).into_iter().collect(),
            success: passed,
            reason: (!passed).then(|| self.failure_reason(&result)),
            counterexample,
            logs,
            ..Default::default()
        });
        test.native_traces = traces;
        test.line_coverage = runner.merge_setup_coverage(coverage);
        Ok(test)
    }

    fn run_fuzz_campaign<D: Database + Clone + 'static>(
        &self,
        runner: &NativeContractRunner<D>,
        function: &Function,
        fixtures: &FuzzFixtures,
        env: &EthereumEnv,
    ) -> Result<TestResult> {
        let config = &self.config.fuzz;
        ensure!(config.fail_on_revert, "native fuzzing requires fail_on_revert");
        ensure!(
            config.corpus.corpus_dir.is_none(),
            "native corpus-guided fuzzing is not implemented"
        );
        ensure!(config.run.is_none(), "native fuzz run selection is not implemented");
        let test_runner_config = proptest::test_runner::Config {
            cases: config.runs,
            max_global_rejects: config.max_test_rejects,
            max_shrink_iters: 0,
            failure_persistence: None,
            ..Default::default()
        };
        let mut generator = if let Some(seed) = config.seed {
            let rng = TestRng::from_seed(RngAlgorithm::ChaCha, &seed.to_be_bytes::<32>());
            TestRunner::new_with_rng(test_runner_config, rng)
        } else {
            TestRunner::new(test_runner_config)
        };
        let strategy = (
            fuzz_calldata(function.clone(), fixtures),
            fuzz_msg_value(if matches!(function.state_mutability, StateMutability::Payable) {
                config.corpus.payable_value_weight
            } else {
                0
            }),
        );
        let balance = runner
            .executor
            .state()
            .database()
            .account_info(&CALLER)
            .map_or(U256::ZERO, |info| info.balance);
        let mut campaign = FuzzTestResult { success: true, ..Default::default() };
        let mut native_traces = Vec::new();
        let mut coverage = runner.setup_coverage.clone();
        let mut rejects = 0;
        let started = Instant::now();
        while campaign.gas_by_case.len() < config.runs as usize {
            if config.timeout.is_some_and(|seconds| started.elapsed().as_secs() >= seconds as u64) {
                break;
            }
            let (input, requested_value) = strategy
                .new_tree(&mut generator)
                .map_err(|reason| eyre::eyre!("failed to generate fuzz input: {reason}"))?
                .current();
            let value = requested_value.unwrap_or_default().min(balance);
            let (result, logs, traces, hits) = runner.run_input(input.clone(), value)?;
            HitMaps::merge_opt(&mut coverage, hits);
            if result.output.as_ref() == MAGIC_ASSUME {
                rejects += 1;
                if rejects > config.max_test_rejects {
                    campaign.success = false;
                    campaign.reason = Some("maximum fuzz test rejections exceeded".to_string());
                    break;
                }
                continue;
            }
            if !result.status {
                let args = function.abi_decode_input(&input[4..]).unwrap_or_default();
                let mut counterexample = BaseCounterExample::from_fuzz_call(input, args, None);
                counterexample.sender = Some(CALLER);
                counterexample.addr = Some(runner.address());
                counterexample.value = (!value.is_zero()).then_some(value);
                campaign.success = false;
                campaign.reason = Some(self.failure_reason(&result));
                campaign.counterexample = Some(CounterExample::Single(counterexample));
                campaign.logs = logs;
                native_traces = traces;
                break;
            }
            let stipend = intrinsic_gas(
                &env.version,
                CALLER,
                TxKind::Call(runner.address()),
                &input,
                0,
                0,
                value,
            );
            let case = FuzzCase { gas: result.tx_gas_used(), stipend };
            if campaign.gas_by_case.is_empty() {
                campaign.first_case = case.clone();
                native_traces = traces;
            }
            campaign.gas_by_case.push((case.gas, case.stipend));
            if config.show_logs {
                campaign.logs = logs;
            }
        }
        let mut test = TestResult::default();
        test.fuzz_result(campaign);
        test.native_traces = native_traces;
        test.line_coverage = coverage;
        Ok(test)
    }

    fn run_table_test<D: Database + Clone + 'static>(
        &self,
        runner: &NativeContractRunner<D>,
        function: &Function,
        fixtures: &FuzzFixtures,
        env: &EthereumEnv,
    ) -> Result<TestResult> {
        let Some(first) = function.inputs.first() else {
            return Ok(TestResult::fail("Table test should have at least one parameter".into()));
        };
        let Some(first_fixtures) = fixtures.param_fixtures(first.name()) else {
            return Ok(TestResult::fail("Table test should have fixtures defined".into()));
        };
        if first_fixtures.is_empty() {
            return Ok(TestResult::fail("Table test should have at least one fixture".into()));
        }
        let mut rows = vec![first_fixtures];
        for param in &function.inputs[1..] {
            let Some(values) = fixtures.param_fixtures(param.name()) else {
                return Ok(TestResult::fail(format!(
                    "No fixture defined for param {}",
                    param.name()
                )));
            };
            if values.len() != first_fixtures.len() {
                return Ok(TestResult::fail(format!(
                    "{} fixtures defined for {} (expected {})",
                    values.len(),
                    param.name(),
                    first_fixtures.len()
                )));
            }
            rows.push(values);
        }

        let mut campaign = FuzzTestResult { success: true, ..Default::default() };
        let mut native_traces = Vec::new();
        let mut coverage = runner.setup_coverage.clone();
        for index in 0..first_fixtures.len() {
            let args = rows.iter().map(|values| values[index].clone()).collect_vec();
            let input = Bytes::from(function.abi_encode_input(&args)?);
            let (result, logs, traces, hits) = runner.run_input(input.clone(), U256::ZERO)?;
            HitMaps::merge_opt(&mut coverage, hits);
            let stipend = intrinsic_gas(
                &env.version,
                CALLER,
                TxKind::Call(runner.address()),
                &input,
                0,
                0,
                U256::ZERO,
            );
            campaign.gas_by_case.push((result.tx_gas_used(), stipend));
            campaign.logs.extend(logs);
            native_traces = traces;
            if !result.status {
                let mut counterexample = BaseCounterExample::from_fuzz_call(input, args, None);
                counterexample.sender = Some(CALLER);
                counterexample.addr = Some(runner.address());
                campaign.counterexample = Some(CounterExample::Single(counterexample));
                campaign.reason = Some(self.failure_reason(&result));
                campaign.success = false;
                break;
            }
        }
        let mut test = TestResult::default();
        test.table_result(campaign);
        test.native_traces = native_traces;
        test.line_coverage = coverage;
        Ok(test)
    }

    fn run_invariant_campaign<D: Database + Clone + 'static>(
        &self,
        runner: &NativeContractRunner<D>,
        invariant: &Function,
        abi: &JsonAbi,
        fixtures: &FuzzFixtures,
    ) -> Result<TestResult> {
        ensure!(invariant.inputs.is_empty(), "invariant functions must take no parameters");
        let targets = runner.invariant_targets(abi, &self.prepared.known_contracts)?;
        let after_invariant = abi
            .functions
            .get("afterInvariant")
            .and_then(|functions| functions.iter().find(|function| function.inputs.is_empty()));
        let config = &self.config.invariant;
        ensure!(!config.call_override, "native invariant call override is not implemented");
        ensure!(!config.has_delay(), "native invariant transaction delays are not implemented");
        let test_runner_config = proptest::test_runner::Config {
            cases: config.runs,
            failure_persistence: None,
            ..Default::default()
        };
        let mut generator = if let Some(seed) = self.config.fuzz.seed {
            let rng = TestRng::from_seed(RngAlgorithm::ChaCha, &seed.to_be_bytes::<32>());
            TestRunner::new_with_rng(test_runner_config, rng)
        } else {
            TestRunner::new(test_runner_config)
        };
        let target_strategy = proptest::sample::select(targets);
        let mut runs = 0;
        let mut calls = 0;
        let mut reverts = 0;
        let mut failure = None;
        let mut native_traces = Vec::new();
        let mut coverage = runner.setup_coverage.clone();
        let started = Instant::now();
        'campaign: for _ in 0..config.runs {
            if config.timeout.is_some_and(|seconds| started.elapsed().as_secs() >= seconds as u64) {
                break;
            }
            runs += 1;
            let mut run = runner.clone();
            run.executor.inspector_mut().take_logs();
            run.executor.inspector_mut().take_traces();
            let mut sequence = Vec::new();
            let depth = match config.depth_mode {
                InvariantDepthMode::Fixed => config.depth,
                InvariantDepthMode::Random => {
                    let minimum = config.min_depth.max(1);
                    if config.depth <= minimum {
                        config.depth.max(1)
                    } else {
                        generator.rng().random_range(minimum..=config.depth)
                    }
                }
            };
            for index in 0..depth {
                let (target, contract, function) = target_strategy
                    .new_tree(&mut generator)
                    .map_err(|reason| eyre::eyre!("failed to select invariant target: {reason}"))?
                    .current();
                let input = fuzz_calldata(function.clone(), fixtures)
                    .new_tree(&mut generator)
                    .map_err(|reason| eyre::eyre!("failed to generate invariant call: {reason}"))?
                    .current();
                let result = run.execute_call(CALLER, target, input.clone(), U256::ZERO)?;
                calls += 1;
                let args = function.abi_decode_input(&input[4..]).unwrap_or_default();
                let mut call = BaseCounterExample::from_fuzz_call(input, args, None);
                call.sender = Some(CALLER);
                call.addr = Some(target);
                sequence.push(call);
                run.executor.inspector_mut().take_logs();
                run.executor.inspector_mut().take_traces();
                HitMaps::merge_opt(&mut coverage, run.executor.inspector_mut().take_coverage());
                if !result.status {
                    reverts += 1;
                    if config.fail_on_revert || Self::is_assert_panic(&result.output) {
                        failure = Some(NativeInvariantFailure::Handler {
                            name: format!("{contract}::{}", function.name),
                            target,
                            selector: function.selector(),
                            reason: self.failure_reason(&result),
                            sequence,
                        });
                        break 'campaign;
                    }
                }
                let check = index + 1 == depth
                    || config.check_interval > 0 && (index + 1) % config.check_interval == 0;
                if check {
                    let (result, _, traces, hits) = run.run_unit(invariant)?;
                    HitMaps::merge_opt(&mut coverage, hits);
                    native_traces = traces;
                    if !result.status {
                        failure = Some(NativeInvariantFailure::Predicate {
                            reason: self.failure_reason(&result),
                            sequence,
                        });
                        break 'campaign;
                    }
                }
            }
            if let Some(after_invariant) = after_invariant {
                let (result, _, traces, hits) = run.run_unit(after_invariant)?;
                HitMaps::merge_opt(&mut coverage, hits);
                native_traces = traces;
                if !result.status {
                    failure = Some(NativeInvariantFailure::Predicate {
                        reason: self.failure_reason(&result),
                        sequence,
                    });
                    break 'campaign;
                }
            }
        }
        let success = failure.is_none();
        let mut failures = Vec::new();
        let mut handler_failures = Vec::new();
        match failure {
            Some(NativeInvariantFailure::Predicate { reason, sequence }) => {
                failures.push(InvariantFailure::Predicate {
                    name: invariant.name.clone(),
                    reason,
                    counterexample: Some(CounterExample::Sequence(sequence.len(), sequence)),
                    artifact: None,
                    minimization: None,
                    persisted_path: Default::default(),
                    is_anchor: true,
                });
            }
            Some(NativeInvariantFailure::Handler { name, target, selector, reason, sequence }) => {
                handler_failures.push(InvariantFailure::Handler {
                    name,
                    reverter: target,
                    selector,
                    reason,
                    counterexample: Some(CounterExample::Sequence(sequence.len(), sequence)),
                    artifact: None,
                });
            }
            None => {}
        }
        let mut test = TestResult::default();
        test.invariant_result(
            invariant_kind(runs as usize, calls as usize, reverts as usize),
            InvariantOutcome { success, failures, handler_failures, ..Default::default() },
        );
        test.native_traces = native_traces;
        test.line_coverage = coverage;
        Ok(test)
    }

    fn is_assert_panic(output: &[u8]) -> bool {
        output.len() == 36
            && output[..4] == [0x4e, 0x48, 0x7b, 0x71]
            && output[4..35].iter().all(|byte| *byte == 0)
            && output[35] == 1
    }

    fn failure_reason(&self, result: &TxResult) -> String {
        if result.output.is_empty() {
            format!("EvmError: {:?}", result.stop)
        } else {
            self.prepared.revert_decoder.decode(&result.output, None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use evm2::{SpecId, bytecode::Bytecode, env::BlockEnvExt, evm::AccountInfo};
    use std::collections::BTreeSet;

    #[test]
    fn executes_linked_unit_test_on_native_state() {
        let abi = serde_json::from_str::<JsonAbi>(
            r#"[{"type":"function","name":"testValue","inputs":[],"outputs":[{"type":"uint256"}],"stateMutability":"nonpayable"}]"#,
        )
        .unwrap();
        let contract = TestContract {
            abi,
            bytecode: Bytes::from_static(&[
                0x60, 0x0a, 0x60, 0x0c, 0x60, 0x00, 0x39, 0x60, 0x0a, 0x60, 0x00, 0xf3, 0x60, 0x2a,
                0x60, 0x00, 0x52, 0x60, 0x20, 0x60, 0x00, 0xf3,
            ]),
            library_addresses: BTreeSet::new(),
        };
        let env = EthereumEnv::new(
            SpecId::CANCUN,
            BlockEnvExt { gas_limit: U256::from(30_000_000), ..Default::default() },
        );
        let sender = Address::with_last_byte(0xa);
        let runner = NativeContractRunner::prepare(
            &contract,
            env,
            LocalState::default(),
            NativeTestSetup {
                sender,
                initial_balance: U256::ZERO,
                gas_limit: 100_000,
                gas_price: 0,
                libraries: NativeLibraries { code: &[], deployment: LibraryDeployment::Nonce },
                tracing: None,
                coverage: false,
                isolation: false,
                artifacts: None,
            },
        )
        .unwrap();
        let NativeContractSetup::Ready(runner) = runner else {
            panic!("native test contract deployment failed");
        };

        assert_eq!(runner.address(), sender.create(1));
        let (result, _, _, _) = runner.run_unit(&contract.abi.functions["testValue"][0]).unwrap();
        assert!(result.status);
        assert_eq!(U256::from_be_slice(&result.output), U256::from(42));
    }

    #[test]
    fn stateful_calls_share_one_run_and_reset_for_the_next() {
        let target = Address::with_last_byte(0x42);
        let mut state = LocalState::default();
        state.database_mut().insert_account_info(
            &target,
            AccountInfo::default().with_code(Bytecode::new_legacy(Bytes::from_static(&[
                0x60, 0x00, 0x54, 0x60, 0x01, 0x01, 0x60, 0x00, 0x55, 0x60, 0x00, 0x54, 0x60, 0x00,
                0x52, 0x60, 0x20, 0x60, 0x00, 0xf3,
            ]))),
        );
        let contract = TestContract {
            abi: JsonAbi::default(),
            bytecode: Bytes::from_static(&[0x60, 0x00, 0x60, 0x00, 0xf3]),
            library_addresses: BTreeSet::new(),
        };
        let env = EthereumEnv::new(
            SpecId::CANCUN,
            BlockEnvExt { gas_limit: U256::from(30_000_000), ..Default::default() },
        );
        let runner = NativeContractRunner::prepare(
            &contract,
            env,
            state,
            NativeTestSetup {
                sender: Address::with_last_byte(0xa),
                initial_balance: U256::ZERO,
                gas_limit: 100_000,
                gas_price: 0,
                libraries: NativeLibraries { code: &[], deployment: LibraryDeployment::Nonce },
                tracing: None,
                coverage: false,
                isolation: false,
                artifacts: None,
            },
        )
        .unwrap();
        let NativeContractSetup::Ready(runner) = runner else {
            panic!("native test contract deployment failed");
        };

        let mut run = runner.clone();
        for expected in [1, 2] {
            let result = run.execute_call(CALLER, target, Bytes::new(), U256::ZERO).unwrap();
            assert!(result.status);
            assert_eq!(U256::from_be_slice(&result.output), U256::from(expected));
        }
        let mut next_run = runner;
        let result = next_run.execute_call(CALLER, target, Bytes::new(), U256::ZERO).unwrap();
        assert!(result.status);
        assert_eq!(U256::from_be_slice(&result.output), U256::ONE);
    }
}
