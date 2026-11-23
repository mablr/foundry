import Symbolic.Word

/-!
# Symbolic expressions and their three meanings

`Expr` and `BExpr` model `SymExprKind` and `SymBoolExprKind`
(`crates/evm/symbolic/src/runtime/expr/expr.rs`, `bool.rs`). Abstractions:

* `Var`, `GasLeft`, `Keccak` and `Hash` leaves are all `var n`: opaque words chosen by the
  environment. Hash collision resistance, which the Rust rewrites assume for Keccak, SHA-256 and
  RIPEMD-160, is outside this model.
* The n-ary `And` is binary `and`, with `const true` as the empty conjunction.

Three evaluators:

* `eval dz` is total. `eval evmDZ` is `SymExpr::eval_model` and `eval smtDZ` is what the solver
  sees from `write_smt_assertions`.
* `evalK` is the Kleene meaning: a raw division by zero is undefined (`none`), `ite` evaluates only
  the selected arm, and `and` is false as soon as one side is false.

The central fact is `evalK_sound`: whenever `evalK` is defined, every total meaning agrees with it.
So a query that is Kleene-defined at every point has one truth value, whichever division-by-zero
convention a layer uses. `Refines` is the rewrite contract: a rewrite may only make an expression
more defined, never change a defined value.
-/

namespace Symbolic

mutual
/-- Mirrors `SymExprKind`. -/
inductive Expr where
  | const (v : Word)
  | var (n : Nat)
  | not (a : Expr)
  | bin (op : BinOp) (a b : Expr)
  | tern (op : TernOp) (a b m : Expr)
  | ite (c : BExpr) (a b : Expr)

/-- Mirrors `SymBoolExprKind`. -/
inductive BExpr where
  | const (b : Bool)
  | not (a : BExpr)
  | and (a b : BExpr)
  | cmp (op : CmpOp) (a b : Expr)
end

/-- Values of the opaque leaves. -/
abbrev Env := Nat → Word

mutual
/-- Total evaluation under a division-by-zero meaning. -/
def eval (dz : DivZero) (env : Env) : Expr → Word
  | .const v => v
  | .var n => env n
  | .not a => ~~~ (eval dz env a)
  | .bin op a b => op.evalWith dz (eval dz env a) (eval dz env b)
  | .tern op a b m => op.eval (eval dz env a) (eval dz env b) (eval dz env m)
  | .ite c a b => if evalB dz env c then eval dz env a else eval dz env b

/-- Total Boolean evaluation under a division-by-zero meaning. -/
def evalB (dz : DivZero) (env : Env) : BExpr → Bool
  | .const b => b
  | .not a => !(evalB dz env a)
  | .and a b => evalB dz env a && evalB dz env b
  | .cmp op a b => op.eval (eval dz env a) (eval dz env b)
end

/-- Kleene conjunction: false dominates an undefined side. -/
def kand : Option Bool → Option Bool → Option Bool
  | some false, _ => some false
  | _, some false => some false
  | some true, some true => some true
  | _, _ => none

/-- A strict binary operator under the Kleene meaning. -/
def kbin (op : BinOp) : Option Word → Option Word → Option Word
  | some x, some y => if op.isDiv ∧ y = 0 then none else some (op.evm x y)
  | _, _ => none

/-- Kleene `ite`: only the selected arm matters. -/
def kite : Option Bool → Option Word → Option Word → Option Word
  | some true, a, _ => a
  | some false, _, b => b
  | none, _, _ => none

/-- A strict ternary operator under the Kleene meaning. -/
def ktern (op : TernOp) : Option Word → Option Word → Option Word → Option Word
  | some x, some y, some z => some (op.eval x y z)
  | _, _, _ => none

/-- A strict comparison under the Kleene meaning. -/
def kcmp (op : CmpOp) : Option Word → Option Word → Option Bool
  | some x, some y => some (op.eval x y)
  | _, _ => none

