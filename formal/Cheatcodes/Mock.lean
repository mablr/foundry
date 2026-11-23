/-!
# Mock-call lookup

Model of `find_mock_returns` in `crates/cheatcodes/src/evm/mock.rs`, which finds the mock for a
call by ranging over a `BTreeMap` whose key order (`Ord for MockCallDataContext`) is reversed.

Fidelity:
- Calldata is a `List Nat` of bytes; `Ord for [u8]` is `lexLt` (lexicographic, a proper prefix
  first). `Option<U256>` is `Option Nat` with `None < Some`.
- The `BTreeMap` is its iteration order: a list strictly sorted by the key order `keyLt`, which
  is the reversed `Ord` from `mock.rs`. `range_mut(ctx..)` is the sublist of keys `≥ ctx`, in
  order.
- `input.get(..mock.calldata.len()) == Some(&mock.calldata[..])` is `mock.cd <+: input`.

The headline theorem `findMock_spec` states that the lookup returns the matching mock with the
longest calldata prefix and, for equal calldata, the value-specific one, and that no matching mock
is skipped by the range: the reversed order sorts every prefix of the input at or after the input.
-/

namespace Cheatcodes.Mock

/-- Lexicographic `<` on byte strings, as `Ord for [u8]`. -/
def lexLt : List Nat → List Nat → Bool
  | [], [] => false
  | [], _ :: _ => true
  | _ :: _, [] => false
  | a :: as, b :: bs => a < b || (a == b && lexLt as bs)

/-- `<` on `Option<U256>`: `None` is the least element. -/
def optLt : Option Nat → Option Nat → Bool
  | none, some _ => true
  | some a, some b => a < b
  | _, _ => false

/-- `MockCallDataContext`: the calldata prefix and optional `msg.value` of a mock. -/
structure Key where
  cd : List Nat
  val : Option Nat
  deriving DecidableEq, Repr

/-- The reversed order of `Ord for MockCallDataContext`: calldata descending, then value
descending. -/
def keyLt (a b : Key) : Bool := lexLt b.cd a.cd || (a.cd == b.cd && optLt b.val a.val)

/-- Whether a mock applies to a call with `input` and `value`. -/
def Matches (input : List Nat) (value : Option Nat) (k : Key) : Prop :=
  k.cd <+: input ∧ (k.val = none ∨ k.val = value)

instance (input : List Nat) (value : Option Nat) (k : Key) : Decidable (Matches input value k) := by
  unfold Matches; infer_instance

/-- Models `find_mock_returns`: the first entry, in map order from the key of the call, whose key
matches. -/
def findMock {α : Type} (m : List (Key × α)) (input : List Nat) (value : Option Nat) :
    Option (Key × α) :=
  let ctx : Key := ⟨input, value⟩
  (m.filter fun p => !keyLt p.1 ctx).find? fun p => decide (Matches input value p.1)

/-- A well-formed map: strictly sorted by the key order. -/
def Sorted {α : Type} (m : List (Key × α)) : Prop := m.Pairwise fun p q => keyLt p.1 q.1

/-- The precedence the lookup must implement: longer calldata wins, then a value-specific mock
wins over a value-agnostic one. `Better a b` means `a` takes precedence over `b`. -/
def Better (a b : Key) : Prop :=
  b.cd.length < a.cd.length ∨ (b.cd.length = a.cd.length ∧ b.val = none ∧ a.val ≠ none)

theorem lexLt_irrefl (a : List Nat) : lexLt a a = false := by
  induction a with
  | nil => rfl
  | cons x xs ih => simp [lexLt, ih]

theorem lexLt_asymm (a b : List Nat) (h : lexLt a b = true) : lexLt b a = false := by
  induction a generalizing b with
  | nil => cases b <;> simp_all [lexLt]
  | cons x xs ih =>
    cases b with
    | nil => simp [lexLt] at h
    | cons y ys =>
      simp only [lexLt, Bool.or_eq_true, Bool.and_eq_true, beq_iff_eq, decide_eq_true_eq] at h ⊢
      rcases h with h | ⟨rfl, h⟩
      · simp only [Bool.or_eq_false_iff, decide_eq_false_iff_not, Bool.and_eq_false_iff,
          beq_eq_false_iff_ne]
        omega
      · simp [ih ys h]

