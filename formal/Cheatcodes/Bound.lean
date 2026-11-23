/-!
# `vm.bound`

Models of `BoundMutator::bound` for `U256` and `I256` in
`crates/evm/fuzz/src/strategies/mutators.rs`, which implement the `vm.bound(current, min, max)`
cheatcode in `crates/cheatcodes/src/utils.rs`. The cheatcode returns a random value in
`[min, max]` different from `current`, and fails when `current` is outside the range or the range
has fewer than two values.

Fidelity:
- `U256` is a `Nat` below `2^256`; `I256` is an `Int` in `[-2^255, 2^255)`, stored by Rust as its
  two's complement `raw` word (`toRaw`, `fromRaw`).
- The RNG is an oracle: the Rust code draws `bits ∈ [8, 256]`, a uniform 256-bit `word` and, for
  `I256`, a `flip` bit. Every value in the oracle's range is drawn with nonzero probability.
- `word >> (256 - bits)` is `word / 2^(256 - bits)`.

The fixed implementation draws once and maps the draw onto `[min, max] \ {current}` without
rejection sampling, so it always terminates. The headline theorems are `uBound_some`,
`uBound_surjective`, `iBound_some` and `iBound_surjective`. The pre-fix sampler is modelled as
`oldUBound`, and `oldUBound_unreachable` records the two reachability bugs it had.
-/

namespace Cheatcodes.Bound

/-- `2^256`. -/
abbrev W : Nat := 2 ^ 256

/-- The bit-width biased draw: the top `bits` bits of `word`. -/
def draw (bits word : Nat) : Nat := word / 2 ^ (256 - bits)

/-- Models `bound_offset`: maps a draw onto an offset in `[0, span] \ {skip}`. -/
def boundOffset (d span skip : Nat) : Nat :=
  let r := d % span
  if skip ≤ r then r + 1 else r

theorem boundOffset_le (d span skip : Nat) (h : 0 < span) : boundOffset d span skip ≤ span := by
  have := Nat.mod_lt d h
  unfold boundOffset; simp only; split <;> omega

theorem boundOffset_ne (d span skip : Nat) : boundOffset d span skip ≠ skip := by
  unfold boundOffset; simp only; split <;> omega

/-- Every offset in `[0, span] \ {skip}` is the image of a draw below `span`. -/
theorem boundOffset_surjective (span skip t : Nat) (hs : skip ≤ span) (ht : t ≤ span)
    (hne : t ≠ skip) :
    ∃ d, d < span ∧ boundOffset d span skip = t := by
  by_cases h : t < skip
  · refine ⟨t, by omega, ?_⟩
    unfold boundOffset; simp only [Nat.mod_eq_of_lt (show t < span by omega)]; split <;> omega
  · refine ⟨t - 1, by omega, ?_⟩
    unfold boundOffset; simp only [Nat.mod_eq_of_lt (show t - 1 < span by omega)]; split <;> omega

/-- Models `<U256 as BoundMutator>::bound`. -/
def uBound (self min max bits word : Nat) : Option Nat :=
  if min < max ∧ min ≤ self ∧ self ≤ max then
    some (min + boundOffset (draw bits word) (max - min) (self - min))
  else none

/-- `vm.bound` fails exactly when `current` is outside `[min, max]` or `min ≥ max`. -/
theorem uBound_none_iff (self min max bits word : Nat) :
    uBound self min max bits word = none ↔ ¬(min < max ∧ min ≤ self ∧ self ≤ max) := by
  unfold uBound; split <;> simp_all

/-- **Headline.** Every result is in `[min, max]` and differs from `current`. The `U256`
additions and subtractions in the Rust code do not wrap because `max < 2^256`. -/
theorem uBound_some (self min max bits word x : Nat)
    (h : uBound self min max bits word = some x) : min ≤ x ∧ x ≤ max ∧ x ≠ self := by
  unfold uBound at h
  split at h
  · rename_i hr
    cases h
    have h1 := boundOffset_le (draw bits word) (max - min) (self - min) (by omega)
    have h2 := boundOffset_ne (draw bits word) (max - min) (self - min)
    omega
  · cases h

/-- **Headline.** Every value of `[min, max]` other than `current` is the result of some RNG
draw, here one with `bits = 256`. -/
theorem uBound_surjective (self min max t : Nat) (hmax : max < W) (hr : min < max)
    (hs : min ≤ self ∧ self ≤ max) (ht : min ≤ t ∧ t ≤ max) (hne : t ≠ self) :
    ∃ word, word < W ∧ uBound self min max 256 word = some t := by
  obtain ⟨d, hd, he⟩ := boundOffset_surjective (max - min) (self - min) (t - min) (by omega)
    (by omega) (by omega)
  refine ⟨d, by omega, ?_⟩
  unfold uBound draw
  rw [if_pos ⟨hr, hs⟩]
  simp only [Nat.sub_self, Nat.pow_zero, Nat.div_one, he]
  congr 1; omega

/-! ## Signed values -/

/-- An `I256` value as a mathematical integer. -/
def IsI256 (x : Int) : Prop := -(2 ^ 255 : Int) ≤ x ∧ x < 2 ^ 255

/-- `I256::into_raw`: the two's complement word. -/
def toRaw (x : Int) : Nat := (x % (W : Int)).toNat

