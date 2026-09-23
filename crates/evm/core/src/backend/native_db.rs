//! Native persistent state for nonforked execution.
//!
//! TODO(evm2): Remove the legacy database traits once configuration and fork consumers are native.

use super::{DatabaseError, FoundryEvmInMemoryDB};
use crate::state_changes::StateChangeset;
use alloy_primitives::{Address, B256, U256, keccak256, map::AddressMap};
use evm2::{
    bytecode::Bytecode as NativeBytecode,
    evm::{AccountInfo as NativeAccount, CacheDB},
};
use revm::{
    Database, DatabaseCommit,
    bytecode::Bytecode,
    database::DatabaseRef,
    state::{Account, AccountInfo},
};
use std::ops::{Deref, DerefMut};

/// The authoritative local state; no REVM cache is retained alongside it.
#[derive(Clone, Debug, Default)]
pub struct NativeDb(CacheDB);

impl NativeDb {
    pub fn account(&self, address: Address) -> Option<NativeAccount> {
        self.cache.accounts.get(&address).cloned().unwrap_or_else(|| Some(NativeAccount::default()))
    }

    pub fn code(&self, hash: B256) -> NativeBytecode {
        self.cache.contracts.get(&hash).cloned().unwrap_or_default()
    }

    pub fn slot(&self, address: Address, key: U256) -> U256 {
        self.cache
            .storage
            .get(&address)
            .and_then(|storage| storage.slots.get(&key))
            .copied()
            .unwrap_or_default()
    }

    pub fn commit_native(&mut self, changes: StateChangeset) {
        for (address, change) in changes {
            if change.deleted {
                self.cache.accounts.insert(address, None);
                self.cache.storage.entry(address).or_default().wipe();
                continue;
            }
            if change.created || change.storage_wiped {
                self.cache.storage.entry(address).or_default().wipe();
            }
            self.0.insert_account_info(&address, change.info);
            let storage = &mut self.cache.storage.entry(address).or_default().slots;
            storage.extend(change.storage.into_iter().map(|(key, slot)| (key, slot.present_value)));
        }
    }

    pub fn insert_account_info(&mut self, address: Address, info: AccountInfo) {
        self.0.insert_account_info(
            &address,
            NativeAccount {
                balance: info.balance,
                nonce: info.nonce,
                code_hash: info.code_hash,
                code: info.code.map(|code| NativeBytecode::new_raw(code.original_bytes())),
                ..Default::default()
            },
        );
    }

    pub fn insert_account_storage(
        &mut self,
        address: Address,
        key: U256,
        value: U256,
    ) -> Result<(), DatabaseError> {
        self.cache.accounts.entry(address).or_insert_with(|| Some(NativeAccount::default()));
        self.0.insert_account_storage(&address, &key, &value);
        Ok(())
    }

    pub fn replace_account_storage(
        &mut self,
        address: Address,
        storage: impl IntoIterator<Item = (U256, U256)>,
    ) -> Result<(), DatabaseError> {
        self.cache.accounts.entry(address).or_insert_with(|| Some(NativeAccount::default()));
        let cached = self.cache.storage.entry(address).or_default();
        cached.wipe();
        cached.slots.extend(storage);
        Ok(())
    }

    /// Temporary export for legacy fork transfer and fuzz dictionary seeding.
    ///
    /// TODO(evm2): Make these consumers read native accounts directly and delete this export.
    pub fn legacy_snapshot(&self) -> FoundryEvmInMemoryDB {
        let mut result = FoundryEvmInMemoryDB::default();
        for (&address, info) in &self.cache.accounts {
            let account = result.cache.accounts.entry(address).or_default();
            if info.is_some() {
                account.info = self.basic_ref(address).unwrap().unwrap();
            } else {
                account.account_state = revm::database::AccountState::NotExisting;
            }
            if let Some(storage) = self.cache.storage.get(&address) {
                account.storage.extend(storage.slots.iter().map(|(&key, &value)| (key, value)));
                if storage.wiped && info.is_some() {
                    account.account_state = revm::database::AccountState::StorageCleared;
                }
            }
        }
        for (&hash, code) in &self.cache.contracts {
            result.cache.contracts.insert(hash, Bytecode::new_raw(code.original_bytes()));
        }
        result.cache.block_hashes.extend(
            self.cache.block_hashes.iter().map(|(&number, &hash)| (number.saturating_to(), hash)),
        );
        result
    }
}

