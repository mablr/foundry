use super::{
    Cheatcodes, CheatsConfig, ChiselState, CmpOperands, CustomPrintTracer, EdgeCovConfig,
    EdgeCovInspector, EdgeCoverage, Fuzzer, LineCoverageCollector, LogCollector, RevertDiagnostic,
    ScriptExecutionInspector, TracingInspector,
};
use crate::executors::{EarlyExit, EvmExecutionCancellation};
use alloy_primitives::{Address, Log, U256, map::AddressHashMap};
use foundry_cheatcodes::{CheatcodeAnalysis, Wallets};
use foundry_common::compile::Analysis;
use foundry_config::FuzzCorpusConfig;
use foundry_evm_core::evm::{BlockEnvFor, EthEvmNetwork, FoundryEvmNetwork};
use foundry_evm_coverage::HitMaps;
use foundry_evm_networks::NetworkConfigs;
use foundry_evm_traces::{SparsedTraceArena, TraceRequirements};
use std::{
    ops::{Deref, DerefMut},
    sync::Arc,
};

#[derive(Clone, Debug)]
#[must_use = "builders do nothing unless you call `build` on them"]
pub struct InspectorStackBuilder<BLOCK: Clone> {
    /// Solar compiler instance, to grant syntactic and semantic analysis capabilities.
    pub analysis: Option<Analysis>,
    /// The block environment.
    ///
    /// Used in the cheatcode handler to overwrite the block environment separately from the
    /// execution block environment.
    pub block: Option<BLOCK>,
    /// The gas price.
    ///
    /// Used in the cheatcode handler to overwrite the gas price separately from the gas price
    /// in the execution environment.
    pub gas_price: Option<u128>,
    /// The cheatcodes config.
    pub cheatcodes: Option<Arc<CheatsConfig>>,
    /// The fuzzer inspector and its state, if it exists.
    pub fuzzer: Option<Fuzzer>,
    /// Whether to enable tracing and revert diagnostics.
    pub trace_requirements: TraceRequirements,
    /// Whether logs should be collected.
    /// - None for no log collection.
    /// - Some(true) for realtime console.log-ing.
    /// - Some(false) for log collection.
    pub logs: Option<bool>,
    /// Whether line coverage info should be collected.
    pub line_coverage: Option<bool>,
    /// Whether to print all opcode traces into the console. Useful for debugging the EVM.
    pub print: Option<bool>,
    /// The chisel state inspector.
    pub chisel_state: Option<usize>,
    /// Whether to enable call isolation.
    /// In isolation mode all top-level calls are executed as a separate transaction in a separate
    /// EVM context, enabling more precise gas accounting and transaction state changes.
    pub enable_isolation: bool,
    /// Configuration retained for Celo precompile support.
    // TODO(celo-execution-owner): Replace this residual with concrete Celo precompile
    // configuration. This is independent of the Monad lifecycle migration.
    pub networks: NetworkConfigs,
    /// Concrete Tempo label inspector selected by the Tempo executor builder.
    //     tempo_labels: Option<Box<TempoLabels>>,
    /// Explicitly resolved additional cheatcode addresses.
    pub extra_cheatcode_addresses: &'static [Address],
    /// The wallets to set in the cheatcodes context.
    pub wallets: Option<Wallets>,
    /// The CREATE2 deployer address.
    pub create2_deployer: Address,
}

impl<BLOCK: Clone> Default for InspectorStackBuilder<BLOCK> {
    fn default() -> Self {
        Self {
            analysis: None,
            block: None,
            gas_price: None,
            cheatcodes: None,
            fuzzer: None,
            trace_requirements: TraceRequirements::none(),
            logs: None,
            line_coverage: None,
            print: None,
            chisel_state: None,
            enable_isolation: false,
            networks: NetworkConfigs::default(),
            //             tempo_labels: None,
            extra_cheatcode_addresses: &[],
            wallets: None,
            create2_deployer: Default::default(),
        }
    }
}

