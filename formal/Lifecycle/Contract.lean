import Lifecycle.RegistryProofs

/-!
# The fork, snapshot and persistent-account contract

Headline theorems over the registry model. `view s c l` is what context `c` holds for location
`l`: the running value for the active context, the accepted value for the others.
-/

namespace Lifecycle

namespace St

variable {backing : Nat → Loc → Nat}

theorem read_eq (s : St) (l : Loc) :
    s.read backing l = (s.committed l).getD (backing s.activeSeed l) := rfl

theorem view_active (s : St) (l : Loc) : s.view backing s.active l = s.read backing l := by
  simp [view]

theorem view_inactive (s : St) {c : Ctx} (hc : c ≠ s.active) (l : Loc) :
    s.view backing c l = ((s.parked c).map (fun p => p.read backing l)).getD 0 := by
  simp [view, hc]

/-! ## Fork switches -/

/-- Unfolds a switch to an existing, inactive fork. -/
theorem selectFork_eq {s : St} {i : Nat} (hi : (s.forks i).isSome) (hne : s.active ≠ some i) :
    s.selectFork i =
      let (t, src) := s.suspend
      let t := if s.active = none then t.inheritAll else t
      resume { t with active := some i } src false := by
  have : ¬((s.forks i).isNone ∨ s.active = some i) := by
    simp only [not_or]; exact ⟨by simpa [Option.isNone_iff_eq_none,
      Option.isSome_iff_ne_none] using hi, hne⟩
  simp only [selectFork, this, if_false]

@[simp] theorem selectFork_persistent (s : St) (i : Nat) :
    (s.selectFork i).persistent = s.persistent := by
  unfold selectFork; split
  · rfl
  · simp only [resume, suspend]; split <;> simp [inheritAll]

/-- **Persistent accounts follow fork switches.** After switching to fork `i`, a persistent
account holds, at every location the source context accepted or loaded, the value it had there
at the switch. Models `ForkRegistry::merge_persistent` in `ExecutionLifecycle::resume`. -/
theorem selectFork_carries_persistent {s : St} {i : Nat} (hi : (s.forks i).isSome)
    (hne : s.active ≠ some i) {a : Addr} (hp : s.persistent a = true) {f : Field} {v : Nat}
    (hv : s.committed (a, f) = some v) : (s.selectFork i).read backing (a, f) = v := by
  rw [selectFork_eq hi hne]
  simp only [suspend]
  split <;>
    simp [read, resume, Running.over, Overlay.over, mergePersistent, hp, suspendedParked, hv,
      inheritAll, setParked_forks_some]

/-- The value a persistent account had on the source context at every location it held there
is what the target context observes. -/
theorem selectFork_persistent_observed {s : St} {i : Nat} (hi : (s.forks i).isSome)
    (hne : s.active ≠ some i) {a : Addr} (hp : s.persistent a = true) {f : Field}
    (hv : (s.committed (a, f)).isSome) :
    (s.selectFork i).read backing (a, f) = s.read backing (a, f) := by
  obtain ⟨v, hv⟩ := Option.isSome_iff_exists.1 hv
  rw [selectFork_carries_persistent hi hne hp hv, read_eq, hv]; rfl

/-- **Fork switches isolate non-persistent accounts.** Switching between forks leaves what every
context holds for a non-persistent account unchanged: the source context accepts its running
value, and the target resumes with its own. -/
theorem selectFork_isolates {s : St} (h : s.WF) {i : Nat} (hi : (s.forks i).isSome)
    (hne : s.active ≠ some i) (hact : s.active ≠ none) {a : Addr} (hp : s.persistent a = false)
    (c : Ctx) (f : Field) :
    (s.selectFork i).view backing c (a, f) = s.view backing c (a, f) := by
  obtain ⟨k, hk⟩ := Option.ne_none_iff_exists'.1 hact
  obtain ⟨p, hpi⟩ := Option.isSome_iff_exists.1 hi
  obtain ⟨q, hq⟩ := Option.isSome_iff_exists.1 (h.active_forks k hk)
  have hik : i ≠ k := fun e => hne (by rw [hk, e])
  rw [selectFork_eq hi hne]
  simp only [suspend, hk, reduceCtorEq, if_false]
  by_cases hci : c = some i
  · subst hci
    simp [view, resume, read, activeSeed, parked_setParked, Running.over, Overlay.over,
      mergePersistent, hp, hpi, hik, hk, Parked.read]
  · by_cases hck : c = some k
    · subst hck
      simp [view, resume, read, activeSeed, parked_setParked, Running.over, Overlay.over,
        hp, hk, hq, Parked.read, suspendedParked, committed, Ne.symm hik]
    · cases c with
      | none => simp [view, resume, hk]
      | some m =>
        have hmi : m ≠ i := fun e => hci (by rw [e])
        have hmk : m ≠ k := fun e => hck (by rw [e])
        simp [view, resume, hk, hmi, hmk]

