/-!
# Approximate-equality assertions

Models of the delta computations behind `assertApproxEqAbs` and `assertApproxEqRel` in
`crates/cheatcodes/src/test/assert.rs`.

Fidelity:
- A `U256` is a `Nat` below `2^256` and an `I256` is an `Int` in `[-2^255, 2^255)`.
- `I256::into_sign_and_abs` is `(x < 0, x.natAbs)`; zero has the positive sign, as in `alloy`.
- `wrapping_add` on `U256` is addition modulo `2^256`; the `U512` product is modelled modulo
  `2^512`, so the theorems show that the Rust code never wraps.
- `U256::checked_from_limbs_slice` on the `U512` quotient succeeds iff the quotient is below
  `2^256`.
-/

namespace Cheatcodes.Approx

/-- `2^256`, the `U256` modulus. -/
abbrev W : Nat := 2 ^ 256

/-- An `I256` value as a mathematical integer. -/
def IsI256 (x : Int) : Prop := -(2 ^ 255 : Int) ≤ x ∧ x < 2 ^ 255

/-- Models `get_delta_int` in `crates/cheatcodes/src/test/assert.rs`. -/
def getDeltaInt (l r : Int) : Nat :=
  let la := l.natAbs
  let ra := r.natAbs
  if (l < 0 ∧ r < 0) ∨ (0 ≤ l ∧ 0 ≤ r) then
    if la > ra then la - ra else ra - la
  else
    (la + ra) % W

/-- The opposite-sign branch of `get_delta_int` never wraps: the sum is below `2^256`. -/
theorem getDeltaInt_no_wrap (l r : Int) (hl : IsI256 l) (hr : IsI256 r)
    (hs : ¬((l < 0 ∧ r < 0) ∨ (0 ≤ l ∧ 0 ≤ r))) : l.natAbs + r.natAbs < W := by
  unfold IsI256 at hl hr
  simp only [W]
  omega

/-- `get_delta_int` is the exact absolute difference `|l - r|` over the integers. -/
theorem getDeltaInt_eq (l r : Int) (hl : IsI256 l) (hr : IsI256 r) :
    getDeltaInt l r = (l - r).natAbs := by
  unfold getDeltaInt
  simp only
  split
  · split <;> omega
  · rename_i hs
    rw [Nat.mod_eq_of_lt (getDeltaInt_no_wrap l r hl hr hs)]
    omega

/-- `U256::abs_diff`. -/
def absDiff (l r : Nat) : Nat := if l > r then l - r else r - l

theorem absDiff_eq (l r : Nat) : (absDiff l r : Int) = ((l : Int) - r).natAbs := by
  unfold absDiff; split <;> omega

/-- `10^EQ_REL_DELTA_RESOLUTION`, with `EQ_REL_DELTA_RESOLUTION = 18`. -/
abbrev scale : Nat := 10 ^ 18

/-- Models `calc_delta_full`: `none` is `EqRelAssertionError::Overflow`. -/
def calcDeltaFull (absDiff right : Nat) : Option Nat :=
  let q := (absDiff * scale % 2 ^ 512) / right
  if q < W then some q else none

set_option exponentiation.threshold 512 in
/-- The `U512` product in `calc_delta_full` never wraps. -/
theorem calcDeltaFull_no_wrap (d : Nat) (hd : d < W) : d * scale < 2 ^ 512 := by
  have h : scale < W := by decide
  have : d * scale < W * W := Nat.mul_lt_mul'' hd h
  rw [show (2 : Nat) ^ 512 = 2 ^ 256 * 2 ^ 256 from (Nat.pow_add 2 256 256)]
  exact this

/-- `calc_delta_full` is the exact floor of `d * 10^18 / right`, failing iff it reaches `2^256`. -/
theorem calcDeltaFull_eq (d right : Nat) (hd : d < W) :
    calcDeltaFull d right = if d * scale / right < W then some (d * scale / right) else none := by
  unfold calcDeltaFull
  rw [Nat.mod_eq_of_lt (calcDeltaFull_no_wrap d hd)]

/-- Outcome of a relative assertion: `ok`, a failure with a defined delta, a failure with an
undefined delta (`right = 0`), or an overflow. -/
inductive RelResult where
  | ok
  | fail (delta : Nat)
  | undefined
  | overflow
  deriving DecidableEq, Repr