impl<BLOCK: Clone> InspectorStackBuilder<BLOCK> {
    /// Create a new inspector stack builder.
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the solar compiler instance that grants syntactic and semantic analysis capabilities
    #[inline]
    pub fn set_analysis(mut self, analysis: Analysis) -> Self {
        self.analysis = Some(analysis);
        self
    }

    /// Set the block environment.
    #[inline]
    pub fn block(mut self, block: BLOCK) -> Self {
        self.block = Some(block);
        self
    }

    /// Set the gas price.
    #[inline]
    pub const fn gas_price(mut self, gas_price: u128) -> Self {
        self.gas_price = Some(gas_price);
        self
    }

    /// Enable cheatcodes with the given config.
    #[inline]
    pub fn cheatcodes(mut self, config: Arc<CheatsConfig>) -> Self {
        self.cheatcodes = Some(config);
        self
    }

    /// Set the wallets.
    #[inline]
    pub fn wallets(mut self, wallets: Wallets) -> Self {
        self.wallets = Some(wallets);
        self
    }

    /// Set the fuzzer inspector.
    #[inline]
    pub fn fuzzer(mut self, fuzzer: Fuzzer) -> Self {
        self.fuzzer = Some(fuzzer);
        self
    }

    /// Set the Chisel inspector.
    #[inline]
    pub const fn chisel_state(mut self, final_pc: usize) -> Self {
        self.chisel_state = Some(final_pc);
        self
    }

    /// Set the log collector, and whether to print the logs directly to stdout.
    #[inline]
    pub const fn logs(mut self, live_logs: bool) -> Self {
        self.logs = Some(live_logs);
        self
    }

    /// Set whether to collect line coverage information.
    #[inline]
    pub const fn line_coverage(mut self, yes: bool) -> Self {
        self.line_coverage = Some(yes);
        self
    }

    /// Set whether to enable the trace printer.
    #[inline]
    pub const fn print(mut self, yes: bool) -> Self {
        self.print = Some(yes);
        self
    }

    /// Set trace data requirements.
    #[inline]
    pub const fn trace_requirements(mut self, requirements: TraceRequirements) -> Self {
        self.trace_requirements = self.trace_requirements.merge(requirements);
        self
    }

    /// Set whether to enable the call isolation.
    /// For description of call isolation, see [`InspectorStack::enable_isolation`].
    #[inline]
    pub const fn enable_isolation(mut self, yes: bool) -> Self {
        self.enable_isolation = yes;
        self
    }

    /// Sets networks when building an inspector stack directly.
    ///
    /// [`ExecutorBuilder::build`](crate::executors::ExecutorBuilder::build) overrides this with its
    /// explicit network configuration so the executor, backend, and inspector remain in sync.
    #[inline]
    pub const fn networks(mut self, networks: NetworkConfigs) -> Self {
        self.networks = networks;
        self
    }

    /* EVM2 migration: disabled non-Ethereum execution.
    /// Installs the Tempo label inspector.
        #[inline]
    //     pub(crate) fn tempo_labels(mut self, inspector: TempoLabels) -> Self {
    //         self.tempo_labels = Some(Box::new(inspector));
            self
        }
    */

    /// Sets explicitly resolved additional cheatcode addresses.
    #[inline]
    pub const fn extra_cheatcode_addresses(mut self, addresses: &'static [Address]) -> Self {
        self.extra_cheatcode_addresses = addresses;
        self
    }

    #[inline]
    pub const fn create2_deployer(mut self, create2_deployer: Address) -> Self {
        self.create2_deployer = create2_deployer;
        self
    }

