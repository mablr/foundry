/-!
# Snapshot restorations inside call frames

A model of `SnapshotRestores` (`crates/evm/core/src/evm/snapshot.rs`). A `vm.revertToState` inside
a call frame replaces the running state, journal included, so the frames open at the time own the
replaced state instead: the first restoration since each frame started is retained, and a frame
that fails reinstates it and replays the persistent writes recorded since.

Fidelity: the running and retained states (`StateSnapshot`) are an opaque type `S`, persistent
writes an opaque type `E` with `reinst` modelling `PersistentWrite::reinstated`, and journal
lengths in the ledger are kept as recorded (`usize::MAX` is `none`). The rollback to the failing
frame's checkpoint is not modeled: `frameEnd` returns the reinstated state. Frames are a list with
the innermost first; Rust's `Vec`s keep the innermost (frames) and newest (retained states) last.
-/

namespace Lifecycle.Restores

/-- A persistent write with the depth and journal length it was recorded at (`none` for
`usize::MAX`). Models `Ledger`'s entries. -/
abbrev Entry (E : Type) := Nat × Option Nat × E

/-- Models `LaterWrites`. -/
structure Later (E : Type) where
  dropped : List (Entry E)
  since : Nat

/-- Models `ReplacedState`. -/
structure Retained (S E : Type) where
  state : S
  ledger : List (Entry E)
  later : Option (Later E)
  earlier : List (Option (Nat × Nat))

/-- Models `RestoreFrame`, without its checkpoint. -/
structure Frame where
  depth : Nat
  replacedLen : Nat

/-- Models `SnapshotRestores`. -/
structure R (S E : Type) where
  /-- Open frames, innermost first. -/
  frames : List Frame
  /-- Retained states, oldest first. -/
  replaced : List (Retained S E)
  kept : Bool

variable {S E : Type}

/-- `SnapshotRestores::default()`. -/
def R.empty : R S E := ⟨[], [], false⟩

/-- `SnapshotRestores::frame_start`. -/
def R.frameStart (r : R S E) (d : Nat) : R S E :=
  { r with frames := ⟨d, r.replaced.length⟩ :: r.frames }

/-- Whether a restoration now is the first since the innermost frame started. -/
def R.first (r : R S E) : Bool :=
  match r.frames with
  | f :: _ => f.replacedLen == r.replaced.length
  | [] => false

/-- `SnapshotRestores::capture`: retains `st`, replaced by a restoration that keeps the first
`len` writes of `ledger`, if it is the first since the innermost frame started. -/
def R.capture (r : R S E) (st : S) (ledger : List (Entry E)) (len : Nat) : R S E :=
  let len := min len ledger.length
  let earlier := if r.first then r.replaced.map fun x => x.later.map fun l => (l.dropped.length, l.since)
    else []
  let replaced := r.replaced.map fun x =>
    { x with later := x.later.map fun l => ⟨l.dropped ++ ledger.drop l.since, len⟩ }
  { r with replaced := if r.first then replaced ++ [⟨st, ledger, some ⟨[], len⟩, earlier⟩]
    else replaced }

/-- `SnapshotRestores::accept_writes`. -/
def R.acceptWrites (r : R S E) : R S E :=
  { r with replaced := r.replaced.map fun x =>
    { x with later := none, earlier := x.earlier.map fun _ => none } }

/-- `iter_mut().zip(..)` updating the retained states that have an `earlier` record. -/
def restoreEarlier : List (Retained S E) → List (Option (Nat × Nat)) → List (Retained S E)
  | x :: xs, e :: es =>
    { x with later := match x.later, e with
      | some l, some (n, since) => some ⟨l.dropped.take n, since⟩
      | _, _ => none } ::
      restoreEarlier xs es
  | xs, [] => xs
  | [], _ => []

/-- What a failing frame reinstates: the retained state, the ledger to replay over it, and the
retained states left. -/
def reinstate (reinst : E → E) (kept : List (Retained S E)) (ledger : List (Entry E)) :
    Option (List (Retained S E) × List (Entry E) × S) :=
  kept.getLast?.map fun x =>
    let later := match x.later with
      | some l => (l.dropped ++ ledger.drop l.since).map fun (dp, _, w) => (dp, none, reinst w)
      | none => []
    (restoreEarlier kept.dropLast x.earlier, x.ledger ++ later, x.state)

