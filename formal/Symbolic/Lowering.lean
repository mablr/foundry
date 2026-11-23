import Symbolic.Rewrites

/-!
# Opcode lowering and ABI input constraints

Models `signextend_word`, `signextend_word_dynamic`, `byte_word_dynamic`, `clz_word` and
`exp_expr_for_concrete_exponent` (`crates/evm/symbolic/src/runtime/evm.rs`), the symbolic `EXP`
chain of `SymbolicState::exp_word` (`state.rs`), and `constrain_uint` / `constrain_int`
(`crates/evm/symbolic/src/abi.rs`).

Each lowering is stated as an `Expr` builder that mirrors the Rust construction with raw
constructors. The Rust code builds the same shape through the smart constructors, which only
refine it (`Symbolic.Rewrites`, `Symbolic.WordRules`), so the value theorems carry over.

The EVM specifications are written independently of the lowering:

* `SIGNEXTEND(k, x)` keeps the low `8 (k + 1)` bits of `x` and copies bit `8 k + 7` above them,
  for `k < 31`, and is `x` otherwise: `(x.setWidth (8 k + 8)).signExtend 256`.
* `BYTE(i, x)` is byte `i` of the big-endian word for `i < 32`, otherwise zero.
* `CLZ(x)` is the number of leading zero bits, 256 for zero, characterized by
  `x >>> (255 - clz) = 1`.
* `EXP(b, e)` is `b ^ e` modulo `2 ^ 256`.

An ABI constraint must hold exactly for the values of its type: a constraint that excluded a
valid value would hide a counterexample and turn it into a false PASS.
-/

namespace Symbolic

/-! ## `SIGNEXTEND` -/

/-- The EVM `SIGNEXTEND(k, x)` for a constant `k`. -/
def signextendSpec (k : Nat) (x : BitVec 256) : BitVec 256 :=
  if k < 31 then (x.setWidth (8 * k + 8)).signExtend 256 else x

/-- `signextend`, the concrete helper, as a word function. -/
def signextendWord (k : Nat) (x : BitVec 256) : BitVec 256 :=
  if 32 ≤ k then x else
    let signBit := BitVec.twoPow 256 (8 * k + 7)
    let mask := signBit - 1
    if x &&& signBit = 0 then x &&& mask else x ||| ~~~mask

/-- `signextend_word(k, v)` for a constant byte index. -/
def signextendExpr (k : Nat) (v : Expr) : Expr :=
  if 32 ≤ k then v else
    let signBit := BitVec.twoPow 256 (8 * k + 7)
    let mask := signBit - 1
    .ite (.cmp .eq (.bin .and v (.const signBit)) (.const 0)) (.bin .and v (.const mask))
      (.bin .or v (.const (~~~mask)))

theorem evalK_signextendExpr {env : Env} {v : Expr} {x : Word} (k : Nat)
    (hv : evalK env v = some x) : evalK env (signextendExpr k v) = some (signextendWord k x) := by
  unfold signextendExpr signextendWord
  split
  · exact hv
  · simp only [evalK_ite', evalKB_cmp', evalK_bin', evalK_const', hv, kbin_some_some,
      BinOp.isDiv, Bool.false_eq_true, false_and, if_false, kcmp_some_some, CmpOp.eval,
      BinOp.evm]
    by_cases h : x &&& BitVec.twoPow 256 (8 * k + 7) = 0#256 <;> simp [h]

theorem signextendWord_spec_0 (x : BitVec 256) :
    signextendWord 0 x = signextendSpec 0 x := by
  simp only [signextendWord, signextendSpec]
  bv_decide

theorem signextendWord_spec_1 (x : BitVec 256) :
    signextendWord 1 x = signextendSpec 1 x := by
  simp only [signextendWord, signextendSpec]
  bv_decide

theorem signextendWord_spec_2 (x : BitVec 256) :
    signextendWord 2 x = signextendSpec 2 x := by
  simp only [signextendWord, signextendSpec]
  bv_decide

theorem signextendWord_spec_3 (x : BitVec 256) :
    signextendWord 3 x = signextendSpec 3 x := by
  simp only [signextendWord, signextendSpec]
  bv_decide

theorem signextendWord_spec_4 (x : BitVec 256) :
    signextendWord 4 x = signextendSpec 4 x := by
  simp only [signextendWord, signextendSpec]
  bv_decide

theorem signextendWord_spec_5 (x : BitVec 256) :
    signextendWord 5 x = signextendSpec 5 x := by
  simp only [signextendWord, signextendSpec]
  bv_decide