    /// Builds the stack of inspectors to use when transacting/committing on the EVM.
    pub fn build<FEN: FoundryEvmNetwork<Block = BLOCK>>(self) -> InspectorStack<FEN> {
        let Self {
            analysis,
            block,
            gas_price,
            cheatcodes,
            fuzzer,
            trace_requirements,
            logs,
            line_coverage,
            print,
            chisel_state,
            enable_isolation,
            networks,
            //             tempo_labels,
            extra_cheatcode_addresses,
            wallets,
            create2_deployer,
        } = self;
        let mut stack = InspectorStack::new();
        // inspectors
        if let Some(config) = cheatcodes {
            let mut cheatcodes = Cheatcodes::new(config);
            cheatcodes.set_extra_cheatcode_addresses(extra_cheatcode_addresses);
            // Set analysis capabilities if they are provided
            if let Some(analysis) = analysis {
                stack.set_analysis(analysis.clone());
                cheatcodes.set_analysis(CheatcodeAnalysis::new(analysis));
            }
            // Set wallets if they are provided
            if let Some(wallets) = wallets {
                cheatcodes.set_wallets(wallets);
            }
            stack.set_cheatcodes(cheatcodes);
        }

        if let Some(fuzzer) = fuzzer {
            stack.set_fuzzer(fuzzer);
        }
        if let Some(chisel_state) = chisel_state {
            stack.set_chisel(chisel_state);
        }
        stack.collect_line_coverage(line_coverage.unwrap_or(false));
        stack.collect_logs(logs);
        stack.print(print.unwrap_or(false));
        stack.tracing_requirements(trace_requirements);

        stack.enable_isolation(enable_isolation);
        stack.networks(networks);
        //         stack.inner.tempo_labels = tempo_labels;
        stack.set_extra_cheatcode_addresses(extra_cheatcode_addresses);
        stack.set_create2_deployer(create2_deployer);

        // environment, must come after all of the inspectors
        if let Some(block) = block {
            stack.set_block(block);
        }
        if let Some(gas_price) = gas_price {
            stack.set_gas_price(gas_price);
        }

        stack
    }
}

/// Helper macro to call the same method on multiple inspectors without resorting to dynamic
/// dispatch.
#[macro_export]
macro_rules! call_inspectors {
    ([$($inspector:expr),+ $(,)?], |$id:ident $(,)?| $body:expr $(,)?) => {
        $(
            if let Some($id) = $inspector {
                $crate::utils::cold_path();
                $body;
            }
        )+
    };
    (#[ret] [$($inspector:expr),+ $(,)?], |$id:ident $(,)?| $body:expr $(,)?) => {{
        $(
            if let Some($id) = $inspector {
                $crate::utils::cold_path();
                if let Some(result) = $body {
                    return result;
                }
            }
        )+
    }};
}

/// The collected results of [`InspectorStack`].
pub struct InspectorData<FEN: FoundryEvmNetwork> {
    pub logs: Vec<Log>,
    pub labels: AddressHashMap<String>,
    pub traces: Option<SparsedTraceArena>,
    pub line_coverage: Option<HitMaps>,
    pub edge_coverage: Option<EdgeCoverage>,
    pub evm_cmp_values: Option<Vec<CmpOperands>>,
    pub cheatcodes: Option<Box<Cheatcodes<FEN>>>,
    pub chisel_state: Option<(Vec<U256>, Vec<u8>)>,
    pub reverter: Option<Address>,
}

/// Configuration and captured data consumed by the native evm2 inspector.
///
/// TODO(evm2): Replace the remaining legacy cheatcode and observer data types as their native
/// implementations land. This stack no longer executes REVM inspection callbacks.
#[derive(Clone, Debug)]
pub struct InspectorStack<FEN: FoundryEvmNetwork = EthEvmNetwork> {
    #[allow(clippy::type_complexity)]
    pub cheatcodes: Option<Box<Cheatcodes<FEN>>>,
    pub inner: InspectorStackInner,
}

#[cfg(test)]
#[derive(Clone, Debug)]
pub(crate) struct EarlyExitTestGate {
    entered: std::sync::mpsc::Sender<()>,
    release: Arc<std::sync::Mutex<std::sync::mpsc::Receiver<()>>>,
    target_pc: usize,
    notified: Arc<std::sync::atomic::AtomicBool>,
}

