import Symbolic.Expr

/-!
# The division guard invariant

`SymbolicState::bin_word_div_zero_guard` (`crates/evm/symbolic/src/runtime/state.rs`) lowers
`DIV`, `SDIV`, `MOD` and `SMOD` to `ite(b == 0, 0, a op b)`. The raw division inside is
undefined at `b == 0` in the Kleene meaning, but the guard never selects it there, so the
lowered word is defined wherever its operands are, with the EVM value (`evalK_divGuard`).

`WellGuarded` is the syntactic shape the executor builds: every raw division either divides by
a nonzero constant or sits in the else arm of its own zero guard. Such expressions are defined
everywhere (`WellGuarded.defined`), so a path condition made of them satisfies the `Defined`
hypothesis of `unsat_transfer`. Construction-time rewrites then only need to be refinements,
because a refinement of a defined expression is defined with the same value.
-/

namespace Symbolic

/-- The lowering of `DIV`, `SDIV`, `MOD` and `SMOD`: `ite(b == 0, 0, a op b)`. -/
def divGuard (op : BinOp) (a b : Expr) : Expr :=
  .ite (.cmp .eq b (.const 0)) (.const 0) (.bin op a b)

/-- EVM semantics of the four guarded opcodes: zero at a zero divisor. -/
theorem evm_div_zero {op : BinOp} (h : op.isDiv = true) (x : Word) : op.evm x 0#256 = 0#256 := by
  cases op <;> simp_all [BinOp.isDiv, BinOp.evm]

