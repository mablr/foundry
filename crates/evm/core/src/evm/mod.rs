//! Shared EVM traits, associated types, and execution helpers.
//!
//! Each network module owns its network marker and environment types. Native execution is
//! constructed directly with evm2; no context or EVM factory is part of this association.

use crate::{EvmEnv, FoundryBlock, FoundryChain, FoundryTransaction, FromAnyRpcTransaction};
use alloy_consensus::{SignableTransaction, Signed, transaction::SignerRecoverable};
use alloy_network::Network;
use alloy_primitives::Signature;
use alloy_rlp::Decodable;
use foundry_common::{FoundryReceiptResponse, FoundryTransactionBuilder, fmt::UIfmt};
use foundry_config::ExecutionSpec;
use foundry_fork_db::ForkBlockEnv;
use revm::{interpreter::InstructionResult, primitives::hardfork::SpecId};
use serde::{Deserialize, Serialize};
use std::fmt::Debug;

/* EVM2 migration: disabled non-Ethereum execution.
#[cfg(feature = "base")]
pub mod base;
*/
pub mod eth;
/* EVM2 migration: disabled non-Ethereum execution.
#[cfg(feature = "monad")]
pub mod monad;
*/
/* EVM2 migration: disabled non-Ethereum execution.
#[cfg(feature = "optimism")]
pub mod op;
*/
// Disabled for the Ethereum-only EVM2 migration.
// pub mod tempo;

pub use eth::*;
// Disabled for the Ethereum-only EVM2 migration.
// pub use tempo::*;

/* EVM2 migration: disabled non-Ethereum execution.
#[cfg(feature = "base")]
pub use base::*;
*/

/* EVM2 migration: disabled non-Ethereum execution.
#[cfg(feature = "monad")]
pub use monad::*;
*/

/* EVM2 migration: disabled non-Ethereum execution.
#[cfg(feature = "optimism")]
pub use op::*;
*/

/// Associates an RPC network with Foundry execution environment and chain-context types.
pub trait FoundryEvmNetwork: Copy + Debug + Default + 'static {
    type Network: Network<
            TxEnvelope: Decodable
                            + SignerRecoverable
                            + From<Signed<<Self::Network as Network>::UnsignedTx>>
                            + for<'d> Deserialize<'d>
                            + Serialize
                            + UIfmt,
            UnsignedTx: SignableTransaction<Signature>,
            TransactionRequest: FoundryTransactionBuilder<Self::Network>
                                    + for<'d> Deserialize<'d>
                                    + Serialize,
            ReceiptResponse: FoundryReceiptResponse,
        >;
    type Spec: Into<SpecId>
        + ExecutionSpec
        + Default
        + Copy
        + Debug
        + Eq
        + std::hash::Hash
        + Unpin
        + Send
        + Sync
        + 'static;
    type Block: FoundryBlock + ForkBlockEnv + Default + Debug + Unpin;
    type Tx: Clone + Debug + FoundryTransaction + FromAnyRpcTransaction + Default + Send + Sync;

    /// Transaction-position context owned by this network.
    type Chain: FoundryChain<Self::Tx>;
}

/// Converts a network-specific halt reason into an [`InstructionResult`].
pub trait IntoInstructionResult {
    fn into_instruction_result(self) -> InstructionResult;
}

/// Convenience type aliases for accessing associated types through [`FoundryEvmNetwork`].
pub type TxEnvFor<FEN> = <FEN as FoundryEvmNetwork>::Tx;
pub type SpecFor<FEN> = <FEN as FoundryEvmNetwork>::Spec;
pub type BlockEnvFor<FEN> = <FEN as FoundryEvmNetwork>::Block;
pub type EvmEnvFor<FEN> = EvmEnv<SpecFor<FEN>, BlockEnvFor<FEN>>;
pub type NetworkFor<FEN> = <FEN as FoundryEvmNetwork>::Network;
pub type TxEnvelopeFor<FEN> = <NetworkFor<FEN> as Network>::TxEnvelope;
pub type TransactionRequestFor<FEN> = <NetworkFor<FEN> as Network>::TransactionRequest;
pub type TransactionResponseFor<FEN> = <NetworkFor<FEN> as Network>::TransactionResponse;
pub type BlockResponseFor<FEN> = <NetworkFor<FEN> as Network>::BlockResponse;

pub type ChainFor<FEN> = <FEN as FoundryEvmNetwork>::Chain;