/-- A proper prefix is lexicographically smaller. -/
theorem lexLt_of_prefix (a b : List Nat) (h : a <+: b) (hne : a ≠ b) : lexLt a b = true := by
  induction a generalizing b with
  | nil => cases b <;> simp_all [lexLt]
  | cons x xs ih =>
    cases b with
    | nil => simp at h
    | cons y ys =>
      rw [List.cons_prefix_cons] at h
      obtain ⟨rfl, h⟩ := h
      have : xs ≠ ys := by intro e; exact hne (e ▸ rfl)
      simp [lexLt, ih ys h this]

/-- Among two prefixes of the same input, the lexicographically smaller one is shorter. -/
theorem length_lt_of_lexLt {a b input : List Nat} (ha : a <+: input) (hb : b <+: input)
    (h : lexLt a b = true) : a.length < b.length := by
  rcases Nat.lt_or_ge a.length b.length with hl | hl
  · exact hl
  · exfalso
    have hba : b <+: a := List.prefix_of_prefix_length_le hb ha hl
    by_cases e : b = a
    · subst e; simp [lexLt_irrefl] at h
    · have := lexLt_asymm _ _ (lexLt_of_prefix b a hba e); simp_all

/-- Every matching mock is in `range_mut(ctx..)`: the reversed order never sorts it before the
key of the call. This rules out a counterexample in which interleaving keys hide a prefix. -/
theorem matches_in_range (input : List Nat) (value : Option Nat) (k : Key)
    (h : Matches input value k) : keyLt k ⟨input, value⟩ = false := by
  obtain ⟨hp, hv⟩ := h
  unfold keyLt
  simp only [Bool.or_eq_false_iff, Bool.and_eq_false_iff, beq_eq_false_iff_ne]
  constructor
  · by_cases e : k.cd = input
    · rw [e]; exact lexLt_irrefl _
    · exact lexLt_asymm _ _ (lexLt_of_prefix _ _ hp e)
  · by_cases e : k.cd = input
    · right; rcases hv with hv | hv <;> rw [hv] <;> cases value <;> simp [optLt]
    · left; exact e

theorem optLt_matching (value x y : Option Nat) (hx : x = none ∨ x = value)
    (hy : y = none ∨ y = value) (hxy : optLt y x = true) : y = none ∧ x ≠ none := by
  cases x <;> cases y <;> cases value <;> simp_all [optLt] <;> omega

/-- Among matching keys, map order is precedence order. -/
theorem better_of_keyLt {input : List Nat} {value : Option Nat} {a b : Key}
    (ha : Matches input value a) (hb : Matches input value b) (h : keyLt a b = true) :
    Better a b := by
  unfold keyLt at h
  simp only [Bool.or_eq_true, Bool.and_eq_true, beq_iff_eq] at h
  rcases h with h | ⟨e, h⟩
  · left; exact length_lt_of_lexLt hb.1 ha.1 h
  · right
    exact ⟨by rw [e], optLt_matching value a.val b.val ha.2 hb.2 h⟩

/-- `find?` on a sorted list returns the least satisfying element. -/
theorem find?_least {β : Type} (R : β → β → Bool) (P : β → Bool) :
    ∀ (l : List β) (x : β), l.Pairwise (fun a b => R a b) → l.find? P = some x →
      x ∈ l ∧ P x ∧ ∀ y ∈ l, P y → y = x ∨ R x y
  | [], _, _, h => by simp at h
  | a :: l, x, hs, h => by
    rw [List.pairwise_cons] at hs
    by_cases hp : P a
    · simp only [List.find?_cons, hp] at h
      cases h
      refine ⟨List.mem_cons_self, hp, fun y hy _ => ?_⟩
      rcases List.mem_cons.1 hy with rfl | hy
      · exact .inl rfl
      · exact .inr (hs.1 y hy)
    · simp only [List.find?_cons, hp] at h
      obtain ⟨hx, hpx, hmin⟩ := find?_least R P l x hs.2 h
      refine ⟨List.mem_cons_of_mem _ hx, hpx, fun y hy hpy => ?_⟩
      rcases List.mem_cons.1 hy with rfl | hy
      · exact absurd hpy hp
      · exact hmin y hy hpy

