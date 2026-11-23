/-!
# Deployed-bytecode matching: ignored ranges and the segment comparison

Models two pieces of `crates/common/src/contracts.rs`:

- `normalize_offsets`, which sorts the ignored byte ranges (immutable references, link
  references, library call protection, metadata) and merges overlapping or adjacent ones;
- the segment loop of `ContractsByArtifact::find_by_deployed_code_exact_inner`, which compares
  the local artifact with the onchain code on the bytes between consecutive ignored ranges, then
  on the suffix after the last one.

## Fidelity

- `Offsets { start: u32, length: u32 }` is `Range` over `Nat`. Rust computes the merged end in
  `u64` and narrows the merged length back to `u32`; `normalize_stop_le` shows every merged end is
  at most the largest input end, so the narrowing is exact whenever every input end fits in `u32`
  (always the case for real bytecode, which is far below 4 GiB).
- `sort_by_key(|o| o.start)` is `List.mergeSort` on `start`. Both are stable; the theorems only
  use that the result is a sorted permutation.
- The local artifact is a `List (Option α)`: `some b` is a byte, `none` a position whose two hex
  characters do not decode (an unlinked `__$…$__` placeholder). For `BytecodeObject::Bytecode`
  every entry is `some`. A Rust slice comparison `local[l..r] == code[l..r]` (after hex decoding
  for `Unlinked`) is the pointwise comparison of indices `l..r`.
- Rust slicing panics when a range starts past the end of the code. The model clamps instead;
  `normalize_starts` shows normalization never introduces a new start, so the loop is panic free
  whenever every input range starts within the code (true for compiler-produced references).
-/

namespace Deploy.Offsets

/-- A byte range `[start, start + length)`; models `foundry_compilers::artifacts::Offsets`. -/
structure Range where
  start : Nat
  length : Nat
  deriving DecidableEq, Repr

/-- The exclusive end of a range. -/
def Range.stop (r : Range) : Nat := r.start + r.length

/-- Byte `x` lies in some range of `rs`. -/
def Covers (rs : List Range) (x : Nat) : Prop := ∃ r ∈ rs, r.start ≤ x ∧ x < r.stop

/-- One step of the merge loop of `normalize_offsets`. The accumulator is reversed: its head is
`merged.last_mut()`. -/
def push : List Range → Range → List Range
  | [], o => [o]
  | last :: rest, o =>
    if o.start ≤ last.stop then
      if o.stop > last.stop then ⟨last.start, o.stop - last.start⟩ :: rest else last :: rest
    else o :: last :: rest

/-- Model of `normalize_offsets` (`crates/common/src/contracts.rs`). -/
def normalize (os : List Range) : List Range :=
  ((os.mergeSort (fun a b => decide (a.start ≤ b.start))).foldl push []).reverse

/-- The shape `normalize` produces: sorted, pairwise disjoint and non-adjacent. -/
def Separated (rs : List Range) : Prop := rs.Pairwise (fun a b => a.stop < b.start)

/-! ## Normalization -/

section normalize

/-- Invariant of the merge loop over the remaining sorted input `l`. -/
structure Inv (acc l : List Range) : Prop where
  sep : acc.Pairwise (fun a b => b.stop < a.start)
  le : ∀ a ∈ acc, ∀ o ∈ l, a.start ≤ o.start

