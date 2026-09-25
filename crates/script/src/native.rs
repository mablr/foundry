//! Ethereum script execution on evm2.

use crate::{
    NestedValue, ScriptArgs, ScriptInputs,
    build::{BuildData, LinkedBuildData, ScriptPredeployLibraries},
    execute::ExecutionData,
    resolve_script_fork, resolve_script_sender_nonce,
};
use alloy_consensus::{TxLegacy, transaction::Recovered};
use alloy_dyn_abi::FunctionExt;
use alloy_json_abi::InternalType;
use alloy_network::{Ethereum, NetworkTransactionBuilder, TransactionBuilder};
use alloy_primitives::{
    Address, Bytes, KECCAK256_EMPTY, Log, Signature, TxKind, U256, keccak256, map::HashMap,
};
use alloy_rpc_types::TransactionRequest;
use evm2::{EvmFeatures, TxResult, ethereum::TxEnvelope, evm::Database};
use eyre::Result;
use foundry_cheatcodes::{
    BroadcastableTransaction, BroadcastableTransactions, CheatsConfig, Wallets,
};
use foundry_cli::{opts::TempoOpts, utils::needs_setup};
use foundry_common::{
    LIBRARY_DEPLOYER, TransactionMaybeSigned,
    fmt::{format_token, format_token_raw},
    shell,
};
use foundry_config::Config;
use foundry_evm::{
    core::{
        constants::{
            CALLER, DEFAULT_CREATE2_DEPLOYER, DEFAULT_CREATE2_DEPLOYER_CODE,
            DEFAULT_CREATE2_DEPLOYER_DEPLOYER,
        },
        fork::ResolvedFork,
        native::{EthereumEnv, EthereumFork, LocalState},
    },
    decode::{RevertDecoder, decode_console_logs},
    native::{EthereumExecutor, EthereumInspectorStack},
    opts::EvmOpts,
    traces::{
        TraceKind,
        native::{
            CallTraceArena, NativeTraceDecoder, TraceWriter, TracingInspectorConfig,
            trace_arena_at_depth,
        },
    },
};
use foundry_evm_networks::NetworkVariant;
use foundry_wallets::wallet_browser::signer::BrowserSigner;
use serde::Serialize;
use yansi::Paint;