/-- **Opcode lowering.** The guarded division is defined wherever its operands are, and its value
is the EVM opcode result. -/
theorem evalK_divGuard {env : Env} {op : BinOp} {a b : Expr} {x y : Word}
    (ha : evalK env a = some x) (hb : evalK env b = some y) (hop : op.isDiv = true) :
    evalK env (divGuard op a b) = some (op.evm x y) := by
  by_cases hy : y = 0
  · subst hy
    simp [divGuard, evalK, evalKB, ha, hb, CmpOp.eval, evm_div_zero hop x]
  · have hy' : y ≠ 0#256 := hy
    simp [divGuard, evalK, evalKB, ha, hb, CmpOp.eval, kbin, hy']

theorem evalK_divGuard_some {env : Env} {op : BinOp} {a b : Expr} {x y : Word}
    (ha : evalK env a = some x) (hb : evalK env b = some y) :
    ∃ v, evalK env (divGuard op a b) = some v := by
  by_cases hy : y = 0
  · subst hy
    exact ⟨0, by simp [divGuard, evalK, evalKB, ha, hb, CmpOp.eval]⟩
  · have hy' : y ≠ 0#256 := hy
    exact ⟨op.evm x y, by simp [divGuard, evalK, evalKB, ha, hb, CmpOp.eval, kbin, hy']⟩

mutual
/-- The shapes the executor builds. -/
inductive WellGuarded : Expr → Prop where
  | const (v : Word) : WellGuarded (.const v)
  | var (n : Nat) : WellGuarded (.var n)
  | not {a : Expr} : WellGuarded a → WellGuarded (.not a)
  | bin {op : BinOp} {a b : Expr} : op.isDiv = false → WellGuarded a → WellGuarded b →
      WellGuarded (.bin op a b)
  | binConst {op : BinOp} {a : Expr} {d : Word} : d ≠ 0 → WellGuarded a →
      WellGuarded (.bin op a (.const d))
  | guard {op : BinOp} {a b : Expr} : WellGuarded a → WellGuarded b →
      WellGuarded (divGuard op a b)
  | tern {op : TernOp} {a b m : Expr} : WellGuarded a → WellGuarded b → WellGuarded m →
      WellGuarded (.tern op a b m)
  | ite {c : BExpr} {a b : Expr} : WellGuardedB c → WellGuarded a → WellGuarded b →
      WellGuarded (.ite c a b)

/-- Boolean expressions over well-guarded words. -/
inductive WellGuardedB : BExpr → Prop where
  | const (b : Bool) : WellGuardedB (.const b)
  | not {a : BExpr} : WellGuardedB a → WellGuardedB (.not a)
  | and {a b : BExpr} : WellGuardedB a → WellGuardedB b → WellGuardedB (.and a b)
  | cmp {op : CmpOp} {a b : Expr} : WellGuarded a → WellGuarded b → WellGuardedB (.cmp op a b)
end

mutual
/-- **Guard invariant.** A well-guarded word is defined everywhere. -/
theorem WellGuarded.defined {e : Expr} (h : WellGuarded e) (env : Env) :
    ∃ v, evalK env e = some v := by
  cases h with
  | const v => exact ⟨v, rfl⟩
  | var n => exact ⟨env n, rfl⟩
  | not ha =>
    obtain ⟨x, hx⟩ := ha.defined env
    exact ⟨~~~ x, by simp [evalK, hx]⟩
  | bin hop ha hb =>
    obtain ⟨x, hx⟩ := ha.defined env
    obtain ⟨y, hy⟩ := hb.defined env
    exact ⟨_, by simp [evalK, hx, hy, kbin, hop]; rfl⟩
  | binConst hd ha =>
    obtain ⟨x, hx⟩ := ha.defined env
    have hd' : _ ≠ 0#256 := hd
    exact ⟨_, by simp [evalK, hx, kbin, hd']; rfl⟩
  | guard ha hb =>
    obtain ⟨x, hx⟩ := ha.defined env
    obtain ⟨y, hy⟩ := hb.defined env
    exact evalK_divGuard_some hx hy
  | tern ha hb hm =>
    obtain ⟨x, hx⟩ := ha.defined env
    obtain ⟨y, hy⟩ := hb.defined env
    obtain ⟨z, hz⟩ := hm.defined env
    exact ⟨_, by simp [evalK, hx, hy, hz]; rfl⟩
  | ite hc ha hb =>
    obtain ⟨c, hc⟩ := hc.defined env
    obtain ⟨x, hx⟩ := ha.defined env
    obtain ⟨y, hy⟩ := hb.defined env
    cases c
    · exact ⟨y, by simp [evalK, hc, hy]⟩
    · exact ⟨x, by simp [evalK, hc, hx]⟩

/-- **Guard invariant** for Boolean expressions. -/
theorem WellGuardedB.defined {e : BExpr} (h : WellGuardedB e) (env : Env) :
    ∃ v, evalKB env e = some v := by
  cases h with
  | const b => exact ⟨b, rfl⟩
  | not ha =>
    obtain ⟨x, hx⟩ := ha.defined env
    exact ⟨!x, by simp [evalKB, hx]⟩
  | and ha hb =>
    obtain ⟨x, hx⟩ := ha.defined env
    obtain ⟨y, hy⟩ := hb.defined env
    cases x <;> cases y <;> exact ⟨_, by simp [evalKB, hx, hy, kand]; rfl⟩
  | cmp ha hb =>
    obtain ⟨x, hx⟩ := ha.defined env
    obtain ⟨y, hy⟩ := hb.defined env
    exact ⟨_, by simp [evalKB, hx, hy]; rfl⟩
end

/-- A well-guarded path condition is `Defined`. -/
theorem WellGuardedB.toDefined {φ : BExpr} (h : WellGuardedB φ) : Defined φ := fun env => by
  obtain ⟨v, hv⟩ := h.defined env
  simp [hv]

/-- **End-to-end PASS soundness.** A path condition built by the executor, normalized by
refinements and contextual rewrites, then reported UNSAT under SMT-LIB semantics, has no EVM
execution. -/
theorem pass_sound {φ φ' : BExpr} (hφ : WellGuardedB φ) (hpres : PreservesTruth φ φ')
    (hunsat : ∀ env, evalB smtDZ env φ' = false) : ∀ env, evalB evmDZ env φ = false :=
  unsat_transfer' hφ.toDefined hpres hunsat

end Symbolic
