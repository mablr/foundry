//! Temporary legacy assertion handlers; native dispatch uses `crate::assertions` directly.

use crate::{CheatcodesExecutor, CheatsCtxt, Result, Vm::*};
use alloy_primitives::U256;
use foundry_evm_core::{
    backend::GLOBAL_FAIL_SLOT, constants::CHEATCODE_ADDRESS, evm::FoundryEvmNetwork,
};
use revm::context::{ContextTr, JournalTr};

// TODO(evm2): Delete these handlers with the legacy cheatcode dispatcher.
macro_rules! legacy_assertions {
    ($(($variant:ident, $call:ident)),* $(,)?) => {$(
        impl crate::Cheatcode for $call {
            fn apply_full<FEN: FoundryEvmNetwork>(
                &self,
                ccx: &mut CheatsCtxt<'_, '_, FEN>,
                executor: &mut dyn CheatcodesExecutor<FEN>,
            ) -> Result {
                let result = crate::assertions::evaluate(&VmCalls::$variant(self.clone())).unwrap();
                if !ccx.state.config.assertions_revert && let Err(error) = result {
                    executor.console_log(&error.to_string());
                    ccx.ecx.journal_mut().sstore(CHEATCODE_ADDRESS, GLOBAL_FAIL_SLOT, U256::from(1))?;
                    Ok(Vec::new())
                } else {
                    result
                }
            }
        }
    )*};
}

