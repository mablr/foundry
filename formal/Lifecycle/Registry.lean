/-!
# Fork registry state machine

An abstract model of `ExecutionLifecycle` and `ForkRegistry`
(`crates/evm/core/src/evm/lifecycle.rs`, `crates/evm/core/src/evm/fork.rs`): which contexts exist,
which one is active, the accepted state each inactive one resumes with, the running state of the
active one, persistent accounts and user-visible snapshots.

The Rust reference model in `crates/evm/core/src/evm/tests/lifecycle_model.rs` mirrors every
definition here one-to-one (`selectFork` is `Model::select_fork`, `inheritLocal` is
`Model::inherit_local`, and so on) and is checked against the real lifecycle step by step.

Abstractions:

* An account has two observable fields, its balance and one storage slot (`Field`). Nonce, code
  and further slots behave like the slot.
* Backing databases are functions `backing seed loc`, fixed per context. Fork RPCs are not modeled.
* Overlays (`Cache` in Rust) are partial maps from locations to values. Writes load the whole
  account first, so an overlay holds an account's balance and slot together, except for the slots
  that leaving the local context copies into forks (`inheritLocal`).
* Account creation, warmth, transient storage, gas originals, logs, environments and network
  extension state are not modeled. Neither are call frames: every operation is a cheatcode of the
  running transaction or a transaction boundary (`endTx`).
-/

namespace Lifecycle

/-- An account. -/
abbrev Addr := Nat

/-- An observable account field: the balance or storage slot zero. -/
inductive Field where
  | bal
  | slot
  deriving DecidableEq, Repr

/-- A location: an account field. -/
abbrev Loc := Addr × Field

/-- A context: `none` for the local context, `some i` for fork `i`. -/
abbrev Ctx := Option Nat

/-- A set of accounts, as its indicator. -/
abbrev AddrSet := Addr → Bool

/-- A partial map of locations to values, layered over a backing database. Models `Cache`. -/
abbrev Overlay := Loc → Option Nat

/-- `b` layered over `a`: `b`'s values take precedence. Models `Cache::merge`. -/
def Overlay.over (a b : Overlay) : Overlay := fun l => (b l).or (a l)

/-- A context that is not running. Models `Parked`.

`ran` models `cache.is_some()`; it is only observed on the local context, through
`inheritLocal`. -/
structure Parked where
  /-- The backing database. -/
  seed : Nat
  /-- The accepted state, `Parked::cache`. -/
  overlay : Overlay
  /-- Whether the context has an accepted state. -/
  ran : Bool
  /-- Accounts loaded before the context was last suspended, `Parked::loaded`. -/
  loaded : AddrSet

/-- A context that never ran, over `seed`. Models `Parked::new`. -/
def Parked.fresh (seed : Nat) : Parked := ⟨seed, fun _ => none, false, fun _ => false⟩

/-- The running state of the active context: evm2's `State`. -/
structure Running where
  /-- The accepted overlay the running transaction started from or a switch installed. -/
  accepted : Overlay
  /-- The running transaction's in-flight changes and loads. -/
  inflight : Overlay
  /-- Accounts the running transaction loaded on this context. -/
  txLoaded : AddrSet

/-- An empty running state over `accepted`. -/
def Running.over (accepted : Overlay) : Running := ⟨accepted, fun _ => none, fun _ => false⟩

/-- A user-visible snapshot. Models `LifecycleSnapshot`: the Rust snapshot clones the whole
registry, but a revert only uses the active context's entry, which is all this keeps. -/
structure Snap where
  active : Ctx
  parked : Parked
  running : Running

/-- The lifecycle. Models `ExecutionLifecycle` with its `ForkRegistry` and `StateSnapshots`. -/
structure St where
  /-- `ForkRegistry::local`. -/
  home : Parked
  /-- `ForkRegistry::forks`. -/
  forks : Nat → Option Parked
  /-- `ForkRegistry::next_id`. -/
  nextFork : Nat
  /-- `ForkRegistry::active`. -/
  active : Ctx
  /-- `ForkRegistry::persistent`. -/
  persistent : AddrSet
  /-- The running EVM's state. -/
  running : Running
  /-- `StateSnapshots`. -/
  snaps : Nat → Option Snap
  /-- The next snapshot ID. -/
  nextSnap : Nat

/-- The initial lifecycle over the local database `seed`. Models `ExecutionLifecycle::new`
followed by `create_evm`. -/
def St.init (seed : Nat) : St :=
  ⟨Parked.fresh seed, fun _ => none, 0, none, fun _ => false, Running.over (fun _ => none),
    fun _ => none, 0⟩

section

-- The backing databases.
variable (backing : Nat → Loc → Nat)

namespace St

/-- A context. Models `ForkRegistry::context`. -/
def parked (s : St) : Ctx → Option Parked
  | none => some s.home
  | some i => s.forks i

