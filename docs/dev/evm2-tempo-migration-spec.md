# evm2 migration: Tempo contracts and evidence

Updated 2026-09-18. Extends the [shared transition specification](evm2-migration-spec.md)
using its S/T/G/V evidence definitions. Ethereum remains the first migration gate;
this assessment does not claim Tempo implementation readiness or change that order.
This records the pre-migration REVM baseline at the pin below. Those runs do not
certify Tempo on the current native Executor, which explicitly accepts Ethereum only.

## Pins and scope

- Foundry: `6f11b0156c1b3caa95215b7ff446d8c27d6a0506`, plus these regression tests.
- Imported Tempo: `b571bf6edc2988899741a2aae3f02d3f98cebe94` in Cargo.lock;
  `tempo-evm`, `tempo-revm`, `tempo-precompiles` are 1.14.0.
- Local evm2: `2c8b67f03fb0c86c0c2502d840d2e529bdf4b1a8`.
- Upstream port: [klkvr/evm2](https://github.com/tempoxyz/tempo/tree/klkvr/evm2),
  live head `83fdedb70e62642cd2a753cb55b86d1ba7f2e740` inspected on 2026-09-18.
  It pins evm2 `3e4e0ada354143143d560e2428acfdd0d2fc8db7`, different from
  our local evm2 pin. The local `investigation/evm2` ref is also different and
  was not substituted for the supplied branch.

### Upstream port assessment and sequencing

**S only:** inspected the pinned branch's Cargo manifests, execution factory,
handler/configuration, context, exports and engine integration. The port provides
native `evm2::Evm<TempoEvmTypes>`, a transaction registry, native precompiles sharing
`non_creditable_slots`, protocol fee-manager integration, and AA user-call batch
checkpoints. Its batch rollback explicitly excludes earlier inline key authorization.
The EVM test source includes storage-credit settlement and reverted-scope cases;
these upstream tests were not executed in this assessment.

Pinned evidence: [factory](https://github.com/tempoxyz/tempo/blob/83fdedb70e62642cd2a753cb55b86d1ba7f2e740/crates/evm/src/evm.rs),
[handler](https://github.com/tempoxyz/tempo/blob/83fdedb70e62642cd2a753cb55b86d1ba7f2e740/crates/evm/src/handler.rs),
[dependencies](https://github.com/tempoxyz/tempo/blob/83fdedb70e62642cd2a753cb55b86d1ba7f2e740/Cargo.toml).

This is substantial reusable protocol work, not a drop-in Foundry adapter. The
inspected factory targets the branch's Reth/evm2 interfaces; it does not demonstrate
Foundry snapshot/fork replacement with suspended frames, storage-hook injection,
or cheatcode rollback parity. Align dependency revisions and run TT-08–10 plus
the shared contracts before claiming that integration is practical.

**Decision:** defer Tempo implementation from the Ethereum migration gate, as
explicitly permitted by the user. Preserve the Tempo baseline/tests for the later
phase; do not require its upstream port or merge timing to unblock Ethereum.
No Tempo removal, dependency switch, or upstream build was performed here.

Tempo is a concrete, unconditionally available execution family in this checkout.
Its custom transaction/block types, native stateful precompiles and accounting are
additional obligations atop the shared contracts. Anvil is used as local test
infrastructure here, not added to migration acceptance scope. Wallet/session UX,
full node consensus, and the entire upstream protocol suite are not audited here.

## Transition contracts

| ID | Transition and current behavior | Evidence and remaining boundary |
| --- | --- | --- |
| TP-01 | Construct a concrete Tempo EVM → install Tempo gas parameters, enforce chain ID, supply hardfork tx cap if absent, extend native precompiles while preserving shared non-creditable slots. | **S:** [factory]. Construction must not accidentally use Ethereum handlers/costs. TT-01/02/09 execute that factory; no complete cap/error matrix yet. |
| TP-02 | Local construction → idempotent test genesis; fork construction → sentinel code without reminting genesis balances. | **S:** [genesis] skips initialization once PathUSD exists. Local setup creates four tokens and standard deployment contracts. Sentinels for future precompile addresses do not imply current activation. TT-08 observes local token state; TT-10 preserves remote token balances. Partial initialization, RPC faults, and every genesis contract remain gaps. |
| TP-03 | Native TIP20 approve/transferFrom → snapshot restore while ancestors are suspended → resume → ancestor success/revert/halt. | **T/V:** TT-08 asserts EVM storage and native balances/allowances together, exact transfer diagnostics and later native calls. Native storage cannot live in an unjournaled side cache. Does not cover every precompile or canonical receipt log behavior. |
| TP-04 | Child selects A→B→A → ancestor settles → inspect both forks. | **T/V:** TT-10 proves active-A token rollback and retained source-B publication in this sequence, including persistent parent/child. It is not a global multi-fork atomicity guarantee. |
| TP-05 | AA batch writes, then reverts → fee/nonce settlement; invalid nonce → admission rejection; next valid tx → normal execution. | **T/V:** TT-09 asserts exact `deadbeef`, rolled-back application storage, nonce 1, TIP20 fee debit, unchanged native balance, exact NonceTooHigh classification, no extra admission fee/nonce, then nonce 2/write success. Tests zero nonce-key, primitive signatures and one fee token; not sponsored/keychain/batched-create parity. |
| TP-06 | Keychain spending-limit clear → fee refund recreates slot → cancel same-transaction storage credit. | **T/V:** TT-02 checks zero final AccountKeychain storage credits at T7. Shared `non_creditable_slots` must survive precompile re-extension. One targeted invariant, not complete credit-mode/refund-cap coverage. |
| TP-07 | Select hardfork / invoke `setEvmVersion` → observable spec, gas table, native selectors and decoder activation. | **S/T:** [env], [cheats], TT-03/04. Reported names alone do not prove coordinated transition. See TG-02; fixed-start-hardfork tests do not test live switching. |
| TP-08 | Typed/RPC transaction → Foundry environment → script simulation/broadcast or replay. | **S/T:** [env] setters also update the first AA call's target/value/input. TT-03 checks only one call. Script execution is synthetic and distinct from signed transaction fee processing (TT-05/06). Permissive RPC conversion is materially weaker than a full typed AA decode; see TG-03. |
| TP-09 | Native call → trace label/ABI/debugger observation without changing execution. | **S/T:** [labels] reads token names through DB, capped at 256 bytes, falls back to TIP20 and caches per address. TT-07 covers ABI/activation and actual debugger display. No full proof of label freshness across fork replacement or observer gas neutrality for native calls. |

## Assertion-audited test ledger

All rows distinguish inspected assertions from runtime scope below. A passing
wrapper is not a claim of all protocol behavior or all hardfork-specific features.

| ID | Sources / tests | What is actually asserted |
| --- | --- | --- |
| TT-01 | [core tests]: `nested_tempo_execution_preserves_halt_reason` | T11 nested `transact_raw`: invalid opcode and stack underflow preserve distinct HaltReason values. Not suspended-parent rollback or exact gas. |
| TT-02 | [core tests]: `foundry_factory_keychain_limit_refund_does_not_leak_storage_credit` | T7 authorization and keychain spend succeed; AccountKeychain credit balance is zero after refund slot recreation. |
| TT-03 | [env]: `tempo_tx_env_setters_update_aa_call_payload`, `from_any_rpc_transaction_for_tempo_aa`, `from_any_rpc_transaction_for_tempo_eth_envelope` | First-call setter synchronization and selected envelope fields. AA test constructs nonce-key and validity metadata but does not assert their preservation. |
| TT-04 | [spec CLI]: `test_set_evm_version_tempo_hardfork`, `test_network_tempo_defaults_to_latest_hardfork`, `test_tempo_implicit_approval_t5`; [approval fixture] | Name changes/default selection; seven T5 Solidity tests cover implicit approval list, registry agreement, explicit approval/allowance/balances, DEX pull without approve, nonimplicit rejection and positive assume. Pre-T5 cheatcode check does not rebuild/test the precompile registry. Event checks mostly require signature presence, not full payload. |
| TT-05 | [scripts]: `tempo_script_runs_with_zero_fee_token_balance` | Constructor/setup/run execute despite clearing caller and fee-manager token balances, with configured nonzero gas price. Does not prove real signed transactions can omit fee funding. |
| TT-06 | [scripts]: `tempo_aa_script_broadcast_deploys_with_fee_token`, `tempo_aa_script_broadcasts_with_local_sponsor`, `script_batch_rewrites_creates_to_create2`, `script_check_contract_sizes_uses_network_specific_spec` | Broadcast success and four artifacts with selected AlphaUSD; sponsor/fee-unit output; dry-run creates rewritten to unique CREATE2 addresses; network-specific size warning. Sponsor test does not compare exact fee balances; batch rewrite test is not an on-chain atomic batch proof. |
| TT-07 | [trace decoder], [trace library], [CLI harness]: `debug_dump_marks_tempo_precompile_call_steps`; [debug CLI]: `debugger_selects_once_across_network_passes` | Synthetic ABI/event/error decoding and activation/label ownership, real FeeManager debug line, single interactive selection across network passes. Full trace library run includes T5/T6/T7 tests whose names do not contain `tempo`. Consumer tests do not prove engine step timing. |
| TT-08 | [CLI harness]: `tempo_suspended_snapshot_settlement`; [native snapshot fixture] | 18 hardforks × 2 isolation modes × 3 outcomes = 108 Solidity cases. Exact native balances/allowances, nonzero EVM storage rollback, pre/post-restore transfer amounts 50/25, continuation event before INVALID, later native execution. Explicitly asserts each requested hardfork's reported name. |
| TT-09 | [core tests]: `tempo_batch_revert_settles_fees_and_nonce_without_publishing_call_state` | One Rust test loops all 18 hardforks; each runs revert, invalid nonce, success. At 10^12 attodollars/gas, sender token debit equals `tx_gas_used()` in token base units, checked independently of the fee-conversion helper. No arbitrary-price rounding or fee-manager conservation proof. |
| TT-10 | [CLI harness]: `fork_tempo_suspended_native_settlement`; [native fork fixture] | T11, two funded local Tempo nodes, both isolation modes. Parent success/revert: A owner 889/1000 and recipient 111/0; B owner 778/recipient 222 in both outcomes; later transfer succeeds. Four Solidity cases; excludes fork-switch halt/transient/credit-state combinations. |
| TT-11 | [shared fixtures], [gas regions], [last gas] | T11 shared snapshot/hooks/metering plus six prior suspended-snapshot cases: 38 passed. Six state-diff observer paired-gas tests passed. Gas-region sweep: 21 passed, 2 failed; see the discrepancy record below. |

The hardfork matrix uses `TempoHardfork::VARIANTS`: Genesis, T0, T1, T1A, T1B,
T1C, T2–T13 at this pin. This covers the selected transitions on all exposed
versions; it does not claim all features introduced by those versions are covered
or activated on live chains. Latest-active selection is clock/chain dependent.

## Reproducible baseline discrepancy: gas regions

Two existing Ethereum comparison fixtures fail when run under Tempo:

- `testGasComparisonInternalCold`: snapshot 254300 versus Solidity measurement
  252306, difference 1994 beyond its tolerance 6.
- `testGasComparisonExternalRefund`: snapshot 3572 versus Solidity measurement
  772. The fixture explicitly disables isolation and asserts gross-gas equality.

Both pass at Ethereum Osaka, Tempo Genesis/T0/T1/T5/T6. Both fail at Tempo
T7/T11/T13; T11 also fails with `--no-isolate`. This locates an observed boundary
between T6 and T7, but does not establish the root cause. T7 adds storage-credit
behavior; the Solidity comparator itself writes storage (`cachedGas`) and uses
a fixed overhead. Neither “snapshot bug” nor “invalid fixture assumption” is yet
proven. Do not bless the discrepancy as intended behavior or weaken assertions.

Reproduce with the rebuilt Forge binary (`FORGE_BIN` below):

```sh
"$FORGE_BIN" test --root testdata --hardfork tempo:T7 --offline -vv \
  --match-path 'paris/cheats/GasSnapshots.t.sol' \
  --match-test 'testGasComparison(InternalCold|ExternalRefund)'
```

## Remaining migration decisions and tests

| ID | Actionable gap |
| --- | --- |
| TG-01 | Isolate the two gas discrepancies with equal initial storage-credit/warmth state and a comparator that does not modify storage inside the measurement. Determine intended behavior before selecting a migration oracle. Expand exact fees to arbitrary prices/rounding, caps, credit modes and native precompile OOG. |
| TG-02 | Characterize live hardfork changes plus snapshot/fork restore: cfg, Tempo gas parameters and captured precompile config must be checked separately. The generic `FoundryCfg` path calls `set_spec_and_mainnet_gas_params`; construction separately installs `tempo_gas_params`. This is a source-observed risk, not a proven end-to-end bug from name-only tests. |
| TG-03 | Build typed versus permissive-RPC replay comparisons for multiple AA calls, nonce-key, validity, fee payer and key authorization. `FromAnyRpcTransaction for TempoTxEnv` currently constructs base fields plus fee token and defaults the remaining fields. Existing tests do not establish complete AA preservation; decide compatibility treatment before freezing this conversion. |
| TG-04 | Exercise keychain/revocation/spending limits, fee sponsorship, failed native calls, storage-credit bookkeeping and native logs across suspended-frame replacement. TT-08/10 cover TIP20 balances/allowances, not all auxiliary state. |
| TG-05 | Establish native observer neutrality and label freshness across rollback/forks; coverage, fuzz/invariant reset/shrink/replay, Cast replay, Chisel and verification remain incomplete. Do not infer these from successful script broadcast or ordinary opcode fixtures. |
| TG-06 | Port located and source-screened above. In the deferred phase, align evm2/Tempo revisions and map journal/storage provider, precompile environment, transaction handler and credit bookkeeping to TP-01–09; build and execute Foundry parity tests. Native transaction processing alone does not establish Foundry cheatcode parity. |

## Execution record

All commands used the rebuilt baseline Forge from the shared specification:
`/Volumes/Stockage/dev-cache/cargo-targets/shell-eec7845ad4615eeabefc/debug/forge`.
Cargo defaults include `jemalloc,asm-keccak,optimism`; no network gates removed.
The three existing spec CLI tests pin Solc 0.8.26; the new harnesses use 0.8.35.
Cargo commands used
`SVM_RELEASES_LIST_JSON=/Volumes/Stockage/dev-cache/solc-releases-e4b80d33-aarch64.json`.

| Command / selection | Result |
| --- | --- |
| `cargo +nightly test -p foundry-evm-core --test tempo -- --nocapture` | 3 passed, none failed/ignored/filtered; includes TT-09's 18-version loop. The original two tests also passed before adding it. |
| `cargo +nightly test -p foundry-evm-core -p foundry-evm-hardforks -p foundry-evm-networks -p foundry-evm-traces -p foundry-evm --lib tempo -- --nocapture` | Per target: core 8, hardforks 3, networks 6, traces 10, evm 2 passed; filtered counts 86/9/39/84/196. Includes an unrelated `temporary_backend` match; 29 is not a Tempo coverage denominator. |
| `cargo +nightly test -p foundry-evm-traces --lib -- --nocapture` | 94 passed, no filtered/ignored; includes the 10 trace tests above, not 94 additional distinct Tempo tests. |
| Ten existing CLI filters listed below | 1 passed each, no ignored; 1569 filtered per invocation before adding the two new harnesses. |
| `cargo +nightly test -p forge --test cli tempo_suspended -- --nocapture` | 2 Rust harnesses passed, 1570 filtered; assert 108 snapshot + 4 fork Solidity cases, none skipped. Solc 0.8.35; runtime hardfork selected explicitly. |
| Shared fixtures at T11 | 38 passed, none skipped; includes 6 previously added Ethereum suspended-snapshot cases. Solc 0.8.37, fixture compiler configuration retained. |
| T11 `GasSnapshots.t.sol` | 21 passed / 2 failed / 0 skipped; boundary runs above retained the same assertions. Paris compilation, Tempo runtime. |
| T11 `LastCallGas.t.sol`, `--match-test testStateDiffRecordingDoesNotWarm` | 6 passed / 0 failed / 0 skipped, inline isolation enabled. |

Ten CLI names (each invoked on the built test executable with `--exact --nocapture`;
equivalent reproduction: `cargo +nightly test -p forge --test cli NAME -- --exact --nocapture`):

```text
test_cmd::spec::test_set_evm_version_tempo_hardfork
test_cmd::spec::test_network_tempo_defaults_to_latest_hardfork
test_cmd::spec::test_tempo_implicit_approval_t5
test_cmd::debug_dump_marks_tempo_precompile_call_steps
debug::debugger_selects_once_across_network_passes
script::tempo_script_runs_with_zero_fee_token_balance
script::tempo_aa_script_broadcast_deploys_with_fee_token
script::tempo_aa_script_broadcasts_with_local_sponsor
script::script_batch_rewrites_creates_to_create2
script::script_check_contract_sizes_uses_network_specific_spec
```

Shared fixture invocation:

```sh
"$FORGE_BIN" test --root testdata --network tempo --hardfork tempo:T11 --offline -vv \
  --match-path 'default/cheats/{SuspendedSnapshot,StateSnapshots,MappingStorageHooks,GasMetering}.t.sol'
```

The T11 gas sweep uses the same flags with `--match-path
'paris/cheats/GasSnapshots.t.sol'`; observer checks use `LastCallGas.t.sol` and
the filter in the table. Shared default isolation applies except fixture overrides;
the existing fuzz entry ran 256 cases without a pinned seed. New tests are deterministic.

Limits and failed attempts:

- Sandboxed unit discovery tests initially failed local socket/DNS access; all
  passed when retried with access. The existing live Moderato probe is not hermetic.
- The first AA test draft's 500000 gas budget halted before reaching the intended
  revert. It now funds 6000000 token base units and uses a 2000000 gas limit;
  exact `deadbeef` ensures the expected path is reached. No production fix applied.
- Ignored live Moderato batch test, project-template installation test, full Cast
  suite and protocol-wide upstream tests were not run. Broad Ethereum gas constants
  are not assumed to be Tempo constants.
- Formatting and strict all-feature/all-target Clippy passed. Baseline behavioral
  status is **not fully green** because the two gas comparisons remain unresolved.

[factory]: ../../crates/evm/core/src/evm/tempo.rs
[genesis]: ../../crates/evm/core/src/tempo.rs
[env]: ../../crates/evm/core/src/env.rs
[cheats]: ../../crates/cheatcodes/src/tempo.rs
[labels]: ../../crates/evm/evm/src/inspectors/tempo_labels.rs
[core tests]: ../../crates/evm/core/tests/tempo.rs
[spec CLI]: ../../crates/forge/tests/cli/test_cmd/spec.rs
[approval fixture]: ../../crates/forge/tests/fixtures/TempoImplicitApproval.t.sol
[scripts]: ../../crates/forge/tests/cli/script.rs
[trace decoder]: ../../crates/evm/traces/src/decoder/mod.rs
[trace library]: ../../crates/evm/traces/src/lib.rs
[CLI harness]: ../../crates/forge/tests/cli/test_cmd/mod.rs
[debug CLI]: ../../crates/forge/tests/cli/debug.rs
[native snapshot fixture]: ../../crates/forge/tests/fixtures/TempoSuspendedState.t.sol
[native fork fixture]: ../../crates/forge/tests/fixtures/TempoSuspendedFork.t.sol
[shared fixtures]: ../../testdata/default/cheats
[gas regions]: ../../testdata/paris/cheats/GasSnapshots.t.sol
[last gas]: ../../testdata/paris/cheats/LastCallGas.t.sol
