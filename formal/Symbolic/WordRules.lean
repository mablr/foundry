import Symbolic.Rewrites

/-!
# Construction rewrites of `SymExpr::binop`: `Sub` through `MulHigh`

Continues `Symbolic.Rewrites`. Each theorem is named after the operator and shape of one match arm
of `SymExpr::binop` in `crates/evm/symbolic/src/runtime/expr/expr.rs` and quotes its comment.
-/

namespace Symbolic

/-! ## Word identities

Stated over `BitVec 256` rather than `Word` so `bv_decide` sees the bit-vector type. -/

theorem bv_add_sub_left (y z : BitVec 256) : z = y + z - y := by bv_decide
theorem bv_add_sub_right (x y : BitVec 256) : x = x + y - y := by bv_decide
theorem bv_sub_sub_cancel (x z : BitVec 256) : z = x - (x - z) := by bv_decide
theorem bv_sub_add_cancel (x y : BitVec 256) : x = x - y + y := by bv_decide
theorem bv_add_sub_cancel (q r : BitVec 256) : q + r - r = q := by bv_decide
theorem bv_and_allOnes (x : BitVec 256) : x &&& BitVec.allOnes 256 = x := by bv_decide
theorem bv_allOnes_or (y : BitVec 256) : BitVec.allOnes 256 = BitVec.allOnes 256 ||| y := by
  bv_decide
theorem bv_xor_cancel_left (x z : BitVec 256) : z = x ^^^ (x ^^^ z) := by bv_decide
theorem bv_xor_cancel_right (x z : BitVec 256) : z = x ^^^ (z ^^^ x) := by bv_decide
theorem bv_xor_select (x z : BitVec 256) : z = x ^^^ ((x ^^^ z) * 1) := by bv_decide

/-! ## Single-operand rules

Most arms rewrite `op(a, k)` for a constant `k` to `a`, a constant, or another operator on `a`.
`unary_rule` reduces such an arm to the word identity it relies on. -/

/-- A binary node with a constant right operand refines `r` when `r` evaluates to the node's value
wherever the node is defined. -/
theorem unary_rule {op : BinOp} {k : Word} {a r : Expr}
    (h : ∀ env x, evalK env a = some x → ¬(op.isDiv ∧ k = 0) → evalK env r = some (op.evm x k)) :
    Expr.bin op a (.const k) ⊑ r := by
  intro env v hv
  obtain ⟨x, y, hx, hy, hd, rfl⟩ := bin_defined hv
  simp at hy; subst hy
  exact h env x hx hd

/-- The mirror of `unary_rule` for a constant left operand. -/
theorem unary_rule_left {op : BinOp} {k : Word} {a r : Expr}
    (h : ∀ env y, evalK env a = some y → ¬(op.isDiv ∧ y = 0) → evalK env r = some (op.evm k y)) :
    Expr.bin op (.const k) a ⊑ r := by
  intro env v hv
  obtain ⟨x, y, hx, hy, hd, rfl⟩ := bin_defined hv
  simp at hx; subst hx
  exact h env y hy hd

/-- A rule whose result is the operand itself. -/
theorem self_rule {op : BinOp} {k : Word} {a : Expr} (h : ∀ x, op.evm x k = x) :
    Expr.bin op a (.const k) ⊑ a :=
  unary_rule fun _ x hx _ => by rw [h x]; exact hx

/-- A rule whose result is a constant. -/
theorem const_rule {op : BinOp} {k c : Word} {a : Expr}
    (h : ∀ x, ¬(op.isDiv ∧ k = 0) → op.evm x k = c) : Expr.bin op a (.const k) ⊑ .const c :=
  unary_rule fun _ x _ hd => by rw [h x hd]; simp

/-- A binary node whose operands are the same subexpression. -/
theorem self_pair_rule {op : BinOp} {a r : Expr}
    (h : ∀ env x, evalK env a = some x → ¬(op.isDiv ∧ x = 0) → evalK env r = some (op.evm x x)) :
    Expr.bin op a a ⊑ r := by
  intro env v hv
  obtain ⟨x, y, hx, hy, hd, rfl⟩ := bin_defined hv
  have : y = x := Option.some.inj (hy.symm.trans hx)
  subst this
  exact h env y hx hd

/-! ## `SymBinOp::Sub` -/

/-- `a - 0 => a`. -/
theorem rw_sub_zero (a : Expr) : Expr.bin .sub a (.const 0) ⊑ a :=
  self_rule fun x => by simp [BinOp.evm]

