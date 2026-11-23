/-!
# EVM words and operator semantics

Models `SymBinOp`, `SymTernOp` and `SymCmpOp` from
`crates/evm/symbolic/src/runtime/expr/expr.rs` and `bool.rs` over `BitVec 256`.

Two meanings of the division operators coexist in the Rust code:

* `SymBinOp::eval` uses EVM semantics: `x / 0 = x % 0 = 0`, for signed operators too.
* `SymBinOp::smt` emits SMT-LIB `bvudiv`, `bvurem`, `bvsdiv`, `bvsrem`, where `x / 0` is all ones
  (or one for a negative signed dividend) and `x % 0 = x`.

Both agree whenever the divisor is nonzero (`BinOp.smt_eq_evm`). `DivZero` names the value a
raw division takes at a zero divisor, so one evaluator covers both meanings.
-/

namespace Symbolic

/-- An EVM word. -/
abbrev Word := BitVec 256

/-- Mirrors `SymBinOp`. -/
inductive BinOp where
  | add | sub | mul | udiv | urem | sdiv | srem | and | or | xor | shl | shr | sar | mulHigh
  deriving DecidableEq, Repr

/-- Mirrors `SymTernOp`. -/
inductive TernOp where
  | addMod | mulMod | mulDiv
  deriving DecidableEq, Repr

/-- Mirrors `SymCmpOp`. -/
inductive CmpOp where
  | eq | ult | ugt | ule | uge | slt | sgt
  deriving DecidableEq, Repr

namespace BinOp

/-- The operators whose EVM and SMT meanings differ at a zero divisor. -/
def isDiv : BinOp → Bool
  | udiv | urem | sdiv | srem => true
  | _ => false

/-- The high word of the 512-bit product, as `SymBinOp::MulHigh`. -/
def mulHighWord (a b : Word) : Word :=
  BitVec.ofNat 256 ((a.toNat * b.toNat) / 2 ^ 256)

/-- EVM meaning, mirroring `SymBinOp::eval`. Lean's `/` and `sdiv` already return zero for a zero
divisor; `%` and `srem` return the dividend, so those two are guarded. -/
def evm : BinOp → Word → Word → Word
  | add, a, b => a + b
  | sub, a, b => a - b
  | mul, a, b => a * b
  | udiv, a, b => a / b
  | urem, a, b => if b = 0 then 0 else a % b
  | sdiv, a, b => a.sdiv b
  | srem, a, b => if b = 0 then 0 else a.srem b
  | and, a, b => a &&& b
  | or, a, b => a ||| b
  | xor, a, b => a ^^^ b
  | shl, a, b => a <<< b
  | shr, a, b => a >>> b
  | sar, a, b => a.sshiftRight' b
  | mulHigh, a, b => mulHighWord a b

/-- SMT-LIB meaning, mirroring `SymBinOp::smt` and the `MulHigh` case of
`SmtCseWriter::write_expr`. -/
def smt : BinOp → Word → Word → Word
  | udiv, a, b => a.smtUDiv b
  | urem, a, b => a % b
  | sdiv, a, b => a.smtSDiv b
  | srem, a, b => a.srem b
  | op, a, b => evm op a b

end BinOp

/-- The value a raw division operator takes when its divisor is zero, as a function of the
operator and the dividend. -/
abbrev DivZero := BinOp → Word → Word

/-- `SymBinOp::eval`: every division by zero is zero. -/
def evmDZ : DivZero := fun _ _ => 0

/-- SMT-LIB: `bvudiv x 0 = ~0`, `bvurem x 0 = x`, `bvsdiv x 0 = (x <s 0 ? 1 : ~0)`,
`bvsrem x 0 = x`. -/
def smtDZ : DivZero := fun op a =>
  match op with
  | .udiv => BitVec.allOnes 256
  | .sdiv => a.smtSDiv 0
  | _ => a

/-- A binary operator under a chosen division-by-zero meaning. -/
def BinOp.evalWith (dz : DivZero) (op : BinOp) (a b : Word) : Word :=
  if op.isDiv ∧ b = 0 then dz op a else op.evm a b

/-- Mirrors `SymTernOp::eval`: the 512-bit intermediate, and zero for a zero modulus. -/
def TernOp.eval (op : TernOp) (a b m : Word) : Word :=
  if m = 0 then 0 else
    match op with
    | addMod => BitVec.ofNat 256 ((a.toNat + b.toNat) % m.toNat)
    | mulMod => BitVec.ofNat 256 ((a.toNat * b.toNat) % m.toNat)
    | mulDiv => BitVec.ofNat 256 ((a.toNat * b.toNat) / m.toNat)

/-- Mirrors `SymCmpOp::eval` and `SymCmpOp::smt`, which agree. -/
def CmpOp.eval : CmpOp → Word → Word → Bool
  | eq, a, b => decide (a = b)
  | ult, a, b => a.ult b
  | ugt, a, b => b.ult a
  | ule, a, b => a.ule b
  | uge, a, b => b.ule a
  | slt, a, b => a.slt b
  | sgt, a, b => b.slt a

theorem BinOp.evalWith_evm (op : BinOp) (a b : Word) : op.evalWith evmDZ a b = op.evm a b := by
  cases op <;> by_cases h : b = 0 <;> simp_all [evalWith, evm, isDiv, evmDZ]

theorem smtSDiv_ne_zero (a b : Word) (h : b ≠ 0) : a.smtSDiv b = a.sdiv b := by
  have hn : -b ≠ 0 := by
    intro hb; apply h; have := congrArg (fun x => -x) hb; simpa using this
  have hn' : BitVec.neg b ≠ 0 := hn
  simp only [BitVec.smtSDiv, BitVec.sdiv, BitVec.smtUDiv, h, hn', if_false]

/-- The SMT emission of every operator is `evalWith smtDZ`. -/
theorem BinOp.evalWith_smt (op : BinOp) (a b : Word) : op.evalWith smtDZ a b = op.smt a b := by
  cases op <;> by_cases h : b = 0 <;>
    simp_all [evalWith, evm, smt, isDiv, smtDZ, BitVec.smtUDiv, smtSDiv_ne_zero]

/-- Away from a zero divisor the two meanings agree. -/
theorem BinOp.evalWith_ne_zero (dz₁ dz₂ : DivZero) (op : BinOp) (a b : Word) (h : b ≠ 0) :
    op.evalWith dz₁ a b = op.evalWith dz₂ a b := by
  simp only [evalWith, h, and_false, if_false]

/-- Non-division operators ignore the division-by-zero meaning. -/
theorem BinOp.evalWith_not_div (dz : DivZero) (op : BinOp) (a b : Word) (h : op.isDiv = false) :
    op.evalWith dz a b = op.evm a b := by
  simp [evalWith, h]

end Symbolic