theorem signextendWord_spec_6 (x : BitVec 256) :
    signextendWord 6 x = signextendSpec 6 x := by
  simp only [signextendWord, signextendSpec]
  bv_decide

theorem signextendWord_spec_7 (x : BitVec 256) :
    signextendWord 7 x = signextendSpec 7 x := by
  simp only [signextendWord, signextendSpec]
  bv_decide

theorem signextendWord_spec_8 (x : BitVec 256) :
    signextendWord 8 x = signextendSpec 8 x := by
  simp only [signextendWord, signextendSpec]
  bv_decide

theorem signextendWord_spec_9 (x : BitVec 256) :
    signextendWord 9 x = signextendSpec 9 x := by
  simp only [signextendWord, signextendSpec]
  bv_decide

theorem signextendWord_spec_10 (x : BitVec 256) :
    signextendWord 10 x = signextendSpec 10 x := by
  simp only [signextendWord, signextendSpec]
  bv_decide

theorem signextendWord_spec_11 (x : BitVec 256) :
    signextendWord 11 x = signextendSpec 11 x := by
  simp only [signextendWord, signextendSpec]
  bv_decide

theorem signextendWord_spec_12 (x : BitVec 256) :
    signextendWord 12 x = signextendSpec 12 x := by
  simp only [signextendWord, signextendSpec]
  bv_decide

theorem signextendWord_spec_13 (x : BitVec 256) :
    signextendWord 13 x = signextendSpec 13 x := by
  simp only [signextendWord, signextendSpec]
  bv_decide

theorem signextendWord_spec_14 (x : BitVec 256) :
    signextendWord 14 x = signextendSpec 14 x := by
  simp only [signextendWord, signextendSpec]
  bv_decide

theorem signextendWord_spec_15 (x : BitVec 256) :
    signextendWord 15 x = signextendSpec 15 x := by
  simp only [signextendWord, signextendSpec]
  bv_decide

theorem signextendWord_spec_16 (x : BitVec 256) :
    signextendWord 16 x = signextendSpec 16 x := by
  simp only [signextendWord, signextendSpec]
  bv_decide

theorem signextendWord_spec_17 (x : BitVec 256) :
    signextendWord 17 x = signextendSpec 17 x := by
  simp only [signextendWord, signextendSpec]
  bv_decide

theorem signextendWord_spec_18 (x : BitVec 256) :
    signextendWord 18 x = signextendSpec 18 x := by
  simp only [signextendWord, signextendSpec]
  bv_decide

theorem signextendWord_spec_19 (x : BitVec 256) :
    signextendWord 19 x = signextendSpec 19 x := by
  simp only [signextendWord, signextendSpec]
  bv_decide

theorem signextendWord_spec_20 (x : BitVec 256) :
    signextendWord 20 x = signextendSpec 20 x := by
  simp only [signextendWord, signextendSpec]
  bv_decide

theorem signextendWord_spec_21 (x : BitVec 256) :
    signextendWord 21 x = signextendSpec 21 x := by
  simp only [signextendWord, signextendSpec]
  bv_decide

theorem signextendWord_spec_22 (x : BitVec 256) :
    signextendWord 22 x = signextendSpec 22 x := by
  simp only [signextendWord, signextendSpec]
  bv_decide

theorem signextendWord_spec_23 (x : BitVec 256) :
    signextendWord 23 x = signextendSpec 23 x := by
  simp only [signextendWord, signextendSpec]
  bv_decide

theorem signextendWord_spec_24 (x : BitVec 256) :
    signextendWord 24 x = signextendSpec 24 x := by
  simp only [signextendWord, signextendSpec]
  bv_decide

theorem signextendWord_spec_25 (x : BitVec 256) :
    signextendWord 25 x = signextendSpec 25 x := by
  simp only [signextendWord, signextendSpec]
  bv_decide

theorem signextendWord_spec_26 (x : BitVec 256) :
    signextendWord 26 x = signextendSpec 26 x := by
  simp only [signextendWord, signextendSpec]
  bv_decide

theorem signextendWord_spec_27 (x : BitVec 256) :
    signextendWord 27 x = signextendSpec 27 x := by
  simp only [signextendWord, signextendSpec]
  bv_decide

theorem signextendWord_spec_28 (x : BitVec 256) :
    signextendWord 28 x = signextendSpec 28 x := by
  simp only [signextendWord, signextendSpec]
  bv_decide

