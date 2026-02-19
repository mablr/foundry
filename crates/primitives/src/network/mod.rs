use alloy_network::Network;

mod receipt;
mod wallet;

use alloy_provider::fillers::{
    BlobGasFiller, ChainIdFiller, GasFiller, JoinFill, NonceFiller, RecommendedFillers,
};
use alloy_rpc_types::Block;
use op_alloy_rpc_types::Transaction;

pub use receipt::*;

/// Re-export Alloy types for convenience.
pub type FoundryHeader = alloy_consensus::Header;
pub type FoundryTransactionResponse = Transaction<crate::FoundryTxEnvelope>;
pub type FoundryHeaderResponse = alloy_rpc_types::Header;
pub type FoundryBlockResponse = Block<FoundryTransactionResponse, FoundryHeaderResponse>;

/// Foundry network type.
///
/// This network type supports standard Ethereum transaction types, along with op-stack deposit and
/// Tempo transaction types.
#[derive(Debug, Clone, Copy)]
pub struct FoundryNetwork;

/// The Foundry's specific configuration of [`Network`] schema and consensus primitives.
impl Network for FoundryNetwork {
    type TxType = crate::FoundryTxType;

    type TxEnvelope = crate::FoundryTxEnvelope;

    type UnsignedTx = crate::FoundryTypedTx;

    type ReceiptEnvelope = crate::FoundryReceiptEnvelope;

    type Header = FoundryHeader;

    type TransactionRequest = crate::FoundryTransactionRequest;

    type TransactionResponse = FoundryTransactionResponse;

    type ReceiptResponse = crate::FoundryTxReceipt;

    type HeaderResponse = FoundryHeaderResponse;

    type BlockResponse = FoundryBlockResponse;
}

impl RecommendedFillers for FoundryNetwork {
    type RecommendedFillers =
        JoinFill<GasFiller, JoinFill<BlobGasFiller, JoinFill<NonceFiller, ChainIdFiller>>>;

    fn recommended_fillers() -> Self::RecommendedFillers {
        Default::default()
    }
}
