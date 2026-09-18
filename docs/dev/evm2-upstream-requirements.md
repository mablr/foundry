# evm2 capabilities needed for Foundry integration

Status: request preparation, 2026-09-18. Nothing has been submitted upstream.
This records the missing capability and its evidence, not an approved API design.
Owner of detailed Foundry behavior: [migration specification](evm2-migration-spec.md).
Historical reproduction archive and retained evidence: [live-state experiment](../../experiments/evm2-live-state/README.md).

## Request summary

Foundry needs to capture and replace live execution state from an inspector callback
while EVM frames remain suspended, then resume and settle those frames correctly.
Snapshots and fork selection require this; ordinary transaction commit/discard is
not the same operation. We need an evm2-supported way to coordinate replacement of
the native state with the rollback boundaries retained by active frames.

The experiment demonstrates a missing integration capability. It does not establish
a consensus-execution bug in unmodified evm2: the failing scenarios perform
Foundry-style restoration using an incomplete adapter or experimental patch.

A bounded transaction-layer copy improves compatibility but leaves invalid frame
checkpoints. Please help establish a native replacement/settlement contract before
we build Foundry's production snapshot and fork adapters around internal details.
No Foundry-specific snapshot IDs, fork orchestration, or cheatcode dispatcher need
to move into evm2.

## Evidence and source boundary

Inspected evm2 revision: `2c8b67f03fb0c86c0c2502d840d2e529bdf4b1a8`.
Foundry reference base: `295647678`, with the experiment's reference example added.
These historical failure results apply to those revisions; the adapted local branch
and its newer-base validation are recorded below.

Relevant evm2 symbols at that pin:

- `crates/evm2/src/evm/state/journal.rs`: `StateCheckpoint` stores journal/log lengths.
- `crates/evm2/src/evm/mod.rs`: `execute_call_message` retains a checkpoint locally
  across `run_interpreter`; `finish_call_message_run` uses it on unsuccessful exit.
  CREATE and precompile execution also have checkpoint/rollback paths to audit.
- `crates/evm2/src/evm/state/mod.rs`: `State::rollback` requires both cursors to fit
  the current history and truncates logs. `set_pending_state` replaces the pending
  overlay without restoring journal, logs, warm sets or transient storage.
- `crates/evm2/src/evm/inspector.rs`: `Inspector::call` provides mutable host access
  through the interpreter, but that alone cannot update checkpoints held by callers.

The 54-case Cancun matrix crosses six snapshot/call families with three child and
three parent outcomes. Unmodified evm2 plus the checkpoint-only adapter matches
30 cases and panics in 12. The experimental native-copy patch matches 39 and still
panics in six. All nine ordinary-call controls match. These counts describe a
small experiment, not the percentage of Foundry that is compatible.

Minimal failure in the historical native-copy candidate, using
`parent_snapshot_immediate_child_revert_parent_success`:

1. Parent A writes persistent/transient value 11 and captures snapshot S.
2. A calls B. evm2 records B's frame checkpoint after call preparation has extended
   the journal beyond S's history.
3. B restores S, replacing the transaction layer and journal while A and B still
   exist as execution frames.
4. B immediately REVERTs, without producing replacement journal entries.
5. B's settlement calls `State::rollback` with its old checkpoint and panics:
   `checkpoint is past journal length`.

INVALID instead of REVERT reproduces the same failure. Varying A's intended
success/revert/INVALID outcome produces six failures before A can settle. The
panic occurs with the native-copy patch as well as the checkpoint-only adapter.
Later writes can make a stale cursor numerically fit again; that must not be taken
as proof that it identifies the correct rollback history.

The superseded runner, reference example and incomplete native-copy patch are
archived outside the active Foundry tree; the [evidence note](../../experiments/evm2-live-state/README.md)
records their location and reference-build requirements. The newer checkpoint
prototype described below fixes these six candidate panics. This historical
reproduction describes the missing contract; it is not a current failure claim
against that newer prototype.

## Required capabilities

