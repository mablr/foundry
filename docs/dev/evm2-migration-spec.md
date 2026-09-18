# evm2 migration: behavioral contracts and evidence

Status: challenged source audit, 2026-09-18; baseline and initial live-frame regressions executed. Scope and decisions live
in [the working document](../../EVM2_MIGRATION.md). Ethereum is the first gate;
Tempo's [additional assessment and tests](evm2-tempo-migration-spec.md) track native
state and AA accounting, including unresolved baseline gas discrepancies.
OP/Base/Monad are deferred; Anvil/Celo breakage is not a
blocker. This document specifies migration obligations, not a new generic engine
architecture or a claim that evm2 already satisfies them.

Baseline: Foundry `6f11b0156c1b3caa95215b7ff446d8c27d6a0506`; local evm2
`2c8b67f03fb0c86c0c2502d840d2e529bdf4b1a8`. Refresh evidence after source changes.

## 1. How to use this specification

Contract IDs are stable. Preserve observable behavior unless an explicit decision
records a deliberate change, its rationale, and affected tests. Internal REVM
representations may change. Do not turn suspicious implementation details into
new public guarantees without a reproducer and a decision.

Evidence labels:

- **S**: implementation inspected; describes current behavior, not runtime proof.
- **T**: named test assertions inspected; covers only the assertions listed.
- **G**: test obligation remains open; a suitable existing test may still exist.
- **V**: executed evidence, requiring revision, exact command, configuration,
  nonzero test count, result, and environment limits.

Runtime evidence is recorded in sections 8–9; unlisted tests remain unexecuted for
this audit. Formatting/Clippy success is not execution parity. A Solidity fixture's existence does not prove
that a particular Cargo test command executes it. Broad areas not yet audited
are explicitly listed in section 7.

For each new transition refine: trigger/preconditions → input state → changes
visible while running → success settlement → revert/halt/host-error settlement
→ parent resumption → observations/tests. Keep EVM revert, exceptional halt,
transaction validation error, database error, and Foundry assertion failure
distinct throughout.

## 2. State ownership and observation model

These are conceptual owners, not proposed Rust traits.

| Owner | Contents | Boundary to audit |
| --- | --- | --- |
| Remote source/cache | Pinned remote accounts, code, storage, block data | Local writes must not become remote truth; fork identity and cache invalidation remain coherent. |
| Accepted local state | Account existence, balance, nonce, code, storage, lifecycle flags | Commit versus speculative execution; create/delete/selfdestruct and empty/nonexistent distinction. |
| Active transaction | Original/current slot values, warmth, transient storage, journal, logs, refunds | Fresh transaction versus nested frame; restore cannot invalidate suspended rollback checkpoints. |
| Active/suspended frames | PC, stack, memory, return data, caller/value/static context, gas, checkpoints | Child settlement, injected frames, fork switch, snapshot restore, parent continuation. |
| Execution environment | Block, configuration/spec, transaction fields, contract-visible overrides | Immediate opcode reads versus fee settlement versus subsequent transactions. |
| Fork lifecycle | Active ID, per-fork journal/local state, persistent accounts, replay position | Create/select/roll, persistence propagation, failure publication. |
| Foundry inspector/session | Expectations, prank/broadcast, mocks, recordings, created-account metadata, snapshot companions | Some state is restored specially; never assume every cheatcode field is snapshot state. |
| Observations | Traces, diagnostic logs, coverage, gas reports, failure/skip/cancellation classification | Observations may survive state rollback; execution success differs from test success. |

Every test should state which owners it observes. Equal final storage alone does
not establish equal gas, rollback, trace, or next-transaction behavior.

## 3. Execution transitions

### EX-01: speculative call and committing transaction

**S:** [Executor][executor] `call_with_env_and_context` clones the inspector stack
and executes against a borrowed `CowBackend`. It returns the changeset and
observations without calling `Executor::commit`. `CowBackend` initializes its
owned backend lazily when mutation requires it.

`transact_with_env_and_context` executes against the mutable backend and commits
the returned changeset. Commit also persists cheatcode state, clears collected
broadcast transactions/ignored-trace bookkeeping as implemented, and persists
block and gas-price changes. Do not substitute evm2 account commit for this whole
operation. An EVM revert may still return transaction-level effects; do not
discard every result whose status is not success. A host error exits before
`Executor::commit`, but this alone does not prove rollback of earlier backend or
external cheatcode effects.

**G:** compare accepted state, inspector state, environment, and returned changes
after speculative/committing success, revert, halt, and database failure. Run a
second call to detect leakage. Include snapshot deletion inherited from setup
(T-03); do not require rollback of filesystem/process/RPC side effects.

### EX-02: inherited nested execution

**S:** [with_inherited_evm][interfaces] inherits cfg/block, journal, and chain
context. An `Ok` closure publishes child journal/environment/chain changes;
`Err` does not publish those replacements. The parent transaction environment
is not replaced. Database and inspector effects are explicitly outside that
publication guarantee. `Ok` here is the host operation result, not necessarily
an EVM success status.

**T:** T-01 asserts journal depth/account publication for both closure outcomes
and preservation of the parent caller. It does not establish database atomicity,
all environment fields, or suspended-frame rollback.

**G:** injected child revert/halt versus host error; parent continuation; a later
parent revert; child logs and inspector effects. Establish intended behavior
before choosing evm2 state transfer APIs.

### EX-03: isolated call/create

**S:** [InspectorStack][stack] isolates a depth-1 ordinary `CALL` when enabled and
outside an inner context. `STATICCALL` instead cools accounts/slots in place,
except arbitrary-storage accounts; `DELEGATECALL`/`CALLCODE` remain ordinary.
Non-CREATE2 creates at the corresponding boundary use synthetic transactions;
CREATE2 has a separate redirect lifecycle (IN-03). Inner entry/exit suppresses
duplicate inspector processing.

Preparation via `prepare_child_state`:

1. Clone account state without modifying the parent; preserve account flags,
   including local creation.
2. Mark accounts outside the protocol warm set cold. This does not promise to
   actively warm every existing account flag in that set.
3. Mark every storage slot cold and set its original value to its current value.

Settlement via `merge_child_state`:

1. Import child account info; combine status flags. A warm parent is not made
   cold by a cold child. A cold parent's account flag can remain cold even if the
   child is warm; this is not the slot rule below.
2. For an existing slot preserve the parent's original, import the child's
   current value, and set coldness to `parent_cold && child_cold`.
3. Retain untouched parent entries; new child entries retain their metadata.
4. Merge the state returned by transaction execution, including on revert/halt;
   do not merge arbitrary pre-rollback child writes.

Outer root failure restores the saved top-frame account state because ordinary
journaling does not cover all isolation changes. Failure captured before outcome
rewriting still triggers restoration. `cheats.on_revert` also handles out-of-band
mutations. These operations are not a claim that every backend/inspector field
is restored.

**T:** T-02 checks preparation and the four parent/child coldness combinations.
T-25 checks a Cancun transient lock within a nested call and its absence on a
subsequent isolated call. It does not cover snapshot/fork restoration of transient state.
**G:** EX-03 complete call-kind matrix, remaining transient-storage boundaries, parent
revert after child success, selfdestruct/local creation, access lists, and root
failure rewritten by `expectRevert`. Gas/environment settlement is GA-01/EN-01.

### EX-04: signed `executeTransaction`

**S:** [executeTransactionCall][cheat-evm] rejects script context, decodes the
selected network envelope, recovers the signer, and creates fresh transaction
context. Ethereum processing enables nonce checks, resolves initcode limits,
restores the spec-defined gas cap, and zeros basefee/gas price/priority fee for
the synthetic execution. This is not literal canonical fee charging.

Prepare child account state as EX-03, suppress recursive isolation, execute,
restore the outer transaction and temporary configuration overrides, retain other
child cfg/block changes, then merge returned account state. Success returns
output bytes; revert/halt become cheatcode errors with different messages. The
code merges returned state before classifying EVM revert/halt as an error. The
enclosing cheatcode/frame rollback therefore needs its own test; do not infer
final visibility from that merge alone. Audit early host-error paths separately.

**T:** T-10 covers malformed encoding, transfers, and subsequent interaction.
**G:** invalid nonce, validation/host failure cleanup, revert/halt effects on
sender nonce and balances, nested environment mutation, then parent revert.

### EX-05: replay and fork transaction execution

**S:** [ordinary executor replay][executor] runs prefix transactions with the
inspector disabled, commits their returned state, then enables inspection for
the target. A reverted prefix is not a host error. Successfully created prefix
contracts become persistent. A later host error does not imply undoing an
already committed prefix in this entry point.

[Backend replay][backend] has different publication boundaries. In particular,
`apply_state_changeset` stages database and both journals before publishing;
failure refreshing loaded state must not publish a partial result. Persistent
accounts are excluded from `update_state` refresh. `vm.transact`, transaction-
position fork creation/rolling, and signed execution must not be collapsed into
one assumed atomic operation.

**T:** T-11 checks ordinary prefix/target state and trace separation; T-12 checks
failed refresh leaves the database and journals unchanged, including nonexistent
account status. **G:** finish auditing each fork replay entry point, target
position, system-envelope skip, inactive-fork effects, and failure atomicity.

## 4. Snapshot and fork transitions

### ST-01: snapshot capture, restore, and deletion

**S:** [Backend::snapshot_state / revert_state][backend] capture database state,
`JournaledState`, and cfg/block (`EvmEnv`). This is not an arbitrary whole-session
clone, nor a full `TxEnv` snapshot despite the broader trait comment.

On successful restoration:

- Restore captured database/journal/cfg/block; a forked snapshot also restores
  its active-fork identity and ensures the current caller is present.
- Keep current journal logs for diagnostics instead of rewinding them.
- Preserve evidence of the global failure slot in `has_state_snapshot_failure`.
- Restore cheatcode companion data through [inner_revert_to_state][cheat-evm]:
  per-fork environment overrides, fork-block override, and created-account
  tracking. Gas-price/blob fields are synchronized separately, including saved
  pre-override values; arbitrary transaction fields are not covered by this rule.
- `revertToState` retains the selected snapshot; `revertToStateAndDelete` removes
  it. A missing ID returns false. Snapshot deletion does not restore state.

**Important distinction:** backend cheatcode snapshots use `remove_at`, which
does not invalidate later IDs. [ForkedDatabase][fork-db] uses the cascading
`StateSnapshots::remove` path. Do not port cascading invalidation into cheatcodes
by analogy. Confirm the cheatcode behavior with G-02 before treating it as a
fully characterized compatibility promise.

**T:** T-03 covers storage/block restoration, deletion, repeated snapshots, and
setup snapshot deletion; T-04 covers nonempty pre-override blob restoration.
T-05 covers cascading invalidation only for `ForkedDatabase`. T-24 covers sticky
assertion failure with `assertions_revert = false`, including a fuzz entry point.
**G:** G-01/G-02; diagnostic log retention, other assertion modes, and precise
companion-state restoration across multiple forks.

### ST-02: create/select fork and persistent accounts

**S:** [Backend::select_fork][backend] returns early for an already active ID.
Switching saves outgoing journal state and current block number/timestamp,
initializes new-fork caller state as needed, propagates persistent accounts,
aligns journal depth, then switches active identity and environment. It preserves
the configured spec instead of accepting a default spec from fork construction.
First selection also establishes initial journal state for other/future forks.

Persistent propagation merges tracked data; it is not a blind replacement of
all target-fork accounts. Shared remote cache and local fork writes have distinct
ownership. Invalid IDs and remote-read failures must be characterized separately;
the sequence above does not establish all-or-nothing failure for every step.

**T:** T-06/T-07 cover ordinary storage separation and explicit persistence.
T-26 asserts source-fork creation metadata survives a switch followed by revert,
and checks dump ordering after snapshot restoration and a reverted roll. Do not
specify that every ancestor revert undoes every fork's mutations: the actual
baseline has more selective behavior. These tests do not assert complete state
or checkpoint correctness for those sequences.
**G:** G-03; switching below multiple suspended frames, then success/revert/halt;
caller availability; setup-to-test transitions; revoked persistence; untouched
remote data; failure injection before and after outgoing state capture.