/// Executes the Ethereum `forge script` command through the native pipeline.
pub(crate) async fn run(args: ScriptArgs, config: Config, evm_opts: EvmOpts) -> Result<()> {
    eyre::ensure!(!args.resume, "native script resume is not implemented");
    eyre::ensure!(!args.broadcast, "native script broadcast is not implemented");
    eyre::ensure!(!args.debug, "native script debugger is not implemented");
    let context = NativeScriptContext::prepare(args, config, evm_opts).await?;
    let execution = context.execute().await?;
    if shell::is_json() {
        context.show_json(&execution)?;
    } else {
        context.show_output(&execution)?;
    }
    if execution.has_transactions()
        && context.evm_opts.fork_url.is_some()
        && !context.args.skip_simulation
    {
        context.simulate_transactions(&execution).await?;
        if !shell::is_json() {
            sh_println!("\nSIMULATION COMPLETE.")?;
        }
    }
    Ok(())
}

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

    /// Runs the prepared Ethereum script on the selected local state or exact RPC fork.
    pub async fn execute(&self) -> Result<NativeScriptExecution> {
        if let Some(resolved) = &self.resolved_fork {
            let fork = EthereumFork::open(&self.config, &self.evm_opts, resolved).await?;
            self.execute_on(fork.env, fork.state)
        } else {
            let env = EthereumEnv::local_from_config(&self.config, &self.evm_opts)?;
            self.execute_on(env, LocalState::default())
        }
    }

    /// Replays broadcastable transactions against a fresh copy of the resolved fork.
    async fn simulate_transactions(&self, execution: &NativeScriptExecution) -> Result<()> {
        let resolved =
            self.resolved_fork.as_ref().ok_or_else(|| eyre::eyre!("fork not resolved"))?;
        let fork = EthereumFork::open(&self.config, &self.evm_opts, resolved).await?;
        let mut env = fork.env;
        env.version.features.remove(EvmFeatures::BALANCE_CHECK);
        env.version.features.insert(EvmFeatures::BALANCE_TOP_UP);
        let executor = EthereumExecutor::new_foundry(env, fork.state);
        let mut runner = NativeScriptRunner::new(
            executor,
            CALLER,
            self.evm_opts.sender,
            self.evm_opts.gas_limit(),
            self.evm_opts.env.gas_price.unwrap_or_default().into(),
        );
        let rpc =
            self.evm_opts.fork_url.as_deref().ok_or_else(|| eyre::eyre!("missing RPC URL"))?;
        for tx in execution.transactions() {
            eyre::ensure!(
                tx.rpc.as_deref().is_none_or(|url| url == rpc),
                "native script simulation across multiple RPCs is not implemented"
            );
            let run = runner.simulate(&tx.transaction)?;
            eyre::ensure!(run.result.status, "on-chain simulation failed: {:?}", run.result.stop);
        }
        Ok(())
    }

    fn execute_on<D: Database + Clone + 'static>(
        &self,
        env: EthereumEnv,
        mut state: LocalState<D>,
    ) -> Result<NativeScriptExecution> {
        state.set_balance(CALLER, U256::MAX)?;
        state.set_nonce(self.evm_opts.sender, self.sender_nonce)?;
        if self.evm_opts.sender == Config::DEFAULT_SENDER {
            state.set_balance(self.evm_opts.sender, U256::MAX)?;
        }

        let mut executor = EthereumExecutor::new_foundry(env, state);
        let inspector = executor.inspector_mut();
        inspector.set_cheatcode_config(CheatsConfig::new(
            &self.config,
            self.evm_opts.clone(),
            Some(self.plan.build.known_contracts.clone()),
            Some(self.plan.build.build_data.target.clone()),
            false,
        ));
        inspector.enable_tracing(
            TracingInspectorConfig::default()
                .set_bytecode(self.config.tracing.verbosity > 3)
                .set_steps_and_state_diffs(self.config.tracing.verbosity > 4),
        );
        if self.evm_opts.isolate {
            inspector.enable_isolation();
        }

        let gas_price =
            self.evm_opts.env.gas_price.unwrap_or_default().max(env.block.basefee.to::<u64>());
        let mut runner = NativeScriptRunner::new(
            executor,
            CALLER,
            self.evm_opts.sender,
            self.evm_opts.gas_limit(),
            u128::from(gas_price),
        );
        if self.evm_opts.fork_url.is_none() && !self.args.broadcast {
            self.install_default_create2_deployer(&mut runner)?;
        }
        let (libraries, library_transactions) = self.deploy_libraries(&mut runner)?;
        let restore_sender_nonce = self.evm_opts.sender == CALLER;
        let mut accepted = runner.executor().state().clone();
        let sender_nonce = Database::get_account(&mut accepted, &CALLER)?.map_or(0, |a| a.nonce);
        let deployer_nonce = if restore_sender_nonce { u64::MAX / 2 } else { sender_nonce };
        if restore_sender_nonce {
            runner.executor_mut().state_mut().set_nonce(CALLER, deployer_nonce)?;
        }
        runner
            .executor_mut()
            .state_mut()
            .set_balance(CALLER.create(deployer_nonce), self.evm_opts.initial_balance)?;
        let deployment = runner.deploy(self.plan.execution.bytecode.clone())?;
        if restore_sender_nonce {
            runner.executor_mut().state_mut().set_nonce(CALLER, sender_nonce)?;
        }
        let Some(address) = deployment.result.created_address.filter(|_| deployment.result.status)
        else {
            return Ok(NativeScriptExecution {
                libraries,
                library_transactions,
                deployment,
                setup: None,
                script: None,
            });
        };
        if self.config.script_execution_protection {
            runner.executor_mut().inspector_mut().set_script_execution(address);
        }
        let setup = if needs_setup(&self.plan.execution.abi) {
            let input = Bytes::copy_from_slice(&keccak256("setUp()")[..4]);
            Some(runner.setup(address, input)?)
        } else {
            None
        };
        let script = if setup.as_ref().is_none_or(|setup| setup.result.status) {
            Some(runner.script(address, self.plan.execution.calldata.clone())?)
        } else {
            None
        };
        Ok(NativeScriptExecution { libraries, library_transactions, deployment, setup, script })
    }

    fn deploy_libraries<D: Database + Clone + 'static>(
        &self,
        runner: &mut NativeScriptRunner<D>,
    ) -> Result<(Vec<NativeScriptRun>, BroadcastableTransactions<Ethereum>)> {
        let (onchain, local, salt) = match &self.plan.build.predeploy_libraries {
            ScriptPredeployLibraries::Default { onchain, local } => (onchain, local, None),
            ScriptPredeployLibraries::Create2 { onchain, local, salt } => {
                (onchain, local, Some(salt))
            }
        };
        let mut runs = Vec::with_capacity(local.len() + onchain.len());
        if !local.is_empty() {
            let mut accepted = runner.executor().state().clone();
            let original =
                Database::get_account(&mut accepted, &LIBRARY_DEPLOYER)?.unwrap_or_default();
            runner.executor_mut().state_mut().set_balance(LIBRARY_DEPLOYER, U256::MAX)?;
            runner.executor_mut().state_mut().set_nonce(LIBRARY_DEPLOYER, 0)?;
            for library in local {
                let run = runner.deploy_from(LIBRARY_DEPLOYER, library.bytecode.clone())?;
                eyre::ensure!(
                    run.result.status && run.result.created_address == Some(library.address),
                    "local library deployed at an unexpected address"
                );
                runs.push(run);
            }
            runner.executor_mut().state_mut().set_balance(LIBRARY_DEPLOYER, original.balance)?;
            runner.executor_mut().state_mut().set_nonce(LIBRARY_DEPLOYER, original.nonce)?;
        }

        let mut transactions = BroadcastableTransactions::default();
        for library in onchain {
            let (run, transaction) = if let Some(salt) = salt {
                let mut accepted = runner.executor().state().clone();
                if Database::get_account(&mut accepted, &library.address)?
                    .is_some_and(|account| account.code_hash != KECCAK256_EMPTY)
                {
                    continue;
                }
                let input = Bytes::from([salt.as_slice(), library.bytecode.as_ref()].concat());
                let run = runner.call_commit(
                    self.evm_opts.sender,
                    self.evm_opts.create2_deployer,
                    input.clone(),
                )?;
                let mut accepted = runner.executor().state().clone();
                eyre::ensure!(
                    run.result.status
                        && Database::get_account(&mut accepted, &library.address)?
                            .is_some_and(|account| account.code_hash != KECCAK256_EMPTY),
                    "CREATE2 library deployed at an unexpected address"
                );
                let transaction = TransactionRequest::default()
                    .with_from(self.evm_opts.sender)
                    .with_to(self.evm_opts.create2_deployer)
                    .with_input(input)
                    .with_nonce(self.sender_nonce + transactions.len() as u64);
                (run, transaction)
            } else {
                let run = runner.deploy_from(self.evm_opts.sender, library.bytecode.clone())?;
                eyre::ensure!(
                    run.result.status && run.result.created_address == Some(library.address),
                    "on-chain library deployed at an unexpected address"
                );
                let transaction = TransactionRequest::default()
                    .with_from(self.evm_opts.sender)
                    .with_input(library.bytecode.clone())
                    .with_nonce(self.sender_nonce + transactions.len() as u64);
                (run, transaction)
            };
            transactions.push_back(BroadcastableTransaction {
                rpc: self.evm_opts.fork_url.clone(),
                transaction: TransactionMaybeSigned::new(transaction),
            });
            runs.push(run);
        }
        Ok((runs, transactions))
    }

    fn install_default_create2_deployer<D: Database + Clone + 'static>(
        &self,
        runner: &mut NativeScriptRunner<D>,
    ) -> Result<()> {
        let mut accepted = runner.executor().state().clone();
        if Database::get_account(&mut accepted, &DEFAULT_CREATE2_DEPLOYER)?
            .is_some_and(|account| account.code_hash != KECCAK256_EMPTY)
        {
            return Ok(());
        }
        let creator = DEFAULT_CREATE2_DEPLOYER_DEPLOYER;
        let original = Database::get_account(&mut accepted, &creator)?.unwrap_or_default();
        runner.executor_mut().state_mut().set_balance(creator, U256::MAX)?;
        let deployment = runner.deploy_from(creator, DEFAULT_CREATE2_DEPLOYER_CODE.into())?;
        runner.executor_mut().state_mut().set_balance(creator, original.balance)?;
        eyre::ensure!(
            deployment.result.status
                && deployment.result.created_address == Some(DEFAULT_CREATE2_DEPLOYER),
            "default CREATE2 deployer deployment failed"
        );
        Ok(())
    }

    fn show_output(&self, execution: &NativeScriptExecution) -> Result<()> {
        let run =
            execution.script.as_ref().or(execution.setup.as_ref()).unwrap_or(&execution.deployment);
        let success = execution.deployment.result.status
            && execution.setup.as_ref().is_none_or(|setup| setup.result.status)
            && execution.script.as_ref().is_none_or(|script| script.result.status);
        let verbosity = self.config.tracing.verbosity;
        if !success || verbosity > 3 {
            sh_println!("Traces:")?;
            let decoder =
                NativeTraceDecoder::new().with_known_contracts(&self.plan.build.known_contracts);
            let stages = if success {
                execution
                    .setup
                    .iter()
                    .filter(|_| verbosity >= 5)
                    .chain(execution.script.iter())
                    .collect::<Vec<_>>()
            } else {
                execution
                    .libraries
                    .iter()
                    .chain(std::iter::once(&execution.deployment))
                    .chain(execution.setup.iter())
                    .chain(execution.script.iter())
                    .collect::<Vec<_>>()
            };
            for stage in stages {
                for arena in &stage.traces {
                    let mut output = Vec::new();
                    TraceWriter::new(&mut output)
                        .with_storage_changes(verbosity > 4)
                        .write_arena(&decoder.decode_for_display(arena))?;
                    sh_println!("{}", String::from_utf8(output)?)?;
                }
            }
            sh_println!()?;
        }
        if success {
            sh_println!("{}", "Script ran successfully.".green())?;
        }
        if self.evm_opts.fork_url.is_none() {
            sh_println!("Gas used: {}", run.result.tx_gas_used())?;
        }
        if success && !run.result.output.is_empty() {
            sh_println!("\n== Return ==")?;
            if let Ok(decoded) = self.plan.execution.func.abi_decode_output(&run.result.output) {
                for (index, (token, output)) in
                    decoded.iter().zip(&self.plan.execution.func.outputs).enumerate()
                {
                    let internal_type =
                        output.internal_type.clone().unwrap_or(InternalType::Other {
                            contract: None,
                            ty: "unknown".to_string(),
                        });
                    let label = if output.name.is_empty() {
                        index.to_string()
                    } else {
                        output.name.clone()
                    };
                    sh_println!("{}: {} {}", label.trim_end(), internal_type, format_token(token))?;
                }
            } else {
                sh_println!("{:x?}", run.result.output)?;
            }
        }
        let logs = execution
            .libraries
            .iter()
            .chain(std::iter::once(&execution.deployment))
            .chain(execution.setup.iter())
            .chain(execution.script.iter())
            .flat_map(|stage| stage.logs.iter().cloned())
            .collect::<Vec<_>>();
        let logs = decode_console_logs(&logs);
        if !logs.is_empty() {
            sh_println!("\n== Logs ==")?;
            for log in logs {
                sh_println!("  {log}")?;
            }
        }
        if !success {
            let reason = if run.result.output.is_empty() {
                format!("EvmError: {:?}", run.result.stop)
            } else {
                RevertDecoder::new()
                    .with_abis(
                        self.plan.build.known_contracts.values().map(|contract| &contract.abi),
                    )
                    .decode(&run.result.output, None)
            };
            eyre::bail!("script failed: {reason}");
        }
        if execution.has_transactions() && self.evm_opts.fork_url.is_none() {
            sh_println!("\nIf you wish to simulate on-chain transactions pass a RPC URL.")?;
        }
        Ok(())
    }

    fn show_json(&self, execution: &NativeScriptExecution) -> Result<()> {
        let run =
            execution.script.as_ref().or(execution.setup.as_ref()).unwrap_or(&execution.deployment);
        let success = execution.deployment.result.status
            && execution.setup.as_ref().is_none_or(|setup| setup.result.status)
            && execution.script.as_ref().is_none_or(|script| script.result.status);
        let decoder =
            NativeTraceDecoder::new().with_known_contracts(&self.plan.build.known_contracts);
        let stages = execution
            .libraries
            .iter()
            .chain(std::iter::once(&execution.deployment))
            .map(|stage| (TraceKind::Deployment, stage))
            .chain(execution.setup.iter().map(|stage| (TraceKind::Setup, stage)))
            .chain(execution.script.iter().map(|stage| (TraceKind::Execution, stage)));
        let traces = stages
            .flat_map(|(kind, stage)| {
                let decoder = &decoder;
                stage.traces.iter().map(move |arena| {
                    let arena = decoder.decode_for_display(arena);
                    let arena = if let Some(depth) = self.config.tracing.trace_depth {
                        trace_arena_at_depth(&arena, depth)
                    } else {
                        arena
                    };
                    (kind, arena)
                })
            })
            .collect::<Vec<_>>();
        let mut returns = HashMap::default();
        if success
            && let Ok(decoded) = self.plan.execution.func.abi_decode_output(&run.result.output)
        {
            for (index, (token, output)) in
                decoded.iter().zip(&self.plan.execution.func.outputs).enumerate()
            {
                let internal_type = output
                    .internal_type
                    .clone()
                    .unwrap_or(InternalType::Other { contract: None, ty: "unknown".to_string() });
                let label =
                    if output.name.is_empty() { index.to_string() } else { output.name.clone() };
                returns.insert(
                    label,
                    NestedValue {
                        internal_type: internal_type.to_string(),
                        value: format_token_raw(token),
                    },
                );
            }
        }
        let logs = execution
            .libraries
            .iter()
            .chain(std::iter::once(&execution.deployment))
            .chain(execution.setup.iter())
            .chain(execution.script.iter())
            .flat_map(|stage| stage.logs.iter().cloned())
            .collect::<Vec<_>>();
        let json = NativeJsonResult {
            logs: decode_console_logs(&logs),
            returns,
            success,
            raw_logs: logs,
            traces,
            gas_used: run.result.tx_gas_used(),
            labeled_addresses: HashMap::default(),
            returned: &run.result.output,
            address: None,
        };
        sh_println!("{}", serde_json::to_string(&json)?)?;
        if !success {
            let reason = if run.result.output.is_empty() {
                format!("EvmError: {:?}", run.result.stop)
            } else {
                RevertDecoder::new()
                    .with_abis(
                        self.plan.build.known_contracts.values().map(|contract| &contract.abi),
                    )
                    .decode(&run.result.output, None)
            };
            eyre::bail!("script failed: {reason}");
        }
        Ok(())
    }
}

