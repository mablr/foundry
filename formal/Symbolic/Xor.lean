import Symbolic.Expr
import Std.Tactic.BVDecide

/-!
# The XOR contradiction certificate

Models `check_certificate` and `affine_terms` in
`crates/evm/symbolic/src/runtime/solver/reasoning/xor.rs`. `checked_xor_result` returns UNSAT
without asking the solver whenever Gaussian elimination proposes a certificate that
`check_certificate` accepts, so the checker alone carries that UNSAT verdict.

A relation is one asserted `left == right` or `left != right` conjunct. `affine_terms` peels a
constant offset (`word + c == t`, `word - c == t`, `c - word == t`) or splits `a - b == 0`, and the
checker flattens XOR and NOT trees into opaque atoms plus a constant. A certificate is accepted
when the atoms of the selected relations cancel pairwise and the constants add up to a
contradiction. `check_certificate_sound` proves that an accepted certificate admits no point that
satisfies every relation, under any division-by-zero meaning.

Abstractions: `Expr` atoms are compared structurally, as hash-consed `SymExpr` pointers are; the
Rust visit budget can only make the checker reject, so it is omitted; the selection bitmask is a
predicate on relation indices, with the out-of-range check expressed by `sel` only ever being
consulted at valid indices.
-/

namespace Symbolic

/-! Word identities over `BitVec 256`, so `bv_decide` sees the bit-vector type. -/

theorem BitVec.add_eq_iff (x v t : BitVec 256) : x + v = t ↔ x = t - v := by bv_decide
theorem BitVec.sub_eq_iff (x v t : BitVec 256) : x - v = t ↔ x = t + v := by bv_decide
theorem BitVec.const_sub_iff (x v t : BitVec 256) : v - x = t ↔ x = v - t := by bv_decide
theorem BitVec.sub_eq_zero_iff' (x y : BitVec 256) : x - y = 0 ↔ x = y := by bv_decide
theorem w_xor_zero (x : Word) : x ^^^ 0 = x := by simp
theorem w_zero_xor (x : Word) : 0 ^^^ x = x := by simp
theorem w_xor_eq_zero_iff (x y : BitVec 256) : x ^^^ y = 0 ↔ x = y := by bv_decide

mutual
/-- Structural equality on words, standing in for hash-consed pointer equality. -/
def Expr.beq : Expr → Expr → Bool
  | .const a, .const b => a == b
  | .var a, .var b => a == b
  | .not a, .not b => a.beq b
  | .bin o a b, .bin o' a' b' => o == o' && a.beq a' && b.beq b'
  | .tern o a b m, .tern o' a' b' m' => o == o' && a.beq a' && b.beq b' && m.beq m'
  | .ite c a b, .ite c' a' b' => c.beq c' && a.beq a' && b.beq b'
  | _, _ => false

/-- Structural equality on Boolean expressions. -/
def BExpr.beq : BExpr → BExpr → Bool
  | .const a, .const b => a == b
  | .not a, .not b => a.beq b
  | .and a b, .and a' b' => a.beq a' && b.beq b'
  | .cmp o a b, .cmp o' a' b' => o == o' && a.beq a' && b.beq b'
  | _, _ => false
end

