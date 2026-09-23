# Ethereum-only evm2 migration

This working branch reduces the execution surface before replacing REVM with EVM2.
Ordinary executor calls and transactions now run natively through evm2.
The surrounding backend and execution configuration still contain REVM compatibility adapters.
Transaction state changes now own native evm2 account metadata.

`EthEvmNetwork` is the only compiled Foundry EVM network implementation. The existing
generic executor, backend, inspector, journal, and cheatcode interfaces remain so the
engine migration can be developed separately from this preparation change.

OP, Base, and Monad feature propagation and engine dependencies are commented out.
Their feature names are retained as empty switches, so `--all-features` cannot restore
the implementations. Network-specific files remain on disk with their module declarations
disabled. Scattered integration code and dependent tests are preserved in comments marked
`EVM2 migration: disabled non-Ethereum execution`.

Tempo's unconditional factory, environment adapters, genesis helpers, label inspector,
transaction fee-token propagation, and precompile-backed cheatcodes are disabled in the
shared execution stack. Forge, local Cast execution, scripts, Chisel, and bytecode
verification reject Tempo execution explicitly. RPC transaction types, signatures, ABI
metadata, and network identification remain where they do not depend on a foreign engine.

Anvil is outside the initial migration. Its Ethereum implementation remains available as
a test dependency. Foreign feature-gated paths are disabled along with the shared crates;
Tempo startup and the execution paths dependent on removed shared adapters reject execution.
Remaining Anvil-local Tempo code is not a supported execution configuration on this branch.

The normal dependency trees of `forge` and `foundry-evm`, including all features, must not
contain `tempo-revm`, `tempo-evm`, `tempo-precompiles`, `op-revm`, `alloy-op-evm`,
`monad-revm`, `alloy-monad-evm`, or `base-common-evm`. Anvil's legacy local dependencies
remain outside that boundary. Do not interpret permissive RPC decoding as support for
executing the decoded network's transactions.

The earlier `evm2_ethereum.rs` preparation tests include snapshots and pranks and are not
an acceptance suite for the current bounded native path. Native acceptance lives in
`crates/forge/tests/cli/test_cmd/evm2.rs`.

## Native capability checkpoint

Reference: Foundry `c756235f9`; engine: evm2 `d746ceea38e5b85f66a0b2bdf0b4f67e32d0e64c`.
Use the Rust toolchain required by evm2 (currently at least 1.96).

| Boundary | Native coverage | Remaining work |
| --- | --- | --- |
| Executor | Nonforked Ethereum calls, deployments, commits, cancellation, beacon-root calls and synthetic replay | Isolation, canonical replay and remaining envelope/configuration support |
| Environment | warp, coinbase, fee, prevrandao, block/chain getters | Prague+ roll history updates and remaining environment cheatcodes |
| State | load, store, deal, nonce setters/getters, ordinary etch | Etch of history-storage account |
| Inspection | Logs, console and shared assertions, including non-reverting assertions | Traces, debugger, coverage, fuzz, invariants and opcode-interest selection |
| Sessions | Block updates, diagnostics, deprecation observations | Pranks, expectations, mocks, snapshot/fork lifecycle |
| Core ownership | Native transaction state changes and nonforked persistent cache | Native fork backend; delete factory/context/REVM adapters |
| Tools | Bounded Forge test and local Script E2E | Broadcast, simulation, Cast and Chisel compatibility |

Unmigrated cheatcodes fail the execution even if Solidity catches their revert. Unknown
selectors retain Foundry's ordinary catchable error. No automatic fallback executes a
rejected transaction through REVM.

`deal`, `setNonce` and `setNonceUnsafe` use native account overrides. Child rollback
retains overrides while undoing earlier transfers and nonce bumps. Top-level failure
restores recorded deal balances; coordination with future expectRevert and isolation
support remains a TODO in the native inspector.

`setEvmVersion` is intentionally unsupported for now, including when its revert is caught.
Select a fixed hardfork before execution with `--evm-version`. No REVM fallback is used.

## Differential validation

Build the reference and candidate Forge executables separately, then run:

```sh
python3 crates/forge/tests/fixtures/evm2/compare.py \
  --reference /absolute/path/to/reference/forge \
  --candidate /absolute/path/to/candidate/forge \
  --output /tmp/evm2-differential.json
```

