import Symbolic.Guard
import Std.Tactic.BVDecide

/-!
# Construction rewrites of `SymExpr::binop`, `ternop`, `mul_div`, `ite` and `not`

One theorem per match arm of `crates/evm/symbolic/src/runtime/expr/expr.rs`, named
`rw_<operator>_<shape>` and carrying the Rust comment of the arm, so `grep` finds both sides.
Every theorem states the rewrite contract `lhs ⊑ rhs` (`Refines`): wherever the original is
defined in the Kleene meaning, the rewritten expression is defined with the same value.

`bool_word(c)` is `SymExpr::bool_word`, the word `ite(c, 1, 0)`. Rust recognizes Boolean words
through `bitwise_bool_word_condition` and `direct_bool_word_condition`, which also accept other
shapes; this file proves the rewrites for the canonical shape only.
-/

namespace Symbolic

/-- `SymExpr::bool_word`. -/
def boolWord (c : BExpr) : Expr := .ite c (.const 1) (.const 0)

/-- `U256::MAX`. -/
def maxWord : Word := BitVec.allOnes 256

section Helpers
variable {env : Env}

@[simp] theorem evalK_const' (v : Word) : evalK env (.const v) = some v := by simp [evalK]
@[simp] theorem evalK_var' (n : Nat) : evalK env (.var n) = some (env n) := by simp [evalK]
@[simp] theorem evalK_not' (a : Expr) : evalK env (.not a) = (evalK env a).map (~~~ ·) := by
  simp [evalK]
@[simp] theorem evalK_bin' (op : BinOp) (a b : Expr) :
    evalK env (.bin op a b) = kbin op (evalK env a) (evalK env b) := by simp [evalK]

@[simp] theorem evalK_ite' (c : BExpr) (a b : Expr) :
    evalK env (.ite c a b) = kite (evalKB env c) (evalK env a) (evalK env b) := by simp [evalK]
@[simp] theorem evalK_tern' (op : TernOp) (a b m : Expr) :
    evalK env (.tern op a b m) = ktern op (evalK env a) (evalK env b) (evalK env m) := by
  simp [evalK]
@[simp] theorem evalKB_cmp' (op : CmpOp) (a b : Expr) :
    evalKB env (.cmp op a b) = kcmp op (evalK env a) (evalK env b) := by simp [evalKB]
@[simp] theorem evalKB_const' (b : Bool) : evalKB env (.const b) = some b := by simp [evalKB]
@[simp] theorem evalKB_not' (a : BExpr) : evalKB env (.not a) = (evalKB env a).map (!·) := by
  simp [evalKB]
@[simp] theorem evalKB_and' (a b : BExpr) :
    evalKB env (.and a b) = kand (evalKB env a) (evalKB env b) := by simp [evalKB]

/-- Destructures a defined binary node. -/
theorem bin_defined {op : BinOp} {a b : Expr} {v : Word} (h : evalK env (.bin op a b) = some v) :
    ∃ x y, evalK env a = some x ∧ evalK env b = some y ∧ ¬(op.isDiv ∧ y = 0) ∧ op.evm x y = v :=
  kbin_some (by simpa using h)