| ID | Required behavior | Evidence / acceptance boundary |
| --- | --- | --- |
| E2-01 | Capture reusable native live state and restore it even after a child has reverted away the captured history. Preserve account flags, storage originals/current values, access warmth, transient state and reversible history. | Checkpoint-only restoration panics for returned-child snapshots after child revert/halt. Native copying fixes those particular cases. Accepted overlay/backing DB capture remains a separately coordinated responsibility; the experiment does not test it. |
| E2-02 | Coordinate state replacement with every active frame's later settlement. Preserve correct undo boundaries for child and ancestor success, revert and halt, including checkpoints newer than the restored snapshot. | The original native-copy experiment panicked on immediate child revert/halt; the newer local checkpoint prototype resolves those cases. This is the principal missing primitive. Numeric cursor validation or clamping cannot establish history identity. |
| E2-03 | Allow the embedding application to retain current journal logs across explicit snapshot restoration, while ordinary frame rollback still follows its log boundary. Keep inspector-recorded diagnostics independent. | Same-depth successful restoration loses a log with ordinary rollback; the native patch's explicit retain policy matches the reference. Fixing log policy alone does not resolve E2-02. |
| E2-04 | Restore state without rewinding or invalidating the actual execution continuation: active frames, depth/call identity, stack, memory, PC and already-consumed gas remain meaningful. Subsequent storage access must work even if its executing account was absent from the captured overlay. | This is a required integration invariant. The experiment exposes account/depth failures in the Foundry reference, not corresponding confirmed evm2 defects. Audit evm2's frame/host invariants when implementing E2-02. |
| E2-05 | Keep ordinary execution native and avoid whole-state copying/conversion at each CALL/return. Make additional bookkeeping and explicit capture/restore costs measurable. | Architectural/performance requirement, not a benchmark result. The prototype copies only at explicit snapshot operations; performance and scaling have not been measured. |

These are capability requirements, not mandatory method names. An opaque native
snapshot type plus a coordinated replacement operation is one possible surface.
Stable checkpoint handles, explicit rebasing, or an alternative history model are
implementation options for E2-02. None is selected or performance-validated yet.
Exposing private fields or an unrestricted state setter would leave the critical
settlement obligation with the caller and would not by itself satisfy the request.

## Semantics to agree before implementing E2-02

The simple reproducer already proves that restoration and outstanding checkpoints
must be coordinated. It does not settle every cross-depth undo rule. In particular:

- If A snapshots at value 11, writes 22, enters B, and B restores 11 then reverts,
  should A observe 11 or 22? Define whether B's rollback can undo the restoration
  itself, independently of undoing writes made after restoration. This exact
  history divergence is not covered by the current matrix.
- When A later reverts, specify which pre-A history remains authoritative. Include
  snapshots captured before A and snapshots captured inside already-returned frames.
- Define the lifetime and identity of reusable snapshots across transaction/state
  replacement boundaries. Invalid combinations must not cause partial restoration
  followed by a panic or silent rollback into unrelated history.
- Separate state restoration from gas already consumed. Refund counters and future
  SSTORE original-value accounting need explicit tests; do not assume cloning state
  proves their correctness. Amsterdam state-gas/reservoir behavior remains untested.

Foundry's intended answers belong in ST-01/G-01 of the migration specification.
Agree them with evm2 maintainers, then express them as native regression tests.
Do not use the current reference's accidental OOG or depth panic as the contract.

## Acceptance tests for the requested change

Start with engine-local regressions that reproduce the callback-time replacement
without depending on Foundry, then run the differential harness:

1. Parent capture → child restore → immediate REVERT and INVALID: no panic; exact
   post-child state; parent continuation; parent success/revert/halt; subsequent tx.
2. Repeat with writes and logs after restoration, plus differing history lengths
   where an obsolete cursor is still in bounds. Assert values, not only no-panic.
3. Same-depth capture/restore: persistent/transient state and warmth restored;
   diagnostic logs retained separately from journal logs and ancestor rollback.
4. Capture inside B → B success/revert/halt → restore from A: snapshot remains
   reusable according to the caller's retention policy; later settlement is valid.
5. Cross-depth capture before the executing account is loaded, with and without
   preloading; tracing enabled; actual call depth and continuation remain coherent.
6. Ordinary CALL/CREATE/precompile success/revert/halt retain existing behavior when
   restoration is unused. Audit all rollback paths, not only the CALL reproducer.
7. After correctness: measure ordinary-call overhead and explicit snapshot costs
   across state sizes/depths. Reject an implementation that copies all state on
   ordinary frame entry/return. Report measured costs; no numerical budget is agreed.