theorem covers_push (acc : List Range) (o : Range) (h : ∀ a ∈ acc.head?, a.start ≤ o.start)
    (x : Nat) : Covers (push acc o) x ↔ Covers acc x ∨ (o.start ≤ x ∧ x < o.stop) := by
  cases acc with
  | nil => simp [push, Covers]
  | cons last rest =>
    have hl : last.start ≤ o.start := h last (by simp)
    simp only [push]
    split
    · split
      · simp only [Covers, List.mem_cons, Range.stop] at *
        constructor
        · rintro ⟨r, hr | hr, h1, h2⟩
          · subst hr; simp at h1 h2
            by_cases hx : x < last.start + last.length
            · exact Or.inl ⟨last, Or.inl rfl, h1, hx⟩
            · exact Or.inr ⟨by omega, by omega⟩
          · exact Or.inl ⟨r, Or.inr hr, h1, h2⟩
        · rintro (⟨r, hr | hr, h1, h2⟩ | ⟨h1, h2⟩)
          · subst hr; exact ⟨_, Or.inl rfl, h1, by simp; omega⟩
          · exact ⟨r, Or.inr hr, h1, h2⟩
          · exact ⟨_, Or.inl rfl, by simp; omega, by simp; omega⟩
      · simp only [Covers, List.mem_cons, Range.stop] at *
        constructor
        · intro h; exact Or.inl h
        · rintro (h | ⟨h1, h2⟩)
          · exact h
          · exact ⟨last, Or.inl rfl, by omega, by omega⟩
    · simp only [Covers, List.mem_cons]
      constructor
      · rintro ⟨r, hr | hr | hr, h1, h2⟩
        · subst hr; exact Or.inr ⟨h1, h2⟩
        · exact Or.inl ⟨r, Or.inl hr, h1, h2⟩
        · exact Or.inl ⟨r, Or.inr hr, h1, h2⟩
      · rintro (⟨r, hr | hr, h1, h2⟩ | ⟨h1, h2⟩)
        · exact ⟨r, Or.inr (Or.inl hr), h1, h2⟩
        · exact ⟨r, Or.inr (Or.inr hr), h1, h2⟩
        · exact ⟨o, Or.inl rfl, h1, h2⟩

theorem inv_push {acc : List Range} {o : Range} {l : List Range}
    (hinv : Inv acc (o :: l)) (hsorted : ∀ b ∈ l, o.start ≤ b.start) : Inv (push acc o) l := by
  obtain ⟨sep, le⟩ := hinv
  cases acc with
  | nil =>
    exact ⟨by simp [push], by simp [push]; exact hsorted⟩
  | cons last rest =>
    have hl : last.start ≤ o.start := le last (by simp) o (by simp)
    simp only [List.pairwise_cons] at sep
    simp only [push]
    split
    · split
      · refine ⟨?_, ?_⟩
        · simp only [List.pairwise_cons]
          exact ⟨fun b hb => sep.1 b hb, sep.2⟩
        · intro a ha b hb
          simp only [List.mem_cons] at ha
          rcases ha with rfl | ha
          · exact Nat.le_trans hl (hsorted b hb)
          · exact le a (by simp [ha]) b (by simp [hb])
      · exact ⟨by simp only [List.pairwise_cons]; exact sep,
          fun a ha b hb => le a ha b (by simp [hb])⟩
    · rename_i hgt
      refine ⟨?_, ?_⟩
      · simp only [List.pairwise_cons, List.mem_cons]
        refine ⟨?_, sep.1, sep.2⟩
        rintro b (rfl | hb)
        · omega
        · have := sep.1 b hb; simp [Range.stop] at *; omega
      · intro a ha b hb
        simp only [List.mem_cons] at ha
        rcases ha with rfl | rfl | ha
        · exact hsorted b hb
        · exact le _ (by simp) b (by simp [hb])
        · exact le a (by simp [ha]) b (by simp [hb])

theorem foldl_push (l acc : List Range) (hl : l.Pairwise (fun a b => a.start ≤ b.start))
    (hinv : Inv acc l) :
    Inv (l.foldl push acc) [] ∧ ∀ x, Covers (l.foldl push acc) x ↔ Covers acc x ∨ Covers l x := by
  induction l generalizing acc with
  | nil => exact ⟨hinv, fun x => by simp [Covers]⟩
  | cons o l ih =>
    simp only [List.pairwise_cons] at hl
    have hhead : ∀ a ∈ acc.head?, a.start ≤ o.start := by
      intro a ha; exact hinv.le a (List.mem_of_mem_head? ha) o (by simp)
    obtain ⟨hinv', hcov⟩ := ih (push acc o) hl.2 (inv_push hinv hl.1)
    refine ⟨hinv', fun x => ?_⟩
    rw [List.foldl_cons, hcov, covers_push acc o hhead]
    simp only [Covers, List.mem_cons]
    constructor
    · rintro ((h | ⟨h1, h2⟩) | ⟨r, hr, h1, h2⟩)
      · exact Or.inl h
      · exact Or.inr ⟨o, Or.inl rfl, h1, h2⟩
      · exact Or.inr ⟨r, Or.inr hr, h1, h2⟩
    · rintro (h | ⟨r, rfl | hr, h1, h2⟩)
      · exact Or.inl (Or.inl h)
      · exact Or.inl (Or.inr ⟨h1, h2⟩)
      · exact Or.inr ⟨r, hr, h1, h2⟩

