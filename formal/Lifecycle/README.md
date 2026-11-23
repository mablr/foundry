# Lifecycle: forks, snapshots and persistent accounts

A Lean model of the fork, snapshot and persistent-account state machine of `crates/evm/core`
(`ExecutionLifecycle`, `ForkRegistry`, `PersistentWrite::apply_all`, `SnapshotRestores`), the
contract it implies, proofs of that contract, and Rust tests that keep the implementation in
conformance with the model.

```bash
./check.sh Lifecycle             # build and proofs; fails on `sorry`
lake build Lifecycle.Axioms      # prints the axioms of every headline theorem
cargo +nightly test -p foundry-evm-core --lib _model   # conformance tests
```

## The contract

A test runs in *contexts*: the local one and one per fork. One context is active; every other one
keeps an *accepted state*, an overlay of the values it has read or written over its backing
database. The active context runs the current transaction, whose writes are *in flight* until a
fork switch or the end of the transaction accepts them.

- **Fork switches** (`vm.selectFork`) accept the running transaction's state into the source
  context and resume the target with its own accepted state. A persistent account carries every
  location the source holds, its balance, nonce, code and the slots it loaded or read, into the
  target. A location the source never loaded is read from the target's database: persistence is
  of cached state, not of the source's database. Every other account keeps each context's state.
- **Leaving the local context seeds forks.** Each time the running transaction leaves the local
  context, every fork takes the slots of the accounts the transaction loaded there, and the
  accounts it created there. Forks created later inherit the same seeds. The local context keeps
  its seeds until it ends a transaction itself.
- **Snapshots** (`vm.snapshotState`) capture the active context's running state, in-flight
  writes included, under an ID that is never reused.
- **Reverts** (`vm.revertToState`) make the snapshot's active context active again with exactly
  that state. Every other context, the persistent set, forks created since and other snapshots
  are left as they are, with one exception: if the running context is another one, it keeps its
  accepted state but drops what the transaction wrote on it since it last became active. Writes
  an earlier switch away from it accepted stay.
- **Rolling** the active fork (`vm.rollFork`) restarts it from the new database, keeping
  persistent accounts and the local seeds; rolling an inactive fork restarts it and touches
  nothing else.
- **Ending a transaction** changes no context's state; it forgets what the transaction loaded.
- **Persistent writes** replay after each enclosing revert. Balance and nonce deltas since an
  account's last absolute write are summed and saturated once into `[0, max]`; the combined change
  takes the highest replay mode and replays to the same value over the same pre-state. Other writes
  apply in order, on fields the deferred balance and nonce writes never touch.
- **Restorations inside frames** are owned by the frames open at the time. Each frame retains only
  the first restoration since it started, so at most one per open frame. A frame that fails
  reinstates it and replays the ledger recorded at the restoration followed by the writes recorded
  since; a frame that succeeds keeps the restoration.

Two parts of this contract are easy to trip over and are pinned by named Rust tests:
`leaving_local_again_reseeds_forks` (after a revert to a snapshot of the local context, selecting a
fork again overwrites what the test wrote on it to slots loaded on the local context) and
`revert_from_another_fork_drops_writes_since_entry` (whether a write on a fork survives a revert
to a snapshot of another fork depends on whether a switch accepted it).

## Files

| File | Contents |
| --- | --- |
| `Registry.lean` | The state machine: `St`, `Parked`, `Running`, `Snap` and the operations `createFork`, `selectFork`, `write`, `peek`, `makePersistent`, `revokePersistent`, `snapshot`, `revertTo`, `deleteSnapshot`, `rollFork`, `endTx`, with `St.step` over `Op`. |
| `RegistryProofs.lean` | Well-formedness `St.WF` and its preservation by every operation (`WF.step`, `WF.run`). |
| `Contract.lean` | The headline theorems over the state machine. |
| `PersistentWrite.lean` | `PersistentWrite::apply_all` and its theorems. |
| `Restores.lean` | `SnapshotRestores` and its theorems. |
| `Axioms.lean` | `#print axioms` for every headline theorem. |

## Proved theorems

All depend on at most `propext`, `Classical.choice` and `Quot.sound`; there are no modelling
axioms.