/// All used inspectors besides [Cheatcodes].
///
/// See [`InspectorStack`].
#[derive(Default, Clone, Debug)]
pub struct InspectorStackInner {
    /// Solar compiler instance, to grant syntactic and semantic analysis capabilities.
    pub analysis: Option<Analysis>,

    // Inspectors.
    // These are boxed to reduce the size of the struct and slightly improve performance of the
    // `if let Some` checks.
    pub chisel_state: Option<Box<ChiselState>>,
    pub edge_coverage: Option<Box<EdgeCovInspector>>,
    pub fuzzer: Option<Box<Fuzzer>>,
    pub line_coverage: Option<Box<LineCoverageCollector>>,
    pub log_collector: Option<Box<LogCollector>>,
    pub printer: Option<Box<CustomPrintTracer>>,
    pub revert_diag: Option<Box<RevertDiagnostic>>,
    pub script_execution_inspector: Option<Box<ScriptExecutionInspector>>,
    //     pub tempo_labels: Option<Box<TempoLabels>>,
    pub tracer: Option<Box<TracingInspector>>,

    /// Whether to collect sancov edge coverage from instrumented native crates.
    pub sancov_edges: bool,
    /// Whether to capture sancov trace-cmp operands for dictionary injection.
    pub sancov_trace_cmp: bool,
    pub enable_isolation: bool,
    pub networks: NetworkConfigs,
    /// Additional addresses installed and recognized as cheatcode contracts.
    pub extra_cheatcode_addresses: &'static [Address],
    pub create2_deployer: Address,
    /// Address that reverted the call, if any.
    pub reverter: Option<Address>,
    /// Shared cancellation state for interruptible EVM execution.
    execution_cancellation: Option<EvmExecutionCancellation>,
    #[cfg(test)]
    early_exit_test_gate: Option<EarlyExitTestGate>,
}

impl<FEN: FoundryEvmNetwork> Default for InspectorStack<FEN> {
    fn default() -> Self {
        Self::new()
    }
}

impl<FEN: FoundryEvmNetwork> InspectorStack<FEN> {
    /// Creates a new inspector stack.
    ///
    /// Note that the stack is empty by default, and you must add inspectors to it.
    /// This is done by calling the `set_*` methods on the stack directly, or by building the stack
    /// with [`InspectorStack`].
    #[inline]
    pub fn new() -> Self {
        Self { cheatcodes: None, inner: InspectorStackInner::default() }
    }

    /// Set the solar compiler instance.
    #[inline]
    pub fn set_analysis(&mut self, analysis: Analysis) {
        self.analysis = Some(analysis);
    }

    /// Set the cancellation state checked during EVM execution.
    #[inline]
    pub(crate) fn set_early_exit(&mut self, early_exit: EarlyExit) {
        self.execution_cancellation = Some(EvmExecutionCancellation::early_exit(early_exit));
    }

    /// Set the complete cancellation state checked during EVM execution.
    #[inline]
    pub(crate) fn set_execution_cancellation(&mut self, cancellation: EvmExecutionCancellation) {
        self.execution_cancellation = Some(cancellation);
    }

    /// Returns the configured execution cancellation state.
    #[inline]
    pub(crate) const fn execution_cancellation(&self) -> Option<&EvmExecutionCancellation> {
        self.inner.execution_cancellation.as_ref()
    }

    #[cfg(test)]
    pub(crate) fn set_early_exit_test_gate(
        &mut self,
        entered: std::sync::mpsc::Sender<()>,
        release: std::sync::mpsc::Receiver<()>,
        target_pc: usize,
    ) {
        self.early_exit_test_gate = Some(EarlyExitTestGate {
            entered,
            release: Arc::new(std::sync::Mutex::new(release)),
            target_pc,
            notified: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        });
    }

    #[cfg(test)]
    pub(crate) fn early_exit_test_gate(&self) -> Option<EarlyExitTestGate> {
        self.inner.early_exit_test_gate.clone()
    }

    /// Sets the block for the relevant inspectors.
    #[inline]
    pub fn set_block(&mut self, block: BlockEnvFor<FEN>) {
        if let Some(cheatcodes) = &mut self.cheatcodes {
            cheatcodes.block = Some(block);
        }
    }

