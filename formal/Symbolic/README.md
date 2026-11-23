# `Symbolic`: soundness of symbolic `PASS` verdicts

A symbolic `PASS` means the solver, or one of the executor's own UNSAT shortcuts, found no
feasible failing path. This library proves the parts of that argument that live in the
expression layer of `crates/evm/symbolic`: what the solver is asked is what the EVM executes,
the rewrites applied on the way preserve that, and the shortcuts that answer UNSAT without the
solver are right.

## The division invariant

The Rust code gives the division operators two meanings. `SymBinOp::eval`, used for constant
folding and model validation, follows the EVM (`x / 0 = x % 0 = 0`). `SymBinOp::smt`, used by the
solver, emits SMT-LIB `bvudiv`/`bvurem`/`bvsdiv`/`bvsrem` (`x / 0` is all ones, `x % 0 = x`). Some
rewrites hold only under one of them: `(a / d) * d + a % d => a` only under SMT-LIB, `a / 0 => 0`
and `a / b == 0 => b == 0 || a < b` only under the EVM.

The model resolves this with a third, Kleene meaning (`evalK`), in which a raw division by zero is
undefined, `ite` looks only at the selected arm, and a conjunction is false as soon as one side is.
Then:

- `evalK_sound`: wherever `evalK` is defined, every total division-by-zero convention, the EVM's
  and SMT-LIB's included, gives the same value.
- `evalK_divGuard`, `WellGuarded.defined`: `DIV`, `SDIV`, `MOD` and `SMOD` are lowered to
  `ite(b == 0, 0, a op b)` (`bin_word_div_zero_guard`), which is defined wherever its operands
  are, with the EVM value. A path condition built from such words is defined everywhere.
- The rewrite contract is refinement (`⊑`): wherever the original is defined, the result is
  defined with the same value. Every rewrite above is a refinement, including the ones that hold
  under only one of the two total meanings, because they differ only where a raw division by zero
  makes the original undefined. Refinement is a congruence (`RefinesAt.bin`, `RefinesBAt.ite`, ...).
- `refinesBUnder_and`: a contextual rewrite of one conjunct, justified by facts of the same query,
  keeps every point that satisfies the query.
- `unsat_transfer'`, `pass_sound`: if the path condition is defined and normalization keeps its
  satisfying points, an SMT-LIB UNSAT answer means no EVM execution takes the path.

So the invariant the code relies on is: **every query handed to the solver or to an UNSAT shortcut
is defined in the Kleene meaning, and every rewrite refines it.** A raw division with a possibly
zero divisor may reach the solver, for example after `ite(d == 0, 0, n / d) == k` is split into
`d != 0 && n / d == k`; it is harmless because the query is still defined. No reachable violation
was found (see Conformance).

A finding along the way: `ConstraintContext::requires_independent_context` and `for_rewrite` keep
a conjunct out of the context that rewrites it. That is not needed for soundness of UNSAT:
`refinesBUnder_and` allows the whole query, the rewritten conjunct included, as context. A
self-justified rewrite can only drop the conjunct (`self_justified_rewrite_weakens`), which costs
completeness (a spurious model that replay rejects), never a false PASS.

## Modules

| Module | Contents | Rust |
| --- | --- | --- |
| `Word` | `BinOp`, `TernOp`, `CmpOp`; EVM and SMT-LIB meanings; `DivZero` | `SymBinOp::eval`, `SymBinOp::smt`, `SymTernOp::eval`, `SymCmpOp::eval` in `runtime/expr/expr.rs`, `bool.rs` |
| `Expr` | `Expr`, `BExpr`; `eval`, `evalK`; agreement, refinement, congruence, UNSAT transfer | `SymExprKind`, `SymBoolExprKind`, `SymExpr::eval_model` |
| `Guard` | `divGuard`, `WellGuarded`, `pass_sound` | `SymbolicState::bin_word_div_zero_guard` in `runtime/state.rs` |
| `Rewrites`, `WordRules`, `BoolRules`, `NormRules`, `Bits` | one theorem per rewrite arm | `SymExpr::binop`, `ternop`, `mul_div`, `ite`, `not`, `and_const`, `shr_const`, `unsigned_bits`; `SymBoolExpr::cmp`, `and`, `or`, `not_bool`; `normalize/mod.rs` |
| `Lowering` | `SIGNEXTEND`, `BYTE`, `CLZ`, `EXP` lowerings; ABI integer constraints | `signextend_word`, `signextend_word_dynamic`, `byte_word_dynamic`, `clz_word`, `exp_expr_for_concrete_exponent` in `runtime/evm.rs`; `SymbolicState::exp_word`; `constrain_uint`, `constrain_int` in `abi.rs` |
| `Smt` | 512-bit `addmod`/`mulmod`/`mulDiv`, `MulHigh`, narrow division encodings | `write_wide_modular_arithmetic`, `write_narrow_division`, `SmtCseWriter::write_expr` in `runtime/solver/smt/mod.rs` |
| `Xor` | `affine_terms`, `check_certificate`, `check_certificate_sound` | `runtime/solver/reasoning/xor.rs` |

