# Foundry → evm2 working document

Updated: 2026-09-18. Status: challenged transition specification; implementation has not started.

Keep this document concise and current as decisions and evidence change. Replace
superseded conclusions rather than accumulating a conversation log. Separate
agreed scope, observed behavior, proposals, and unresolved questions. The
[transition specification and test ledger](docs/dev/evm2-migration-spec.md) contain
the detailed contracts, evidence limits, and coverage gaps.
The [Tempo assessment](docs/dev/evm2-tempo-migration-spec.md) records its additional
native-state, AA settlement, hardfork and precompile obligations.

## Agreed scope

- Replace Foundry's REVM/alloy-evm execution integration with evm2. Target a
  one-shot migration without maintaining both engines for migrated workflows.
- Full Ethereum support is the acceptance scope, centered on `foundry-evm` and
  `foundry-cheatcodes` behavior. Track downstream Forge, scripts, Cast, Chisel,
  verification, tracing/debugging, coverage, and fuzz/invariant consumers.
- Defer Tempo implementation until after the Ethereum gate. Its upstream
  [evm2 port](https://github.com/tempoxyz/tempo/tree/klkvr/evm2) is identified and
  source-screened; align dependency revisions and establish Foundry parity before
  integrating it. Retain Tempo tests; its upstream progress must not block Ethereum.
- OP/Base/Monad support is deferred. Their feature-gated integration code may be
  trimmed from the migration branch to reduce context and implementation burden;
  revisit support after validating the MVP.
- Anvil is outside current success criteria. Temporary breakage, including shared
  crate fallout, must not block progress on execution and cheatcodes.
- Celo is best-effort; breakage is acceptable during migration.
- Keep the existing network feature gates during baseline validation and regression
  work; defer trimming until it helps integration translation.
- Current work: baseline execution and suspended-parent regression tests. No
  engine migration or network trimming has been performed.

## Main findings and risks

| Area | Behavior or coupling to preserve / resolve |
| --- | --- |
| Integration boundary | `FoundryEvmFactory`, `FoundryContextExt`, `DatabaseExt`, and `NestedEvm` expose REVM contexts, journals, frames, and result types. Replacing the factory alone is insufficient. |
| Live state | Snapshots and fork selection replace execution state while parent frames remain suspended. Preserve checkpoint meaning, transient storage, warmth, persistent accounts, environments, and subsequent rollback. Snapshot restoration deliberately retains diagnostic logs and assertion-failure evidence. |
| Nested execution | Distinguish inherited frames, synthetic isolated transactions, `executeTransaction`, and historical replay. They have different inheritance, commit, and failure boundaries. |
| Isolation | Child storage originals/warmth differ from the parent's. Settlement preserves parent originals and merges current values. Outer rollback also compensates for mutations outside ordinary journal accounting. |
| Inspectors | Cheatcodes control execution: mutate gas/stack/outcomes, inject storage-hook callbacks, and redirect CREATE2 frames. Callback order, short circuits, parent resumption, and result rewriting are behavioral contracts. |
| Gas and environment | Preserve intrinsic gas, refunds, regular/state gas, reservoirs, nonce accounting, and contract-visible environment overrides separately from synthetic transaction settlement. |
| State ownership | `DatabaseExt` mixes storage, fork lifecycle, snapshots, replay, and execution. Explore separating responsibilities by owner; a larger generic abstraction is not an agreed design. |
| Consumers | Trace/debugger types, coverage, fuzz state observation/reset, invariant replay/shrinking, and symbolic execution adapters also depend on REVM. Script execution and broadcast simulation remain separate phases. |
| Performance | Avoid adding full-state conversions or extra clones at transaction/nested-call boundaries. Measure real Foundry workloads, including reset and inspector costs; no performance benefit has been established. |

## evm2 observations and open questions

The inspected checkout already provides mutable interpreter hooks, transaction
commit/discard/detach, live block-environment replacement, and `evm2-inspectors`.
These are useful foundations, not demonstrated Foundry parity.

- `PendingState` is finalized transaction state, not a complete suspended execution
  snapshot. Reattachment does not restore journal history, logs, or transient
  storage. How should live snapshot/fork replacement preserve parent checkpoints?
- How should storage-hook frame injection and CREATE2 frame rewriting map to evm2's
  message lifecycle? Its inspected API has no direct `frame_start`/`frame_end`
  equivalent to the current Foundry contract.
- How should live transaction/configuration mutation work? Block mutation is
  supported; interpreter transaction environments are borrowed immutably, and
  execution-config replacement is guarded during execution.
- Which state can remain native to evm2 across execution, speculative calls,
  worker cloning/reset, snapshots, and fork switching? Audit `foundry-fork-db`
  separately from the execution journal.
- What does Tempo's prior integration actually cover: transaction processing,
  precompiles, gas rules, and Foundry-specific nested execution/cheatcodes?
- Enumerate the full existing Ethereum workflow/hardfork matrix. Sequence validation
  without reducing compatibility; choose live-state mechanisms from an experiment.

## Specification progress and next work

The first specification covers execution, inherited/isolated children, signed
transactions, replay, snapshots/forks, inspector-controlled frames, gas,
environment, and reporting. It now links 28 source-audited test groups across
17 contracts and defines seven priority cross-boundary test obligations.
Baseline runs now include 265 existing Solidity test executions across isolation
configurations, nine existing CLI tests, and ten new suspended-parent cases
(six snapshot cases plus four local-fork cases). The earlier 133 Rust tests also
passed. G-01/G-03 have executable partial coverage, not closure; no evm2 parity
or exhaustive coverage is claimed.

Tempo now has 108 native-token snapshot cases across all 18 exposed hardforks,
four local native-token fork cases, and an AA settlement sequence at every
hardfork. Selected existing unit/CLI/shared-fixture tests passed. Two gas-region
comparisons fail at T7/T11/T13 and pass at T6; the cause is unresolved. Complete AA
RPC replay and live hardfork/precompile/gas coherence remain explicit risks.

Important refinements: cheatcode and lower-level fork-database snapshots have
different invalidation rules; account and slot warmth merge differently; snapshot
restoration captures cfg/block plus selected tx overrides, not arbitrary `TxEnv`.
Replay publication guarantees also differ by entry point. Source-fork dump tests
reject a blanket rollback rule. Gas fixtures distinguish exact, approximate and
positivity assertions; the default last-call-gas contract is excluded from the
main fixture harness. Synthetic debugger tests do not prove step production.

For each operation record: trigger, state owners, preconditions, observable
effects, success settlement, failure settlement, suspended-frame effects, gas
effects, existing tests, and the proposed evm2 mechanism.

Proposed investigation order:

1. Prove live snapshot/fork replacement followed by parent resumption and later
   revert, including nested calls, warmth, and transient storage.
2. Specify inherited execution, isolation, fresh transaction execution, and replay
   separately; establish state, environment, nonce, and gas invariants.
3. Map inspector-controlled frames and callback ordering; preserve trace and
   failure reporting alongside execution results.
4. Expand the full Ethereum acceptance matrix and resolve the Tempo assessment gaps. Benchmark migrated
   paths against the baseline with identical inputs and inspector settings.

A narrow `cast run` port would not resolve the main Forge lifecycle questions.
An opcode-interest optimization is not a prerequisite for this migration.

## Evidence anchors

Source pins and principal anchors (runtime results are in the specification):

- Foundry: `6f11b0156c1b3caa95215b7ff446d8c27d6a0506`.
- Local evm2: `2c8b67f03fb0c86c0c2502d840d2e529bdf4b1a8`.
- [Execution interfaces and child-state helpers](crates/evm/core/src/evm/mod.rs),
  [context](crates/evm/core/src/env.rs),
  [backend](crates/evm/core/src/backend/mod.rs),
  [executor](crates/evm/evm/src/executors/mod.rs),
  [inspector stack](crates/evm/evm/src/inspectors/stack.rs), and
  [cheatcode inspector](crates/cheatcodes/src/inspector.rs).
- Existing test anchors: child-state/inheritance unit tests in the execution
  interfaces above; `StateSnapshots.t.sol`, `Fork.t.sol`, `Fork2.t.sol`,
  `ExecuteTransaction.t.sol`, `MappingStorageHooks.t.sol`, and `GasMetering.t.sol`
  under [cheatcode fixtures](testdata/default/cheats); Amsterdam state-gas tests
  in the executor; `fork_nested_broadcast_nonces` in
  [script CLI tests](crates/forge/tests/cli/script.rs).
- [Gas snapshot and last-frame fixtures](testdata/paris/cheats), trace requirements
  and serialization, debugger consumer and CLI dump tests, coverage LCOV/branch
  fixtures, and executor cancellation tests are mapped separately in the ledger.

Update source pins and validation evidence when revisiting these conclusions.