/-- `SnapshotRestores::frame_end`, with `reinst` as `PersistentWrite::reinstated`. Returns the new
bookkeeping and, if a frame at depth `d` was open, the ledger and the state the failing frame
reinstates, if any. -/
def R.frameEnd (reinst : E → E) (r : R S E) (ledger : List (Entry E)) (d : Nat) (failed : Bool) :
    R S E × Option (List (Entry E) × Option S) :=
  let frames := r.frames.dropWhile fun f => decide (f.depth > d)
  match frames with
  | f :: rest =>
    if f.depth = d then
      let (replaced, ledger, st) :=
        if failed ∧ r.replaced.length > f.replacedLen then
          match reinstate reinst (r.replaced.take (f.replacedLen + 1)) ledger with
          | some (replaced, ledger, st) => (replaced, ledger, some st)
          | none => (r.replaced, ledger, none)
        else (r.replaced, ledger, none)
      let r' : R S E := match rest with
        | caller :: _ => ⟨rest, replaced.take (caller.replacedLen + 1), r.kept⟩
        | [] => ⟨[], [], r.kept || !replaced.isEmpty⟩
      (r', some (ledger, st))
    else ({ r with frames := frames }, none)
  | [] => ({ r with frames := frames }, none)

/-! ## Bounded by call depth -/

/-- Each frame started with at most one more retained state than its caller, and the outermost
with none. -/
def Chain : List Frame → Prop
  | [] => True
  | [f] => f.replacedLen = 0
  | f :: g :: fs => f.replacedLen ≤ g.replacedLen + 1 ∧ Chain (g :: fs)

theorem Chain.tail {f : Frame} {fs : List Frame} (h : Chain (f :: fs)) : Chain fs := by
  cases fs with
  | nil => trivial
  | cons g gs => exact h.2

theorem Chain.dropWhile {fs : List Frame} (h : Chain fs) (p : Frame → Bool) :
    Chain (fs.dropWhile p) := by
  induction fs with
  | nil => trivial
  | cons f fs ih =>
    simp only [List.dropWhile_cons]
    split
    · exact ih h.tail
    · exact h

theorem Chain.le_length {f : Frame} {fs : List Frame} (h : Chain (f :: fs)) :
    f.replacedLen ≤ fs.length := by
  induction fs generalizing f with
  | nil => exact Nat.le_of_eq h
  | cons g gs ih => have := ih h.2; simp only [List.length_cons]; have := h.1; omega

/-- The bookkeeping invariant. -/
structure R.Inv (r : R S E) : Prop where
  chain : Chain r.frames
  top : ∀ f, r.frames.head? = some f → r.replaced.length ≤ f.replacedLen + 1
  closed : r.frames = [] → r.replaced = []

/-- **Retained states are bounded by call depth**: at most one per open frame. -/
theorem R.Inv.bound {r : R S E} (h : r.Inv) : r.replaced.length ≤ r.frames.length := by
  cases hf : r.frames with
  | nil => simp [h.closed hf]
  | cons f fs =>
    have := h.top f (by simp [hf])
    have := (hf ▸ h.chain : Chain (f :: fs)).le_length
    simp only [List.length_cons]; omega

theorem R.Inv.empty : (R.empty : R S E).Inv := ⟨trivial, by simp [R.empty], fun _ => rfl⟩

theorem R.Inv.frameStart {r : R S E} (h : r.Inv) (d : Nat) : (r.frameStart d).Inv where
  chain := by
    simp only [R.frameStart]
    cases hf : r.frames with
    | nil => simp [Chain, h.closed hf]
    | cons g gs => exact ⟨h.top g (by simp [hf]), hf ▸ h.chain⟩
  top f hf := by
    simp only [R.frameStart, List.head?_cons, Option.some.injEq] at hf
    subst hf; simp [R.frameStart]
  closed := by simp [R.frameStart]

theorem R.Inv.capture {r : R S E} (h : r.Inv) (st : S) (ledger : List (Entry E)) (len : Nat) :
    (r.capture st ledger len).Inv where
  chain := h.chain
  top f hf := by
    have hf0 : r.frames.head? = some f := hf
    have := h.top f hf0
    have hf' : r.frames = f :: r.frames.tail := by
      cases hr : r.frames <;> simp_all
    by_cases hfirst : r.first = true
    · have : f.replacedLen = r.replaced.length := by
        rw [R.first, hf'] at hfirst; simpa using hfirst
      simp only [R.capture, hfirst, if_true, List.length_append, List.length_map,
        List.length_singleton]
      omega
    · simp only [R.capture, hfirst]; simpa using this
  closed hf := by
    have hf' : r.frames = [] := hf
    have := h.closed hf'
    simp [R.capture, R.first, hf', this]

theorem R.Inv.acceptWrites {r : R S E} (h : r.Inv) : r.acceptWrites.Inv where
  chain := h.chain
  top f hf := by simpa [R.acceptWrites] using h.top f hf
  closed hf := by simp [R.acceptWrites, h.closed hf]

/-- Closing a frame keeps the invariant, unless it pops frames that aborted without finding its
own, which leaves their retained states until the next frame closes. -/
theorem R.Inv.frameEnd {r : R S E} (h : r.Inv) (reinst : E → E) (ledger : List (Entry E))
    (d : Nat) (failed : Bool)
    (hno : ∀ f, r.frames.head? = some f → f.depth ≤ d) :
    (r.frameEnd reinst ledger d failed).1.Inv := by
  have hdw : r.frames.dropWhile (fun f => decide (f.depth > d)) = r.frames := by
    cases hf : r.frames with
    | nil => rfl
    | cons f fs =>
      have := hno f (by simp [hf])
      simp [List.dropWhile_cons]; omega
  unfold R.frameEnd
  simp only [hdw]
  split
  · rename_i f rest hf
    have hc : Chain rest := (hf ▸ h.chain : Chain (f :: rest)).tail
    split
    · constructor
      · split <;> simp_all
      · intro g hg
        split at hg <;> simp_all <;> omega
      · intro hr
        split <;> simp_all
    · exact h
  · exact h

/-! ## Reinstating -/

/-- **A failing frame reinstates its first restoration and replays the writes since.** A frame
that has retained nothing yet captures a restoration that keeps the first `n` writes of `ledger`;
if it then fails with `ledger'` as the running ledger, it reinstates the replaced state, and the
ledger to replay over it is the one recorded at the restoration followed by the writes recorded
since, reinstated and replayed after every journaled write. -/
theorem frameEnd_after_capture (reinst : E → E) (r : R S E) {f : Frame} {rest : List Frame}
    (hf : r.frames = f :: rest) (hfirst : f.replacedLen = r.replaced.length) (st : S)
    (ledger : List (Entry E)) (n : Nat) (hn : n ≤ ledger.length) (ledger' : List (Entry E)) :
    ((r.capture st ledger n).frameEnd reinst ledger' f.depth true).2 =
      some (ledger ++ (ledger'.drop n).map (fun (dp, _, w) => (dp, none, reinst w)), some st) := by
  have hfst : r.first = true := by simp [R.first, hf, hfirst]
  have hmin : min n ledger.length = n := by omega
  simp only [R.frameEnd, R.capture, hfst, if_true, hmin, hf]
  simp only [List.dropWhile_cons, gt_iff_lt, Nat.lt_irrefl, decide_false, if_true]
  simp [reinstate, hfirst, List.take_of_length_le]

/-- **A frame that succeeds keeps the restoration**: its ledger is unchanged and nothing is
reinstated. -/
theorem frameEnd_success (reinst : E → E) (r : R S E) {f : Frame} {rest : List Frame}
    (hf : r.frames = f :: rest) (ledger : List (Entry E)) :
    (r.frameEnd reinst ledger f.depth false).2 = some (ledger, none) := by
  simp [R.frameEnd, hf]

/-- **Frames close in stack order**: closing the frame at depth `d` pops it and the aborted
frames above it, and nothing else. -/
theorem frameEnd_frames (reinst : E → E) (r : R S E) (ledger : List (Entry E)) (d : Nat)
    (failed : Bool) :
    (r.frameEnd reinst ledger d failed).1.frames =
      match r.frames.dropWhile (fun f => decide (f.depth > d)) with
      | f :: rest => if f.depth = d then rest else f :: rest
      | [] => [] := by
  unfold R.frameEnd
  dsimp only
  split <;> (try split) <;> (try split) <;> simp_all

end Lifecycle.Restores