/-- Destructures a defined `ite`. -/
theorem ite_defined {c : BExpr} {a b : Expr} {v : Word} (h : evalK env (.ite c a b) = some v) :
    (evalKB env c = some true ∧ evalK env a = some v) ∨
      (evalKB env c = some false ∧ evalK env b = some v) := by
  rw [evalK_ite'] at h
  exact kite_some h

theorem bin_of_defined {op : BinOp} {a b : Expr} {x y : Word} (ha : evalK env a = some x)
    (hb : evalK env b = some y) (hd : ¬(op.isDiv ∧ y = 0)) :
    evalK env (.bin op a b) = some (op.evm x y) := by
  rw [evalK_bin', ha, hb, kbin_some_some, if_neg hd]

theorem ite_of_true {c : BExpr} {a b : Expr} (hc : evalKB env c = some true) :
    evalK env (.ite c a b) = evalK env a := by simp [evalK_ite', hc]

theorem ite_of_false {c : BExpr} {a b : Expr} (hc : evalKB env c = some false) :
    evalK env (.ite c a b) = evalK env b := by simp [evalK_ite', hc]

/-- A defined `bool_word` is 1 or 0 according to its condition. -/
theorem boolWord_defined {c : BExpr} {v : Word} (h : evalK env (boolWord c) = some v) :
    (evalKB env c = some true ∧ v = 1) ∨ (evalKB env c = some false ∧ v = 0) := by
  rcases ite_defined h with ⟨hc, hv⟩ | ⟨hc, hv⟩
  · simp at hv; exact .inl ⟨hc, hv.symm⟩
  · simp at hv; exact .inr ⟨hc, hv.symm⟩

theorem toNat_udiv_mul (x y : Word) : (x / y * y).toNat = x.toNat / y.toNat * y.toNat := by
  rw [BitVec.toNat_mul, BitVec.toNat_udiv]
  exact Nat.mod_eq_of_lt (Nat.lt_of_le_of_lt (Nat.div_mul_le_self _ _) x.isLt)

theorem udiv_mul_add_umod (x y : Word) : x / y * y + x % y = x := by
  apply BitVec.eq_of_toNat_eq
  rw [BitVec.toNat_add, toNat_udiv_mul, BitVec.toNat_umod, Nat.div_add_mod',
    Nat.mod_eq_of_lt x.isLt]

end Helpers

/-! ## Constant folding

All `const ⊙ const => const` arms evaluate with `SymBinOp::eval`, the EVM meaning. The raw node
is undefined at a zero divisor, so folding it to the EVM zero only makes it more defined. -/

/-- `const + const => const` and every other `const ⊙ const => const` arm of `SymExpr::binop`. -/
theorem rw_binop_const_const (op : BinOp) (x y : Word) :
    Expr.bin op (.const x) (.const y) ⊑ .const (op.evm x y) := by
  intro env v hv
  obtain ⟨x', y', hx, hy, _, rfl⟩ := bin_defined hv
  simp at hx hy; subst hx hy; simp

/-- `addmod/mulmod(const, const, const) => const` and
`mul_div(const, const, const) => const`. -/
theorem rw_ternop_const_const_const (op : TernOp) (x y z : Word) :
    Expr.tern op (.const x) (.const y) (.const z) ⊑ .const (op.eval x y z) := by
  intro env v hv
  simp [evalK] at hv
  simp [hv]

/-- `!const => const` for words (`SymExpr::not`). -/
theorem rw_not_const (x : Word) : Expr.not (.const x) ⊑ .const (~~~ x) := by
  intro env v hv; simp at hv; simp [hv]

/-- `!!a => a` for words (`SymExpr::not`). -/
theorem rw_not_not (a : Expr) : Expr.not (.not a) ⊑ a := by
  intro env v hv
  simp only [evalK_not', Option.map_map, Option.map_eq_some_iff] at hv
  obtain ⟨x, hx, rfl⟩ := hv
  simpa using hx

/-! ## `SymBinOp::Add` -/

/-- `0 + a => a`. -/
theorem rw_add_zero_left (a : Expr) : Expr.bin .add (.const 0) a ⊑ a := by
  intro env v hv
  obtain ⟨x, y, hx, hy, _, rfl⟩ := bin_defined hv
  simp at hx; subst hx; simpa [BinOp.evm] using hy

/-- `a + 0 => a`. -/
theorem rw_add_zero_right (a : Expr) : Expr.bin .add a (.const 0) ⊑ a := by
  intro env v hv
  obtain ⟨x, y, hx, hy, _, rfl⟩ := bin_defined hv
  simp at hy; subst hy; simpa [BinOp.evm] using hx

/-- `a + b => b + a` (`ordered_commutative_operands`), and likewise for the other commutative
operators `Mul`, `And`, `Or`, `Xor` and `MulHigh`. -/
theorem rw_commutative (op : BinOp)
    (hop : op = .add ∨ op = .mul ∨ op = .and ∨ op = .or ∨ op = .xor ∨ op = .mulHigh)
    (a b : Expr) : Expr.bin op a b ⊑ .bin op b a := by
  intro env v hv
  obtain ⟨x, y, hx, hy, _, rfl⟩ := bin_defined hv
  have hnd : op.isDiv = false := by rcases hop with rfl | rfl | rfl | rfl | rfl | rfl <;> rfl
  rw [bin_of_defined hy hx (by simp [hnd])]
  congr 1
  rcases hop with rfl | rfl | rfl | rfl | rfl | rfl <;>
    simp [BinOp.evm, BinOp.mulHighWord, BitVec.add_comm, BitVec.mul_comm, BitVec.and_comm,
      BitVec.or_comm, BitVec.xor_comm, Nat.mul_comm]

/-- `bool_word(c) + MAX => ite(c, 0, MAX)`. -/
theorem rw_add_bool_word_max (c : BExpr) :
    Expr.bin .add (boolWord c) (.const maxWord) ⊑ .ite c (.const 0) (.const maxWord) := by
  intro env v hv
  obtain ⟨x, y, hx, hy, _, rfl⟩ := bin_defined hv
  simp at hy; subst hy
  rcases boolWord_defined hx with ⟨hc, rfl⟩ | ⟨hc, rfl⟩
  · rw [ite_of_true hc]; simp [BinOp.evm, maxWord]
  · rw [ite_of_false hc]; simp [BinOp.evm, maxWord]

/-- `one_bit_word(c) + k => ite(c, k + 1, k)`. -/
theorem rw_add_bool_word_const (c : BExpr) (k : Word) :
    Expr.bin .add (boolWord c) (.const k) ⊑ .ite c (.const (k + 1)) (.const k) := by
  intro env v hv
  obtain ⟨x, y, hx, hy, _, rfl⟩ := bin_defined hv
  simp at hy; subst hy
  rcases boolWord_defined hx with ⟨hc, rfl⟩ | ⟨hc, rfl⟩
  · rw [ite_of_true hc]; simp [BinOp.evm, BitVec.add_comm]
  · rw [ite_of_false hc]; simp [BinOp.evm]

/-- `add_with_const_ite`: `a + ite(c, k1, k2) => ite(c, a + k1, a + k2)`. -/
theorem rw_add_const_ite (a : Expr) (c : BExpr) (k₁ k₂ : Word) :
    Expr.bin .add a (.ite c (.const k₁) (.const k₂)) ⊑
      .ite c (.bin .add a (.const k₁)) (.bin .add a (.const k₂)) := by
  intro env v hv
  obtain ⟨x, y, hx, hy, _, rfl⟩ := bin_defined hv
  rcases ite_defined hy with ⟨hc, hk⟩ | ⟨hc, hk⟩ <;> simp at hk <;> subst hk
  · rw [ite_of_true hc]; simp [hx, BinOp.isDiv]
  · rw [ite_of_false hc]; simp [hx, BinOp.isDiv]

/-- `(a / d) * d + a % d => a` (`euclidean_sum_dividend`, raw `UDiv` and `URem`). Under SMT-LIB
semantics this holds for every `d`; under EVM semantics only for `d != 0`. It is a refinement
because the raw nodes are undefined at `d == 0`. -/
theorem rw_add_euclidean_sum (a d : Expr) :
    Expr.bin .add (.bin .mul (.bin .udiv a d) d) (.bin .urem a d) ⊑ a := by
  intro env v hv
  obtain ⟨p, r, hp, hr, _, rfl⟩ := bin_defined hv
  obtain ⟨q, y, hq, hy, _, rfl⟩ := bin_defined hp
  obtain ⟨x, e, hx, he, hd, rfl⟩ := bin_defined hq
  obtain ⟨x₂, e₂, hx₂, he₂, _, rfl⟩ := bin_defined hr
  have h₁ : x₂ = x := Option.some.inj (hx₂.symm.trans hx)
  have h₂ : y = e := Option.some.inj (hy.symm.trans he)
  have h₃ : e₂ = e := Option.some.inj (he₂.symm.trans he)
  have hd0 : ¬ e = 0 := fun h => hd ⟨rfl, h⟩
  rw [hx, h₁, h₂, h₃]; congr 1
  simp only [BinOp.evm]
  rw [if_neg hd0]
  exact (udiv_mul_add_umod x e).symm

end Symbolic