    /// Sets the gas price for the relevant inspectors.
    #[inline]
    pub fn set_gas_price(&mut self, gas_price: u128) {
        if let Some(cheatcodes) = &mut self.cheatcodes {
            cheatcodes.gas_price = Some(gas_price);
        }
    }

    /// Set the cheatcodes inspector.
    #[inline]
    pub fn set_cheatcodes(&mut self, cheatcodes: Cheatcodes<FEN>) {
        self.cheatcodes = Some(cheatcodes.into());
    }

    /// Set the fuzzer inspector.
    #[inline]
    pub fn set_fuzzer(&mut self, fuzzer: Fuzzer) {
        self.fuzzer = Some(fuzzer.into());
    }

    /// Set the Chisel inspector.
    #[inline]
    pub fn set_chisel(&mut self, final_pc: usize) {
        self.chisel_state = Some(ChiselState::new(final_pc).into());
    }

    /// Set whether to enable the line coverage collector.
    #[inline]
    pub fn collect_line_coverage(&mut self, yes: bool) {
        self.line_coverage = yes.then(Default::default);
    }

    /// Set whether to enable the edge coverage collector with default config.
    #[inline]
    pub fn collect_edge_coverage(&mut self, yes: bool) {
        self.edge_coverage =
            yes.then(|| EdgeCovInspector::with_config(EdgeCovConfig::default()).into());
    }

    /// Configure the edge coverage collector from a [`FuzzCorpusConfig`].
    ///
    /// Derives both the on/off gate and [`EdgeCovConfig`] from `corpus`.
    #[inline]
    pub fn collect_edge_coverage_with_config(&mut self, corpus: &FuzzCorpusConfig) {
        self.edge_coverage = corpus
            .collect_evm_edge_coverage()
            .then(|| EdgeCovInspector::with_config(corpus.into()).into());
    }

    /// Set whether to collect EVM comparison operands.
    #[inline]
    pub fn collect_evm_cmp_log(&mut self, yes: bool) {
        if yes {
            self.edge_coverage
                .get_or_insert_with(|| EdgeCovInspector::with_cmp_log_only().into())
                .enable_cmp_log(true);
        } else if let Some(edge_coverage) = &mut self.edge_coverage {
            edge_coverage.enable_cmp_log(false);
        }
    }

    /// Set whether to collect sancov edge coverage from instrumented native crates.
    #[inline]
    pub const fn collect_sancov_edges(&mut self, yes: bool) {
        self.inner.sancov_edges = yes;
    }

    /// Set whether to capture sancov trace-cmp operands for dictionary injection.
    #[inline]
    pub const fn collect_sancov_trace_cmp(&mut self, yes: bool) {
        self.inner.sancov_trace_cmp = yes;
    }

    /// Set whether to enable call isolation.
    #[inline]
    pub const fn enable_isolation(&mut self, yes: bool) {
        self.inner.enable_isolation = yes;
    }

    /// Set networks with enabled features.
    #[inline]
    pub const fn networks(&mut self, networks: NetworkConfigs) {
        self.inner.networks = networks;
    }