### ST-03: roll/reset and external mutation

**S:** `DatabaseExt` exposes active/inactive roll, transaction-position roll,
and account/storage refresh. Their complete transition contracts are not yet
audited. **G:** specify each separately, including local/persistent state retained,
block/hash caches, source versus execution chain ID, and `vm.rpc` refresh.
Do not treat “fork switching works” as coverage of rolling or external mutation.

## 5. Inspection, gas, and reporting

### IN-01: callback order and outcome rewriting

**S:** [InspectorStack][stack] invokes observers in explicit order. For ordinary
calls, tracing begins before cheatcode caller overrides, then its caller is
synchronized to executed inputs. Short-circuit paths can stop further dispatch.
At call end, tracer/cheatcodes/printer/revert diagnostics can observe or rewrite
outcomes in order; a changed result/message can stop the remaining dispatch.
Root failure is captured before these rewrites (EX-03).

**G:** record ordered event sequences for normal execution, mock/cheatcode
short-circuit, empty code, precompile, depth/funds failure, constructor revert,
and host error. Assert balanced trace frames, executed caller/code/storage
address, precompile identity, and original versus reported failure separately.
The exact REVM engine-side checkpoint/hook ordering still requires an audit.

### IN-02: storage-hook injection and parent continuation

**S:** [Cheatcodes][cheat-inspector] captures SLOAD/SSTORE information, injects a
callback only after successful opcode execution, and resumes the parent with
saved gas/return data and the expected stack result. Callback failure becomes a
parent revert. Callback state writes remain journaled for enclosing rollback.

The callback temporarily suppresses expectation/mock/recording state; the stack
traces the instrumentation frame but bypasses its prank/broadcast/isolation path.
Warmth entries added by callbacks are removed/cooled while other journal entries
are retained. A stale REVM-version TODO exists in `restore_storage_hook_access`;
test original-slot-value behavior rather than treating the comment as evidence.

**T:** T-08 checks callback rollback, error bytes, proxy storage address, and
subtree suppression; T-09 checks provenance after failed KECCAK execution.
**G:** G-04; raw SLOAD/SSTORE hooks, stack at capacity, return-data preservation,
warmth/originals/refunds, callback OOG/static restrictions, nested callback calls.

### IN-03: CREATE2 redirection

**S:** When factory routing is selected, `frame_start` validates the factory and
rewrites create input into a call;
`frame_end` reconstructs the create result. Invalid factories surface through the
normal create inspector lifecycle to keep tracing balanced. Pending redirects
belong to the outer lifecycle and are saved around nested inspector borrowing.
Selection depends on depth and prank/broadcast context: CREATE2 requires broadcast
or `always_use_create_2_factory`; ordinary CREATE requires broadcast plus
`batch_rewrite_creates`. Do not redirect every creation unconditionally.
**G:** G-05; expected address/code/value/constructor caller, missing or invalid
factory, nested redirects, revert/OOG, expectation rewriting, and state gas.

### GA-01: gas and synthetic transaction accounting

**S:** [transact_inner][stack] adds intrinsic gas to the child regular limit,
carries the reservoir, compensates precharged state gas, applies configured
block/transaction limits, and settles regular/state costs separately. Refunds
are imported on success. A host error currently becomes an empty revert result
on this path; distinguish that from EX-04 error reporting.

[convert_executed_result][executor] exports receipt-style transaction gas and
final refunds plus a separately calculated stipend. Preserve frame gas, receipt
gas, gasleft(), snapshot values, and refund counters as separate observations.
Do not apply one subtraction formula indiscriminately across hardforks.

**T:** T-13 checks gas pause/resume; T-14 checks Amsterdam conditional charges
with upper-bound assertions, not exact total-gas equality. T-17/T-18 add actual
snapshot comparisons and exact fixture gas values; GA-02 defines their scope.
**G:** exact differential
gas for success/revert/OOG, cold/warm SLOAD/SSTORE, stipend/EIP-150, refund caps,
calldata floor, reservoir exhaustion, precompiles, and nested pause/reset/resume.

### GA-02: last-call/frame gas, snapshot regions, and observer neutrality

**T:** [LastCallGas][last-gas-tests] distinguishes a call from a creation:
`lastCallGas`/`snapshotGasLastCall` must not acquire a creation result, while
`lastFrameGas` accepts call/create/CREATE2. Expected-revert processing can clear
cached last-frame gas, including a previously successful creation's cache.

`LastCallGasIsolatedTest::testRecordGasRefund` asserts total 26180, refund 4800,
and last-call/frame snapshots 21380. The excluded `LastCallGasDefaultTest`
asserts total 216, refund 19900, and snapshots 216 for its fixture. These are
different measures, not interchangeable receipt fields. Preserve the tests with
their compiler/configuration; these constants are not universal gas formulas.
`testSnapshotGasForFailedCharge` asserts a failed invalid-opcode call followed
by zero last-call/frame snapshot values, not zero actual execution cost.

[GasSnapshots][gas-snapshot-tests] compares regions with a Solidity `gasleft()`
measurement that subtracts a fixture-specific 138 overhead. Internal cold/warm
comparisons allow absolute error 6; external/create/nested comparisons assert
equality. The refund comparison explicitly disables isolation and measures gross
gas. Several other fixture tests only assert positivity or make calls without
asserting a numerical value; do not count them as exact accounting evidence.

State-diff recording must not change the tested gas total for storage reads/writes
or BALANCE/EXTCODESIZE/EXTCODEHASH/EXTCODECOPY. T-18 checks these paired executions;
equal gas is evidence against observer-induced warming in those cases, not a
complete state-equivalence proof. **G:** extend observer neutrality to return
data, state, nested rollback, and combinations with other inspectors.

### EN-01: contract-visible environment versus settlement

**S:** isolation zeros fee-related execution fields and restores the outer tx,
basefee, and temporary gas cap afterwards while retaining other nested cfg/block
changes. EIP-4844 synthetic calls are downgraded for validation; opcode overrides
preserve contract-visible blob information. Snapshot restoration also restores
override metadata (ST-01).