The existing 54 cases are necessary evidence, not the complete acceptance suite.
Items involving divergent history, CREATE/precompiles, refunds and cost scaling
are additional tests to implement, not tests already passed.

## Foundry ownership and follow-up assessment

Foundry retains snapshot IDs/deletion rules, fork identity and persistence policy,
backing database orchestration, cfg/block and selected transaction overrides,
cheatcode companion state, sticky assertion failures and diagnostic presentation.

Two reference defects need separate treatment: restoring an absent executing
account currently causes REVM's assumed-present storage path to report OutOfGas;
restoring journal depth violates Foundry's trace-depth assertion. With tracing
turned on, 36 cross-depth reference cases panic. These are not evm2 change requests.

Fork switching, isolated child transactions, live environment/configuration
replacement and transaction replay need follow-up API assessments. We have not
executed their evm2 experiments and cannot yet claim a complete missing-API list
for them. In particular, this document does not request that evm2 implement a
Foundry multifork session or clone a dynamic backing database.

The immediate upstream discussion is E2-01 through E2-04, centered on E2-02's
settlement contract. The snapshot gate blocks adoption of the tested mechanism;
it does not demonstrate that the wider evm2 migration is infeasible.

## Local implementation branch (2026-09-18)

The fix is prepared in `~/sources/evm2` on `mablr/live-state-snapshots`, commit
`04179271d782f6189809c70d8e6a9b1d6782cf6c`, based on `main` at `0a5314e`.
It is committed locally; nothing has been pushed or submitted. Foundry still uses
its original unpatched dependency pin.

The branch adapts the isolated prototype to the newer storage-pooling code. Registered
active-frame identities preserve captured ancestor undo boundaries and rebase newer
frames to the restored boundary; returned frames are not resurrected. Ordinary
entry/settlement performs no full-state copy. Explicit capture/restore copies native
state plus O(depth) frame bookkeeping; there is no measured performance claim.

Validation on this branch: 508 library tests passed; the final nine snapshot tests
passed after test-helper cleanup; package all-feature/all-target strict Clippy,
formatting and warning-clean rustdoc passed. The current-head differential run gives
45/54 exact matches and no candidate panics. The nine known reference account-loading
differences remain failures. Full-workspace Clippy is blocked by missing LLVM 22.
The checkout's `docs/live-state-snapshots.md` documents semantics and API limits.

Snapshots belong to one state and transaction lifecycle. Cross-state/transaction
restoration is rejected before mutation. Restoration invalidates public raw
checkpoints; only engine-managed CALL/CREATE/precompile scopes coordinate automatically.
`StateCheckpoint::new` becomes crate-private: callers obtain generation-bound tokens
through `State::checkpoint`. Custom-handler scopes still need a public coordinated
API. The base EIP-7702 handler's raw checkpoint is only rolled back before message
execution, so it does not span a restoration callback.

This is a review-ready bounded implementation, not full Foundry snapshot acceptance.
Setup/test snapshot lifetime, fork/backend replacement, isolation, refund/state-gas
behavior and performance remain open. E2-02 has a tested candidate mechanism;
upstream API agreement and Foundry integration have not been completed. The earlier
prototype/report remain separately archived under
`/Volumes/Stockage/dev-cache/evm2-live-state-agent/`.


## Native cheatcode follow-up assessment

Native cheatcode integration exposes further requirements beyond snapshots.
These are requests to assess the supported extension surface, **not confirmed
consensus bugs or proof that engine changes are the only implementation option**.