    /// Returns additional addresses installed and recognized as cheatcode contracts.
    #[inline]
    pub const fn extra_cheatcode_addresses(&self) -> &'static [Address] {
        self.inner.extra_cheatcode_addresses
    }

    /// Sets additional addresses installed and recognized as cheatcode contracts.
    #[inline]
    pub const fn set_extra_cheatcode_addresses(&mut self, addresses: &'static [Address]) {
        self.inner.extra_cheatcode_addresses = addresses;
    }

    /// Set the CREATE2 deployer address.
    #[inline]
    pub fn set_create2_deployer(&mut self, deployer: Address) {
        self.create2_deployer = deployer;
    }

    /// Set whether to enable the log collector.
    /// - None for no log collection.
    /// - Some(true) for realtime console.log-ing.
    /// - Some(false) for log collection.
    #[inline]
    pub fn collect_logs(&mut self, live_logs: Option<bool>) {
        self.log_collector = live_logs.map(|live_logs| {
            Box::new(if live_logs {
                LogCollector::LiveLogs
            } else {
                LogCollector::Capture { logs: Vec::new() }
            })
        });
    }

    /// Set whether to enable the trace printer.
    #[inline]
    pub fn print(&mut self, yes: bool) {
        self.printer = yes.then(Default::default);
    }

    /// Set trace data requirements.
    #[inline]
    pub fn tracing_requirements(&mut self, requirements: TraceRequirements) {
        let config = requirements.into_config();
        self.revert_diag = config.is_some().then(RevertDiagnostic::default).map(Into::into);

        if let Some(config) = config {
            *self.tracer.get_or_insert_with(Default::default).config_mut() = config;
        } else {
            self.tracer = None;
        }
    }

    /// Set whether to enable script execution inspector.
    #[inline]
    pub fn script(&mut self, script_address: Address) {
        self.script_execution_inspector.get_or_insert_with(Default::default).script_address =
            script_address;
        if let Some(cheatcodes) = &mut self.cheatcodes {
            cheatcodes.script_address = Some(script_address);
        }
    }

    /// Collects all the data gathered during inspection into a single struct.
    pub fn collect(self) -> InspectorData<FEN> {
        let Self {
            mut cheatcodes,
            inner:
                InspectorStackInner {
                    chisel_state,
                    line_coverage,
                    edge_coverage,
                    log_collector,
                    //                     tempo_labels,
                    tracer,
                    revert_diag,
                    reverter,
                    ..
                },
        } = self;

        let trace_diagnostics =
            revert_diag.map(|revert_diag| revert_diag.into_diagnostics()).unwrap_or_default();

        let traces = tracer.map(|tracer| tracer.into_traces()).map(|arena| {
            let ignored = cheatcodes
                .as_mut()
                .map(|cheatcodes| {
                    let mut ignored = std::mem::take(&mut cheatcodes.ignored_traces.ignored);

                    // If the last pause call was not resumed, ignore the rest of the trace
                    if let Some(last_pause_call) = cheatcodes.ignored_traces.last_pause_call {
                        ignored.insert(last_pause_call, (arena.nodes().len(), 0));
                    }

                    ignored
                })
                .unwrap_or_default();

            SparsedTraceArena { arena, ignored, diagnostics: trace_diagnostics }
        });

        let (edge_coverage, evm_cmp_values) = edge_coverage
            .map(|edge_coverage| {
                let (hitcount, cmp_values) = edge_coverage.into_parts();
                (Some(hitcount), (!cmp_values.is_empty()).then_some(cmp_values))
            })
            .unwrap_or_default();

        InspectorData {
            logs: log_collector.and_then(|logs| logs.into_captured_logs()).unwrap_or_default(),
            labels: {
                let labels = cheatcodes.as_ref().map(|c| c.labels.clone()).unwrap_or_default();
                /* EVM2 migration: disabled non-Ethereum execution.
                // if let Some(tempo_labels) = tempo_labels {
                //                     labels.extend(tempo_labels.labels);
                                }
                */
                labels
            },
            traces,
            line_coverage: line_coverage.map(|line_coverage| line_coverage.finish()),
            edge_coverage,
            evm_cmp_values,
            cheatcodes,
            chisel_state: chisel_state.and_then(|state| state.state),
            reverter,
        }
    }
}

impl<FEN: FoundryEvmNetwork> Deref for InspectorStack<FEN> {
    type Target = InspectorStackInner;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<FEN: FoundryEvmNetwork> DerefMut for InspectorStack<FEN> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

#[cfg(test)]
impl EarlyExitTestGate {
    pub(crate) fn check_step(&self, pc: usize) {
        if pc == self.target_pc && !self.notified.swap(true, std::sync::atomic::Ordering::SeqCst) {
            self.entered.send(()).unwrap();
            self.release.lock().unwrap().recv().unwrap();
        }
    }
}
