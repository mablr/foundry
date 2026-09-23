//! Native evm2 hardfork selection for Foundry configuration.
//!
//! TODO(evm2): Remove the parallel REVM implementations after environment and Anvil migration.

use crate::{EthereumHardfork, ExecutionSpec, FoundryHardfork, FromEvmVersion};
use evm2::SpecId;
use foundry_compilers::artifacts::EvmVersion;
use std::str::FromStr;

impl From<FoundryHardfork> for SpecId {
    fn from(fork: FoundryHardfork) -> Self {
        match fork {
            FoundryHardfork::Ethereum(hardfork) => spec_id_from_ethereum_hardfork(hardfork),
            // Preserve the Ethereum base rules used for Tempo metadata. This does not enable
            // native Tempo execution, which must still be rejected by the executor.
            FoundryHardfork::Tempo(_) => Self::OSAKA,
        }
    }
}

impl FromEvmVersion for SpecId {
    fn from_evm_version(version: EvmVersion) -> Self {
        match version {
            EvmVersion::Homestead => Self::HOMESTEAD,
            EvmVersion::TangerineWhistle => Self::TANGERINE,
            EvmVersion::SpuriousDragon => Self::SPURIOUS_DRAGON,
            EvmVersion::Byzantium => Self::BYZANTIUM,
            EvmVersion::Constantinople => Self::PETERSBURG,
            EvmVersion::Petersburg => Self::PETERSBURG,
            EvmVersion::Istanbul => Self::ISTANBUL,
            EvmVersion::Berlin => Self::BERLIN,
            EvmVersion::London => Self::LONDON,
            EvmVersion::Paris => Self::MERGE,
            EvmVersion::Shanghai => Self::SHANGHAI,
            EvmVersion::Cancun => Self::CANCUN,
            EvmVersion::Prague => Self::PRAGUE,
            EvmVersion::Osaka => Self::OSAKA,
            EvmVersion::Amsterdam => Self::AMSTERDAM,
        }
    }
}

impl ExecutionSpec for SpecId {
    // Returns the user-facing name for the active execution spec.
    fn evm_version_name(&self) -> String {
        match self {
            Self::FRONTIER => "Frontier",
            Self::HOMESTEAD => "Homestead",
            Self::TANGERINE => "Tangerine",
            Self::SPURIOUS_DRAGON => "Spurious",
            Self::BYZANTIUM => "Byzantium",
            Self::PETERSBURG => "Petersburg",
            Self::ISTANBUL => "Istanbul",
            Self::BERLIN => "Berlin",
            Self::LONDON => "London",
            Self::MERGE => "Merge",
            Self::SHANGHAI => "Shanghai",
            Self::CANCUN => "Cancun",
            Self::PRAGUE => "Prague",
            Self::OSAKA => "Osaka",
            Self::AMSTERDAM => "Amsterdam",
            _ => unreachable!("unknown native spec"),
        }
        .to_owned()
    }

    // Parses an unnamespaced Ethereum hardfork name.
    fn from_network_hardfork(hardfork: &str) -> Option<Self> {
        EthereumHardfork::from_str(hardfork).ok().map(spec_id_from_ethereum_hardfork)
    }

    // Converts only Ethereum namespaced hardforks to an Ethereum spec.
    fn from_foundry_hardfork(hardfork: FoundryHardfork) -> Option<Self> {
        match hardfork {
            FoundryHardfork::Ethereum(hardfork) => Some(spec_id_from_ethereum_hardfork(hardfork)),
            _ => None,
        }
    }

    fn fork_hardfork(
        _chain_id: u64,
        _timestamp: u64,
        _endpoint_hardfork: Option<FoundryHardfork>,
    ) -> Option<FoundryHardfork> {
        None
    }
}

