//! # foundry-evm-core
//!
//! Generic execution, environment, fork, backend, and state abstractions shared by Foundry tools.
//!
//! [`evm::FoundryEvmNetwork`] associates an Alloy network with environment and chain-context
//! types. Native evm2 execution owns its interpreter, journal and precompiles directly.

#![cfg_attr(not(test), warn(unused_crate_dependencies))]
#![cfg_attr(docsrs, feature(doc_cfg))]

use alloy_primitives::{Address, map::HashMap};

/* EVM2 migration: disabled non-Ethereum execution.
#[cfg(feature = "optimism")]
use op_alloy_rpc_types as _;
*/

/// Map keyed by breakpoints char to their location (contract address, pc)
pub type Breakpoints = HashMap<char, (Address, usize)>;

#[macro_use]
extern crate tracing;

pub mod abi {
    pub use foundry_cheatcodes_spec::Vm;
    pub use foundry_evm_abi::*;
}

pub mod env;
pub use env::*;

pub mod backend;
pub mod buffer;
pub mod bytecode;
pub mod config;
pub use config::ExecutionConfig;

pub mod constants;
pub mod decode;
pub mod eip2935;
pub mod evm;
pub mod fork;
pub mod hardfork;
pub mod ic;
pub mod native;
pub mod opts;
pub mod precompiles;
pub mod state_changes;
pub mod state_snapshot;
// Disabled for the Ethereum-only EVM2 migration.
// pub mod tempo;
pub mod utils;
