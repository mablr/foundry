//! Serializable block inputs, independent of interpreter context types.

use alloy_eips::{eip4844::fake_exponential, eip7840::BlobParams};
use alloy_primitives::{Address, B256, U256};
use serde::{Deserialize, Serialize};

/// Block header metadata and synthetic execution overrides.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockEnv {
    pub number: U256,
    pub beneficiary: Address,
    pub timestamp: U256,
    pub gas_limit: u64,
    pub basefee: u64,
    pub difficulty: U256,
    pub prevrandao: Option<B256>,
    pub slot_num: u64,
    pub blob_excess_gas_and_price: Option<BlobExcessGasAndPrice>,
}

/// Cached blob fee alongside its source header field.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlobExcessGasAndPrice {
    pub excess_blob_gas: u64,
    pub blob_gasprice: u128,
}

impl BlockEnv {
    /// Stores header excess gas and computes the corresponding blob fee.
    pub fn set_blob_excess_gas_and_price(
        &mut self,
        excess_blob_gas: u64,
        base_fee_update_fraction: u64,
    ) {
        self.blob_excess_gas_and_price = Some(BlobExcessGasAndPrice {
            excess_blob_gas,
            blob_gasprice: fake_exponential(
                1,
                excess_blob_gas as u128,
                base_fee_update_fraction as u128,
            ),
        });
    }
}

impl Default for BlockEnv {
    fn default() -> Self {
        let mut block = Self {
            number: U256::ZERO,
            beneficiary: Address::ZERO,
            timestamp: U256::ONE,
            gas_limit: u64::MAX,
            basefee: 0,
            difficulty: U256::ZERO,
            prevrandao: Some(B256::ZERO),
            slot_num: 0,
            blob_excess_gas_and_price: None,
        };
        block.set_blob_excess_gas_and_price(0, BlobParams::prague().update_fraction as u64);
        block
    }
}

/// Header inputs and synthetic block overrides used by Foundry execution.
pub trait FoundryBlock {
    /// Returns the block number.
    fn number(&self) -> U256;

    /// Returns the block beneficiary.
    fn beneficiary(&self) -> Address;

    /// Returns the block timestamp.
    fn timestamp(&self) -> U256;

    /// Returns the block gas limit.
    fn gas_limit(&self) -> u64;

    /// Returns the block basefee.
    fn basefee(&self) -> u64;

    /// Returns the block difficulty.
    fn difficulty(&self) -> U256;

    /// Returns the block prevrandao.
    fn prevrandao(&self) -> Option<B256>;

    /// Returns the block slot num.
    fn slot_num(&self) -> u64;

    /// Returns the computed blob base fee, when supplied.
    fn blob_gasprice(&self) -> Option<u128>;

    /// Sets the block number.
    fn set_number(&mut self, number: U256);

    /// Sets the slot number.
    fn set_slot_num(&mut self, slot_num: u64);

    /// Sets the beneficiary (coinbase) address.
    fn set_beneficiary(&mut self, beneficiary: Address);

    /// Sets the block timestamp.
    fn set_timestamp(&mut self, timestamp: U256);

    /// Sets the gas limit.
    fn set_gas_limit(&mut self, gas_limit: u64);

    /// Sets the base fee per gas.
    fn set_basefee(&mut self, basefee: u64);

    /// Sets the block difficulty.
    fn set_difficulty(&mut self, difficulty: U256);

    /// Sets the prevrandao value.
    fn set_prevrandao(&mut self, prevrandao: Option<B256>);

    /// Sets the excess blob gas and blob gasprice.
    fn set_blob_excess_gas_and_price(
        &mut self,
        _excess_blob_gas: u64,
        _base_fee_update_fraction: u64,
    );

    /* EVM2 migration: disabled non-Ethereum execution.
        // Tempo methods

        /// Returns the milliseconds portion of the block timestamp.
        fn timestamp_millis_part(&self) -> u64 {
            0
        }

        /// Sets the milliseconds portion of the block timestamp.
        fn set_timestamp_millis_part(&mut self, _millis: u64) {}
    */
}

