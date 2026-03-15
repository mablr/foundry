pub use alloy_evm::EvmEnv;
use alloy_primitives::{Address, B256, Bytes, U256};
use revm::{
    Context, Database,
    context::{Block, BlockEnv, Cfg, CfgEnv, JournalTr, Transaction, TxEnv},
    context_interface::{
        ContextTr,
        block::blob::BlobExcessGasAndPrice,
        either::Either,
        transaction::{AccessList, AccessListItem, RecoveredAuthorization, SignedAuthorization},
    },
    primitives::{TxKind, hardfork::SpecId},
};

/// Helper container type for [`EvmEnv`] and [`TxEnv`].
#[derive(Clone, Debug, Default)]
pub struct Env {
    pub evm_env: EvmEnv,
    pub tx: TxEnv,
}

/// Helper container type for [`EvmEnv`] and [`TxEnv`].
impl Env {
    pub fn from(cfg: CfgEnv, block: BlockEnv, tx: TxEnv) -> Self {
        Self { evm_env: EvmEnv { cfg_env: cfg, block_env: block }, tx }
    }

    pub fn new_with_spec_id(cfg: CfgEnv, block: BlockEnv, tx: TxEnv, spec_id: SpecId) -> Self {
        let mut cfg = cfg;
        cfg.spec = spec_id;

        Self::from(cfg, block, tx)
    }

    /// Clones the evm env and tx env separately from a [`FoundryContextExt`] context.
    pub fn clone_evm_and_tx(ecx: &mut impl FoundryContextExt) -> (EvmEnv, TxEnv) {
        (
            EvmEnv { cfg_env: ecx.cfg_mut().clone(), block_env: ecx.block_mut().clone() },
            ecx.tx_mut().clone(),
        )
    }

    /// Writes the split evm env and tx env back into a [`FoundryContextExt`] context.
    pub fn apply_evm_and_tx(ecx: &mut impl FoundryContextExt, evm_env: EvmEnv, tx_env: TxEnv) {
        *ecx.block_mut() = evm_env.block_env;
        *ecx.cfg_mut() = evm_env.cfg_env;
        *ecx.tx_mut() = tx_env;
    }
}

/// Extension of [`Block`] with mutable setters, allowing EVM-agnostic mutation of block fields.
pub trait FoundryBlock: Block {
    /// Sets the block number.
    fn set_number(&mut self, number: U256);

    /// Sets the beneficiary (coinbase) address.
    fn set_beneficiary(&mut self, beneficiary: Address);

    /// Sets the block timestamp.
    fn set_timestamp(&mut self, timestamp: U256);

    /// Sets the gas limit.
    fn set_gas_limit(&mut self, gas_limit: u64);

    /// Sets the base fee per gas.
    fn set_basefee(&mut self, basefee: u64);

    /// Sets the block difficulty.
    fn set_difficulty(&mut self, difficulty: U256);

    /// Sets the prevrandao value.
    fn set_prevrandao(&mut self, prevrandao: Option<B256>);

    /// Sets the blob excess gas and price from an [`Option<BlobExcessGasAndPrice>`].
    fn set_blob_excess_gas_and_price(
        &mut self,
        blob_excess_gas_and_price: Option<BlobExcessGasAndPrice>,
    );

    /// Sets all block fields from another [`FoundryBlock`] implementation.
    fn set_all(&mut self, block: &impl FoundryBlock) {
        self.set_number(block.number());
        self.set_beneficiary(block.beneficiary());
        self.set_timestamp(block.timestamp());
        self.set_gas_limit(block.gas_limit());
        self.set_basefee(block.basefee());
        self.set_difficulty(block.difficulty());
        self.set_prevrandao(block.prevrandao());
        self.set_blob_excess_gas_and_price(block.blob_excess_gas_and_price());
    }
}

impl FoundryBlock for BlockEnv {
    fn set_number(&mut self, number: U256) {
        self.number = number;
    }

    fn set_beneficiary(&mut self, beneficiary: Address) {
        self.beneficiary = beneficiary;
    }

    fn set_timestamp(&mut self, timestamp: U256) {
        self.timestamp = timestamp;
    }