fn spec_id_from_ethereum_hardfork(hardfork: EthereumHardfork) -> SpecId {
    match hardfork {
        EthereumHardfork::Frontier => SpecId::FRONTIER,
        EthereumHardfork::Homestead => SpecId::HOMESTEAD,
        EthereumHardfork::Dao => SpecId::HOMESTEAD,
        EthereumHardfork::Tangerine => SpecId::TANGERINE,
        EthereumHardfork::SpuriousDragon => SpecId::SPURIOUS_DRAGON,
        EthereumHardfork::Byzantium => SpecId::BYZANTIUM,
        EthereumHardfork::Constantinople => SpecId::PETERSBURG,
        EthereumHardfork::Petersburg => SpecId::PETERSBURG,
        EthereumHardfork::Istanbul => SpecId::ISTANBUL,
        EthereumHardfork::MuirGlacier => SpecId::ISTANBUL,
        EthereumHardfork::Berlin => SpecId::BERLIN,
        EthereumHardfork::London => SpecId::LONDON,
        EthereumHardfork::ArrowGlacier => SpecId::LONDON,
        EthereumHardfork::GrayGlacier => SpecId::LONDON,
        EthereumHardfork::Paris => SpecId::MERGE,
        EthereumHardfork::Shanghai => SpecId::SHANGHAI,
        EthereumHardfork::Cancun => SpecId::CANCUN,
        EthereumHardfork::Prague => SpecId::PRAGUE,
        EthereumHardfork::Osaka => SpecId::OSAKA,
        EthereumHardfork::Bpo1 | EthereumHardfork::Bpo2 => SpecId::OSAKA,
        EthereumHardfork::Bpo3 | EthereumHardfork::Bpo4 | EthereumHardfork::Bpo5 => {
            unimplemented!()
        }
        EthereumHardfork::Amsterdam => SpecId::AMSTERDAM,
        f => unreachable!("unimplemented {}", f),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use revm::primitives::hardfork::SpecId as LegacySpec;

    #[test]
    fn native_compiler_versions_match_existing_selection() {
        for version in [
            EvmVersion::Homestead,
            EvmVersion::TangerineWhistle,
            EvmVersion::SpuriousDragon,
            EvmVersion::Byzantium,
            EvmVersion::Constantinople,
            EvmVersion::Petersburg,
            EvmVersion::Istanbul,
            EvmVersion::Berlin,
            EvmVersion::London,
            EvmVersion::Paris,
            EvmVersion::Shanghai,
            EvmVersion::Cancun,
            EvmVersion::Prague,
            EvmVersion::Osaka,
            EvmVersion::Amsterdam,
        ] {
            let native = SpecId::from_evm_version(version);
            let legacy = LegacySpec::from_evm_version(version);
            assert_eq!(format!("{native:?}"), format!("{legacy:?}"));
            assert_eq!(native.evm_version_name(), legacy.evm_version_name());
        }
    }

    #[test]
    fn native_hardfork_selection_preserves_aliases_and_namespace() {
        for hardfork in [
            EthereumHardfork::Frontier,
            EthereumHardfork::Dao,
            EthereumHardfork::Constantinople,
            EthereumHardfork::MuirGlacier,
            EthereumHardfork::ArrowGlacier,
            EthereumHardfork::GrayGlacier,
            EthereumHardfork::Paris,
            EthereumHardfork::Cancun,
            EthereumHardfork::Prague,
            EthereumHardfork::Osaka,
            EthereumHardfork::Bpo1,
            EthereumHardfork::Bpo2,
            EthereumHardfork::Amsterdam,
        ] {
            let fork = FoundryHardfork::Ethereum(hardfork);
            let native = SpecId::from(fork);
            assert_eq!(native.evm_version_name(), LegacySpec::from(fork).evm_version_name());
            assert_eq!(SpecId::from_foundry_hardfork(fork), Some(native));
        }
        assert_eq!(crate::evm_spec_id_from_str::<SpecId>("ethereum:cancun"), Some(SpecId::CANCUN));
        assert!(
            SpecId::from_foundry_hardfork(FoundryHardfork::Tempo(crate::TempoHardfork::T3))
                .is_none()
        );
        assert_eq!(SpecId::fork_hardfork(1, u64::MAX, None), None);
    }
}