| ID | Required behavior | Evidence / next decision |
| --- | --- | --- |
| E2-06 | Controlled account overrides with Foundry's rollback policy, distinct from ordinary journaled balance/nonce writes. | The retained `AccountMutation.t.sol` probe passes on preserved REVM: parent sets balance/nonce to 11/7, child overrides to 99/8 then reverts, parent still reads 99/8. Native journaled setters would undo the overrides. Decide an explicit override API or adapter policy; test ancestor rollback, prior ordinary writes, top-level failure, deal cleanup and snapshots before enabling. |
| E2-07 | Callback-time changes to selected configuration/transaction fields, visible to continued execution with defined persistence. | Source assessment: chainId mutates Foundry cfg and txGasPrice mutates live tx state; native version/config mutation is guarded during execution and transaction context is borrowed. Block replacement already works for the implemented subset. Assess a narrow supported mutation surface; runtime parity for cfg/tx overrides has not been demonstrated. |
| E2-08 | Preserve access to pre-settlement gas when an inspector rewrites failure to success. | Terminal interpreter counters are captured in Foundry for tested cases. Failures before interpreter entry have no such capture; assess a raw outcome or supported settlement/rewrite hook. See the detailed boundary below. |
| E2-09 | Preserve the original call operand separately from storage context and EIP-7702-resolved code identity. | Prague reference probe matches an expectation on the delegated-call operand. Native message loses that operand for DELEGATECALL/CALLCODE; guarded pending an identity-preserving hook/adapter. |
| E2-10 | Checked transfer behavior for synthetic balances, including recipient overflow. | Native `State::transfer` saturates recipient addition; REVM reports `OverflowPayment`. Mock path rejects before mutation. Assess checked adapter logic before requesting an engine API change. |

E2-06 evidence is a Cancun REVM run, not native acceptance. In particular, it does
not justify making all account mutations irreversible: store/etch have tested normal
frame rollback, and deal has separate top-level cleanup. E2-07 needs focused probes
before prescribing an upstream signature. Both capabilities remain explicitly
unsupported in native Foundry. Neither prevents further independent milestone 2
work on assertions, expectations or inspector lifecycle.

The existing snapshot branch remains transaction-scoped. Using it for setup/test
snapshots requires a separate lifetime design covering committed backend state and
Foundry companion state; removing its transaction-identity check alone is unsafe.


### E2-08: outcome rewriting and gas visibility

A further source-backed boundary appeared during revert-expectation integration:
`execute_message_impl` settles gas before `call_end`/`create_end`, dropping failure
refunds and burning exceptional remaining gas. Foundry's expected-revert handler
rewrites the status to success and historically retains those original counters.
The Cancun fixture demonstrated both differences (INVALID and storage-clear/revert).

Foundry can preserve interpreter-terminal counters in `step_end`, keyed by frame
depth and consumed at its end callback. This adapter now matches the tested cases;
it is **not evidence that an engine patch is mandatory for interpreted failures**.
For failures before interpreter entry, no such capture occurs. Request/assess access
to the pre-settlement result or a supported outcome-rewrite boundary, with explicit
rollback/refund/state-gas ordering. Native Foundry currently rejects these expected
failures explicitly. Include failed precompile and CREATE preparation/code-deposit
cases before declaring the callback surface sufficient; Amsterdam remains untested.


### E2-07 detail: prank origin overrides (native port assessment)

At pinned `2c8b67f`, `Interpreter::tx_env()` in
`crates/evm2/src/interpreter/runtime.rs` returns `&TxEnv<T>` with the frame lifetime.
`instructions/env.rs::origin` reads `cx.state.tx().origin` directly. The mutable
`Message` supplied to `Inspector::call/create` contains `caller`, but no transaction
origin. No safe live origin setter was found on this inspected surface. Mutating
behind the shared reference is not an acceptable adapter.

Required contract: a prank may replace the origin for the selected child and its
nested calls/creates, then restore the previous origin when that selected call
returns, including revert/halt. Nested pranks need a stack of prior origins;
cheatcode/console calls must not consume the one-shot prank. Persistent pranks must
retain their configuration across setup/test transactions while each invocation's
origin override has the correct frame lifetime. Fee-paying transaction identity
must not accidentally change. Add differential tests for all these transitions.

This is a **missing safe extension surface at the inspected pin**, not a claim that
an upstream change is the sole possible design. Opcode-result interception would
need separate correctness/performance assessment and is not implemented. Native
Foundry rejects origin-changing overloads even when caught; the retained
`NativePrankOriginUnsupportedTest` and CLI snapshot verify that boundary.

Sender-only and delegate pranks now use mutable messages successfully. CREATE and
CREATE2 require recalculating `destination` after changing `caller`, using the new
sender's current nonce (CREATE) or salt/initcode hash (CREATE2). evm2 exposes
`derive_create_destination`; this is **Foundry adapter work, not a new API gap**.
Eleven positive Cancun prank tests, including setup persistence, match REVM gas.

For E2-06, `AccountHandle::get_or_insert` is not an unjournaled escape hatch: it calls
`present_mut`, which records a revert snapshot, just like the balance/nonce setters.
The controlled override policy remains unresolved. E2-08 also remains open for
failures without captured interpreter counters; the prank port introduces no new
requirement to change snapshot APIs E2-01 through E2-05.