/-- `a - a => 0`. -/
theorem rw_sub_self (a : Expr) : Expr.bin .sub a a ⊑ .const 0 :=
  self_pair_rule fun _ x _ _ => by simp [BinOp.evm]

/-- `(a + b) - a => b`. -/
theorem rw_sub_add_left (a b : Expr) : Expr.bin .sub (.bin .add a b) a ⊑ b := by
  intro env v hv
  obtain ⟨s, y, hs, hy, _, rfl⟩ := bin_defined hv
  obtain ⟨x, z, hx, hz, _, rfl⟩ := bin_defined hs
  have : y = x := Option.some.inj (hy.symm.trans hx)
  subst this; rw [hz]; simp only [BinOp.evm, Option.some.injEq]; exact bv_add_sub_left _ _

/-- `(a + b) - b => a`. -/
theorem rw_sub_add_right (a b : Expr) : Expr.bin .sub (.bin .add a b) b ⊑ a := by
  intro env v hv
  obtain ⟨s, y, hs, hy, _, rfl⟩ := bin_defined hv
  obtain ⟨x, z, hx, hz, _, rfl⟩ := bin_defined hs
  have : y = z := Option.some.inj (hy.symm.trans hz)
  subst this; rw [hx]; simp only [BinOp.evm, Option.some.injEq]; exact bv_add_sub_right _ _

/-- `a - (a - b) => b`. -/
theorem rw_sub_sub_cancel (a b : Expr) : Expr.bin .sub a (.bin .sub a b) ⊑ b := by
  intro env v hv
  obtain ⟨x, s, hx, hs, _, rfl⟩ := bin_defined hv
  obtain ⟨x', z, hx', hz, _, rfl⟩ := bin_defined hs
  have : x' = x := Option.some.inj (hx'.symm.trans hx)
  subst this; rw [hz]; simp only [BinOp.evm, Option.some.injEq]; exact bv_sub_sub_cancel _ _

/-- `(a - b) + b => a` (`cancelled_difference_minuend`). -/
theorem rw_add_sub_cancel (a b : Expr) : Expr.bin .add (.bin .sub a b) b ⊑ a := by
  intro env v hv
  obtain ⟨s, y, hs, hy, _, rfl⟩ := bin_defined hv
  obtain ⟨x, z, hx, hz, _, rfl⟩ := bin_defined hs
  have : y = z := Option.some.inj (hy.symm.trans hz)
  subst this; rw [hx]; simp only [BinOp.evm, Option.some.injEq]; exact bv_sub_add_cancel _ _

/-- `a - a % d => (a / d) * d` (raw `URem`). Like the Euclidean sum, valid for every `d` under
SMT-LIB semantics and only for `d != 0` under EVM semantics; a refinement either way. -/
theorem rw_sub_urem (a d : Expr) :
    Expr.bin .sub a (.bin .urem a d) ⊑ .bin .mul (.bin .udiv a d) d := by
  intro env v hv
  obtain ⟨x, r, hx, hr, _, rfl⟩ := bin_defined hv
  obtain ⟨x', e, hx', he, hd, rfl⟩ := bin_defined hr
  have : x' = x := Option.some.inj (hx'.symm.trans hx)
  subst this
  have hd0 : ¬ e = 0 := fun h => hd ⟨rfl, h⟩
  have hq := bin_of_defined (op := .udiv) hx' he hd
  rw [bin_of_defined hq he (by simp [BinOp.isDiv])]
  congr 1
  simp only [BinOp.evm]
  rw [if_neg hd0]
  have := udiv_mul_add_umod x' e
  generalize x' / e * e = q at this ⊢
  generalize x' % e = r at this ⊢
  subst this
  exact (bv_add_sub_cancel _ _).symm

/-- `bool_word(c) - 1 => ite(c, 0, MAX)`. -/
theorem rw_sub_bool_word_one (c : BExpr) :
    Expr.bin .sub (boolWord c) (.const 1) ⊑ .ite c (.const 0) (.const maxWord) := by
  intro env v hv
  obtain ⟨x, y, hx, hy, _, rfl⟩ := bin_defined hv
  simp at hy; subst hy
  rcases boolWord_defined hx with ⟨hc, rfl⟩ | ⟨hc, rfl⟩
  · rw [ite_of_true hc]; simp [BinOp.evm]
  · rw [ite_of_false hc]; simp [BinOp.evm, maxWord]