impl Deref for NativeDb {
    type Target = CacheDB;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for NativeDb {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl DatabaseRef for NativeDb {
    type Error = DatabaseError;
    fn basic_ref(&self, address: Address) -> Result<Option<AccountInfo>, Self::Error> {
        Ok(self.account(address).map(|info| AccountInfo {
            balance: info.balance,
            nonce: info.nonce,
            code_hash: info.code_hash,
            code: Some(Bytecode::new_raw(self.code(info.code_hash).original_bytes())),
            ..Default::default()
        }))
    }
    fn code_by_hash_ref(&self, hash: B256) -> Result<Bytecode, Self::Error> {
        Ok(Bytecode::new_raw(self.code(hash).original_bytes()))
    }
    fn storage_ref(&self, address: Address, key: U256) -> Result<U256, Self::Error> {
        Ok(self.slot(address, key))
    }
    fn block_hash_ref(&self, number: u64) -> Result<B256, Self::Error> {
        if let Some(hash) = self.cache.block_hashes.get(&U256::from(number)) {
            return Ok(*hash);
        }
        // Preserve Foundry's deterministic empty-database block hashes during migration.
        Ok(keccak256(number.to_string().as_bytes()))
    }
}

impl Database for NativeDb {
    type Error = DatabaseError;
    fn basic(&mut self, address: Address) -> Result<Option<AccountInfo>, Self::Error> {
        self.basic_ref(address)
    }
    fn code_by_hash(&mut self, hash: B256) -> Result<Bytecode, Self::Error> {
        self.code_by_hash_ref(hash)
    }
    fn storage(&mut self, address: Address, key: U256) -> Result<U256, Self::Error> {
        self.storage_ref(address, key)
    }
    fn block_hash(&mut self, number: u64) -> Result<B256, Self::Error> {
        self.block_hash_ref(number)
    }
}

impl DatabaseCommit for NativeDb {
    fn commit(&mut self, changes: AddressMap<Account>) {
        for (address, account) in changes {
            if !account.is_touched() {
                continue;
            }
            if account.is_selfdestructed() {
                self.cache.accounts.insert(address, None);
                self.cache.storage.entry(address).or_default().wipe();
                continue;
            }
            if account.is_created() {
                self.cache.storage.entry(address).or_default().wipe();
            }
            self.insert_account_info(address, account.info);
            self.cache
                .storage
                .entry(address)
                .or_default()
                .slots
                .extend(account.storage.into_iter().map(|(key, slot)| (key, slot.present_value)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state_changes::{AccountChange, StorageChange};

    #[test]
    fn native_commit_wipes_storage_and_deletion_survives_cloning() {
        let address = Address::with_last_byte(1);
        let mut db = NativeDb::default();
        let info = NativeAccount::default().with_balance(U256::from(42)).with_nonce(3);
        db.0.insert_account_info(&address, info.clone());
        db.insert_account_storage(address, U256::ZERO, U256::from(7)).unwrap();
        let snapshot = db.clone();
        db.commit_native(
            [(
                address,
                AccountChange {
                    info,
                    touched: true,
                    storage_wiped: true,
                    storage: [(
                        U256::from(1),
                        StorageChange { original_value: U256::ZERO, present_value: U256::from(9) },
                    )]
                    .into_iter()
                    .collect(),
                    ..Default::default()
                },
            )]
            .into_iter()
            .collect(),
        );
        assert_eq!(db.account(address).unwrap().balance, U256::from(42));
        assert_eq!(db.slot(address, U256::ZERO), U256::ZERO);
        assert_eq!(db.slot(address, U256::from(1)), U256::from(9));
        assert_eq!(snapshot.slot(address, U256::ZERO), U256::from(7));
        db.commit_native(
            [(address, AccountChange { deleted: true, ..Default::default() })]
                .into_iter()
                .collect(),
        );
        let restored = db.clone();
        assert!(restored.account(address).is_none());
        assert_eq!(restored.slot(address, U256::from(1)), U256::ZERO);
    }
}
