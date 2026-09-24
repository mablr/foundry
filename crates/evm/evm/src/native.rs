//! Native Ethereum EVM construction.

use evm2::{
    BaseEvmTypes, Evm, ExecutionConfig, Precompiles, ethereum::ethereum_tx_registry,
    evm::DynDatabase,
};
use foundry_evm_core::native::EthereumEnv;

/// Constructs the Ethereum execution host used by Foundry.
#[derive(Clone, Copy, Debug, Default)]
pub struct EthereumFactory;

impl EthereumFactory {
    /// Creates an EVM with Ethereum transaction handlers and precompiles for `env.spec`.
    pub fn create<'db>(
        self,
        env: EthereumEnv,
        database: impl DynDatabase + 'db,
    ) -> Evm<'db, BaseEvmTypes> {
        Evm::new_with_execution_config(
            ExecutionConfig::for_spec_and_version(env.spec, env.version),
            env.spec,
            env.block,
            ethereum_tx_registry(env.spec),
            database,
            Precompiles::base(env.spec),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_consensus::{TxLegacy, transaction::Recovered};
    use alloy_primitives::{Address, Bytes, TxKind, U256};
    use evm2::{
        SpecId,
        bytecode::Bytecode,
        ethereum::TxEnvelope,
        evm::{AccountInfo, InMemoryDB},
    };
    use foundry_compilers::artifacts::EvmVersion;
    use foundry_evm_core::opts::EvmOpts;

    #[test]
    fn factory_preserves_call_and_transaction_state_boundaries() {
        let config =
            foundry_config::Config { evm_version: EvmVersion::Cancun, ..Default::default() };
        let caller = Address::with_last_byte(0xa);
        let recipient = Address::with_last_byte(0xb);
        let mut database = InMemoryDB::default();
        database.insert_account_info(
            &recipient,
            AccountInfo::default().with_code(Bytecode::new_legacy(Bytes::from_static(&[
                0x46, 0x5f, 0x52, 0x60, 0x20, 0x5f, 0xf3,
            ]))),
        );
        let mut opts = EvmOpts::default();
        opts.env.chain_id = Some(31_337);
        opts.env.gas_limit = foundry_config::GasLimit(30_000_000);
        opts.memory_limit = 1_000_000;
        let env = EthereumEnv::local_from_config(&config, &opts).unwrap();
        assert_eq!(env.spec, SpecId::CANCUN);
        let mut evm = EthereumFactory.create(env, database);
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
        assert_eq!(U256::from_be_slice(&call.output), U256::from(31_337));
        assert!(evm.state_mut().account_info_untracked(&caller).unwrap().is_none());

        let transaction = evm.transact(&tx).unwrap().commit();
        assert!(transaction.status);
        assert_eq!(evm.state_mut().account_info_untracked(&caller).unwrap().unwrap().nonce, 1);
    }
}