theorem sorted_input (os : List Range) :
    (os.mergeSort (fun a b => decide (a.start ≤ b.start))).Pairwise
      (fun a b => a.start ≤ b.start) := by
  have := List.pairwise_mergeSort (le := fun a b : Range => decide (a.start ≤ b.start))
    (by intro a b c h1 h2; simp at *; omega) (by intro a b; simp; omega) os
  simpa using this

/-- **Headline (normalization, shape).** The normalized ranges are sorted, pairwise disjoint and
non-adjacent. -/
theorem normalize_separated (os : List Range) : Separated (normalize os) := by
  have h := (foldl_push _ [] (sorted_input os) ⟨by simp, by simp⟩).1.sep
  unfold Separated normalize
  rw [List.pairwise_reverse]
  exact h

/-- **Headline (normalization, coverage).** Normalization ignores exactly the bytes the input
ranges ignore. -/
theorem covers_normalize (os : List Range) (x : Nat) : Covers (normalize os) x ↔ Covers os x := by
  have h := (foldl_push _ [] (sorted_input os) ⟨by simp, by simp⟩).2 x
  have hp := List.mergeSort_perm os (fun a b => decide (a.start ≤ b.start))
  unfold normalize
  simp only [Covers, List.mem_reverse] at h ⊢
  rw [h]
  simp only [List.not_mem_nil, false_and, exists_false, false_or]
  constructor
  · rintro ⟨r, hr, h1⟩; exact ⟨r, hp.mem_iff.mp hr, h1⟩
  · rintro ⟨r, hr, h1⟩; exact ⟨r, hp.mem_iff.mpr hr, h1⟩

theorem push_bounds (S : List Nat) (acc : List Range) (o : Range) (B : Nat)
    (hacc : ∀ r ∈ acc, r.stop ≤ B ∧ ∃ s ∈ S, r.start = s) (ho : o.stop ≤ B) (hos : o.start ∈ S)
    (hl : ∀ a ∈ acc.head?, a.start ≤ o.start) :
    ∀ r ∈ push acc o, r.stop ≤ B ∧ ∃ s ∈ S, r.start = s := by
  cases acc with
  | nil => simp [push]; exact ⟨ho, hos⟩
  | cons last rest =>
    have hlo : last.start ≤ o.start := hl last (by simp)
    simp only [push]
    split
    · split
      · intro r hr
        simp only [List.mem_cons] at hr
        rcases hr with rfl | hr
        · refine ⟨by simp [Range.stop] at *; omega, (hacc last (by simp)).2⟩
        · exact hacc r (by simp [hr])
      · exact hacc
    · intro r hr
      simp only [List.mem_cons] at hr
      rcases hr with rfl | rfl | hr
      · exact ⟨ho, _, hos, rfl⟩
      · exact hacc _ (by simp)
      · exact hacc r (by simp [hr])

