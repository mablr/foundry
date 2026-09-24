//! Native Ethereum EVM construction.

use alloy_consensus::transaction::Recovered;
use evm2::{
    BaseEvmTypes, Evm, ExecutionConfig, Precompiles, TxResult,
    ethereum::{TxEnvelope, ethereum_tx_registry},
    evm::{Db, DynDatabase, registry::HandlerResult},
};
use foundry_evm_core::native::{EthereumEnv, LocalState};

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

/// Ethereum execution with copy-on-write local state.
#[derive(Clone, Debug)]
pub struct EthereumExecutor {
    env: EthereumEnv,
    state: LocalState,
}

impl EthereumExecutor {
    /// Creates an executor over local Ethereum state.
    pub const fn new(env: EthereumEnv, state: LocalState) -> Self {
        Self { env, state }
    }

    /// Returns the accepted state.
    pub const fn state(&self) -> &LocalState {
        &self.state
    }

    /// Returns mutable accepted state, cloning it if shared by another executor.
    pub const fn state_mut(&mut self) -> &mut LocalState {
        &mut self.state
    }

    /// Executes a transaction without accepting its state changes.
    pub fn call(&self, tx: &Recovered<TxEnvelope>) -> HandlerResult<TxResult> {
        let mut evm = EthereumFactory.create(self.env, Db::new(&self.state));
        Ok(evm.transact(tx)?.discard())
    }

    /// Executes and accepts a transaction's state changes.
    pub fn transact(&mut self, tx: &Recovered<TxEnvelope>) -> HandlerResult<TxResult> {
        let outcome = {
            let mut evm = EthereumFactory.create(self.env, Db::new(&self.state));
            evm.transact(tx)?.detach()
        };
        self.state.commit(&outcome.pending_state);
        Ok(outcome.result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_consensus::TxLegacy;
    use alloy_primitives::{Address, Bytes, TxKind, U256};
    use evm2::{SpecId, bytecode::Bytecode, env::BlockEnvExt, evm::AccountInfo};
    use foundry_compilers::artifacts::EvmVersion;
    use foundry_evm_core::opts::EvmOpts;

    #[test]
    fn executor_discards_calls_and_commits_copy_on_write_transactions() {
        let config =
            foundry_config::Config { evm_version: EvmVersion::Cancun, ..Default::default() };
        let caller = Address::with_last_byte(0xa);
        let recipient = Address::with_last_byte(0xb);
        let mut state = LocalState::default();
        state.database_mut().insert_account_info(
            &recipient,
            AccountInfo::default().with_code(Bytecode::new_legacy(Bytes::from_static(&[
                0x60, 0x01, 0x5f, 0x55, 0x46, 0x5f, 0x52, 0x60, 0x20, 0x5f, 0xf3,
            ]))),
        );
        let mut opts = EvmOpts::default();
        opts.env.chain_id = Some(31_337);
        opts.env.gas_limit = foundry_config::GasLimit(30_000_000);
        opts.memory_limit = 1_000_000;
        let env = EthereumEnv::local_from_config(&config, &opts).unwrap();
        assert_eq!(env.spec, SpecId::CANCUN);
        let mut executor = EthereumExecutor::new(env, state);
        let tx = Recovered::new_unchecked(
            TxEnvelope::Legacy(TxLegacy {
                gas_limit: 100_000,
                to: TxKind::Call(recipient),
                ..Default::default()
            }),
            caller,
        );

        let call = executor.call(&tx).unwrap();
        assert!(call.status);
        assert_eq!(U256::from_be_slice(&call.output), U256::from(31_337));
        assert!(!executor.state().database().cache.accounts.contains_key(&caller));
        assert!(!executor.state().database().cache.storage.contains_key(&recipient));

        let transaction = executor.transact(&tx).unwrap();
        assert!(transaction.status);
        assert_eq!(
            executor.state().database().cache.storage[&recipient].slots[&U256::ZERO],
            U256::ONE
        );
        let snapshot = executor.clone();
        assert!(executor.transact(&tx).unwrap().status);
        assert_eq!(executor.state().database().cache.accounts[&caller].as_ref().unwrap().nonce, 2);
        assert_eq!(snapshot.state().database().cache.accounts[&caller].as_ref().unwrap().nonce, 1);
    }

    #[test]
    fn reverted_transaction_commits_nonce_without_storage() {
        let caller = Address::with_last_byte(0xa);
        let recipient = Address::with_last_byte(0xb);
        let mut state = LocalState::default();
        state.database_mut().insert_account_info(
            &recipient,
            AccountInfo::default().with_code(Bytecode::new_legacy(Bytes::from_static(&[
                0x60, 0x01, 0x5f, 0x55, 0x5f, 0x5f, 0xfd,
            ]))),
        );
        let env = EthereumEnv::new(
            SpecId::CANCUN,
            BlockEnvExt { gas_limit: U256::from(30_000_000), ..Default::default() },
        );
        let mut executor = EthereumExecutor::new(env, state);
        let tx = Recovered::new_unchecked(
            TxEnvelope::Legacy(TxLegacy {
                gas_limit: 100_000,
                to: TxKind::Call(recipient),
                ..Default::default()
            }),
            caller,
        );

        assert!(!executor.transact(&tx).unwrap().status);
        assert_eq!(executor.state().database().cache.accounts[&caller].as_ref().unwrap().nonce, 1);
        assert!(!executor.state().database().cache.storage.contains_key(&recipient));
    }
}
