import Lifecycle.Contract
import Lifecycle.PersistentWrite
import Lifecycle.Restores

/-!
# Axioms of the headline theorems

Every theorem below depends on at most `propext`, `Classical.choice` and `Quot.sound`, Lean's
standard axioms; none uses `sorry` or a modelling axiom. `lake build Lifecycle.Axioms` prints them.
-/

open Lifecycle Lifecycle.St Lifecycle.PersistentWrite Lifecycle.Restores

#print axioms St.WF.run
#print axioms selectFork_carries_persistent
#print axioms selectFork_isolates
#print axioms selectFork_from_local_seeds_forks
#print axioms snapshot_spec
#print axioms revertTo_restores
#print axioms revertTo_frame
#print axioms revertTo_leaves_running
#print axioms revertTo_keeps
#print axioms rollFork_active_resets
#print axioms rollFork_active_persistent
#print axioms rollFork_inactive
#print axioms endTx_view
#print axioms createFork_fresh
#print axioms peek_view
#print axioms write_view
#print axioms applyAll_le
#print axioms accOf_absolute
#print axioms accOf_mode
#print axioms applyAll_applied
#print axioms applyAll_eq_applySeq
#print axioms applyAccount_separates
#print axioms R.Inv.bound
#print axioms R.Inv.frameEnd
#print axioms frameEnd_after_capture
#print axioms frameEnd_frames
