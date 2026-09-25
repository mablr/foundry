//! Transactions collected by script broadcast cheatcodes.

use alloy_network::{Ethereum, Network};
use foundry_common::TransactionMaybeSigned;
use std::collections::VecDeque;

/// A transaction captured for later broadcast, optionally associated with a fork RPC.
#[derive(Clone, Debug)]
pub struct BroadcastableTransaction<N: Network = Ethereum> {
    /// The optional RPC URL.
    pub rpc: Option<String>,
    /// The transaction to broadcast.
    pub transaction: TransactionMaybeSigned<N>,
}

/// Transactions captured in execution order.
pub type BroadcastableTransactions<N> = VecDeque<BroadcastableTransaction<N>>;