/-- Every normalized range ends no later than the largest input end, and starts where some input
range starts. The first fact makes the `u32` narrowing in Rust exact; the second keeps the
segment loop panic free. -/
theorem normalize_bounds (os : List Range) (B : Nat) (h : ∀ o ∈ os, o.stop ≤ B) :
    ∀ r ∈ normalize os, r.stop ≤ B ∧ ∃ o ∈ os, r.start = o.start := by
  let S := os.map (·.start)
  have hp := List.mergeSort_perm os (fun a b => decide (a.start ≤ b.start))
  suffices ∀ l acc, l.Pairwise (fun a b => a.start ≤ b.start) → (∀ o ∈ l, o ∈ os) →
      Inv acc l → (∀ r ∈ acc, r.stop ≤ B ∧ ∃ s ∈ S, r.start = s) →
      ∀ r ∈ l.foldl push acc, r.stop ≤ B ∧ ∃ s ∈ S, r.start = s by
    intro r hr
    unfold normalize at hr
    rw [List.mem_reverse] at hr
    obtain ⟨hb, s, hs, he⟩ := this _ [] (sorted_input os) (fun o ho => hp.mem_iff.mp ho)
      ⟨by simp, by simp⟩ (by simp) r hr
    simp only [S, List.mem_map] at hs
    obtain ⟨o, ho, rfl⟩ := hs
    exact ⟨hb, o, ho, he⟩
  intro l
  induction l with
  | nil => intro acc _ _ _ h; exact h
  | cons o l ih =>
    intro acc hl hmem hinv hacc
    simp only [List.pairwise_cons] at hl
    have hhead : ∀ a ∈ acc.head?, a.start ≤ o.start := by
      intro a ha; exact hinv.le a (List.mem_of_mem_head? ha) o (by simp)
    exact ih _ hl.2 (fun o' h' => hmem o' (by simp [h'])) (inv_push hinv hl.1)
      (push_bounds S acc o B hacc (h o (hmem o (by simp))) (List.mem_map_of_mem (hmem o (by simp)))
        hhead)

end normalize

/-! ## The segment loop -/

section segments

variable {α : Type} [DecidableEq α]

/-- Local byte `i` equals onchain byte `i`. Out of range positions compare equal (both `none`),
matching the clamped slices of the model. -/
def byteEq (loc : List (Option α)) (code : List α) (i : Nat) : Bool :=
  loc[i]? == code[i]?.map some

