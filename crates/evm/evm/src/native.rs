//! Native Ethereum EVM construction.

use evm2::{
    BaseEvmTypes, Evm, ExecutionConfig, Precompiles, SpecId, Version, env::BlockEnvExt,
    ethereum::ethereum_tx_registry, evm::DynDatabase,
};

/// Constructs the Ethereum execution host used by Foundry.
#[derive(Clone, Copy, Debug, Default)]
pub struct EthereumFactory;

impl EthereumFactory {
    /// Creates an EVM with Ethereum transaction handlers and precompiles for `spec`.
    pub fn create<'db>(
        self,
        spec: SpecId,
        version: Version,
        block: BlockEnvExt,
        database: impl DynDatabase + 'db,
    ) -> Evm<'db, BaseEvmTypes> {
        Evm::new_with_execution_config(
            ExecutionConfig::for_spec_and_version(spec, version),
            spec,
            block,
            ethereum_tx_registry(spec),
            database,
            Precompiles::base(spec),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_consensus::{TxLegacy, transaction::Recovered};
    use alloy_primitives::{Address, TxKind, U256};
    use evm2::{ethereum::TxEnvelope, evm::InMemoryDB};

    #[test]
    fn factory_preserves_call_and_transaction_state_boundaries() {
        let spec = SpecId::CANCUN;
        let caller = Address::with_last_byte(0xa);
        let recipient = Address::with_last_byte(0xb);
        let mut evm = EthereumFactory.create(
            spec,
            Version::new(spec),
            BlockEnvExt { gas_limit: U256::from(30_000_000), ..Default::default() },
            InMemoryDB::default(),
        );
        let tx = Recovered::new_unchecked(
            TxEnvelope::Legacy(TxLegacy {
                gas_limit: 30_000,
                to: TxKind::Call(recipient),
                ..Default::default()
            }),
            caller,
        );

        let call = evm.transact(&tx).unwrap().discard();
        assert!(call.status);
        assert!(evm.state_mut().account_info_untracked(&caller).unwrap().is_none());

        let transaction = evm.transact(&tx).unwrap().commit();
        assert!(transaction.status);
        assert_eq!(evm.state_mut().account_info_untracked(&caller).unwrap().unwrap().nonce, 1);
    }
}
