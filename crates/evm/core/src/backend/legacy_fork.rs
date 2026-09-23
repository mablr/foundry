//! Compatibility for legacy fork caches while their consumers migrate to evm2.
//!
//! TODO(evm2): Delete this adapter with the REVM fork cache. The RPC database itself is native.

use super::{DatabaseError, DatabaseResult};
use alloy_network::{AnyNetwork, Network};
use alloy_primitives::{Address, B256, U256};
use foundry_fork_db::{ForkBlockEnv, SharedBackend};
use revm::{bytecode::Bytecode, context::BlockEnv, database::DatabaseRef, state::AccountInfo};
use std::ops::{Deref, DerefMut};

/// Adapts the native RPC reader for remaining REVM cache consumers.
#[derive(Clone, Debug)]
pub struct LegacyForkDb<N: Network = AnyNetwork, B: ForkBlockEnv = BlockEnv>(
    pub SharedBackend<N, B>,
);

impl<N: Network, B: ForkBlockEnv> Deref for LegacyForkDb<N, B> {
    type Target = SharedBackend<N, B>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<N: Network, B: ForkBlockEnv> DerefMut for LegacyForkDb<N, B> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<N: Network, B: ForkBlockEnv> DatabaseRef for LegacyForkDb<N, B> {
    type Error = DatabaseError;

    fn basic_ref(&self, address: Address) -> DatabaseResult<Option<AccountInfo>> {
        Ok(self.account(address)?.map(to_legacy_account))
    }

    fn code_by_hash_ref(&self, hash: B256) -> DatabaseResult<Bytecode> {
        Err(foundry_fork_db::DatabaseError::MissingCode(hash).into())
    }

    fn storage_ref(&self, address: Address, key: U256) -> DatabaseResult<U256> {
        self.storage_at(address, key).map_err(Into::into)
    }

    fn block_hash_ref(&self, number: u64) -> DatabaseResult<B256> {
        self.block_hash(number).map_err(Into::into)
    }
}

/// Converts metadata at the legacy cache boundary.
pub fn to_legacy_account(info: evm2::evm::AccountInfo) -> AccountInfo {
    AccountInfo {
        balance: info.balance,
        nonce: info.nonce,
        code_hash: info.code_hash,
        code: info.code.map(|code| Bytecode::new_raw(code.original_bytes())),
        ..Default::default()
    }
}

/// Converts metadata when restoring a legacy snapshot of the RPC cache.
pub fn to_native_account(info: AccountInfo) -> evm2::evm::AccountInfo {
    evm2::evm::AccountInfo {
        balance: info.balance,
        nonce: info.nonce,
        code_hash: info.code_hash,
        code: info.code.map(|code| evm2::bytecode::Bytecode::new_raw(code.original_bytes())),
        ..Default::default()
    }
}
