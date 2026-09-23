//! Foundry execution overrides applied to native evm2 versions.

use evm2::{EvmFeatures, SpecId, Version};

/// Execution settings independent of the interpreter's context and gas-table representation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionConfig<Spec = SpecId> {
    pub spec: Spec,
    pub chain_id: u64,
    pub memory_limit: u64,
    pub limit_contract_code_size: Option<usize>,
    pub limit_contract_initcode_size: Option<usize>,
    pub tx_gas_limit_cap: Option<u64>,
    pub tx_chain_id_check: bool,
    pub disable_nonce_check: bool,
    pub disable_balance_check: bool,
    pub disable_block_gas_limit: bool,
    pub disable_base_fee: bool,
    pub disable_eip3607: bool,
    pub disable_fee_charge: bool,
    pub disable_priority_fee_check: bool,
    pub disable_eip7623: bool,
}

impl<Spec> ExecutionConfig<Spec> {
    /// Creates default execution settings for a specification.
    pub const fn new(spec: Spec) -> Self {
        Self {
            spec,
            chain_id: 1,
            memory_limit: (1 << 32) - 1,
            limit_contract_code_size: None,
            limit_contract_initcode_size: None,
            tx_gas_limit_cap: None,
            tx_chain_id_check: true,
            disable_nonce_check: false,
            disable_balance_check: false,
            disable_block_gas_limit: false,
            disable_base_fee: false,
            disable_eip3607: false,
            disable_fee_charge: false,
            disable_priority_fee_check: false,
            disable_eip7623: false,
        }
    }

    /// Changes the spec while preserving explicit overrides. Gas rules are derived at execution.
    pub fn set_spec(&mut self, spec: Spec) {
        self.spec = spec;
    }

    /// Applies explicit overrides to this specification's native rules.
    ///
    /// TODO(evm2): Use the stored spec directly after the network spec association migrates.
    pub fn version(&self, spec: SpecId) -> Version {
        let mut version = Version::new(spec);
        version.chain_id = self.chain_id;
        version.memory_limit = self.memory_limit;
        if let Some(limit) = self.tx_gas_limit_cap {
            version.tx_gas_limit_cap = limit;
        }
        if let Some(limit) = self.limit_contract_code_size {
            version.max_code_size = limit;
            version.max_initcode_size = limit.saturating_mul(2);
        }
        if let Some(limit) = self.limit_contract_initcode_size {
            version.max_initcode_size = limit;
        }
        for (feature, enabled) in [
            (EvmFeatures::NONCE_CHECK, !self.disable_nonce_check),
            (EvmFeatures::BALANCE_CHECK, !self.disable_balance_check),
            (EvmFeatures::BLOCK_GAS_LIMIT_CHECK, !self.disable_block_gas_limit),
            (EvmFeatures::BASE_FEE_CHECK, !self.disable_base_fee),
            (EvmFeatures::EIP3607, !self.disable_eip3607),
            (EvmFeatures::FEE_CHARGE, !self.disable_fee_charge),
            (EvmFeatures::PRIORITY_FEE_CHECK, !self.disable_priority_fee_check),
            (EvmFeatures::TX_CHAIN_ID_CHECK, self.tx_chain_id_check),
        ] {
            version.features.set(feature, enabled);
        }
        if self.disable_eip7623 {
            version.features.remove(EvmFeatures::EIP7623);
        }
        version
    }
}

impl<Spec: Default> Default for ExecutionConfig<Spec> {
    fn default() -> Self {
        Self::new(Spec::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_limits_follow_spec_and_preserve_overrides() {
        let mut cfg = ExecutionConfig::new(SpecId::CANCUN);
        let cancun = cfg.version(cfg.spec);
        assert_eq!(cancun.tx_gas_limit_cap, u64::MAX);
        assert_eq!(cancun.max_code_size, 24_576);
        cfg.set_spec(SpecId::AMSTERDAM);
        let amsterdam = cfg.version(cfg.spec);
        assert_eq!(amsterdam.max_code_size, 65_536);
        assert_eq!(amsterdam.max_initcode_size, 131_072);
        assert!(amsterdam.features.contains(EvmFeatures::EIP8037));
        cfg.limit_contract_code_size = Some(40_000);
        cfg.tx_gas_limit_cap = Some(1_000_000);
        assert_eq!(cfg.version(cfg.spec).max_initcode_size, 80_000);
        cfg.limit_contract_initcode_size = Some(90_000);
        cfg.set_spec(SpecId::CANCUN);
        let overridden = cfg.version(cfg.spec);
        assert_eq!(overridden.max_code_size, 40_000);
        assert_eq!(overridden.max_initcode_size, 90_000);
        assert_eq!(overridden.tx_gas_limit_cap, 1_000_000);
        cfg.limit_contract_initcode_size = None;
        cfg.limit_contract_code_size = Some(usize::MAX);
        assert_eq!(cfg.version(cfg.spec).max_initcode_size, usize::MAX);
    }

    #[test]
    fn native_validation_flags_and_resource_limits_are_applied() {
        let cfg = ExecutionConfig {
            spec: SpecId::CANCUN,
            chain_id: 42,
            memory_limit: 1024,
            disable_nonce_check: true,
            disable_balance_check: true,
            disable_block_gas_limit: true,
            disable_base_fee: true,
            disable_eip3607: true,
            disable_fee_charge: true,
            disable_priority_fee_check: true,
            disable_eip7623: true,
            tx_chain_id_check: false,
            ..Default::default()
        };
        let version = cfg.version(SpecId::PRAGUE);
        assert_eq!(version.chain_id, 42);
        assert_eq!(version.memory_limit, 1024);
        for feature in [
            EvmFeatures::NONCE_CHECK,
            EvmFeatures::BALANCE_CHECK,
            EvmFeatures::BLOCK_GAS_LIMIT_CHECK,
            EvmFeatures::BASE_FEE_CHECK,
            EvmFeatures::EIP3607,
            EvmFeatures::FEE_CHARGE,
            EvmFeatures::PRIORITY_FEE_CHECK,
            EvmFeatures::TX_CHAIN_ID_CHECK,
            EvmFeatures::EIP7623,
        ] {
            assert!(!version.features.contains(feature));
        }
    }
}