mutual
theorem Expr.eq_of_beq : (a b : Expr) → a.beq b = true → a = b
  | .const _, .const _, h => by simp [Expr.beq] at h; rw [h]
  | .var _, .var _, h => by simp [Expr.beq] at h; rw [h]
  | .not a, .not b, h => by simp only [Expr.beq] at h; rw [Expr.eq_of_beq a b h]
  | .bin _ a b, .bin _ a' b', h => by
    simp only [Expr.beq, Bool.and_eq_true, beq_iff_eq] at h
    obtain ⟨⟨rfl, ha⟩, hb⟩ := h
    rw [Expr.eq_of_beq a a' ha, Expr.eq_of_beq b b' hb]
  | .tern _ a b m, .tern _ a' b' m', h => by
    simp only [Expr.beq, Bool.and_eq_true, beq_iff_eq] at h
    obtain ⟨⟨⟨rfl, ha⟩, hb⟩, hm⟩ := h
    rw [Expr.eq_of_beq a a' ha, Expr.eq_of_beq b b' hb, Expr.eq_of_beq m m' hm]
  | .ite c a b, .ite c' a' b', h => by
    simp only [Expr.beq, Bool.and_eq_true] at h
    obtain ⟨⟨hc, ha⟩, hb⟩ := h
    rw [BExpr.eq_of_beq c c' hc, Expr.eq_of_beq a a' ha, Expr.eq_of_beq b b' hb]
  | .const _, .var _, h | .const _, .not _, h | .const _, .bin .., h | .const _, .tern .., h
  | .const _, .ite .., h | .var _, .const _, h | .var _, .not _, h | .var _, .bin .., h
  | .var _, .tern .., h | .var _, .ite .., h | .not _, .const _, h | .not _, .var _, h
  | .not _, .bin .., h | .not _, .tern .., h | .not _, .ite .., h | .bin .., .const _, h
  | .bin .., .var _, h | .bin .., .not _, h | .bin .., .tern .., h | .bin .., .ite .., h
  | .tern .., .const _, h | .tern .., .var _, h | .tern .., .not _, h | .tern .., .bin .., h
  | .tern .., .ite .., h | .ite .., .const _, h | .ite .., .var _, h | .ite .., .not _, h
  | .ite .., .bin .., h | .ite .., .tern .., h => by simp [Expr.beq] at h

theorem BExpr.eq_of_beq : (a b : BExpr) → a.beq b = true → a = b
  | .const _, .const _, h => by simp [BExpr.beq] at h; rw [h]
  | .not a, .not b, h => by simp only [BExpr.beq] at h; rw [BExpr.eq_of_beq a b h]
  | .and a b, .and a' b', h => by
    simp only [BExpr.beq, Bool.and_eq_true] at h
    rw [BExpr.eq_of_beq a a' h.1, BExpr.eq_of_beq b b' h.2]
  | .cmp _ a b, .cmp _ a' b', h => by
    simp only [BExpr.beq, Bool.and_eq_true, beq_iff_eq] at h
    obtain ⟨⟨rfl, ha⟩, hb⟩ := h
    rw [Expr.eq_of_beq a a' ha, Expr.eq_of_beq b b' hb]
  | .const _, .not _, h | .const _, .and .., h | .const _, .cmp .., h | .not _, .const _, h
  | .not _, .and .., h | .not _, .cmp .., h | .and .., .const _, h | .and .., .not _, h
  | .and .., .cmp .., h | .cmp .., .const _, h | .cmp .., .not _, h | .cmp .., .and .., h => by
    simp [BExpr.beq] at h
end

/-- One asserted equality or disequality (`Relation`). -/
structure Relation where
  left : Expr
  right : Expr
  negated : Bool

/-- Whether a relation holds at a point. -/
def Relation.holds (dz : DivZero) (env : Env) (r : Relation) : Prop :=
  (eval dz env r.left = eval dz env r.right) ↔ r.negated = false

/-- The peeling loop of `affine_terms`, at most `fuel` steps (32 in Rust). It returns the words
whose XOR the relation constrains and the target constant. -/
def affineLoop : Nat → Expr → Word → List Expr × Word
  | 0, word, target => ([word], target)
  | fuel + 1, word, target =>
    match word with
    | .bin .add left (.const value) => affineLoop fuel left (target - value)
    | .bin .sub left (.const value) => affineLoop fuel left (target + value)
    | .bin .sub (.const value) right => affineLoop fuel right (value - target)
    | .bin .sub left right => if target = 0 then ([left, right], 0) else ([word], target)
    | _ => ([word], target)

/-- `affine_terms`. -/
def affineTerms (r : Relation) : List Expr × Word :=
  match r.right with
  | .const target => affineLoop 32 r.left target
  | _ => ([r.left, r.right], 0)

/-- XOR of a list of words. -/
def xorAll : List Word → Word
  | [] => 0
  | w :: ws => w ^^^ xorAll ws

/-- Toggles `t` in a duplicate-free list: the `HashSet` insert-or-remove of `check_certificate`. -/
def toggle (t : Expr) : List Expr → List Expr
  | [] => [t]
  | x :: xs => if x.beq t then xs else x :: toggle t xs

/-- Flattens one word into the toggled atom set and constant, as the `pending` loop does. -/
def flatten : Expr → List Expr × Word → List Expr × Word
  | .const v, (ts, c) => (ts, c ^^^ v)
  | .bin .xor a b, acc => flatten b (flatten a acc)
  | .not a, (ts, c) => flatten a (ts, c ^^^ BitVec.allOnes 256)
  | e, (ts, c) => (toggle e ts, c)

/-- Flattens every word of one relation and adds its offset. -/
def flattenRelation (r : Relation) (acc : List Expr × Word) : List Expr × Word :=
  (affineTerms r).1.foldl (fun acc w => flatten w acc) (acc.1, acc.2 ^^^ (affineTerms r).2)

/-- The relations a certificate uses: the selected equalities and the optional disequality. -/
def used (sel : Nat → Bool) (diseq : Option Nat) (i : Nat) : Bool := sel i || diseq == some i

/-- `check_certificate`. -/
def checkCertificate (rels : List Relation) (sel : Nat → Bool) (diseq : Option Nat) : Bool :=
  let validDiseq := match diseq with
    | none => true
    | some j => match rels[j]? with
      | some r => r.negated
      | none => false
  let selectedEqualities := (List.range rels.length).all fun i =>
    !(sel i && (rels[i]?.map (·.negated)).getD false)
  let acc := (List.range rels.length).foldl (fun acc i =>
    if used sel diseq i then
      match rels[i]? with
      | some r => flattenRelation r acc
      | none => acc
    else acc) ([], 0)
  validDiseq && selectedEqualities && acc.1.isEmpty && ((acc.2 == 0) == diseq.isSome)

/-! ## Soundness -/

section
variable (dz : DivZero) (env : Env)

/-- The value of an atom set and constant. -/
def accValue (acc : List Expr × Word) : Word := xorAll (acc.1.map (eval dz env)) ^^^ acc.2

theorem xorAll_toggle (t : Expr) (ts : List Expr) :
    xorAll ((toggle t ts).map (eval dz env)) = eval dz env t ^^^ xorAll (ts.map (eval dz env)) := by
  induction ts with
  | nil => simp [toggle, xorAll]
  | cons x xs ih =>
    simp only [toggle]
    split
    · rename_i h
      rw [Expr.eq_of_beq x t h]
      simp only [List.map_cons, xorAll]
      rw [← BitVec.xor_assoc, BitVec.xor_self]; simp
    · simp only [List.map_cons, xorAll, ih]
      rw [← BitVec.xor_assoc, BitVec.xor_comm (eval dz env x), BitVec.xor_assoc]

theorem accValue_flatten : (e : Expr) → (acc : List Expr × Word) →
    accValue dz env (flatten e acc) = eval dz env e ^^^ accValue dz env acc
  | .const v, (ts, c) => by
    simp only [flatten, accValue, eval]
    ac_rfl
  | .bin .xor a b, acc => by
    rw [flatten, accValue_flatten b, accValue_flatten a]
    simp only [eval, BinOp.evalWith, BinOp.isDiv, Bool.false_eq_true, false_and, if_false,
      BinOp.evm]
    ac_rfl
  | .not a, (ts, c) => by
    rw [flatten, accValue_flatten a]
    simp only [accValue, eval]
    rw [show ~~~eval dz env a = BitVec.allOnes 256 ^^^ eval dz env a from
      BitVec.allOnes_xor.symm]
    ac_rfl
  | .var _, (ts, c) => by simp only [flatten, accValue, xorAll_toggle]; ac_rfl
  | .tern .., (ts, c) => by simp only [flatten, accValue, xorAll_toggle]; ac_rfl
  | .ite .., (ts, c) => by simp only [flatten, accValue, xorAll_toggle]; ac_rfl
  | .bin .add .., (ts, c) | .bin .sub .., (ts, c) | .bin .mul .., (ts, c)
  | .bin .udiv .., (ts, c) | .bin .urem .., (ts, c) | .bin .sdiv .., (ts, c)
  | .bin .srem .., (ts, c) | .bin .and .., (ts, c) | .bin .or .., (ts, c)
  | .bin .shl .., (ts, c) | .bin .shr .., (ts, c) | .bin .sar .., (ts, c)
  | .bin .mulHigh .., (ts, c) => by simp only [flatten, accValue, xorAll_toggle]; ac_rfl

theorem accValue_foldl (ws : List Expr) (acc : List Expr × Word) :
    accValue dz env (ws.foldl (fun acc w => flatten w acc) acc) =
      xorAll (ws.map (eval dz env)) ^^^ accValue dz env acc := by
  induction ws generalizing acc with
  | nil => simp [xorAll]
  | cons w ws ih =>
    simp only [List.foldl_cons, ih, accValue_flatten, List.map_cons, xorAll]
    ac_rfl

/-- The value a relation contributes: zero exactly when the equality holds. -/
def relationValue (r : Relation) : Word :=
  xorAll ((affineTerms r).1.map (eval dz env)) ^^^ (affineTerms r).2

theorem accValue_flattenRelation (r : Relation) (acc : List Expr × Word) :
    accValue dz env (flattenRelation r acc) = relationValue dz env r ^^^ accValue dz env acc := by
  simp only [flattenRelation, relationValue]
  rw [accValue_foldl]
  simp only [accValue]
  ac_rfl

theorem affineLoop_sound (fuel : Nat) (word : Expr) (target : Word) :
    (eval dz env word = target) ↔
      (xorAll ((affineLoop fuel word target).1.map (eval dz env)) ^^^
        (affineLoop fuel word target).2 = 0) := by
  induction fuel generalizing word target with
  | zero => simp [affineLoop, xorAll, w_xor_eq_zero_iff]
  | succ fuel ih =>
    have base : (eval dz env word = target) ↔
        (xorAll (([word] : List Expr).map (eval dz env)) ^^^ target = 0) := by
      simp [xorAll, w_xor_eq_zero_iff]
    unfold affineLoop
    split
    · rename_i left value
      rw [← ih]
      simp only [eval, BinOp.evalWith, BinOp.isDiv, Bool.false_eq_true, false_and, if_false,
        BinOp.evm]
      exact BitVec.add_eq_iff _ _ _
    · rename_i left value
      rw [← ih]
      simp only [eval, BinOp.evalWith, BinOp.isDiv, Bool.false_eq_true, false_and, if_false,
        BinOp.evm]
      exact BitVec.sub_eq_iff _ _ _
    · rename_i value right _
      rw [← ih]
      simp only [eval, BinOp.evalWith, BinOp.isDiv, Bool.false_eq_true, false_and, if_false,
        BinOp.evm]
      exact BitVec.const_sub_iff _ _ _
    · rename_i left right _ _
      split
      · rename_i ht
        subst ht
        simp only [eval, BinOp.evalWith, BinOp.isDiv, Bool.false_eq_true, false_and, if_false,
          BinOp.evm, List.map_cons, List.map_nil, xorAll, w_xor_zero]
        rw [BitVec.sub_eq_zero_iff', w_xor_eq_zero_iff]
      · exact base
    · exact base

theorem relationValue_eq_zero (r : Relation) :
    (eval dz env r.left = eval dz env r.right) ↔ relationValue dz env r = 0 := by
  unfold relationValue affineTerms
  split
  · rename_i target h
    rw [h]
    exact affineLoop_sound dz env 32 r.left target
  · simp only [List.map_cons, List.map_nil, xorAll, w_xor_zero]
    exact (w_xor_eq_zero_iff _ _).symm

theorem foldl_acc (rels : List Relation) (sel : Nat → Bool) (diseq : Option Nat)
    (hrel : ∀ i r, rels[i]? = some r → used sel diseq i = true → diseq ≠ some i →
      relationValue dz env r = 0) (n : Nat) :
    accValue dz env ((List.range n).foldl (fun acc i =>
      if used sel diseq i then
        match rels[i]? with
        | some r => flattenRelation r acc
        | none => acc
      else acc) ([], 0)) =
      (match diseq with
        | some j => if j < n then (match rels[j]? with
          | some r => relationValue dz env r
          | none => 0) else 0
        | none => 0) := by
  induction n with
  | zero => cases diseq <;> simp [accValue, xorAll]
  | succ n ih =>
    rw [List.range_succ, List.foldl_append, List.foldl_cons, List.foldl_nil]
    split
    · rename_i hu
      cases hr : rels[n]? with
      | none =>
        simp only []
        rw [ih]
        cases diseq with
        | none => rfl
        | some j =>
          simp only []
          by_cases hj : j = n
          · subst hj; simp [hr]
          · have : j < n ↔ j < n + 1 := by omega
            by_cases hjn : j < n
            · simp [hjn, show j < n + 1 by omega]
            · simp [hjn, show ¬ j < n + 1 by omega]
      | some r =>
        simp only []
        rw [accValue_flattenRelation, ih]
        cases diseq with
        | none =>
          simp only []
          rw [hrel n r hr hu (by simp), w_zero_xor]
        | some j =>
          simp only []
          by_cases hj : j = n
          · subst hj
            simp [hr]
          · rw [hrel n r hr hu (by simp [Ne.symm hj] ; omega), w_zero_xor]
            by_cases hjn : j < n
            · simp [hjn, show j < n + 1 by omega]
            · simp [hjn, show ¬ j < n + 1 by omega]
    · rename_i hu
      rw [ih]
      cases diseq with
      | none => rfl
      | some j =>
        simp only []
        by_cases hj : j = n
        · subst hj; simp [used] at hu
        · by_cases hjn : j < n
          · simp [hjn, show j < n + 1 by omega]
          · simp [hjn, show ¬ j < n + 1 by omega]

/-- **XOR certificate soundness.** If `check_certificate` accepts, no point satisfies every
relation. -/
theorem check_certificate_sound (rels : List Relation) (sel : Nat → Bool) (diseq : Option Nat)
    (hcheck : checkCertificate rels sel diseq = true) :
    ¬ ∀ r ∈ rels, r.holds dz env := by
  intro hall
  simp only [checkCertificate, Bool.and_eq_true, beq_iff_eq] at hcheck
  obtain ⟨⟨⟨hvalid, hsel⟩, hempty⟩, hconst⟩ := hcheck
  have hrel : ∀ i r, rels[i]? = some r → used sel diseq i = true → diseq ≠ some i →
      relationValue dz env r = 0 := by
    intro i r hr hu hne
    have hi : i < rels.length := (List.getElem?_eq_some_iff.mp hr).1
    have hsi : sel i = true := by
      simp only [used, Bool.or_eq_true, beq_iff_eq] at hu
      exact hu.resolve_right hne
    have hneg : r.negated = false := by
      have := List.all_eq_true.mp hsel i (List.mem_range.mpr hi)
      simp [hsi, hr] at this
      exact this
    have hh := hall r (List.mem_of_getElem? hr)
    simp only [Relation.holds, hneg, iff_true] at hh
    exact (relationValue_eq_zero dz env r).mp hh
  have hfold := foldl_acc dz env rels sel diseq hrel rels.length
  have hnil := List.isEmpty_iff.mp hempty
  simp only [accValue, hnil, List.map_nil, xorAll, w_zero_xor] at hfold
  cases diseq with
  | none =>
    simp only [Option.isSome_none, beq_iff_eq] at hconst
    simp only [] at hfold
    exact absurd hfold (by simpa using hconst)
  | some j =>
    simp only [Option.isSome_some, beq_iff_eq] at hconst
    cases hr : rels[j]? with
    | none => simp [hr] at hvalid
    | some r =>
      simp only [hr] at hvalid
      have hj : j < rels.length := (List.getElem?_eq_some_iff.mp hr).1
      simp only [hj, if_true, hr] at hfold
      have hh := hall r (List.mem_of_getElem? hr)
      simp only [Relation.holds, hvalid] at hh
      have hne : eval dz env r.left ≠ eval dz env r.right := fun h => by simpa using hh.mp h
      apply hne
      apply (relationValue_eq_zero dz env r).mpr
      rw [← hfold]
      simpa using hconst
end

end Symbolic
