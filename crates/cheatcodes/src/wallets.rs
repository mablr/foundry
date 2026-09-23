//! Wallets shared by script execution and transaction publishing.

use crate::Result;
use alloy_primitives::Address;
use alloy_signer_local::PrivateKeySigner;
use foundry_wallets::{WalletSigner, wallet_multi::MultiWallet};
use parking_lot::Mutex;
use std::sync::Arc;

/// Contains context for wallet management.
#[derive(Debug)]
pub struct WalletsInner {
    /// All signers in scope of the script.
    pub multi_wallet: MultiWallet,
    /// Optional signer provided as `--sender` flag.
    pub provided_sender: Option<Address>,
}

/// Cloneable wrapper around [`WalletsInner`].
#[derive(Debug, Clone)]
pub struct Wallets {
    /// Inner data.
    pub inner: Arc<Mutex<WalletsInner>>,
}

impl Wallets {
    #[expect(missing_docs)]
    pub fn new(multi_wallet: MultiWallet, provided_sender: Option<Address>) -> Self {
        Self { inner: Arc::new(Mutex::new(WalletsInner { multi_wallet, provided_sender })) }
    }

    /// Consumes [Wallets] and returns [MultiWallet].
    ///
    /// Panics if [Wallets] is still in use.
    pub fn into_multi_wallet(self) -> MultiWallet {
        Arc::into_inner(self.inner)
            .map(|m| m.into_inner().multi_wallet)
            .unwrap_or_else(|| panic!("not all instances were dropped"))
    }

    /// Locks inner Mutex and adds a signer to the [MultiWallet].
    pub fn add_local_signer(&self, wallet: PrivateKeySigner) {
        self.inner.lock().multi_wallet.add_signer(WalletSigner::Local(wallet));
    }

    /// Locks inner Mutex and returns all signer addresses in the [MultiWallet].
    pub fn signers(&self) -> Result<Vec<Address>> {
        Ok(self.inner.lock().multi_wallet.signers()?.keys().copied().collect())
    }

    /// Locks inner Mutex and returns all available addresses in the [MultiWallet].
    pub fn addresses(&self) -> Vec<Address> {
        self.inner.lock().multi_wallet.available_addresses()
    }

    /// Number of signers in the [MultiWallet].
    pub fn len(&self) -> usize {
        let mut inner = self.inner.lock();
        inner.multi_wallet.signers().map_or(0, |signers| signers.len())
    }

    /// Whether the [MultiWallet] is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
