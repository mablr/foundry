/-!
# `expectEmit` matching

Model of the engine-free matcher in `crates/cheatcodes/src/expected_emit.rs`: `observe_log`,
which fills or checks the queue of expectations against each emitted log, `check_call_emits`,
which checks the queue when the asserted call ends, and `first_unmet_root_emit`, which checks
leftovers when the test ends.

Fidelity:
- `ExpectedEmitTracker` (a `VecDeque`) is a `List`; `remove`, `insert`, `push_front` and
  `push_back` are `eraseIdx`, `insertIdx`, `cons` and `++ [_]`.
- Log bodies (`LogData`: topics and data) and check flags are abstract types `B` and `C`;
  `checks_topics_and_data` is the parameter `M.ck`, and `log.topics().is_empty()` is
  `M.noTopics`. `checksTopicsAndData` gives the concrete function and proves it reflexive, the one
  property the theorems need (`Matcher.refl`).
- The per-emitter `AddressHashMap<LogCountMap>` of an expectation is `seen`, the list of emitters
  of the logs it counted. `LogCountMap` only counts logs that satisfy its checks, so its total
  for emitter `a` is `seen.count a`, and the sum over emitters is `seen.length`.
- Mismatch diagnostics (`mismatch_error`), call depths and static calls are not modelled:
  `checkCall` is `check_call_emits` once the call end it applies to is reached.

The headline theorem is `run_spec`: from a freshly declared queue, logs never trip a `count = 0`
expectation iff none of them matches one, and in that case the call-end check passes iff the
positive expectations are matched by consecutive blocks of the logs, in declaration order, each
block holding at least `count` matching logs (extra logs allowed). `step_found_iff` states that an
emitter-agnostic expectation is found iff the logs matching it, summed over all emitters, reach
its count. `old_misses_two_emitters` shows the pre-fix rule, which read the count of one arbitrary
emitter, rejects such a trace for every `HashMap` iteration order.
-/

namespace Cheatcodes.ExpectEmit

/-- A log: its emitter address and its body. -/
structure Log (B : Type) where
  emitter : Nat
  body : B

/-- `ExpectedEmit` with its count map. -/
structure Exp (B C : Type) where
  pat : Option B
  checks : C
  addr : Option Nat
  anon : Bool
  found : Bool
  count : Nat
  seen : List Nat

/-- The log matching functions. -/
structure Matcher (B C : Type) where
  /-- `checks_topics_and_data(checks, expected, actual)`. -/
  ck : C → B → B → Bool
  /-- `log.topics().is_empty()`. -/
  noTopics : B → Bool
  /-- A log matches itself under any checks. -/
  refl : ∀ c b, ck c b b = true

/-- Failures reported by `observe_log`. -/
inductive LogErr where
  | countZero
  | anonymous
  deriving DecidableEq, Repr

/-- Failures reported by `check_call_emits`. -/
inductive CallErr where
  | unmatched
  | count
  deriving DecidableEq, Repr

variable {B C : Type} (M : Matcher B C)

/-- Whether a log is one the expectation looks for: its checks pass and, if set, the emitter
matches. -/
def relevant (e : Exp B C) (l : Log B) : Bool :=
  match e.pat with
  | none => false
  | some p => M.ck e.checks p l.body && e.addr.all (· == l.emitter)

/-- The count the expectation compares with `count`: the emitter's count if an emitter is set,
otherwise the sum over all emitters. -/
def relCount (e : Exp B C) : Nat :=
  match e.addr with
  | some a => e.seen.count a
  | none => e.seen.length

/-- `observe_log` once the expectation `e` to check has been removed from the queue, with the
count rule `cnt`: returns the updated expectation. -/
def checkOne (cnt : Exp B C → Nat) (e : Exp B C) (p : B) (l : Log B) : Exp B C :=
  let seen := if M.ck e.checks p l.body then e.seen ++ [l.emitter] else e.seen
  let e1 := { e with seen := seen }
  { e1 with
    found := M.ck e.checks p l.body && e.addr.all (· == l.emitter) && decide (e.count ≤ cnt e1) }

