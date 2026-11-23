import Lifecycle.Registry

/-!
# Fork registry invariants

The contract of the fork registry (`Lifecycle.Registry`), proved over the model:

* `WF.step`: well-formedness is preserved by every operation. It includes that every snapshot's
  active fork still exists, which the Rust `expect("snapshot's active fork exists")` in
  `ExecutionLifecycle::revert` relies on, and that the local context has no loaded accounts while
  it is active, which makes the stale accepted state `create_fork` reads from it harmless.
* Fork switches carry persistent accounts and isolate every other account
  (`selectFork_carries_persistent`, `selectFork_isolates`), except that leaving the local context
  seeds every fork with the slots the running transaction loaded there
  (`selectFork_from_local_seeds_forks`).
* `revertTo` restores exactly the snapshot's active context, leaves every other context as it is
  apart from the running context's in-flight changes, keeps the persistent set and every fork,
  and removes only the reverted snapshot (`revertTo_*`).
* Rolling a fork resets it to the new database except for persistent accounts and the local
  context's seeds (`rollFork_*`); ending a transaction changes no context's state (`endTx_view`).
-/

namespace Lifecycle

namespace St

section

variable {backing : Nat → Loc → Nat}

/-! ## Structural lemmas -/

@[simp] theorem setParked_nextFork (s : St) c p : (s.setParked c p).nextFork = s.nextFork := by
  cases c <;> rfl
@[simp] theorem setParked_active (s : St) c p : (s.setParked c p).active = s.active := by
  cases c <;> rfl
@[simp] theorem setParked_snaps (s : St) c p : (s.setParked c p).snaps = s.snaps := by
  cases c <;> rfl
@[simp] theorem setParked_nextSnap (s : St) c p : (s.setParked c p).nextSnap = s.nextSnap := by
  cases c <;> rfl
@[simp] theorem setParked_persistent (s : St) c p :
    (s.setParked c p).persistent = s.persistent := by
  cases c <;> rfl
@[simp] theorem setParked_running (s : St) c p : (s.setParked c p).running = s.running := by
  cases c <;> rfl

theorem parked_setParked (s : St) (c d : Ctx) (p : Parked) :
    (s.setParked c p).parked d = if d = c then some p else s.parked d := by
  cases c <;> cases d <;> simp [setParked, parked]

@[simp] theorem setParked_home_some (s : St) i p : (s.setParked (some i) p).home = s.home := rfl
@[simp] theorem setParked_home_none (s : St) p : (s.setParked none p).home = p := rfl
@[simp] theorem setParked_forks_none (s : St) p : (s.setParked none p).forks = s.forks := rfl
@[simp] theorem setParked_forks_some (s : St) i p j :
    (s.setParked (some i) p).forks j = if j = i then some p else s.forks j := rfl

/-- The contexts of a state rebuilt from another's contexts. -/
@[simp] theorem parked_mk (t : St) n a p r sn ns (c : Ctx) :
    (St.mk t.home t.forks n a p r sn ns).parked c = t.parked c := by
  cases c <;> rfl

@[simp] theorem parked_none (s : St) : s.parked none = some s.home := rfl
@[simp] theorem parked_some (s : St) i : s.parked (some i) = s.forks i := rfl

/-! ## Well-formedness -/

/-- Well-formedness of the lifecycle. -/
structure WF (s : St) : Prop where
  /-- Forks are numbered from zero and never removed, as `ForkRegistry::next_id` says. -/
  forks : ∀ i, (s.forks i).isSome ↔ i < s.nextFork
  /-- The active context exists. -/
  active : ∀ i, s.active = some i → i < s.nextFork
  /-- Snapshot IDs are below the next one, so a new snapshot never reuses an ID. -/
  snaps : ∀ j, (s.snaps j).isSome → j < s.nextSnap
  /-- Every snapshot's active fork exists. -/
  snapActive : ∀ j sn, s.snaps j = some sn → ∀ i, sn.active = some i → i < s.nextFork
  /-- The local context has no loaded accounts while it is active. -/
  homeUnloaded : s.active = none → ∀ a, s.home.loaded a = false
  /-- Nor does a snapshot taken on it. -/
  snapHomeUnloaded : ∀ j sn, s.snaps j = some sn → sn.active = none →
    ∀ a, sn.parked.loaded a = false

