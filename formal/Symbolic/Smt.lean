import Symbolic.Word

set_option exponentiation.threshold 512

/-!
# SMT-LIB encodings of the solver writer

Models the non-trivial cases of `SmtCseWriter::write_expr`
(`crates/evm/symbolic/src/runtime/solver/smt/mod.rs`), with SMT-LIB operators as Lean's
`BitVec` operations (`bvudiv` is `smtUDiv`, `bvurem` is `%`, `(_ zero_extend k)` is `setWidth`,
`(_ extract i j)` is `extractLsb'`):

* `write_wide_modular_arithmetic`: `addmod`, `mulmod` and `mulDiv` over a 512-bit intermediate.
* The `MulHigh` case: `((_ extract 511 256) (bvmul (zext a) (zext b)))`.
* `write_narrow_division`: a division whose operands fit `width` bits, computed at that width.

Each theorem says the emitted term denotes the model meaning: `TernOp.eval`, `mulHighWord`, or
the SMT meaning `BinOp.smt` of the full-width operator. With `BinOp.evalWith_smt` and
`evalK_sound`, the solver therefore sees the Kleene meaning wherever it is defined.
-/

namespace Symbolic

/-- `(_ zero_extend 256)` of a word. -/
abbrev zext512 (a : Word) : BitVec 512 := a.setWidth 512

/-- The `write_wide_modular_arithmetic` term for `op` and `reduce`:
`ite(m == 0, 0, (_ extract 255 0) (reduce (op (zext a) (zext b)) (zext m)))`. -/
def wideModular (op reduce : BitVec 512 → BitVec 512 → BitVec 512) (a b m : Word) : Word :=
  if m = 0 then 0 else (reduce (op (zext512 a) (zext512 b)) (zext512 m)).setWidth 256

theorem pow256_le_pow512 : 2 ^ 256 ≤ 2 ^ 512 := Nat.pow_le_pow_right (by decide) (by decide)

theorem lt_pow512 (a : Word) : a.toNat < 2 ^ 512 := Nat.lt_of_lt_of_le a.isLt pow256_le_pow512

theorem toNat_zext512 (a : Word) : (zext512 a).toNat = a.toNat := by
  rw [zext512, BitVec.toNat_setWidth, Nat.mod_eq_of_lt (lt_pow512 a)]

theorem sum_lt_pow512 (a b : Word) : a.toNat + b.toNat < 2 ^ 512 := by
  have := a.isLt; have := b.isLt; omega

theorem prod_lt_pow512 (a b : Word) : a.toNat * b.toNat < 2 ^ 512 := by
  have h := Nat.mul_lt_mul'' a.isLt b.isLt
  rwa [← Nat.pow_add] at h

/-- **`addmod`.** `(bvadd, bvurem)` over 512 bits is `SymTernOp::AddMod`. -/
theorem wideModular_addMod (a b m : Word) :
    wideModular (· + ·) (· % ·) a b m = TernOp.addMod.eval a b m := by
  unfold wideModular TernOp.eval
  split
  · rfl
  · apply BitVec.eq_of_toNat_eq
    rw [BitVec.toNat_setWidth, BitVec.toNat_umod, BitVec.toNat_add, toNat_zext512, toNat_zext512,
      toNat_zext512, Nat.mod_eq_of_lt (sum_lt_pow512 a b), BitVec.toNat_ofNat]

/-- **`mulmod`.** `(bvmul, bvurem)` over 512 bits is `SymTernOp::MulMod`. -/
theorem wideModular_mulMod (a b m : Word) :
    wideModular (· * ·) (· % ·) a b m = TernOp.mulMod.eval a b m := by
  unfold wideModular TernOp.eval
  split
  · rfl
  · apply BitVec.eq_of_toNat_eq
    rw [BitVec.toNat_setWidth, BitVec.toNat_umod, BitVec.toNat_mul, toNat_zext512, toNat_zext512,
      toNat_zext512, Nat.mod_eq_of_lt (prod_lt_pow512 a b), BitVec.toNat_ofNat]