/-- **Leaving the local context seeds every fork.** Every switch away from the local context
copies the slot of every account the running transaction loaded there into every other fork,
whatever the fork held. Models `Parked::inherit_local` in `ExecutionLifecycle::select_fork`. -/
theorem selectFork_from_local_seeds_forks {s : St} {i : Nat} (hi : (s.forks i).isSome)
    (hact : s.active = none) {a : Addr} (htx : s.running.txLoaded a = true) {v : Nat}
    (hv : s.committed (a, .slot) = some v) {j : Nat} (hj : (s.forks j).isSome) (hji : j ≠ i) :
    (s.selectFork i).view backing (some j) (a, .slot) = v := by
  obtain ⟨p, hpj⟩ := Option.isSome_iff_exists.1 hj
  rw [selectFork_eq hi (by simp [hact])]
  simp [suspend, hact, view, resume, parked_setParked, hji, inheritAll, hpj, inheritLocal,
    suspendedParked, htx, hv, Parked.read]

/-! ## Snapshots -/

/-- What a snapshot holds for `l`. -/
def _root_.Lifecycle.Snap.read (backing : Nat → Loc → Nat) (sn : Snap) (l : Loc) : Nat :=
  ((sn.running.accepted.over sn.running.inflight) l).getD (backing sn.parked.seed l)

/-- A snapshot records the active context's running state, under a fresh ID. -/
theorem snapshot_spec {s : St} (h : s.WF) :
    s.snaps s.nextSnap = none ∧
    ∃ sn, s.snapshot.snaps s.nextSnap = some sn ∧ sn.active = s.active ∧
      ∀ l, sn.read backing l = s.read backing l := by
  refine ⟨?_, ⟨s.active, (s.parked s.active).getD (Parked.fresh 0), s.running⟩,
    by simp [snapshot], rfl, fun l => ?_⟩
  · cases hs : s.snaps s.nextSnap
    · rfl
    · have := h.snaps s.nextSnap (by simp [hs]); omega
  · obtain ⟨p, hp⟩ := Option.isSome_iff_exists.1 h.active_isSome
    simp [Snap.read, read, activeSeed, hp, committed]

/-- A snapshot leaves every context, the selection and the other snapshots as they are. -/
theorem snapshot_frame (s : St) (c : Ctx) (l : Loc) {j : Nat} (hj : j ≠ s.nextSnap) :
    s.snapshot.view backing c l = s.view backing c l ∧ s.snapshot.snaps j = s.snaps j := by
  have : s.snapshot.parked = s.parked := by funext c; cases c <;> rfl
  simp only [view, read, activeSeed, this]
  exact ⟨rfl, by simp [snapshot, hj]⟩

section revert

variable {s : St} {j : Nat} {sn : Snap} (h : s.WF) (hs : s.snaps j = some sn) (keep : Bool)
include hs

/-- **A revert restores the snapshot's active context.** The snapshot's active context becomes
active again with the running state the snapshot recorded. -/
theorem revertTo_restores :
    (s.revertTo j keep).active = sn.active ∧ (s.revertTo j keep).running = sn.running ∧
    (s.revertTo j keep).parked sn.active = some sn.parked ∧
    ∀ l, (s.revertTo j keep).read backing l = sn.read backing l := by
  have key : (s.revertTo j keep).parked sn.active = some sn.parked := by
    simp [revertTo, hs, restore, parked_setParked]
  have ha : (s.revertTo j keep).active = sn.active := by simp [revertTo, hs, restore]
  have hr : (s.revertTo j keep).running = sn.running := by simp [revertTo, hs, restore]
  refine ⟨ha, hr, key, fun l => ?_⟩
  simp [read, activeSeed, ha, hr, key, Snap.read, committed]

