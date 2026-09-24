use alloy_evm::precompiles::{DynPrecompile, PrecompilesMap};
use alloy_primitives::Address;
use foundry_evm::core::SpecIdConversion;
use std::fmt::Debug;

/* EVM2 migration: disabled non-Ethereum execution.
#[cfg(feature = "optimism")]
mod optimism;
*/

/// Object-safe trait that enables injecting extra precompiles when using
/// `anvil` as a library.
pub trait PrecompileFactory: Send + Sync + Unpin + Debug {
    /// Returns a set of precompiles to extend the EVM with.
    fn precompiles(&self) -> Vec<(Address, DynPrecompile)>;

    /// Installs precompiles into the EVM precompile map.
    fn install(&self, precompiles: &mut PrecompilesMap) {
        precompiles.extend_precompiles(self.precompiles());
    }
}

/// Projects only the metadata read by Foundry's shared replay normalization helpers.
/// Anvil keeps its full REVM configuration, including custom gas parameters.
pub(crate) fn foundry_replay_env(env: &alloy_evm::EvmEnv) -> foundry_evm::core::EvmEnv {
    foundry_evm::core::EvmEnv::new(
        foundry_evm::core::ExecutionConfig {
            spec: env.cfg_env.spec.native_spec(),
            chain_id: env.cfg_env.chain_id,
            disable_priority_fee_check: env.cfg_env.disable_priority_fee_check,
            ..Default::default()
        },
        foundry_block(&env.block_env),
    )
}

/// Publishes shared replay metadata changes without replacing Anvil's execution configuration.
pub(crate) fn apply_foundry_replay_env(
    env: &mut alloy_evm::EvmEnv,
    updated: foundry_evm::core::EvmEnv,
) {
    env.cfg_env.chain_id = updated.cfg_env.chain_id;
    env.cfg_env.disable_priority_fee_check = updated.cfg_env.disable_priority_fee_check;
    env.block_env = legacy_block(updated.block_env);
}

/// Copies serializable header inputs out of Anvil's execution environment.
fn foundry_block(block: &revm::context::BlockEnv) -> foundry_evm::core::BlockEnv {
    foundry_evm::core::BlockEnv {
        number: block.number,
        beneficiary: block.beneficiary,
        timestamp: block.timestamp,
        gas_limit: block.gas_limit,
        basefee: block.basefee,
        difficulty: block.difficulty,
        prevrandao: block.prevrandao,
        slot_num: block.slot_num,
        blob_excess_gas_and_price: block.blob_excess_gas_and_price.map(|blob| {
            foundry_evm::core::block::BlobExcessGasAndPrice {
                excess_blob_gas: blob.excess_blob_gas,
                blob_gasprice: blob.blob_gasprice,
            }
        }),
    }
}

