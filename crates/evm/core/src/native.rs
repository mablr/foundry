//! Native evm2 database reads, environment conversion and transaction write collection.
//!
//! TODO(evm2): Replace the legacy spec, block, and fork-cache types used at this boundary.

use crate::{
    ExecutionConfig, FoundryBlock,
    backend::{Backend, DatabaseError},
    evm::FoundryEvmNetwork,
    state_changes::StateChangeset,
};
use alloy_primitives::{Address, B256, U256};
use evm2::{
    Version,
    bytecode::Bytecode as NativeBytecode,
    env::BlockEnvExt,
    evm::{AccountChangeRef, AccountInfo as NativeAccount, StateChangeSink, StorageChange},
};
use revm::{database::DatabaseRef, primitives::hardfork::SpecId};
use std::convert::Infallible;

impl<FEN: FoundryEvmNetwork> evm2::evm::Database for &Backend<FEN> {
    type Error = DatabaseError;

    fn get_account(&mut self, address: &Address) -> Result<Option<NativeAccount>, Self::Error> {
        self.native_account(*address)
    }

    fn get_code_by_hash(&mut self, hash: &B256) -> Result<NativeBytecode, Self::Error> {
        if !self.is_in_forking_mode() {
            return Ok(self.mem_db().code(*hash));
        }
        self.code_by_hash_ref(*hash).map(|code| NativeBytecode::new_raw(code.original_bytes()))
    }

    fn get_storage(&mut self, address: &Address, key: &U256) -> Result<U256, Self::Error> {
        self.storage_ref(*address, *key)
    }

    fn get_block_hash(&mut self, number: &U256) -> Result<B256, Self::Error> {
        self.block_hash_ref(number.saturating_to())
    }
}

/// Collects native transaction writes for backend persistence.
#[derive(Default)]
pub struct StateChangesetCollector(pub StateChangeset);

impl StateChangeSink for StateChangesetCollector {
    type Error = Infallible;

    fn account(&mut self, change: AccountChangeRef<'_>) -> Result<(), Self::Error> {
        let account = self.0.entry(change.address).or_default();
        if let Some(info) = change.current {
            account.info = info.clone();
        }
        account.touched = true;
        if change.created {
            account.created = true;
        }
        if change.selfdestructed || change.current.is_none() {
            account.deleted = true;
        }
        Ok(())
    }

    fn storage(&mut self, change: StorageChange) -> Result<(), Self::Error> {
        let account = self.0.entry(change.address).or_default();
        account.touched = true;
        account.storage.insert(
            change.key,
            crate::state_changes::StorageChange {
                original_value: change.original,
                present_value: change.current,
            },
        );
        Ok(())
    }

    fn storage_wipe(&mut self, address: Address) -> Result<(), Self::Error> {
        self.0.entry(address).or_default().storage_wiped = true;
        Ok(())
    }

    fn account_read(
        &mut self,
        address: Address,
        info: Option<&NativeAccount>,
    ) -> Result<(), Self::Error> {
        // Native storage changes precede account callbacks. A storage-only write still needs
        // the unchanged account metadata when committed through Foundry's current backend.
        if let Some(account) = self.0.get_mut(&address)
            && let Some(info) = info
        {
            account.info = info.clone();
        }
        Ok(())
    }
}

/// Converts the supported fixed Ethereum environment to evm2.
pub fn environment<S: Copy + Into<SpecId>, B: FoundryBlock>(
    cfg: &ExecutionConfig<S>,
    block: &B,
) -> eyre::Result<(evm2::SpecId, Version, BlockEnvExt)> {
    let spec = match cfg.spec.into() {
        SpecId::MERGE => evm2::SpecId::MERGE,
        SpecId::SHANGHAI => evm2::SpecId::SHANGHAI,
        SpecId::CANCUN => evm2::SpecId::CANCUN,
        SpecId::PRAGUE => evm2::SpecId::PRAGUE,
        SpecId::OSAKA => evm2::SpecId::OSAKA,
        other => eyre::bail!("evm2 milestone 1 hardfork not yet wired: {other:?}"),
    };
    let version = cfg.version(spec);
    let block = BlockEnvExt {
        number: block.number(),
        beneficiary: block.beneficiary(),
        timestamp: block.timestamp(),
        gas_limit: U256::from(block.gas_limit()),
        basefee: U256::from(block.basefee()),
        difficulty: block.difficulty(),
        prevrandao: U256::from_be_bytes(block.prevrandao().unwrap_or_default().0),
        blob_basefee: U256::from(block.blob_gasprice().unwrap_or_default()),
        slot_num: U256::from(block.slot_num()),
        ..Default::default()
    };
    Ok((spec, version, block))
}

/// Converts the remaining legacy spec association to a native evm2 specification.
///
/// TODO(evm2): Remove this boundary when FoundryEvmNetwork owns a native spec.
pub const fn spec_id(spec: SpecId) -> evm2::SpecId {
    match spec {
        SpecId::FRONTIER => evm2::SpecId::FRONTIER,
        SpecId::HOMESTEAD => evm2::SpecId::HOMESTEAD,
        SpecId::TANGERINE => evm2::SpecId::TANGERINE,
        SpecId::SPURIOUS_DRAGON => evm2::SpecId::SPURIOUS_DRAGON,
        SpecId::BYZANTIUM => evm2::SpecId::BYZANTIUM,
        SpecId::PETERSBURG => evm2::SpecId::PETERSBURG,
        SpecId::ISTANBUL => evm2::SpecId::ISTANBUL,
        SpecId::BERLIN => evm2::SpecId::BERLIN,
        SpecId::LONDON => evm2::SpecId::LONDON,
        SpecId::MERGE => evm2::SpecId::MERGE,
        SpecId::SHANGHAI => evm2::SpecId::SHANGHAI,
        SpecId::CANCUN => evm2::SpecId::CANCUN,
        SpecId::PRAGUE => evm2::SpecId::PRAGUE,
        SpecId::OSAKA => evm2::SpecId::OSAKA,
        SpecId::AMSTERDAM => evm2::SpecId::AMSTERDAM,
    }
}