### E2-09: original operand lost during EIP-7702 resolution

At `2c8b67f`, `interpreter/instructions/system.rs::load_acc_and_calc_gas` resolves
EIP-7702 code and returns `resolved_code_address`. `prepare_call` constructs the
message with that address. For DELEGATECALL/CALLCODE, `destination` is the parent's
storage context, so neither field retains the original `to` operand. In contrast,
Foundry's inspector matches expectations/mocks using REVM `CallInputs.bytecode_address`.

`Calls.t.sol::DelegatedCallIdentityTest::testDelegatedOperandIdentity` etches a
designation on authority A pointing to implementation B, registers
`expectDelegateCall(A, data)`, and delegatecalls A. It passes on preserved REVM
at Prague. Native Foundry explicitly fails with a delegated-identity capability
error; the regression locks down this guard, not native parity.

Reproduce using either binary with `test --root crates/forge/tests/fixtures/evm2
--use 0.8.35 --no-isolate --evm-version prague --match-contract
'^DelegatedCallIdentityTest$'`. Inspected source, reference result and native guard
are the evidence; this is not a consensus-execution bug. For ordinary CALL/STATICCALL,
`destination` can recover the operand; the general hook still needs all three
identities. Request/assess an original-target field or earlier hook; avoid reconstructing
it from already-consumed parent stack operands. Include CALL, STATICCALL, DELEGATECALL,
CALLCODE, prank interaction and delegated precompile targets in acceptance tests.

### E2-10: recipient balance overflow in synthetic transfers

At the same pin, `evm/state/mod.rs::State::transfer` subtracts the sender balance
then uses `saturating_add` for the recipient. REVM's `JournalInner::transfer_loaded`
uses checked addition and returns `TransferError::OverflowPayment`. The native stop
enum contains `OverflowPayment`, but the state helper returns only a success boolean
(or database error). Do not infer matching rollback semantics from similar signatures.

Such balances are unreachable with Ethereum's real supply but relevant to unrestricted
Foundry account overrides. The native mock adapter rejects overflow before transfer;
`native::tests::overflowing_mock_transfer_is_rejected_before_mutation` asserts both
balances and queue length are unchanged. This proves the adapter guard, not a complete
differential overflow contract. A source-level semantic mismatch is established;
reference post-error warmth/touch/rollback and ancestor behavior still need probes.
Checked adapter code may suffice: this is not yet a mandatory upstream patch request.

### Boundaries resolved by the call/mock adapter

Ordinary call expectations need no new API: match before prank/mock interception and
verify counts at root completion, preserving earlier revert diagnostics. Mock overrides
skip engine execution, so the adapter performs native checkpointed value transfer and
reverts it on mocked failure; unsuccessful transfer does not consume the response queue.
Mocked outcomes already carry their original gas and can satisfy expectRevert without
an interpreter. E2-08 remains open for other uncaptured failure paths. Log recording
retains diagnostics through enclosing rollback without changing journal logs.

### Creation-expectation adapter assessment

The pinned engine clears `MessageResult.created_address` on constructor failure
(`Evm::finish_create`). Reference Foundry can match an empty-code creation after a
constructor revert, but not after a pre-frame CREATE2 collision. The native adapter
now tracks constructor entry by depth and uses `Message.destination` for an entered
failed constructor; this does not require a new engine API. `Creates.t.sol` contains
the constructor-revert obligation and collision negative probe. Code-deposit/halt
and other early-rejection combinations remain to be validated. Do not infer their
acceptance from the successful-address field or turn this solved identity slice
into an upstream blocker. E2-08 raw-gas requirements remain independent.

### Event-expectation adapter assessment

The native log hook has no interpreter argument. For opcode LOG0–LOG4, the adapter
identifies the emitting depth during step, records an immediate matching error at
log, stops the frame with Revert during step_end and supplies the error bytes at
frame end. This preserves native rollback and raw gas without adding an engine API.
`Emits.t.sol` checks caught count-zero rollback and expected-revert handling. A
separate caught end-mismatch probe confirms reference behavior retains already
settled child state. Non-opcode/system/precompile logging is not certified by these
probes; the reference log-only callback also cannot immediately stop an interpreter.
