//! Conversion at the remaining REVM journal boundary.

use revm::primitives::hardfork::SpecId as LegacySpecId;

/// A specification selectable for native execution and the temporary legacy fork journal.
///
/// TODO(evm2): Remove `legacy_spec` when fork journals use native state.
pub trait SpecIdConversion: Copy {
    fn native_spec(self) -> evm2::SpecId;
    fn legacy_spec(self) -> LegacySpecId;
}

impl SpecIdConversion for LegacySpecId {
    fn native_spec(self) -> evm2::SpecId {
        match self {
            Self::FRONTIER => evm2::SpecId::FRONTIER,
            Self::HOMESTEAD => evm2::SpecId::HOMESTEAD,
            Self::TANGERINE => evm2::SpecId::TANGERINE,
            Self::SPURIOUS_DRAGON => evm2::SpecId::SPURIOUS_DRAGON,
            Self::BYZANTIUM => evm2::SpecId::BYZANTIUM,
            Self::PETERSBURG => evm2::SpecId::PETERSBURG,
            Self::ISTANBUL => evm2::SpecId::ISTANBUL,
            Self::BERLIN => evm2::SpecId::BERLIN,
            Self::LONDON => evm2::SpecId::LONDON,
            Self::MERGE => evm2::SpecId::MERGE,
            Self::SHANGHAI => evm2::SpecId::SHANGHAI,
            Self::CANCUN => evm2::SpecId::CANCUN,
            Self::PRAGUE => evm2::SpecId::PRAGUE,
            Self::OSAKA => evm2::SpecId::OSAKA,
            Self::AMSTERDAM => evm2::SpecId::AMSTERDAM,
        }
    }

    fn legacy_spec(self) -> Self {
        self
    }
}

impl SpecIdConversion for evm2::SpecId {
    fn native_spec(self) -> Self {
        self
    }

    fn legacy_spec(self) -> LegacySpecId {
        match self {
            Self::FRONTIER => LegacySpecId::FRONTIER,
            Self::HOMESTEAD => LegacySpecId::HOMESTEAD,
            Self::TANGERINE => LegacySpecId::TANGERINE,
            Self::SPURIOUS_DRAGON => LegacySpecId::SPURIOUS_DRAGON,
            Self::BYZANTIUM => LegacySpecId::BYZANTIUM,
            Self::PETERSBURG => LegacySpecId::PETERSBURG,
            Self::ISTANBUL => LegacySpecId::ISTANBUL,
            Self::BERLIN => LegacySpecId::BERLIN,
            Self::LONDON => LegacySpecId::LONDON,
            Self::MERGE => LegacySpecId::MERGE,
            Self::SHANGHAI => LegacySpecId::SHANGHAI,
            Self::CANCUN => LegacySpecId::CANCUN,
            Self::PRAGUE => LegacySpecId::PRAGUE,
            Self::OSAKA => LegacySpecId::OSAKA,
            Self::AMSTERDAM => LegacySpecId::AMSTERDAM,
            _ => unreachable!("unknown native Ethereum specification"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_specs_preserve_legacy_journal_activation() {
        for spec in [
            evm2::SpecId::FRONTIER,
            evm2::SpecId::BYZANTIUM,
            evm2::SpecId::BERLIN,
            evm2::SpecId::MERGE,
            evm2::SpecId::CANCUN,
            evm2::SpecId::PRAGUE,
            evm2::SpecId::OSAKA,
            evm2::SpecId::AMSTERDAM,
        ] {
            assert_eq!(spec.legacy_spec().native_spec(), spec);
        }
    }
}