theorem signextendWord_spec_29 (x : BitVec 256) :
    signextendWord 29 x = signextendSpec 29 x := by
  simp only [signextendWord, signextendSpec]
  bv_decide

theorem signextendWord_spec_30 (x : BitVec 256) :
    signextendWord 30 x = signextendSpec 30 x := by
  simp only [signextendWord, signextendSpec]
  bv_decide

theorem signextendWord_spec_31 (x : BitVec 256) :
    signextendWord 31 x = signextendSpec 31 x := by
  simp only [signextendWord, signextendSpec]
  bv_decide

/-- **`SIGNEXTEND` lowering.** For every constant byte index, `signextend_word` is the EVM
opcode. -/
theorem signextendWord_spec (k : Nat) (x : BitVec 256) : signextendWord k x = signextendSpec k x := by
  match k with
  | 0 => exact signextendWord_spec_0 x
  | 1 => exact signextendWord_spec_1 x
  | 2 => exact signextendWord_spec_2 x
  | 3 => exact signextendWord_spec_3 x
  | 4 => exact signextendWord_spec_4 x
  | 5 => exact signextendWord_spec_5 x
  | 6 => exact signextendWord_spec_6 x
  | 7 => exact signextendWord_spec_7 x
  | 8 => exact signextendWord_spec_8 x
  | 9 => exact signextendWord_spec_9 x
  | 10 => exact signextendWord_spec_10 x
  | 11 => exact signextendWord_spec_11 x
  | 12 => exact signextendWord_spec_12 x
  | 13 => exact signextendWord_spec_13 x
  | 14 => exact signextendWord_spec_14 x
  | 15 => exact signextendWord_spec_15 x
  | 16 => exact signextendWord_spec_16 x
  | 17 => exact signextendWord_spec_17 x
  | 18 => exact signextendWord_spec_18 x
  | 19 => exact signextendWord_spec_19 x
  | 20 => exact signextendWord_spec_20 x
  | 21 => exact signextendWord_spec_21 x
  | 22 => exact signextendWord_spec_22 x
  | 23 => exact signextendWord_spec_23 x
  | 24 => exact signextendWord_spec_24 x
  | 25 => exact signextendWord_spec_25 x
  | 26 => exact signextendWord_spec_26 x
  | 27 => exact signextendWord_spec_27 x
  | 28 => exact signextendWord_spec_28 x
  | 29 => exact signextendWord_spec_29 x
  | 30 => exact signextendWord_spec_30 x
  | 31 => exact signextendWord_spec_31 x
  | k + 32 =>
    simp only [signextendWord, signextendSpec, show 32 ≤ k + 32 by omega, if_true,
      show ¬ k + 32 < 31 by omega, if_false]

/-- `signextend_word_dynamic(i, v)`: `ite(i == 0, se(0), ite(i == 1, se(1), ... ite(i == 30, se(30),
v)))`, built from index 30 down. -/
def signextendChain (i v : Expr) : Nat → Expr
  | 0 => v
  | n + 1 => .ite (.cmp .eq i (.const (BitVec.ofNat 256 (31 - (n + 1)))))
      (signextendExpr (31 - (n + 1)) v) (signextendChain i v n)

/-- The dynamic lowering, `signextendChain i v 31`. -/
def signextendDynamic (i v : Expr) : Expr := signextendChain i v 31

theorem evalK_signextendChain {env : Env} {i v : Expr} {k x : Word} (hi : evalK env i = some k)
    (hv : evalK env v = some x) (n : Nat) (hn : n ≤ 31) :
    evalK env (signextendChain i v n) =
      some (if 31 - n ≤ k.toNat ∧ k.toNat < 31 then signextendSpec k.toNat x else x) := by
  induction n with
  | zero =>
    simp only [signextendChain, hv, Option.some.injEq]
    rw [if_neg (by omega)]
  | succ n ih =>
    simp only [signextendChain, evalK_ite', evalKB_cmp', hi, evalK_const', kcmp_some_some,
      CmpOp.eval]
    by_cases hk : k = BitVec.ofNat 256 (31 - (n + 1))
    · have hkn : k.toNat = 31 - (n + 1) := by
        rw [hk, BitVec.toNat_ofNat]; exact Nat.mod_eq_of_lt (by omega)
      simp only [hk, decide_true, kite_true]
      rw [evalK_signextendExpr _ hv, signextendWord_spec]
      rw [← hk, hkn]
      simp only [show 31 - (n + 1) < 31 by omega, and_true, show 31 - (n + 1) ≤ 31 - (n + 1) from
        Nat.le_refl _, if_true]
    · simp only [decide_eq_true_eq, hk, decide_false, kite_false]
      rw [ih (by omega)]
      congr 1
      have hne : k.toNat ≠ 31 - (n + 1) := by
        intro h; apply hk; apply BitVec.eq_of_toNat_eq; rw [h, BitVec.toNat_ofNat]
        exact (Nat.mod_eq_of_lt (by omega)).symm
      by_cases h1 : 31 - n ≤ k.toNat ∧ k.toNat < 31
      · rw [if_pos h1, if_pos ⟨by omega, h1.2⟩]
      · rw [if_neg h1]
        by_cases h2 : 31 - (n + 1) ≤ k.toNat ∧ k.toNat < 31
        · exfalso; apply h1; constructor <;> omega
        · rw [if_neg h2]

