//! Ethereum script execution on evm2.

use crate::{
    ScriptArgs, ScriptInputs,
    build::{BuildData, LinkedBuildData},
    execute::ExecutionData,
    resolve_script_fork, resolve_script_sender_nonce,
};
use alloy_consensus::{TxLegacy, transaction::Recovered};
use alloy_network::Ethereum;
use alloy_primitives::{Address, Bytes, Log, TxKind};
use evm2::{TxResult, ethereum::TxEnvelope, evm::Database};
use eyre::Result;
use foundry_cheatcodes::{BroadcastableTransactions, Wallets};
use foundry_cli::opts::TempoOpts;
use foundry_config::Config;
use foundry_evm::{
    core::fork::ResolvedFork,
    native::{EthereumExecutor, EthereumInspectorStack},
    opts::EvmOpts,
    traces::native::CallTraceArena,
};
use foundry_evm_networks::NetworkVariant;
use foundry_wallets::wallet_browser::signer::BrowserSigner;

/// Ethereum script inputs prepared without a legacy executor or fork backend.
pub struct NativeScriptContext {
    pub args: ScriptArgs,
    pub config: Config,
    pub evm_opts: EvmOpts,
    pub script_wallets: Wallets,
    pub browser_wallet: Option<BrowserSigner<Ethereum>>,
    pub tempo: TempoOpts,
    pub sender_nonce: u64,
    pub resolved_fork: Option<ResolvedFork>,
    pub plan: NativeScriptPlan,
}

impl NativeScriptContext {
    /// Prepares Ethereum script inputs, the exact fork, and linked artifacts.
    pub async fn prepare(args: ScriptArgs, config: Config, evm_opts: EvmOpts) -> Result<Self> {
        let ScriptInputs { args, mut config, mut evm_opts, script_wallets, browser_wallet, tempo } =
            args.preprocess_inputs::<Ethereum>(config, evm_opts).await?;
        eyre::ensure!(
            evm_opts.networks.execution_network() == NetworkVariant::Ethereum,
            "native script requires Ethereum"
        );
        let resolved_fork = resolve_script_fork(&mut config, &mut evm_opts, None).await?;
        let sender_nonce =
            resolve_script_sender_nonce(args.sender_nonce, &evm_opts, resolved_fork.as_ref())
                .await?;
        let plan = NativeScriptPlan::prepare(
            &args,
            &config,
            &evm_opts,
            sender_nonce,
            resolved_fork.as_ref(),
        )
        .await?;
        Ok(Self {
            args,
            config,
            evm_opts,
            script_wallets,
            browser_wallet,
            tempo,
            sender_nonce,
            resolved_fork,
            plan,
        })
    }
}

/// Compiled, linked, and ABI-encoded inputs for native Ethereum script execution.
pub struct NativeScriptPlan {
    pub build: LinkedBuildData,
    pub execution: ExecutionData,
}

impl NativeScriptPlan {
    /// Prepares script artifacts without constructing a legacy EVM runner.
    pub async fn prepare(
        args: &ScriptArgs,
        config: &Config,
        evm_opts: &EvmOpts,
        sender_nonce: u64,
        resolved_fork: Option<&ResolvedFork>,
    ) -> Result<Self> {
        let build = BuildData::compile_target(args, config)?
            .link(config, evm_opts, sender_nonce, resolved_fork)
            .await?;
        let execution = ExecutionData::prepare(args, &build)?;
        Ok(Self { build, execution })
    }
}

/// Observations from one native script execution stage.
pub struct NativeScriptRun {
    /// The evm2 transaction result.
    pub result: TxResult,
    /// Logs emitted during this stage.
    pub logs: Vec<Log>,
    /// Completed call traces for this stage.
    pub traces: Vec<CallTraceArena>,
    /// Transactions collected from broadcast cheatcodes.
    pub transactions: BroadcastableTransactions<Ethereum>,
}

/// Executes local Ethereum script stages over an evm2 state.
pub struct NativeScriptRunner<D: Database + Clone> {
    executor: EthereumExecutor<D, EthereumInspectorStack<D>>,
    deployer: Address,
    sender: Address,
    gas_limit: u64,
    gas_price: u128,
}

