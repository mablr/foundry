//! Ethereum network types for native execution.
//!
//! TODO(evm2): Replace the remaining legacy transaction type and move the Anvil-only halt
//! conversion to its compatibility boundary.

use super::{FoundryEvmNetwork, IntoInstructionResult};
use crate::BlockEnv;
use alloy_network::Ethereum;
use revm::{
    context::{TxEnv, result::HaltReason},
    interpreter::InstructionResult,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct EthEvmNetwork;

impl FoundryEvmNetwork for EthEvmNetwork {
    type Network = Ethereum;
    type Spec = evm2::SpecId;
    type Block = BlockEnv;
    type Tx = TxEnv;
    type Chain = ();
}

impl IntoInstructionResult for HaltReason {
    fn into_instruction_result(self) -> InstructionResult {
        self.into()
    }
}