| Theorem | Statement |
| --- | --- |
| `St.WF.run` | Every reachable state is well-formed: forks are numbered from zero and never removed, the active context and every snapshot's active fork exist (the `expect` in `revert`), snapshot IDs are below the next one, and the local context has no loaded accounts while it is active, nor does a snapshot of it. |
| `selectFork_carries_persistent` | After a switch, a persistent account holds at every location the source accepted or loaded the value it had there. |
| `selectFork_isolates` | A switch between forks leaves what every context holds for a non-persistent account unchanged. |
| `selectFork_from_local_seeds_forks` | Leaving the local context sets the slot of every account the transaction loaded there in every other fork, whatever the fork held. |
| `snapshot_spec` | A snapshot gets an unused ID and records the active context's running values. |
| `revertTo_restores` | A revert makes the snapshot's context active with the recorded running state and values. |
| `revertTo_frame` | A revert leaves every context other than the snapshot's and the running one as it is. |
| `revertTo_leaves_running` | The running context, if another one, keeps its accepted state and drops its in-flight writes. |
| `revertTo_keeps` | A revert keeps the persistent set, every fork and the next IDs, and removes only the reverted snapshot unless kept. |
| `rollFork_active_resets` | Rolling the active fork reads every non-persistent, unseeded location from the new database. |
| `rollFork_active_persistent` | Rolling the active fork keeps persistent accounts at every location it held. |
| `rollFork_inactive` | Rolling an inactive fork restarts it from its seeds and leaves every other context as it is. |
| `endTx_view` | Ending a transaction changes no context's state. |
| `createFork_fresh` | A fork created while the local context is active reads its database, under a fresh ID; the stale accepted state `create_fork` reads from the active local context is never used. |
| `write_view`, `peek_view` | A write changes exactly its location on the active context; a peek changes nothing. |
| `applyAll_le` | `apply_all` leaves balances and nonces in range. |
| `accOf_absolute` | Writes before an account's last absolute write do not matter. |
| `accOf_mode` | The combined change takes the highest replay mode. |
| `applyAll_applied` | Replaying the returned changes over the same pre-state leaves the same values ("a later replay keeps the saturation"). |
| `applyAll_eq_applySeq` | Without intermediate saturation, combining equals applying the writes one by one; an `example` shows they differ otherwise. |
| `applyAccount_separates` | Each field depends only on the writes of its kind in recorded order, so applying balance and nonce writes after the others, as `apply_all` does, is sound. |
| `R.Inv.bound` | Retained states never outnumber open frames, for properly nested frames (`R.Inv.frameStart`, `capture`, `acceptWrites`, `frameEnd`). |
| `frameEnd_after_capture` | A frame that fails after its first restoration reinstates the replaced state, with the ledger recorded at the restoration followed by the writes since, reinstated. |
| `frameEnd_success`, `frameEnd_frames` | A succeeding frame keeps its ledger; frames close in stack order. |

## Fidelity

The model is written by hand: extraction is not practical over evm2's `State`, `Arc` sharing,
hash maps and traits.

| Lean | Rust |
| --- | --- |
| `Overlay`, `Overlay.over` | `Cache`, `Cache::merge` |
| `Parked` (`overlay`, `ran`, `loaded`) | `Parked` (`cache`, `cache.is_some()`, `loaded`) |
| `Running` | evm2's `State`: accepted overlay, in-flight changes, loaded accounts |
| `Snap` | `LifecycleSnapshot`, keeping only the active context's entry, the only one `revert` uses |
| `St` | `ExecutionLifecycle` with `ForkRegistry` and `StateSnapshots` |
| `inheritLocal` | `Parked::inherit_local` |
| `mergePersistent` | `ForkRegistry::merge_persistent` |
| `suspend`, `resume` | `ExecutionLifecycle::suspend`, `resume` |
| `createFork`, `selectFork`, `snapshot`, `revertTo`, `deleteSnapshot`, `rollFork` | the `ExecutionLifecycle` methods of the same names; `rollFork` of an inactive fork is `replace_inactive` |
| `endTx` | `State::commit_transaction`, `ExecutionLifecycle::park`, `create_evm` |
| `peek` | `State::account_info_untracked`, `storage_slot_untracked` |
| `PersistentWrite.Acc`, `applyAll`, `applied` | the two loops of `PersistentWrite::apply_all` |
| `Restores.R` and its operations | `SnapshotRestores::{frame_start, capture, accept_writes, frame_end}` |

