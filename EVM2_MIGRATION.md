# Foundry → evm2 migration

Updated: 2026-09-18. **M1 passed its bounded gate; M2 partial; M3 not integrated.**
This is the current roadmap, not an Ethereum-parity claim.
Contracts/tests: [transition ledger](docs/dev/evm2-migration-spec.md).
Engine requests: [API gaps](docs/dev/evm2-upstream-requirements.md).
Deferred network: [Tempo assessment](docs/dev/evm2-tempo-migration-spec.md).

## Scope and execution boundary

- Replace REVM/alloy-evm for Foundry workflows, without a maintained dual-engine mode.
  Final acceptance includes full existing Ethereum hardforks, transactions, cheatcodes,
  forks, isolation, gas, scripts/replay, tracing/debugging, coverage and fuzz/invariants.
- Ethereum first; Tempo follows assessment. OP/Base/Monad remain gated and deferred;
  no network removal has been performed. Anvil breakage is acceptable during migration;
  Anvil is outside acceptance. Celo is best-effort.
- Ordinary Executor call/deploy paths use native evm2 with lazy backend reads and a
  transaction-delta bridge. Nested frames stay native. REVM dependencies, backend/result
  types and other replay/system execution paths remain. No performance gain is proven.
- Current runnable scope: `--no-isolate`, Ethereum synthetic zero-fee transactions,
  Merge–Osaka mapping; acceptance fixtures use Cancun. Unmigrated capabilities fail
  explicitly, including when Solidity catches an unsupported cheatcode.
- Foundry pins unpatched evm2 `2c8b67f0` (Rust >=1.96); workspace declares Rust 1.89.
  Validation uses nightly. Dependency/MSRV policy must be resolved before acceptance.

## Progress and exit criteria

| Milestone | Current state | Remaining exit criteria |
| --- | --- | --- |
| **1. Ordinary Forge execution** | **Bounded gate passed.** Constructor/setup persistence, independent tests, nested success/revert/halt, CREATE/CREATE2, value, STATICCALL and transient lifetime work. | Broader hardfork/transaction coverage belongs to M4; this is not general Forge parity. |
| **2. Cheatcodes and inspectors** | **Partial.** Assertions, selected environment/storage/code operations, revert expectations, console, sender/delegate pranks and caller introspection execute natively. | Origin-changing pranks; call/emit/create expectations; mocks; recordings/storage hooks; gas metering/snapshots; remaining mutations and utilities; callback/failure/cancellation regressions. Resolve E2-06–08 where needed. |
| **3. Live replacement and nested execution** | **Experiment only; no native Foundry integration.** Same-transaction engine patch exists separately. | Prove setup→test snapshot lifetime, companion/backend state, fork replacement, inherited execution and isolation. Test suspended ancestors, storage originals/warmth/transient state, rollback and gas. G-01/G-03 remain open. |
| **4. Full Ethereum and consumers** | **Not accepted.** Existing consumers and legacy interfaces remain. | All supported hardforks/tx types, replay, gas/state gas, tracing/debugger, coverage, fuzz/invariant reset/shrinking, scripts/Cast/Chisel, cancellation. Remove the temporary bridge and execution dependencies; run representative benchmarks. |
| **5. Tempo** | **Assessed, implementation deferred.** Prior upstream port inspected at a recorded pin. | Align dependencies; resolve baseline gas discrepancies; verify native-token/precompile, AA, rollback and observer contracts before integration. |

## Evidence at this checkpoint

- **38 positive Solidity cases** match preserved REVM reported gas: M1 lifecycle 7,
  initial cheatcodes 9, revert expectations 11, pranks 11. Cancun, Solc 0.8.35,
  optimizer enabled; these are gas observations, not performance measurements.
- Four expectation-failure cases and one legacy assertion also match reference
  diagnostics/gas. Eleven native CLI regressions and three executor unit tests pass.
- Earlier isolated runs passed two unchanged setup tests, twelve state/environment
  tests and two deterministic assertion tests. The full assertion suite and full
  behavioral suite are not certified. [Reproduction](crates/forge/tests/fixtures/evm2/README.md).
- Nightly formatting and strict all-feature/all-target workspace Clippy pass.

## Next execution order

1. Continue independent M2 work with call expectations/mocks and emit/log recording;
   test registration → nested execution → settlement → verification, including failures.
2. Turn E2-06 (account overrides), E2-07 (origin/cfg/tx mutation) and E2-08 (raw gas at
   outcome rewriting) into minimal engine/adapter acceptance probes and reviewable
   capability requests. Do not classify all remaining M2 implementation as API blockers.
3. Complete gas controls and storage-hook/callback coverage; rerun unchanged fixtures
   as capabilities become available. Close M2 against the transition ledger.
4. Review/adopt a compatible engine revision only after its lifecycle gates pass;
   establish cross-transaction snapshot semantics before enabling M3 workflows.

## Separate evm2 workstream

`~/sources/evm2`, branch `mablr/live-state-snapshots`, commit `04179271` (base `0a5314e`):
local checkpoint candidate, not pushed or adopted by Foundry. Recorded validation:
45/54 differential matches, zero candidate panics, 508 library tests and nine snapshot
cases; package Clippy/fmt/rustdoc passed. Nine known reference account-loading
mismatches remain. Workspace Clippy was blocked by missing LLVM 22.

The patch supports same-transaction snapshots with coordinated frame settlement.
Cross-transaction/custom-handler scopes, fork replacement, isolation, refund/state-gas
coverage and performance remain open. **45/54 is an experiment result, not Foundry
coverage.** [Historical evidence](experiments/evm2-live-state/README.md) retains provenance;
obsolete harness sources are already archived outside the active tree.
