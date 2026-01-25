use alloy_consensus::{Sealed, SignableTransaction};
use alloy_network::{Ethereum, EthereumWallet, NetworkWallet, TxSigner};
use alloy_primitives::Address;
use tempo_primitives::TempoSignature;

use crate::{FoundryNetwork, FoundryTxEnvelope, FoundryTypedTx};

impl NetworkWallet<FoundryNetwork> for EthereumWallet {
    fn default_signer_address(&self) -> Address {
        NetworkWallet::<Ethereum>::default_signer_address(self)
    }

    fn has_signer_for(&self, address: &Address) -> bool {
        NetworkWallet::<Ethereum>::has_signer_for(self, address)
    }

    fn signer_addresses(&self) -> impl Iterator<Item = Address> {
        NetworkWallet::<Ethereum>::signer_addresses(self)
    }

    async fn sign_transaction_from(
        &self,
        sender: Address,
        tx: FoundryTypedTx,
    ) -> alloy_signer::Result<FoundryTxEnvelope> {
        match tx {
            FoundryTypedTx::Legacy(mut inner) => {
                let sig = TxSigner::sign_transaction(
                    &self
                        .signer_by_address(sender)
                        .ok_or(alloy_signer::Error::other("Signer not found"))?,
                    &mut inner,
                )
                .await?;
                Ok(FoundryTxEnvelope::Legacy(inner.into_signed(sig)))
            }
            FoundryTypedTx::Eip2930(mut inner) => {
                let sig = TxSigner::sign_transaction(
                    &self
                        .signer_by_address(sender)
                        .ok_or(alloy_signer::Error::other("Signer not found"))?,
                    &mut inner,
                )
                .await?;
                Ok(FoundryTxEnvelope::Eip2930(inner.into_signed(sig)))
            }
            FoundryTypedTx::Eip1559(mut inner) => {
                let sig = TxSigner::sign_transaction(
                    &self
                        .signer_by_address(sender)
                        .ok_or(alloy_signer::Error::other("Signer not found"))?,
                    &mut inner,
                )
                .await?;
                Ok(FoundryTxEnvelope::Eip1559(inner.into_signed(sig)))
            }
            FoundryTypedTx::Eip4844(mut inner) => {
                let sig = TxSigner::sign_transaction(
                    &self
                        .signer_by_address(sender)
                        .ok_or(alloy_signer::Error::other("Signer not found"))?,
                    &mut inner,
                )
                .await?;
                Ok(FoundryTxEnvelope::Eip4844(inner.into_signed(sig)))
            }
            FoundryTypedTx::Eip7702(mut inner) => {
                let sig = TxSigner::sign_transaction(
                    &self
                        .signer_by_address(sender)
                        .ok_or(alloy_signer::Error::other("Signer not found"))?,
                    &mut inner,
                )
                .await?;
                Ok(FoundryTxEnvelope::Eip7702(inner.into_signed(sig)))
            }
            FoundryTypedTx::Deposit(inner) => {
                // Deposit transactions don't require signing
                Ok(FoundryTxEnvelope::Deposit(Sealed::new(inner)))
            }
            FoundryTypedTx::Tempo(mut inner) => {
                let sig = TxSigner::sign_transaction(
                    &self
                        .signer_by_address(sender)
                        .ok_or(alloy_signer::Error::other("Signer not found"))?,
                    &mut inner,
                )
                .await?;
                let tempo_sig: TempoSignature = sig.into();
                Ok(FoundryTxEnvelope::Tempo(inner.into_signed(tempo_sig)))
            }
        }
    }
}