Abstractions:

- An account has a balance and one storage slot; nonce, code and other slots behave like them.
  Backing databases are fixed functions of a seed, so RPC forks, block environments and fork
  identities (`ForkInfo`) are not modeled.
- Account creation is not modeled: `inheritLocal` copies only loaded slots, and the created-account
  rules of `inherit`, `resume` and `replace_inactive` are out of scope. So are warmth, transient
  storage, gas originals (`originals`, `switched`, `restore_originals`), logs, environments,
  network extension state, `vm.transact` and isolated calls.
- Call frames are not part of the registry model: every operation is a cheatcode of the running
  transaction at the top level or a transaction boundary. `Restores` models frames separately, with
  states and writes opaque and the checkpoint rollback left out.
- `PersistentWrite` uses unbounded integers for `I512` and `i128` sums, which cannot overflow for
  fewer than `2^255` (`2^63`) writes.

## Conformance

The model is abstract, so the Rust tests check it against the implementation rather than share
code with it. Each has a reference implementation whose types and functions mirror the Lean
definitions one-to-one (`Model::select_fork` is `St.selectFork`, `Model::inherit_local` is
`inheritLocal`, `Acc::push` is `Acc.push`, ...), so a change to either side shows as a divergence:

- `crates/evm/core/src/evm/tests/lifecycle_model.rs` drives the real `ExecutionLifecycle` over
  in-memory databases (no RPC) with 5,000 pseudo-random sequences of 32 operations over three
  accounts and up to three forks, including unknown fork and snapshot IDs. After every step it
  compares each operation's result, the active selection, the fork count, the persistent set,
  live snapshots and the accepted state of every inactive context; `Op::Peek` compares the running
  state. 100,000 sequences of 40 operations also pass. Removing the write-back of the running
  context from `revert`, or making `merge_persistent` keep target slots, makes it fail in the
  first sequences.
- `crates/evm/core/src/evm/tests/persistent_write_model.rs` applies 20,000 random write lists with
  balances and nonces near both bounds to a real `State` and checks values, modes, replay
  idempotence and the order of other writes.
- `crates/evm/core/src/evm/snapshot/tests.rs` drives the real `SnapshotRestores` like the
  cheatcode inspector, with 20,000 random sequences of frames, writes, restorations and fork
  switches, comparing retained states, ledgers, the reinstated state and `kept`.

One regime restriction applies: evm2 caches what untracked reads return, and resuming a context
can cache further locations, such as the accounts whose slot originals `restore_originals`
reinstates. Which locations are cached is observable only through what a persistent account
carries. The lifecycle test therefore peeks at every location after each operation that installs a
context, which makes every location cached on entry; the theorems hold for any cache contents.

## Dropped and rescoped

- *Account creation* ("an account is marked created in context C iff the running transaction
  created it on C or holds it created"). The rules live mostly in the cheatcode inspector's
  isolated calls (`crates/evm/evm`), and modelling them would need a model of frames and nested
  transactions. Left for a follow-up with its own conformance harness.
- *Warmth cleared at `begin_transaction`* is a one-line loop with no interesting invariant beyond
  its code; dropped.
- *`ReloadDiscarded` and nested-transaction merges* (`merge_changes`) depend on evm2 journal
  internals; out of scope.

## Next steps

- Model created accounts and their carry rules (`inherit`, `resume`, `replace_inactive`), then
  extend the conformance harness with creations.
- Model the remaining cache effects of `resume` so the lifecycle test can drop its peek-on-entry
  restriction.
- Decide whether re-seeding forks on a second departure from the local context is intended; if
  not, seed each fork only once per transaction and update `selectFork_from_local_seeds_forks`.
- Prove the full ledger theorem of `SnapshotRestores` with nested restorations (`earlier`).
