import Symbolic

/-! `#print axioms` for the headline theorems, listed in `Symbolic/README.md`. Not imported by the
library root; check with `lake env lean Symbolic/Axioms.lean`. -/

#print axioms Symbolic.evalK_sound
#print axioms Symbolic.unsat_transfer'
#print axioms Symbolic.refinesBUnder_and
#print axioms Symbolic.pass_sound
#print axioms Symbolic.evalK_divGuard
#print axioms Symbolic.WellGuarded.defined
#print axioms Symbolic.check_certificate_sound
#print axioms Symbolic.rw_add_euclidean_sum
#print axioms Symbolic.rw_sub_urem
#print axioms Symbolic.rw_udiv_eq_zero
#print axioms Symbolic.unsignedBits_sound
#print axioms Symbolic.signextendWord_spec
#print axioms Symbolic.evalK_signextendDynamic
#print axioms Symbolic.evalK_byteDynamic
#print axioms Symbolic.clzWord_spec
#print axioms Symbolic.evalK_expChain
#print axioms Symbolic.constrain_int_exact
#print axioms Symbolic.narrowUDiv_eq
#print axioms Symbolic.wideModular_mulDiv
#print axioms Symbolic.mulHigh_encoding
