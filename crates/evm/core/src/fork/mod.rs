use super::opts::EvmOpts;

mod init;
use alloy_evm::EvmEnv;
pub use init::{configure_env, environment};

pub mod database;

mod multi;
pub use multi::{ForkId, MultiFork, MultiForkHandler};
use revm::context::TxEnv;

/// Represents a _fork_ of a remote chain whose data is available only via the `url` endpoint.
#[derive(Clone, Debug)]
pub struct CreateFork {
    /// Whether to enable rpc storage caching for this fork
    pub enable_caching: bool,
    /// The URL to a node for fetching remote state
    pub url: String,
    /// The EvmEnv to create this fork, main purpose is to provide some metadata for the fork
    pub evm_env: EvmEnv,
    /// The TxEnv to create this fork, main purpose is to provide some metadata for the fork
    pub tx_env: TxEnv,
    /// All env settings as configured by the user
    pub evm_opts: EvmOpts,
}
