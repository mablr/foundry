use alloy_network::Network;
use alloy_primitives::{Address, B256, Bytes};
use foundry_common::TransactionMaybeSigned;
use serde::{Deserialize, Serialize};

/// Call classification stored in script broadcast files.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ScriptTransactionKind {
    #[default]
    Call,
    StaticCall,
    CallCode,
    DelegateCall,
    AuthCall,
    Create,
    Create2,
}

impl ScriptTransactionKind {
    /// Returns whether the transaction creates a contract.
    pub const fn is_any_create(self) -> bool {
        matches!(self, Self::Create | Self::Create2)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdditionalContract {
    #[serde(rename = "transactionType")]
    pub call_kind: ScriptTransactionKind,
    pub contract_name: Option<String>,
    pub address: Address,
    pub init_code: Bytes,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub creator_code_addresses: Vec<Address>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(
    rename_all = "camelCase",
    bound(
        serialize = "N::TransactionRequest: Serialize, N::TxEnvelope: Serialize",
        deserialize = "N::TransactionRequest: for<'de2> Deserialize<'de2>, N::TxEnvelope: for<'de2> Deserialize<'de2>"
    )
)]
pub struct TransactionWithMetadata<N: Network> {
    pub hash: Option<B256>,
    #[serde(rename = "transactionType")]
    pub call_kind: ScriptTransactionKind,
    #[serde(default = "default_string")]
    pub contract_name: Option<String>,
    #[serde(default = "default_address")]
    pub contract_address: Option<Address>,
    #[serde(default = "default_string")]
    pub function: Option<String>,
    pub function_abi: Option<String>,
    #[serde(skip)]
    pub display_function: Option<String>,
    #[serde(default = "default_vec_of_strings")]
    pub arguments: Option<Vec<String>>,
    #[serde(skip)]
    pub rpc: String,
    pub transaction: TransactionMaybeSigned<N>,
    #[serde(default)]
    pub additional_contracts: Vec<AdditionalContract>,
    #[serde(default)]
    pub is_fixed_gas_limit: bool,
}

const fn default_string() -> Option<String> {
    Some(String::new())
}

const fn default_address() -> Option<Address> {
    Some(Address::ZERO)
}

const fn default_vec_of_strings() -> Option<Vec<String>> {
    Some(vec![])
}

impl<N: Network> TransactionWithMetadata<N> {
    pub fn from_tx_request(transaction: TransactionMaybeSigned<N>) -> Self {
        Self {
            transaction,
            hash: Default::default(),
            call_kind: Default::default(),
            contract_name: Default::default(),
            contract_address: Default::default(),
            function: Default::default(),
            function_abi: Default::default(),
            display_function: Default::default(),
            arguments: Default::default(),
            is_fixed_gas_limit: Default::default(),
            additional_contracts: Default::default(),
            rpc: Default::default(),
        }
    }

    pub const fn tx(&self) -> &TransactionMaybeSigned<N> {
        &self.transaction
    }

    pub const fn tx_mut(&mut self) -> &mut TransactionMaybeSigned<N> {
        &mut self.transaction
    }

    pub fn is_create2(&self) -> bool {
        self.call_kind == ScriptTransactionKind::Create2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_transaction_kind_preserves_broadcast_json() {
        for (kind, json) in [
            (ScriptTransactionKind::Call, "\"CALL\""),
            (ScriptTransactionKind::StaticCall, "\"STATICCALL\""),
            (ScriptTransactionKind::CallCode, "\"CALLCODE\""),
            (ScriptTransactionKind::DelegateCall, "\"DELEGATECALL\""),
            (ScriptTransactionKind::AuthCall, "\"AUTHCALL\""),
            (ScriptTransactionKind::Create, "\"CREATE\""),
            (ScriptTransactionKind::Create2, "\"CREATE2\""),
        ] {
            assert_eq!(serde_json::to_string(&kind).unwrap(), json);
            assert_eq!(serde_json::from_str::<ScriptTransactionKind>(json).unwrap(), kind);
        }
        assert_eq!(ScriptTransactionKind::default(), ScriptTransactionKind::Call);
    }

    #[test]
    fn additional_contract_creator_code_addresses_are_backward_compatible() {
        let old_json = serde_json::json!({
            "transactionType": "CREATE",
            "contractName": null,
            "address": Address::repeat_byte(0x11),
            "initCode": "0x6000"
        });
        let contract: AdditionalContract = serde_json::from_value(old_json.clone()).unwrap();
        assert!(contract.creator_code_addresses.is_empty());
        assert_eq!(serde_json::to_value(contract).unwrap(), old_json);

        let creator = Address::repeat_byte(0x22);
        let contract = AdditionalContract {
            call_kind: ScriptTransactionKind::Create2,
            contract_name: None,
            address: Address::repeat_byte(0x33),
            init_code: Bytes::new(),
            creator_code_addresses: vec![creator],
        };
        assert_eq!(
            serde_json::to_value(contract).unwrap()["creatorCodeAddresses"],
            serde_json::json!([creator])
        );
    }
}