/-- Model of the segment loop of `find_by_deployed_code_exact_inner`, starting at `left`: compare
`[left, start)` before each range, continue at its end, and compare the suffix after the last. -/
def segments (loc : List (Option α)) (code : List α) : Nat → List Range → Bool
  | left, [] => (List.range' left (code.length - left)).all (byteEq loc code)
  | left, o :: os =>
    (List.range' left (o.start - left)).all (byteEq loc code) && segments loc code o.stop os

theorem byteEq_of_ge {loc : List (Option α)} {code : List α} (hlen : loc.length = code.length)
    {i : Nat} (hi : code.length ≤ i) : byteEq loc code i = true := by
  simp [byteEq, List.getElem?_eq_none (show loc.length ≤ i by omega),
    List.getElem?_eq_none hi]

theorem segments_iff {loc : List (Option α)} {code : List α} (hlen : loc.length = code.length)
    (rs : List Range) (hsep : Separated rs) (left : Nat) (hleft : ∀ r ∈ rs, left ≤ r.start) :
    segments loc code left rs = true ↔
      ∀ i, left ≤ i → i < code.length → ¬ Covers rs i → byteEq loc code i = true := by
  induction rs generalizing left with
  | nil =>
    simp only [segments, List.all_eq_true, List.mem_range'_1, Covers, List.not_mem_nil,
      false_and, exists_false, not_false_eq_true, forall_const]
    constructor
    · intro h i h1 h2; exact h i ⟨h1, by omega⟩
    · intro h i ⟨h1, h2⟩; exact h i h1 (by omega)
  | cons o os ih =>
    simp only [Separated, List.pairwise_cons] at hsep
    have hos : ∀ r ∈ os, o.stop ≤ r.start := fun r hr => Nat.le_of_lt (hsep.1 r hr)
    have hol : left ≤ o.start := hleft o (by simp)
    simp only [segments, Bool.and_eq_true, List.all_eq_true, List.mem_range'_1, ih hsep.2 o.stop hos]
    constructor
    · rintro ⟨h1, h2⟩ i hi hn hc
      by_cases hlo : i < o.start
      · exact h1 i ⟨hi, by omega⟩
      · by_cases hhi : i < o.stop
        · exact absurd ⟨o, by simp, by omega, hhi⟩ hc
        · exact h2 i (by omega) hn (fun ⟨r, hr, h3⟩ => hc ⟨r, by simp [hr], h3⟩)
    · intro h
      refine ⟨fun i ⟨h1, h2⟩ => ?_, fun i h1 h2 h3 => h i (by simp [Range.stop] at h1; omega) h2 ?_⟩
      · by_cases hn : i < code.length
        · refine h i h1 hn ?_
          rintro ⟨r, hr, h3, h4⟩
          simp only [List.mem_cons] at hr
          rcases hr with rfl | hr
          · omega
          · have := hos r hr; simp [Range.stop] at *; omega
        · exact byteEq_of_ge hlen (by omega)
      · rintro ⟨r, hr, h4, h5⟩
        simp only [List.mem_cons] at hr
        rcases hr with rfl | hr
        · omega
        · exact h3 ⟨r, hr, h4, h5⟩

/-- Model of the partial-match decision of `find_by_deployed_code_exact_inner`: equal lengths,
then the segment loop over the normalized ignored ranges. -/
def partialMatch (loc : List (Option α)) (code : List α) (ignored : List Range) : Bool :=
  loc.length == code.length && segments loc code 0 (normalize ignored)

/-- **Headline (segment loop).** A partial match holds iff the lengths agree and every byte
outside the ignored ranges matches. Ranges may overlap, touch, or come in any order. -/
theorem partialMatch_iff (loc : List (Option α)) (code : List α) (ignored : List Range) :
    partialMatch loc code ignored = true ↔
      loc.length = code.length ∧
        ∀ i < code.length, ¬ Covers ignored i → loc[i]? = code[i]?.map some := by
  unfold partialMatch
  simp only [Bool.and_eq_true, beq_iff_eq]
  constructor
  · rintro ⟨hlen, h⟩
    refine ⟨hlen, fun i hi hc => ?_⟩
    have := (segments_iff hlen _ (normalize_separated ignored) 0 (by simp)).mp h i (by omega) hi
      (fun h' => hc ((covers_normalize ignored i).mp h'))
    simpa [byteEq] using this
  · rintro ⟨hlen, h⟩
    refine ⟨hlen, (segments_iff hlen _ (normalize_separated ignored) 0 (by simp)).mpr ?_⟩
    intro i _ hi hc
    simpa [byteEq] using h i hi (fun h' => hc ((covers_normalize ignored i).mpr h'))

/-- Model of the exact-match decision: a partial match whose bytes from the metadata start `m`
onwards are all equal (`matches_metadata`). -/
def exactMatch (loc : List (Option α)) (code : List α) (ignored : List Range) (m : Nat) : Bool :=
  partialMatch loc code (⟨m, code.length - 2 - m⟩ :: ignored) &&
    (List.range' m (code.length - m)).all (byteEq loc code)

/-- **Headline (exact match).** With the metadata range `[m, len - 2)` added to the ignored
ranges, an exact match holds iff every byte outside the *other* ignored ranges matches, and so
does every byte from `m` on: the metadata is compared in full. -/
theorem exactMatch_iff (loc : List (Option α)) (code : List α) (ignored : List Range) (m : Nat) :
    exactMatch loc code ignored m = true ↔
      loc.length = code.length ∧
        ∀ i < code.length, (m ≤ i ∨ ¬ Covers ignored i) → loc[i]? = code[i]?.map some := by
  unfold exactMatch
  rw [Bool.and_eq_true, partialMatch_iff]
  simp only [List.all_eq_true, List.mem_range'_1]
  constructor
  · rintro ⟨⟨hlen, h⟩, hm⟩
    refine ⟨hlen, fun i hi hc => ?_⟩
    by_cases him : m ≤ i
    · simpa [byteEq] using hm i ⟨him, by omega⟩
    · refine h i hi ?_
      rintro ⟨r, hr, h1, h2⟩
      simp only [List.mem_cons] at hr
      rcases hr with rfl | hr
      · exact him h1
      · exact (hc.resolve_left him) ⟨r, hr, h1, h2⟩
  · rintro ⟨hlen, h⟩
    refine ⟨⟨hlen, fun i hi hc => h i hi (Or.inr fun ⟨r, hr, h'⟩ => hc ⟨r, by simp [hr], h'⟩)⟩,
      ?_⟩
    intro i ⟨h1, h2⟩
    simpa [byteEq] using h i (by omega) (Or.inl h1)

end segments

end Deploy.Offsets