/-- **Dynamic `SIGNEXTEND` lowering.** For a symbolic index, the chain is the EVM opcode. -/
theorem evalK_signextendDynamic {env : Env} {i v : Expr} {k x : Word}
    (hi : evalK env i = some k) (hv : evalK env v = some x) :
    evalK env (signextendDynamic i v) = some (signextendSpec k.toNat x) := by
  rw [signextendDynamic, evalK_signextendChain hi hv 31 (Nat.le_refl _)]
  by_cases h : k.toNat < 31
  · simp [h]
  · simp [h, signextendSpec]

/-! ## `BYTE` -/

/-- The EVM `BYTE(i, x)`. -/
def byteSpec (i : Nat) (x : BitVec 256) : BitVec 256 :=
  if i < 32 then (x >>> (8 * (31 - i))) &&& 0xff else 0

/-- `SymExpr::extracted_byte`: `(x >> (31 - i) * 8) & 0xff`. -/
def byteExpr (i : Nat) (v : Expr) : Expr :=
  .bin .and (.bin .shr v (.const (BitVec.ofNat 256 ((31 - i) * 8)))) (.const 0xff)

/-- `byte_word_dynamic(i, v)` for a symbolic word: a chain over the 32 indices that defaults to 0. -/
def byteChain (i v : Expr) : Nat → Expr
  | 0 => .const 0
  | n + 1 => .ite (.cmp .eq i (.const (BitVec.ofNat 256 (32 - (n + 1)))))
      (byteExpr (32 - (n + 1)) v) (byteChain i v n)