**G:** table-test each supported environment cheatcode with immediate opcode read,
nested read, parent resumption, snapshot restore, fork switch, and next transaction.
Include nonzero pre-override gas price, nonempty blob hashes, failed opcodes, and
both isolation modes. Do not assume cfg/block restoration restores all tx fields.

### OB-01: observations and execution classification

**S:** [result conversion][executor] distinguishes execution outcome from
cheatcode skip payloads, cancellation, and test success. Collected diagnostic logs
can differ from execution-result logs. Trace bytecode and call-kind data feed
debugging; coverage and fuzz inspectors consume opcode/stack observations.

**T:** T-15 checks active cancellation and completed-result stability. Its
Ctrl-C test signals thread startup before transaction execution and then sleeps;
it is timing-sensitive evidence, not a deterministic checkpoint-controlled proof.
**G:** unchanged failure/revert bytes, skip/assume/assert classification, trace
structure and source mapping, coverage hits, fuzz feedback, and cancellation
without other opcode observers. Blocking host work is a separate cancellation
question; opcode polling alone does not prove it interruptible.

### OB-02: trace production versus debugger consumption

**T:** T-19 separates four kinds of evidence: configuration requirements, actual
executor step production, trace-tree projection, and serialization/diagnostics.
Debug mode records seven steps with memory/stack for the bytecode fixture;
internal decoding records two; disabling tracing again yields no trace. Full
internal decode plus state diff requires unfiltered steps and full stack/memory;
all-steps alone deliberately avoids those expensive snapshots.

Diagnostic resolution must not replace raw EVM return bytes. Serialized ignored
ranges must survive until resolution. Depth projection reindexes children/parents.
T-20 checks called-contract ABI decoding in CLI output, with gas redacted: it
does not prove gas parity. T-21 consumes synthetic step data in debugger tests;
it proves consumer interpretation, not correctness of engine-produced steps.
T-22 supplies partial end-to-end evidence: exact SHA256 precompile display and
presence of SLOAD/SSTORE entries in a debug dump, not all dump fields/values.

**G:** compare actual baseline/candidate step streams: PC/opcode identity,
pre/post stack timing, memory/return data, code versus storage address, depth,
status, gas, storage changes, and creation/runtime bytecode. Include failed
instructions, delegatecall, fork switching, source-map/internal-call decoding,
and pause/ignore ranges. Synthetic consumer tests alone cannot close this gap.

### OB-03: coverage is execution observation, not just a hit total

**S:** [LineCoverageCollector][coverage-inspector] records PC hits before opcode
execution, distinguishes empty/shared-memory calldata and value for function
attribution, and attaches successful creation bytecode information. Do not change
to successful-instructions-only recording without a compatibility decision.

**T:** T-23 checks exact LCOV for empty shared calldata in non-isolated standard
and IR-minimum modes; a nested ternary output asserts three of four branches;
a failing test run must still write coverage. The last test only checks file
existence and SF/FN/DA markers, not exact counts. **G:** bytecode/source identity,
initcode/runtime mapping, full branch counters, failed opcodes, and inspector
combinations. Coverage percentages are not evidence of state rollback parity.

## 6. Test evidence ledger

All rows are **T**, source-audited; executed subsets are identified in sections 8–9. Scope is
deliberately narrower than the feature name. Grouped tests share a source link.

