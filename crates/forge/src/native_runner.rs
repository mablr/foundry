//! Native Ethereum execution of a linked Forge test contract.

use crate::{
    TestContract, TestFilter,
    multi_runner::{TestFunctionMatcher, is_generated_symbolic_regression_contract},
    result::{SuiteResult, TestKind, TestResult, TestStatus},
    test_contract::PreparedTestArtifacts,
};
use alloy_consensus::{TxLegacy, transaction::Recovered};
use alloy_json_abi::Function;
use alloy_primitives::{Address, Bytes, TxKind, U256};
use evm2::{
    TxResult,
    ethereum::{TxEnvelope, intrinsic_gas},
};
use eyre::{Result, ensure};
use foundry_cheatcodes::native::NativeCheatcodes;
use foundry_common::TestFunctionKind;
use foundry_compilers::ProjectCompileOutput;
use foundry_config::{Config, InlineConfig};
use foundry_evm::{
    core::{
        constants::CALLER,
        native::{EthereumEnv, LocalState},
    },
    native::EthereumExecutor,
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
pub struct NativeContractRunner {
    executor: EthereumExecutor,
    inspector: NativeCheatcodes,
    address: Address,
    gas_limit: u64,
    gas_price: u128,
}

impl NativeContractRunner {
    /// Deploys a linked test contract and executes its optional `setUp()` function.
    pub fn new(
        contract: &TestContract,
        env: EthereumEnv,
        sender: Address,
        initial_balance: U256,
        gas_limit: u64,
        gas_price: u128,
    ) -> Result<Self> {
        ensure!(
            contract.library_addresses.is_empty(),
            "native library deployment is not implemented"
        );
        let mut state = LocalState::default();
        let mut inspector = NativeCheatcodes;
        inspector.install(&mut state);
        state.set_balance(sender, U256::MAX);
        state.set_nonce(sender, 1);
        state.set_balance(CALLER, U256::MAX);
        let expected_address = sender.create(1);
        state.set_balance(expected_address, initial_balance);
        let mut executor = EthereumExecutor::new(env, state);
        let deploy = Self::transaction(
            sender,
            1,
            TxKind::Create,
            contract.bytecode.clone(),
            gas_limit,
            gas_price,
        );
        let result = executor.inspect_transact(&deploy, &mut inspector)?;
        ensure!(result.status, "native test contract deployment failed: {:?}", result.stop);
        let address = result
            .created_address
            .ok_or_else(|| eyre::eyre!("native deployment returned no address"))?;
        ensure!(address == expected_address, "native deployment returned an unexpected address");

        let mut runner = Self { executor, inspector, address, gas_limit, gas_price };
        if let Some(setup) = contract
            .abi
            .functions
            .get("setUp")
            .and_then(|functions| functions.iter().find(|function| function.inputs.is_empty()))
        {
            let result = runner.execute(setup.selector().into())?;
            ensure!(
                result.status,
                "native setUp() failed: {:?}, output: {}, error: {:?}",
                result.stop,
                result.output,
                result.error_code
            );
        }
        Ok(runner)
    }

    /// Returns the deployed test contract address.
    pub const fn address(&self) -> Address {
        self.address
    }

    /// Executes one no-argument unit test against an isolated copy of setup state.
    pub fn run_unit(&self, function: &Function) -> Result<TxResult> {
        ensure!(function.inputs.is_empty(), "native unit execution requires no arguments");
        let mut runner = self.clone();
        runner.execute(function.selector().into())
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
        Ok(self.executor.inspect_transact(&tx, &mut self.inspector)?)
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
        let gas_price = u128::try_from(env.block.basefee)?;
        let matcher = TestFunctionMatcher::new(&self.config, &self.inline_config, None);
        let mut suites = BTreeMap::new();
        for (id, contract) in &self.prepared.contracts {
            if !matcher.matches_contract(filter, id, &contract.abi) {
                continue;
            }
            let timer = Instant::now();
            let runner = NativeContractRunner::new(
                contract,
                env,
                self.sender,
                self.evm_opts.initial_balance,
                self.evm_opts.gas_limit(),
                gas_price,
            )?;
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
                let result = runner.run_unit(function)?;
                let passed = result.status;
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
                        reason: (!passed)
                            .then(|| format!("EVM execution stopped: {:?}", result.stop)),
                        kind: TestKind::Unit { gas: result.tx_gas_used().saturating_sub(stipend) },
                        logs: result.logs,
                        ..Default::default()
                    },
                );
            }
            suites.insert(id.identifier(), SuiteResult::new(timer.elapsed(), tests, Vec::new()));
        }
        Ok(suites)
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
        let runner =
            NativeContractRunner::new(&contract, env, sender, U256::ZERO, 100_000, 0).unwrap();

        assert_eq!(runner.address(), sender.create(1));
        let result = runner.run_unit(&contract.abi.functions["testValue"][0]).unwrap();
        assert!(result.status);
        assert_eq!(U256::from_be_slice(&result.output), U256::from(42));
    }
}
