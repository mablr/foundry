//! Legacy network precompile adapters owned by Anvil's REVM execution.
//!
//! TODO(evm2): Port these adapters when Anvil migrates to native execution.

use alloy_evm::precompiles::{DynPrecompile, PrecompilesMap};
use alloy_primitives::{Address, ChainId, address};
use revm::precompile::{
    Precompile as RevmPrecompile,
    secp256r1::{P256VERIFY, P256VERIFY_OSAKA},
};

pub(super) mod arbitrum;
pub(super) mod celo;

/// BSC secp256r1 precompile address introduced by the Haber hardfork.
const BSC_P256_ADDRESS: Address = address!("0000000000000000000000000000000000000100");

const BSC_MAINNET_CHAIN_ID: u64 = 56;
const BSC_TESTNET_CHAIN_ID: u64 = 97;
const BSC_MAINNET_HABER_TIMESTAMP: u64 = 1_718_863_500;
const BSC_TESTNET_HABER_TIMESTAMP: u64 = 1_716_962_820;
const BSC_MAINNET_OSAKA_TIMESTAMP: u64 = 1_777_343_400;
const BSC_TESTNET_OSAKA_TIMESTAMP: u64 = 1_774_319_400;

/// Applies the BSC P256 precompile active at the given timestamp.
pub fn apply_bsc_p256_precompile(
    precompiles: &mut PrecompilesMap,
    chain_id: ChainId,
    timestamp: u64,
) {
    let Some(p256verify) = bsc_p256_precompile(chain_id, timestamp) else { return };
    precompiles.apply_precompile(&BSC_P256_ADDRESS, move |_| {
        p256verify.map(|p256verify| {
            DynPrecompile::new(p256verify.id().clone(), move |input| {
                p256verify.execute(input.data, input.gas, input.reservoir)
            })
        })
    });
}

/// Returns the BSC P256 precompile for the given timestamp. The outer option distinguishes BSC
/// chains from unrelated chains, while the inner option disables P256 before Haber.
const fn bsc_p256_precompile(chain_id: ChainId, timestamp: u64) -> Option<Option<RevmPrecompile>> {
    let (haber_timestamp, osaka_timestamp) = match chain_id {
        BSC_MAINNET_CHAIN_ID => (BSC_MAINNET_HABER_TIMESTAMP, BSC_MAINNET_OSAKA_TIMESTAMP),
        BSC_TESTNET_CHAIN_ID => (BSC_TESTNET_HABER_TIMESTAMP, BSC_TESTNET_OSAKA_TIMESTAMP),
        _ => return None,
    };

    if timestamp < haber_timestamp {
        Some(None)
    } else if timestamp < osaka_timestamp {
        Some(Some(P256VERIFY))
    } else {
        Some(Some(P256VERIFY_OSAKA))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use revm::precompile::{
        Precompiles,
        secp256r1::{P256VERIFY_BASE_GAS_FEE, P256VERIFY_BASE_GAS_FEE_OSAKA},
    };

    fn bsc_p256_gas_used(chain_id: ChainId, timestamp: u64) -> Option<u64> {
        bsc_p256_precompile(chain_id, timestamp)
            .flatten()
            .map(|precompile| precompile.execute(&[], u64::MAX, 0).unwrap().gas_used)
    }

    fn assert_bsc_p256_boundaries(chain_id: ChainId, haber_timestamp: u64, osaka_timestamp: u64) {
        assert!(matches!(bsc_p256_precompile(chain_id, haber_timestamp - 1), Some(None)));
        assert_eq!(bsc_p256_gas_used(chain_id, haber_timestamp), Some(P256VERIFY_BASE_GAS_FEE));
        assert_eq!(bsc_p256_gas_used(chain_id, osaka_timestamp - 1), Some(P256VERIFY_BASE_GAS_FEE));
        assert_eq!(
            bsc_p256_gas_used(chain_id, osaka_timestamp),
            Some(P256VERIFY_BASE_GAS_FEE_OSAKA)
        );
    }

    #[test]
    fn selects_bsc_p256_at_mainnet_boundaries() {
        assert_bsc_p256_boundaries(
            BSC_MAINNET_CHAIN_ID,
            BSC_MAINNET_HABER_TIMESTAMP,
            BSC_MAINNET_OSAKA_TIMESTAMP,
        );
    }

    #[test]
    fn selects_bsc_p256_at_testnet_boundaries() {
        assert_bsc_p256_boundaries(
            BSC_TESTNET_CHAIN_ID,
            BSC_TESTNET_HABER_TIMESTAMP,
            BSC_TESTNET_OSAKA_TIMESTAMP,
        );
    }

    #[test]
    fn removes_bsc_p256_before_haber() {
        let mut precompiles = PrecompilesMap::from_static(Precompiles::osaka());
        assert!(precompiles.get(&BSC_P256_ADDRESS).is_some());
        apply_bsc_p256_precompile(
            &mut precompiles,
            BSC_MAINNET_CHAIN_ID,
            BSC_MAINNET_HABER_TIMESTAMP - 1,
        );
        assert!(precompiles.get(&BSC_P256_ADDRESS).is_none());
    }
}
