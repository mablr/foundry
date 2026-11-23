import Symbolic.WordRules

/-!
# Ternary, `ite`, comparison and normalization rewrites

Continues `Symbolic.WordRules` with the arms of `SymExpr::ternop`, `mul_div` and `ite`
(`crates/evm/symbolic/src/runtime/expr/expr.rs`), `SymBoolExpr::cmp`, `and`, `or` and `not`
(`bool.rs`), and the division normalizations of
`crates/evm/symbolic/src/runtime/solver/normalize/mod.rs`. `Or` is the Rust encoding
`a || b => !(!a && !b)`.
-/

namespace Symbolic

/-- `SymBoolExpr::or`, which builds `!(!a && !b)`. -/
def BExpr.or (a b : BExpr) : BExpr := .not (.and (.not a) (.not b))

section Helpers
variable {env : Env}

theorem cmp_defined {op : CmpOp} {a b : Expr} {v : Bool}
    (h : evalKB env (.cmp op a b) = some v) :
    ∃ x y, evalK env a = some x ∧ evalK env b = some y ∧ op.eval x y = v :=
  kcmp_some (by simpa using h)

theorem cmp_of_defined {op : CmpOp} {a b : Expr} {x y : Word} (ha : evalK env a = some x)
    (hb : evalK env b = some y) : evalKB env (.cmp op a b) = some (op.eval x y) := by
  simp [ha, hb]

theorem tern_defined {op : TernOp} {a b m : Expr} {v : Word}
    (h : evalK env (.tern op a b m) = some v) :
    ∃ x y z, evalK env a = some x ∧ evalK env b = some y ∧ evalK env m = some z ∧
      op.eval x y z = v :=
  ktern_some (by simpa using h)

@[simp] theorem kand_true_left (b : Option Bool) : kand (some true) b = b := by
  match b with
  | none => rfl
  | some true => rfl
  | some false => rfl

@[simp] theorem kand_false_left (b : Option Bool) : kand (some false) b = some false := rfl

@[simp] theorem kand_false_right (a : Option Bool) : kand a (some false) = some false := by
  match a with
  | none => rfl
  | some true => rfl
  | some false => rfl

@[simp] theorem kand_true_right (a : Option Bool) : kand a (some true) = a := by
  match a with
  | none => rfl
  | some true => rfl
  | some false => rfl

theorem evalKB_or (a b : BExpr) :
    evalKB env (BExpr.or a b) = (kand ((evalKB env a).map (!·)) ((evalKB env b).map (!·))).map (!·)
    := by simp [BExpr.or]

end Helpers

/-! ## `SymExpr::ternop` and `mul_div` -/

/-- `addmod/mulmod(a, b, 0) => 0`, also for a modulus of 1. -/
theorem rw_ternop_modulus_zero_or_one (op : TernOp) (hop : op ≠ .mulDiv) (a b : Expr) (m : Word)
    (hm : m = 0 ∨ m = 1) : Expr.tern op a b (.const m) ⊑ .const 0 := by
  intro env v hv
  obtain ⟨x, y, z, _, _, hz, rfl⟩ := tern_defined hv
  simp at hz; subst hz
  rcases hm with rfl | rfl
  · simp [TernOp.eval]
  · cases op <;> simp_all [TernOp.eval, Nat.mod_one]

