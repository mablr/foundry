//! Ethereum network types for native execution.
//!
//! TODO(evm2): Replace the remaining legacy environment types and move the Anvil-only halt
//! conversion to its compatibility boundary.

use super::{FoundryEvmNetwork, IntoInstructionResult};
use alloy_network::Ethereum;
use revm::{
    context::{BlockEnv, TxEnv, result::HaltReason},
    interpreter::InstructionResult,
    primitives::hardfork::SpecId,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct EthEvmNetwork;

impl FoundryEvmNetwork for EthEvmNetwork {
    type Network = Ethereum;
    type Spec = SpecId;
    type Block = BlockEnv;
    type Tx = TxEnv;
    type Chain = ();
}

impl IntoInstructionResult for HaltReason {
    fn into_instruction_result(self) -> InstructionResult {
        self.into()
    }
}
