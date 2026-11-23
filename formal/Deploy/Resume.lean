/-!
# Script resume: the completed transaction prefix

Models `completed_transaction_prefix` in `crates/script/src/sequence.rs` (through its pure core
`completed_prefix`). On `forge script --resume`, the returned index `k` is where broadcasting
restarts: transactions `0..k` are treated as mined and never re-sent, transactions `k..` are
(re)broadcast. A wrong `k` therefore either double-sends a transaction or skips one.

## Fidelity

- A transaction is `Option H`: its recorded hash, or `none` if it was never signed or sent.
  `H` is any type with decidable equality (Rust: `B256`).
- The receipt count map (`HashMap<B256, usize>`) is modelled as the list of remaining receipt
  hashes. Decrementing a count is `List.erase`; "all counts are zero" is "the list is empty".
  Both represent the same multiset.
- Rust bails at the first complete transaction after an incomplete one, and after the loop if a
  receipt is left over. Both are `none` here; the model does not distinguish the two messages.
-/

namespace Deploy.Resume

variable {H : Type} [DecidableEq H]

/-- The greedy left-to-right scan of `completed_prefix`. `rem` is the multiset of receipts not
yet matched to a transaction, `inc` records whether an incomplete transaction was seen, and `p`
counts completed transactions. -/
def scan : List (Option H) → List H → Bool → Nat → Option Nat
  | [], rem, _, p => if rem.isEmpty then some p else none
  | some h :: ts, rem, inc, p =>
    if h ∈ rem then (if inc then none else scan ts (rem.erase h) inc (p + 1))
    else scan ts rem true p
  | none :: ts, rem, _, p => scan ts rem true p

/-- Model of `completed_prefix(hashes, receipts)`: `some k` is `Ok(k)`, `none` is an error. -/
def completedPrefix (txs : List (Option H)) (receipts : List H) : Option Nat :=
  scan txs receipts false 0

/-- The contract: the first `k` transactions all have hashes, and the receipts are exactly the
multiset of those hashes. Nothing is said about the later transactions; they are re-broadcast. -/
def Spec (txs : List (Option H)) (receipts : List H) (k : Nat) : Prop :=
  ∃ (hs : List H) (rest : List (Option H)), txs = hs.map some ++ rest ∧ hs.Perm receipts ∧ hs.length = k

/-- Once an incomplete transaction was seen, the scan succeeds iff no receipt is left. -/
theorem scan_incomplete (ts : List (Option H)) (rem : List H) (p : Nat) :
    scan ts rem true p = if rem = [] then some p else none := by
  induction ts with
  | nil => cases rem <;> simp [scan]
  | cons t ts ih =>
    cases t with
    | none => simp [scan, ih]
    | some h =>
      by_cases hm : h ∈ rem
      · have : rem ≠ [] := by intro h'; simp [h'] at hm
        simp [scan, hm, this]
      · simp [scan, hm, ih]