/-- Two matching keys with the same precedence are the same key, so the best mock is unique. -/
theorem eq_of_not_better {input : List Nat} {value : Option Nat} {a b : Key}
    (ha : Matches input value a) (hb : Matches input value b) (h1 : ¬ Better a b)
    (h2 : ¬ Better b a) : a = b := by
  unfold Better at h1 h2
  have hl : a.cd.length = b.cd.length := by omega
  have hcd : a.cd = b.cd := by
    have := List.prefix_of_prefix_length_le ha.1 hb.1 (Nat.le_of_eq hl)
    exact this.eq_of_length hl
  have hv : a.val = b.val := by
    rcases ha.2 with h | h <;> rcases hb.2 with h' | h'
    · rw [h, h']
    · cases hv : value with
      | none => rw [h, h', hv]
      | some v => exfalso; apply h2; right; exact ⟨hl, h, by rw [h', hv]; simp⟩
    · cases hv : value with
      | none => rw [h, h', hv]
      | some v => exfalso; apply h1; right; exact ⟨hl.symm, h', by rw [h, hv]; simp⟩
    · rw [h, h']
  cases a; cases b; simp_all

/-- **Headline.** On a well-formed map, `find_mock_returns` finds a mock iff some mock matches the
call, and the mock it returns matches and takes precedence over every other matching mock: it has
the longest calldata prefix of the input and, for equal calldata, the mock with the call's value
beats the value-agnostic one. With `eq_of_not_better`, it is the unique best mock. -/
theorem findMock_spec {α : Type} (m : List (Key × α)) (hs : Sorted m) (input : List Nat)
    (value : Option Nat) :
    (findMock m input value = none ↔ ∀ p ∈ m, ¬ Matches input value p.1) ∧
    ∀ p, findMock m input value = some p →
      p ∈ m ∧ Matches input value p.1 ∧
      ∀ q ∈ m, Matches input value q.1 → q.1 = p.1 ∨ Better p.1 q.1 := by
  have mem_range : ∀ q ∈ m, Matches input value q.1 →
      q ∈ m.filter fun p => !keyLt p.1 ⟨input, value⟩ := by
    intro q hq hm
    simp [List.mem_filter, hq, matches_in_range input value q.1 hm]
  constructor
  · unfold findMock
    simp only [List.find?_eq_none, decide_eq_true_eq]
    constructor
    · intro h q hq hm; exact h q (mem_range q hq hm) hm
    · intro h q hq; exact h q (List.mem_filter.1 hq).1
  · intro p hp
    have hsf : (m.filter fun p => !keyLt p.1 ⟨input, value⟩).Pairwise
        (fun a b => keyLt a.1 b.1) := List.Pairwise.filter _ hs
    obtain ⟨hx, hpx, hmin⟩ := find?_least _ _ _ p hsf hp
    refine ⟨(List.mem_filter.1 hx).1, of_decide_eq_true hpx, fun q hq hm => ?_⟩
    rcases hmin q (mem_range q hq hm) (decide_eq_true hm) with e | h
    · exact .inl (e ▸ rfl)
    · exact .inr (better_of_keyLt (of_decide_eq_true hpx) hm h)

/-- A mock whose calldata and value equal the call's is always the one used. -/
theorem findMock_exact {α : Type} (m : List (Key × α)) (hs : Sorted m) (input : List Nat)
    (value : Option Nat) (r : α) (hm : (⟨input, value⟩, r) ∈ m) :
    ∃ p, findMock m input value = some p ∧ p.1 = ⟨input, value⟩ := by
  have hmat : Matches input value ⟨input, value⟩ := ⟨List.prefix_refl _, .inr rfl⟩
  obtain ⟨hnone, hsome⟩ := findMock_spec m hs input value
  cases h : findMock m input value with
  | none => exact absurd (hnone.1 h _ hm) (by simpa using hmat)
  | some p =>
    refine ⟨p, rfl, ?_⟩
    obtain ⟨_, hpm, hbest⟩ := hsome p h
    rcases hbest _ hm hmat with e | hb
    · exact e.symm
    · exfalso
      have hlen := hpm.1.length_le
      unfold Better at hb
      simp only at hb
      rcases hb with hb | ⟨_, hv, hne⟩
      · omega
      · rcases hpm.2 with h' | h' <;> simp_all

/-- A call without a transfer value never uses a value-specific mock, even one for value `0`. -/
theorem findMock_no_value {α : Type} (m : List (Key × α)) (hs : Sorted m) (input : List Nat)
    (p : Key × α) (h : findMock m input none = some p) : p.1.val = none := by
  obtain ⟨_, hpm, _⟩ := (findMock_spec m hs input none).2 p h
  rcases hpm.2 with h | h <;> exact h

end Cheatcodes.Mock
