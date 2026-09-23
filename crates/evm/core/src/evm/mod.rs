//! Shared EVM traits, associated types, and execution helpers.
//!
//! Each network module owns its network marker and concrete EVM implementations.

use crate::{
    EvmEnv, FoundryBlock, FoundryChain, FoundryContextExt, FoundryInspectorExt, FoundryJournal,
    FoundryTransaction, FromAnyRpcTransaction,
    backend::{DatabaseExt, JournaledState},
};
use alloy_consensus::{SignableTransaction, Signed, transaction::SignerRecoverable};
use alloy_evm::{Evm, EvmFactory, FromRecoveredTx, precompiles::PrecompilesMap};
use alloy_network::Network;
use alloy_primitives::Signature;
use alloy_rlp::Decodable;
use foundry_common::{FoundryReceiptResponse, FoundryTransactionBuilder, fmt::UIfmt};
use foundry_config::ExecutionSpec;
use foundry_fork_db::ForkBlockEnv;
use revm::{
    context::{
        ContextTr,
        result::{HaltReason, ResultAndState},
    },
    inspector::NoOpInspector,
    interpreter::InstructionResult,
    primitives::hardfork::SpecId,
};
use serde::{Deserialize, Serialize};
use std::{fmt::Debug, ops::DerefMut};

/* EVM2 migration: disabled non-Ethereum execution.
#[cfg(feature = "base")]
pub mod base;
*/
pub mod eth;
/* EVM2 migration: disabled non-Ethereum execution.
#[cfg(feature = "monad")]
pub mod monad;
*/
/* EVM2 migration: disabled non-Ethereum execution.
#[cfg(feature = "optimism")]
pub mod op;
*/
// Disabled for the Ethereum-only EVM2 migration.
// pub mod tempo;

pub use eth::*;
// Disabled for the Ethereum-only EVM2 migration.
// pub use tempo::*;

/* EVM2 migration: disabled non-Ethereum execution.
#[cfg(feature = "base")]
pub use base::*;
*/

/* EVM2 migration: disabled non-Ethereum execution.
#[cfg(feature = "monad")]
pub use monad::*;
*/

/* EVM2 migration: disabled non-Ethereum execution.
#[cfg(feature = "optimism")]
pub use op::*;
*/

/// Foundry's compatibility trait associating a [`Network`] with a [`FoundryEvmFactory`].
pub trait FoundryEvmNetwork: Copy + Debug + Default + 'static {
    type Network: Network<
            TxEnvelope: Decodable
                            + SignerRecoverable
                            + From<Signed<<Self::Network as Network>::UnsignedTx>>
                            + for<'d> Deserialize<'d>
                            + Serialize
                            + UIfmt,
            UnsignedTx: SignableTransaction<Signature>,
            TransactionRequest: FoundryTransactionBuilder<Self::Network>
                                    + for<'d> Deserialize<'d>
                                    + Serialize,
            ReceiptResponse: FoundryReceiptResponse,
        >;
    type Spec: Into<SpecId>
        + ExecutionSpec
        + Default
        + Copy
        + Debug
        + Eq
        + std::hash::Hash
        + Unpin
        + Send
        + Sync
        + 'static;
    type Block: FoundryBlock + ForkBlockEnv + Default + Debug + Unpin;
    type Tx: Clone
        + Debug
        + FoundryTransaction
        + FromAnyRpcTransaction
        + Default
        + Send
        + Sync
        + FromRecoveredTx<<Self::Network as Network>::TxEnvelope>;

    // TODO(evm2): Remove the legacy factory association when fork and inspector contexts migrate.
    type EvmFactory: FoundryEvmFactory<Spec = Self::Spec, BlockEnv = Self::Block, Tx = Self::Tx>;
}

pub trait FoundryEvmFactory:
    EvmFactory<
        Spec: Into<SpecId> + ExecutionSpec + Default + Copy + Unpin + Send + 'static,
        BlockEnv: FoundryBlock + ForkBlockEnv + Default + Unpin,
        Tx: Clone + Debug + FoundryTransaction + FromAnyRpcTransaction + Default + Send + Sync,
        HaltReason: IntoInstructionResult,
        Precompiles = PrecompilesMap,
    > + Clone
    + Debug
    + Default
    + 'static
{
    /// Chain type for EVM's context created by this factory.
    type Chain: FoundryChain<Self::Tx>;

    /// Foundry Context abstraction
    type FoundryContext<'db>: FoundryContextExt<
            Block = Self::BlockEnv,
            Tx = Self::Tx,
            Spec = Self::Spec,
            Chain = Self::Chain,
            Journal: FoundryJournal,
            Db: DatabaseExt<Self>,
        >
    where
        Self: 'db;

    /// The Foundry-wrapped EVM type produced by this factory.
    type FoundryEvm<'db, I: FoundryInspectorExt<Self::FoundryContext<'db>>>: Evm<
            DB = &'db mut dyn DatabaseExt<Self>,
            Tx = Self::Tx,
            BlockEnv = Self::BlockEnv,
            Spec = Self::Spec,
            HaltReason = Self::HaltReason,
            Precompiles = PrecompilesMap,
        > + DerefMut<Target = Self::FoundryContext<'db>>
    where
        Self: 'db;

    /// Creates a Foundry-wrapped EVM with the given inspector.
    ///
    /// Callers carrying execution context must install it through the returned context's
    /// `chain_mut` before executing. This also preserves OP's block-derived L1 fee information.
    fn create_foundry_evm_with_inspector<'db, I: FoundryInspectorExt<Self::FoundryContext<'db>>>(
        &self,
        db: &'db mut dyn DatabaseExt<Self>,
        evm_env: EvmEnv<Self::Spec, Self::BlockEnv>,
        inspector: I,
    ) -> Self::FoundryEvm<'db, I>;

    /// Creates a Foundry-wrapped nested EVM without an inspector.
    fn create_nested_evm<'db>(
        &self,
        db: &'db mut dyn DatabaseExt<Self>,
        evm_env: EvmEnv<Self::Spec, Self::BlockEnv>,
    ) -> NestedEvmFor<'db, Self> {
        self.create_nested_evm_with_inspector(db, evm_env, NoOpInspector)
    }

    /// Creates a Foundry-wrapped nested EVM with the given inspector.
    /// Install inherited chain state with [`NestedEvm::chain_mut`] before executing or restoring
    /// journal-derived state.
    fn create_nested_evm_with_inspector<'db, I>(
        &self,
        db: &'db mut dyn DatabaseExt<Self>,
        evm_env: EvmEnv<Self::Spec, Self::BlockEnv>,
        inspector: I,
    ) -> NestedEvmFor<'db, Self>
    where
        I: FoundryInspectorExt<Self::FoundryContext<'db>> + 'db;
}

