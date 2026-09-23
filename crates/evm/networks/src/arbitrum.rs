//! Minimal Arbitrum system contract compatibility helpers.

use alloy_chains::Chain;
use alloy_primitives::{Address, Bytes, U256, address, hex};

/// ArbSys system contract address.
pub const ARB_SYS_ADDRESS: Address = address!("0000000000000000000000000000000000000064");

/// `ArbSys.arbBlockNumber()` selector.
pub const ARB_BLOCK_NUMBER_SELECTOR: [u8; 4] = hex!("a3b1b31d");

/// Gas charged by Nitro for returning the 32-byte `arbBlockNumber()` result.
pub const ARB_BLOCK_NUMBER_GAS_COST: u64 = 3;

/// Returns whether `chain_id` is an Arbitrum chain.
pub fn is_arbitrum_chain(chain_id: u64) -> bool {
    Chain::from_id(chain_id).is_arbitrum()
}

/// Returns the ABI-encoded result for `ArbSys.arbBlockNumber()`.
pub fn arb_block_number_output(block_number: u64) -> Bytes {
    Bytes::copy_from_slice(&U256::from(block_number).to_be_bytes::<32>())
}

/// Returns the gas cost and ABI-encoded result for `ArbSys.arbBlockNumber()`.
pub fn arb_block_number_call(gas_limit: u64, block_number: u64) -> Option<(u64, Bytes)> {
    (gas_limit >= ARB_BLOCK_NUMBER_GAS_COST)
        .then(|| (ARB_BLOCK_NUMBER_GAS_COST, arb_block_number_output(block_number)))
}