    fn set_gas_limit(&mut self, gas_limit: u64) {
        self.gas_limit = gas_limit;
    }

    fn set_basefee(&mut self, basefee: u64) {
        self.basefee = basefee;
    }

    fn set_difficulty(&mut self, difficulty: U256) {
        self.difficulty = difficulty;
    }

    fn set_prevrandao(&mut self, prevrandao: Option<B256>) {
        self.prevrandao = prevrandao;
    }

    fn set_blob_excess_gas_and_price(
        &mut self,
        blob_excess_gas_and_price: Option<BlobExcessGasAndPrice>,
    ) {
        self.blob_excess_gas_and_price = blob_excess_gas_and_price;
    }
}

/// Extension of [`Transaction`] with mutable setters, allowing EVM-agnostic mutation of transaction
/// fields.
pub trait FoundryTransaction: Transaction {
    /// Sets the transaction type.
    fn set_tx_type(&mut self, tx_type: u8);

    /// Sets the caller (sender) address.
    fn set_caller(&mut self, caller: Address);

    /// Sets the gas limit.
    fn set_gas_limit(&mut self, gas_limit: u64);

    /// Sets the gas price (or max fee per gas for EIP-1559).
    fn set_gas_price(&mut self, gas_price: u128);

    /// Sets the transaction kind (call or create).
    fn set_kind(&mut self, kind: TxKind);

    /// Sets the value sent with the transaction.
    fn set_value(&mut self, value: U256);

    /// Sets the transaction input data.
    fn set_data(&mut self, data: Bytes);

    /// Sets the nonce.
    fn set_nonce(&mut self, nonce: u64);

    /// Sets the chain ID.
    fn set_chain_id(&mut self, chain_id: Option<u64>);

    /// Sets the access list from an iterator of [`Self::AccessListItem`], aligned with
    /// [`Transaction::access_list`].
    fn set_access_list<'a>(
        &mut self,
        access_list: impl IntoIterator<Item = Self::AccessListItem<'a>>,
    ) where
        Self: 'a;

    /// Sets the EIP-7702 authorization list from an iterator of [`Self::Authorization`], aligned
    /// with [`Transaction::authorization_list`].
    fn set_authorization_list<'a>(
        &mut self,
        authorization_list: impl IntoIterator<Item = Self::Authorization<'a>>,
    ) where
        Self: 'a;

    /// Sets the max priority fee per gas.
    fn set_gas_priority_fee(&mut self, gas_priority_fee: Option<u128>);

    /// Sets the blob versioned hashes.
    fn set_blob_hashes(&mut self, blob_hashes: Vec<B256>);

    /// Sets the max fee per blob gas.
    fn set_max_fee_per_blob_gas(&mut self, max_fee_per_blob_gas: u128);

    /// Sets all transaction fields from another instance of the same type.
    fn set_all(&mut self, tx: &Self);
}

impl FoundryTransaction for TxEnv {
    fn set_tx_type(&mut self, tx_type: u8) {
        self.tx_type = tx_type;
    }

    fn set_caller(&mut self, caller: Address) {
        self.caller = caller;
    }

    fn set_gas_limit(&mut self, gas_limit: u64) {
        self.gas_limit = gas_limit;
    }

    fn set_gas_price(&mut self, gas_price: u128) {
        self.gas_price = gas_price;
    }

    fn set_kind(&mut self, kind: TxKind) {
        self.kind = kind;
    }

    fn set_value(&mut self, value: U256) {
        self.value = value;
    }

    fn set_data(&mut self, data: Bytes) {
        self.data = data;
    }

    fn set_nonce(&mut self, nonce: u64) {
        self.nonce = nonce;
    }

    fn set_chain_id(&mut self, chain_id: Option<u64>) {
        self.chain_id = chain_id;
    }