#[derive(Serialize)]
struct NativeJsonResult<'a> {
    logs: Vec<String>,
    returns: HashMap<String, NestedValue>,
    success: bool,
    raw_logs: Vec<Log>,
    traces: Vec<(TraceKind, CallTraceArena)>,
    gas_used: u64,
    labeled_addresses: HashMap<Address, String>,
    returned: &'a Bytes,
    address: Option<Address>,
}

/// Native constructor, setup, and script-call observations.
pub struct NativeScriptExecution {
    pub libraries: Vec<NativeScriptRun>,
    pub library_transactions: BroadcastableTransactions<Ethereum>,
    pub deployment: NativeScriptRun,
    pub setup: Option<NativeScriptRun>,
    pub script: Option<NativeScriptRun>,
}

impl NativeScriptExecution {
    fn has_transactions(&self) -> bool {
        !self.library_transactions.is_empty()
            || self
                .libraries
                .iter()
                .chain(std::iter::once(&self.deployment))
                .chain(self.setup.iter())
                .chain(self.script.iter())
                .any(|stage| !stage.transactions.is_empty())
    }

    fn transactions(&self) -> impl Iterator<Item = &BroadcastableTransaction<Ethereum>> {
        self.library_transactions
            .iter()
            .chain(self.libraries.iter().flat_map(|stage| &stage.transactions))
            .chain(&self.deployment.transactions)
            .chain(self.setup.iter().flat_map(|stage| &stage.transactions))
            .chain(self.script.iter().flat_map(|stage| &stage.transactions))
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
        self.deploy_from(self.deployer, code)
    }