/-- `I256::from_raw`. -/
def fromRaw (u : Nat) : Int := if u < 2 ^ 255 then u else (u : Int) - W

/-- Models `<I256 as BoundMutator>::bound`: the span and the skipped offset are computed on raw
words with wrapping subtraction, and the result with wrapping addition. -/
def iBound (self min max : Int) (bits word : Nat) (flip : Bool) : Option Int :=
  if min < max ∧ min ≤ self ∧ self ≤ max then
    let span := (toRaw max + W - toRaw min) % W
    let skip := (toRaw self + W - toRaw min) % W
    let r := draw bits word % span
    let r := if flip then span - 1 - r else r
    some (fromRaw ((toRaw min + boundOffset r span skip) % W))
  else none

/-- Wrapping subtraction of raw words is exact subtraction for ordered `I256` values. -/
theorem raw_sub (a b : Int) (ha : IsI256 a) (hb : IsI256 b) (h : a ≤ b) :
    (((toRaw b + W - toRaw a) % W : Nat) : Int) = b - a := by
  unfold toRaw; unfold IsI256 at ha hb; simp only [W] at *; omega

/-- Wrapping addition of a raw word and an in-range offset is exact addition. -/
theorem raw_add (a : Int) (o : Nat) (ha : IsI256 a) (h : IsI256 (a + o)) :
    fromRaw ((toRaw a + o) % W) = a + o := by
  unfold fromRaw toRaw; unfold IsI256 at ha h; simp only [W] at *; split <;> omega

/-- **Headline.** Every signed result is in `[min, max]` and differs from `current`. -/
theorem iBound_some (self min max : Int) (bits word : Nat) (flip : Bool) (x : Int)
    (hmin : IsI256 min) (hmax : IsI256 max) (hself : IsI256 self)
    (h : iBound self min max bits word flip = some x) : min ≤ x ∧ x ≤ max ∧ x ≠ self := by
  unfold iBound at h
  split at h
  · rename_i hr
    simp only [Option.some.injEq] at h
    have hspan := raw_sub min max hmin hmax (by omega)
    have hskip := raw_sub min self hmin hself (by omega)
    generalize hS : (toRaw max + W - toRaw min) % W = span at h hspan
    generalize hK : (toRaw self + W - toRaw min) % W = skip at h hskip
    generalize hR : (if flip then span - 1 - draw bits word % span else draw bits word % span)
      = r at h
    have h1 := boundOffset_le r span skip (by omega)
    have h2 := boundOffset_ne r span skip
    rw [raw_add min _ hmin (by unfold IsI256 at *; omega)] at h
    omega
  · cases h

/-- **Headline.** Every value of `[min, max]` other than `current` is the result of some RNG
draw, here one with `bits = 256` and no flip. -/
theorem iBound_surjective (self min max t : Int) (hmin : IsI256 min) (hmax : IsI256 max)
    (hr : min < max) (hs : min ≤ self ∧ self ≤ max) (ht : min ≤ t ∧ t ≤ max) (hne : t ≠ self) :
    ∃ word, word < W ∧ iBound self min max 256 word false = some t := by
  have hself : IsI256 self := by unfold IsI256 at *; omega
  have hspan := raw_sub min max hmin hmax (by omega)
  have hskip := raw_sub min self hmin hself (by omega)
  obtain ⟨d, hd, he⟩ := boundOffset_surjective ((toRaw max + W - toRaw min) % W)
    ((toRaw self + W - toRaw min) % W) (t - min).toNat (by omega) (by omega) (by omega)
  refine ⟨d, by have := Nat.mod_lt (toRaw max + W - toRaw min) (show 0 < W by decide); omega, ?_⟩
  unfold iBound draw
  rw [if_pos ⟨hr, hs⟩]
  simp only [Nat.sub_self, Nat.pow_zero, Nat.div_one, Nat.mod_eq_of_lt hd, Bool.false_eq_true,
    if_false, he, Option.some.injEq]
  rw [raw_add min _ hmin (by unfold IsI256 at *; omega)]
  omega

/-! ## The pre-fix sampler -/

/-- Models the pre-fix `<U256 as BoundMutator>::bound` loop body: the candidate is a `u128` draw
`c` masked to `bits` bits, mapped with `min + c % (max - min).saturating_add(1)`. -/
def oldCandidate (min max c bits : Nat) : Nat :=
  let span := Nat.min (max - min + 1) (W - 1)
  min + (c % 2 ^ bits) % span

/-- The pre-fix sampler could not produce `max` on the full range, and could not produce any
value at or above `min + 2^128` on any range. -/
theorem oldUBound_unreachable (c bits : Nat) (hc : c < 2 ^ 128) :
    oldCandidate 0 (W - 1) c bits ≠ W - 1 ∧
    ∀ min max, oldCandidate min max c bits < min + 2 ^ 128 := by
  constructor
  · unfold oldCandidate
    have : Nat.min (W - 1 - 0 + 1) (W - 1) = W - 1 := by decide
    simp only [this]
    have := Nat.mod_lt (c % 2 ^ bits) (show 0 < W - 1 by decide)
    omega
  · intro min max
    unfold oldCandidate
    dsimp only
    have h1 := Nat.mod_le (c % 2 ^ bits) (Nat.min (max - min + 1) (W - 1))
    have h2 := Nat.mod_le c (2 ^ bits)
    omega

end Cheatcodes.Bound
