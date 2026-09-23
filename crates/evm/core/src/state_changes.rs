//! Owned transaction changes produced by native execution.

use alloy_primitives::{
    Address, Bytes, U256,
    map::{AddressMap, HashMap},
};

pub use evm2::{bytecode::Bytecode, evm::AccountInfo, interpreter::InstrStop as ExecutionStatus};

/// Account and storage changes ready for persistence or result inspection.
pub type StateChangeset = AddressMap<AccountChange>;

/// Final account metadata and storage writes for one transaction.
#[derive(Clone, Debug, Default)]
pub struct AccountChange {
    pub info: AccountInfo,
    pub storage: HashMap<U256, StorageChange>,
    pub touched: bool,
    pub created: bool,
    pub deleted: bool,
    pub storage_wiped: bool,
}

/// Transaction-boundary storage values.
#[derive(Clone, Copy, Debug, Default)]
pub struct StorageChange {
    pub original_value: U256,
    pub present_value: U256,
}

/// Return data and optional deployed address from native execution.
#[derive(Clone, Debug)]
pub enum ExecutionOutput {
    Call(Bytes),
    Create(Bytes, Option<Address>),
}