/// Object-safe EVM operations used by nested execution and fork replay.
///
/// This abstracts over the concrete EVM type (`FoundryEvm`, future `TempoEvm`, etc.)
/// so that cheatcode impls can build and run nested EVMs without knowing the concrete type.
pub trait NestedEvm {
    /// The spec type.
    type Spec;
    /// The block environment type.
    type Block;
    /// The transaction environment type.
    type Tx: FoundryTransaction;
    /// Chain context identifying the active transaction position.
    type Chain: FoundryChain<Self::Tx>;
    /// The Journal type, which may own Monad's reserve-balance-tracker state.
    type Journal: FoundryJournal;
    /// Returns a mutable reference to the journal inner state (`JournaledState`).
    fn journal_inner_mut(&mut self) -> &mut JournaledState;

    /// Returns a mutable reference to the transaction environment.
    fn tx_mut(&mut self) -> &mut Self::Tx;

    /// Returns a mutable reference to the chain-position context.
    fn chain_mut(&mut self) -> &mut Self::Chain;

    /// Returns the precompile map.
    fn precompiles_mut(&mut self) -> &mut PrecompilesMap;

    /// Returns a mutable reference to the Journal.
    fn journal_mut(&mut self) -> &mut Self::Journal;

    /// Executes a full transaction with the given tx env.
    fn transact_raw(&mut self, tx: Self::Tx) -> eyre::Result<ResultAndState<HaltReason>>;

    /// Replays a transaction, skipping unsupported system envelopes.
    ///
    /// `is_system` preserves the RPC envelope classification that conversion to `Self::Tx` may
    /// discard. Returning `None` must not mutate the EVM, database, or inspector.
    fn transact_replay(
        &mut self,
        tx: Self::Tx,
        is_system: bool,
    ) -> eyre::Result<Option<ResultAndState<HaltReason>>> {
        if is_system {
            return Ok(None);
        }
        self.transact_raw(tx).map(Some)
    }

    fn to_evm_env(&self) -> EvmEnv<Self::Spec, Self::Block>;
}

/// Converts a network-specific halt reason into an [`InstructionResult`].
pub trait IntoInstructionResult {
    fn into_instruction_result(self) -> InstructionResult;
}

/// Convenience type aliases for accessing associated types through [`FoundryEvmNetwork`].
pub type EvmFactoryFor<FEN> = <FEN as FoundryEvmNetwork>::EvmFactory;
pub type FoundryContextFor<'db, FEN> =
    <EvmFactoryFor<FEN> as FoundryEvmFactory>::FoundryContext<'db>;
pub type TxEnvFor<FEN> = <FEN as FoundryEvmNetwork>::Tx;
pub type HaltReasonFor<FEN> = <EvmFactoryFor<FEN> as EvmFactory>::HaltReason;
pub type SpecFor<FEN> = <FEN as FoundryEvmNetwork>::Spec;
pub type BlockEnvFor<FEN> = <FEN as FoundryEvmNetwork>::Block;
pub type PrecompilesFor<FEN> = <EvmFactoryFor<FEN> as EvmFactory>::Precompiles;
pub type EvmEnvFor<FEN> = EvmEnv<SpecFor<FEN>, BlockEnvFor<FEN>>;
pub type NetworkFor<FEN> = <FEN as FoundryEvmNetwork>::Network;
pub type TxEnvelopeFor<FEN> = <NetworkFor<FEN> as Network>::TxEnvelope;
pub type TransactionRequestFor<FEN> = <NetworkFor<FEN> as Network>::TransactionRequest;
pub type TransactionResponseFor<FEN> = <NetworkFor<FEN> as Network>::TransactionResponse;
pub type BlockResponseFor<FEN> = <NetworkFor<FEN> as Network>::BlockResponse;

pub type ChainFor<FEN> = <EvmFactoryFor<FEN> as FoundryEvmFactory>::Chain;

/// Boxed nested EVM produced by a Foundry EVM factory.
pub type NestedEvmFor<'db, F> = Box<
    dyn NestedEvm<
            Spec = <F as EvmFactory>::Spec,
            Block = <F as EvmFactory>::BlockEnv,
            Tx = <F as EvmFactory>::Tx,
            Chain = <F as FoundryEvmFactory>::Chain,
            Journal = <<F as FoundryEvmFactory>::FoundryContext<'db> as ContextTr>::Journal,
        > + 'db,
>;

// TODO(evm2): Fork replay still uses the legacy transaction factory above. Implement native
// replay and remove this association; do not reintroduce suspended-frame execution adapters.
