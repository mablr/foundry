//! # foundry-cheatcodes
//!
//! Foundry cheatcodes implementations.

#![cfg_attr(not(test), warn(unused_crate_dependencies))]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![allow(elided_lifetimes_in_paths)] // Cheats context uses 3 lifetimes

#[macro_use]
extern crate foundry_common;

#[macro_use]
pub extern crate foundry_cheatcodes_spec as spec;

#[macro_use]
extern crate tracing;

use alloy_primitives::Address;
use alloy_sol_types::SolInterface;
use foundry_evm_core::{
    backend::DatabaseExt,
    evm::{FoundryContextFor, FoundryEvmNetwork},
};
use revm::context::{ContextTr, JournalTr};

pub use Vm::ForgeContext;
pub use config::CheatsConfig;
pub use error::{Error, ErrorKind, Result};
pub use foundry_evm_core::evm::NestedEvmClosureFor;
pub use inspector::{
    BroadcastableTransaction, BroadcastableTransactions, Cheatcodes, CheatcodesExecutor,
};
pub use spec::{CheatcodeDef, Vm};

#[macro_use]
mod error;

mod base64;

mod config;

mod crypto;

mod version;

mod env;
pub use env::{current_execution_context, set_execution_context};

mod evm;

pub mod native;

mod external_storage;

mod fs;

mod inspector;
pub use inspector::CheatcodeAnalysis;

mod json;

/* EVM2 migration: disabled non-Ethereum execution.
#[cfg(feature = "monad")]
mod monad;
*/

mod script;
pub use script::{Wallets, WalletsInner};

mod string;

// Disabled for the Ethereum-only EVM2 migration.
// mod tempo;

mod test;
pub use test::expect::ExpectedCallTracker;

mod toml;

mod utils;

/// Cheatcode implementation.
pub(crate) trait Cheatcode: CheatcodeDef {
    /// Evaluates an assertion independently of execution state.
    fn assertion_result(&self) -> Option<Result> {
        None
    }

    /// Applies this cheatcode to the given state.
    ///
    /// Implement this function if you don't need access to the EVM data.
    fn apply<FEN: FoundryEvmNetwork>(&self, state: &mut Cheatcodes<FEN>) -> Result {
        let _ = state;
        unimplemented!("{}", Self::CHEATCODE.func.id)
    }

    /// Applies this cheatcode to the given context.
    ///
    /// Implement this function if you need access to the EVM data.
    #[inline(always)]
    fn apply_stateful<FEN: FoundryEvmNetwork>(&self, ccx: &mut CheatsCtxt<'_, '_, FEN>) -> Result {
        self.apply(ccx.state)
    }

    /// Applies this cheatcode to the given context and executor.
    ///
    /// Implement this function if you need access to the executor.
    #[inline(always)]
    fn apply_full<FEN: FoundryEvmNetwork>(
        &self,
        ccx: &mut CheatsCtxt<'_, '_, FEN>,
        executor: &mut dyn CheatcodesExecutor<FEN>,
    ) -> Result {
        let _ = executor;
        self.apply_stateful(ccx)
    }
}

/// The cheatcode context.
pub struct CheatsCtxt<'a, 'db, FEN: FoundryEvmNetwork + 'db> {
    /// The cheatcodes inspector state.
    pub(crate) state: &'a mut Cheatcodes<FEN>,
    /// The EVM context.
    pub(crate) ecx: &'a mut FoundryContextFor<'db, FEN>,
    /// The original `msg.sender`.
    pub(crate) caller: Address,
    /// Gas limit of the current cheatcode call.
    pub(crate) gas_limit: u64,
    /// Whether the current cheatcode call is static.
    pub(crate) is_static: bool,
}

impl<'a, 'db, FEN: FoundryEvmNetwork> std::ops::Deref for CheatsCtxt<'a, 'db, FEN> {
    type Target = FoundryContextFor<'db, FEN>;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        self.ecx
    }
}

impl<'db, FEN: FoundryEvmNetwork> std::ops::DerefMut for CheatsCtxt<'_, 'db, FEN> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.ecx
    }
}

impl<FEN: FoundryEvmNetwork> CheatsCtxt<'_, '_, FEN> {
    pub(crate) fn ensure_not_precompile(&self, address: &Address) -> Result<()> {
        if self.is_precompile(address) { Err(precompile_error(address)) } else { Ok(()) }
    }

    pub(crate) fn is_precompile(&self, address: &Address) -> bool {
        self.ecx.journal().precompile_addresses().contains(address)
    }
}

#[cold]
fn precompile_error(address: &Address) -> Error {
    fmt_err!("cannot use precompile {address} as an argument")
}

// Tempo's precompile implementation is outside the initial EVM2 migration.
impl Cheatcode for Vm::isImplicitlyApprovedCall {
    fn apply<FEN: FoundryEvmNetwork>(&self, _state: &mut Cheatcodes<FEN>) -> Result {
        Err(fmt_err!("Tempo execution is disabled on the Ethereum-only EVM2 migration branch"))
    }
}

impl Cheatcode for Vm::assumeImplicitApprovalCall {
    fn apply<FEN: FoundryEvmNetwork>(&self, _state: &mut Cheatcodes<FEN>) -> Result {
        Err(fmt_err!("Tempo execution is disabled on the Ethereum-only EVM2 migration branch"))
    }
}

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