## Headline theorems

- `evalK_sound`: a defined Kleene value is the value under every division-by-zero meaning.
- `pass_sound`: well-guarded path condition + truth-preserving normalization + SMT UNSAT ⇒ no EVM
  execution satisfies the path condition.
- `refinesBUnder_and`: contextual rewriting under facts of the query preserves its satisfying points.
- `check_certificate_sound`: an accepted XOR certificate admits no point satisfying all relations.
- `unsignedBits_sound`: every defined value fits `unsigned_bits` bits.
- `signextendWord_spec`, `evalK_signextendDynamic`, `evalK_byteDynamic`, `clzWord_spec`,
  `evalK_expProduct`, `evalK_expChain`: the opcode lowerings equal the EVM opcodes for all inputs
  (the symbolic `EXP` chain under the path bound on the exponent).
- `constrain_uint_exact`, `constrain_int_exact`: the ABI constraints hold exactly for the values of
  `uintN` and `intN`, so they never exclude a valid input.
- `narrowUDiv_eq`, `narrowURem_eq`, `wideModular_addMod`, `wideModular_mulMod`,
  `wideModular_mulDiv`, `mulHigh_encoding`: the SMT encodings denote the model operators.

## Axioms

`lake env lean Symbolic/Axioms.lean` prints them. Everything depends only on `propext`,
`Quot.sound` and `Classical.choice`, except proofs by `bv_decide`, which add one
`*._native.bv_decide.ax_*` axiom each (trust in Lean's native evaluation of the LRAT checker):
`check_certificate_sound` (one lemma), `signextendWord_spec` and `evalK_signextendDynamic` (32
lemmas), `clzWord_spec`, `constrain_int_exact` (31 lemmas), and the small word identities in
`WordRules` and `BoolRules`. No `sorry`, no new axioms.

## Fidelity

- Leaves: `Var`, `GasLeft`, `Keccak` and `Hash` are all opaque `var n`. Hash collision resistance,
  which `SymBoolExpr::cmp` assumes for Keccak, SHA-256 and RIPEMD-160, is outside the model and is
  the main modelling assumption behind a PASS that involves hashes.
- `And` is binary; Rust's n-ary flattening is `rw_and_assoc`. `Or` is the Rust encoding
  `!(!a && !b)`.
- Hash-consed pointer equality is structural equality (`Expr.beq`).
- `bool_word(c)` is modelled in its canonical shape `ite(c, 1, 0)`. Rust also recognizes
  `ite(c, 0, 1)` and one-bit words (`bitwise_bool_word_condition`); the latter is
  `rw_one_bit_word`.
- Rewrites are stated over the shapes their match arms test. A theorem does not check the Rust
  side conditions that select an arm (for example, that `power_of_two_shift` returns `n` only for
  `2^n`); those are named hypotheses where they matter.
- Contextual rewrites (`ConstraintContext`) are covered by the generic `refinesBUnder_and` plus
  per-rule theorems that take the context fact as a hypothesis (`rw_udiv_lt`, `rw_le_udiv`). The
  interval analysis that produces those facts (`ConstraintContext::interval`) is not modelled.
- Budgets (visit limits, `MAX_LOCAL_ANALYSIS_NODES`) can only make the Rust code decline a rewrite
  or answer 256 bits, so they are omitted.
- Lean's `smtUDiv`, `smtSDiv`, `%` and `srem` stand for SMT-LIB `bvudiv`, `bvsdiv`, `bvurem` and
  `bvsrem`; the solver itself is trusted.

## Conformance

- `check-inventory.sh` fails when a rewrite arm in `runtime/expr/expr.rs`, `runtime/expr/bool.rs`
  or `runtime/solver/normalize/mod.rs` carries a `// \`lhs => rhs\`` comment that has no entry in
  the inventory below, so a new or reworded rule cannot land without a decision about its proof.
- The Rust functions that carry an invariant cite their theorem in rustdoc (`SymExpr::binop`,
  `SymBinOp::eval`/`smt`, `unsigned_bits`, `bin_word_div_zero_guard`, `check_certificate`,
  `write_narrow_division`, `write_wide_modular_arithmetic`, `constrain_uint`/`constrain_int`,
  `normalize_contextually`), so a change there points at the proof to revisit.
- A differential fuzzer (`differential/fuzz.rs`, run as described in its README) was run against the Rust implementation while building this model: random
  and template-driven expression DAGs (Euclidean sums, `a - a % d`, checked-multiply guards,
  ceiling division, Remco's `mul_high`, power-of-two moduli, shifts, `SIGNEXTEND`/`BYTE`/`CLZ`,
  guarded signed division) were built through the smart constructors and compared with a
  reference EVM evaluator, and random constraint sets true at a point were normalized
  (`normalize_sat_constraints`) and re-evaluated with SMT-LIB division semantics, together with
  `constraints_are_directly_unsat`, `product_monotonic_unsat_normalized` and
  `checked_xor_result`. About two million cases found no mismatch, dropped satisfying point or
  false UNSAT. It is not committed: `crates/evm/symbolic/AGENTS.md` keeps crate-level test
  harnesses out of the crate. Adding it as an exception would be the strongest drift check.

## Not modelled, and why

- Byte-level reasoning (`known_byte`, `rebuild_from_or_terms`, `byte_term`) and the word rebuilds
  from extracted bytes: a separate byte model is needed; the fuzzer exercises them.
- The checked-multiply guard recognizers (`checked_product_guard_operands`,
  `normalize_bool_selected_eq_zero`, `word_bool_always_true`), `full_mul_div` and the rounding and
  monotonic-product reasoning (`normalize/rounding.rs`, `reasoning/monotonic_product.rs`): they
  depend on `ConstraintContext` intervals, whose model is the natural next step.
- Memory, calldata, storage and call modelling are outside the expression layer.

## Next steps

Model `ConstraintContext::interval` and the bound recorders, then prove the contextual arms and
the rounding lemmas against it; model `known_byte`; replace the remaining deferred bit-vector
identities in the inventory; and decide whether the differential fuzzer may live in the crate.

## Inventory

One entry per distinct rewrite comment, by Rust file under `crates/evm/symbolic/src/runtime`. A
"proved" entry names its Lean theorem in namespace `Symbolic`.

### `expr/expr.rs`

- `a & low_mask => a`: not modelled, helper `low_masked_source`; a matcher, not a rewrite.
- `const + const => const`: proved, `rw_binop_const_const`.
- `0 + a => a`: proved, `rw_add_zero_left`.
- `a + 0 => a`: proved, `rw_add_zero_right`.
- `bool_word(c) + MAX => ite(c, 0, MAX)`: proved, `rw_add_bool_word_max` (canonical `bool_word` shape).
- `one_bit_word(c) + k => ite(c, k + 1, k)`: proved, `rw_add_bool_word_const`, with rw_one_bit_word for one-bit words.
- `(a / d) * d + a % d => a`: proved, `rw_add_euclidean_sum` (refinement: SMT-only identity).
- `(a - b) + b => a`: proved, `rw_add_sub_cancel`.
- `const - const => const`: proved, `rw_binop_const_const`.
- `a - 0 => a`: proved, `rw_sub_zero`.
- `a - a => 0`: proved, `rw_sub_self`.
- `(a + b) - a => b`: proved, `rw_sub_add_left`, rw_sub_add_right.
- `a - (a - b) => b`: proved, `rw_sub_sub_cancel`.
- `a - a % d => (a / d) * d`: proved, `rw_sub_urem` (refinement: SMT-only identity).
- `bool_word(c) - 1 => ite(c, 0, MAX)`: proved, `rw_sub_bool_word_one`.
- `k - bool_word(c) => ite(c, k - 1, k)`: proved, `rw_sub_const_bool_word`.
- `const * const => const`: proved, `rw_binop_const_const`.
- `0 * a => 0`: proved, `rw_mul_zero_left`, rw_mul_zero_right.
- `1 * a => a`: proved, `rw_mul_one_left`.
- `a * 1 => a`: proved, `rw_mul_one_right`.
- `bool_word(c) * a => ite(c, a, 0)`: proved, `rw_mul_bool_word_left`.
- `a * bool_word(c) => ite(c, a, 0)`: proved, `rw_mul_bool_word_right`.
- `a * 2**n => a << n`: proved, `rw_mul_pow2`.
- `const / const => const`: proved, `rw_binop_const_const` (EVM `x / 0 = 0` refines the undefined raw node).
- `a / 0 => 0`: proved, `rw_div_zero` (EVM-only identity; a refinement).
- `a / 1 => a`: proved, `rw_div_one`.
- `(a - (a & (2**n - 1))) u/ 2**n => a >> n`: not modelled, deferred; same toNat argument as rw_udiv_pow2.
- `a u/ 2**n => a >> n`: proved, `rw_udiv_pow2`.
- `const % const => const`: proved, `rw_binop_const_const`.
- `a % 0 => 0`: proved, `rw_rem_zero` (EVM-only identity; a refinement).
- `a % 1 => 0`: proved, `rw_rem_one`.
- `a u% 2**n => a & (2**n - 1)`: proved, `rw_urem_pow2`.
- `const & const => const`: proved, `rw_binop_const_const`.
- `0 & a => 0`: proved, `rw_and_zero` with rw_commutative.
- `MAX & a => a`: proved, `rw_and_max` with rw_commutative.
- `a & MAX => a`: proved, `rw_and_max`.
- `a & a => a`: proved, `rw_and_self`.
- `const | const => const`: proved, `rw_binop_const_const`.
- `0 | a => a`: proved, `rw_or_zero` with rw_commutative.
- `a | 0 => a`: proved, `rw_or_zero`.
- `a | a => a`: proved, `rw_or_self`.
- `const ^ const => const`: proved, `rw_binop_const_const`.
- `0 ^ a => a`: proved, `rw_xor_zero` with rw_commutative.
- `a ^ 0 => a`: proved, `rw_xor_zero`.
- `a ^ a => 0`: proved, `rw_xor_self`.
- `a ^ (a ^ b) => b`: proved, `rw_xor_shared_left`, rw_xor_shared_right.
- `a ^ ((a ^ b) * bool_word(c)) => ite(c, b, a)`: proved, `rw_xor_bool_select`.
- `a ^ ite(c, b, 0) => ite(c, a ^ b, a)`: proved, `rw_xor_zero_ite_else`, rw_xor_zero_ite_then.
- `const << const => const`: proved, `rw_binop_const_const`.
- `a << 0 => a`: proved, `rw_shl_zero`.
- `0 << a => 0`: proved, `rw_zero_shl`.
- `a << 256 => 0`: proved, `rw_shl_ge_256`.
- `const >> const => const`: proved, `rw_binop_const_const`.
- `a >> 0 => a`: proved, `rw_shr_zero`.
- `0 >> a => 0`: proved, `rw_zero_shr`.
- `const >>s const => const`: proved, `rw_binop_const_const`.
- `a >>s 0 => a`: proved, `rw_sar_zero`.
- `mul_high(const, const) => const`: proved, `rw_binop_const_const`.
- `mul_high(a, 0 | 1) => 0`: proved, `rw_mul_high_small`.
- `addmod/mulmod(a, b, 0) => 0`: proved, `rw_ternop_modulus_zero_or_one`.
- `mulmod(a, m, m) => 0`: proved, `rw_mulmod_self_left`, rw_mulmod_self_right.
- `addmod/mulmod(const, const, const) => const`: proved, `rw_ternop_const_const_const`.
- `addmod/mulmod(a, b, 2**n) => op(a, b) & (2**n - 1)`: not modelled, deferred; a `Nat.mod_mod_of_dvd` argument.
- `addmod/mulmod(a, b, m) => addmod/mulmod(ordered(a, b), m)`: proved, `rw_ternop_commutative`.
- `mul_div(const, const, const) => const`: proved, `rw_ternop_const_const_const`.
- `mul_div(0, b, d) => 0`: proved, `rw_mul_div_zero`.
- `mul_div(a, d, d) => ite(d == 0, 0, a)`: proved, `rw_mul_div_self`.
- `ite(true, a, b) => a`: proved, `rw_ite_true`.
- `ite(false, a, b) => b`: proved, `rw_ite_false`.
- `ite(c, a, a) => a`: proved, `rw_ite_same` (also the normalizer copy).
- `ite(a == 0, 0, a / a) => a != 0`: proved, `rw_ite_self_div`.
- `ite(c, 1, bool_word(c)) => bool_word(c)`: proved, `rw_ite_one_bool_word` (also the normalizer copy).
- `ite(c, bool_word(c), 0) => bool_word(c)`: proved, `rw_ite_bool_word_zero` (also the normalizer copy).
- `byte_parts(a) | byte_parts(a) => a`: not modelled, byte reassembly (`rebuild_from_or_terms`) needs a model of `known_byte` and byte terms.
- `a + b => b + a`: proved, `rw_commutative`.
- `a & 0 => 0`: proved, `rw_and_zero`.
- `const & mask => const`: proved, `rw_binop_const_const`.
- `(a | b) & mask => (a & mask) | (b & mask)`: proved, `rw_and_const_or`.
- `(a << n) & low_mask(n) => 0`: not modelled, deferred; needs a bit-level argument over a symbolic mask width.
- `(a & mask) & mask => a & mask`: proved, `rw_and_const_idem`.
- `(a & a) & mask => a & mask`: proved, `rw_and_const_self`.
- `a >> 256 => 0`: proved, `rw_shr_ge_256`.
- `small(a) >> bits(a) => 0`: proved, `rw_shr_small` via unsignedBits_sound.
- `(a << n) >> n => a`: proved, `rw_shr_shl` via unsignedBits_sound.

### `expr/bool.rs`

- `a == a => true`: proved, `rw_eq_self`.
- `const == const => const`: proved, `rw_cmp_const_const`.
- `known(a) == const => const`: not modelled, `known_byte` is not modelled.
- `a == b => ordered(a, b)`: proved, `rw_eq_commutative`.
- `const == known(a) => const`: not modelled, `known_byte` is not modelled.
- `a + c == b + c => a == b`: proved, `rw_eq_common_offset`.
- `keccak(a) == keccak(b) => false`: not modelled, modelling assumption: Keccak is injective on lengths and bytes.
- `keccak(a) == keccak(b) => len(a) == len(b) && bytes(a) == bytes(b)`: not modelled, modelling assumption: Keccak collision resistance.
- `hash(a) == hash(b) => bytes(a) == bytes(b)`: not modelled, modelling assumption: SHA-256 and RIPEMD-160 collision resistance.
- `a < a => false`: proved, `rw_ult_self`.
- `const < const => const`: proved, `rw_cmp_const_const`.
- `a < 0 => false`: proved, `rw_ult_zero`.
- `MAX < a => false`: proved, `rw_max_ult`.
- `a < a & low_mask => false`: proved, `rw_ult_masked` (any mask).
- `a > a => false`: proved, `rw_ugt_self`.
- `const > const => const`: proved, `rw_cmp_const_const`.
- `0 > a => false`: proved, `rw_zero_ugt`.
- `a > MAX => false`: proved, `rw_ugt_max`.
- `a & low_mask > a => false`: proved, `rw_masked_ugt` (any mask).
- `a <= a => true`: proved, `rw_ule_self`.
- `const <= const => const`: proved, `rw_cmp_const_const`.
- `0 <= a => true`: proved, `rw_zero_ule`.
- `a <= MAX => true`: proved, `rw_ule_max`.
- `a & low_mask <= a => true`: proved, `rw_masked_ule` (any mask).
- `a >= a => true`: proved, `rw_uge_self`.
- `const >= const => const`: proved, `rw_cmp_const_const`.
- `a >= 0 => true`: proved, `rw_uge_zero`.
- `MAX >= a => true`: proved, `rw_max_uge`.
- `a >= a & low_mask => true`: proved, `rw_uge_masked` (any mask).
- `a <s a => false`: proved, `rw_slt_self`, rw_sgt_self.
- `const <s const => const`: proved, `rw_cmp_const_const`.
- `true && a => a`: proved, `rw_and_true`.
- `false && a => false`: proved, `rw_and_false`, rw_and_false_right.
- `(a && b) && c => a && b && c`: proved, `rw_and_assoc`.
- `and() => true`: proved, by definition: the empty conjunction is `const true`.
- `and(a) => a`: proved, `rw_and_true`.
- `false || a => a`: proved, `rw_and_true` under the `or` encoding.
- `true || a => true`: proved, `rw_and_false` under the `or` encoding.
- `or() => false`: proved, by definition.
- `or(a) => a`: proved, `rw_bnot_not` under the `or` encoding.
- `a || b => !(!a && !b)`: proved, by definition (`BExpr.or`).
- `!const => const`: proved, `rw_bnot_const`, rw_not_const for words.
- `!!a => a`: proved, `rw_bnot_not`, rw_not_not for words.
- `keccak(a) == keccak(b) => bytes(a) == bytes(b)`: not modelled, modelling assumption: Keccak collision resistance.
- `!(a > k) => a <= k`: not modelled, a bound extraction (`cmp_upper_bound_usize`), not a rewrite.

### `solver/normalize/mod.rs`

- `a > b => b < a`: proved, definitional: `CmpOp.eval .ugt a b = ult b a`.
- `a >= b => b <= a`: proved, definitional.
- `a >s b => b <s a`: proved, definitional.
- `x & mask == x => true`: not modelled, contextual; needs the `ConstraintContext` interval model (soundness shape: refinesBUnder_and).
- `x & mask != x => false`: not modelled, contextual; needs the interval model.
- `always_true_word == 0 => false`: not modelled, needs the checked-multiply guard model.
- `always_true_word != 0 => true`: not modelled, needs the checked-multiply guard model.
- `x & low_mask => x`: not modelled, contextual; needs the interval model.
- `x != MAX => x <= MAX - 1`: not modelled, a bound extraction, not a rewrite.
- `n / d < k => n < k * d`: proved, `rw_udiv_lt`, given the no-wrap fact (contextual).
- `k <= n / d => k * d <= n`: proved, `rw_le_udiv`, given the no-wrap fact (contextual).
- `bool_word(c) == 1 => c`: proved, `rw_bool_word_eq_one`.
- `word_bool(c) == 0 => !c`: proved, `rw_bool_word_eq_zero`.
- `a > 0 => a != 0`: proved, `rw_ugt_zero`.
- `1 > a => a == 0`: proved, `rw_one_ugt`.
- `a >= 1 => a != 0`: proved, `rw_uge_one`.
- `0 >= a => a == 0`: proved, `rw_zero_uge`.
- `a <= 0 => a == 0`: proved, `rw_ule_zero`.
- `1 <= a => a != 0`: proved, `rw_one_ule`.
- `a < 1 => a == 0`: proved, `rw_ult_one`.
- `0 < a => a != 0`: proved, `rw_zero_ult`.
- `a * guard(x, y) == 0 => mul_high(x, y) != 0 || a == 0`: not modelled, needs the checked-multiply guard model.
- `a / b == 0 => b == 0 || a < b`: proved, `rw_udiv_eq_zero` (refinement: false under SMT `a / 0`).
- `ite(c, a, b) == 0 => (c && a == 0) || (!c && b == 0)`: proved, `rw_ite_eq_zero`.
- `a / b != 0 => b != 0 && a >= b`: proved, `rw_udiv_ne_zero`.
- `ite(c, a, b) != 0 => (c && a != 0) || (!c && b != 0)`: proved, negation of rw_ite_eq_zero by congruence.
- `c || !c => true`: not modelled, a word-level disjunction recognizer, not modelled.
- `a == 0 || guarded_mul_div(a) => true`: not modelled, needs the checked-multiply guard model.
