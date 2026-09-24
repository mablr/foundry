//! Foundry-owned Ethereum transaction input.
//!
//! Legacy transaction parity is checked in tests while the backend migrates.

use alloy_eips::{
    eip2930::AccessList,
    eip7702::{RecoveredAuthorization, SignedAuthorization},
};
use alloy_primitives::{Address, B256, Bytes, TxKind, U256};
use revm::context_interface::either::Either;

#[cfg(test)]
use revm::context::TxEnv;

/// Execution fields shared by Foundry's Ethereum transaction workflows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransactionEnv {
    pub tx_type: u8,
    pub caller: Address,
    pub gas_limit: u64,
    pub gas_price: u128,
    pub kind: TxKind,
    pub value: U256,
    pub data: Bytes,
    pub nonce: u64,
    pub chain_id: Option<u64>,
    pub access_list: AccessList,
    pub gas_priority_fee: Option<u128>,
    pub blob_hashes: Vec<B256>,
    pub max_fee_per_blob_gas: u128,
    pub authorization_list: Vec<Either<SignedAuthorization, RecoveredAuthorization>>,
}

impl Default for TransactionEnv {
    fn default() -> Self {
        Self {
            tx_type: 0,
            caller: Address::ZERO,
            gas_limit: 16_777_216,
            gas_price: 0,
            kind: TxKind::Call(Address::ZERO),
            value: U256::ZERO,
            data: Bytes::new(),
            nonce: 0,
            chain_id: Some(1),
            access_list: AccessList::default(),
            gas_priority_fee: None,
            blob_hashes: Vec::new(),
            max_fee_per_blob_gas: 0,
            authorization_list: Vec::new(),
        }
    }
}

#[cfg(test)]
impl From<TxEnv> for TransactionEnv {
    fn from(tx: TxEnv) -> Self {
        Self {
            tx_type: tx.tx_type,
            caller: tx.caller,
            gas_limit: tx.gas_limit,
            gas_price: tx.gas_price,
            kind: tx.kind,
            value: tx.value,
            data: tx.data,
            nonce: tx.nonce,
            chain_id: tx.chain_id,
            access_list: tx.access_list,
            gas_priority_fee: tx.gas_priority_fee,
            blob_hashes: tx.blob_hashes,
            max_fee_per_blob_gas: tx.max_fee_per_blob_gas,
            authorization_list: tx.authorization_list,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_matches_legacy_execution_input() {
        assert_eq!(TransactionEnv::default(), TxEnv::default().into());
    }
}