The runner uses independent temporary projects, records executable/fixture hashes, rejects
zero-test runs, and compares statuses, reasons, logs, gas, labels, and gas snapshots.
Scenarios cover ordinary lifecycle/state operations, console ordering, and both assertion
modes. This bounded suite is not full Foundry compatibility coverage.

## Initial timing experiment

Build both engines with `cargo build --locked --profile profiling -p forge --bin forge`.
The `evm2_migration` example invokes the existing `foundry-bench` warm-cache runner on the
same bounded fixture. Put each selected profiling Forge binary on PATH and run the example
separately for the reference and candidate, passing a version label and JSON output path:

```sh
cargo run -p foundry-bench --example evm2_migration -- reference /tmp/reference.json
cargo run -p foundry-bench --example evm2_migration -- candidate /tmp/candidate.json
```

Use the same runner executable for both timings after all builds finish. These small-fixture
measurements establish a baseline only. Real-project gains remain a release gate once the
required cheatcodes and inspectors are migrated.

### Local checkpoint results

The profiling binaries matched across 22 differential executions, including gas, logs,
statuses, and failure reasons. Five native Forge CLI tests, three executor tests and the
restricted-cheatcode unit test passed. Nightly formatting and strict workspace Clippy
(`--all --all-features --all-targets -- -D warnings`) passed.

On this machine with Rust 1.97.1, the warm-cache runner measured two batches of 30 samples
per engine and workload, ordered reference/candidate/candidate/reference. No compiler
activity was detected during the measurements. Times include the Forge command lifecycle.

| Workload | REVM mean | evm2 mean | Wall-time change |
| --- | ---: | ---: | ---: |
| 50,000 scratch-memory hashes | 52.75 ms | 52.91 ms | +0.3% |
| 1,000 cheatcode storage reads | 53.91 ms | 55.08 ms | +2.2% |
| Both tests in one invocation | 68.52 ms | 64.88 ms | -5.3% |

The computation-only result is effectively neutral; storage reads show no gain and varied
between batches. The combined invocation improved in both candidate batches, but this
small suite includes scheduling and startup costs and does not establish a general engine
speedup. Gas matched exactly: 5,100,174 for computation and 484,054 for cheatcode reads.
A real-project performance claim remains unvalidated.

The example accepts an optional final `compute`, `cheatcodes`, or `both` argument to
reproduce each workload. Raw benchmark samples and differential reports are generated
locally at the output paths supplied to the runners; they are not checked into this branch.

## Native executor follow-up

The minimal local `forge script --no-isolate` path now deploys contracts, executes
nested calls and returns values through evm2. Script address protection also uses a
native opcode hook. Trace production/debugging is temporarily unavailable; broadcast
cheatcodes, fork simulation and isolation are still pending. This is not a REVM-free
binary yet: configuration, fork and inspector interfaces retain legacy types.

The executor no longer directly depends on alloy-evm. Beacon-root calls use evm2's
system-call API, and the ordinary replay loop uses native transaction execution.
Canonical envelopes and BAL replay still require migration; unsupported inputs fail
rather than invoking a REVM fallback. Native Rust sancov collection wraps evm2 execution.

Transaction state changes carry evm2 account metadata and owned storage deltas through
executor and fuzz consumers. Nonforked persistent state now lives in an evm2 cache,
with native account/code reads and commits. Temporary REVM database traits adapt
unmigrated callers; fork persistence still uses the legacy cache. Fork transfer and fuzz
dictionary initialization temporarily export a legacy snapshot instead of maintaining
a second persistent database.
Anvil retains its separate REVM state type while its migration is deferred.

Execution outcomes now retain evm2 termination statuses and Foundry-owned call/create
outputs through script, fuzz and failure-reporting consumers. The partial conversion
into REVM instruction results is gone; native halts such as `OpcodeNotFound` reach
error reporting without a migration-adapter failure. Legacy trace decoding retains
its separate REVM status entry point until inspectors migrate.

Foundry now owns its execution-environment container and selects block, transaction
and hardfork types directly through the network interface. The legacy factory
association remains only as a migration constraint for fork/inspector contexts; the
configuration fields still use REVM types and must be replaced. Backend initialization
reads evm2 precompile addresses directly rather than constructing a REVM instance.
Anvil converts at shared normalization boundaries and retains its legacy environment.
