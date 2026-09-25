//! Native Ethereum execution of a linked Forge test contract.

use crate::{
    TestContract, TestFilter,
    result::{SuiteResult, TestKind, TestResult, TestStatus},
    test_contract::{LibraryDeployment, PreparedTestArtifacts},
    test_matcher::{TestFunctionMatcher, is_generated_symbolic_regression_contract},
};
use alloy_consensus::{TxLegacy, transaction::Recovered};
use alloy_json_abi::Function;
use alloy_primitives::{Address, Bytes, Log, TxKind, U256};
use evm2::{
    TxResult,
    ethereum::{TxEnvelope, intrinsic_gas},
    evm::{Database, EmptyDB},
};
use eyre::{Result, ensure};
use foundry_common::{LIBRARY_DEPLOYER, TestFunctionKind};
use foundry_compilers::ProjectCompileOutput;
use foundry_config::{Config, InlineConfig};
use foundry_evm::{
    core::{
        constants::{
            CALLER, DEFAULT_CREATE2_DEPLOYER, DEFAULT_CREATE2_DEPLOYER_CODE,
            DEFAULT_CREATE2_DEPLOYER_DEPLOYER,
        },
        native::{EthereumEnv, LocalState},
    },
    native::{EthereumExecutor, EthereumInspectorStack},
    opts::EvmOpts,
};
use std::{collections::BTreeMap, sync::Arc, time::Instant};

/// Linked local tests and native Ethereum execution options.
pub(crate) struct NativeMultiContractRunner {
    pub prepared: PreparedTestArtifacts,
    config: Arc<Config>,
    inline_config: Arc<InlineConfig>,
    evm_opts: EvmOpts,
    sender: Address,
}

/// A deployed test contract with state shared by its individual test runs.
#[derive(Clone, Debug)]
pub struct NativeContractRunner<D: Database + Clone = EmptyDB> {
    executor: EthereumExecutor<D, EthereumInspectorStack<D>>,
    address: Address,
    gas_limit: u64,
    gas_price: u128,
}

/// The result of deploying and setting up one native test contract.
pub(crate) enum NativeContractSetup<D: Database + Clone> {
    Ready(Box<NativeContractRunner<D>>),
    Failed { stage: &'static str, result: TxResult, logs: Vec<Log> },
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
}

impl<D: Database + Clone + 'static> NativeContractRunner<D> {
    /// Deploys a linked test contract and executes its optional `setUp()` function.
    pub(crate) fn prepare(
        contract: &TestContract,
        mut env: EthereumEnv,
        mut state: LocalState<D>,
        setup: NativeTestSetup<'_>,
    ) -> Result<NativeContractSetup<D>> {
        let NativeTestSetup { sender, initial_balance, gas_limit, gas_price, libraries } = setup;
        env.block.gas_limit = U256::from(gas_limit);
        state.set_balance(sender, U256::MAX)?;
        state.set_nonce(sender, 1)?;
        state.set_balance(CALLER, U256::MAX)?;
        state.set_balance(LIBRARY_DEPLOYER, U256::MAX)?;
        let expected_address = sender.create(1);
        state.set_balance(expected_address, initial_balance)?;
        let mut executor = EthereumExecutor::new_foundry(env, state);
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
        );
        let result = executor.transact(&tx)?;
        if !result.status {
            return Ok(NativeContractSetup::Failed {
                stage: "constructor()",
                result,
                logs: executor.inspector_mut().take_logs(),
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

        let mut runner = Self { executor, address, gas_limit, gas_price };
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
                    result,
                    logs: runner.executor.inspector_mut().take_logs(),
                });
            }
        }
        Ok(NativeContractSetup::Ready(Box::new(runner)))
    }

    /// Returns the deployed test contract address.
    pub const fn address(&self) -> Address {
        self.address
    }

    /// Executes one no-argument unit test against an isolated copy of setup state.
    pub fn run_unit(&self, function: &Function) -> Result<(TxResult, Vec<Log>)> {
        ensure!(function.inputs.is_empty(), "native unit execution requires no arguments");
        let mut runner = self.clone();
        let result = runner.execute(function.selector().into())?;
        Ok((result, runner.executor.inspector_mut().take_logs()))
    }

    fn execute(&mut self, input: Bytes) -> Result<TxResult> {
        let nonce =
            self.executor.state().database().account_info(&CALLER).map_or(0, |info| info.nonce);
        let tx = Self::transaction(
            CALLER,
            nonce,
            TxKind::Call(self.address),
            input,
            self.gas_limit,
            self.gas_price,
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
        let tx = Self::transaction(caller, nonce, TxKind::Create, code, gas_limit, gas_price);
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
    ) -> Recovered<TxEnvelope> {
        Recovered::new_unchecked(
            TxEnvelope::Legacy(TxLegacy {
                nonce,
                gas_limit,
                gas_price,
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
    ) -> Result<Self> {
        let prepared = PreparedTestArtifacts::new(
            &config,
            &inline_config,
            None,
            output,
            &evm_opts,
            create2_deployer_available,
        )?;
        Ok(Self { prepared, config, inline_config, evm_opts, sender })
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
                },
            )?;
            let runner = match runner {
                NativeContractSetup::Ready(runner) => runner,
                NativeContractSetup::Failed { stage, result, logs } => {
                    let mut failure = TestResult::fail(self.failure_reason(&result));
                    failure.logs = logs;
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
            for function in matcher.matching_test_functions(filter, id, &contract.abi) {
                let kind = matcher.test_function_kind(
                    &id.identifier(),
                    function,
                    is_generated_symbolic_regression_contract(&contract.abi),
                );
                ensure!(
                    matches!(kind, TestFunctionKind::UnitTest { should_fail: false }),
                    "native execution does not yet support {} tests",
                    kind.name()
                );
                let (result, logs) = runner.run_unit(function)?;
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
                        ..Default::default()
                    },
                );
            }
            suites.insert(id.identifier(), SuiteResult::new(timer.elapsed(), tests, Vec::new()));
        }
        Ok(suites)
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
    use alloy_json_abi::JsonAbi;
    use evm2::{SpecId, env::BlockEnvExt};
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
            },
        )
        .unwrap();
        let NativeContractSetup::Ready(runner) = runner else {
            panic!("native test contract deployment failed");
        };

        assert_eq!(runner.address(), sender.create(1));
        let (result, _) = runner.run_unit(&contract.abi.functions["testValue"][0]).unwrap();
        assert!(result.status);
        assert_eq!(U256::from_be_slice(&result.output), U256::from(42));
    }
}
