//! Foundry cheatcode configuration, native execution and session data.

#![cfg_attr(not(test), warn(unused_crate_dependencies))]
#![cfg_attr(docsrs, feature(doc_cfg))]

#[macro_use]
pub extern crate foundry_cheatcodes_spec as spec;

use alloy_sol_types::SolInterface;

pub use Vm::ForgeContext;
pub use spec::{CheatcodeDef, Vm};

#[macro_use]
mod error;
pub use error::{Error, ErrorKind, Result};

mod config;
pub use config::CheatsConfig;

mod analysis;
pub use analysis::CheatcodeAnalysis;

mod wallets;
pub use wallets::{Wallets, WalletsInner};

mod execution_context;
pub use execution_context::{current_execution_context, set_execution_context};

mod session;
pub use session::{BroadcastableTransaction, BroadcastableTransactions, Cheatcodes};

mod assertions;
pub mod native;

// TODO(evm2): Port remaining cheatcodes to native dispatch. Legacy handler source is retained
// temporarily for reference, but the REVM dispatcher and its handlers are no longer compiled.

/// Decodes a cheatcode call with Foundry's unknown-selector diagnostic.
pub fn decode_cheatcode(input: &[u8]) -> Result<Vm::VmCalls> {
    Ok(Vm::VmCalls::abi_decode(input).map_err(|error| {
        if let alloy_sol_types::Error::UnknownSelector { name: _, selector } = error {
            let message = format!(
                "unknown cheatcode with selector {selector}; \
                 you may have a mismatch between the `Vm` interface (likely in `forge-std`) \
                 and the `forge` version"
            );
            return alloy_sol_types::Error::Other(std::borrow::Cow::Owned(message));
        }
        error
    })?)
}
