# evm2 capabilities needed for Foundry live-state replacement

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