/-- **A revert leaves the other contexts as they are.** Contexts other than the snapshot's active
one and the running one keep their accepted state, including forks created after the snapshot. -/
theorem revertTo_frame {c : Ctx} (hc : c ≠ sn.active) (hc' : c ≠ s.active) :
    (s.revertTo j keep).parked c = s.parked c := by
  simp only [revertTo, hs, restore]
  split <;> simp [parked_setParked, hc, hc']

/-- **A revert from another context keeps its accepted state and drops its in-flight changes**:
what the running transaction wrote on it since it last became active. Writes accepted by an
earlier switch away from it are kept. -/
theorem revertTo_leaves_running (hne : sn.active ≠ s.active) :
    (s.revertTo j keep).parked s.active =
      some { (s.parked s.active).getD (Parked.fresh 0) with
        overlay := s.running.accepted
        ran := true } := by
  simp [revertTo, hs, restore, hne, parked_setParked, Ne.symm hne]

include h in
/-- **A revert keeps persistent accounts, forks and snapshot IDs**, and removes only the reverted
snapshot unless it is kept. -/
theorem revertTo_keeps :
    (s.revertTo j keep).persistent = s.persistent ∧
    (s.revertTo j keep).nextFork = s.nextFork ∧
    (∀ i, ((s.revertTo j keep).forks i).isSome = (s.forks i).isSome) ∧
    (s.revertTo j keep).nextSnap = s.nextSnap ∧
    ∀ k, (s.revertTo j keep).snaps k = if !keep && k = j then none else s.snaps k := by
  have hw := h.revertTo j keep
  refine ⟨?_, ?_, fun i => ?_, ?_, fun k => ?_⟩
  · simp only [revertTo, hs, restore]; split <;> simp
  · simp only [revertTo, hs, restore]; split <;> simp
  · have hn : (s.revertTo j keep).nextFork = s.nextFork := by
      simp only [revertTo, hs, restore]; split <;> simp
    have h1 := hw.forks i
    have h2 := h.forks i
    rw [hn] at h1
    exact Bool.eq_iff_iff.2 (h1.trans h2.symm)
  · simp only [revertTo, hs, restore]; split <;> simp
  · simp only [revertTo, hs, restore]
    split <;> cases keep <;> by_cases hk : k = j <;> simp_all

end revert

/-- Deleting a snapshot removes exactly it. -/
theorem deleteSnapshot_spec (s : St) (j k : Nat) :
    (s.deleteSnapshot j).snaps k = if k = j then none else s.snaps k := rfl

/-! ## Rolling forks -/

/-- **Rolling the active fork resets it to the new database** for every non-persistent account,
apart from the slots the local context seeds. -/
theorem rollFork_active_resets {s : St} {i : Nat} (hi : (s.forks i).isSome)
    (ha : s.active = some i) {a : Addr} (hp : s.persistent a = false) {f : Field}
    (hl : f = .bal ∨ s.home.loaded a = false ∨ s.home.ran = false) (seed : Nat) :
    (s.rollFork i seed).read backing (a, f) = backing seed (a, f) := by
  have : ¬(s.forks i).isNone := by simpa [Option.isNone_iff_eq_none, Option.isSome_iff_ne_none]
    using hi
  simp only [rollFork, this, if_false, ha, if_true, suspend]
  cases hr : s.home.ran <;> rcases hl with hl | hl | hl <;>
    simp [read, resume, activeSeed, parked_setParked, ha, Running.over, Overlay.over,
      mergePersistent, hp, inheritLocal, Parked.fresh, hl, hr]

/-- **Rolling the active fork keeps persistent accounts** at every location the fork held. -/
theorem rollFork_active_persistent {s : St} {i : Nat} (hi : (s.forks i).isSome)
    (ha : s.active = some i) {a : Addr} (hp : s.persistent a = true) {f : Field} {v : Nat}
    (hv : s.committed (a, f) = some v) (seed : Nat) :
    (s.rollFork i seed).read backing (a, f) = v := by
  have : ¬(s.forks i).isNone := by simpa [Option.isNone_iff_eq_none, Option.isSome_iff_ne_none]
    using hi
  simp only [rollFork, this, if_false, ha, if_true, suspend]
  simp [read, resume, parked_setParked, ha, Running.over, Overlay.over, mergePersistent, hp,
    suspendedParked, hv]

/-- **Rolling an inactive fork resets it** and leaves every other context as it is. -/
theorem rollFork_inactive {s : St} {i : Nat} (hi : (s.forks i).isSome) (hne : s.active ≠ some i)
    (seed : Nat) (c : Ctx) (hc : c ≠ some i) (l : Loc) :
    (s.rollFork i seed).view backing c l = s.view backing c l ∧
    (s.rollFork i seed).parked (some i) = some (inheritLocal (Parked.fresh seed) s.home) := by
  have : ¬(s.forks i).isNone := by simpa [Option.isNone_iff_eq_none, Option.isSome_iff_ne_none]
    using hi
  simp only [rollFork, this, if_false, hne]
  constructor
  · by_cases hca : c = s.active
    · subst hca
      simp [view, read, activeSeed, parked_setParked, hne]
    · simp [view, parked_setParked, hc, hca]
  · simp [parked_setParked]

/-! ## Transactions and forks -/

/-- **Ending a transaction changes no context's state.** -/
theorem endTx_view {s : St} (h : s.WF) (c : Ctx) (l : Loc) :
    s.endTx.view backing c l = s.view backing c l := by
  obtain ⟨p, hp⟩ := Option.isSome_iff_exists.1 h.active_isSome
  by_cases hc : c = s.active
  · subst hc
    simp [endTx, view, read, activeSeed, parked_setParked, hp, Running.over, Overlay.over,
      committed]
  · simp [endTx, view, parked_setParked, hc]

/-- A write changes exactly the written location of the active context. -/
theorem write_view (s : St) (l : Loc) (v : Nat) (c : Ctx) (k : Loc) :
    (s.write backing l v).view backing c k =
      if c = s.active ∧ k = l then v else s.view backing c k := by
  by_cases hc : c = s.active
  · subst hc
    by_cases hk : k = l
    · subst hk; simp [write, load, view, read, activeSeed, committed, Overlay.over]
    · by_cases ha : k.1 = l.1
      · simp [write, load, view, read, activeSeed, committed, Overlay.over, hk, ha]
        cases s.running.inflight k <;> simp
      · simp [write, load, view, read, activeSeed, committed, Overlay.over, hk, ha]
  · simp [write, load, view, hc]

/-- A peek changes no context's state. -/
theorem peek_view (s : St) (l : Loc) (c : Ctx) (k : Loc) :
    (s.peek backing l).view backing c k = s.view backing c k := by
  unfold peek
  split
  · rfl
  · rename_i hl
    by_cases hc : c = s.active
    · subst hc
      by_cases hk : k = l
      · subst hk
        simp at hl
        simp [view, read, activeSeed, committed, Overlay.over, hl]
      · simp [view, read, activeSeed, committed, Overlay.over, hk]
    · simp [view, hc]

/-- **A new fork starts from its database** when created while the local context is active,
because the local context has no loaded accounts then (`WF.homeUnloaded`): the stale accepted
state of the active local context that `create_fork` reads is never used. It gets a fresh ID. -/
theorem createFork_fresh {s : St} (h : s.WF) (hact : s.active = none) (seed : Nat) (l : Loc) :
    s.forks s.nextFork = none ∧
    (s.createFork seed).view backing (some s.nextFork) l = backing seed l := by
  constructor
  · cases hf : s.forks s.nextFork
    · rfl
    · have := (h.forks s.nextFork).1 (by simp [hf]); omega
  · have hu := h.homeUnloaded hact
    simp [createFork, view, hact, inheritLocal, hu, Parked.read, Parked.fresh]

/-- Creating a fork leaves every other context as it is. -/
theorem createFork_frame (s : St) (seed : Nat) (c : Ctx) (hc : c ≠ some s.nextFork)
    (l : Loc) : (s.createFork seed).view backing c l = s.view backing c l := by
  have hp : ∀ d, d ≠ some s.nextFork → (s.createFork seed).parked d = s.parked d := by
    intro d hd
    cases d with
    | none => rfl
    | some k =>
      have : k ≠ s.nextFork := fun e => hd (by rw [e])
      simp [createFork, this]
  by_cases hca : c = s.active
  · subst hca
    have hpa := hp _ hc
    have : (s.createFork seed).running = s.running := rfl
    have ha : (s.createFork seed).active = s.active := rfl
    simp only [view, read, activeSeed, hpa, this, ha, if_true]
  · have : (s.createFork seed).active = s.active := rfl
    simp [view, hca, hp c hc, this]

end St

end Lifecycle