/-- **`mulDiv`.** `(bvmul, bvudiv)` over 512 bits, truncated to the low word, is
`SymTernOp::MulDiv`. -/
theorem wideModular_mulDiv (a b m : Word) :
    wideModular (· * ·) (· / ·) a b m = TernOp.mulDiv.eval a b m := by
  unfold wideModular TernOp.eval
  split
  · rfl
  · apply BitVec.eq_of_toNat_eq
    rw [BitVec.toNat_setWidth, BitVec.toNat_udiv, BitVec.toNat_mul, toNat_zext512, toNat_zext512,
      toNat_zext512, Nat.mod_eq_of_lt (prod_lt_pow512 a b), BitVec.toNat_ofNat]

/-- **`MulHigh`.** `((_ extract 511 256) (bvmul (zext a) (zext b)))` is the high word of the
512-bit product. -/
theorem mulHigh_encoding (a b : Word) :
    (zext512 a * zext512 b).extractLsb' 256 256 = BinOp.mulHighWord a b := by
  apply BitVec.eq_of_toNat_eq
  rw [BitVec.extractLsb'_toNat, BitVec.toNat_mul, toNat_zext512, toNat_zext512,
    Nat.mod_eq_of_lt (prod_lt_pow512 a b), BinOp.mulHighWord, BitVec.toNat_ofNat,
    Nat.shiftRight_eq_div_pow]

/-- The `write_narrow_division` term for `UDiv`:
`ite(b == 0, ~0, (_ zero_extend 256 - w) (bvudiv ((_ extract w-1 0) a) ((_ extract w-1 0) b)))`. -/
def narrowUDiv (w : Nat) (a b : Word) : Word :=
  if b = 0 then BitVec.allOnes 256 else ((a.setWidth w).smtUDiv (b.setWidth w)).setWidth 256

/-- The `write_narrow_division` term for `URem`:
`(_ zero_extend 256 - w) (bvurem ((_ extract w-1 0) a) ((_ extract w-1 0) b))`. -/
def narrowURem (w : Nat) (a b : Word) : Word :=
  ((a.setWidth w) % (b.setWidth w)).setWidth 256

theorem toNat_setWidth_of_lt {w : Nat} {a : Word} (ha : a.toNat < 2 ^ w) :
    (a.setWidth w).toNat = a.toNat := by
  rw [BitVec.toNat_setWidth, Nat.mod_eq_of_lt ha]

/-- **Narrow `UDiv`.** When both operands fit `w < 256` bits, the narrow term is SMT-LIB
`bvudiv` at full width, including the all-ones result for a zero divisor. -/
theorem narrowUDiv_eq {w : Nat} {a b : Word} (ha : a.toNat < 2 ^ w)
    (hb : b.toNat < 2 ^ w) : narrowUDiv w a b = BinOp.udiv.smt a b := by
  unfold narrowUDiv
  simp only [BinOp.smt, BitVec.smtUDiv]
  split
  · rfl
  · rename_i hb0
    have hb0' : b.setWidth w ≠ 0 := by
      intro h
      apply hb0
      apply BitVec.eq_of_toNat_eq
      have := congrArg BitVec.toNat h
      rw [toNat_setWidth_of_lt hb] at this
      simpa using this
    rw [if_neg hb0']
    apply BitVec.eq_of_toNat_eq
    have hq : a.toNat / b.toNat < 2 ^ 256 := Nat.lt_of_le_of_lt (Nat.div_le_self _ _) a.isLt
    rw [BitVec.toNat_setWidth, BitVec.udiv_eq, BitVec.toNat_udiv, toNat_setWidth_of_lt ha,
      toNat_setWidth_of_lt hb, BitVec.udiv_eq, BitVec.toNat_udiv, Nat.mod_eq_of_lt hq]

/-- **Narrow `URem`.** When both operands fit `w <= 256` bits, the narrow term is SMT-LIB
`bvurem` at full width, including the dividend for a zero divisor. -/
theorem narrowURem_eq {w : Nat} {a b : Word} (ha : a.toNat < 2 ^ w) (hb : b.toNat < 2 ^ w) :
    narrowURem w a b = BinOp.urem.smt a b := by
  unfold narrowURem
  simp only [BinOp.smt]
  apply BitVec.eq_of_toNat_eq
  have hr : a.toNat % b.toNat < 2 ^ 256 := Nat.lt_of_le_of_lt (Nat.mod_le _ _) a.isLt
  rw [BitVec.toNat_setWidth, BitVec.toNat_umod, toNat_setWidth_of_lt ha, toNat_setWidth_of_lt hb,
    BitVec.toNat_umod, Nat.mod_eq_of_lt hr]

end Symbolic