/-- Replaces a context. Models `ForkRegistry::context_mut`. -/
def setParked (s : St) (c : Ctx) (p : Parked) : St :=
  match c with
  | none => { s with home := p }
  | some i => { s with forks := fun j => if j = i then some p else s.forks j }

/-- The backing database of the active context. -/
def activeSeed (s : St) : Nat := ((s.parked s.active).map Parked.seed).getD 0

/-- The running value of `l` on the active context. Models `State::account_info_untracked` and
`State::storage_slot_untracked`. -/
def read (s : St) (l : Loc) : Nat :=
  (s.running.accepted.over s.running.inflight l).getD (backing s.activeSeed l)

end St

/-- The accepted value of `l` in an inactive context. Models `Parked::accepted_db`. -/
def Parked.read (p : Parked) (l : Loc) : Nat := (p.overlay l).getD (backing p.seed l)

/-- What context `c` holds for `l`: the running value for the active context, the accepted one
otherwise, and `0` for a context that does not exist. -/
def St.view (s : St) (c : Ctx) (l : Loc) : Nat :=
  if c = s.active then s.read backing l else ((s.parked c).map (fun p => p.read backing l)).getD 0

/-- Loads account `a` in the running transaction: its balance and slot join the in-flight
overlay with their current values. Models `State::account` and `State::storage_slot`. -/
def St.load (s : St) (a : Addr) : St :=
  { s with
    running :=
      { s.running with
        inflight := (fun l => if l.1 = a then some ((s.running.inflight l).getD (s.read backing l))
          else s.running.inflight l)
        txLoaded := fun b => b = a || s.running.txLoaded b } }

/-- Writes `v` to `l` in the running transaction, after loading its account. Models a cheatcode
such as `vm.deal` or `vm.store` as a journaled write. -/
def St.write (s : St) (l : Loc) (v : Nat) : St :=
  let s := s.load backing l.1
  { s with
    running := { s.running with inflight := fun k => if k = l then some v else s.running.inflight k } }

/-- Reads `l` on the running state without loading it. The read caches a value it takes from the
backing database in the accepted state, as `CacheDB` does under `State::account_info_untracked` and
`State::storage_slot_untracked`, so a peeked persistent location follows the next switch. -/
def St.peek (s : St) (l : Loc) : St :=
  if (s.running.inflight l).isSome then s
  else
    { s with
      running := { s.running with
        accepted := fun k => if k = l then some ((s.running.accepted l).getD (backing s.activeSeed l))
          else s.running.accepted k } }

end

/-- Makes `target` start from what the running transaction loaded in the local context `home`:
the slots of the accounts loaded there. Models `Parked::inherit_local` without created
accounts; a missing slot is copied as zero, as `unwrap_or_default` does. -/
def inheritLocal (target home : Parked) : Parked :=
  if home.ran then
    { target with
      overlay := (fun l =>
        if l.2 = .slot ∧ home.loaded l.1 then some ((home.overlay l).getD 0) else target.overlay l)
      loaded := fun a => target.loaded a || home.loaded a }
  else target

/-- Copies persistent accounts' accepted state from `source` over `target`. Models
`ForkRegistry::merge_persistent`. -/
def mergePersistent (persistent : AddrSet) (source target : Overlay) : Overlay :=
  fun l => if persistent l.1 then (source l).or (target l) else target l

namespace St

/-- What a suspended context carries into the next one: its accepted state and loaded accounts. -/
structure Suspended where
  overlay : Overlay
  loaded : AddrSet

/-- The running state, committed: what suspending the active context accepts. -/
def committed (s : St) : Overlay := s.running.accepted.over s.running.inflight

/-- The active context after suspending it. -/
def suspendedParked (s : St) : Parked :=
  let p := (s.parked s.active).getD (Parked.fresh 0)
  { p with
    overlay := s.committed
    ran := true
    loaded := fun a => p.loaded a || s.running.txLoaded a }

/-- Accepts the running state into the active context. Models `ExecutionLifecycle::suspend`. -/
def suspend (s : St) : St × Suspended :=
  let p := s.suspendedParked
  (s.setParked s.active p, ⟨p.overlay, p.loaded⟩)