/-- Models `observe_log` with the count rule `cnt` used to decide `found`. -/
def observeWith (cnt : Exp B C → Nat) (t : List (Exp B C)) (l : Log B) :
    List (Exp B C) × Option LogErr :=
  if t.all (·.found) then (t, none)
  else if t.any (fun e => e.count == 0 && !e.found && relevant M e l) then (t, some .countZero)
  else
    let fill := t.any (·.pat.isNone)
    let i := if fill then t.findIdx (·.found) - 1
      else t.findIdx (fun e => !e.found && decide (0 < e.count))
    if !fill && t.all (fun e => e.found || e.count == 0) then (t, none)
    else match t[i]? with
      | none => (t, none)
      | some e =>
        let t' := t.eraseIdx i
        match e.pat with
        | none =>
          if e.anon || !M.noTopics l.body then (t'.insertIdx i { e with pat := some l.body }, none)
          else (t', some .anonymous)
        | some p =>
          let e' := checkOne M cnt e p l
          if e'.found then (t' ++ [e'], none) else (e' :: t', none)

/-- Models the fixed `observe_log`. -/
def observe := observeWith M relCount

/-- Models `check_call_emits` once it applies: `none` clears the queue. -/
def checkCall (t : List (Exp B C)) : Option CallErr :=
  if t.any (fun e => !e.found && decide (0 < e.count)) then some .unmatched
  else if t.any (fun e => relCount e != e.count) then some .count
  else none

/-- Models `first_unmet_root_emit`: `true` iff an expectation is left unmet. -/
def rootUnmet (t : List (Exp B C)) : Bool :=
  t.any (fun e => !e.found && decide (0 < e.count))

/-- Feeds logs to `observeWith cnt` until one fails, as the failing frame reverts. -/
def runWith (cnt : Exp B C → Nat) : List (Exp B C) → List (Log B) → List (Exp B C) × Option LogErr
  | t, [] => (t, none)
  | t, l :: ls =>
    match observeWith M cnt t l with
    | (t', none) => runWith cnt t' ls
    | r => r

/-- Feeds logs to the fixed `observe_log`. -/
def run := runWith M relCount

/-! ## Specification -/

/-- Logs relevant to `e`, counted. -/
def relIn (e : Exp B C) (L : List (Log B)) : Nat := (L.filter (relevant M e)).length

/-- The positive expectations `es` are matched in order by consecutive blocks of `L`, each with
at least `count` relevant logs on top of those already counted. -/
def Sat : List (Exp B C) → List (Log B) → Prop
  | [], _ => True
  | e :: es, L => ∃ L₁ L₂, L = L₁ ++ L₂ ∧ e.count ≤ relCount e + relIn M e L₁ ∧ Sat es L₂

/-- An expectation as declared and filled, before any log of the asserted call. -/
def Fresh (e : Exp B C) : Prop := e.pat.isSome ∧ e.found = false ∧ e.seen = []

/-! ## Step lemmas -/

theorem relCount_checkOne_seen (e : Exp B C) (p : B) (l : Log B) (hp : e.pat = some p) :
    relCount { e with seen := if M.ck e.checks p l.body then e.seen ++ [l.emitter] else e.seen }
      = relCount e + if relevant M e l then 1 else 0 := by
  unfold relCount relevant
  rw [hp]
  cases ha : e.addr with
  | none => by_cases h : M.ck e.checks p l.body <;> simp [h]
  | some a =>
    by_cases h : M.ck e.checks p l.body
    · by_cases he : l.emitter = a
      · subst he; simp [h, List.count_append]
      · have h2 : (a == l.emitter) = false := by
          simp only [beq_eq_false_iff_ne]; exact fun e => he e.symm
        have h3 : List.count a [l.emitter] = 0 := by
          simp only [List.count_cons, List.count_nil]
          have : (l.emitter == a) = false := by simp only [beq_eq_false_iff_ne]; exact he
          simp [this]
        simp [h, List.count_append, h2, h3]
    · simp [h]

/-- **Headline (per log).** The fixed rule marks a checked expectation found iff the log is
relevant and the relevant logs counted so far, summed over all emitters when no emitter is set,
reach `count`. -/
theorem step_found_iff (e : Exp B C) (p : B) (l : Log B) (hp : e.pat = some p) :
    (checkOne M relCount e p l).found = true ↔
      relevant M e l = true ∧ e.count ≤ relCount e + 1 := by
  have h := relCount_checkOne_seen M e p l hp
  unfold checkOne
  simp only
  rw [h]
  unfold relevant at *
  rw [hp] at *
  simp only at *
  by_cases hr : (M.ck e.checks p l.body && e.addr.all (· == l.emitter)) = true
  · simp only [hr, if_true]
    simp only [Bool.and_eq_true] at hr
    simp
  · simp only [Bool.not_eq_true] at hr
    simp only [hr, Bool.false_eq_true, if_false, false_and, iff_false]
    simp only [Bool.and_eq_false_iff] at hr
    rcases hr with hr | hr <;> simp

/-! ## The pending queue -/

/-- One log against the pending positive expectations: only the head is checked; once found it
leaves the queue. -/
def pstep : List (Exp B C) → Log B → List (Exp B C)
  | [], _ => []
  | e :: es, l =>
    match e.pat with
    | none => e :: es
    | some p =>
      let e' := checkOne M relCount e p l
      if e'.found then es else e' :: es

/-- `pstep` over a sequence of logs. -/
def prun : List (Exp B C) → List (Log B) → List (Exp B C)
  | pend, [] => pend
  | pend, l :: L => prun (pstep M pend l) L

/-- A pending expectation: filled, unfound and below its count. -/
def Pending (e : Exp B C) : Prop := e.pat.isSome ∧ e.found = false ∧ relCount e < e.count

theorem relIn_cons (e : Exp B C) (l : Log B) (L : List (Log B)) :
    relIn M e (l :: L) = (if relevant M e l then 1 else 0) + relIn M e L := by
  unfold relIn; by_cases h : relevant M e l <;> simp [h] <;> omega

theorem relIn_append (e : Exp B C) (L₁ L₂ : List (Log B)) :
    relIn M e (L₁ ++ L₂) = relIn M e L₁ + relIn M e L₂ := by
  unfold relIn; simp [List.filter_append]

theorem Sat_mono : ∀ (es : List (Exp B C)) (X L : List (Log B)), Sat M es L → Sat M es (X ++ L)
  | [], _, _, _ => trivial
  | _ :: _, X, _, ⟨L₁, L₂, rfl, hc, hs⟩ =>
    ⟨X ++ L₁, L₂, by simp, by rw [relIn_append]; omega, hs⟩

theorem Sat_nil_iff (es : List (Exp B C)) (h : ∀ e ∈ es, Pending e) : Sat M es [] ↔ es = [] := by
  cases es with
  | nil => simp [Sat]
  | cons e es =>
    simp only [Sat, reduceCtorEq, iff_false]
    rintro ⟨L₁, L₂, hL, hc, _⟩
    have h1 : L₁ = [] := (List.append_eq_nil_iff.1 hL.symm).1
    subst h1
    have := (h e List.mem_cons_self).2.2
    simp [relIn] at hc
    omega

theorem relCount_checkOne (e : Exp B C) (p : B) (l : Log B) (hp : e.pat = some p) :
    relCount (checkOne M relCount e p l) = relCount e + if relevant M e l then 1 else 0 := by
  rw [← relCount_checkOne_seen M e p l hp]; rfl

theorem Sat_cons_cons (e : Exp B C) (es : List (Exp B C)) (p : B) (l : Log B)
    (L : List (Log B)) (hp : e.pat = some p) (he : relCount e < e.count) :
    Sat M (e :: es) (l :: L) ↔
      if (checkOne M relCount e p l).found then Sat M es L
      else Sat M (checkOne M relCount e p l :: es) L := by
  have hf := step_found_iff M e p l hp
  have hc := relCount_checkOne M e p l hp
  have hcount : (checkOne M relCount e p l).count = e.count := rfl
  have hrel : relIn M (checkOne M relCount e p l) = relIn M e := rfl
  constructor
  · rintro ⟨L₁, L₂, hL, hcnt, hs⟩
    cases L₁ with
    | nil => simp [relIn] at hcnt; omega
    | cons x L₁ =>
      simp only [List.cons_append, List.cons.injEq] at hL
      obtain ⟨rfl, rfl⟩ := hL
      rw [relIn_cons] at hcnt
      split
      · exact Sat_mono M es L₁ L₂ hs
      · exact ⟨L₁, L₂, rfl, by rw [hcount, hc, hrel]; omega, hs⟩
  · intro h
    split at h
    · rename_i hfound
      obtain ⟨hr, hle⟩ := hf.1 hfound
      exact ⟨[l], L, rfl, by rw [relIn_cons]; simp [hr, relIn]; omega, h⟩
    · obtain ⟨L₁, L₂, rfl, hcnt, hs⟩ := h
      refine ⟨l :: L₁, L₂, rfl, ?_, hs⟩
      rw [hcount, hc, hrel] at hcnt
      rw [relIn_cons]
      omega

theorem pstep_pending (pend : List (Exp B C)) (l : Log B) (h : ∀ e ∈ pend, Pending e) :
    ∀ e ∈ pstep M pend l, Pending e := by
  cases pend with
  | nil => simp [pstep]
  | cons e es =>
    obtain ⟨hp, hfd, hlt⟩ := h e List.mem_cons_self
    obtain ⟨p, hp⟩ := Option.isSome_iff_exists.1 hp
    simp only [pstep, hp]
    split
    · exact fun x hx => h x (List.mem_cons_of_mem _ hx)
    · rename_i hnf
      intro x hx
      rcases List.mem_cons.1 hx with rfl | hx
      · have hf := step_found_iff M e p l hp
        have hc := relCount_checkOne M e p l hp
        refine ⟨by simp [checkOne, hp], by simpa using hnf, ?_⟩
        show relCount _ < e.count
        rw [hc]
        by_cases hr : relevant M e l = true
        · have : ¬ e.count ≤ relCount e + 1 := fun hle => hnf (hf.2 ⟨hr, hle⟩)
          simp [hr]; omega
        · simp [hr]; omega
      · exact h x (List.mem_cons_of_mem _ hx)

/-- The pending queue empties iff the logs satisfy `Sat`. -/
theorem prun_nil_iff : ∀ (pend : List (Exp B C)) (L : List (Log B)),
    (∀ e ∈ pend, Pending e) → (prun M pend L = [] ↔ Sat M pend L)
  | pend, [], h => by rw [Sat_nil_iff M pend h]; rfl
  | [], l :: L, _ => by
    simp only [prun, pstep]
    have : prun M ([] : List (Exp B C)) L = [] := by
      induction L with
      | nil => rfl
      | cons _ _ ih => simpa [prun, pstep] using ih
    simp [this, Sat]
  | e :: es, l :: L, h => by
    obtain ⟨hp, _, hlt⟩ := h e List.mem_cons_self
    obtain ⟨p, hp⟩ := Option.isSome_iff_exists.1 hp
    have ih := prun_nil_iff (pstep M (e :: es) l) L (pstep_pending M _ l h)
    simp only [prun] at ih ⊢
    rw [ih, Sat_cons_cons M e es p l L hp hlt]
    simp only [pstep, hp]
    split <;> rfl

/-! ## The full queue -/

/-- Expectations with a positive count. -/
def pos (e : Exp B C) : Bool := decide (0 < e.count)

/-- Expectations with `count = 0`. -/
def zero (e : Exp B C) : Bool := e.count == 0

/-- Invariant of the queue during the asserted call: every expectation is filled, `count = 0`
expectations are unfound with an empty count map, and the positive ones are the pending queue
`pend`, in order, followed by found ones whose count is exactly met. -/
def TInv (t pend : List (Exp B C)) : Prop :=
  (∀ e ∈ t, e.pat.isSome) ∧ (∀ e ∈ t, e.count = 0 → e.found = false ∧ e.seen = []) ∧
  (∀ e ∈ pend, Pending e) ∧
  ∃ done, t.filter pos = pend ++ done ∧ ∀ e ∈ done, e.found = true ∧ relCount e = e.count

theorem not_pos_iff (e : Exp B C) : pos e = false ↔ e.count = 0 := by
  unfold pos; simp

theorem mem_pend_or_done {t pend done : List (Exp B C)} (hf : t.filter pos = pend ++ done)
    {e : Exp B C} (he : e ∈ t) (hp : 0 < e.count) : e ∈ pend ∨ e ∈ done := by
  have : e ∈ t.filter pos := List.mem_filter.2 ⟨he, by simp [pos, hp]⟩
  rw [hf] at this; exact List.mem_append.1 this

/-- A `count = 0` expectation only fails on a log that matches it. -/
theorem observe_zero_err (t pend : List (Exp B C)) (l : Log B) (hi : TInv t pend)
    (h : ∃ z ∈ t, z.count = 0 ∧ relevant M z l = true) :
    (observe M t l).2 = some .countZero := by
  obtain ⟨z, hz, hz0, hr⟩ := h
  have hzf := (hi.2.1 z hz hz0).1
  have hall : t.all (·.found) = false := by
    rw [List.all_eq_false]; exact ⟨z, hz, by simp [hzf]⟩
  have hany : t.any (fun e => e.count == 0 && !e.found && relevant M e l) = true := by
    rw [List.any_eq_true]; exact ⟨z, hz, by simp [hz0, hzf, hr]⟩
  simp [observe, observeWith, hall, hany]

/-- One log that matches no `count = 0` expectation advances the pending queue by `pstep`,
preserves the invariant, and leaves the `count = 0` expectations unchanged. -/
theorem observe_step (t pend : List (Exp B C)) (l : Log B) (hi : TInv t pend)
    (h : ∀ z ∈ t, z.count = 0 → relevant M z l = false) :
    (observe M t l).2 = none ∧ TInv (observe M t l).1 (pstep M pend l) ∧
      (observe M t l).1.filter zero = t.filter zero := by
  obtain ⟨hfill, hzero, hpend, done, hf, hdone⟩ := hi
  have hany : t.any (fun e => e.count == 0 && !e.found && relevant M e l) = false := by
    rw [List.any_eq_false]
    intro z hz
    by_cases hz0 : z.count = 0
    · simp [h z hz hz0]
    · simp [hz0]
  have hnofill : t.any (·.pat.isNone) = false := by
    rw [List.any_eq_false]; intro e he; simp [Option.isSome_iff_ne_none.1 (hfill e he)]
  cases pend with
  | nil =>
    have hall : t.all (fun e => e.found || e.count == 0) = true := by
      rw [List.all_eq_true]
      intro e he
      by_cases h0 : e.count = 0
      · simp [h0]
      · rcases mem_pend_or_done hf he (by omega) with hm | hm
        · simp at hm
        · simp [(hdone e hm).1]
    have : observe M t l = (t, none) := by
      unfold observe observeWith
      by_cases ha : t.all (·.found) = true
      · simp [ha]
      · simp [ha, hany, hnofill, hall]
    rw [this]
    exact ⟨rfl, ⟨hfill, hzero, by simp [pstep], done, by simpa [pstep] using hf, hdone⟩, rfl⟩
  | cons e es =>
    obtain ⟨zs, rest, rfl, hzs, hpe, hrest⟩ := List.filter_eq_cons_iff.1 hf
    have hzs0 : ∀ z ∈ zs, z.count = 0 := fun z hz => (not_pos_iff z).1 (by simpa using hzs z hz)
    obtain ⟨hep, hef, helt⟩ := hpend e List.mem_cons_self
    obtain ⟨p, hp⟩ := Option.isSome_iff_exists.1 hep
    have hall : (zs ++ e :: rest).all (·.found) = false := by
      rw [List.all_eq_false]; exact ⟨e, by simp, by simp [hef]⟩
    have hall2 : (zs ++ e :: rest).all (fun e => e.found || e.count == 0) = false := by
      rw [List.all_eq_false]; exact ⟨e, by simp, by simp [hef]; omega⟩
    have hidx : (zs ++ e :: rest).findIdx (fun e => !e.found && decide (0 < e.count))
        = zs.length := by
      rw [List.findIdx_append]
      have : zs.findIdx (fun e => !e.found && decide (0 < e.count)) = zs.length :=
        List.findIdx_eq_length.2 (fun z hz => by simp [hzs0 z hz])
      rw [this]
      simp [List.findIdx_cons, hef, show 0 < e.count by omega]
    have hget : (zs ++ e :: rest)[zs.length]? = some e := by
      rw [List.getElem?_append_right (Nat.le_refl _)]; simp
    have herase : (zs ++ e :: rest).eraseIdx zs.length = zs ++ rest := by
      rw [List.eraseIdx_append_of_length_le (Nat.le_refl _)]; simp
    have hobs : observe M (zs ++ e :: rest) l =
        let e' := checkOne M relCount e p l
        if e'.found then (zs ++ rest ++ [e'], none) else (e' :: (zs ++ rest), none) := by
      unfold observe observeWith
      simp only [hall, hany, hnofill, hall2, hidx, hget, herase, hp, Bool.false_eq_true,
        if_false, Bool.not_false, Bool.true_and]
    have hf' := step_found_iff M e p l hp
    have hc := relCount_checkOne M e p l hp
    have hcnt : (checkOne M relCount e p l).count = e.count := rfl
    have hpat : (checkOne M relCount e p l).pat = some p := hp
    have hpos' : pos (checkOne M relCount e p l) = true := by simp [pos, hcnt]; omega
    have hzf : (checkOne M relCount e p l).count ≠ 0 := by rw [hcnt]; omega
    have hmem : ∀ x, x ∈ zs ++ rest → x ∈ zs ++ e :: rest := by
      intro x hx; simp only [List.mem_append, List.mem_cons] at hx ⊢; rcases hx with hx | hx <;> simp [hx]
    rw [hobs]
    simp only
    split
    · rename_i hfound
      obtain ⟨hr, hle⟩ := hf'.1 hfound
      refine ⟨rfl, ⟨?_, ?_, ?_, done ++ [checkOne M relCount e p l], ?_, ?_⟩, ?_⟩
      · intro x hx
        rcases List.mem_append.1 hx with hx | hx
        · exact hfill x (hmem x hx)
        rw [List.mem_singleton.1 hx]
        · simp [hpat]
      · intro x hx hx0
        rcases List.mem_append.1 hx with hx | hx
        · exact hzero x (hmem x hx) hx0
        rw [List.mem_singleton.1 hx] at hx0
        · exact absurd hx0 hzf
      · simp only [pstep, hp, hfound, if_true]
        exact fun x hx => hpend x (List.mem_cons_of_mem _ hx)
      · simp only [pstep, hp, hfound, if_true]
        rw [List.filter_append, List.filter_append, List.filter_eq_nil_iff.2 hzs, hrest]
        simp [hpos']
      · intro x hx
        simp only [List.mem_append, List.mem_singleton] at hx
        rcases hx with hx | rfl
        · exact hdone x hx
        · refine ⟨hfound, ?_⟩
          rw [hc, hcnt]; simp [hr]; omega
      · simp [List.filter_append, zero, hzf, show e.count ≠ 0 by omega]
    · rename_i hnf
      refine ⟨rfl, ⟨?_, ?_, ?_, done, ?_, hdone⟩, ?_⟩
      · intro x hx
        rcases List.mem_cons.1 hx with rfl | hx
        · simp [hpat]
        · exact hfill x (hmem x hx)
      · intro x hx hx0
        rcases List.mem_cons.1 hx with rfl | hx
        · exact absurd hx0 hzf
        · exact hzero x (hmem x hx) hx0
      · exact pstep_pending M (e :: es) l hpend
      · simp only [pstep, hp, hnf, if_false, Bool.false_eq_true]
        rw [List.filter_cons, List.filter_append, List.filter_eq_nil_iff.2 hzs, hrest]
        simp [hpos']
      · simp [List.filter_append, zero, hzf, show e.count ≠ 0 by omega]

theorem mem_zero_iff (t : List (Exp B C)) (z : Exp B C) :
    z ∈ t.filter zero ↔ z ∈ t ∧ z.count = 0 := by
  simp [List.mem_filter, zero]

/-- The run preserves the invariant, and fails iff some log matches a `count = 0`
expectation. -/
theorem run_inv : ∀ (L : List (Log B)) (t pend : List (Exp B C)), TInv t pend →
    ((run M t L).2 = none ↔ ∀ l ∈ L, ∀ z ∈ t, z.count = 0 → relevant M z l = false) ∧
    ((run M t L).2 = none → TInv (run M t L).1 (prun M pend L))
  | [], t, pend, hi => by simp [run, runWith, prun, hi]
  | l :: L, t, pend, hi => by
    by_cases hz : ∀ z ∈ t, z.count = 0 → relevant M z l = false
    · obtain ⟨hnone, hi', hzeq⟩ := observe_step M t pend l hi hz
      have hrun : run M t (l :: L) = run M (observe M t l).1 L := by
        simp only [run, runWith]
        unfold observe at hnone ⊢
        cases h : observeWith M relCount t l with
        | mk t' err => rw [h] at hnone; simp at hnone; subst hnone; rfl
      have ih := run_inv L _ _ hi'
      rw [hrun]
      refine ⟨?_, ?_⟩
      · rw [ih.1]
        constructor
        · intro h' l' hl' z hz' hz0
          rcases List.mem_cons.1 hl' with rfl | hl'
          · exact hz z hz' hz0
          · have : z ∈ (observe M t l).1.filter zero := by
              rw [hzeq]; exact (mem_zero_iff t z).2 ⟨hz', hz0⟩
            exact h' l' hl' z ((mem_zero_iff _ z).1 this).1 hz0
        · intro h' l' hl' z hz' hz0
          have : z ∈ t.filter zero := by
            rw [← hzeq]; exact (mem_zero_iff _ z).2 ⟨hz', hz0⟩
          exact h' l' (List.mem_cons_of_mem _ hl') z ((mem_zero_iff t z).1 this).1 hz0
      · intro h'; exact ih.2 h'
    · have herr : (run M t (l :: L)).2 = some .countZero := by
        have hex : ∃ z ∈ t, z.count = 0 ∧ relevant M z l = true := by
          refine Classical.byContradiction fun hne => hz fun z hz' hz0 => ?_
          cases hr : relevant M z l
          · rfl
          · exact absurd ⟨z, hz', hz0, hr⟩ hne
        have ho := observe_zero_err M t pend l hi hex
        simp only [run, runWith]
        unfold observe at ho
        cases h : observeWith M relCount t l with
        | mk t' err => rw [h] at ho; simp at ho; subst ho; rfl
      rw [herr]
      simp only [reduceCtorEq, false_iff, false_implies, and_true]
      intro h'
      exact hz (h' l List.mem_cons_self)

/-- At the call end, the queue passes and is cleared iff no positive expectation is pending. -/
theorem checkCall_iff (t pend : List (Exp B C)) (hi : TInv t pend) :
    checkCall t = none ↔ pend = [] := by
  obtain ⟨_, hzero, hpend, done, hf, hdone⟩ := hi
  unfold checkCall
  constructor
  · intro h
    cases pend with
    | nil => rfl
    | cons e es =>
      exfalso
      have he : e ∈ t := (List.mem_filter.1 (hf ▸ List.mem_cons_self : e ∈ t.filter pos)).1
      obtain ⟨_, hef, hlt⟩ := hpend e List.mem_cons_self
      have : t.any (fun e => !e.found && decide (0 < e.count)) = true :=
        List.any_eq_true.2 ⟨e, he, by simp [hef]; omega⟩
      simp [this] at h
  · rintro rfl
    have h1 : t.any (fun e => !e.found && decide (0 < e.count)) = false := by
      rw [List.any_eq_false]
      intro e he
      by_cases h0 : e.count = 0
      · simp [h0]
      · rcases mem_pend_or_done hf he (by omega) with hm | hm
        · simp at hm
        · simp [(hdone e hm).1]
    have h2 : t.any (fun e => relCount e != e.count) = false := by
      rw [List.any_eq_false]
      intro e he
      by_cases h0 : e.count = 0
      · have := (hzero e he h0).2
        simp [relCount, this, h0]; cases e.addr <;> simp
      · rcases mem_pend_or_done hf he (by omega) with hm | hm
        · simp at hm
        · simp [(hdone e hm).2]
    simp [h1, h2]

/-- At the end of the test, an expectation is left unmet iff a positive one is pending. -/
theorem rootUnmet_iff (t pend : List (Exp B C)) (hi : TInv t pend) :
    rootUnmet t = false ↔ pend = [] := by
  have := checkCall_iff t pend hi
  obtain ⟨_, hzero, hpend, done, hf, hdone⟩ := hi
  unfold rootUnmet
  constructor
  · intro h
    cases pend with
    | nil => rfl
    | cons e es =>
      exfalso
      have he : e ∈ t := (List.mem_filter.1 (hf ▸ List.mem_cons_self : e ∈ t.filter pos)).1
      obtain ⟨_, hef, hlt⟩ := hpend e List.mem_cons_self
      have : t.any (fun e => !e.found && decide (0 < e.count)) = true :=
        List.any_eq_true.2 ⟨e, he, by simp [hef]; omega⟩
      simp [this] at h
  · intro hp
    have h := this.2 hp
    unfold checkCall at h
    by_cases h1 : t.any (fun e => !e.found && decide (0 < e.count)) = true
    · simp [h1] at h
    · simpa using h1

theorem fresh_inv (t : List (Exp B C)) (h : ∀ e ∈ t, Fresh e) : TInv t (t.filter pos) := by
  refine ⟨fun e he => (h e he).1, fun e he _ => ⟨(h e he).2.1, (h e he).2.2⟩, ?_, [], by simp,
    by simp⟩
  intro e he
  obtain ⟨he, hp⟩ := List.mem_filter.1 he
  obtain ⟨h1, h2, h3⟩ := h e he
  refine ⟨h1, h2, ?_⟩
  simp only [pos, decide_eq_true_eq] at hp
  unfold relCount; rw [h3]; cases e.addr <;> simpa

/-- **Headline.** From a freshly declared queue, the logs `L` of the asserted call never trip a
`count = 0` expectation iff none of them matches one; in that case the call-end check passes
(and clears the queue) iff the positive expectations, in declaration order, are matched by
consecutive blocks of `L` with at least `count` matching logs each. -/
theorem run_spec (t : List (Exp B C)) (L : List (Log B)) (h : ∀ e ∈ t, Fresh e) :
    ((run M t L).2 = none ↔ ∀ l ∈ L, ∀ z ∈ t, z.count = 0 → relevant M z l = false) ∧
    ((run M t L).2 = none → (checkCall (run M t L).1 = none ↔ Sat M (t.filter pos) L)) := by
  have hi := fresh_inv t h
  have hr := run_inv M L t _ hi
  refine ⟨hr.1, fun hn => ?_⟩
  rw [checkCall_iff _ _ (hr.2 hn), prun_nil_iff M _ L hi.2.2.1]

/-- `es` and `L` have the same length and each expectation matches the log at its position. -/
inductive AllMatch : List (Exp B C) → List (Log B) → Prop
  | nil : AllMatch [] []
  | cons {e es l L} : relevant M e l = true → AllMatch es L → AllMatch (e :: es) (l :: L)

/-- With `count = 1` everywhere, `Sat` is ordered-subsequence matching: the expectations match,
one to one and in order, a subsequence of the logs; other logs may come before, between and
after them. -/
theorem Sat_count_one_iff : ∀ (es : List (Exp B C)) (L : List (Log B)),
    (∀ e ∈ es, e.count = 1 ∧ relCount e = 0) →
    (Sat M es L ↔ ∃ L', L'.Sublist L ∧ AllMatch M es L')
  | [], L, _ => by simp only [Sat, true_iff]; exact ⟨[], List.nil_sublist _, .nil⟩
  | e :: es, L, h => by
    obtain ⟨h1, h0⟩ := h e List.mem_cons_self
    have ih := fun L => Sat_count_one_iff es L (fun x hx => h x (List.mem_cons_of_mem _ hx))
    constructor
    · rintro ⟨L₁, L₂, rfl, hc, hs⟩
      rw [h1, h0] at hc
      obtain ⟨l, hl⟩ := List.length_pos_iff_exists_mem.1 (show 0 < (L₁.filter (relevant M e)).length by unfold relIn at hc; omega)
      obtain ⟨hl, hr⟩ := List.mem_filter.1 hl
      obtain ⟨L', hsub, hf⟩ := (ih L₂).1 hs
      exact ⟨l :: L', (List.singleton_sublist.2 hl).append hsub, .cons hr hf⟩
    · rintro ⟨L', hsub, hf⟩
      cases hf with
      | cons hr hf =>
        rename_i l L''
        obtain ⟨r₁, r₂, rfl, hl, hsub'⟩ := List.cons_sublist_iff.1 hsub
        refine ⟨r₁, r₂, rfl, ?_, (ih r₂).2 ⟨L'', hsub', hf⟩⟩
        rw [h1, h0]
        have : 0 < (r₁.filter (relevant M e)).length :=
          List.length_pos_iff_exists_mem.2 ⟨l, List.mem_filter.2 ⟨hl, hr⟩⟩
        unfold relIn; omega

/-! ## The pre-fix count rule -/

/-- The pre-fix rule: without an emitter, `found` read the count of the first emitter in
`HashMap` iteration order, chosen here by the oracle `pick`. -/
def oldCnt (pick : List Nat → Nat) (e : Exp B C) : Nat :=
  match e.addr with
  | some a => e.seen.count a
  | none => if e.seen = [] then 0 else e.seen.count (pick e.seen)

/-- `expectEmit(count = 2)` without an emitter, and two matching logs from different emitters:
the specification and the fixed matcher accept the trace, and the pre-fix matcher rejects it for
every iteration order. -/
theorem old_misses_two_emitters (b : B) (c : C) (pick : List Nat → Nat)
    (hpick : ∀ s, s ≠ [] → pick s ∈ s) :
    let e : Exp B C := ⟨some b, c, none, false, false, 2, []⟩
    let L : List (Log B) := [⟨1, b⟩, ⟨2, b⟩]
    Sat M [e] L ∧ checkCall (run M [e] L).1 = none ∧
      checkCall (runWith M (oldCnt pick) [e] L).1 = some .unmatched := by
  intro e L
  have hfresh : ∀ x ∈ [e], Fresh x := by simp [e, Fresh]
  have hsat : Sat M [e] L :=
    ⟨L, [], by simp, by simp [L, relIn, relevant, relCount, e, M.refl], trivial⟩
  have hspec := run_spec M [e] L hfresh
  have hnone : (run M [e] L).2 = none := hspec.1.2 (by simp [e])
  refine ⟨hsat, (hspec.2 hnone).2 (by simpa [pos, e] using hsat), ?_⟩
  have h1 : pick [1] = 1 := by simpa using hpick [1] (by simp)
  have h2 : [1, 2].count (pick [1, 2]) ≤ 1 := by
    have := hpick [1, 2] (by simp)
    simp at this; rcases this with h | h <;> simp [h]
  have h2' : decide (2 ≤ [1, 2].count (pick [1, 2])) = false := by simp; omega
  simp [runWith, observeWith, checkOne, oldCnt, relevant, M.refl, e, L, h1, h2', checkCall]

/-! ## The concrete matching function -/

/-- `LogData`: topics and data. -/
structure RawLog where
  topics : List Nat
  data : List Nat
  deriving DecidableEq, Repr

/-- Models `checks_topics_and_data`: the topic counts agree, each checked topic agrees, and the
data agrees if checked (flag 4). -/
def checksTopicsAndData (checks : List Bool) (expected log : RawLog) : Bool :=
  log.topics.length == expected.topics.length &&
    (List.range log.topics.length).all
      (fun i => !(checks.getD i false) || log.topics[i]? == expected.topics[i]?) &&
    (!(checks.getD 4 false) || expected.data == log.data)

theorem checksTopicsAndData_refl (checks : List Bool) (b : RawLog) :
    checksTopicsAndData checks b b = true := by
  simp [checksTopicsAndData]

/-- The matcher of `expected_emit.rs`, for which all theorems above hold. -/
def rawMatcher : Matcher RawLog (List Bool) where
  ck := checksTopicsAndData
  noTopics b := b.topics.isEmpty
  refl := checksTopicsAndData_refl

end Cheatcodes.ExpectEmit
