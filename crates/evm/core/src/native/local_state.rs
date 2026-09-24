//! Copy-on-write local state for Ethereum execution.

use alloy_primitives::{Address, B256, U256, keccak256};
use evm2::{
    bytecode::Bytecode,
    evm::{AccountInfo, Database, InMemoryDB, PendingState},
};
use std::{convert::Infallible, sync::Arc};

/// Persistent local state shared by cloned Forge executors until a transaction commits.
#[derive(Clone, Debug, Default)]
pub struct LocalState(Arc<InMemoryDB>);

impl LocalState {
    /// Returns the accepted state.
    pub fn database(&self) -> &InMemoryDB {
        &self.0
    }

    /// Returns mutable accepted state, cloning it if another executor still shares it.
    pub fn database_mut(&mut self) -> &mut InMemoryDB {
        Arc::make_mut(&mut self.0)
    }

    /// Accepts a detached transaction's state changes.
    pub fn commit(&mut self, pending: &PendingState) {
        self.database_mut().commit_pending(pending);
    }
}

impl Database for &LocalState {
    type Error = Infallible;

    fn get_account(&mut self, address: &Address) -> Result<Option<AccountInfo>, Self::Error> {
        Ok(self.0.cache.accounts.get(address).cloned().flatten())
    }

    fn get_code_by_hash(&mut self, code_hash: &B256) -> Result<Bytecode, Self::Error> {
        Ok(self.0.cache.contracts.get(code_hash).cloned().unwrap_or_default())
    }

    fn get_storage(&mut self, address: &Address, key: &U256) -> Result<U256, Self::Error> {
        Ok(self
            .0
            .cache
            .storage
            .get(address)
            .and_then(|storage| storage.slots.get(key))
            .copied()
            .unwrap_or_default())
    }

    fn get_block_hash(&mut self, number: &U256) -> Result<B256, Self::Error> {
        Ok(self
            .0
            .cache
            .block_hashes
            .get(number)
            .copied()
            .unwrap_or_else(|| keccak256(number.to_string().as_bytes())))
    }
}
