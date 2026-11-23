import Symbolic.BoolRules
import Symbolic.Xor

/-!
# Bit-width bounds and the rewrites that rely on them

Models `SymExpr::unsigned_bits` (`crates/evm/symbolic/src/runtime/expr/expr.rs`), the structural
upper bound on the number of significant bits of a word. Several construction rewrites trust it:
`small(a) >> bits(a) => 0` and `(a << n) >> n => a` in `shr_const`, and the one-bit words that
`bitwise_bool_word_condition` turns into conditions. `unsignedBits_sound` proves the bound for
every defined value, including the `UDiv` and `URem` cases whose raw SMT values at a zero
divisor (all ones, the dividend) would break it: only the Kleene meaning is bounded.

The Rust visit budget can only return 256, which is always sound, so it is omitted.
-/

namespace Symbolic

/-- `U256::bit_len`. -/
def bitLen (n : Nat) : Nat := if n = 0 then 0 else n.log2 + 1

theorem lt_two_pow_bitLen (n : Nat) : n < 2 ^ bitLen n := by
  unfold bitLen
  split
  · subst_vars; decide
  · exact Nat.lt_log2_self

/-- `SymExpr::unsigned_bits`. -/
def unsignedBits : Expr → Nat
  | .const v => max (bitLen v.toNat) 1
  | .bin .and l (.const m) => min (unsignedBits l) (bitLen m.toNat)
  | .bin .add l r => min (max (unsignedBits l) (unsignedBits r) + 1) 256
  | .bin .mul (.bin .udiv a d) r =>
    if d.beq r then unsignedBits a else min (unsignedBits a + unsignedBits r) 256
  | .bin .mul l (.bin .udiv a d) =>
    if d.beq l then unsignedBits a else min (unsignedBits l + unsignedBits a) 256
  | .bin .mul l r => min (unsignedBits l + unsignedBits r) 256
  | .bin .shl l (.const s) =>
    if s.toNat < 2 ^ 64 then min (unsignedBits l + s.toNat) 256 else 256
  | .bin .shr l (.const s) => if s.toNat < 2 ^ 64 then max (unsignedBits l - s.toNat) 1 else 256
  | .bin .udiv l _ => unsignedBits l
  | .bin .urem l (.const d) => if d = 0 then unsignedBits l else min (unsignedBits l) (bitLen d.toNat)
  | .bin .urem l _ => unsignedBits l
  | .tern .addMod _ _ m => unsignedBits m
  | .tern .mulMod _ _ m => unsignedBits m
  | .ite _ l r => max (unsignedBits l) (unsignedBits r)
  | _ => 256

section
variable {env : Env}

theorem lt_pow_of_lt_pow_le {a m n : Nat} (h : a < 2 ^ m) (hle : m ≤ n) : a < 2 ^ n :=
  Nat.lt_of_lt_of_le h (Nat.pow_le_pow_right (by decide) hle)

theorem lt_pow_min {a m n : Nat} (hm : a < 2 ^ m) (hn : a < 2 ^ n) : a < 2 ^ min m n := by
  rcases Nat.le_total m n with h | h
  · rw [Nat.min_eq_left h]; exact hm
  · rw [Nat.min_eq_right h]; exact hn

theorem lt_pow_max_left {a m n : Nat} (h : a < 2 ^ m) : a < 2 ^ max m n :=
  lt_pow_of_lt_pow_le h (Nat.le_max_left _ _)

theorem lt_pow_max_right {a m n : Nat} (h : a < 2 ^ n) : a < 2 ^ max m n :=
  lt_pow_of_lt_pow_le h (Nat.le_max_right _ _)

theorem mul_lt_pow_add {x y m n : Nat} (hx : x < 2 ^ m) (hy : y < 2 ^ n) :
    x * y < 2 ^ (m + n) := by
  rw [Nat.pow_add]
  rcases Nat.eq_zero_or_pos y with h | h
  · subst h; rw [Nat.mul_zero]; exact Nat.mul_pos (Nat.two_pow_pos _) (Nat.two_pow_pos _)
  · exact Nat.lt_of_lt_of_le (Nat.mul_lt_mul_of_pos_right hx h)
      (Nat.mul_le_mul_left _ (Nat.le_of_lt hy))

theorem toNat_mul_lt {x y : Word} {m n : Nat} (hx : x.toNat < 2 ^ m) (hy : y.toNat < 2 ^ n) :
    (x * y).toNat < 2 ^ min (m + n) 256 := by
  rw [BitVec.toNat_mul]
  apply lt_pow_min
  · exact Nat.lt_of_le_of_lt (Nat.mod_le _ _) (mul_lt_pow_add hx hy)
  · exact Nat.mod_lt _ (by decide)

