use alloy_primitives::Address;
use revm::context_interface::result::EVMError;
use std::{convert::Infallible, sync::Arc};

/// Temporary error boundary for unmigrated REVM database consumers.
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct DatabaseError(#[from] pub foundry_fork_db::DatabaseError);

pub type DatabaseResult<T> = Result<T, DatabaseError>;

// TODO(evm2): Remove this marker when legacy database consumers are migrated.
impl revm::database::DBErrorMarker for DatabaseError {}

impl From<Infallible> for DatabaseError {
    fn from(value: Infallible) -> Self {
        match value {}
    }
}

pub type BackendResult<T> = Result<T, BackendError>;

/// Errors that can happen when working with [`revm::Database`]
#[derive(Debug, thiserror::Error)]
#[expect(missing_docs)]
pub enum BackendError {
    #[error("{0}")]
    Message(String),
    #[error("cheatcodes are not enabled for {0}; see `vm.allowCheatcodes(address)`")]
    NoCheats(Address),
    #[error(transparent)]
    Database(#[from] DatabaseError),
    #[error("failed to fetch account info for {0}")]
    MissingAccount(Address),
    #[error(
        "CREATE2 Deployer (0x4e59b44847b379578588920ca78fbf26c0b4956c) not present on this chain.\n\
         For a production environment, you can deploy it using the pre-signed transaction from \
         https://github.com/Arachnid/deterministic-deployment-proxy.\n\
         For a test environment, you can use `etch` to place the required bytecode at that address."
    )]
    MissingCreate2Deployer,
}

impl BackendError {
    /// Create a new error with a message
    pub fn msg(msg: impl Into<String>) -> Self {
        Self::Message(msg.into())
    }

    /// Create a new error with a message
    pub fn display(msg: impl std::fmt::Display) -> Self {
        Self::Message(msg.to_string())
    }
}

impl From<tokio::task::JoinError> for BackendError {
    fn from(value: tokio::task::JoinError) -> Self {
        Self::display(value)
    }
}

impl From<Infallible> for BackendError {
    fn from(value: Infallible) -> Self {
        match value {}
    }
}

// Note: this is mostly necessary to use some revm internals that return an [EVMError]
impl<T: Into<Self>> From<EVMError<T>> for BackendError {
    fn from(err: EVMError<T>) -> Self {
        match err {
            EVMError::Database(err) => err.into(),
            EVMError::Custom(err) => Self::msg(err),
            EVMError::Header(err) => Self::msg(err.to_string()),
            EVMError::Transaction(err) => Self::msg(err.to_string()),
            EVMError::CustomAny(err) => Self::msg(err.to_string()),
        }
    }
}

impl DatabaseError {
    /// Wraps an application error for legacy database consumers.
    pub const fn other(error: Arc<eyre::Error>) -> Self {
        Self(foundry_fork_db::DatabaseError::AnyRequest(error))
    }
}
