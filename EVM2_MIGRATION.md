# Foundry → evm2 migration

Updated: 2026-09-18. **Milestone 1 passed; native cheatcode integration is next.**
This is the current roadmap. Detailed contracts and historical evidence belong in
[the transition/test ledger](docs/dev/evm2-migration-spec.md),
[evm2 requirements](docs/dev/evm2-upstream-requirements.md), and
[the Tempo assessment](docs/dev/evm2-tempo-migration-spec.md).

## Scope

- Replace REVM/alloy-evm for Foundry workflows; no maintained dual-engine mode.
  Final acceptance requires full existing Ethereum support, including cheatcodes,
  forks, isolation, gas, scripts/replay, tracing/debugging, coverage and fuzz/invariants.
- Ethereum first. Tempo implementation follows assessment and the Ethereum gate.
  OP/Base/Monad are deferred; gates remain, trimming is allowed where useful.
- Anvil is outside current acceptance; temporary shared-crate breakage is acceptable.
  Celo is best-effort. No network trimming has been performed in this checkpoint.
- Preserve existing tests and specify state transitions, not merely API mappings.
  Choose live replacement mechanisms from experiments; no performance gain is proven.

## Milestone 1: ordinary Forge execution

Normal Executor call/deploy paths now use evm2 directly. Lazy database reads and a
transaction-delta bridge retain the existing Backend/RawCallResult boundary; nested
frames execute entirely with native evm2 state. Other replay/system entry points
still use REVM. This is an intermediate branch, not a release-ready replacement.

**Verified:** seven lifecycle cases (constructor/setup persistence, independent test
state, nested CALL success/revert/halt, CREATE/CREATE2, value, STATICCALL, transient
lifetime); both unchanged `SetupConsistency.t.sol` tests; three executor unit tests;
three CLI regressions covering success, failure reasons and caught unsupported
cheatcodes. Seven fixture gas values matched the preserved REVM baseline.
Nightly formatting and strict workspace Clippy with all features/targets passed.
The full behavioral suite is not green-certified.
[Fixture and reproduction](crates/forge/tests/fixtures/evm2/README.md).

**Boundary:** use `--no-isolate`. Only ordinary zero-fee synthetic transactions are
wired, with Merge–Osaka spec mapping; acceptance fixtures ran on Cancun. Forks,
cheatcodes/console, tracing and advanced inspector modes fail explicitly. REVM
interfaces/dependencies remain. evm2 is pinned to `2c8b67f0`; it requires Rust >=1.96,
while the workspace still declares 1.89. Validation used nightly; MSRV policy remains
an integration decision.

## Next milestones

1. **Cheatcodes and inspectors:** port the native context/callback boundary, starting
   with assertions, environment and ordinary state mutations. Preserve callback
   ordering, gas and outcome rewriting, diagnostic logs and cancellation. Extend
   real Forge tests as each capability becomes available.
2. **Live replacement and nested execution:** review the evm2 checkpoint fix; prove
   setup/test snapshot lifetime, fork/backend replacement, inherited execution,
   isolation and fresh transactions separately before enabling those operations.
   Preserve storage originals/current values, warmth, transient state, environment
   and ancestor rollback. G-01/G-03 remain open.
3. **Complete Ethereum acceptance:** cover every existing hardfork/transaction type,
   replay, regular/state gas, traces/debug steps, coverage, fuzz/invariant reset and
   shrinking, scripts/Cast/Chisel and other consumers. Remove the temporary bridge
   and remaining execution dependencies; benchmark representative workloads.
4. **Tempo:** resolve the documented gas-region discrepancies and native-token/AA
   lifecycle obligations before integrating its upstream evm2 work.

## evm2 workstream

The isolated checkpoint prototype achieved 45/54 differential matches with no
candidate panics, retained all previous matches, and passed 506 engine tests plus
three divergent-history comparisons. Nine known reference account-loading
mismatches remain. This proves a candidate mechanism, not Forge snapshot parity.

A PR-ready local branch is being prepared in `~/sources/evm2`; it is **not** part of
Foundry's pinned dependency. Cross-transaction snapshots, custom-handler raw
checkpoints, forks/isolation, refunds/state gas and performance remain unvalidated
or unsupported. The [requirements document](docs/dev/evm2-upstream-requirements.md)
tracks the exact boundary. Historical harness sources are archived outside Foundry;
[retained evidence](experiments/evm2-live-state/README.md) records their provenance.
