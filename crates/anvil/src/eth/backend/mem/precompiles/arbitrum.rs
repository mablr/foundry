//! Arbitrum REVM adapter.

use alloy_evm::precompiles::{DynPrecompile, PrecompileInput};
use foundry_evm_networks::arbitrum::{ARB_BLOCK_NUMBER_SELECTOR, arb_block_number_call};
use revm::precompile::{PrecompileHalt, PrecompileId, PrecompileOutput, PrecompileResult};
use std::borrow::Cow;

/// ID for the ArbSys precompile.
pub static PRECOMPILE_ID_ARB_SYS: PrecompileId = PrecompileId::Custom(Cow::Borrowed("ArbSys"));

/// Returns an ArbSys precompile for the provided L2 block number.
pub fn arb_sys_precompile(block_number: u64) -> DynPrecompile {
    DynPrecompile::new_stateful(PRECOMPILE_ID_ARB_SYS.clone(), move |input| {
        arb_sys_precompile_call(input, block_number)
    })
}

fn arb_sys_precompile_call(input: PrecompileInput<'_>, block_number: u64) -> PrecompileResult {
    if input.data.get(..4) != Some(&ARB_BLOCK_NUMBER_SELECTOR) {
        return Ok(PrecompileOutput::halt(
            PrecompileHalt::Other("unsupported ArbSys selector".into()),
            input.reservoir,
        ));
    }

    let Some((gas_cost, output)) = arb_block_number_call(input.gas, block_number) else {
        return Ok(PrecompileOutput::halt(PrecompileHalt::OutOfGas, input.reservoir));
    };

    Ok(PrecompileOutput::new(gas_cost, output, input.reservoir))
}