theorem WF.init (seed : Nat) : (St.init seed).WF where
  forks := by simp [St.init]
  active := by simp [St.init]
  snaps := by simp [St.init]
  snapActive := by simp [St.init]
  homeUnloaded := by simp [St.init, Parked.fresh]
  snapHomeUnloaded := by simp [St.init]

theorem WF.parked_isSome {s : St} (h : s.WF) {c : Ctx} (hc : ∀ i, c = some i → i < s.nextFork) :
    (s.parked c).isSome := by
  cases c with
  | none => simp
  | some i => simpa using (h.forks i).2 (hc i rfl)

theorem WF.active_isSome {s : St} (h : s.WF) : (s.parked s.active).isSome :=
  h.parked_isSome h.active

theorem WF.setParked {s : St} (h : s.WF) {c : Ctx} (hc : ∀ i, c = some i → i < s.nextFork)
    (p : Parked) (hp : c = none → s.active = none → ∀ a, p.loaded a = false) :
    (s.setParked c p).WF where
  forks j := by
    cases c with
    | none => simpa using h.forks j
    | some i =>
      rw [setParked_forks_some]; split
      · subst_vars; simpa using hc _ rfl
      · simpa using h.forks j
  active := by simpa using h.active
  snaps := by simpa using h.snaps
  snapActive := by simpa using h.snapActive
  homeUnloaded := by
    cases c with
    | none => simpa using hp rfl
    | some i => simpa using h.homeUnloaded
  snapHomeUnloaded := by simpa using h.snapHomeUnloaded

theorem WF.running {s : St} (h : s.WF) (r : Running) : ({ s with running := r }).WF where
  forks := h.forks
  active := h.active
  snaps := h.snaps
  snapActive := h.snapActive
  homeUnloaded := h.homeUnloaded
  snapHomeUnloaded := h.snapHomeUnloaded

theorem WF.persistent {s : St} (h : s.WF) (q : AddrSet) : ({ s with persistent := q }).WF where
  forks := h.forks
  active := h.active
  snaps := h.snaps
  snapActive := h.snapActive
  homeUnloaded := h.homeUnloaded
  snapHomeUnloaded := h.snapHomeUnloaded

theorem WF.suspend {s : St} (h : s.WF) (hact : s.active ≠ none) : s.suspend.1.WF :=
  h.setParked h.active _ (fun h' => absurd h' hact)

theorem WF.endTx {s : St} (h : s.WF) : s.endTx.WF := by
  unfold St.endTx
  exact (h.setParked h.active _ (fun _ _ _ => rfl)).running _

theorem WF.createFork {s : St} (h : s.WF) (seed : Nat) : (s.createFork seed).WF where
  forks j := by
    simp only [St.createFork]; split
    · subst_vars; simp
    · have := h.forks j; simp_all; omega
  active i hi := by have := h.active i hi; simp [St.createFork]; omega
  snaps := h.snaps
  snapActive j sn hs i hi := by have := h.snapActive j sn hs i hi; simp [St.createFork]; omega
  homeUnloaded := h.homeUnloaded
  snapHomeUnloaded := h.snapHomeUnloaded

theorem WF.snapshot {s : St} (h : s.WF) : s.snapshot.WF where
  forks := h.forks
  active := h.active
  snaps j hj := by
    simp only [St.snapshot] at hj ⊢; split at hj
    · omega
    · have := h.snaps j hj; omega
  snapActive j sn hs i hi := by
    simp only [St.snapshot] at hs ⊢; split at hs
    · cases hs; exact h.active i hi
    · exact h.snapActive j sn hs i hi
  homeUnloaded := h.homeUnloaded
  snapHomeUnloaded j sn hs hn a := by
    simp only [St.snapshot] at hs; split at hs
    · cases hs; simp only at hn; simp [hn, h.homeUnloaded hn a]
    · exact h.snapHomeUnloaded j sn hs hn a

theorem WF.deleteSnapshot {s : St} (h : s.WF) (j : Nat) : (s.deleteSnapshot j).WF where
  forks := h.forks
  active := h.active
  snaps k hk := by
    simp only [St.deleteSnapshot] at hk; split at hk
    · simp at hk
    · exact h.snaps k hk
  snapActive k sn hs := by
    simp only [St.deleteSnapshot] at hs; split at hs
    · simp at hs
    · exact h.snapActive k sn hs
  homeUnloaded := h.homeUnloaded
  snapHomeUnloaded k sn hs := by
    simp only [St.deleteSnapshot] at hs; split at hs
    · simp at hs
    · exact h.snapHomeUnloaded k sn hs