/-- Installs the active context as the running one, carrying persistent accounts from `src` and,
with `carry`, the suspended context's loaded accounts. Models `ExecutionLifecycle::resume`. -/
def resume (s : St) (src : Suspended) (carry : Bool) : St :=
  let p := (s.parked s.active).getD (Parked.fresh 0)
  let loaded := fun a => p.loaded a || (carry && src.loaded a) || (s.persistent a && src.loaded a)
  let s' := s.setParked s.active { p with overlay := fun _ => none, ran := false, loaded }
  { s' with running := Running.over (mergePersistent s.persistent src.overlay p.overlay) }

/-- Every fork inherits from the local context. -/
def inheritAll (s : St) : St := { s with forks := fun j => (s.forks j).map (inheritLocal · s.home) }

/-- `vm.createFork` over `seed`. Models `ExecutionLifecycle::create_fork`. -/
def createFork (s : St) (seed : Nat) : St :=
  { s with
    forks := (fun j => if j = s.nextFork then some (inheritLocal (Parked.fresh seed) s.home)
      else s.forks j)
    nextFork := s.nextFork + 1 }

/-- `vm.selectFork i`. Models `ExecutionLifecycle::select_fork`; an unknown fork leaves the state
unchanged, as the Rust error does. -/
def selectFork (s : St) (i : Nat) : St :=
  if (s.forks i).isNone ∨ s.active = some i then s
  else
    let (t, src) := s.suspend
    let t := if s.active = none then t.inheritAll else t
    resume { t with active := some i } src false

/-- `vm.makePersistent`. -/
def makePersistent (s : St) (a : Addr) : St :=
  { s with persistent := fun b => b = a || s.persistent b }

/-- `vm.revokePersistent`. -/
def revokePersistent (s : St) (a : Addr) : St :=
  { s with persistent := fun b => b ≠ a && s.persistent b }

/-- `vm.snapshotState`. Models `ExecutionLifecycle::snapshot`. -/
def snapshot (s : St) : St :=
  { s with
    snaps := (fun j => if j = s.nextSnap then
      some ⟨s.active, (s.parked s.active).getD (Parked.fresh 0), s.running⟩ else s.snaps j)
    nextSnap := s.nextSnap + 1 }

/-- Restores snapshot `sn`, kept or removed as `snaps'` says. Models the body of
`ExecutionLifecycle::revert`. -/
def restore (s : St) (sn : Snap) (snaps' : Nat → Option Snap) : St :=
  -- The running context keeps its accepted state and drops the transaction's in-flight changes.
  let s := if sn.active = s.active then s
    else s.setParked s.active
      { (s.parked s.active).getD (Parked.fresh 0) with
        overlay := s.running.accepted
        ran := true }
  { s.setParked sn.active sn.parked with
    active := sn.active
    running := sn.running
    snaps := snaps' }

/-- `vm.revertToState` (`keep`) or `vm.revertToStateAndDelete`. Models
`ExecutionLifecycle::revert`. -/
def revertTo (s : St) (j : Nat) (keep : Bool) : St :=
  match s.snaps j with
  | none => s
  | some sn => s.restore sn (if keep then s.snaps else fun k => if k = j then none else s.snaps k)

/-- `vm.deleteStateSnapshot`. -/
def deleteSnapshot (s : St) (j : Nat) : St :=
  { s with snaps := fun k => if k = j then none else s.snaps k }

/-- `vm.rollFork i` onto `seed`. Models `ExecutionLifecycle::roll_fork` and, for an inactive
fork, `ExecutionLifecycle::replace_inactive`, without a replayed cache. -/
def rollFork (s : St) (i : Nat) (seed : Nat) : St :=
  if (s.forks i).isNone then s
  else
    let fresh := inheritLocal (Parked.fresh seed) s.home
    if s.active = some i then
      let (t, src) := s.suspend
      resume (t.setParked (some i) fresh) src true
    else s.setParked (some i) fresh

/-- The end of a transaction: commits it and accepts it into the active context, which forgets
what it loaded. Models `State::commit_transaction`, `ExecutionLifecycle::park` and
`ExecutionLifecycle::create_evm`. -/
def endTx (s : St) : St :=
  let p := (s.parked s.active).getD (Parked.fresh 0)
  let s' := s.setParked s.active
    { p with
      overlay := s.committed
      ran := true
      loaded := fun _ => false }
  { s' with running := Running.over s.committed }

end St

/-- The operations a test can invoke. -/
inductive Op where
  | createFork (seed : Nat)
  | selectFork (i : Nat)
  | setBalance (a : Addr) (v : Nat)
  | setSlot (a : Addr) (v : Nat)
  | makePersistent (a : Addr)
  | revokePersistent (a : Addr)
  | snapshot
  | revertTo (j : Nat) (keep : Bool)
  | deleteSnapshot (j : Nat)
  | rollFork (i seed : Nat)
  | endTx
  | peek (l : Loc)

/-- One step of the state machine. Models `Model::step` and, through it, the real lifecycle. -/
def St.step (backing : Nat → Loc → Nat) (s : St) : Op → St
  | .createFork seed => s.createFork seed
  | .selectFork i => s.selectFork i
  | .setBalance a v => s.write backing (a, .bal) v
  | .setSlot a v => s.write backing (a, .slot) v
  | .makePersistent a => s.makePersistent a
  | .revokePersistent a => s.revokePersistent a
  | .snapshot => s.snapshot
  | .revertTo j keep => s.revertTo j keep
  | .deleteSnapshot j => s.deleteSnapshot j
  | .rollFork i seed => s.rollFork i seed
  | .endTx => s.endTx
  | .peek l => s.peek backing l

end Lifecycle