mutual
/-- Kleene evaluation: a raw division by zero is undefined. -/
def evalK (env : Env) : Expr → Option Word
  | .const v => some v
  | .var n => some (env n)
  | .not a => (evalK env a).map (~~~ ·)
  | .bin op a b => kbin op (evalK env a) (evalK env b)
  | .tern op a b m => ktern op (evalK env a) (evalK env b) (evalK env m)
  | .ite c a b => kite (evalKB env c) (evalK env a) (evalK env b)

/-- Kleene Boolean evaluation. -/
def evalKB (env : Env) : BExpr → Option Bool
  | .const b => some b
  | .not a => (evalKB env a).map (!·)
  | .and a b => kand (evalKB env a) (evalKB env b)
  | .cmp op a b => kcmp op (evalK env a) (evalK env b)
end

@[simp] theorem kite_true (a b : Option Word) : kite (some true) a b = a := rfl
@[simp] theorem kite_false (a b : Option Word) : kite (some false) a b = b := rfl
@[simp] theorem kite_none (a b : Option Word) : kite none a b = none := rfl
@[simp] theorem kcmp_some_some (op : CmpOp) (x y : Word) :
    kcmp op (some x) (some y) = some (op.eval x y) := rfl
@[simp] theorem ktern_some_some (op : TernOp) (x y z : Word) :
    ktern op (some x) (some y) (some z) = some (op.eval x y z) := rfl
@[simp] theorem kbin_some_some (op : BinOp) (x y : Word) :
    kbin op (some x) (some y) = if op.isDiv ∧ y = 0 then none else some (op.evm x y) := rfl
@[simp] theorem kbin_none_left (op : BinOp) (b : Option Word) : kbin op none b = none := by
  cases b <;> rfl
@[simp] theorem kbin_none_right (op : BinOp) (a : Option Word) : kbin op a none = none := by
  cases a <;> rfl

theorem kbin_some {op : BinOp} {a b : Option Word} {v : Word} (h : kbin op a b = some v) :
    ∃ x y, a = some x ∧ b = some y ∧ ¬(op.isDiv ∧ y = 0) ∧ op.evm x y = v := by
  match a, b, h with
  | some x, some y, h =>
    simp only [kbin] at h
    by_cases hd : op.isDiv ∧ y = 0
    · simp [hd] at h
    · rw [if_neg hd] at h
      exact ⟨x, y, rfl, rfl, hd, Option.some.inj h⟩

theorem kand_some {a b : Option Bool} {v : Bool} (h : kand a b = some v) :
    (v = false ∧ (a = some false ∨ b = some false)) ∨ (v = true ∧ a = some true ∧ b = some true) := by
  match a, b, h with
  | some false, _, h => simp [kand] at h; simp [← h]
  | some true, some false, h => simp [kand] at h; simp [← h]
  | some true, some true, h => simp [kand] at h; simp [← h]
  | none, some false, h => simp [kand] at h; simp [← h]

theorem ktern_some {op : TernOp} {a b m : Option Word} {v : Word}
    (h : ktern op a b m = some v) :
    ∃ x y z, a = some x ∧ b = some y ∧ m = some z ∧ op.eval x y z = v := by
  match a, b, m, h with
  | some x, some y, some z, h => exact ⟨x, y, z, rfl, rfl, rfl, Option.some.inj h⟩

theorem kite_some {c : Option Bool} {a b : Option Word} {v : Word} (h : kite c a b = some v) :
    (c = some true ∧ a = some v) ∨ (c = some false ∧ b = some v) := by
  match c, h with
  | some true, h => exact .inl ⟨rfl, h⟩
  | some false, h => exact .inr ⟨rfl, h⟩

theorem kcmp_some {op : CmpOp} {a b : Option Word} {v : Bool} (h : kcmp op a b = some v) :
    ∃ x y, a = some x ∧ b = some y ∧ op.eval x y = v := by
  match a, b, h with
  | some x, some y, h => exact ⟨x, y, rfl, rfl, Option.some.inj h⟩

