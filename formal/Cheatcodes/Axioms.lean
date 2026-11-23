import Cheatcodes.Approx
import Cheatcodes.Bound
import Cheatcodes.ExpectCall
import Cheatcodes.ExpectEmit
import Cheatcodes.Mock

/-!
# Axioms of the headline theorems

Every headline theorem depends only on Lean's standard axioms (`propext`, `Classical.choice`,
`Quot.sound`); none uses `sorry`, `Lean.ofReduceBool` or a modelling axiom.
-/

#print axioms Cheatcodes.ExpectEmit.run_spec
#print axioms Cheatcodes.ExpectEmit.step_found_iff
#print axioms Cheatcodes.ExpectEmit.Sat_count_one_iff
#print axioms Cheatcodes.ExpectEmit.old_misses_two_emitters
#print axioms Cheatcodes.Bound.uBound_some
#print axioms Cheatcodes.Bound.uBound_surjective
#print axioms Cheatcodes.Bound.iBound_some
#print axioms Cheatcodes.Bound.iBound_surjective
#print axioms Cheatcodes.Bound.oldUBound_unreachable
#print axioms Cheatcodes.Mock.findMock_spec
#print axioms Cheatcodes.Mock.eq_of_not_better
#print axioms Cheatcodes.Mock.findMock_exact
#print axioms Cheatcodes.Mock.findMock_no_value
#print axioms Cheatcodes.ExpectCall.observe_counts
#print axioms Cheatcodes.ExpectCall.repeated_filters_dropped
#print axioms Cheatcodes.Approx.getDeltaInt_eq
#print axioms Cheatcodes.Approx.intRel_eq_spec
#print axioms Cheatcodes.Approx.uintRel_eq_spec
#print axioms Cheatcodes.Approx.intAbs_iff