/-- `mulmod(a, m, m) => 0`, also for `m == 0`. -/
theorem rw_mulmod_self_right (a m : Expr) : Expr.tern .mulMod a m m ⊑ .const 0 := by
  intro env v hv
  obtain ⟨x, y, z, _, hy, hz, rfl⟩ := tern_defined hv
  have : y = z := Option.some.inj (hy.symm.trans hz)
  subst this
  simp only [TernOp.eval, evalK_const']
  split
  · rfl
  · simp [Nat.mul_mod_left]

/-- `mulmod(m, b, m) => 0`, also for `m == 0`. -/
theorem rw_mulmod_self_left (b m : Expr) : Expr.tern .mulMod m b m ⊑ .const 0 := by
  intro env v hv
  obtain ⟨x, y, z, hx, _, hz, rfl⟩ := tern_defined hv
  have : x = z := Option.some.inj (hx.symm.trans hz)
  subst this
  simp only [TernOp.eval, evalK_const']
  split
  · rfl
  · simp [Nat.mul_mod_right]

/-- `addmod/mulmod(a, b, m) => addmod/mulmod(ordered(a, b), m)`. -/
theorem rw_ternop_commutative (op : TernOp) (a b m : Expr) :
    Expr.tern op a b m ⊑ .tern op b a m := by
  intro env v hv
  obtain ⟨x, y, z, hx, hy, hz, rfl⟩ := tern_defined hv
  simp only [evalK_tern', hx, hy, hz, ktern_some_some, Option.some.injEq]
  cases op <;> simp [TernOp.eval, Nat.add_comm, Nat.mul_comm]

/-- `mul_div(0, b, d) => 0`, `mul_div(a, 0, d) => 0`, `mul_div(a, b, 0) => 0`. -/
theorem rw_mul_div_zero (a b d : Expr) (hz : a = .const 0 ∨ b = .const 0 ∨ d = .const 0) :
    Expr.tern .mulDiv a b d ⊑ .const 0 := by
  intro env v hv
  obtain ⟨x, y, z, hx, hy, hzz, rfl⟩ := tern_defined hv
  rcases hz with rfl | rfl | rfl <;> simp at hx hy hzz
  · subst hx; simp [TernOp.eval]
  · subst hy; simp [TernOp.eval]
  · subst hzz; simp [TernOp.eval]

/-- `mul_div(a, d, d) => ite(d == 0, 0, a)`. -/
theorem rw_mul_div_self (a d : Expr) :
    Expr.tern .mulDiv a d d ⊑ .ite (.cmp .eq d (.const 0)) (.const 0) a := by
  intro env v hv
  obtain ⟨x, y, z, hx, hy, hz, rfl⟩ := tern_defined hv
  have : y = z := Option.some.inj (hy.symm.trans hz)
  subst this
  simp only [evalK_ite', evalKB_cmp', hy, evalK_const', kcmp_some_some, CmpOp.eval, hx]
  by_cases h0 : y = 0
  · simp [h0, TernOp.eval]
  · have hpos : 0 < y.toNat := by
      rcases Nat.eq_zero_or_pos y.toNat with h | h
      · exact absurd (BitVec.eq_of_toNat_eq (by simpa using h)) h0
      · exact h
    have h0' : ¬ y = 0#256 := by simpa using h0
    simp [h0', TernOp.eval, Nat.mul_div_cancel _ hpos]

/-! ## `SymExpr::ite` -/

/-- `ite(true, a, b) => a`. -/
theorem rw_ite_true (a b : Expr) : Expr.ite (.const true) a b ⊑ a := by
  intro env v hv; simpa using hv

/-- `ite(false, a, b) => b`. -/
theorem rw_ite_false (a b : Expr) : Expr.ite (.const false) a b ⊑ b := by
  intro env v hv; simpa using hv

/-- `ite(c, a, a) => a`. The original is undefined where `c` is, so this only refines. -/
theorem rw_ite_same (c : BExpr) (a : Expr) : Expr.ite c a a ⊑ a := by
  intro env v hv
  rcases ite_defined hv with ⟨_, h⟩ | ⟨_, h⟩ <;> exact h

/-- `ite(a == 0, 0, a / a) => a != 0` (raw `UDiv`). -/
theorem rw_ite_self_div (a : Expr) :
    Expr.ite (.cmp .eq a (.const 0)) (.const 0) (.bin .udiv a a) ⊑
      boolWord (.not (.cmp .eq a (.const 0))) := by
  intro env v hv
  rcases ite_defined hv with ⟨hc, h⟩ | ⟨hc, h⟩
  · simp at h; subst h
    rw [boolWord, evalK_ite', evalKB_not', hc]
    simp
  · obtain ⟨x, y, hx, hy, hd, rfl⟩ := bin_defined h
    have : y = x := Option.some.inj (hy.symm.trans hx)
    subst this
    have hy0 : ¬ y = 0 := fun h => hd ⟨rfl, h⟩
    rw [boolWord, evalK_ite', evalKB_not', hc]
    have hy0' : ¬ y = 0#256 := by simpa using hy0
    simp [BinOp.evm, BitVec.udiv_self, hy0']

/-- `ite(c, 1, bool_word(c)) => bool_word(c)`. -/
theorem rw_ite_one_bool_word (c : BExpr) : Expr.ite c (.const 1) (boolWord c) ⊑ boolWord c := by
  intro env v hv
  rcases ite_defined hv with ⟨hc, h⟩ | ⟨hc, h⟩
  · simpa [boolWord, hc] using h
  · exact h

/-- `ite(c, bool_word(c), 0) => bool_word(c)`. -/
theorem rw_ite_bool_word_zero (c : BExpr) : Expr.ite c (boolWord c) (.const 0) ⊑ boolWord c := by
  intro env v hv
  rcases ite_defined hv with ⟨hc, h⟩ | ⟨hc, h⟩
  · exact h
  · simpa [boolWord, hc] using h

/-! ## `SymBoolExpr::cmp` -/

/-- `const == const => const`, `const < const => const` and the other constant comparisons. -/
theorem rw_cmp_const_const (op : CmpOp) (x y : Word) :
    BExpr.cmp op (.const x) (.const y) ⊑ᵇ .const (op.eval x y) := by
  intro env v hv; simpa using hv

/-- A comparison of a subexpression with itself has a fixed value. -/
theorem self_cmp_rule {op : CmpOp} {r : Bool} (h : ∀ x, op.eval x x = r) (a : Expr) :
    BExpr.cmp op a a ⊑ᵇ .const r := by
  intro env v hv
  obtain ⟨x, y, hx, hy, rfl⟩ := cmp_defined hv
  have : y = x := Option.some.inj (hy.symm.trans hx)
  subst this; simp [h]

/-- `a == a => true`. -/
theorem rw_eq_self : ∀ a, BExpr.cmp .eq a a ⊑ᵇ .const true :=
  self_cmp_rule fun x => by simp [CmpOp.eval]

/-- `a < a => false`. -/
theorem rw_ult_self : ∀ a, BExpr.cmp .ult a a ⊑ᵇ .const false :=
  self_cmp_rule fun x => by simp [CmpOp.eval, BitVec.ult]

/-- `a > a => false`. -/
theorem rw_ugt_self : ∀ a, BExpr.cmp .ugt a a ⊑ᵇ .const false :=
  self_cmp_rule fun x => by simp [CmpOp.eval, BitVec.ult]

/-- `a <= a => true`. -/
theorem rw_ule_self : ∀ a, BExpr.cmp .ule a a ⊑ᵇ .const true :=
  self_cmp_rule fun x => by simp [CmpOp.eval, BitVec.ule]

/-- `a >= a => true`. -/
theorem rw_uge_self : ∀ a, BExpr.cmp .uge a a ⊑ᵇ .const true :=
  self_cmp_rule fun x => by simp [CmpOp.eval, BitVec.ule]

/-- `a <s a => false`. -/
theorem rw_slt_self : ∀ a, BExpr.cmp .slt a a ⊑ᵇ .const false :=
  self_cmp_rule fun x => by simp [CmpOp.eval, BitVec.slt]

/-- `a >s a => false`. -/
theorem rw_sgt_self : ∀ a, BExpr.cmp .sgt a a ⊑ᵇ .const false :=
  self_cmp_rule fun x => by simp [CmpOp.eval, BitVec.slt]

/-- A comparison with one constant operand that holds or fails for every other operand. -/
theorem const_cmp_rule_right {op : CmpOp} {k : Word} {r : Bool} (h : ∀ x, op.eval x k = r)
    (a : Expr) : BExpr.cmp op a (.const k) ⊑ᵇ .const r := by
  intro env v hv
  obtain ⟨x, y, hx, hy, rfl⟩ := cmp_defined hv
  simp at hy; subst hy; simp [h]

theorem const_cmp_rule_left {op : CmpOp} {k : Word} {r : Bool} (h : ∀ x, op.eval k x = r)
    (a : Expr) : BExpr.cmp op (.const k) a ⊑ᵇ .const r := by
  intro env v hv
  obtain ⟨x, y, hx, hy, rfl⟩ := cmp_defined hv
  simp at hx; subst hx; simp [h]

theorem bv_ult_zero (x : BitVec 256) : x.ult 0 = false := by bv_decide
theorem bv_max_ult (x : BitVec 256) : (BitVec.allOnes 256).ult x = false := by bv_decide
theorem bv_ule_max (x : BitVec 256) : x.ule (BitVec.allOnes 256) = true := by bv_decide
theorem bv_zero_ule (x : BitVec 256) : (0 : BitVec 256).ule x = true := by bv_decide
theorem bv_and_ule (x m : BitVec 256) : (x &&& m).ule x = true := by bv_decide

/-- `a < 0 => false`. -/
theorem rw_ult_zero : ∀ a, BExpr.cmp .ult a (.const 0) ⊑ᵇ .const false :=
  const_cmp_rule_right fun x => bv_ult_zero x

/-- `MAX < a => false`. -/
theorem rw_max_ult : ∀ a, BExpr.cmp .ult (.const maxWord) a ⊑ᵇ .const false :=
  const_cmp_rule_left fun x => bv_max_ult x

/-- `0 > a => false`. -/
theorem rw_zero_ugt : ∀ a, BExpr.cmp .ugt (.const 0) a ⊑ᵇ .const false :=
  const_cmp_rule_left fun x => bv_ult_zero x

/-- `a > MAX => false`. -/
theorem rw_ugt_max : ∀ a, BExpr.cmp .ugt a (.const maxWord) ⊑ᵇ .const false :=
  const_cmp_rule_right fun x => bv_max_ult x

/-- `0 <= a => true`. -/
theorem rw_zero_ule : ∀ a, BExpr.cmp .ule (.const 0) a ⊑ᵇ .const true :=
  const_cmp_rule_left fun x => bv_zero_ule x

/-- `a <= MAX => true`. -/
theorem rw_ule_max : ∀ a, BExpr.cmp .ule a (.const maxWord) ⊑ᵇ .const true :=
  const_cmp_rule_right fun x => bv_ule_max x

/-- `a >= 0 => true`. -/
theorem rw_uge_zero : ∀ a, BExpr.cmp .uge a (.const 0) ⊑ᵇ .const true :=
  const_cmp_rule_right fun x => bv_zero_ule x

/-- `MAX >= a => true`. -/
theorem rw_max_uge : ∀ a, BExpr.cmp .uge (.const maxWord) a ⊑ᵇ .const true :=
  const_cmp_rule_left fun x => bv_ule_max x

/-- A comparison between `a` and `a & m`. -/
theorem masked_cmp_rule {op : CmpOp} {r : Bool} (swap : Bool)
    (h : ∀ x m, (if swap then op.eval (x &&& m) x else op.eval x (x &&& m)) = r) (a : Expr)
    (m : Word) :
    (if swap then BExpr.cmp op (.bin .and a (.const m)) a else BExpr.cmp op a (.bin .and a (.const m)))
      ⊑ᵇ .const r := by
  intro env v hv
  cases swap
  · obtain ⟨x, y, hx, hy, rfl⟩ := cmp_defined hv
    obtain ⟨x', k, hx', hk, _, rfl⟩ := bin_defined hy
    have : x' = x := Option.some.inj (hx'.symm.trans hx)
    subst this; simp at hk; subst hk
    simpa [BinOp.evm] using (h x' m).symm
  · obtain ⟨y, x, hy, hx, rfl⟩ := cmp_defined hv
    obtain ⟨x', k, hx', hk, _, rfl⟩ := bin_defined hy
    have : x' = x := Option.some.inj (hx'.symm.trans hx)
    subst this; simp at hk; subst hk
    simpa [BinOp.evm] using (h x' m).symm

/-- `a < a & low_mask => false`. -/
theorem rw_ult_masked (a : Expr) (m : Word) :
    BExpr.cmp .ult a (.bin .and a (.const m)) ⊑ᵇ .const false :=
  masked_cmp_rule (op := .ult) false (fun x m => by
    simp only [CmpOp.eval]; have := bv_and_ule x m; simp [BitVec.ule, BitVec.ult] at *; omega) a m

/-- `a & low_mask > a => false`. -/
theorem rw_masked_ugt (a : Expr) (m : Word) :
    BExpr.cmp .ugt (.bin .and a (.const m)) a ⊑ᵇ .const false :=
  masked_cmp_rule (op := .ugt) true (fun x m => by
    simp only [CmpOp.eval]; have := bv_and_ule x m; simp [BitVec.ule, BitVec.ult] at *; omega) a m

/-- `a & low_mask <= a => true`. -/
theorem rw_masked_ule (a : Expr) (m : Word) :
    BExpr.cmp .ule (.bin .and a (.const m)) a ⊑ᵇ .const true :=
  masked_cmp_rule (op := .ule) true (fun x m => by simpa [CmpOp.eval] using bv_and_ule x m) a m

/-- `a >= a & low_mask => true`. -/
theorem rw_uge_masked (a : Expr) (m : Word) :
    BExpr.cmp .uge a (.bin .and a (.const m)) ⊑ᵇ .const true :=
  masked_cmp_rule (op := .uge) false (fun x m => by simpa [CmpOp.eval] using bv_and_ule x m) a m

/-- `a == b => ordered(a, b)`. -/
theorem rw_eq_commutative (a b : Expr) : BExpr.cmp .eq a b ⊑ᵇ .cmp .eq b a := by
  intro env v hv
  obtain ⟨x, y, hx, hy, rfl⟩ := cmp_defined hv
  rw [cmp_of_defined hy hx]
  simp [CmpOp.eval, eq_comm]

/-- `a + c == b + c => a == b`. -/
theorem rw_eq_common_offset (a b : Expr) (c : Word) :
    BExpr.cmp .eq (.bin .add a (.const c)) (.bin .add b (.const c)) ⊑ᵇ .cmp .eq a b := by
  intro env v hv
  obtain ⟨s, t, hs, ht, rfl⟩ := cmp_defined hv
  obtain ⟨x, c', hx, hc, _, rfl⟩ := bin_defined hs
  obtain ⟨y, c'', hy, hc', _, rfl⟩ := bin_defined ht
  simp at hc hc'; subst hc hc'
  rw [cmp_of_defined hx hy]
  simp [CmpOp.eval, BinOp.evm]

/-- `bool_word(c) == k` for constants, as `bool_word_eq_const`: `== 0 => !c`, `== 1 => c`,
otherwise `false`. -/
theorem rw_bool_word_eq_zero (c : BExpr) :
    BExpr.cmp .eq (boolWord c) (.const 0) ⊑ᵇ .not c := by
  intro env v hv
  obtain ⟨x, y, hx, hy, rfl⟩ := cmp_defined hv
  simp at hy; subst hy
  rcases boolWord_defined hx with ⟨hc, rfl⟩ | ⟨hc, rfl⟩ <;> simp [hc, CmpOp.eval]

theorem rw_bool_word_eq_one (c : BExpr) : BExpr.cmp .eq (boolWord c) (.const 1) ⊑ᵇ c := by
  intro env v hv
  obtain ⟨x, y, hx, hy, rfl⟩ := cmp_defined hv
  simp at hy; subst hy
  rcases boolWord_defined hx with ⟨hc, rfl⟩ | ⟨hc, rfl⟩ <;> simp [hc, CmpOp.eval]

theorem rw_bool_word_eq_other (c : BExpr) (k : Word) (h0 : k ≠ 0) (h1 : k ≠ 1) :
    BExpr.cmp .eq (boolWord c) (.const k) ⊑ᵇ .const false := by
  intro env v hv
  obtain ⟨x, y, hx, hy, rfl⟩ := cmp_defined hv
  simp at hy; subst hy
  rcases boolWord_defined hx with ⟨hc, rfl⟩ | ⟨hc, rfl⟩
  · simp only [CmpOp.eval, evalKB_const', Option.some.injEq]
    exact (decide_eq_false (fun h => h1 h.symm)).symm
  · simp only [CmpOp.eval, evalKB_const', Option.some.injEq]
    exact (decide_eq_false (fun h => h0 h.symm)).symm

/-- `ite_eq_arm`: `ite(c, e, b) == e => c || b == e`. -/
theorem rw_ite_eq_then_arm (c : BExpr) (e b : Expr) :
    BExpr.cmp .eq (.ite c e b) e ⊑ᵇ BExpr.or c (.cmp .eq b e) := by
  intro env v hv
  obtain ⟨x, y, hx, hy, rfl⟩ := cmp_defined hv
  rw [evalKB_or]
  rcases ite_defined hx with ⟨hc, h⟩ | ⟨hc, h⟩
  · have : x = y := Option.some.inj (h.symm.trans hy)
    subst this; simp [hc, CmpOp.eval]
  · rw [hc, cmp_of_defined h hy]; simp [CmpOp.eval]

/-- `ite_eq_arm`: `ite(c, a, e) == e => !c || a == e`. -/
theorem rw_ite_eq_else_arm (c : BExpr) (a e : Expr) :
    BExpr.cmp .eq (.ite c a e) e ⊑ᵇ BExpr.or (.not c) (.cmp .eq a e) := by
  intro env v hv
  obtain ⟨x, y, hx, hy, rfl⟩ := cmp_defined hv
  rw [evalKB_or]
  rcases ite_defined hx with ⟨hc, h⟩ | ⟨hc, h⟩
  · rw [evalKB_not', hc, cmp_of_defined h hy]; simp [CmpOp.eval]
  · have : x = y := Option.some.inj (h.symm.trans hy)
    subst this; simp [hc, CmpOp.eval]

/-- `constant_ite_eq`, one level: `ite(c, t, e) == k => (c && t == k) || (!c && e == k)`. -/
theorem rw_constant_ite_eq (c : BExpr) (t e : Expr) (k : Word) :
    BExpr.cmp .eq (.ite c t e) (.const k) ⊑ᵇ
      BExpr.or (.and c (.cmp .eq t (.const k))) (.and (.not c) (.cmp .eq e (.const k))) := by
  intro env v hv
  obtain ⟨x, y, hx, hy, rfl⟩ := cmp_defined hv
  simp at hy; subst hy
  rw [evalKB_or]
  rcases ite_defined hx with ⟨hc, h⟩ | ⟨hc, h⟩
  · simp only [evalKB_and', evalKB_not', hc, cmp_of_defined h (evalK_const' (env := env) k)]
    cases CmpOp.eval .eq x k <;> simp
  · simp only [evalKB_and', evalKB_not', hc, cmp_of_defined h (evalK_const' (env := env) k)]
    cases CmpOp.eval .eq x k <;> simp

/-- Aligned constant-arm `ite`s compare pointwise: `cmp(ite(c, k1, k2), ite(c, k3, k4))` is
`true`, `false`, `c` or `!c` by the table in `SymBoolExpr::cmp`. -/
def alignedCmp (op : CmpOp) (c : BExpr) (k₁ k₂ k₃ k₄ : Word) : BExpr :=
  match op.eval k₁ k₃, op.eval k₂ k₄ with
  | true, true => .const true
  | false, false => .const false
  | true, false => c
  | false, true => .not c

theorem rw_cmp_aligned_ite (op : CmpOp) (c : BExpr) (k₁ k₂ k₃ k₄ : Word) :
    BExpr.cmp op (.ite c (.const k₁) (.const k₂)) (.ite c (.const k₃) (.const k₄)) ⊑ᵇ
      alignedCmp op c k₁ k₂ k₃ k₄ := by
  intro env v hv
  obtain ⟨x, y, hx, hy, rfl⟩ := cmp_defined hv
  rcases ite_defined hx with ⟨hc, h⟩ | ⟨hc, h⟩ <;> rcases ite_defined hy with ⟨hc', h'⟩ | ⟨hc', h'⟩
  · simp at h h'; subst h h'
    unfold alignedCmp; split <;> simp_all
  · rw [hc] at hc'; cases hc'
  · rw [hc] at hc'; cases hc'
  · simp at h h'; subst h h'
    unfold alignedCmp; split <;> simp_all

/-! ## `SymBoolExpr::and`, `or`, `not` -/

/-- `true && a => a`. -/
theorem rw_and_true (a : BExpr) : BExpr.and (.const true) a ⊑ᵇ a := by
  intro env v hv; simpa using hv

/-- `false && a => false`; the right-hand `false` needs the Kleene conjunction. -/
theorem rw_and_false (a : BExpr) : BExpr.and (.const false) a ⊑ᵇ .const false := by
  intro env v hv; simpa using hv

theorem rw_and_false_right (a : BExpr) : BExpr.and a (.const false) ⊑ᵇ .const false := by
  intro env v hv; simpa using hv

/-- `(a && b) && c => a && b && c`. -/
theorem rw_and_assoc (a b c : BExpr) : BExpr.and (.and a b) c ⊑ᵇ .and a (.and b c) := by
  intro env v hv
  simp only [evalKB_and'] at hv ⊢
  cases ha : evalKB env a <;> cases hb : evalKB env b <;> cases hc : evalKB env c <;>
    simp_all [kand] <;> (try rename_i x; cases x) <;> (try rename_i y; cases y) <;>
    (try rename_i z; cases z) <;> simp_all [kand]

/-- `!const => const`. -/
theorem rw_bnot_const (b : Bool) : BExpr.not (.const b) ⊑ᵇ .const (!b) := by
  intro env v hv; simpa using hv

/-- `!!a => a`. -/
theorem rw_bnot_not (a : BExpr) : BExpr.not (.not a) ⊑ᵇ a := by
  intro env v hv
  simp only [evalKB_not', Option.map_map, Option.map_eq_some_iff] at hv
  obtain ⟨x, hx, rfl⟩ := hv
  simpa using hx

/-! ## Division normalization (`normalize/mod.rs`) -/

/-- `a / b == 0 => b == 0 || a < b` (raw `UDiv`). Under SMT-LIB semantics `a / 0` is all ones,
so the rewrite is only a refinement of the Kleene meaning. -/
theorem rw_udiv_eq_zero (a b : Expr) :
    BExpr.cmp .eq (.bin .udiv a b) (.const 0) ⊑ᵇ BExpr.or (.cmp .eq b (.const 0)) (.cmp .ult a b) := by
  intro env v hv
  obtain ⟨q, z, hq, hz, rfl⟩ := cmp_defined hv
  simp at hz; subst hz
  obtain ⟨x, y, hx, hy, hd, rfl⟩ := bin_defined hq
  have hy0 : ¬ y = 0 := fun h => hd ⟨rfl, h⟩
  have hy0' : y ≠ 0#256 := hy0
  rw [evalKB_or, cmp_of_defined hy (evalK_const' 0), cmp_of_defined hx hy]
  have hpos : 0 < y.toNat := by
    rcases Nat.eq_zero_or_pos y.toNat with h | h
    · exact absurd (BitVec.eq_of_toNat_eq (by simpa using h)) hy0'
    · exact h
  simp only [CmpOp.eval, hy0', decide_false, Option.map_some, Bool.not_false, kand_true_left,
    BinOp.evm, BitVec.ult]
  have : (x / y = 0#256) ↔ x.toNat < y.toNat := by
    rw [← BitVec.toNat_inj, BitVec.toNat_udiv, BitVec.toNat_ofNat, Nat.zero_mod,
      Nat.div_eq_zero_iff_lt hpos]
  by_cases h : x.toNat < y.toNat <;> simp [h, this, hy0']

/-- `a / b != 0 => b != 0 && a >= b` (raw `UDiv`). -/
theorem rw_udiv_ne_zero (a b : Expr) :
    BExpr.not (.cmp .eq (.bin .udiv a b) (.const 0)) ⊑ᵇ
      .and (.not (.cmp .eq b (.const 0))) (.cmp .uge a b) := by
  intro env v hv
  simp only [evalKB_not', Option.map_eq_some_iff] at hv
  obtain ⟨w, hw, rfl⟩ := hv
  obtain ⟨q, z, hq, hz, rfl⟩ := cmp_defined hw
  simp at hz; subst hz
  obtain ⟨x, y, hx, hy, hd, rfl⟩ := bin_defined hq
  have hy0 : ¬ y = 0#256 := fun h => hd ⟨rfl, h⟩
  have hpos : 0 < y.toNat := by
    rcases Nat.eq_zero_or_pos y.toNat with h | h
    · exact absurd (BitVec.eq_of_toNat_eq (by simpa using h)) hy0
    · exact h
  have key : (x / y = 0#256) ↔ x.toNat < y.toNat := by
    rw [← BitVec.toNat_inj, BitVec.toNat_udiv, BitVec.toNat_ofNat, Nat.zero_mod,
      Nat.div_eq_zero_iff_lt hpos]
  rw [evalKB_and', evalKB_not', cmp_of_defined hy (evalK_const' 0), cmp_of_defined hx hy]
  simp only [CmpOp.eval, BinOp.evm, hy0, decide_false, Option.map_some, Bool.not_false,
    kand_true_left, BitVec.ule]
  by_cases h : x.toNat < y.toNat
  · simp [key.mpr h, Nat.not_le.mpr h, hy0]
  · simp [show ¬ x / y = 0#256 from fun e => h (key.mp e), Nat.le_of_not_lt h, hy0]

/-- `ite(c, a, b) == 0 => (c && a == 0) || (!c && b == 0)`. -/
theorem rw_ite_eq_zero (c : BExpr) (a b : Expr) :
    BExpr.cmp .eq (.ite c a b) (.const 0) ⊑ᵇ
      BExpr.or (.and c (.cmp .eq a (.const 0))) (.and (.not c) (.cmp .eq b (.const 0))) :=
  rw_constant_ite_eq c a b 0

set_option simprocs false in
/-- `n / d == MAX => d == 1 && n == MAX` (`normalize_quotient_offset_eq`, raw `UDiv`). -/
theorem rw_udiv_eq_max (n d : Expr) :
    BExpr.cmp .eq (.bin .udiv n d) (.const maxWord) ⊑ᵇ
      .and (.cmp .eq d (.const 1)) (.cmp .eq n (.const maxWord)) := by
  intro env v hv
  obtain ⟨q, z, hq, hz, rfl⟩ := cmp_defined hv
  simp at hz; subst hz
  obtain ⟨x, y, hx, hy, hd, rfl⟩ := bin_defined hq
  have hy0 : y ≠ 0#256 := fun h => hd ⟨rfl, h⟩
  rw [evalKB_and', cmp_of_defined hy (evalK_const' 1), cmp_of_defined hx (evalK_const' maxWord)]
  simp only [CmpOp.eval, BinOp.evm]
  have key : (x / y = maxWord) ↔ (y = 1 ∧ x = maxWord) := by
    unfold maxWord
    constructor
    · intro h
      have hpos : 0 < y.toNat := by
        rcases Nat.eq_zero_or_pos y.toNat with h' | h'
        · exact absurd (BitVec.eq_of_toNat_eq (by simpa using h')) hy0
        · exact h'
      have h1 := congrArg BitVec.toNat h
      rw [BitVec.toNat_udiv, BitVec.toNat_allOnes] at h1
      have hxl := x.isLt
      have hle : x.toNat / y.toNat ≤ x.toNat := Nat.div_le_self _ _
      have hx : x.toNat = 2 ^ 256 - 1 := by omega
      have hy1 : y.toNat = 1 := by
        rcases Nat.lt_or_ge 1 y.toNat with h2 | h2
        · have : x.toNat / y.toNat < x.toNat := Nat.div_lt_self (by omega) h2
          omega
        · omega
      exact ⟨BitVec.eq_of_toNat_eq (by simpa using hy1),
        BitVec.eq_of_toNat_eq (by rw [hx, BitVec.toNat_allOnes])⟩
    · rintro ⟨rfl, rfl⟩; simp
  by_cases h : x / y = maxWord
  · obtain ⟨h1, h2⟩ := key.mp h
    simp only [h, decide_true]
    simp only [h1, h2, decide_true, kand_true_left]
  · by_cases hy1 : y = 1
    · have h2 : ¬ x = maxWord := fun h2 => h (key.mpr ⟨hy1, h2⟩)
      simp only [h, decide_false]
      simp only [hy1, h2, decide_true, decide_false, kand_true_left]
    · simp only [h, decide_false]
      simp only [hy1, decide_false, kand_false_left]

/-- `mul_high(a, b) < b => b != 0` (`normalize_mul_high_factor_cmp`). -/
theorem rw_mul_high_lt_factor (a b : Expr) :
    BExpr.cmp .ult (.bin .mulHigh a b) b ⊑ᵇ .not (.cmp .eq b (.const 0)) := by
  intro env v hv
  obtain ⟨h, y, hh, hy, rfl⟩ := cmp_defined hv
  obtain ⟨x, y', hx, hy', _, rfl⟩ := bin_defined hh
  have : y' = y := Option.some.inj (hy'.symm.trans hy)
  subst this
  rw [evalKB_not', cmp_of_defined hy' (evalK_const' 0)]
  simp only [CmpOp.eval, BinOp.evm, BinOp.mulHighWord, Option.map_some, Option.some.injEq]
  have hlt : x.toNat * y'.toNat / 2 ^ 256 < 2 ^ 256 :=
    Nat.div_lt_of_lt_mul (Nat.mul_lt_mul'' x.isLt y'.isLt)
  by_cases h0 : y' = 0#256
  · subst h0; simp [BitVec.ult]
  · have hpos : 0 < y'.toNat := by
      rcases Nat.eq_zero_or_pos y'.toNat with h' | h'
      · exact absurd (BitVec.eq_of_toNat_eq (by simpa using h')) h0
      · exact h'
    simp only [h0, decide_false, Bool.not_false, BitVec.ult, BitVec.toNat_ofNat,
      Nat.mod_eq_of_lt hlt, decide_eq_true_eq]
    have hlt' := Nat.div_lt_of_lt_mul (Nat.mul_lt_mul_of_pos_right x.isLt hpos)
    have h0' : ¬ y' = 0 := h0
    simp [h0', hlt', h0]

theorem nat_le_sqrt {m n : Nat} : m ≤ n.sqrt ↔ m * m ≤ n := by
  constructor
  · intro h; exact Nat.le_trans (Nat.mul_le_mul h h) (Nat.sqrt_le n)
  · intro h
    apply Nat.le_of_lt_succ
    apply Nat.lt_of_not_le
    intro hle
    have := Nat.mul_le_mul hle hle
    have := Nat.lt_succ_sqrt n
    omega

theorem nat_sqrt_le_self (n : Nat) : n.sqrt ≤ n := by
  rcases Nat.eq_zero_or_pos n.sqrt with h | h
  · omega
  · exact Nat.le_trans (Nat.le_mul_of_pos_left _ h) (Nat.sqrt_le n)

/-- `a <= c / a => a <= isqrt(c)` (`normalize_const_over_self_udiv_cmp`), for the guarded
`ite(a == 0, 0, c / a)` of `DIV`. -/
theorem rw_le_const_over_self (a : Expr) (c : Word) :
    BExpr.cmp .ule a (divGuard .udiv (.const c) a) ⊑ᵇ
      .cmp .ule a (.const (BitVec.ofNat 256 c.toNat.sqrt)) := by
  intro env v hv
  obtain ⟨x, q, hx, hq, rfl⟩ := cmp_defined hv
  have hsq : c.toNat.sqrt < 2 ^ 256 := Nat.lt_of_le_of_lt (nat_sqrt_le_self _) c.isLt
  rw [cmp_of_defined hx (evalK_const' _)]
  rw [evalK_divGuard (evalK_const' c) hx rfl] at hq
  simp only [Option.some.injEq] at hq
  subst hq
  simp only [CmpOp.eval, BinOp.evm, BitVec.ule, BitVec.toNat_udiv, BitVec.toNat_ofNat,
    Nat.mod_eq_of_lt hsq, Option.some.injEq, decide_eq_decide]
  rcases Nat.eq_zero_or_pos x.toNat with h0 | hpos
  · rw [h0]; simp
  · rw [Nat.le_div_iff_mul_le hpos, nat_le_sqrt]

end Symbolic