mutual
/-- **Agreement.** A defined Kleene value is the value under every division-by-zero meaning. -/
theorem evalK_sound (dz : DivZero) (env : Env) :
    (e : Expr) → (v : Word) → evalK env e = some v → eval dz env e = v
  | .const _, v, h => by simp [evalK] at h; simp [eval, h]
  | .var _, v, h => by simp [evalK] at h; simp [eval, h]
  | .not a, v, h => by
    simp only [evalK, Option.map_eq_some_iff] at h
    obtain ⟨x, hx, rfl⟩ := h
    simp [eval, evalK_sound dz env a x hx]
  | .bin op a b, v, h => by
    simp only [evalK] at h
    obtain ⟨x, y, hx, hy, hdiv, rfl⟩ := kbin_some h
    simp only [eval, evalK_sound dz env a x hx, evalK_sound dz env b y hy, BinOp.evalWith, hdiv,
      if_false]
  | .tern op a b m, v, h => by
    simp only [evalK] at h
    obtain ⟨x, y, z, hx, hy, hz, rfl⟩ := ktern_some h
    simp [eval, evalK_sound dz env a x hx, evalK_sound dz env b y hy, evalK_sound dz env m z hz]
  | .ite c a b, v, h => by
    simp only [evalK] at h
    rcases kite_some h with ⟨hc, h⟩ | ⟨hc, h⟩
    · simp [eval, evalKB_sound dz env c true hc, evalK_sound dz env a v h]
    · simp [eval, evalKB_sound dz env c false hc, evalK_sound dz env b v h]

/-- **Agreement** for Boolean expressions. -/
theorem evalKB_sound (dz : DivZero) (env : Env) :
    (e : BExpr) → (v : Bool) → evalKB env e = some v → evalB dz env e = v
  | .const _, v, h => by simp [evalKB] at h; simp [evalB, h]
  | .not a, v, h => by
    simp only [evalKB, Option.map_eq_some_iff] at h
    obtain ⟨x, hx, rfl⟩ := h
    simp [evalB, evalKB_sound dz env a x hx]
  | .and a b, v, h => by
    simp only [evalKB] at h
    rcases kand_some h with ⟨rfl, ha | hb⟩ | ⟨rfl, ha, hb⟩
    · simp [evalB, evalKB_sound dz env a false ha]
    · simp [evalB, evalKB_sound dz env b false hb]
    · simp [evalB, evalKB_sound dz env a true ha, evalKB_sound dz env b true hb]
  | .cmp op a b, v, h => by
    simp only [evalKB] at h
    obtain ⟨x, y, hx, hy, rfl⟩ := kcmp_some h
    simp [evalB, evalK_sound dz env a x hx, evalK_sound dz env b y hy]
end

/-- A query is defined when its Kleene value exists at every point. -/
def Defined (φ : BExpr) : Prop := ∀ env, (evalKB env φ).isSome