/-- `k - bool_word(c) => ite(c, k - 1, k)`. -/
theorem rw_sub_const_bool_word (c : BExpr) (k : Word) :
    Expr.bin .sub (.const k) (boolWord c) ⊑ .ite c (.const (k - 1)) (.const k) := by
  intro env v hv
  obtain ⟨x, y, hx, hy, _, rfl⟩ := bin_defined hv
  simp at hx; subst hx
  rcases boolWord_defined hy with ⟨hc, rfl⟩ | ⟨hc, rfl⟩
  · rw [ite_of_true hc]; simp [BinOp.evm]
  · rw [ite_of_false hc]; simp [BinOp.evm]

/-! ## `SymBinOp::Mul` -/

/-- `0 * a => 0`. -/
theorem rw_mul_zero_left (a : Expr) : Expr.bin .mul (.const 0) a ⊑ .const 0 :=
  unary_rule_left fun _ y _ _ => by simp [BinOp.evm]

/-- `0 * a => 0`, with the zero on the right. -/
theorem rw_mul_zero_right (a : Expr) : Expr.bin .mul a (.const 0) ⊑ .const 0 :=
  const_rule fun x _ => by simp [BinOp.evm]

/-- `1 * a => a`. -/
theorem rw_mul_one_left (a : Expr) : Expr.bin .mul (.const 1) a ⊑ a :=
  unary_rule_left fun _ y hy _ => by simpa [BinOp.evm] using hy

/-- `a * 1 => a`. -/
theorem rw_mul_one_right (a : Expr) : Expr.bin .mul a (.const 1) ⊑ a :=
  self_rule fun x => by simp [BinOp.evm]

/-- `bool_word(c) * a => ite(c, a, 0)`. -/
theorem rw_mul_bool_word_left (c : BExpr) (a : Expr) :
    Expr.bin .mul (boolWord c) a ⊑ .ite c a (.const 0) := by
  intro env v hv
  obtain ⟨x, y, hx, hy, _, rfl⟩ := bin_defined hv
  rcases boolWord_defined hx with ⟨hc, rfl⟩ | ⟨hc, rfl⟩
  · rw [ite_of_true hc]; simpa [BinOp.evm] using hy
  · rw [ite_of_false hc]; simp [BinOp.evm]

/-- `a * bool_word(c) => ite(c, a, 0)`. -/
theorem rw_mul_bool_word_right (c : BExpr) (a : Expr) :
    Expr.bin .mul a (boolWord c) ⊑ .ite c a (.const 0) := by
  intro env v hv
  obtain ⟨x, y, hx, hy, _, rfl⟩ := bin_defined hv
  rcases boolWord_defined hy with ⟨hc, rfl⟩ | ⟨hc, rfl⟩
  · rw [ite_of_true hc]; simpa [BinOp.evm] using hx
  · rw [ite_of_false hc]; simp [BinOp.evm]

