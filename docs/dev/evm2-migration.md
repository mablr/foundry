# Ethereum-only evm2 migration

This working branch reduces the execution surface before replacing REVM with EVM2.
Ordinary executor calls and transactions now run natively through evm2.
The surrounding backend and result types still contain REVM compatibility adapters.

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
| Executor | Nonforked Ethereum calls, deployments, commits, cancellation | Isolation, replay, system calls and remaining envelope/configuration support |
| Environment | warp, coinbase, fee, prevrandao, block/chain getters | Prague+ roll history updates and remaining environment cheatcodes |
| State | load, store, nonce getters, ordinary etch | Direct balance/nonce mutation semantics; etch of history-storage account |
| Inspection | Logs, console and shared assertions, including non-reverting assertions | Traces, debugger, coverage, fuzz, invariants and opcode-interest selection |
| Sessions | Block updates, diagnostics, deprecation observations | Pranks, expectations, mocks, snapshot/fork lifecycle |
| Core ownership | Lazy backend reads and transaction-delta conversion | Native persistent backend/results; delete factory/context/REVM adapters |
| Tools | Bounded Forge E2E | Cast, Script and Chisel compatibility |

Unmigrated cheatcodes fail the execution even if Solidity catches their revert. Unknown
selectors retain Foundry's ordinary catchable error. No automatic fallback executes a
rejected transaction through REVM.

`deal` and nonce setters require deliberate compatibility work. On the pinned REVM
reference, a previously loaded/touched account changed through `deal` and `setNonceUnsafe`
inside a child retains those changes after the child reverts. Foundry mutates account
metadata directly and separately repairs deals before top-level rollback. evm2's ordinary
account setters record revert snapshots. Porting these cheatcodes as simple setter calls
would therefore change behavior. `MutationProbe.t.sol` preserves the reference regression;
it is intentionally excluded from candidate parity and timing runs until implemented.
Keep these setters unsupported until the nested and top-level cases have an explicit,
differentially tested implementation.

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