legacy_assertions! {
    (assertTrue_0, assertTrue_0Call),
    (assertTrue_1, assertTrue_1Call),
    (assertFalse_0, assertFalse_0Call),
    (assertFalse_1, assertFalse_1Call),
    (assertEq_0, assertEq_0Call),
    (assertEq_1, assertEq_1Call),
    (assertEq_2, assertEq_2Call),
    (assertEq_3, assertEq_3Call),
    (assertEq_4, assertEq_4Call),
    (assertEq_5, assertEq_5Call),
    (assertEq_6, assertEq_6Call),
    (assertEq_7, assertEq_7Call),
    (assertEq_8, assertEq_8Call),
    (assertEq_9, assertEq_9Call),
    (assertEq_10, assertEq_10Call),
    (assertEq_11, assertEq_11Call),
    (assertEq_12, assertEq_12Call),
    (assertEq_13, assertEq_13Call),
    (assertEq_14, assertEq_14Call),
    (assertEq_15, assertEq_15Call),
    (assertEq_16, assertEq_16Call),
    (assertEq_17, assertEq_17Call),
    (assertEq_18, assertEq_18Call),
    (assertEq_19, assertEq_19Call),
    (assertEq_20, assertEq_20Call),
    (assertEq_21, assertEq_21Call),
    (assertEq_22, assertEq_22Call),
    (assertEq_23, assertEq_23Call),
    (assertEq_24, assertEq_24Call),
    (assertEq_25, assertEq_25Call),
    (assertEq_26, assertEq_26Call),
    (assertEq_27, assertEq_27Call),
    (assertEqDecimal_0, assertEqDecimal_0Call),
    (assertEqDecimal_1, assertEqDecimal_1Call),
    (assertEqDecimal_2, assertEqDecimal_2Call),
    (assertEqDecimal_3, assertEqDecimal_3Call),
    (assertNotEq_0, assertNotEq_0Call),
    (assertNotEq_1, assertNotEq_1Call),
    (assertNotEq_2, assertNotEq_2Call),
    (assertNotEq_3, assertNotEq_3Call),
    (assertNotEq_4, assertNotEq_4Call),
    (assertNotEq_5, assertNotEq_5Call),
    (assertNotEq_6, assertNotEq_6Call),
    (assertNotEq_7, assertNotEq_7Call),
    (assertNotEq_8, assertNotEq_8Call),
    (assertNotEq_9, assertNotEq_9Call),
    (assertNotEq_10, assertNotEq_10Call),
    (assertNotEq_11, assertNotEq_11Call),
    (assertNotEq_12, assertNotEq_12Call),
    (assertNotEq_13, assertNotEq_13Call),
    (assertNotEq_14, assertNotEq_14Call),
    (assertNotEq_15, assertNotEq_15Call),
    (assertNotEq_16, assertNotEq_16Call),
    (assertNotEq_17, assertNotEq_17Call),
    (assertNotEq_18, assertNotEq_18Call),
    (assertNotEq_19, assertNotEq_19Call),
    (assertNotEq_20, assertNotEq_20Call),
    (assertNotEq_21, assertNotEq_21Call),
    (assertNotEq_22, assertNotEq_22Call),
    (assertNotEq_23, assertNotEq_23Call),
    (assertNotEq_24, assertNotEq_24Call),
    (assertNotEq_25, assertNotEq_25Call),
    (assertNotEq_26, assertNotEq_26Call),
    (assertNotEq_27, assertNotEq_27Call),
    (assertNotEqDecimal_0, assertNotEqDecimal_0Call),
    (assertNotEqDecimal_1, assertNotEqDecimal_1Call),
    (assertNotEqDecimal_2, assertNotEqDecimal_2Call),
    (assertNotEqDecimal_3, assertNotEqDecimal_3Call),
    (assertGt_0, assertGt_0Call),
    (assertGt_1, assertGt_1Call),
    (assertGt_2, assertGt_2Call),
    (assertGt_3, assertGt_3Call),
    (assertGtDecimal_0, assertGtDecimal_0Call),
    (assertGtDecimal_1, assertGtDecimal_1Call),
    (assertGtDecimal_2, assertGtDecimal_2Call),
    (assertGtDecimal_3, assertGtDecimal_3Call),
    (assertGe_0, assertGe_0Call),
    (assertGe_1, assertGe_1Call),
    (assertGe_2, assertGe_2Call),
    (assertGe_3, assertGe_3Call),
    (assertGeDecimal_0, assertGeDecimal_0Call),
    (assertGeDecimal_1, assertGeDecimal_1Call),
    (assertGeDecimal_2, assertGeDecimal_2Call),
    (assertGeDecimal_3, assertGeDecimal_3Call),
    (assertLt_0, assertLt_0Call),
    (assertLt_1, assertLt_1Call),
    (assertLt_2, assertLt_2Call),
    (assertLt_3, assertLt_3Call),
    (assertLtDecimal_0, assertLtDecimal_0Call),
    (assertLtDecimal_1, assertLtDecimal_1Call),
    (assertLtDecimal_2, assertLtDecimal_2Call),
    (assertLtDecimal_3, assertLtDecimal_3Call),
    (assertLe_0, assertLe_0Call),
    (assertLe_1, assertLe_1Call),
    (assertLe_2, assertLe_2Call),
    (assertLe_3, assertLe_3Call),
    (assertLeDecimal_0, assertLeDecimal_0Call),
    (assertLeDecimal_1, assertLeDecimal_1Call),
    (assertLeDecimal_2, assertLeDecimal_2Call),
    (assertLeDecimal_3, assertLeDecimal_3Call),
    (assertApproxEqAbs_0, assertApproxEqAbs_0Call),
    (assertApproxEqAbs_1, assertApproxEqAbs_1Call),
    (assertApproxEqAbs_2, assertApproxEqAbs_2Call),
    (assertApproxEqAbs_3, assertApproxEqAbs_3Call),
    (assertApproxEqAbsDecimal_0, assertApproxEqAbsDecimal_0Call),
    (assertApproxEqAbsDecimal_1, assertApproxEqAbsDecimal_1Call),
    (assertApproxEqAbsDecimal_2, assertApproxEqAbsDecimal_2Call),
    (assertApproxEqAbsDecimal_3, assertApproxEqAbsDecimal_3Call),
    (assertApproxEqRel_0, assertApproxEqRel_0Call),
    (assertApproxEqRel_1, assertApproxEqRel_1Call),
    (assertApproxEqRel_2, assertApproxEqRel_2Call),
    (assertApproxEqRel_3, assertApproxEqRel_3Call),
    (assertApproxEqRelDecimal_0, assertApproxEqRelDecimal_0Call),
    (assertApproxEqRelDecimal_1, assertApproxEqRelDecimal_1Call),
    (assertApproxEqRelDecimal_2, assertApproxEqRelDecimal_2Call),
    (assertApproxEqRelDecimal_3, assertApproxEqRelDecimal_3Call),
}