impl FoundryBlock for BlockEnv {
    fn number(&self) -> U256 {
        self.number
    }
    fn beneficiary(&self) -> Address {
        self.beneficiary
    }
    fn timestamp(&self) -> U256 {
        self.timestamp
    }
    fn gas_limit(&self) -> u64 {
        self.gas_limit
    }
    fn basefee(&self) -> u64 {
        self.basefee
    }
    fn difficulty(&self) -> U256 {
        self.difficulty
    }
    fn prevrandao(&self) -> Option<B256> {
        self.prevrandao
    }
    fn slot_num(&self) -> u64 {
        self.slot_num
    }
    fn blob_gasprice(&self) -> Option<u128> {
        self.blob_excess_gas_and_price.map(|blob| blob.blob_gasprice)
    }

    fn set_number(&mut self, number: U256) {
        self.number = number;
    }

    fn set_slot_num(&mut self, slot_num: u64) {
        self.slot_num = slot_num;
    }

    fn set_beneficiary(&mut self, beneficiary: Address) {
        self.beneficiary = beneficiary;
    }

    fn set_timestamp(&mut self, timestamp: U256) {
        self.timestamp = timestamp;
    }

    fn set_gas_limit(&mut self, gas_limit: u64) {
        self.gas_limit = gas_limit;
    }

    fn set_basefee(&mut self, basefee: u64) {
        self.basefee = basefee;
    }

    fn set_difficulty(&mut self, difficulty: U256) {
        self.difficulty = difficulty;
    }

    fn set_prevrandao(&mut self, prevrandao: Option<B256>) {
        self.prevrandao = prevrandao;
    }

    fn set_blob_excess_gas_and_price(
        &mut self,
        excess_blob_gas: u64,
        base_fee_update_fraction: u64,
    ) {
        self.set_blob_excess_gas_and_price(excess_blob_gas, base_fee_update_fraction);
    }
}

// TODO(evm2): Move this compatibility implementation behind Anvil's boundary.
impl FoundryBlock for revm::context::BlockEnv {
    fn number(&self) -> U256 {
        self.number
    }
    fn beneficiary(&self) -> Address {
        self.beneficiary
    }
    fn timestamp(&self) -> U256 {
        self.timestamp
    }
    fn gas_limit(&self) -> u64 {
        self.gas_limit
    }
    fn basefee(&self) -> u64 {
        self.basefee
    }
    fn difficulty(&self) -> U256 {
        self.difficulty
    }
    fn prevrandao(&self) -> Option<B256> {
        self.prevrandao
    }
    fn slot_num(&self) -> u64 {
        self.slot_num
    }
    fn blob_gasprice(&self) -> Option<u128> {
        self.blob_excess_gas_and_price.map(|blob| blob.blob_gasprice)
    }

    fn set_number(&mut self, number: U256) {
        self.number = number;
    }

    fn set_slot_num(&mut self, slot_num: u64) {
        self.slot_num = slot_num;
    }

    fn set_beneficiary(&mut self, beneficiary: Address) {
        self.beneficiary = beneficiary;
    }

    fn set_timestamp(&mut self, timestamp: U256) {
        self.timestamp = timestamp;
    }

    fn set_gas_limit(&mut self, gas_limit: u64) {
        self.gas_limit = gas_limit;
    }

    fn set_basefee(&mut self, basefee: u64) {
        self.basefee = basefee;
    }

    fn set_difficulty(&mut self, difficulty: U256) {
        self.difficulty = difficulty;
    }

    fn set_prevrandao(&mut self, prevrandao: Option<B256>) {
        self.prevrandao = prevrandao;
    }

    fn set_blob_excess_gas_and_price(
        &mut self,
        excess_blob_gas: u64,
        base_fee_update_fraction: u64,
    ) {
        self.set_blob_excess_gas_and_price(excess_blob_gas, base_fee_update_fraction);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_cache_format_matches_existing_forks() {
        let mut legacy = revm::context::BlockEnv {
            number: U256::from(123),
            timestamp: U256::from(456),
            prevrandao: None,
            ..Default::default()
        };
        legacy.set_blob_excess_gas_and_price(123_456, BlobParams::cancun().update_fraction as u64);

        let cached = serde_json::to_value(&legacy).unwrap();
        let native = serde_json::from_value::<BlockEnv>(cached.clone()).unwrap();
        assert_eq!(serde_json::to_value(&native).unwrap(), cached);
        assert_eq!(native.blob_gasprice(), legacy.blob_gasprice());

        let default = serde_json::to_value(revm::context::BlockEnv::default()).unwrap();
        assert_eq!(serde_json::to_value(BlockEnv::default()).unwrap(), default);
    }
}