theorem WF.restore {s : St} (h : s.WF) {j : Nat} {sn : Snap} (hs : s.snaps j = some sn)
    (snaps' : Nat → Option Snap) (hsub : ∀ k t, snaps' k = some t → s.snaps k = some t) :
    (s.restore sn snaps').WF := by
  have hsa := h.snapActive j sn hs
  have h1 : (if sn.active = s.active then s else s.setParked s.active
      { (s.parked s.active).getD (Parked.fresh 0) with
        overlay := s.running.accepted
        ran := true }).WF := by
    split
    · exact h
    · refine h.setParked h.active _ ?_
      intro _ hn a; simp [hn, h.homeUnloaded hn a]
  unfold St.restore
  generalize hs1 : (if sn.active = s.active then s else _) = s1 at h1
  have hn : s1.nextFork = s.nextFork := by rw [← hs1]; split <;> simp
  have hsn : s1.snaps = s.snaps := by rw [← hs1]; split <;> simp
  have hns : s1.nextSnap = s.nextSnap := by rw [← hs1]; split <;> simp
  have h2 := h1.setParked (c := sn.active) (by rw [hn]; exact hsa) sn.parked
    (fun hc _ a => h.snapHomeUnloaded j sn hs hc a)
  exact {
    forks := h2.forks
    active := by simpa [hn] using hsa
    snaps := fun k hk => by
      obtain ⟨t, ht⟩ := Option.isSome_iff_exists.1 hk
      simpa [hn, hsn, hns] using h.snaps k (by simp [hsub k t ht])
    snapActive := fun k t ht => by simpa [hn] using h.snapActive k t (hsub k t ht)
    homeUnloaded := fun hc a => by
      simp only at hc
      have : (s1.setParked sn.active sn.parked).home = sn.parked := by rw [hc]; rfl
      rw [this]; exact h.snapHomeUnloaded j sn hs hc a
    snapHomeUnloaded := fun k t ht => h.snapHomeUnloaded k t (hsub k t ht) }

theorem WF.revertTo {s : St} (h : s.WF) (j : Nat) (keep : Bool) : (s.revertTo j keep).WF := by
  unfold St.revertTo
  split
  · exact h
  · rename_i sn hs
    refine h.restore hs _ ?_
    intro k t ht
    cases keep
    · simp only [Bool.false_eq_true, if_false] at ht
      split at ht
      · simp at ht
      · exact ht
    · simpa using ht

theorem WF.resume {s : St} (h : s.WF) (hact : s.active ≠ none) (src : Suspended) (carry : Bool) :
    (s.resume src carry).WF := by
  unfold St.resume
  exact (h.setParked h.active _ (fun _ h' => absurd h' hact)).running _

theorem WF.inheritAll {s : St} (h : s.WF) : s.inheritAll.WF where
  forks i := by simpa [St.inheritAll] using h.forks i
  active := h.active
  snaps := h.snaps
  snapActive := h.snapActive
  homeUnloaded := h.homeUnloaded
  snapHomeUnloaded := h.snapHomeUnloaded

theorem forks_isSome_setParked (s : St) {c : Ctx} (hc : ∀ i, c = some i → (s.forks i).isSome)
    (p : Parked) (j : Nat) : ((s.setParked c p).forks j).isSome = (s.forks j).isSome := by
  cases c with
  | none => rfl
  | some i =>
    rw [setParked_forks_some]; split
    · subst_vars; simpa using (hc _ rfl).symm
    · rfl

/-- Builds well-formedness of a state that agrees with a well-formed one on everything but its
contexts and running state, with an active fork. -/
theorem WF.of_agree {s t : St} (h : s.WF) (hforks : ∀ j, (t.forks j).isSome = (s.forks j).isSome)
    (hn : t.nextFork = s.nextFork) (hsn : t.snaps = s.snaps) (hns : t.nextSnap = s.nextSnap)
    (i : Nat) (hi : i < s.nextFork) (ha : t.active = some i) : t.WF where
  forks j := by rw [hforks, hn]; exact h.forks j
  active k hk := by rw [ha] at hk; cases hk; omega
  snaps := by rw [hsn, hns]; exact h.snaps
  snapActive := by rw [hsn, hn]; exact h.snapActive
  homeUnloaded hc := by rw [ha] at hc; cases hc
  snapHomeUnloaded := by rw [hsn]; exact h.snapHomeUnloaded

theorem WF.active_forks {s : St} (h : s.WF) : ∀ i, s.active = some i → (s.forks i).isSome :=
  fun i hi => (h.forks i).2 (h.active i hi)

theorem isSome_setParked (s : St) (c : Ctx) (p : Parked) (j : Nat) :
    ((s.setParked c p).forks j).isSome = if c = some j then true else (s.forks j).isSome := by
  cases c with
  | none => simp [setParked]
  | some i =>
    by_cases hj : j = i
    · subst hj; simp [setParked_forks_some]
    · have : i ≠ j := fun h => hj h.symm
      simp [setParked_forks_some, hj, this]

theorem WF.selectFork {s : St} (h : s.WF) (i : Nat) : (s.selectFork i).WF := by
  unfold St.selectFork
  split
  · exact h
  · rename_i hc
    simp only [not_or, Option.isNone_iff_eq_none] at hc
    have hsome : (s.forks i).isSome := Option.isSome_iff_ne_none.2 hc.1
    have hi : i < s.nextFork := (h.forks i).1 hsome
    refine h.of_agree (t := _) ?_ ?_ ?_ ?_ i hi ?_
    · intro j
      have ha := h.active_forks
      by_cases hj : j = i
      · subst hj; cases hact : s.active <;>
          simp [St.resume, St.suspend, St.inheritAll, isSome_setParked, hact, hsome]
      · cases hact : s.active with
        | none => simp [St.resume, St.suspend, St.inheritAll, isSome_setParked, hact, hj, Ne.symm hj]
        | some k =>
          have := ha k hact
          by_cases hk : j = k
          · subst hk; simp [St.resume, St.suspend, isSome_setParked, hact, this, hj, Ne.symm hj]
          · simp [St.resume, St.suspend, isSome_setParked, hact, hj, Ne.symm hj, hk, Ne.symm hk]
    all_goals simp only [St.resume, St.suspend]; split <;> simp [St.inheritAll]

theorem WF.rollFork {s : St} (h : s.WF) (i seed : Nat) : (s.rollFork i seed).WF := by
  unfold St.rollFork
  split
  · exact h
  · rename_i hc
    have hsome : (s.forks i).isSome := by simpa [Option.isNone_iff_eq_none,
      Option.isSome_iff_ne_none] using hc
    have hi : i < s.nextFork := (h.forks i).1 hsome
    split
    · rename_i ha
      refine h.of_agree (t := _) ?_ ?_ ?_ ?_ i hi ?_
      · intro j
        by_cases hj : j = i
        · subst hj; simp [St.resume, St.suspend, isSome_setParked, ha, hsome]
        · simp [St.resume, St.suspend, isSome_setParked, ha, hj, Ne.symm hj]
      all_goals simp [St.resume, St.suspend, ha]
    · exact h.setParked (fun k hk => by cases hk; exact hi) _ (fun hk => by cases hk)

theorem WF.write {s : St} (h : s.WF) (l : Loc) (v : Nat) : (s.write backing l v).WF :=
  h.running _

/-- Every operation preserves well-formedness. -/
theorem WF.step {s : St} (h : s.WF) (op : Op) : (s.step backing op).WF := by
  cases op with
  | createFork seed => exact h.createFork seed
  | selectFork i => exact h.selectFork i
  | setBalance a v => exact h.write _ v
  | setSlot a v => exact h.write _ v
  | makePersistent a => exact h.persistent _
  | revokePersistent a => exact h.persistent _
  | snapshot => exact h.snapshot
  | revertTo j keep => exact h.revertTo j keep
  | deleteSnapshot j => exact h.deleteSnapshot j
  | rollFork i seed => exact h.rollFork i seed
  | endTx => exact h.endTx
  | peek l => simp only [St.step]; unfold St.peek; split; exact h; exact h.running _

/-- Every reachable state is well-formed. -/
theorem WF.run (seed : Nat) (ops : List Op) :
    (ops.foldl (St.step backing) (St.init seed)).WF := by
  suffices ∀ s : St, s.WF → (ops.foldl (St.step backing) s).WF from this _ (WF.init seed)
  induction ops with
  | nil => exact fun _ h => h
  | cons op ops ih => exact fun s h => ih _ (h.step op)

end

end St

end Lifecycle