/-- The rewrite contract at one point: wherever `e` is defined, `e'` is defined with the same
value. -/
def RefinesAt (env : Env) (e e' : Expr) : Prop := ∀ v, evalK env e = some v → evalK env e' = some v

/-- `RefinesAt` for Boolean expressions. -/
def RefinesBAt (env : Env) (e e' : BExpr) : Prop :=
  ∀ v, evalKB env e = some v → evalKB env e' = some v

/-- The rewrite contract of a context-free rewrite `e => e'`. -/
def Refines (e e' : Expr) : Prop := ∀ env, RefinesAt env e e'

/-- The rewrite contract of a context-free Boolean rewrite. -/
def RefinesB (e e' : BExpr) : Prop := ∀ env, RefinesBAt env e e'

/-- The contract of a contextual rewrite, justified by facts `ctx` from the other conjuncts. -/
def RefinesUnder (ctx : BExpr) (e e' : Expr) : Prop :=
  ∀ env, evalKB env ctx = some true → RefinesAt env e e'

/-- `RefinesUnder` for Boolean expressions. -/
def RefinesBUnder (ctx : BExpr) (e e' : BExpr) : Prop :=
  ∀ env, evalKB env ctx = some true → RefinesBAt env e e'

infix:50 " ⊑ " => Refines
infix:50 " ⊑ᵇ " => RefinesB

theorem Refines.refl (e : Expr) : e ⊑ e := fun _ _ h => h

theorem Refines.trans {a b c : Expr} (h₁ : a ⊑ b) (h₂ : b ⊑ c) : a ⊑ c :=
  fun env v h => h₂ env v (h₁ env v h)

theorem RefinesB.refl (e : BExpr) : e ⊑ᵇ e := fun _ _ h => h

theorem RefinesB.trans {a b c : BExpr} (h₁ : a ⊑ᵇ b) (h₂ : b ⊑ᵇ c) : a ⊑ᵇ c :=
  fun env v h => h₂ env v (h₁ env v h)

/-! ## Congruence: refinement of a subterm refines the whole expression. -/

section Congruence
variable {env : Env}

theorem RefinesAt.not {a a' : Expr} (h : RefinesAt env a a') :
    RefinesAt env (.not a) (.not a') := by
  intro v hv
  simp only [evalK, Option.map_eq_some_iff] at hv ⊢
  obtain ⟨x, hx, rfl⟩ := hv
  exact ⟨x, h x hx, rfl⟩

theorem RefinesAt.bin {op : BinOp} {a a' b b' : Expr} (ha : RefinesAt env a a')
    (hb : RefinesAt env b b') : RefinesAt env (.bin op a b) (.bin op a' b') := by
  intro v hv
  simp only [evalK] at hv ⊢
  obtain ⟨x, y, hx, hy, _, _⟩ := kbin_some hv
  rw [hx, hy] at hv
  rw [ha x hx, hb y hy]
  exact hv

theorem RefinesAt.tern {op : TernOp} {a a' b b' m m' : Expr} (ha : RefinesAt env a a')
    (hb : RefinesAt env b b') (hm : RefinesAt env m m') :
    RefinesAt env (.tern op a b m) (.tern op a' b' m') := by
  intro v hv
  simp only [evalK] at hv ⊢
  obtain ⟨x, y, z, hx, hy, hz, rfl⟩ := ktern_some hv
  rw [ha x hx, hb y hy, hm z hz]
  rfl

theorem RefinesBAt.ite {c c' : BExpr} {a a' b b' : Expr} (hc : RefinesBAt env c c')
    (ha : RefinesAt env a a') (hb : RefinesAt env b b') :
    RefinesAt env (.ite c a b) (.ite c' a' b') := by
  intro v hv
  simp only [evalK] at hv ⊢
  rcases kite_some hv with ⟨h, hv⟩ | ⟨h, hv⟩
  · rw [hc true h, ha v hv]; rfl
  · rw [hc false h, hb v hv]; rfl

theorem RefinesBAt.not {a a' : BExpr} (h : RefinesBAt env a a') :
    RefinesBAt env (.not a) (.not a') := by
  intro v hv
  simp only [evalKB, Option.map_eq_some_iff] at hv ⊢
  obtain ⟨x, hx, rfl⟩ := hv
  exact ⟨x, h x hx, rfl⟩

theorem RefinesBAt.and {a a' b b' : BExpr} (ha : RefinesBAt env a a') (hb : RefinesBAt env b b') :
    RefinesBAt env (.and a b) (.and a' b') := by
  intro v hv
  simp only [evalKB] at hv ⊢
  rcases kand_some hv with ⟨rfl, h | h⟩ | ⟨rfl, h₁, h₂⟩
  · rw [ha false h]; simp [kand]
  · rw [hb false h]
    cases evalKB env a' with
    | none => rfl
    | some x => cases x <;> rfl
  · rw [ha true h₁, hb true h₂]; simp [kand]

theorem RefinesBAt.cmp {op : CmpOp} {a a' b b' : Expr} (ha : RefinesAt env a a')
    (hb : RefinesAt env b b') : RefinesBAt env (.cmp op a b) (.cmp op a' b') := by
  intro v hv
  simp only [evalKB] at hv ⊢
  obtain ⟨x, y, hx, hy, rfl⟩ := kcmp_some hv
  rw [ha x hx, hb y hy]
  rfl

end Congruence

/-! ## Soundness of an UNSAT answer. -/

/-- **PASS soundness.** If the path query `φ` is Kleene-defined and normalization produced a
refinement `φ'` that the solver reports UNSAT under SMT-LIB semantics, then `φ` is false at
every point under EVM semantics: no concrete execution takes the path. -/
theorem unsat_transfer {φ φ' : BExpr} (hdef : Defined φ) (href : φ ⊑ᵇ φ')
    (hunsat : ∀ env, evalB smtDZ env φ' = false) : ∀ env, evalB evmDZ env φ = false := by
  intro env
  obtain ⟨b, hb⟩ := Option.isSome_iff_exists.mp (hdef env)
  have h' := href env b hb
  rw [evalKB_sound evmDZ env φ b hb]
  rw [← evalKB_sound smtDZ env φ' b h']
  exact hunsat env

/-- The contract of a whole query: every point where the original query is true keeps the
rewritten query true. Refinement implies it, and so does a contextual rewrite. -/
def PreservesTruth (φ φ' : BExpr) : Prop :=
  ∀ env, evalKB env φ = some true → evalKB env φ' = some true

theorem RefinesB.preservesTruth {φ φ' : BExpr} (h : φ ⊑ᵇ φ') : PreservesTruth φ φ' :=
  fun env => h env true

theorem PreservesTruth.trans {a b c : BExpr} (h₁ : PreservesTruth a b) (h₂ : PreservesTruth b c) :
    PreservesTruth a c := fun env h => h₂ env (h₁ env h)

/-- **PASS soundness** with the weakest whole-query contract. -/
theorem unsat_transfer' {φ φ' : BExpr} (hdef : Defined φ) (hpres : PreservesTruth φ φ')
    (hunsat : ∀ env, evalB smtDZ env φ' = false) : ∀ env, evalB evmDZ env φ = false := by
  intro env
  obtain ⟨b, hb⟩ := Option.isSome_iff_exists.mp (hdef env)
  rw [evalKB_sound evmDZ env φ b hb]
  cases b
  · rfl
  · have := evalKB_sound smtDZ env φ' true (hpres env hb)
    rw [hunsat env] at this
    exact absurd this (by decide)

/-- **Contextual rewriting.** A conjunct `φ` rewritten under facts of the whole query keeps
every point where the query is true. This is the shape of `ConstraintContext::normalize_bool`.

The context here is the entire query `ctx ∧ φ`, including `φ` itself. A conjunct that justifies
its own rewrite (the hazard `ConstraintContext::requires_independent_context` and `for_rewrite`
avoid) can only weaken the query, as in `self_justified_rewrite_weakens`: that may produce a
spurious solver model, which replay validation rejects, but never a false UNSAT. -/
theorem refinesBUnder_and {ctx φ φ' : BExpr} (h : RefinesBUnder (.and ctx φ) φ φ') :
    PreservesTruth (BExpr.and ctx φ) (BExpr.and ctx φ') := by
  intro env hv
  have hq := hv
  simp only [evalKB] at hv ⊢
  rcases kand_some hv with ⟨h₁, _⟩ | ⟨_, hc, hφ⟩
  · cases h₁
  · rw [hc, h env hq true hφ]; simp [kand]

/-- Self-justification drops a constraint: `x == 0` justifies `x == 0 => true` under itself, and
the rewritten query is strictly weaker. It loses completeness, not soundness. -/
theorem self_justified_rewrite_weakens :
    ∃ φ : BExpr, RefinesBUnder φ φ (.const true) ∧
      ∃ env, evalKB env φ = some false ∧ evalKB env (.const true) = some true := by
  refine ⟨.cmp .eq (.var 0) (.const 0), ?_, fun _ => 1, ?_, rfl⟩
  · intro env hc v hv
    rw [hc] at hv
    simp_all [evalKB]
  · simp [evalKB, evalK, CmpOp.eval, kcmp]

/-- Model validation agrees with the solver: at a point where the query is defined, the EVM
evaluator used by `eval_model_constraints` and the SMT meaning give the same verdict. -/
theorem evm_eq_smt_of_defined {φ : BExpr} {env : Env} (h : (evalKB env φ).isSome) :
    evalB evmDZ env φ = evalB smtDZ env φ := by
  obtain ⟨b, hb⟩ := Option.isSome_iff_exists.mp h
  rw [evalKB_sound evmDZ env φ b hb, evalKB_sound smtDZ env φ b hb]

end Symbolic