/-- `a * 2**n => a << n`. -/
theorem rw_mul_pow2 (a : Expr) (n : Nat) (hn : n < 256) :
    Expr.bin .mul a (.const (BitVec.twoPow 256 n)) ⊑ .bin .shl a (.const (BitVec.ofNat 256 n)) :=
  unary_rule fun env x hx _ => by
    rw [bin_of_defined hx (evalK_const' _) (by simp [BinOp.isDiv])]
    congr 1
    have hn' : n % 2 ^ 256 = n := Nat.mod_eq_of_lt (Nat.lt_trans hn (by decide))
    simp only [BinOp.evm, BitVec.shiftLeft_eq', BitVec.toNat_ofNat, hn',
      BitVec.shiftLeft_eq_mul_twoPow]

/-! ## `SymBinOp::UDiv` and `SDiv` -/

/-- `a / 0 => 0`: the raw node is undefined, so any result refines it. -/
theorem rw_div_zero (op : BinOp) (hop : op = .udiv ∨ op = .sdiv) (a : Expr) :
    Expr.bin op a (.const 0) ⊑ .const 0 :=
  unary_rule fun _ _ _ hd => by rcases hop with rfl | rfl <;> simp [BinOp.isDiv] at hd

/-- `a / 1 => a`, unsigned and signed. -/
theorem rw_div_one (op : BinOp) (hop : op = .udiv ∨ op = .sdiv) (a : Expr) :
    Expr.bin op a (.const 1) ⊑ a :=
  self_rule fun x => by
    rcases hop with rfl | rfl <;> simp [BinOp.evm, BitVec.udiv_one, BitVec.sdiv_one]

/-- `a u/ 2**n => a >> n`. -/
theorem rw_udiv_pow2 (a : Expr) (n : Nat) (hn : n < 256) :
    Expr.bin .udiv a (.const (BitVec.twoPow 256 n)) ⊑ .bin .shr a (.const (BitVec.ofNat 256 n)) :=
  unary_rule fun env x hx _ => by
    rw [bin_of_defined hx (evalK_const' _) (by simp [BinOp.isDiv])]
    congr 1
    have hn' : n % 2 ^ 256 = n := Nat.mod_eq_of_lt (Nat.lt_trans hn (by decide))
    simp only [BinOp.evm, BitVec.ushiftRight_eq', BitVec.toNat_ofNat, hn',
      BitVec.udiv_twoPow_eq_of_lt hn]

/-! ## `SymBinOp::URem` and `SRem` -/

/-- `a % 0 => 0`: the raw node is undefined. -/
theorem rw_rem_zero (op : BinOp) (hop : op = .urem ∨ op = .srem) (a : Expr) :
    Expr.bin op a (.const 0) ⊑ .const 0 :=
  unary_rule fun _ _ _ hd => by rcases hop with rfl | rfl <;> simp [BinOp.isDiv] at hd

/-- `a % 1 => 0`, unsigned and signed. -/
theorem rw_rem_one (op : BinOp) (hop : op = .urem ∨ op = .srem) (a : Expr) :
    Expr.bin op a (.const 1) ⊑ .const 0 :=
  const_rule fun x _ => by rcases hop with rfl | rfl <;> simp [BinOp.evm, BitVec.umod_one]

theorem twoPow_ne_zero {n : Nat} (hn : n < 256) : BitVec.twoPow 256 n ≠ 0 := by
  intro h
  have := congrArg BitVec.toNat h
  rw [BitVec.toNat_twoPow_of_lt hn] at this
  simp at this

theorem toNat_twoPow_sub_one {n : Nat} (hn : n < 256) :
    (BitVec.twoPow 256 n - 1).toNat = 2 ^ n - 1 := by
  have hp : 0 < 2 ^ n := Nat.two_pow_pos n
  have hlt : 2 ^ n < 2 ^ 256 := Nat.pow_lt_pow_right (by decide) hn
  rw [BitVec.toNat_sub, BitVec.toNat_twoPow_of_lt hn]
  have h1 : (1 : Word).toNat = 1 := rfl
  rw [h1, show 2 ^ 256 - 1 + 2 ^ n = (2 ^ n - 1) + 2 ^ 256 by omega, Nat.add_mod_right,
    Nat.mod_eq_of_lt (by omega)]

theorem umod_twoPow (x : Word) {n : Nat} (hn : n < 256) :
    x % BitVec.twoPow 256 n = x &&& (BitVec.twoPow 256 n - 1) := by
  apply BitVec.eq_of_toNat_eq
  rw [BitVec.toNat_umod, BitVec.toNat_and, toNat_twoPow_sub_one hn, BitVec.toNat_twoPow_of_lt hn,
    Nat.and_two_pow_sub_one_eq_mod]

/-- `a u% 2**n => a & (2**n - 1)`. -/
theorem rw_urem_pow2 (a : Expr) (n : Nat) (hn : n < 256) :
    Expr.bin .urem a (.const (BitVec.twoPow 256 n)) ⊑
      .bin .and a (.const (BitVec.twoPow 256 n - 1)) :=
  unary_rule fun env x hx _ => by
    rw [bin_of_defined hx (evalK_const' _) (by simp [BinOp.isDiv])]
    congr 1
    simp only [BinOp.evm]
    rw [if_neg (twoPow_ne_zero hn)]
    exact (umod_twoPow x hn).symm

/-! ## `SymBinOp::And`, `Or`, `Xor` -/

/-- `0 & a => 0`. -/
theorem rw_and_zero (a : Expr) : Expr.bin .and a (.const 0) ⊑ .const 0 :=
  const_rule fun x _ => by simp [BinOp.evm]

/-- `a & MAX => a`. -/
theorem rw_and_max (a : Expr) : Expr.bin .and a (.const maxWord) ⊑ a :=
  self_rule fun x => by simp only [BinOp.evm, maxWord]; exact bv_and_allOnes x

/-- `a & a => a`. -/
theorem rw_and_self (a : Expr) : Expr.bin .and a a ⊑ a :=
  self_pair_rule fun _ x hx _ => by simpa [BinOp.evm] using hx

/-- `a | 0 => a`. -/
theorem rw_or_zero (a : Expr) : Expr.bin .or a (.const 0) ⊑ a :=
  self_rule fun x => by simp [BinOp.evm]

/-- `a | a => a`. -/
theorem rw_or_self (a : Expr) : Expr.bin .or a a ⊑ a :=
  self_pair_rule fun _ x hx _ => by simpa [BinOp.evm] using hx

/-- `or_with_absorbing_ite`: `ite(c, 0, MAX) | a => ite(c, a, MAX)`. -/
theorem rw_or_absorbing_ite_zero_max (c : BExpr) (a : Expr) :
    Expr.bin .or (.ite c (.const 0) (.const maxWord)) a ⊑ .ite c a (.const maxWord) := by
  intro env v hv
  obtain ⟨x, y, hx, hy, _, rfl⟩ := bin_defined hv
  rcases ite_defined hx with ⟨hc, hk⟩ | ⟨hc, hk⟩ <;> simp at hk <;> subst hk
  · rw [ite_of_true hc]; simpa [BinOp.evm] using hy
  · rw [ite_of_false hc]; simp only [BinOp.evm, maxWord, evalK_const', Option.some.injEq]
    exact bv_allOnes_or _

/-- `or_with_absorbing_ite`: `ite(c, MAX, 0) | a => ite(c, MAX, a)`. -/
theorem rw_or_absorbing_ite_max_zero (c : BExpr) (a : Expr) :
    Expr.bin .or (.ite c (.const maxWord) (.const 0)) a ⊑ .ite c (.const maxWord) a := by
  intro env v hv
  obtain ⟨x, y, hx, hy, _, rfl⟩ := bin_defined hv
  rcases ite_defined hx with ⟨hc, hk⟩ | ⟨hc, hk⟩ <;> simp at hk <;> subst hk
  · rw [ite_of_true hc]; simp only [BinOp.evm, maxWord, evalK_const', Option.some.injEq]
    exact bv_allOnes_or _
  · rw [ite_of_false hc]; simpa [BinOp.evm] using hy

/-- `a ^ 0 => a`. -/
theorem rw_xor_zero (a : Expr) : Expr.bin .xor a (.const 0) ⊑ a :=
  self_rule fun x => by simp [BinOp.evm]

/-- `a ^ a => 0`. -/
theorem rw_xor_self (a : Expr) : Expr.bin .xor a a ⊑ .const 0 :=
  self_pair_rule fun _ x _ _ => by simp [BinOp.evm]

/-- `a ^ (a ^ b) => b` (`xor_with_shared_operand`, left occurrence). -/
theorem rw_xor_shared_left (a b : Expr) : Expr.bin .xor a (.bin .xor a b) ⊑ b := by
  intro env v hv
  obtain ⟨x, s, hx, hs, _, rfl⟩ := bin_defined hv
  obtain ⟨x', z, hx', hz, _, rfl⟩ := bin_defined hs
  have : x' = x := Option.some.inj (hx'.symm.trans hx)
  subst this; rw [hz]; simp only [BinOp.evm, Option.some.injEq]; exact bv_xor_cancel_left _ _

/-- `a ^ (a ^ b) => b` (`xor_with_shared_operand`, right occurrence). -/
theorem rw_xor_shared_right (a b : Expr) : Expr.bin .xor a (.bin .xor b a) ⊑ b := by
  intro env v hv
  obtain ⟨x, s, hx, hs, _, rfl⟩ := bin_defined hv
  obtain ⟨z, x', hz, hx', _, rfl⟩ := bin_defined hs
  have : x' = x := Option.some.inj (hx'.symm.trans hx)
  subst this; rw [hz]; simp only [BinOp.evm, Option.some.injEq]; exact bv_xor_cancel_right _ _

/-- `a ^ ((a ^ b) * bool_word(c)) => ite(c, b, a)` (`xor_with_bool_select`). -/
theorem rw_xor_bool_select (a b : Expr) (c : BExpr) :
    Expr.bin .xor a (.bin .mul (.bin .xor a b) (boolWord c)) ⊑ .ite c b a := by
  intro env v hv
  obtain ⟨x, s, hx, hs, _, rfl⟩ := bin_defined hv
  obtain ⟨d, w, hd, hw, _, rfl⟩ := bin_defined hs
  obtain ⟨x', z, hx', hz, _, rfl⟩ := bin_defined hd
  have : x' = x := Option.some.inj (hx'.symm.trans hx)
  subst this
  rcases boolWord_defined hw with ⟨hc, rfl⟩ | ⟨hc, rfl⟩
  · rw [ite_of_true hc, hz]; simp only [BinOp.evm, Option.some.injEq]; exact bv_xor_select _ _
  · rw [ite_of_false hc, hx']; congr 1; simp [BinOp.evm]

/-- `a ^ ite(c, b, 0) => ite(c, a ^ b, a)` (`xor_with_zero_ite`, zero then-arm). -/
theorem rw_xor_zero_ite_then (a b : Expr) (c : BExpr) :
    Expr.bin .xor a (.ite c (.const 0) b) ⊑ .ite c a (.bin .xor a b) := by
  intro env v hv
  obtain ⟨x, y, hx, hy, _, rfl⟩ := bin_defined hv
  rcases ite_defined hy with ⟨hc, hk⟩ | ⟨hc, hk⟩
  · simp at hk; subst hk; rw [ite_of_true hc, hx]; simp [BinOp.evm]
  · rw [ite_of_false hc, bin_of_defined hx hk (by simp [BinOp.isDiv])]

/-- `a ^ ite(c, b, 0) => ite(c, a ^ b, a)` (`xor_with_zero_ite`, zero else-arm). -/
theorem rw_xor_zero_ite_else (a b : Expr) (c : BExpr) :
    Expr.bin .xor a (.ite c b (.const 0)) ⊑ .ite c (.bin .xor a b) a := by
  intro env v hv
  obtain ⟨x, y, hx, hy, _, rfl⟩ := bin_defined hv
  rcases ite_defined hy with ⟨hc, hk⟩ | ⟨hc, hk⟩
  · rw [ite_of_true hc, bin_of_defined hx hk (by simp [BinOp.isDiv])]
  · simp at hk; subst hk; rw [ite_of_false hc, hx]; simp [BinOp.evm]

/-! ## Shifts -/

/-- `a << 0 => a`. -/
theorem rw_shl_zero (a : Expr) : Expr.bin .shl a (.const 0) ⊑ a :=
  self_rule fun x => by simp [BinOp.evm]

/-- `0 << a => 0`. -/
theorem rw_zero_shl (a : Expr) : Expr.bin .shl (.const 0) a ⊑ .const 0 :=
  unary_rule_left fun _ y _ _ => by simp [BinOp.evm]

/-- `a << 256 => 0`, for every constant shift of at least 256. -/
theorem rw_shl_ge_256 (a : Expr) (k : Word) (hk : 256 ≤ k.toNat) :
    Expr.bin .shl a (.const k) ⊑ .const 0 :=
  const_rule fun x _ => by simp [BinOp.evm, BitVec.shiftLeft_eq', BitVec.shiftLeft_eq_zero hk]

/-- `a >> 0 => a`. -/
theorem rw_shr_zero (a : Expr) : Expr.bin .shr a (.const 0) ⊑ a :=
  self_rule fun x => by simp [BinOp.evm]

/-- `0 >> a => 0`. -/
theorem rw_zero_shr (a : Expr) : Expr.bin .shr (.const 0) a ⊑ .const 0 :=
  unary_rule_left fun _ y _ _ => by simp [BinOp.evm]

/-- `a >>s 0 => a`. -/
theorem rw_sar_zero (a : Expr) : Expr.bin .sar a (.const 0) ⊑ a :=
  self_rule fun x => by simp [BinOp.evm]

/-! ## `SymBinOp::MulHigh` -/

/-- `mul_high(a, 0 | 1) => 0`. -/
theorem rw_mul_high_small (a : Expr) (k : Word) (hk : k = 0 ∨ k = 1) :
    Expr.bin .mulHigh a (.const k) ⊑ .const 0 :=
  const_rule fun x _ => by
    rcases hk with rfl | rfl
    · simp [BinOp.evm, BinOp.mulHighWord]
    · simp [BinOp.evm, BinOp.mulHighWord, Nat.div_eq_of_lt x.isLt]

end Symbolic