| ID | Source and tests | Assertions / limitations |
| --- | --- | --- |
| T-01 | [interfaces]: `inherited_journal_publishes_only_after_success` | Both closure outcomes; depth/balance and parent caller. No suspended-frame execution. |
| T-02 | [interfaces]: `preparation_preserves_creation_and_protocol_warmth`, `settlement_preserves_parent_original_values_and_combines_warmth` | Preparation immutability/flags/slot originals; four warmth combinations on one existing account/slot. |
| T-03 | [StateSnapshots][snapshot-tests]: `testStateSnapshot`, `testStateSnapshotRevertDelete`, `testStateSnapshotDelete`, `testStateSnapshotDeleteAll`, `testStateSnapshotsMany`, `testBlockValues`; `StateSnapshotDeleteFromSetUpTest` | Ordinary restore/delete, block values, setup and fuzz-run snapshot ownership. No full live-frame state matrix. |
| T-04 | [executor]: `pre_override_blob_hashes_restored_on_revert_to_state` | Original nonempty hashes restored; inactive overrides removed. Separate executor transactions, not nested live frames. |
| T-05 | [fork-db]: `fork_db_revert_invalidates_newer_snapshots` | First snapshot revert invalidates second in lower-level database API only. Constructs an RPC-backed provider; execution requirements must be recorded. |
| T-06 | [Fork.t.sol][fork-tests]: `testForksHaveSeparatedStorage` | A write on one fork does not overwrite the other fork's slot. Requires configured RPC endpoints. |
| T-07 | [Fork2.t.sol][fork2-tests]: `testMarkPersistent` | Contract remains callable with expected value after explicit persistence and fork switch. Uses mainnet and Optimism RPC endpoints; retain the assertion with two Ethereum fixtures if trimming removes this setup. |
| T-08 | [MappingStorageHooks][hook-tests]: `testEnclosingRevertRollsBackCallbackState`, `testCallbackRevertPropagates`, `testDelegatecallUsesProxyStorageAccount`, `testCallbackSubtreeIsSuppressed` | Callback/target rollback, exact error payload, proxy rather than implementation callback, one callback despite subtree. |
| T-09 | [hook-tests]: `testOogKeccakDoesNotRecordProvenance` | Failed hash must not create provenance that later triggers a hook. |
| T-10 | [ExecuteTransaction][execute-tests]: `test_revert_not_a_tx`, `test_execute_legacy_transfer`, `test_execute_eip1559_transfer`, `test_execute_then_interact` | Malformed input, funded transfers, subsequent interaction. Does not close failure settlement matrix. |
| T-11 | [executor]: `block_replay_commits_prefix_and_traces_only_target` | Prefix increment plus reverted create; target value, nonce 3, committed storage, one trace node. |
| T-12 | [backend]: `failed_fork_state_refresh_does_not_publish_transaction_changes`, `failed_fork_state_refresh_preserves_not_existing_account` | Failed refresh preserves database/journals and nonexistent-account status; fault injected via closed backend. |
| T-13 | [GasMetering][gas-tests]: `testGasMetering`, `testGasMeteringExternal`, `testGasMeteringContractCreate` | Paused gas delta zero; resumed delta matches ordinary execution where asserted. |
| T-14 | [executor]: `amsterdam_intercepted_create_refunds_state_gas`, `amsterdam_mocked_call_revert_refunds_state_gas` | Success of enclosing call and gas below conditional charge; not exact accounting across all outcomes. |
| T-15 | [executor]: `early_exit_interrupts_active_evm_execution`, `completed_execution_is_not_retroactively_cancelled`, `campaign_deadline_interrupts_active_evm_execution` | Cancellation marker/Stop, bounded gas, completed result unchanged. No blocked-host cancellation proof. |
| T-16 | [script CLI][script-tests]: `fork_nested_broadcast_nonces` | Both isolation modes; six transactions/receipts, nonces 0–5, provider nonce 6, nested deployment code. Uses in-process Anvil as test infrastructure. |
| T-17 | [gas-snapshot-tests]: `testGasComparisonEmpty`, `testGasComparisonInternalCold`, `testGasComparisonInternalWarm`, `testGasComparisonExternal`, `testGasComparisonExternalRefund`, `testGasComparisonCreate`, `testGasComparisonNestedCalls` | Region comparison against gasleft minus 138; cold/warm tolerance 6; refund case explicitly non-isolated. Paris compilation restriction; no claim of all-hardfork coverage. |
| T-18 | [last-gas-tests]: `testRecordGasRefund`, `testSnapshotGasForFailedCharge`, `testExpectedRevertCreateClearsCachedLastFrameGas`, `testNestedExpectedRevertCallClearsCachedLastFrameGas`, `testLastCallGasDoesNotRecordCreate`, `testStateDiffRecordingDoesNotWarmStorageReads`, `testStateDiffRecordingDoesNotWarmStorageWrites`, `testStateDiffRecordingDoesNotWarmBalanceReads`, `testStateDiffRecordingDoesNotWarmExtcodesizeReads`, `testStateDiffRecordingDoesNotWarmExtcodehashReads`, `testStateDiffRecordingDoesNotWarmExtcodecopyReads` | Exact fixture gas/refund/snapshot distinctions, cache invalidation and observer neutrality. Qualify duplicate test names by isolated/default contract. Default contract is excluded from main harness. |
| T-19 | [trace-lib]: `requirements_preserve_internal_decode_with_state_diff`, `requirements_all_steps_avoid_debug_snapshots`, `trace_depth_projection_removes_and_reindexes_nodes`, `revert_diagnostic_only_changes_resolved_trace`, `serialization_resolves_diagnostics_without_consuming_ignored_ranges`; [executor]: `set_trace_requirements_replaces_trace_mode_between_transactions` | Config/synthetic arena checks versus actual executor step generation are separate evidence. |
| T-20 | [trace-cli]: `trace_struct_outputs_use_called_contract_abi` | Two contracts with same selector decode using their own struct ABI; CLI snapshot redacts gas. |
| T-21 | [debug-context]: `command_prompt_finds_warm_sload_from_stack_snapshots`, `command_prompt_finds_warm_sstore_from_stack_snapshot`, `command_prompt_ignores_failed_sstore_stack_snapshot`, `command_prompt_ignores_failed_sstore_storage_change` | Synthetic steps drive exact navigation/status; static violation and OOG stores ignored. Does not execute EVM bytecode. |
| T-22 | [test-harness]: `debug_dump_marks_precompile_call_steps`, `debug_dump_includes_storage_changes` | Real CLI execution; exact decoded SHA256 line and presence of load/store entries. Dump existence alone would be weaker evidence. |
| T-23 | [coverage-cli]: `empty_shared_calldata`, `ternary_nested_partial`, `coverage_with_failing_tests` | Exact LCOV in two non-isolated compiler modes; 75% branch assertion; failing-suite artifact/marker check only. |
| T-24 | [repros]: `issue_3055` | Three failing test outputs after snapshot restoration with non-reverting assertions, including a fuzz entry point. Existing evidence for sticky failure; not every assertion mode. |
| T-25 | [test-harness]: `can_test_transient_storage_with_isolation` | Cancun/isolation: nested reentrancy observes transient lock; subsequent isolated call sees it cleared. Does not exercise snapshot/fork restore. |
| T-26 | [fork2-tests]: `testForkDumpStatePreservesPropagationAfterSnapshotRevert`, `testForkDumpStatePreservesSourceOrderAfterRevertedSwitch`, `testForkDumpStatePreservesOrderAfterRevertedRoll` | Dump inclusion/order and nonzero source-created address after switch/revert; not complete balances/storage/warmth. Mainnet/Optimism RPC prerequisites. |
| T-27 | [SuspendedSnapshot](../../testdata/default/cheats/SuspendedSnapshot.t.sol): `testSuspendedSnapshotParentSuccess`, `testSuspendedSnapshotParentRevert`, `testSuspendedSnapshotParentHalt` | Both isolation modes. Nonzero original storage; snapshot/restore within child, leaf rollback, transient state, exact cold/warm SLOAD delta, return/revert bytes, retained diagnostic logs, parent continuation marker before INVALID, later call. Six cases executed; partial G-01. |
| T-28 | [local fork harness][test-harness]: `fork_suspended_parent_settlement`; [fixture](../../crates/forge/tests/fixtures/SuspendedFork.t.sol) | Two local Ethereum nodes, both isolation modes, two Solidity cases each. A→B→A inside child, persistent parent/child, nonpersistent cell, exact revert payload, parent/active-A rollback, source-B write retained, later call. Output snapshot enforces two non-skipped tests per mode; partial G-03. |

### Harness reachability and strength of evidence

[The testdata harness][test-harness] runs `forge test` from `testdata`, populates
RPC endpoints, applies contract exclusions, and retries failures up to three
times. Record first-attempt failures as well as eventual success. Its main run
excludes `LastCallGasDefaultTest` (among other contracts); the separate flaky run
selects different contracts. No dedicated selection of this default gas contract
was found in the searched repository/workflow sources. To validate it, explicitly
select the contract with `--no-isolate` and verify nonzero count; do not cite the
main harness as coverage. This is an execution gap, not proof the fixture fails.