/// Applies shared header normalization to an Anvil execution block.
fn legacy_block(block: foundry_evm::core::BlockEnv) -> revm::context::BlockEnv {
    revm::context::BlockEnv {
        number: block.number,
        beneficiary: block.beneficiary,
        timestamp: block.timestamp,
        gas_limit: block.gas_limit,
        basefee: block.basefee,
        difficulty: block.difficulty,
        prevrandao: block.prevrandao,
        slot_num: block.slot_num,
        blob_excess_gas_and_price: block.blob_excess_gas_and_price.map(|blob| {
            revm::context_interface::block::BlobExcessGasAndPrice {
                excess_blob_gas: blob.excess_blob_gas,
                blob_gasprice: blob.blob_gasprice,
            }
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::Infallible;

    use alloy_evm::{EthEvm, Evm, eth::EthEvmContext};
    use alloy_primitives::{Bytes, TxKind, address};
    use itertools::Itertools;
    use revm::{
        Journal,
        context::{BlockEnv, CfgEnv, Evm as RevmEvm, JournalTr, LocalContext, TxEnv},
        database::{EmptyDB, EmptyDBTyped},
        handler::{EthPrecompiles, instructions::EthInstructions},
        inspector::NoOpInspector,
        interpreter::interpreter::EthInterpreter,
        precompile::{PrecompileOutput, PrecompileSpecId, PrecompileStatus, Precompiles},
        primitives::hardfork::SpecId,
    };

    // A precompile activated in the `Prague` spec (BLS12-381 G2 map).
    pub(super) const ETH_PRAGUE_PRECOMPILE: Address =
        address!("0x0000000000000000000000000000000000000011");

    // A precompile activated in the `Osaka` spec (EIP-7951).
    const ETH_OSAKA_PRECOMPILE: Address = address!("0x0000000000000000000000000000000000000100");

    // A custom precompile address and payload for testing.
    pub(super) const PRECOMPILE_ADDR: Address =
        address!("0x0000000000000000000000000000000000000071");
    const DYNAMIC_PRECOMPILE_ADDR: Address = address!("0xdead000000000000000000000000000000000071");
    const DYNAMIC_PRECOMPILE_PREFIX: [u8; 2] = [0xde, 0xad];
    pub(super) const PAYLOAD: &[u8] = &[0xde, 0xad, 0xbe, 0xef];

    fn echo_precompile() -> DynPrecompile {
        use alloy_evm::precompiles::PrecompileInput;
        DynPrecompile::from(|input: PrecompileInput<'_>| {
            Ok(PrecompileOutput {
                status: PrecompileStatus::Success,
                bytes: Bytes::copy_from_slice(input.data),
                gas_used: 0,
                gas_refunded: 0,
                state_gas_used: 0,
                state_gas_spilled: 0,
                reservoir: input.reservoir,
            })
        })
    }

    #[derive(Debug)]
    pub(super) struct CustomPrecompileFactory;

    impl PrecompileFactory for CustomPrecompileFactory {
        fn precompiles(&self) -> Vec<(Address, DynPrecompile)> {
            vec![(PRECOMPILE_ADDR, echo_precompile())]
        }
    }

    #[derive(Debug)]
    struct DynamicLookupPrecompileFactory;

    impl PrecompileFactory for DynamicLookupPrecompileFactory {
        fn precompiles(&self) -> Vec<(Address, DynPrecompile)> {
            Vec::new()
        }

        fn install(&self, precompiles: &mut PrecompilesMap) {
            precompiles.set_precompile_lookup(|address: &Address| {
                address.as_slice().starts_with(&DYNAMIC_PRECOMPILE_PREFIX).then(echo_precompile)
            });
        }
    }

    /// Creates a new Eth EVM instance.
    fn create_eth_evm(
        spec: SpecId,
    ) -> (TxEnv, EthEvm<EmptyDBTyped<Infallible>, NoOpInspector, PrecompilesMap>) {
        let tx_env = TxEnv {
            kind: TxKind::Call(PRECOMPILE_ADDR),
            data: PAYLOAD.into(),
            ..Default::default()
        };

        let eth_evm_context = EthEvmContext {
            journaled_state: Journal::new(EmptyDB::default()),
            block: BlockEnv::default(),
            cfg: CfgEnv::new_with_spec(spec),
            tx: tx_env.clone(),
            chain: (),
            local: LocalContext::default(),
            error: Ok(()),
        };

        let eth_precompiles = EthPrecompiles {
            precompiles: Precompiles::new(PrecompileSpecId::from_spec_id(spec)),
            spec,
        }
        .precompiles;
        let eth_evm = EthEvm::new(
            RevmEvm::new_with_inspector(
                eth_evm_context,
                NoOpInspector,
                EthInstructions::<EthInterpreter, EthEvmContext<EmptyDB>>::new_mainnet_with_spec(
                    spec,
                ),
                PrecompilesMap::from_static(eth_precompiles),
            ),
            true,
        );

        (tx_env, eth_evm)
    }

    #[test]
    fn build_eth_evm_with_extra_precompiles_osaka_spec() {
        let (tx_env, mut evm) = create_eth_evm(SpecId::OSAKA);

        assert!(evm.precompiles().addresses().contains(&ETH_OSAKA_PRECOMPILE));
        assert!(evm.precompiles().addresses().contains(&ETH_PRAGUE_PRECOMPILE));
        assert!(!evm.precompiles().addresses().contains(&PRECOMPILE_ADDR));

        CustomPrecompileFactory.install(evm.precompiles_mut());

        assert!(evm.precompiles().addresses().contains(&PRECOMPILE_ADDR));

        let result = evm.transact(tx_env).unwrap();
        assert!(result.result.is_success());
        assert_eq!(result.result.output(), Some(&PAYLOAD.into()));
    }

    #[test]
    fn build_eth_evm_with_extra_precompiles_london_spec() {
        let (tx_env, mut evm) = create_eth_evm(SpecId::LONDON);

        assert!(!evm.precompiles().addresses().contains(&ETH_OSAKA_PRECOMPILE));
        assert!(!evm.precompiles().addresses().contains(&ETH_PRAGUE_PRECOMPILE));
        assert!(!evm.precompiles().addresses().contains(&PRECOMPILE_ADDR));

        CustomPrecompileFactory.install(evm.precompiles_mut());

        assert!(evm.precompiles().addresses().contains(&PRECOMPILE_ADDR));

        let result = evm.transact(tx_env).unwrap();
        assert!(result.result.is_success());
        assert_eq!(result.result.output(), Some(&PAYLOAD.into()));
    }

    #[test]
    fn build_eth_evm_with_extra_precompiles_prague_spec() {
        let (tx_env, mut evm) = create_eth_evm(SpecId::PRAGUE);

        assert!(!evm.precompiles().addresses().contains(&ETH_OSAKA_PRECOMPILE));
        assert!(evm.precompiles().addresses().contains(&ETH_PRAGUE_PRECOMPILE));
        assert!(!evm.precompiles().addresses().contains(&PRECOMPILE_ADDR));

        CustomPrecompileFactory.install(evm.precompiles_mut());

        assert!(evm.precompiles().addresses().contains(&PRECOMPILE_ADDR));

        let result = evm.transact(tx_env).unwrap();
        assert!(result.result.is_success());
        assert_eq!(result.result.output(), Some(&PAYLOAD.into()));
    }

    #[test]
    fn factory_install_supports_dynamic_lookup() {
        let (mut tx_env, mut evm) = create_eth_evm(SpecId::PRAGUE);
        tx_env.kind = TxKind::Call(DYNAMIC_PRECOMPILE_ADDR);

        assert!(!evm.precompiles().addresses().contains(&DYNAMIC_PRECOMPILE_ADDR));
        assert!(evm.precompiles().get(&DYNAMIC_PRECOMPILE_ADDR).is_none());

        DynamicLookupPrecompileFactory.install(evm.precompiles_mut());

        assert!(!evm.precompiles().addresses().contains(&DYNAMIC_PRECOMPILE_ADDR));
        assert!(evm.precompiles().get(&DYNAMIC_PRECOMPILE_ADDR).is_some());

        let result = evm.transact(tx_env).unwrap();
        assert!(result.result.is_success());
        assert_eq!(result.result.output(), Some(&PAYLOAD.into()));
    }

    #[test]
    fn replay_metadata_preserves_legacy_execution_configuration() {
        let mut env = alloy_evm::EvmEnv::default();
        env.cfg_env.gas_params =
            revm::context_interface::cfg::GasParams::new_spec(SpecId::HOMESTEAD);
        env.cfg_env.limit_contract_code_size = Some(40_000);
        env.cfg_env.disable_fee_charge = true;
        let mut expected = env.cfg_env.clone();
        let mut replay = foundry_replay_env(&env);
        replay.cfg_env.chain_id = 42;
        replay.cfg_env.disable_priority_fee_check = true;
        replay.block_env.number = alloy_primitives::U256::from(12);
        apply_foundry_replay_env(&mut env, replay);
        expected.chain_id = 42;
        expected.disable_priority_fee_check = true;
        assert_eq!(env.cfg_env, expected);
        assert_eq!(env.block_env.number, alloy_primitives::U256::from(12));
    }
}
