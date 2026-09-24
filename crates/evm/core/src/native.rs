//! Ethereum execution environment shared by native Foundry components.

use evm2::{SpecId, Version, env::BlockEnvExt};

/// Configuration and block data for an Ethereum execution.
#[derive(Clone, Copy, Debug)]
pub struct EthereumEnv {
    pub spec: SpecId,
    pub version: Version,
    pub block: BlockEnvExt,
}

impl EthereumEnv {
    /// Uses the protocol defaults for `spec`.
    pub const fn new(spec: SpecId, block: BlockEnvExt) -> Self {
        Self { spec, version: Version::new(spec), block }
    }
}