    fn set_access_list<'a>(&mut self, access_list: impl IntoIterator<Item = &'a AccessListItem>)
    where
        Self: 'a,
    {
        self.access_list = AccessList(access_list.into_iter().cloned().collect());
    }

    fn set_authorization_list<'a>(
        &mut self,
        authorization_list: impl IntoIterator<
            Item = &'a Either<SignedAuthorization, RecoveredAuthorization>,
        >,
    ) where
        Self: 'a,
    {
        self.authorization_list = authorization_list.into_iter().cloned().collect();
    }

    fn set_gas_priority_fee(&mut self, gas_priority_fee: Option<u128>) {
        self.gas_priority_fee = gas_priority_fee;
    }

    fn set_blob_hashes(&mut self, blob_hashes: Vec<B256>) {
        self.blob_hashes = blob_hashes;
    }

    fn set_max_fee_per_blob_gas(&mut self, max_fee_per_blob_gas: u128) {
        self.max_fee_per_blob_gas = max_fee_per_blob_gas;
    }

    fn set_all(&mut self, tx: &Self) {
        self.set_tx_type(tx.tx_type);
        self.set_caller(tx.caller);
        self.set_gas_limit(tx.gas_limit);
        self.set_gas_price(tx.gas_price);
        self.set_kind(tx.kind);
        self.set_value(tx.value);
        self.set_data(tx.data.clone());
        self.set_nonce(tx.nonce);
        self.set_chain_id(tx.chain_id);
        self.set_access_list(tx.access_list.0.iter());
        self.set_gas_priority_fee(tx.gas_priority_fee);
        self.set_blob_hashes(tx.blob_hashes.clone());
        self.set_max_fee_per_blob_gas(tx.max_fee_per_blob_gas);
        self.set_authorization_list(tx.authorization_list.iter());
    }
}

/// Extension of [`Cfg`] with mutable setters, allowing EVM-agnostic mutation of EVM configuration
/// fields.
pub trait FoundryCfg: Cfg {
    /// Sets the EVM spec (hardfork).
    fn set_spec(&mut self, spec: Self::Spec);

    /// Sets the chain ID.
    fn set_chain_id(&mut self, chain_id: u64);

    /// Sets the contract code size limit.
    fn set_limit_contract_code_size(&mut self, limit: Option<usize>);

    /// Sets the contract initcode size limit.
    fn set_limit_contract_initcode_size(&mut self, limit: Option<usize>);

    /// Sets whether nonce checks are disabled.
    fn set_disable_nonce_check(&mut self, disabled: bool);

    /// Sets the max blobs per transaction.
    fn set_max_blobs_per_tx(&mut self, max: Option<u64>);

    /// Sets the blob base fee update fraction.
    fn set_blob_base_fee_update_fraction(&mut self, fraction: Option<u64>);

    /// Sets the transaction gas limit cap.
    fn set_tx_gas_limit_cap(&mut self, cap: Option<u64>);

    /// Returns the contract code size limit.
    fn limit_contract_code_size(&self) -> Option<usize>;

    /// Returns the contract initcode size limit.
    fn limit_contract_initcode_size(&self) -> Option<usize>;

    /// Returns the blob base fee update fraction.
    fn blob_base_fee_update_fraction(&self) -> Option<u64>;

    /// Returns the transaction gas limit cap.
    fn tx_gas_limit_cap_opt(&self) -> Option<u64>;

    /// Sets all cfg fields from another [`FoundryCfg`] implementation.
    fn set_all(&mut self, cfg: &impl FoundryCfg<Spec = Self::Spec>) {
        self.set_spec(cfg.spec());
        self.set_chain_id(cfg.chain_id());
        self.set_limit_contract_code_size(cfg.limit_contract_code_size());
        self.set_limit_contract_initcode_size(cfg.limit_contract_initcode_size());
        self.set_disable_nonce_check(cfg.is_nonce_check_disabled());
        self.set_max_blobs_per_tx(cfg.max_blobs_per_tx());
        self.set_blob_base_fee_update_fraction(cfg.blob_base_fee_update_fraction());
        self.set_tx_gas_limit_cap(cfg.tx_gas_limit_cap_opt());
    }
}

impl<S: Into<SpecId> + Clone> FoundryCfg for CfgEnv<S> {
    fn set_spec(&mut self, spec: S) {
        self.spec = spec;
    }

    fn set_chain_id(&mut self, chain_id: u64) {
        self.chain_id = chain_id;
    }

    fn set_limit_contract_code_size(&mut self, limit: Option<usize>) {
        self.limit_contract_code_size = limit;
    }

