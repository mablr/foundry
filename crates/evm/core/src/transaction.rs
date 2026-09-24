//! Foundry-owned Ethereum transaction input.
//!
//! TODO(evm2): Remove the REVM transaction trait adapter when the remaining generic
//! consumers use Foundry transaction accessors directly.

use alloy_eips::{
    eip2930::AccessList,
    eip7702::{RecoveredAuthorization, SignedAuthorization},
};
use alloy_primitives::{Address, B256, Bytes, TxKind, U256};
use revm::{
    context::TxEnv,
    context_interface::{either::Either, transaction::Transaction},
};

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

impl Transaction for TransactionEnv {
    type AccessListItem<'a> = &'a alloy_eips::eip2930::AccessListItem;
    type Authorization<'a> = &'a Either<SignedAuthorization, RecoveredAuthorization>;

    fn tx_type(&self) -> u8 {
        self.tx_type
    }
    fn caller(&self) -> Address {
        self.caller
    }
    fn gas_limit(&self) -> u64 {
        self.gas_limit
    }
    fn gas_price(&self) -> u128 {
        self.gas_price
    }
    fn kind(&self) -> TxKind {
        self.kind
    }
    fn value(&self) -> U256 {
        self.value
    }
    fn input(&self) -> &Bytes {
        &self.data
    }
    fn nonce(&self) -> u64 {
        self.nonce
    }
    fn chain_id(&self) -> Option<u64> {
        self.chain_id
    }
    fn access_list(&self) -> Option<impl Iterator<Item = Self::AccessListItem<'_>>> {
        Some(self.access_list.0.iter())
    }
    fn blob_versioned_hashes(&self) -> &[B256] {
        &self.blob_hashes
    }
    fn max_fee_per_blob_gas(&self) -> u128 {
        self.max_fee_per_blob_gas
    }
    fn authorization_list_len(&self) -> usize {
        self.authorization_list.len()
    }
    fn authorization_list(&self) -> impl Iterator<Item = Self::Authorization<'_>> {
        self.authorization_list.iter()
    }
    fn max_priority_fee_per_gas(&self) -> Option<u128> {
        self.gas_priority_fee
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