theorem pos_of_ne_zero {y : Word} (h : ¬ y = 0) : 0 < y.toNat := by
  rcases Nat.eq_zero_or_pos y.toNat with h' | h'
  · exact absurd (BitVec.eq_of_toNat_eq (by simpa using h')) h
  · exact h'

/-- The value bound that `unsignedBits` promises. -/
def BitsSound (env : Env) (e : Expr) : Prop :=
  ∀ v, evalK env e = some v → v.toNat < 2 ^ unsignedBits e

theorem udiv_bound {a : Expr} {x e : Word} (ha : BitsSound env a) (hx : evalK env a = some x) :
    (BinOp.udiv.evm x e).toNat < 2 ^ unsignedBits a := by
  simp only [BinOp.evm, BitVec.toNat_udiv]
  exact Nat.lt_of_le_of_lt (Nat.div_le_self _ _) (ha x hx)

/-- **Bit-width soundness.** Every defined value fits `unsignedBits` bits. -/
theorem unsignedBits_sound (e : Expr) : BitsSound env e := by
  induction e using unsignedBits.induct with
  | case1 c =>
    intro v h; simp at h; subst h
    simp only [unsignedBits]; exact lt_pow_max_left (lt_two_pow_bitLen _)
  | case2 l m ih =>
    intro v h
    obtain ⟨x, k, hx, hk, _, rfl⟩ := bin_defined h
    simp at hk; subst hk
    simp only [unsignedBits, BinOp.evm, BitVec.toNat_and]
    exact lt_pow_min (Nat.lt_of_le_of_lt Nat.and_le_left (ih x hx))
      (Nat.lt_of_le_of_lt Nat.and_le_right (lt_two_pow_bitLen _))
  | case3 l r ihl ihr =>
    intro v h
    obtain ⟨x, y, hx, hy, _, rfl⟩ := bin_defined h
    have hl := lt_pow_max_left (n := unsignedBits r) (ihl x hx)
    have hr := lt_pow_max_right (m := unsignedBits l) (ihr y hy)
    simp only [unsignedBits, BinOp.evm, BitVec.toNat_add]
    apply lt_pow_min
    · apply Nat.lt_of_le_of_lt (Nat.mod_le _ _)
      rw [Nat.pow_succ]; omega
    · exact Nat.mod_lt _ (by decide)
  | case4 a d r hb iha =>
    intro v h
    obtain ⟨q, y, hq, hy, _, rfl⟩ := bin_defined h
    obtain ⟨x, e, hx, he, _, rfl⟩ := bin_defined hq
    rw [Expr.eq_of_beq d r hb] at he
    have : e = y := Option.some.inj (he.symm.trans hy)
    subst this
    simp only [unsignedBits, hb, if_true, BinOp.evm]
    rw [toNat_udiv_mul]
    exact Nat.lt_of_le_of_lt (Nat.div_mul_le_self _ _) (iha x hx)
  | case5 a d r hb iha ihr =>
    intro v h
    obtain ⟨q, y, hq, hy, _, rfl⟩ := bin_defined h
    obtain ⟨x, e, hx, he, _, rfl⟩ := bin_defined hq
    simp only [unsignedBits, hb, if_false, Bool.false_eq_true]
    exact toNat_mul_lt (udiv_bound iha hx) (ihr y hy)
  | case6 l a d hl hb iha =>
    intro v h
    obtain ⟨x, q, hx, hq, _, rfl⟩ := bin_defined h
    obtain ⟨y, e, hy, he, _, rfl⟩ := bin_defined hq
    rw [Expr.eq_of_beq d l hb] at he
    have : e = x := Option.some.inj (he.symm.trans hx)
    subst this
    have hu : unsignedBits (.bin .mul l (.bin .udiv a d)) = unsignedBits a := by
      conv => lhs; rw [unsignedBits.eq_def]
      split <;> try simp_all
      all_goals (obtain ⟨-, hr⟩ := ‹_ ∧ _›; subst hr; simp_all)
    rw [hu]
    simp only [BinOp.evm]
    rw [BitVec.mul_comm, toNat_udiv_mul]
    exact Nat.lt_of_le_of_lt (Nat.div_mul_le_self _ _) (iha y hy)
  | case7 l a d hl hb ihl iha =>
    intro v h
    obtain ⟨x, q, hx, hq, _, rfl⟩ := bin_defined h
    obtain ⟨y, e, hy, he, _, rfl⟩ := bin_defined hq
    have hu : unsignedBits (.bin .mul l (.bin .udiv a d)) =
        min (unsignedBits l + unsignedBits a) 256 := by
      conv => lhs; rw [unsignedBits.eq_def]
      split <;> try simp_all
      all_goals (obtain ⟨-, hr⟩ := ‹_ ∧ _›; subst hr; simp_all)
    rw [hu]
    exact toNat_mul_lt (ihl x hx) (udiv_bound iha hy)
  | case8 l r hl hr ihl ihr =>
    intro v h
    obtain ⟨x, y, hx, hy, _, rfl⟩ := bin_defined h
    have hu : unsignedBits (.bin .mul l r) = min (unsignedBits l + unsignedBits r) 256 := by
      conv => lhs; rw [unsignedBits.eq_def]
      split <;> try simp_all
      all_goals (obtain ⟨-, hr⟩ := ‹_ ∧ _›; subst hr; simp_all)
    rw [hu]
    exact toNat_mul_lt (ihl x hx) (ihr y hy)
  | case9 l s hs ih =>
    intro v h
    obtain ⟨x, k, hx, hk, _, rfl⟩ := bin_defined h
    simp at hk; subst hk
    simp only [unsignedBits, hs, if_true, BinOp.evm, BitVec.shiftLeft_eq', BitVec.toNat_shiftLeft]
    apply lt_pow_min
    · apply Nat.lt_of_le_of_lt (Nat.mod_le _ _)
      rw [Nat.shiftLeft_eq, Nat.pow_add]
      exact Nat.mul_lt_mul_of_pos_right (ih x hx) (Nat.two_pow_pos _)
    · exact Nat.mod_lt _ (by decide)
  | case10 l s hs =>
    intro v h
    simp only [unsignedBits, hs, if_false]
    exact v.isLt
  | case11 l s hs ih =>
    intro v h
    obtain ⟨x, k, hx, hk, _, rfl⟩ := bin_defined h
    simp at hk; subst hk
    simp only [unsignedBits, hs, if_true, BinOp.evm, BitVec.ushiftRight_eq',
      BitVec.toNat_ushiftRight, Nat.shiftRight_eq_div_pow]
    apply lt_pow_max_left
    rw [Nat.div_lt_iff_lt_mul (Nat.two_pow_pos _), ← Nat.pow_add]
    exact lt_pow_of_lt_pow_le (ih x hx) (by omega)
  | case12 l s hs =>
    intro v h
    simp only [unsignedBits, hs, if_false]
    exact v.isLt
  | case13 l b ih =>
    intro v h
    obtain ⟨x, y, hx, hy, _, rfl⟩ := bin_defined h
    simp only [unsignedBits]
    exact udiv_bound ih hx
  | case14 l ih =>
    intro v h
    obtain ⟨x, y, hx, hy, hd, rfl⟩ := bin_defined h
    simp at hy; subst hy
    exact absurd ⟨rfl, rfl⟩ hd
  | case15 l c hc ih =>
    intro v h
    obtain ⟨x, y, hx, hy, hd, rfl⟩ := bin_defined h
    simp at hy; subst hy
    simp only [unsignedBits, hc, if_false, BinOp.evm, BitVec.toNat_umod]
    apply lt_pow_min
    · exact Nat.lt_of_le_of_lt (Nat.mod_le _ _) (ih x hx)
    · exact Nat.lt_trans (Nat.mod_lt _ (pos_of_ne_zero hc)) (lt_two_pow_bitLen _)
  | case16 l b hb ih =>
    intro v h
    obtain ⟨x, y, hx, hy, hd, rfl⟩ := bin_defined h
    have hy0 : ¬ y = 0 := fun h => hd ⟨rfl, h⟩
    have hu : unsignedBits (.bin .urem l b) = unsignedBits l := by
      conv => lhs; rw [unsignedBits.eq_def]
      split <;> try simp_all
      all_goals (obtain ⟨-, hr⟩ := ‹_ ∧ _›; subst hr; simp_all)
    rw [hu]
    simp only [BinOp.evm, hy0, if_false, BitVec.toNat_umod]
    exact Nat.lt_of_le_of_lt (Nat.mod_le _ _) (ih x hx)
  | case17 a b m ih | case18 a b m ih =>
    intro v h
    obtain ⟨x, y, z, _, _, hz, rfl⟩ := tern_defined h
    simp only [unsignedBits, TernOp.eval]
    split
    · simpa using Nat.two_pow_pos _
    · rename_i hz0
      rw [BitVec.toNat_ofNat]
      exact Nat.lt_of_le_of_lt (Nat.mod_le _ _)
        (Nat.lt_trans (Nat.mod_lt _ (pos_of_ne_zero hz0)) (ih z hz))
  | case19 c l r ihl ihr =>
    intro v h
    simp only [unsignedBits]
    rcases ite_defined h with ⟨_, h⟩ | ⟨_, h⟩
    · exact lt_pow_max_left (ihl v h)
    · exact lt_pow_max_right (ihr v h)
  | case20 t h1 h2 h3 h4 h5 h6 h7 h8 h9 h10 h11 h12 h13 h14 =>
    intro v _
    have hu : unsignedBits t = 256 := by
      rw [unsignedBits.eq_def]
      split
      all_goals first
        | rfl
        | exact absurd rfl (h1 _) | exact absurd rfl (h2 _ _) | exact absurd rfl (h3 _ _)
        | exact absurd rfl (h4 _ _ _) | exact absurd rfl (h5 _ _ _) | exact absurd rfl (h6 _ _)
        | exact absurd rfl (h7 _ _) | exact absurd rfl (h8 _ _) | exact absurd rfl (h9 _ _)
        | exact absurd rfl (h10 _ _) | exact absurd rfl (h11 _ _) | exact absurd rfl (h12 _ _ _)
        | exact absurd rfl (h13 _ _ _) | exact absurd rfl (h14 _ _ _)
    rw [hu]; exact v.isLt

/-! ## Rewrites that trust `unsignedBits` -/

/-- `small(a) >> bits(a) => 0` (`shr_const`). -/
theorem rw_shr_small (a : Expr) (s : Word) (hs : unsignedBits a ≤ s.toNat) :
    Expr.bin .shr a (.const s) ⊑ .const 0 := by
  intro env v hv
  obtain ⟨x, k, hx, hk, _, rfl⟩ := bin_defined hv
  simp at hk; subst hk
  have hl := unsignedBits_sound a x hx
  simp only [evalK_const', BinOp.evm, Option.some.injEq]
  apply BitVec.eq_of_toNat_eq
  rw [BitVec.ushiftRight_eq', BitVec.toNat_ushiftRight, Nat.shiftRight_eq_div_pow,
    Nat.div_eq_of_lt (lt_pow_of_lt_pow_le hl hs)]
  rfl

/-- `(a << n) >> n => a` (`shr_const`), when `a` fits `256 - n` bits. -/
theorem rw_shr_shl (a : Expr) (n : Nat) (hn : n < 256) (hfit : unsignedBits a ≤ 256 - n) :
    Expr.bin .shr (.bin .shl a (.const (BitVec.ofNat 256 n))) (.const (BitVec.ofNat 256 n)) ⊑ a := by
  intro env v hv
  obtain ⟨y, k, hy, hk, _, rfl⟩ := bin_defined hv
  obtain ⟨x, k', hx, hk', _, rfl⟩ := bin_defined hy
  simp at hk hk'; subst hk hk'
  have hl := lt_pow_of_lt_pow_le (unsignedBits_sound a x hx) hfit
  rw [hx]; congr 1
  have hn' : (BitVec.ofNat 256 n).toNat = n := by
    rw [BitVec.toNat_ofNat]; exact Nat.mod_eq_of_lt (by omega)
  apply BitVec.eq_of_toNat_eq
  simp only [BinOp.evm, BitVec.ushiftRight_eq', BitVec.shiftLeft_eq', BitVec.toNat_ushiftRight,
    BitVec.toNat_shiftLeft, hn', Nat.shiftRight_eq_div_pow, Nat.shiftLeft_eq]
  have hlt : x.toNat * 2 ^ n < 2 ^ 256 := by
    have := Nat.mul_lt_mul_of_pos_right hl (Nat.two_pow_pos n)
    rwa [← Nat.pow_add, Nat.sub_add_cancel (by omega)] at this
  rw [Nat.mod_eq_of_lt hlt, Nat.mul_div_cancel _ (Nat.two_pow_pos n)]

/-- The one-bit leaf of `bitwise_bool_word_condition`: a word that fits one bit is the Boolean word
of its nonzero check. -/
theorem rw_one_bit_word (a : Expr) (h1 : unsignedBits a = 1) :
    a ⊑ boolWord (.not (.cmp .eq a (.const 0))) := by
  intro env v hv
  have hl := unsignedBits_sound a v hv
  rw [h1] at hl
  simp only [boolWord, evalK_ite', evalKB_not', evalKB_cmp', hv, evalK_const', kcmp_some_some,
    CmpOp.eval]
  rcases Nat.lt_or_ge v.toNat 1 with h | h
  · have : v = 0#256 := BitVec.eq_of_toNat_eq (by rw [BitVec.toNat_ofNat]; omega)
    subst this; rfl
  · have : v = 1#256 := BitVec.eq_of_toNat_eq (by rw [BitVec.toNat_ofNat]; omega)
    subst this; rfl

end

end Symbolic