`testdata/foundry.toml` restricts `paris/**` compilation to Paris; individual tests
can override execution settings (for example Cancun). Record compilation target
and execution hardfork separately. Rust unit tests do not execute Solidity
fixtures; `cargo clippy --all-targets` executes neither. CLI tests rely on their
built Forge binary, Solc, and where applicable RPC/forge-std and genhtml support.

The debugger CLI's `manual_debug_setup` is ignored; its existence cannot count
as an automated acceptance gate. The separate network-selection debugger test
uses Tempo and is not an Ethereum-first substitute for T-21/T-22.

### Priority cross-boundary tests to characterize next

These are **G** obligations, not test names. T-27/T-28 now cover initial subsets
of G-01/G-03; neither obligation is closed. First establish baseline outcomes;
if a sequence exposes a baseline bug, record it and decide its compatibility
treatment instead of silently freezing or fixing it during the engine port.

| ID | Sequence | Required observations |
| --- | --- | --- |
| G-01 | A calls B; B snapshots, changes storage/transient state, calls C, restores, continues; vary B/C/parent success, revert, halt. Also restore a snapshot taken at another depth/setup. | No invalid checkpoints; exact state at each continuation and final settlement; warmth/originals/refunds; distinguish journal logs retained on snapshot restore from frame-reverted logs and independent inspector diagnostics. |
| G-02 | Take S1/S2; restore S1 with keep/remove; use S2; delete missing IDs; repeat keep; assert failure before/after capture. | Snapshot validity independently per API; no arbitrary tx/inspector rewind; assertion failure remains visible. |
| G-03 | Select A→B→A while nested; modify persistent/nonpersistent accounts; later revert ancestor. Repeat same-ID selection and remote-read failures. | Active identity, code/storage/nonce/balance, configured spec, depth, caller, environment, each fork's state; explicitly determine failure atomicity. |
| G-04 | Successful SLOAD/SSTORE injects callback; callback calls another contract/writes/reverts; parent reads gas/return data/slot then reverts. | Stack/result integrity, callback state rollback, no recursive instrumentation, original-slot/refund preservation, restored inspector state. |
| G-05 | CREATE/CREATE2 factory redirect inside isolated/nested broadcasts, success/revert/OOG; follow with another broadcast. | Addresses, value, constructor caller, nonce sequence, gas/reservoir, traces, no leaked pending redirect. |
| G-06 | Signed transaction succeeds/reverts/fails validation inside a parent; follow with a second signed transaction and parent revert. | Nonce/balance/code/storage settlement, outer tx/config/override restoration, error class, no leaked inner-context state. |
| G-07 | Clone setup baseline into two fuzz workers; delete snapshots/mutate mocks/forks in one; run and replay invariant sequence. | Worker independence, fresh transaction scratch, accepted sequence state, reproducible failure classification and shrink replay. |

Use raw-bytecode Rust tests for gas/checkpoint details, Solidity for contract-
visible behavior, and CLI tests for output/workflow contracts. Prefer deterministic
local RPC fixtures for new fork tests. If temporary Anvil breakage prevents the
existing harness running, retain its tests and record the gap; use a pinned
external baseline node or an RPC fixture where feasible. Do not make Anvil repair
a prerequisite for implementing the core transition tests.

## 7. Coverage boundaries and acceptance

This is a lifecycle-first specification, not an exhaustive cheatcode audit.
The next granular passes must inventory:

- Individual state mutations (`deal`, `store`, `etch`, nonce changes, arbitrary
  storage), prank/broadcast/delegation, mocks, expect/assume/skip behavior.
- Fuzz/invariant setup, reset, dictionary/feedback, rejection accounting, replay,
  shrinking, and corpus compatibility; G-07 is only the initial cross-cutting test.
- Remaining trace/debugger/coverage field contracts beyond OB-02/OB-03, script simulation versus collection,
  Cast replay, Chisel persistence, verification, and symbolic adapters.
- Hardfork and transaction-type coverage: pre/post warm-access rules, transient
  storage, selfdestruct, authorization/delegation, blob context, and Amsterdam gas.
  Full existing Ethereum support is required. Inventory and sequence the matrix;
  no fork or workflow is dropped because the first tests use a smaller subset.
- Precompile success/error/OOG, custom test configuration, resource limits,
  invalid bytecode/initcode, and host/database errors.

Proposed acceptance gates, to refine as the inventory grows:

1. Each critical transition above has explicit baseline outcomes and tests for
   success plus relevant EVM/host failure paths; G-01 through G-06 are resolved.
2. Preserve existing Ethereum behavioral assertions; add gaps rather than weaken
   tests or broadly bless changed snapshots. Stable contract IDs survive renames.
3. Run baseline and candidate with the same compiler, hardfork, isolation mode,
   transaction/state inputs, seed, and inspector configuration. Compare state,
   outputs/status, gas, traces/logs, and later execution—not only process exit.
4. Record exact commands and nonzero executed counts. Identify ignored/skipped
   tests and RPC/solver/toolchain dependencies; no coverage percentage without a
   defined denominator. Rebuild the actual Forge binary used by CLI/fixture tests.
5. Ethereum workflow parity includes the consumers in section 7; temporary
   deferred-network/Anvil build failures remain separate from Ethereum regressions.
6. Benchmark matched real workloads after semantic gates: plain/inspected calls,
   isolation, snapshots/forks, fuzz reset and invariant campaigns. Track wall time,
   allocations/state-copy costs where measurable, and domain counters. No speedup
   claim follows solely from replacing the interpreter.

evm2 API feasibility remains separate evidence: detached transaction state does
not include a live journal/log/transient snapshot; checkpoint cursors must remain
valid after replacement; call/create hooks do not directly supply the current
frame hooks; mutable block support does not imply mutable tx/config support.
Resolve these against the pinned implementation before selecting an adapter design.

## 8. Initial adversarial audit and validation record

The initial document did not use every advertised anchor to equal depth. In
particular gas snapshots, debugger and coverage had not been assertion-audited.
This revision adds representative assertions from every original anchor family;
it does not claim exhaustive inspection of every test in those files.