impl<D: Database + Clone + 'static> NativeScriptRunner<D> {
    /// Creates a runner with separate script deployer and script-call sender.
    pub const fn new(
        executor: EthereumExecutor<D, EthereumInspectorStack<D>>,
        deployer: Address,
        sender: Address,
        gas_limit: u64,
        gas_price: u128,
    ) -> Self {
        Self { executor, deployer, sender, gas_limit, gas_price }
    }

    /// Returns the native executor and its accepted setup state.
    pub const fn executor(&self) -> &EthereumExecutor<D, EthereumInspectorStack<D>> {
        &self.executor
    }

    /// Returns the native executor for setup and library installation.
    pub const fn executor_mut(&mut self) -> &mut EthereumExecutor<D, EthereumInspectorStack<D>> {
        &mut self.executor
    }

    /// Deploys the local script contract and accepts its constructor state.
    pub fn deploy(&mut self, code: Bytes) -> Result<NativeScriptRun> {
        let tx = self.transaction(self.deployer, TxKind::Create, code)?;
        let result = self.executor.transact(&tx)?;
        Ok(Self::collect(result, self.executor.inspector_mut()))
    }

    /// Executes `setUp()` and accepts its state for the subsequent script call.
    pub fn setup(&mut self, address: Address, input: Bytes) -> Result<NativeScriptRun> {
        let tx = self.transaction(self.sender, TxKind::Call(address), input)?;
        let result = self.executor.transact(&tx)?;
        Ok(Self::collect(result, self.executor.inspector_mut()))
    }

    /// Executes the script while retaining its observations and discarding its state changes.
    pub fn script(&self, address: Address, input: Bytes) -> Result<NativeScriptRun> {
        let tx = self.transaction(self.sender, TxKind::Call(address), input)?;
        let (result, mut inspector) = self.executor.inspect(&tx)?;
        Ok(Self::collect(result, &mut inspector))
    }

    fn transaction(
        &self,
        sender: Address,
        to: TxKind,
        input: Bytes,
    ) -> Result<Recovered<TxEnvelope>> {
        let mut state = self.executor.state().clone();
        let nonce = Database::get_account(&mut state, &sender)?.map_or(0, |account| account.nonce);
        Ok(Recovered::new_unchecked(
            TxEnvelope::Legacy(TxLegacy {
                nonce,
                gas_limit: self.gas_limit,
                gas_price: self.gas_price,
                to,
                input,
                ..Default::default()
            }),
            sender,
        ))
    }

    fn collect(result: TxResult, inspector: &mut EthereumInspectorStack<D>) -> NativeScriptRun {
        NativeScriptRun {
            result,
            logs: inspector.take_logs(),
            traces: inspector.take_traces(),
            transactions: inspector.take_broadcast_transactions(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::U256;
    use alloy_sol_types::SolCall;
    use evm2::{SpecId, env::BlockEnvExt};
    use foundry_cheatcodes::Vm;
    use foundry_evm::core::{constants::CHEATCODE_ADDRESS, native::LocalState};

    #[tokio::test]
    async fn prepared_solidity_script_runs_on_native_executor() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("src/Counter.s.sol");
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::write(
            &source,
            "pragma solidity ^0.8.20; contract CounterScript { uint256 public value; function setUp() public { value = 3; } function run() public returns (uint256) { value += 2; return value; } }",
        )
        .unwrap();
        let mut config = Config::with_root(root.path());
        config.cache_path = root.path().join("cache");
        let args = ScriptArgs {
            path: source.to_string_lossy().into_owned(),
            sig: "run()".into(),
            ..Default::default()
        };
        let mut opts = EvmOpts::default();
        opts.env.gas_limit = foundry_config::GasLimit(30_000_000);
        opts.memory_limit = 1_000_000;
        let context = NativeScriptContext::prepare(args, config, opts).await.unwrap();
        assert_eq!(context.sender_nonce, 1);
        let plan = context.plan;
        assert_eq!(plan.build.predeploy_libraries.libraries_count(), 0);

        let deployer = Address::with_last_byte(1);
        let sender = Address::with_last_byte(2);
        let mut state = LocalState::default();
        state.set_balance(deployer, U256::MAX).unwrap();
        state.set_balance(sender, U256::MAX).unwrap();
        let env = foundry_evm::core::native::EthereumEnv::local_from_config(
            &context.config,
            &context.evm_opts,
        )
        .unwrap();
        let executor = EthereumExecutor::new_foundry(env, state);
        let mut runner = NativeScriptRunner::new(executor, deployer, sender, 1_000_000, 0);
        let deployment = runner.deploy(plan.execution.bytecode.clone()).unwrap();
        assert!(deployment.result.status, "{:#?}", deployment.result);
        let address = deployment.result.created_address.unwrap();

        let setup = runner
            .setup(address, Bytes::copy_from_slice(&alloy_primitives::keccak256("setUp()")[..4]))
            .unwrap();
        assert!(setup.result.status);
        let script = runner.script(address, plan.execution.calldata).unwrap();
        assert!(script.result.status);
        assert_eq!(U256::from_be_slice(&script.result.output), U256::from(5));

        let read = runner
            .script(address, Bytes::copy_from_slice(&alloy_primitives::keccak256("value()")[..4]))
            .unwrap();
        assert_eq!(U256::from_be_slice(&read.result.output), U256::from(3));
    }

    #[test]
    fn script_keeps_setup_state_and_discards_its_own_writes() {
        let sender = Address::with_last_byte(1);
        let mut state = LocalState::default();
        state.set_balance(sender, U256::MAX).unwrap();
        let env = foundry_evm::core::native::EthereumEnv::new(
            SpecId::CANCUN,
            BlockEnvExt { gas_limit: U256::from(30_000_000), ..Default::default() },
        );
        let executor = EthereumExecutor::new_foundry(env, state);
        let mut runner = NativeScriptRunner::new(executor, sender, sender, 100_000, 0);
        runner.executor_mut().inspector_mut().enable_tracing(Default::default());
        let runtime = Bytes::from_static(&[
            0x5f, 0x54, 0x60, 0x01, 0x01, 0x80, 0x5f, 0x55, 0x5f, 0x52, 0x60, 0x20, 0x5f, 0xf3,
        ]);
        let mut code = vec![
            0x60,
            runtime.len() as u8,
            0x60,
            0x0c,
            0x60,
            0x00,
            0x39,
            0x60,
            runtime.len() as u8,
            0x60,
            0x00,
            0xf3,
        ];
        code.extend_from_slice(&runtime);
        let deployment = runner.deploy(code.into()).unwrap();
        assert!(deployment.result.status);
        let address = deployment.result.created_address.unwrap();
        assert_eq!(address, sender.create(0));

        let setup = runner.setup(address, Bytes::new()).unwrap();
        assert!(setup.result.status);
        assert_eq!(U256::from_be_slice(&setup.result.output), U256::ONE);

        let script = runner.script(address, Bytes::new()).unwrap();
        assert!(script.result.status);
        assert_eq!(script.traces.len(), 1);
        assert_eq!(U256::from_be_slice(&script.result.output), U256::from(2));
        assert_eq!(
            Database::get_storage(&mut runner.executor().state().clone(), &address, &U256::ZERO)
                .unwrap(),
            U256::ONE
        );

        let broadcaster = Address::with_last_byte(0x30);
        let start = runner
            .setup(
                CHEATCODE_ADDRESS,
                Vm::startBroadcast_1Call { signer: broadcaster }.abi_encode().into(),
            )
            .unwrap();
        assert!(start.result.status);
        let broadcast = runner.script(address, Bytes::new()).unwrap();
        assert!(broadcast.result.status);
        let transaction = &broadcast.transactions.front().unwrap().transaction;
        assert_eq!(broadcast.transactions.len(), 1);
        assert_eq!(transaction.from(), Some(broadcaster));
        assert_eq!(transaction.to(), Some(address));
        assert_eq!(transaction.nonce(), Some(0));
        assert_eq!(
            Database::get_storage(&mut runner.executor().state().clone(), &address, &U256::ZERO)
                .unwrap(),
            U256::ONE
        );
    }
}