theorem scan_complete (ts : List (Option H)) (rem : List H) (p r : Nat) :
    scan ts rem false p = some r ↔
      ∃ (hs : List H) (rest : List (Option H)), ts = hs.map some ++ rest ∧ hs.Perm rem ∧ r = p + hs.length := by
  induction ts generalizing rem p with
  | nil =>
    constructor
    · intro h
      cases rem with
      | nil => simp [scan] at h; exact ⟨[], [], by simp, by simp, by simp_all⟩
      | cons _ _ => simp [scan] at h
    · rintro ⟨hs, rest, he, hp, hr⟩
      have : hs = [] := by cases hs <;> simp_all
      subst this
      have : rem = [] := List.nil_perm.mp hp
      subst this
      simp [scan, hr]
  | cons t ts ih =>
    cases t with
    | none =>
      simp only [scan, scan_incomplete]
      constructor
      · intro h
        split at h
        · subst_vars; simp at h; exact ⟨[], none :: ts, by simp, by simp, by simp_all⟩
        · simp at h
      · rintro ⟨hs, rest, he, hp, hr⟩
        cases hs with
        | cons _ _ => simp at he
        | nil => have := List.nil_perm.mp hp; subst this; simp [hr]
    | some h =>
      by_cases hm : h ∈ rem
      · simp only [scan, hm, if_true, Bool.false_eq_true, if_false, ih]
        constructor
        · rintro ⟨hs, rest, he, hp, hr⟩
          refine ⟨h :: hs, rest, by simp [he], ?_, by simp; omega⟩
          exact List.cons_perm_iff_perm_erase.mpr ⟨hm, hp⟩
        · rintro ⟨hs, rest, he, hp, hr⟩
          cases hs with
          | nil =>
            have := List.nil_perm.mp hp; subst this; simp at hm
          | cons h' hs =>
            simp at he
            obtain ⟨rfl, rfl⟩ := he
            refine ⟨hs, rest, rfl, (List.cons_perm_iff_perm_erase.mp hp).2, ?_⟩
            simp at hr; omega
      · simp only [scan, hm, if_false, scan_incomplete]
        constructor
        · intro hs
          split at hs
          · subst_vars; simp at hs
            exact ⟨[], some h :: ts, by simp, by simp, by simp_all⟩
          · simp at hs
        · rintro ⟨hs, rest, he, hp, hr⟩
          cases hs with
          | nil => have := List.nil_perm.mp hp; subst this; simp [hr]
          | cons h' hs =>
            simp at he
            obtain ⟨rfl, -⟩ := he
            exact absurd ((List.cons_perm_iff_perm_erase.mp hp).1) hm

/-- **Headline.** `completed_prefix` returns `Ok(k)` exactly when `k` satisfies the contract. -/
theorem completedPrefix_iff (txs : List (Option H)) (receipts : List H) (k : Nat) :
    completedPrefix txs receipts = some k ↔ Spec txs receipts k := by
  unfold completedPrefix Spec
  rw [scan_complete]
  constructor
  · rintro ⟨hs, rest, he, hp, hr⟩; exact ⟨hs, rest, he, hp, by omega⟩
  · rintro ⟨hs, rest, he, hp, hr⟩; exact ⟨hs, rest, he, hp, by omega⟩

omit [DecidableEq H] in
/-- The resume index is determined by the receipts alone: it is their count. -/
theorem spec_length {txs : List (Option H)} {receipts : List H} {k : Nat}
    (h : Spec txs receipts k) : k = receipts.length := by
  obtain ⟨hs, _, _, hp, rfl⟩ := h
  exact hp.length_eq

omit [DecidableEq H] in
/-- At most one resume index satisfies the contract, so the result is never a matter of
choice: an error means no index is consistent with the recorded receipts. -/
theorem spec_unique {txs : List (Option H)} {receipts : List H} {k₁ k₂ : Nat}
    (h₁ : Spec txs receipts k₁) (h₂ : Spec txs receipts k₂) : k₁ = k₂ := by
  rw [spec_length h₁, spec_length h₂]

/-- No double-send and no skip: on `Ok(k)`, every receipt is accounted for by exactly one
transaction before `k` (as a multiset), and the transactions re-sent are exactly `txs.drop k`. -/
theorem completedPrefix_split {txs : List (Option H)} {receipts : List H} {k : Nat}
    (h : completedPrefix txs receipts = some k) :
    ∃ hs : List H, txs.take k = hs.map some ∧ hs.Perm receipts ∧ k ≤ txs.length := by
  obtain ⟨hs, rest, rfl, hp, rfl⟩ := (completedPrefix_iff _ _ _).mp h
  refine ⟨hs, ?_, hp, by simp⟩
  simp

/-! Concrete checks, mirrored by the Rust exhaustive test. -/

example : completedPrefix [some 1, some 2, none] [2, 1] = some 2 := by decide
example : completedPrefix [none, some 1] [1] = none := by decide
example : completedPrefix [some 1, some 1] [1] = some 1 := by decide
example : completedPrefix [some 1] [1, 1] = none := by decide
example : completedPrefix ([] : List (Option Nat)) [] = some 0 := by decide

end Deploy.Resume
