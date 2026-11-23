import Symbolic.Bits

/-!
# Comparison normalizations of `normalize/mod.rs`

`SymBoolExpr::normalize_udiv_cmp` turns comparisons against 0 and 1 into zero checks, and
`ConstraintContext::normalize_udiv_comparison` cross-multiplies a comparison with a constant-divisor
quotient once the context proves the product cannot wrap.
-/

namespace Symbolic

/-- `a > 0 => a != 0`. -/
theorem rw_ugt_zero (a : Expr) : BExpr.cmp .ugt a (.const 0) ⊑ᵇ .not (.cmp .eq a (.const 0)) := by
  intro env v hv
  obtain ⟨x, y, hx, hy, rfl⟩ := cmp_defined hv
  simp only [evalK_const', Option.some.injEq] at hy
  subst hy
  rw [evalKB_not', cmp_of_defined hx (evalK_const' 0)]
  simp only [Option.map_some, Option.some.injEq]
  by_cases h0 : x = 0
  · subst h0; decide
  · have hp := pos_of_ne_zero h0
    have h0' : ¬ x = 0#256 := h0
    simp [CmpOp.eval, BitVec.ult, BitVec.ule, h0']
    try omega

/-- `1 > a => a == 0`. -/
theorem rw_one_ugt (a : Expr) : BExpr.cmp .ugt (.const 1) a ⊑ᵇ .cmp .eq a (.const 0) := by
  intro env v hv
  obtain ⟨x, y, hx, hy, rfl⟩ := cmp_defined hv
  simp only [evalK_const', Option.some.injEq] at hx
  subst hx
  rw [cmp_of_defined hy (evalK_const' 0)]
  simp only [Option.map_some, Option.some.injEq]
  by_cases h0 : y = 0
  · subst h0; decide
  · have hp := pos_of_ne_zero h0
    have h0' : ¬ y = 0#256 := h0
    simp [CmpOp.eval, BitVec.ult, BitVec.ule, h0']
    try omega

/-- `a >= 1 => a != 0`. -/
theorem rw_uge_one (a : Expr) : BExpr.cmp .uge a (.const 1) ⊑ᵇ .not (.cmp .eq a (.const 0)) := by
  intro env v hv
  obtain ⟨x, y, hx, hy, rfl⟩ := cmp_defined hv
  simp only [evalK_const', Option.some.injEq] at hy
  subst hy
  rw [evalKB_not', cmp_of_defined hx (evalK_const' 0)]
  simp only [Option.map_some, Option.some.injEq]
  by_cases h0 : x = 0
  · subst h0; decide
  · have hp := pos_of_ne_zero h0
    have h0' : ¬ x = 0#256 := h0
    simp [CmpOp.eval, BitVec.ult, BitVec.ule, h0']
    try omega

/-- `0 >= a => a == 0`. -/
theorem rw_zero_uge (a : Expr) : BExpr.cmp .uge (.const 0) a ⊑ᵇ .cmp .eq a (.const 0) := by
  intro env v hv
  obtain ⟨x, y, hx, hy, rfl⟩ := cmp_defined hv
  simp only [evalK_const', Option.some.injEq] at hx
  subst hx
  rw [cmp_of_defined hy (evalK_const' 0)]
  simp only [Option.map_some, Option.some.injEq]
  by_cases h0 : y = 0
  · subst h0; decide
  · have hp := pos_of_ne_zero h0
    have h0' : ¬ y = 0#256 := h0
    simp [CmpOp.eval, BitVec.ult, BitVec.ule, h0']
    try omega

/-- `a <= 0 => a == 0`. -/
theorem rw_ule_zero (a : Expr) : BExpr.cmp .ule a (.const 0) ⊑ᵇ .cmp .eq a (.const 0) := by
  intro env v hv
  obtain ⟨x, y, hx, hy, rfl⟩ := cmp_defined hv
  simp only [evalK_const', Option.some.injEq] at hy
  subst hy
  rw [cmp_of_defined hx (evalK_const' 0)]
  simp only [Option.map_some, Option.some.injEq]
  by_cases h0 : x = 0
  · subst h0; decide
  · have hp := pos_of_ne_zero h0
    have h0' : ¬ x = 0#256 := h0
    simp [CmpOp.eval, BitVec.ult, BitVec.ule, h0']
    try omega

/-- `1 <= a => a != 0`. -/
theorem rw_one_ule (a : Expr) : BExpr.cmp .ule (.const 1) a ⊑ᵇ .not (.cmp .eq a (.const 0)) := by
  intro env v hv
  obtain ⟨x, y, hx, hy, rfl⟩ := cmp_defined hv
  simp only [evalK_const', Option.some.injEq] at hx
  subst hx
  rw [evalKB_not', cmp_of_defined hy (evalK_const' 0)]
  simp only [Option.map_some, Option.some.injEq]
  by_cases h0 : y = 0
  · subst h0; decide
  · have hp := pos_of_ne_zero h0
    have h0' : ¬ y = 0#256 := h0
    simp [CmpOp.eval, BitVec.ult, BitVec.ule, h0']
    try omega

/-- `a < 1 => a == 0`. -/
theorem rw_ult_one (a : Expr) : BExpr.cmp .ult a (.const 1) ⊑ᵇ .cmp .eq a (.const 0) := by
  intro env v hv
  obtain ⟨x, y, hx, hy, rfl⟩ := cmp_defined hv
  simp only [evalK_const', Option.some.injEq] at hy
  subst hy
  rw [cmp_of_defined hx (evalK_const' 0)]
  simp only [Option.map_some, Option.some.injEq]
  by_cases h0 : x = 0
  · subst h0; decide
  · have hp := pos_of_ne_zero h0
    have h0' : ¬ x = 0#256 := h0
    simp [CmpOp.eval, BitVec.ult, BitVec.ule, h0']
    try omega

/-- `0 < a => a != 0`. -/
theorem rw_zero_ult (a : Expr) : BExpr.cmp .ult (.const 0) a ⊑ᵇ .not (.cmp .eq a (.const 0)) := by
  intro env v hv
  obtain ⟨x, y, hx, hy, rfl⟩ := cmp_defined hv
  simp only [evalK_const', Option.some.injEq] at hx
  subst hx
  rw [evalKB_not', cmp_of_defined hy (evalK_const' 0)]
  simp only [Option.map_some, Option.some.injEq]
  by_cases h0 : y = 0
  · subst h0; decide
  · have hp := pos_of_ne_zero h0
    have h0' : ¬ y = 0#256 := h0
    simp [CmpOp.eval, BitVec.ult, BitVec.ule, h0']
    try omega

/-- `n / d < k => n < k * d` (`normalize_udiv_comparison`), for a nonzero constant `d`, at every
point where the context proves `k * d` does not wrap (`mul_cannot_overflow_256`). -/
theorem rw_udiv_lt (n k : Expr) (d : Word) (hd : d ≠ 0) (env : Env)
    (hfit : ∀ x, evalK env k = some x → x.toNat * d.toNat < 2 ^ 256) :
    RefinesBAt env (.cmp .ult (.bin .udiv n (.const d)) k) (.cmp .ult n (.bin .mul k (.const d))) := by
  intro v hv
  obtain ⟨q, t, hq, ht, rfl⟩ := cmp_defined hv
  obtain ⟨x, e, hx, he, _, rfl⟩ := bin_defined hq
  simp at he; subst he
  have hpos := pos_of_ne_zero hd
  rw [cmp_of_defined hx (bin_of_defined ht (evalK_const' _) (by simp [BinOp.isDiv]))]
  simp only [CmpOp.eval, BinOp.evm, BitVec.ult, BitVec.toNat_udiv, BitVec.toNat_mul,
    Nat.mod_eq_of_lt (hfit t ht), Nat.div_lt_iff_lt_mul hpos]

/-- `k <= n / d => k * d <= n` (`normalize_udiv_comparison`), under the same no-wrap fact. -/
theorem rw_le_udiv (n k : Expr) (d : Word) (hd : d ≠ 0) (env : Env)
    (hfit : ∀ x, evalK env k = some x → x.toNat * d.toNat < 2 ^ 256) :
    RefinesBAt env (.cmp .ule k (.bin .udiv n (.const d))) (.cmp .ule (.bin .mul k (.const d)) n) := by
  intro v hv
  obtain ⟨t, q, ht, hq, rfl⟩ := cmp_defined hv
  obtain ⟨x, e, hx, he, _, rfl⟩ := bin_defined hq
  simp at he; subst he
  have hpos := pos_of_ne_zero hd
  rw [cmp_of_defined (bin_of_defined ht (evalK_const' _) (by simp [BinOp.isDiv])) hx]
  simp only [CmpOp.eval, BinOp.evm, BitVec.ule, BitVec.toNat_udiv, BitVec.toNat_mul,
    Nat.mod_eq_of_lt (hfit t ht), Nat.le_div_iff_mul_le hpos]

end Symbolic