    fn set_limit_contract_initcode_size(&mut self, limit: Option<usize>) {
        self.limit_contract_initcode_size = limit;
    }

    fn set_disable_nonce_check(&mut self, disabled: bool) {
        self.disable_nonce_check = disabled;
    }

    fn set_max_blobs_per_tx(&mut self, max: Option<u64>) {
        self.max_blobs_per_tx = max;
    }

    fn set_blob_base_fee_update_fraction(&mut self, fraction: Option<u64>) {
        self.blob_base_fee_update_fraction = fraction;
    }

    fn set_tx_gas_limit_cap(&mut self, cap: Option<u64>) {
        self.tx_gas_limit_cap = cap;
    }

    fn limit_contract_code_size(&self) -> Option<usize> {
        self.limit_contract_code_size
    }

    fn limit_contract_initcode_size(&self) -> Option<usize> {
        self.limit_contract_initcode_size
    }

    fn blob_base_fee_update_fraction(&self) -> Option<u64> {
        self.blob_base_fee_update_fraction
    }

    fn tx_gas_limit_cap_opt(&self) -> Option<u64> {
        self.tx_gas_limit_cap
    }
}

/// Extension trait providing mutable field access to block, tx, and cfg environments.
///
/// [`ContextTr`] only exposes immutable references for block, tx, and cfg.
/// Cheatcodes like `vm.warp()`, `vm.roll()`, `vm.chainId()` need to mutate these fields.
pub trait FoundryContextExt:
    ContextTr<Block: FoundryBlock + Clone, Tx: FoundryTransaction + Clone, Cfg: FoundryCfg + Clone>
{
    /// Mutable reference to the block environment.
    fn block_mut(&mut self) -> &mut BlockEnv;
    /// Mutable reference to the transaction environment.
    fn tx_mut(&mut self) -> &mut TxEnv;
    /// Mutable reference to the configuration environment.
    fn cfg_mut(&mut self) -> &mut CfgEnv;
}

impl<DB: Database, J: JournalTr<Database = DB>, C> FoundryContextExt
    for Context<BlockEnv, TxEnv, CfgEnv, DB, J, C>
{
    fn block_mut(&mut self) -> &mut BlockEnv {
        &mut self.block
    }
    fn tx_mut(&mut self) -> &mut TxEnv {
        &mut self.tx
    }
    fn cfg_mut(&mut self) -> &mut CfgEnv {
        &mut self.cfg
    }
}

/// Alternative to [`FoundryContextExt`]
pub trait FoundryContextTr:
    ContextTr<Block: FoundryBlock + Clone, Tx: FoundryTransaction + Clone, Cfg: FoundryCfg + Clone>
{
    /// Mutable reference to the block environment.
    fn block_mut(&mut self) -> &mut Self::Block;
    /// Mutable reference to the transaction environment.
    fn tx_mut(&mut self) -> &mut Self::Tx;
    /// Mutable reference to the configuration environment.
    fn cfg_mut(&mut self) -> &mut Self::Cfg;
    /// Sets all block fields from a [`FoundryBlock`] implementation.
    fn set_block(&mut self, block: &Self::Block) {
        self.block_mut().set_all(block);
    }
    /// Sets all transaction fields from the context's transaction type.
    fn set_tx(&mut self, tx: &Self::Tx) {
        self.tx_mut().set_all(tx);
    }
    /// Sets all cfg fields from a [`FoundryCfg`] implementation with matching spec.
    fn set_cfg(&mut self, cfg: &Self::Cfg) {
        self.cfg_mut().set_all(cfg);
    }
}

impl<
    Block: FoundryBlock + Clone,
    Tx: FoundryTransaction + Clone,
    Cfg: FoundryCfg + Clone,
    DB: Database,
    J: JournalTr<Database = DB>,
> FoundryContextTr for Context<Block, Tx, Cfg, DB, J>
{
    fn block_mut(&mut self) -> &mut Block {
        &mut self.block
    }
    fn tx_mut(&mut self) -> &mut Tx {
        &mut self.tx
    }
    fn cfg_mut(&mut self) -> &mut Cfg {
        &mut self.cfg
    }
}