    /// Deploys a linked library from its assigned deployer.
    pub fn deploy_from(&mut self, sender: Address, code: Bytes) -> Result<NativeScriptRun> {
        let tx = self.transaction(sender, TxKind::Create, code)?;
        let result = self.executor.transact(&tx)?;
        Ok(Self::collect(result, self.executor.inspector_mut()))
    }

    /// Executes and accepts a transaction sent to a deployer contract.
    pub fn call_commit(
        &mut self,
        sender: Address,
        address: Address,
        input: Bytes,
    ) -> Result<NativeScriptRun> {
        let tx = self.transaction(sender, TxKind::Call(address), input)?;
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

    /// Replays a collected on-chain transaction and accepts its state for subsequent calls.
    pub fn simulate(
        &mut self,
        transaction: &TransactionMaybeSigned<Ethereum>,
    ) -> Result<NativeScriptRun> {
        let (typed, sender) = match transaction {
            TransactionMaybeSigned::Signed { tx, from } => (tx.clone().into(), *from),
            TransactionMaybeSigned::Unsigned(request) => {
                let sender =
                    request.from.ok_or_else(|| eyre::eyre!("missing transaction sender"))?;
                let mut request = request.clone();
                let mut state = self.executor.state().clone();
                let nonce =
                    Database::get_account(&mut state, &sender)?.map_or(0, |account| account.nonce);
                request.nonce.get_or_insert(nonce);
                request.gas.get_or_insert(self.effective_gas_limit());
                request.chain_id.get_or_insert(self.executor.env().version.chain_id);
                let basefee = self.executor.env().block.basefee.to::<u128>();
                let price = self.gas_price.max(basefee);
                if request.max_fee_per_gas.is_some()
                    || request.max_priority_fee_per_gas.is_some()
                    || request.authorization_list.is_some()
                    || request.blob_versioned_hashes.is_some()
                {
                    request.max_fee_per_gas.get_or_insert(price);
                    request.max_priority_fee_per_gas.get_or_insert(0);
                } else {
                    request.gas_price.get_or_insert(price);
                }
                let typed = request
                    .build_unsigned()
                    .map_err(|err| eyre::eyre!("invalid simulation transaction: {err}"))?;
                (typed, sender)
            }
        };
        let typed =
            alloy_consensus::EthereumTypedTransaction::<alloy_consensus::TxEip4844>::from(typed);
        let envelope =
            TxEnvelope::from(typed.into_envelope(Signature::new(U256::ONE, U256::ONE, false)));
        let tx = Recovered::new_unchecked(envelope, sender);
        let result = self.executor.transact(&tx)?;
        Ok(Self::collect(result, self.executor.inspector_mut()))
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
                gas_limit: self.effective_gas_limit(),
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

    fn effective_gas_limit(&self) -> u64 {
        if self.executor.env().version.features.contains(EvmFeatures::BLOCK_GAS_LIMIT_CHECK) {
            self.executor.env().block.gas_limit.min(U256::from(self.gas_limit)).to::<u64>()
        } else {
            self.gas_limit
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_sol_types::SolCall;
    use evm2::{SpecId, env::BlockEnvExt};
    use foundry_cheatcodes::Vm;
    use foundry_evm::core::constants::CHEATCODE_ADDRESS;

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
        let opts = EvmOpts {
            sender: CALLER,
            env: foundry_evm::opts::Env {
                gas_limit: foundry_config::GasLimit(30_000_000),
                ..Default::default()
            },
            memory_limit: 1_000_000,
            ..Default::default()
        };
        let context = NativeScriptContext::prepare(args, config, opts).await.unwrap();
        assert_eq!(context.sender_nonce, 1);
        let execution = context.execute().await.unwrap();
        assert!(execution.deployment.result.status, "{:#?}", execution.deployment.result);
        assert_eq!(execution.deployment.result.created_address, Some(CALLER.create(u64::MAX / 2)));
        assert!(execution.setup.as_ref().unwrap().result.status);
        let script = execution.script.unwrap();
        assert!(script.result.status);
        assert_eq!(U256::from_be_slice(&script.result.output), U256::from(5));
    }

    #[tokio::test]
    async fn linked_library_deploys_before_native_script() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("src/Linked.s.sol");
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::write(
            &source,
            "pragma solidity ^0.8.20; library Lib { function bump(uint256 value) public pure returns (uint256) { return value + 1; } } contract LinkedScript { uint256 value; function run() external returns (uint256) { value = 4; return Lib.bump(value); } }",
        )
        .unwrap();
        let mut config = Config::with_root(root.path());
        config.cache_path = root.path().join("cache");
        let args = ScriptArgs {
            path: source.to_string_lossy().into_owned(),
            target_contract: Some("LinkedScript".into()),
            sig: "run()".into(),
            ..Default::default()
        };
        let opts = EvmOpts {
            sender: CALLER,
            create2_deployer: Address::ZERO,
            env: foundry_evm::opts::Env {
                gas_limit: foundry_config::GasLimit(30_000_000),
                ..Default::default()
            },
            memory_limit: 1_000_000,
            ..Default::default()
        };
        let context =
            NativeScriptContext::prepare(args.clone(), config.clone(), opts).await.unwrap();
        assert_eq!(context.plan.build.predeploy_libraries.libraries_count(), 1);
        let execution = context.execute().await.unwrap();
        assert_eq!(execution.libraries.len(), 1);
        assert_eq!(execution.library_transactions.len(), 1);
        assert!(execution.libraries[0].result.status);
        let library_tx = &execution.library_transactions.front().unwrap().transaction;
        assert_eq!(library_tx.from(), Some(CALLER));
        assert_eq!(library_tx.nonce(), Some(1));
        assert_eq!(library_tx.to(), None);
        assert!(execution.deployment.result.status);
        let script = execution.script.unwrap();
        assert!(script.result.status);
        assert_eq!(U256::from_be_slice(&script.result.output), U256::from(5));

        let opts = EvmOpts {
            sender: CALLER,
            env: foundry_evm::opts::Env {
                gas_limit: foundry_config::GasLimit(30_000_000),
                ..Default::default()
            },
            memory_limit: 1_000_000,
            ..Default::default()
        };
        let context = NativeScriptContext::prepare(args, config, opts).await.unwrap();
        assert!(matches!(
            context.plan.build.predeploy_libraries,
            ScriptPredeployLibraries::Create2 { .. }
        ));
        let execution = context.execute().await.unwrap();
        assert_eq!(execution.libraries.len(), 1);
        assert_eq!(execution.library_transactions.len(), 1);
        let library_tx = &execution.library_transactions.front().unwrap().transaction;
        assert_eq!(library_tx.from(), Some(CALLER));
        assert_eq!(library_tx.to(), Some(DEFAULT_CREATE2_DEPLOYER));
        assert_eq!(library_tx.nonce(), Some(1));
        assert!(execution.deployment.result.status);
        let script = execution.script.unwrap();
        assert!(script.result.status);
        assert_eq!(U256::from_be_slice(&script.result.output), U256::from(5));
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

        let mut simulation_env = env;
        simulation_env.version.features.remove(EvmFeatures::BALANCE_CHECK);
        simulation_env.version.features.insert(EvmFeatures::BALANCE_TOP_UP);
        let executor =
            EthereumExecutor::new_foundry(simulation_env, runner.executor().state().clone());
        let mut simulation = NativeScriptRunner::new(executor, sender, sender, 100_000, 0);
        let simulated = simulation.simulate(transaction).unwrap();
        assert!(simulated.result.status, "{:#?}", simulated.result);
        assert_eq!(
            Database::get_storage(
                &mut simulation.executor().state().clone(),
                &address,
                &U256::ZERO
            )
            .unwrap(),
            U256::from(2)
        );
    }
}