Corrections to the confidence assessment:

- Gas-region coverage was understated; positivity, approximate comparison, exact
  comparison and exact fixture constants now have separate descriptions.
- Sticky snapshot failure and isolated transient boundaries already have tests;
  remaining gaps concern combinations/restoration, not total absence of coverage.
- Source-fork effects after switch/revert prohibit a blanket rollback invariant.
- Tracing consumer/config tests cannot establish producer callback timing; coverage
  totals and redacted CLI traces cannot establish gas/state parity.
- Harness exclusions, retries, ignored tests and compiler profiles are part of
  the evidence. Merely resolving a test name is not a coverage check.
- G-01 through G-07 remain open. No passing current suite alone would establish
  evm2 parity; the same assertions must run against the migrated implementation.

Runtime attempts and successful runs are recorded below. Commands
use the baseline above with documentation-only working changes. Default Cargo
features apply unless a command states otherwise.

- Initial `cargo +nightly test -p foundry-evm-core --lib evm::tests:: -- --nocapture`
  failed in `svm-rs-builds` fetching its pinned Solc manifest due to DNS resolution;
  zero tests executed. Retried with the matching cached manifest through
  `SVM_RELEASES_LIST_JSON`.

All four successful commands below used the prefix
`SVM_RELEASES_LIST_JSON=/Volumes/Stockage/dev-cache/solc-releases-e4b80d33-aarch64.json`.
Each reported zero failed and zero ignored tests. Counts include tests beyond
the ledger where a filter selects a broader group; they are not coverage counts.

| Command | Passed | Filtered out | Evidence |
| --- | ---: | ---: | --- |
| `cargo +nightly test -p foundry-evm-core --lib evm::tests:: -- --nocapture` | 3 | 91 | T-01/T-02 child-state helpers. |
| `cargo +nightly test -p foundry-evm --lib executors::tests:: -- --nocapture` | 20 | 178 | Includes T-04, T-11, T-14, T-15 and T-19 step production. |
| `cargo +nightly test -p foundry-evm-traces --lib tests:: -- --nocapture` | 94 | 0 | Includes T-19 configuration, tree and serialization tests; also network-specific decoding. |
| `cargo +nightly test -p foundry-debugger --lib command_prompt_ -- --nocapture` | 16 | 108 | Includes T-21 synthetic consumer tests and other command-prompt tests. |

Original anchor reconciliation:

| Anchor family | Assertion audit | Runtime in this audit |
| --- | --- | --- |
| Inherited journal / child preparation and settlement | T-01/T-02 | Passed. |
| StateSnapshots, Fork, Fork2 | T-03/T-06/T-07/T-26; sticky failure T-24 | Source only; executor snapshot metadata T-04 passed separately. |
| ExecuteTransaction | T-10 | Source only. |
| MappingStorageHooks, rollback and suppression | T-08/T-09 | Source only. |
| Gas metering / snapshots | T-13/T-17/T-18 | Source only; default gas contract needs explicit harness selection. |
| Amsterdam intercepted create / mocked call | T-14 | Passed. |
| Nested broadcast nonces | T-16 | Source only; requires Anvil test infrastructure. |
| Trace / debugger / coverage / cancellation | T-15/T-19 through T-23 | Rust trace/debugger/cancellation subsets passed; CLI and coverage source only. |

Confidence boundary: these 133 passing baseline tests check the current engine.
They neither execute the Solidity/CLI anchors above nor establish evm2 behavior.
At this stage, the highest-risk missing proof was live replacement while a parent
was suspended. Section 9 adds initial executable evidence and supersedes the
source-only runtime labels in the reconciliation table above.

## 9. Baseline execution and first live-frame regressions

2026-09-18, same pinned engine with test/documentation changes only. Full existing
Ethereum support is the acceptance scope; network gates remain intact. The live
state implementation decision remains deferred to an evm2 experiment.

### Configuration and reproduction

Built this checkout with `cargo +nightly build -p forge --bin forge`, prefixed by
the cached `SVM_RELEASES_LIST_JSON` assignment in section 8. Cargo default features
are `jemalloc,asm-keccak,optimism`; Base/Monad are not enabled. This is Ethereum
execution with OP support compiled in, not a build proving OP dependencies absent.
The executed Forge binary was
`/Volumes/Stockage/dev-cache/cargo-targets/shell-eec7845ad4615eeabefc/debug/forge`.
Below, `FORGE_BIN` denotes that rebuilt binary.

Fixture commands used `--root testdata --offline -vv`. Automatic compiler
selection chose Solc 0.8.37 for these Solidity fixtures. Default execution/config
is Osaka, optimizer on/200 runs, no IR; `paris/**` compiles for Paris. Inline
fixture configuration still applies (notably Cancun and isolation overrides).
Existing fuzz entries ran 256 cases; their seed was not pinned in this baseline
sweep, so this is not yet a reproducible differential fuzz campaign. The new
regressions use deterministic inputs. CLI harness projects use Solc 0.8.35.

| `--match-path` | Additional options | Passed / failed / skipped |
| --- | --- | --- |
| `default/cheats/{StateSnapshots,ExecuteTransaction,MappingStorageHooks,GasMetering}.t.sol` | `--no-isolate` | 39 / 0 / 0 |
| Same | `--isolate` | 39 / 0 / 0 |
| `paris/cheats/{GasSnapshots,LastCallGas}.t.sol` | `--no-isolate` | 59 / 0 / 0; includes all nine `LastCallGasDefaultTest` tests. |
| Same | `--isolate --no-match-contract LastCallGasDefaultTest` | 50 / 0 / 0; excluded default contract intentionally requires non-isolated accounting. |
| `default/cheats/{Fork,Fork2}.t.sol` | `--no-isolate` | 39 / 0 / 0 |
| Same | `--isolate` | 39 / 0 / 0 |
| `default/cheats/SuspendedSnapshot.t.sol` | No isolation flag; two contracts explicitly select false/true and Cancun execution. | 6 / 0 / 0 |

Example exact fixture invocation:

```sh
"$FORGE_BIN" test --root testdata --offline -vv --no-isolate \
  --match-path 'default/cheats/{StateSnapshots,ExecuteTransaction,MappingStorageHooks,GasMetering}.t.sol'
```

Fork runs supplied these public RPC variables:
`RPC_MAINNET=https://ethereum.reth.rs/rpc`,
`RPC_MAINNET2=https://ethereum.reth.rs/rpc`,
`RPC_OPTIMISM=https://mainnet.optimism.io`,
`RPC_SEPOLIA=https://ethereum-sepolia-rpc.publicnode.com`.
Existing latest-block fixtures remain dependent on changing remote state; these
results are an observed baseline, not hermetic replay. New T-28 uses local nodes.

Nine existing CLI tests each executed exactly once and passed, with no ignored
tests: `coverage::empty_shared_calldata`, `coverage::ternary_nested_partial`,
`coverage::coverage_with_failing_tests`,
`test_cmd::debug_dump_marks_precompile_call_steps`,
`test_cmd::debug_dump_includes_storage_changes`,
`test_cmd::trace::trace_struct_outputs_use_called_contract_abi`,
`test_cmd::repros::issue_3055`,
`test_cmd::can_test_transient_storage_with_isolation`,
`script::fork_nested_broadcast_nonces`.
The coverage test runs both standard and IR-minimum paths; broadcast tests both
isolation modes. CLI gas redactions still do not establish exact gas parity.

The first used `cargo +nightly test -p forge --test cli
coverage::empty_shared_calldata -- --exact --nocapture`. The remaining eight
invoked the built `cli-3a3ea9140981f177` test executable with each full name plus
`--exact --nocapture`; each reported 1 passed and 1568 filtered out. Reproduce via
that same Cargo command with the corresponding full name and the cached manifest.

New T-28 used `cargo +nightly test -p forge --test cli
test_cmd::fork_suspended_parent_settlement -- --exact --nocapture`: 1 Rust harness
passed, 1569 filtered out. Its output assertions require 2 Solidity tests passed
in each isolation mode (4 cases), with no failed/skipped cases. Local socket
access is required. No external RPC or forge-std download is needed by T-28.

### What the new tests establish—and leave open

- T-27 restores child/leaf persistent and transient storage while the parent is
  suspended. Parent success retains child writes; parent revert/INVALID restores
  nonzero pre-parent storage. A 2000-gas cold/warm SLOAD difference survives
  restoration. Exact child return data and parent error data are checked.
- The same transaction retains child transient state after successful parent
  return; isolated execution clears it before the next top-level call. Both
  contracts assert this distinction. The initial draft's shared expectation of
  30 failed in isolated mode; it was a test-model error, not an engine regression.
- Three inspector-recorded events survive snapshot/ancestor rollback; the final
  event proves the parent resumed before its deliberate INVALID. These assertions
  concern diagnostic logs, not canonical receipt/EVM logs.
- T-28 establishes a concrete publication boundary: after A→B→A, parent revert
  restores the active A cell to 11 and persistent parent to 0, but B retains 222.
  Successful parent execution leaves A at 111. Both modes subsequently switch
  forks and execute another write successfully. Do not generalize this into
  atomic rollback across every fork, or to all persistent-account mutations.
- G-01/G-03 remain partial: cross-depth/setup snapshot restoration, child/leaf
  failure permutations, refund counters and caps, canonical logs, account warmth,
  fork-switch transient state, same-ID selection and remote failures still need
  targeted characterization. Six/four passing cases are not the full matrix.

### Failed attempts and checks

- Forcing `--use 0.8.35` on the mixed-version testdata root failed compiler
  resolution before tests; automatic installed-version selection resolved it.
- Sandboxed remote fork setup failed DNS (zero bodies executed). With network
  access, an incomplete RPC configuration produced 32 passes/7 failures due to
  missing `RPC_SEPOLIA`; all 39 passed after configuring it. No test assertions
  were weakened or snapshots blessed to obtain success.
- The initial broadcast run failed to bind its local node (`Operation not
  permitted`); the same test passed with local socket access.
- `cargo +nightly fmt`, strict all-feature/all-target Clippy from the root
  instructions, and Solidity formatting were run. This work changes tests and
  documentation only; no engine implementation or network trimming occurred.
- If submitted as a test/documentation-only PR, request maintainer `L-ignore`
  rather than claiming a user-facing behavior change in a release note.

[interfaces]: ../../crates/evm/core/src/evm/mod.rs
[executor]: ../../crates/evm/evm/src/executors/mod.rs
[backend]: ../../crates/evm/core/src/backend/mod.rs
[stack]: ../../crates/evm/evm/src/inspectors/stack.rs
[cheat-evm]: ../../crates/cheatcodes/src/evm.rs
[cheat-inspector]: ../../crates/cheatcodes/src/inspector.rs
[fork-db]: ../../crates/evm/core/src/fork/database.rs
[snapshot-tests]: ../../testdata/default/cheats/StateSnapshots.t.sol
[fork-tests]: ../../testdata/default/cheats/Fork.t.sol
[fork2-tests]: ../../testdata/default/cheats/Fork2.t.sol
[hook-tests]: ../../testdata/default/cheats/MappingStorageHooks.t.sol
[execute-tests]: ../../testdata/default/cheats/ExecuteTransaction.t.sol
[gas-tests]: ../../testdata/default/cheats/GasMetering.t.sol
[script-tests]: ../../crates/forge/tests/cli/script.rs
[gas-snapshot-tests]: ../../testdata/paris/cheats/GasSnapshots.t.sol
[last-gas-tests]: ../../testdata/paris/cheats/LastCallGas.t.sol
[trace-lib]: ../../crates/evm/traces/src/lib.rs
[trace-cli]: ../../crates/forge/tests/cli/test_cmd/trace.rs
[debug-context]: ../../crates/debugger/src/tui/context.rs
[test-harness]: ../../crates/forge/tests/cli/test_cmd/mod.rs
[coverage-cli]: ../../crates/forge/tests/cli/coverage.rs
[coverage-inspector]: ../../crates/evm/coverage/src/inspector.rs
[repros]: ../../crates/forge/tests/cli/test_cmd/repros.rs
