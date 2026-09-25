//! Ethereum script broadcasting from native evm2 execution.

use super::NativeScriptContext;
use crate::{
    broadcast::{SendTransactionKind, SendTransactionsKind},
    receipts::{TxStatus, check_tx_status, format_receipt},
};
use alloy_chains::Chain;
use alloy_network::{Ethereum, EthereumWallet, TransactionBuilder};
use alloy_primitives::map::{AddressHashMap, AddressHashSet, HashMap};
use alloy_provider::Provider;
use eyre::Result;
use forge_script_sequence::ScriptSequence;
use foundry_common::{
    TransactionMaybeSigned,
    provider::{
        ProviderBuilder,
        fee::{estimate_eip1559_fees, resolve_broadcast_eip1559_fees},
    },
};
use foundry_config::Config;
use std::sync::Arc;

impl NativeScriptContext {
    /// Sends a simulated Ethereum sequence and checkpoints every submission and receipt.
    pub(super) async fn broadcast(self, mut sequence: ScriptSequence<Ethereum>) -> Result<()> {
        sequence.paths = Some(ScriptSequence::<Ethereum>::get_paths(
            &self.config,
            &self.args.sig,
            &self.plan.build.build_data.target,
            sequence.chain,
            false,
        )?);
        sequence.save(true, true)?;

        let senders = sequence
            .transactions
            .iter()
            .filter_map(|tx| tx.transaction.from())
            .collect::<AddressHashSet>();
        eyre::ensure!(
            !senders.contains(&Config::DEFAULT_SENDER),
            "You seem to be using Foundry's default sender. Be sure to set your own --sender."
        );
        let send_kind = if self.args.unlocked {
            SendTransactionsKind::Unlocked(senders)
        } else {
            let signers = self.script_wallets.into_multi_wallet().into_signers()?;
            let eth_wallets: AddressHashMap<EthereumWallet> =
                signers.into_iter().map(|(addr, signer)| (addr, signer.into())).collect();
            SendTransactionsKind::Raw {
                eth_wallets,
                browser: self.browser_wallet,
                access_keys: HashMap::default(),
            }
        };

        let provider = Arc::new(
            ProviderBuilder::<Ethereum>::from_config_with_url(&self.config, sequence.rpc_url())?
                .build()?,
        );
        let chain = Chain::from(sequence.chain);
        let legacy = chain.is_legacy() || self.args.legacy;
        let (gas_price, fees) = if legacy {
            (
                Some(match self.args.with_gas_price {
                    Some(price) => price.to(),
                    None => provider.get_gas_price().await?,
                }),
                None,
            )
        } else {
            let estimate =
                estimate_eip1559_fees(&provider, self.config.eip1559_fee_estimate).await?;
            let fees = resolve_broadcast_eip1559_fees(
                estimate,
                self.args.with_gas_price.map(|price| price.to()),
                self.args.priority_gas_price.map(|price| price.to()),
                None,
            )?;
            (None, Some(fees.estimation()))
        };

        for index in 0..sequence.transactions.len() {
            let metadata = &sequence.transactions[index];
            let kind = match metadata.transaction.clone() {
                TransactionMaybeSigned::Signed { tx, .. } => SendTransactionKind::Signed(tx),
                TransactionMaybeSigned::Unsigned(mut request) => {
                    let sender =
                        request.from.ok_or_else(|| eyre::eyre!("missing transaction sender"))?;
                    request.set_chain_id(sequence.chain);
                    if request.kind().is_none() {
                        request.set_create();
                    }
                    if let Some(price) = gas_price {
                        request.set_gas_price(price);
                    } else if let Some(fees) = fees {
                        request.set_max_fee_per_gas(fees.max_fee_per_gas);
                        request.set_max_priority_fee_per_gas(fees.max_priority_fee_per_gas);
                    }
                    send_kind.for_sender(sequence.chain, &sender, request)?
                }
            };
            let estimate_via_rpc = self.args.skip_simulation;
            let hash = kind
                .prepare_and_send(
                    provider.clone(),
                    true,
                    metadata.is_fixed_gas_limit,
                    estimate_via_rpc,
                    self.args.gas_estimate_multiplier,
                    None,
                    Some(chain),
                )
                .await?;
            sequence.add_pending(index, hash);
            sequence.save(true, false)?;

            let (_, status) = check_tx_status(
                &provider,
                hash,
                self.config.transaction_timeout,
                self.args.confirmations,
            )
            .await;
            match status? {
                TxStatus::Dropped => eyre::bail!("transaction {hash} was dropped"),
                TxStatus::Success(receipt) | TxStatus::Revert(receipt) => {
                    let success = receipt.status();
                    let message = format_receipt(chain, &receipt, Some(&sequence));
                    if !message.is_empty() {
                        sh_println!("{message}")?;
                    }
                    sequence.remove_pending(hash);
                    sequence.add_receipt(receipt);
                    sequence.save(true, false)?;
                    eyre::ensure!(success, "Transaction Failure: {hash:?}");
                }
            }
        }
        sequence.save(false, true)?;
        Ok(())
    }
}