/-- Models `uint_assert_approx_eq_rel` in `crates/cheatcodes/src/test/assert.rs`. -/
def uintRel (l r maxDelta : Nat) : RelResult :=
  if r = 0 then (if l = 0 then .ok else .undefined)
  else match calcDeltaFull (absDiff l r) r with
    | none => .overflow
    | some d => if d ≤ maxDelta then .ok else .fail d

/-- Models `int_assert_approx_eq_rel` in `crates/cheatcodes/src/test/assert.rs`. -/
def intRel (l r : Int) (maxDelta : Nat) : RelResult :=
  if r = 0 then (if l = 0 then .ok else .undefined)
  else match calcDeltaFull (getDeltaInt l r) r.natAbs with
    | none => .overflow
    | some d => if d ≤ maxDelta then .ok else .fail d

/-- The mathematical relative delta `⌊|l - r| * 10^18 / |r|⌋`. -/
def relDelta (l r : Int) : Nat := (l - r).natAbs * scale / r.natAbs

/-- Contract of the relative assertions, shared by the `uint` and `int` variants. -/
def relSpec (l r : Int) (maxDelta : Nat) : RelResult :=
  if r = 0 then (if l = 0 then .ok else .undefined)
  else if relDelta l r < W then
    (if relDelta l r ≤ maxDelta then .ok else .fail (relDelta l r))
  else .overflow

/-- `assertApproxEqRel` on signed values computes exactly `⌊|l - r| * 10^18 / |r|⌋`, reports an
overflow iff that is at least `2^256`, and with `r = 0` passes iff `l = 0`. -/
theorem intRel_eq_spec (l r : Int) (maxDelta : Nat) (hl : IsI256 l) (hr : IsI256 r) :
    intRel l r maxDelta = relSpec l r maxDelta := by
  unfold intRel relSpec
  split
  · rfl
  · have hd : getDeltaInt l r < W := by
      rw [getDeltaInt_eq l r hl hr]; unfold IsI256 at hl hr; simp only [W]; omega
    rw [calcDeltaFull_eq _ _ hd, getDeltaInt_eq l r hl hr]
    unfold relDelta
    by_cases hq : (l - r).natAbs * scale / r.natAbs < W <;> simp only [hq, if_true, if_false]

/-- The unsigned variant satisfies the same contract. -/
theorem uintRel_eq_spec (l r maxDelta : Nat) (hl : l < W) (hr : r < W) :
    uintRel l r maxDelta = relSpec l r maxDelta := by
  unfold uintRel relSpec
  by_cases h : r = 0
  · simp [h]
  · have hd : absDiff l r < W := by unfold absDiff; split <;> omega
    have hrz : ¬ ((r : Int) = 0) := by omega
    simp only [h, hrz, if_false]
    rw [calcDeltaFull_eq _ _ hd]
    have ha : absDiff l r = ((l : Int) - r).natAbs := by have := absDiff_eq l r; omega
    unfold relDelta
    rw [ha, Int.natAbs_natCast]
    by_cases hq : ((l : Int) - r).natAbs * scale / r < W <;> simp only [hq, if_true, if_false]

/-- Models `int_assert_approx_eq_abs`: passes iff the computed delta is at most `maxDelta`. -/
def intAbs (l r : Int) (maxDelta : Nat) : Bool := decide (getDeltaInt l r ≤ maxDelta)

/-- `assertApproxEqAbs` on signed values passes iff `|l - r| ≤ maxDelta` over the integers. -/
theorem intAbs_iff (l r : Int) (maxDelta : Nat) (hl : IsI256 l) (hr : IsI256 r) :
    intAbs l r maxDelta = true ↔ (l - r).natAbs ≤ maxDelta := by
  unfold intAbs; rw [getDeltaInt_eq l r hl hr]; simp

/-- Adversarial vectors for the Rust conformance test `approx_eq_matches_lean_vectors` in
`crates/cheatcodes/src/test/assert.rs`, which hard-codes the `#eval` output below. -/
def vectors : List (Int × Int) :=
  [ (-(2 ^ 255), 2 ^ 255 - 1), (2 ^ 255 - 1, -(2 ^ 255)), (-(2 ^ 255), -(2 ^ 255)),
    (-5, 3), (3, -5), (-3, -5), (0, 0), (7, 0), (2 ^ 255 - 1, 1), (-(2 ^ 255), -1), (1, 3),
    (-1, 2 ^ 255 - 1) ]

#eval vectors.map fun (l, r) => (getDeltaInt l r, intRel l r 0)

end Cheatcodes.Approx
