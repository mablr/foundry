use alloy_evm::EvmEnv;
use foundry_evm_networks::NetworkConfigs;

/// Helper container type for [`EvmEnv`] and [`NetworkConfigs`].
#[derive(Clone, Debug, Default)]
pub struct Env {
    pub evm_env: EvmEnv,
    pub networks: NetworkConfigs,
}

impl Env {
    pub fn new(evm_env: EvmEnv, networks: NetworkConfigs) -> Self {
        Self { evm_env, networks }
    }
}
