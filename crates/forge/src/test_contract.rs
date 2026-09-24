//! Linked test artifacts shared by Forge execution runners.

use alloy_json_abi::JsonAbi;
use alloy_primitives::{Address, Bytes};
use foundry_compilers::ArtifactId;
use std::collections::{BTreeMap, BTreeSet};

/// A deployable test contract and the libraries linked into its bytecode.
#[derive(Debug, Clone)]
pub struct TestContract {
    pub abi: JsonAbi,
    pub bytecode: Bytes,
    pub library_addresses: BTreeSet<Address>,
}

/// Test contracts indexed by compiler artifact ID.
pub type DeployableContracts = BTreeMap<ArtifactId, TestContract>;