theorem evalK_byteExpr {env : Env} {v : Expr} {x : Word} (i : Nat) (hi : i < 32)
    (hv : evalK env v = some x) : evalK env (byteExpr i v) = some (byteSpec i x) := by
  simp only [byteExpr, evalK_bin', evalK_const', hv, kbin_some_some, BinOp.isDiv,
    Bool.false_eq_true, false_and, if_false, BinOp.evm, byteSpec, hi, if_true]
  congr 2
  rw [BitVec.ushiftRight_eq', BitVec.toNat_ofNat, Nat.mod_eq_of_lt (by omega), Nat.mul_comm]

theorem evalK_byteChain {env : Env} {i v : Expr} {k x : Word} (hi : evalK env i = some k)
    (hv : evalK env v = some x) (n : Nat) (hn : n ≤ 32) :
    evalK env (byteChain i v n) =
      some (if 32 - n ≤ k.toNat ∧ k.toNat < 32 then byteSpec k.toNat x else 0) := by
  induction n with
  | zero =>
    simp only [byteChain, evalK_const', Option.some.injEq]
    rw [if_neg (by omega)]
  | succ n ih =>
    simp only [byteChain, evalK_ite', evalKB_cmp', hi, evalK_const', kcmp_some_some, CmpOp.eval]
    by_cases hk : k = BitVec.ofNat 256 (32 - (n + 1))
    · have hkn : k.toNat = 32 - (n + 1) := by
        rw [hk, BitVec.toNat_ofNat]; exact Nat.mod_eq_of_lt (by omega)
      simp only [hk, decide_true, kite_true]
      rw [evalK_byteExpr _ (by omega) hv, ← hk, hkn]
      simp only [show 32 - (n + 1) < 32 by omega, and_true, Nat.le_refl, if_true]
    · simp only [decide_eq_true_eq, hk, decide_false, kite_false]
      rw [ih (by omega)]
      congr 1
      have hne : k.toNat ≠ 32 - (n + 1) := by
        intro h; apply hk; apply BitVec.eq_of_toNat_eq; rw [h, BitVec.toNat_ofNat]
        exact (Nat.mod_eq_of_lt (by omega)).symm
      by_cases h1 : 32 - n ≤ k.toNat ∧ k.toNat < 32
      · rw [if_pos h1, if_pos ⟨by omega, h1.2⟩]
      · rw [if_neg h1]
        by_cases h2 : 32 - (n + 1) ≤ k.toNat ∧ k.toNat < 32
        · exfalso; apply h1; constructor <;> omega
        · rw [if_neg h2]

/-- **Dynamic `BYTE` lowering.** For a symbolic index and word, the chain is the EVM opcode,
including zero for every index of at least 32. -/
theorem evalK_byteDynamic {env : Env} {i v : Expr} {k x : Word} (hi : evalK env i = some k)
    (hv : evalK env v = some x) : evalK env (byteChain i v 32) = some (byteSpec k.toNat x) := by
  rw [evalK_byteChain hi hv 32 (Nat.le_refl _)]
  by_cases h : k.toNat < 32
  · simp [h]
  · simp [h, byteSpec]

/-! ## `CLZ` -/

/-- `clz_word` as a word function: the binary search shifts out a zero high half of `bits` bits and
adds `bits` to the count, for `bits = 128, 64, ..., 1`. -/
def clzWord (x : BitVec 256) : BitVec 256 :=
  if x = 0 then 256 else
    let z0 := x.ult (BitVec.twoPow 256 128)
    let c0 : BitVec 256 := if z0 then 128 else 0
    let w0 := if z0 then x <<< 128 else x
    let z1 := w0.ult (BitVec.twoPow 256 192)
    let c1 := if z1 then c0 + 64 else c0
    let w1 := if z1 then w0 <<< 64 else w0
    let z2 := w1.ult (BitVec.twoPow 256 224)
    let c2 := if z2 then c1 + 32 else c1
    let w2 := if z2 then w1 <<< 32 else w1
    let z3 := w2.ult (BitVec.twoPow 256 240)
    let c3 := if z3 then c2 + 16 else c2
    let w3 := if z3 then w2 <<< 16 else w2
    let z4 := w3.ult (BitVec.twoPow 256 248)
    let c4 := if z4 then c3 + 8 else c3
    let w4 := if z4 then w3 <<< 8 else w3
    let z5 := w4.ult (BitVec.twoPow 256 252)
    let c5 := if z5 then c4 + 4 else c4
    let w5 := if z5 then w4 <<< 4 else w4
    let z6 := w5.ult (BitVec.twoPow 256 254)
    let c6 := if z6 then c5 + 2 else c5
    let w6 := if z6 then w5 <<< 2 else w5
    let z7 := w6.ult (BitVec.twoPow 256 255)
    let c7 := if z7 then c6 + 1 else c6
    c7

/-- **`CLZ` lowering.** For a nonzero word, the binary search returns the number of leading zero
bits: shifting right by `255 - clz` leaves exactly 1. -/
theorem clzWord_spec (x : BitVec 256) (hx : x ≠ 0) :
    clzWord x ≤ 255 ∧ x >>> (255 - clzWord x) = 1 := by
  simp only [clzWord, hx, if_false]
  bv_decide

theorem clzWord_zero : clzWord 0 = 256 := by simp [clzWord]

/-! ## `EXP` -/

/-- `exp_expr_for_concrete_exponent(base, n)`: `1` for `n == 0`, else `base * base * ... * base`. -/
def expProduct (base : Expr) : Nat → Expr
  | 0 => .const 1
  | 1 => base
  | n + 2 => .bin .mul (expProduct base (n + 1)) base

/-- The EVM `EXP(b, e)`. -/
def expSpec (b : Word) (e : Nat) : Word := BitVec.ofNat 256 (b.toNat ^ e)

theorem evalK_expProduct {env : Env} {base : Expr} {b : Word} (hb : evalK env base = some b)
    (n : Nat) : evalK env (expProduct base n) = some (expSpec b n) := by
  induction n using Nat.strongRecOn with
  | _ n ih =>
    match n with
    | 0 => simp [expProduct, expSpec]
    | 1 => simp [expProduct, expSpec, hb]
    | n + 2 =>
      simp only [expProduct, evalK_bin', ih (n + 1) (by omega), hb, kbin_some_some, BinOp.isDiv,
        Bool.false_eq_true, false_and, if_false, BinOp.evm, Option.some.injEq]
      apply BitVec.eq_of_toNat_eq
      rw [expSpec, expSpec, BitVec.toNat_mul, BitVec.toNat_ofNat, BitVec.toNat_ofNat,
        Nat.pow_succ b.toNat (n + 1), Nat.mod_mul_mod]

/-- The symbolic-exponent chain of `exp_word`: `ite(e == 0, b^0, ite(e == 1, b^1, ... ite(e == max,
b^max, 0)))`. -/
def expChain (base e : Expr) (max : Nat) : Nat → Expr
  | 0 => .const 0
  | n + 1 => .ite (.cmp .eq e (.const (BitVec.ofNat 256 (max + 1 - (n + 1)))))
      (expProduct base (max + 1 - (n + 1))) (expChain base e max n)

/-- **Symbolic `EXP` lowering.** Under the path bound `e <= max` that `upper_bound_usize` reads
from the constraints, the chain is the EVM `EXP`. The bound must come from the path, since the
chain is 0 above it. -/
theorem evalK_expChain {env : Env} {base e : Expr} {b k : Word} (hb : evalK env base = some b)
    (he : evalK env e = some k) (max : Nat) (hmax : max < 2 ^ 256) (hk : k.toNat ≤ max) :
    evalK env (expChain base e max (max + 1)) = some (expSpec b k.toNat) := by
  have key : ∀ n, n ≤ max + 1 → evalK env (expChain base e max n) =
      some (if max + 1 - n ≤ k.toNat then expSpec b k.toNat else 0) := by
    intro n hn
    induction n with
    | zero => simp [expChain, show ¬ max + 1 ≤ k.toNat by omega]
    | succ n ih =>
      simp only [expChain, evalK_ite', evalKB_cmp', he, evalK_const', kcmp_some_some, CmpOp.eval]
      by_cases hkn : k = BitVec.ofNat 256 (max + 1 - (n + 1))
      · have hv : k.toNat = max + 1 - (n + 1) := by
          rw [hkn, BitVec.toNat_ofNat]; exact Nat.mod_eq_of_lt (by omega)
        simp only [hkn, decide_true, kite_true]
        rw [evalK_expProduct hb, ← hkn, hv]
        simp
      · simp only [decide_eq_true_eq, hkn, decide_false, kite_false]
        rw [ih (by omega)]
        have hne : k.toNat ≠ max + 1 - (n + 1) := by
          intro h; apply hkn; apply BitVec.eq_of_toNat_eq; rw [h, BitVec.toNat_ofNat]
          exact (Nat.mod_eq_of_lt (by omega)).symm
        by_cases h1 : max + 1 - n ≤ k.toNat
        · rw [if_pos h1, if_pos (by omega)]
        · rw [if_neg h1, if_neg (by omega)]
  rw [key (max + 1) (Nat.le_refl _)]
  simp

/-! ## ABI input constraints -/

/-- **`constrain_uint`.** `x < 2 ^ bits` holds exactly for the words that are `uintN` values. -/
theorem constrain_uint_exact (bits : Nat) (hbits : bits < 256) (x : Word) :
    x.ult (BitVec.twoPow 256 bits) = true ↔ x.toNat < 2 ^ bits := by
  rw [BitVec.ult_iff_lt, BitVec.lt_def, BitVec.toNat_twoPow_of_lt hbits]

/-- Signed range of `intN` for `N = 8 * (k + 1)`: `-2^(N-1) <= x < 2^(N-1)`, written without
`toInt` as the wrapping offset `x + 2^(N-1) < 2^N`. -/
def fitsSigned (k : Nat) (x : Word) : Prop :=
  (x + BitVec.twoPow 256 (8 * k + 7)).toNat < 2 ^ (8 * k + 8)

theorem constrain_int_exact_0 (x : BitVec 256) :
    x = signextendWord 0 x ↔ (x + BitVec.twoPow 256 7).ult (BitVec.twoPow 256 8) := by
  simp only [signextendWord]
  bv_decide

theorem constrain_int_exact_1 (x : BitVec 256) :
    x = signextendWord 1 x ↔ (x + BitVec.twoPow 256 15).ult (BitVec.twoPow 256 16) := by
  simp only [signextendWord]
  bv_decide

theorem constrain_int_exact_2 (x : BitVec 256) :
    x = signextendWord 2 x ↔ (x + BitVec.twoPow 256 23).ult (BitVec.twoPow 256 24) := by
  simp only [signextendWord]
  bv_decide

theorem constrain_int_exact_3 (x : BitVec 256) :
    x = signextendWord 3 x ↔ (x + BitVec.twoPow 256 31).ult (BitVec.twoPow 256 32) := by
  simp only [signextendWord]
  bv_decide

theorem constrain_int_exact_4 (x : BitVec 256) :
    x = signextendWord 4 x ↔ (x + BitVec.twoPow 256 39).ult (BitVec.twoPow 256 40) := by
  simp only [signextendWord]
  bv_decide

theorem constrain_int_exact_5 (x : BitVec 256) :
    x = signextendWord 5 x ↔ (x + BitVec.twoPow 256 47).ult (BitVec.twoPow 256 48) := by
  simp only [signextendWord]
  bv_decide

theorem constrain_int_exact_6 (x : BitVec 256) :
    x = signextendWord 6 x ↔ (x + BitVec.twoPow 256 55).ult (BitVec.twoPow 256 56) := by
  simp only [signextendWord]
  bv_decide

theorem constrain_int_exact_7 (x : BitVec 256) :
    x = signextendWord 7 x ↔ (x + BitVec.twoPow 256 63).ult (BitVec.twoPow 256 64) := by
  simp only [signextendWord]
  bv_decide

theorem constrain_int_exact_8 (x : BitVec 256) :
    x = signextendWord 8 x ↔ (x + BitVec.twoPow 256 71).ult (BitVec.twoPow 256 72) := by
  simp only [signextendWord]
  bv_decide

theorem constrain_int_exact_9 (x : BitVec 256) :
    x = signextendWord 9 x ↔ (x + BitVec.twoPow 256 79).ult (BitVec.twoPow 256 80) := by
  simp only [signextendWord]
  bv_decide

theorem constrain_int_exact_10 (x : BitVec 256) :
    x = signextendWord 10 x ↔ (x + BitVec.twoPow 256 87).ult (BitVec.twoPow 256 88) := by
  simp only [signextendWord]
  bv_decide

theorem constrain_int_exact_11 (x : BitVec 256) :
    x = signextendWord 11 x ↔ (x + BitVec.twoPow 256 95).ult (BitVec.twoPow 256 96) := by
  simp only [signextendWord]
  bv_decide

theorem constrain_int_exact_12 (x : BitVec 256) :
    x = signextendWord 12 x ↔ (x + BitVec.twoPow 256 103).ult (BitVec.twoPow 256 104) := by
  simp only [signextendWord]
  bv_decide

theorem constrain_int_exact_13 (x : BitVec 256) :
    x = signextendWord 13 x ↔ (x + BitVec.twoPow 256 111).ult (BitVec.twoPow 256 112) := by
  simp only [signextendWord]
  bv_decide

theorem constrain_int_exact_14 (x : BitVec 256) :
    x = signextendWord 14 x ↔ (x + BitVec.twoPow 256 119).ult (BitVec.twoPow 256 120) := by
  simp only [signextendWord]
  bv_decide

theorem constrain_int_exact_15 (x : BitVec 256) :
    x = signextendWord 15 x ↔ (x + BitVec.twoPow 256 127).ult (BitVec.twoPow 256 128) := by
  simp only [signextendWord]
  bv_decide

theorem constrain_int_exact_16 (x : BitVec 256) :
    x = signextendWord 16 x ↔ (x + BitVec.twoPow 256 135).ult (BitVec.twoPow 256 136) := by
  simp only [signextendWord]
  bv_decide

theorem constrain_int_exact_17 (x : BitVec 256) :
    x = signextendWord 17 x ↔ (x + BitVec.twoPow 256 143).ult (BitVec.twoPow 256 144) := by
  simp only [signextendWord]
  bv_decide

theorem constrain_int_exact_18 (x : BitVec 256) :
    x = signextendWord 18 x ↔ (x + BitVec.twoPow 256 151).ult (BitVec.twoPow 256 152) := by
  simp only [signextendWord]
  bv_decide

theorem constrain_int_exact_19 (x : BitVec 256) :
    x = signextendWord 19 x ↔ (x + BitVec.twoPow 256 159).ult (BitVec.twoPow 256 160) := by
  simp only [signextendWord]
  bv_decide

theorem constrain_int_exact_20 (x : BitVec 256) :
    x = signextendWord 20 x ↔ (x + BitVec.twoPow 256 167).ult (BitVec.twoPow 256 168) := by
  simp only [signextendWord]
  bv_decide

theorem constrain_int_exact_21 (x : BitVec 256) :
    x = signextendWord 21 x ↔ (x + BitVec.twoPow 256 175).ult (BitVec.twoPow 256 176) := by
  simp only [signextendWord]
  bv_decide

theorem constrain_int_exact_22 (x : BitVec 256) :
    x = signextendWord 22 x ↔ (x + BitVec.twoPow 256 183).ult (BitVec.twoPow 256 184) := by
  simp only [signextendWord]
  bv_decide

theorem constrain_int_exact_23 (x : BitVec 256) :
    x = signextendWord 23 x ↔ (x + BitVec.twoPow 256 191).ult (BitVec.twoPow 256 192) := by
  simp only [signextendWord]
  bv_decide

theorem constrain_int_exact_24 (x : BitVec 256) :
    x = signextendWord 24 x ↔ (x + BitVec.twoPow 256 199).ult (BitVec.twoPow 256 200) := by
  simp only [signextendWord]
  bv_decide

theorem constrain_int_exact_25 (x : BitVec 256) :
    x = signextendWord 25 x ↔ (x + BitVec.twoPow 256 207).ult (BitVec.twoPow 256 208) := by
  simp only [signextendWord]
  bv_decide

theorem constrain_int_exact_26 (x : BitVec 256) :
    x = signextendWord 26 x ↔ (x + BitVec.twoPow 256 215).ult (BitVec.twoPow 256 216) := by
  simp only [signextendWord]
  bv_decide

theorem constrain_int_exact_27 (x : BitVec 256) :
    x = signextendWord 27 x ↔ (x + BitVec.twoPow 256 223).ult (BitVec.twoPow 256 224) := by
  simp only [signextendWord]
  bv_decide

theorem constrain_int_exact_28 (x : BitVec 256) :
    x = signextendWord 28 x ↔ (x + BitVec.twoPow 256 231).ult (BitVec.twoPow 256 232) := by
  simp only [signextendWord]
  bv_decide

theorem constrain_int_exact_29 (x : BitVec 256) :
    x = signextendWord 29 x ↔ (x + BitVec.twoPow 256 239).ult (BitVec.twoPow 256 240) := by
  simp only [signextendWord]
  bv_decide

theorem constrain_int_exact_30 (x : BitVec 256) :
    x = signextendWord 30 x ↔ (x + BitVec.twoPow 256 247).ult (BitVec.twoPow 256 248) := by
  simp only [signextendWord]
  bv_decide

/-- **`constrain_int`.** `x == signextend(N / 8 - 1, x)` holds exactly for the words in the signed
range of `intN`, for every `N = 8, 16, ..., 248`. -/
theorem constrain_int_exact (k : Nat) (hk : k < 31) (x : Word) :
    x = signextendWord k x ↔ fitsSigned k x := by
  have hlt : 8 * k + 8 < 256 := by omega
  rw [fitsSigned, ← BitVec.toNat_twoPow_of_lt hlt, ← BitVec.lt_def, ← BitVec.ult_iff_lt]
  match k, hk with
  | 0, _ => exact constrain_int_exact_0 x
  | 1, _ => exact constrain_int_exact_1 x
  | 2, _ => exact constrain_int_exact_2 x
  | 3, _ => exact constrain_int_exact_3 x
  | 4, _ => exact constrain_int_exact_4 x
  | 5, _ => exact constrain_int_exact_5 x
  | 6, _ => exact constrain_int_exact_6 x
  | 7, _ => exact constrain_int_exact_7 x
  | 8, _ => exact constrain_int_exact_8 x
  | 9, _ => exact constrain_int_exact_9 x
  | 10, _ => exact constrain_int_exact_10 x
  | 11, _ => exact constrain_int_exact_11 x
  | 12, _ => exact constrain_int_exact_12 x
  | 13, _ => exact constrain_int_exact_13 x
  | 14, _ => exact constrain_int_exact_14 x
  | 15, _ => exact constrain_int_exact_15 x
  | 16, _ => exact constrain_int_exact_16 x
  | 17, _ => exact constrain_int_exact_17 x
  | 18, _ => exact constrain_int_exact_18 x
  | 19, _ => exact constrain_int_exact_19 x
  | 20, _ => exact constrain_int_exact_20 x
  | 21, _ => exact constrain_int_exact_21 x
  | 22, _ => exact constrain_int_exact_22 x
  | 23, _ => exact constrain_int_exact_23 x
  | 24, _ => exact constrain_int_exact_24 x
  | 25, _ => exact constrain_int_exact_25 x
  | 26, _ => exact constrain_int_exact_26 x
  | 27, _ => exact constrain_int_exact_27 x
  | 28, _ => exact constrain_int_exact_28 x
  | 29, _ => exact constrain_int_exact_29 x
  | 30, _ => exact constrain_int_exact_30 x
  | k + 31, h => exact absurd h (by omega)

end Symbolic
