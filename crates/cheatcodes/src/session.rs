//! Persistent cheatcode session data, separate from the native execution hooks.
//!
//! TODO(evm2): Port recording, fuzz storage hooks and broadcast production to native callbacks.
//! Their session data remains available to runners, but unsupported cheatcodes are rejected by
//! native dispatch. This module does not execute a legacy interpreter or own its journal.

use crate::{CheatcodeAnalysis, CheatsConfig, Vm::AccountAccess, Wallets};
use alloy_network::{Ethereum, Network};
use alloy_primitives::{
    Address, B256, U256,
    map::{AddressHashMap, HashMap, HashSet},
};
use foundry_common::TransactionMaybeSigned;
use foundry_evm_core::{
    Breakpoints,
    evm::{BlockEnvFor, EthEvmNetwork, FoundryEvmNetwork},
};
use proptest::test_runner::{RngAlgorithm, TestRng, TestRunner};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::Arc,
};

/// Helps collecting transactions from different forks.
#[derive(Clone, Debug)]
pub struct BroadcastableTransaction<N: Network = Ethereum> {
    /// The optional RPC URL.
    pub rpc: Option<String>,
    /// The transaction to broadcast.
    pub transaction: TransactionMaybeSigned<N>,
}

/// Transactions collected for broadcasting.
pub type BroadcastableTransactions<N = Ethereum> = VecDeque<BroadcastableTransaction<N>>;

/// A callback registered for a storage access hook.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StorageHook {
    /// Contract that receives the callback.
    pub callback_target: Address,
    /// Callback function selector.
    pub callback_selector: [u8; 4],
}

/// Holds data about arbitrary storage.
#[derive(Clone, Debug, Default)]
pub struct ArbitraryStorage {
    /// Mapping of arbitrary storage addresses to generated values (slot, arbitrary value).
    /// (SLOADs return random value if storage slot wasn't accessed).
    /// Changed values are recorded and used to copy storage to different addresses.
    values: HashMap<Address, HashMap<U256, U256>>,
    /// Mapping of address with storage copied to arbitrary storage address source.
    copies: HashMap<Address, Address>,
    /// Address with storage slots that should be overwritten even if previously set.
    overwrites: HashSet<Address>,
    /// Storage slots explicitly written with `vm.store`, grouped by address.
    explicit_slots: HashMap<Address, HashSet<U256>>,
}

impl ArbitraryStorage {
    /// Marks an address with arbitrary storage.
    pub fn mark_arbitrary(&mut self, address: &Address, overwrite: bool) {
        self.values.insert(*address, HashMap::default());
        self.explicit_slots.remove(address);
        if overwrite {
            self.overwrites.insert(*address);
        } else {
            self.overwrites.remove(address);
        }
    }

    /// Maps an address that copies storage with the arbitrary storage address.
    pub fn mark_copy(&mut self, from: &Address, to: &Address) {
        if self.values.contains_key(from) {
            self.copies.insert(*to, *from);
            if let Some(slots) = self.explicit_slots.get(from).cloned() {
                self.explicit_slots.insert(*to, slots);
            } else {
                self.explicit_slots.remove(to);
            }
        }
    }

    /// Marks a slot as explicitly written if the address has arbitrary or copied storage.
    fn mark_explicit(&mut self, address: Address, slot: U256) {
        if self.values.contains_key(&address) || self.copies.contains_key(&address) {
            self.explicit_slots.entry(address).or_default().insert(slot);
        }
    }

    /// Returns whether a slot was explicitly written for the given address.
    fn is_explicit(&self, address: Address, slot: U256) -> bool {
        self.explicit_slots.get(&address).is_some_and(|slots| slots.contains(&slot))
    }

    /// Returns addresses explicitly marked with arbitrary storage.
    fn targets(&self) -> impl Iterator<Item = Address> + '_ {
        self.values.keys().copied()
    }

    /// Returns addresses explicitly marked with arbitrary storage and whether nonzero slots are
    /// overwritten.
    fn target_overwrite_modes(&self) -> impl Iterator<Item = (Address, bool)> + '_ {
        self.values.keys().map(|address| (*address, self.overwrites.contains(address)))
    }

    /// Returns addresses that copy storage from arbitrary-storage targets.
    fn copied_targets(&self) -> impl Iterator<Item = Address> + '_ {
        self.copies.keys().copied()
    }

    /// Returns copied arbitrary-storage targets and their source address.
    fn copied_target_sources(&self) -> impl Iterator<Item = (Address, Address)> + '_ {
        self.copies.iter().map(|(target, source)| (*target, *source))
    }

    /// Caches a concrete value for a slot on an arbitrary-storage address or copied target.
    fn cache_value(&mut self, address: Address, slot: U256, data: U256) {
        if let Some(values) = self.values.get_mut(&address) {
            values.insert(slot, data);
            return;
        }

        let Some(source) = self.copies.get(&address).copied() else {
            return;
        };
        if let Some(values) = self.values.get_mut(&source) {
            values.insert(slot, data);
        }
    }

    /// Returns a cached arbitrary value for a slot.
    fn cached_value(&self, address: Address, slot: U256) -> Option<U256> {
        self.values.get(&address).and_then(|values| values.get(&slot)).copied()
    }
}

/// Contains locations of traces ignored via cheatcodes.
///
/// The way we identify location in traces is by (node_idx, item_idx) tuple where node_idx is an
/// index of a call trace node, and item_idx is a value between 0 and `node.ordering.len()` where i
/// represents point after ith item, and 0 represents the beginning of the node trace.
#[derive(Debug, Default, Clone)]
pub struct IgnoredTraces {
    /// Mapping from (start_node_idx, start_item_idx) to (end_node_idx, end_item_idx) representing
    /// ranges of trace nodes to ignore.
    pub ignored: HashMap<(usize, usize), (usize, usize)>,
    /// Keeps track of (start_node_idx, start_item_idx) of the last `vm.pauseTracing` call.
    pub last_pause_call: Option<(usize, usize)>,
}

/// Gas observations retained across calls.
#[derive(Clone, Debug, Default)]
pub struct GasMetering {
    /// Gas used by the preceding call.
    pub last_call_gas: Option<crate::Vm::Gas>,
    /// Gas used by the preceding call or creation.
    pub last_frame_gas: Option<crate::Vm::Gas>,
}

/// Persistent configuration and observations for a native cheatcode session.
#[derive(Clone, Debug)]
pub struct Cheatcodes<FEN: FoundryEvmNetwork = EthEvmNetwork> {
    /// Source analysis available to cheatcodes.
    pub analysis: Option<CheatcodeAnalysis>,
    /// Block updates retained between native transactions.
    pub block: Option<BlockEnvFor<FEN>>,
    /// Gas price override retained between transactions.
    pub gas_price: Option<u128>,
    /// User-provided address labels.
    pub labels: AddressHashMap<String>,
    /// Transactions collected for script broadcasting.
    pub broadcastable_transactions: BroadcastableTransactions<FEN::Network>,
    /// Cheatcode configuration.
    pub config: Arc<CheatsConfig>,
    /// Additional addresses recognized as cheatcode contracts.
    pub extra_cheatcode_addresses: &'static [Address],
    /// Gas observations exposed to runners.
    pub gas_metering: GasMetering,
    /// Gas snapshots collected during a test suite.
    pub gas_snapshots: BTreeMap<String, BTreeMap<String, String>>,
    /// Source breakpoints collected during execution.
    pub breakpoints: Breakpoints,
    /// Trace ranges excluded from reporting.
    pub ignored_traces: IgnoredTraces,
    /// Arbitrary-storage replay data.
    pub arbitrary_storage: Option<ArbitraryStorage>,
    /// Deprecated selectors encountered during execution.
    pub deprecated: HashMap<&'static str, Option<&'static str>>,
    /// Main script contract when execution protection is enabled.
    pub script_address: Option<Address>,
    /// Wallets available to scripts.
    pub wallets: Option<Wallets>,
    recorded_account_diffs_stack: Option<Vec<Vec<AccountAccess>>>,
    pending_account_diffs: Option<Arc<[AccountAccess]>>,
    recorded_account_diffs_prefix: Option<Arc<[AccountAccess]>>,
    test_runner: Option<TestRunner>,
    storage_load_hooks: AddressHashMap<StorageHook>,
    storage_store_hooks: AddressHashMap<StorageHook>,
    mapping_storage_store_hooks: AddressHashMap<HashMap<B256, StorageHook>>,
}

impl Default for Cheatcodes {
    fn default() -> Self {
        Self::new(Arc::default())
    }
}

impl<FEN: FoundryEvmNetwork> Cheatcodes<FEN> {
    /// Creates a native cheatcode session with the supplied configuration.
    pub fn new(config: Arc<CheatsConfig>) -> Self {
        Self {
            labels: config.labels.clone(),
            config,
            extra_cheatcode_addresses: &[],
            analysis: Default::default(),
            block: Default::default(),
            gas_price: Default::default(),
            broadcastable_transactions: Default::default(),
            gas_metering: Default::default(),
            gas_snapshots: Default::default(),
            breakpoints: Default::default(),
            ignored_traces: Default::default(),
            arbitrary_storage: Default::default(),
            deprecated: Default::default(),
            script_address: Default::default(),
            wallets: Default::default(),
            recorded_account_diffs_stack: Default::default(),
            pending_account_diffs: Default::default(),
            recorded_account_diffs_prefix: Default::default(),
            test_runner: Default::default(),
            storage_load_hooks: Default::default(),
            storage_store_hooks: Default::default(),
            mapping_storage_store_hooks: Default::default(),
        }
    }

    /// Sets additional addresses recognized as cheatcode contracts.
    #[inline]
    pub const fn set_extra_cheatcode_addresses(&mut self, addresses: &'static [Address]) {
        self.extra_cheatcode_addresses = addresses;
    }

    /// Enables cheatcode analysis capabilities by providing a solar compiler instance.
    pub fn set_analysis(&mut self, analysis: CheatcodeAnalysis) {
        self.analysis = Some(analysis);
    }

    /// Starts an internal account diff recording session for test runner setup.
    pub fn start_internal_state_diff_recording(&mut self) -> bool {
        if self.recorded_account_diffs_stack.is_some()
            || self.recorded_account_diffs_prefix.is_some()
        {
            return false;
        }
        self.recorded_account_diffs_stack = Some(Default::default());
        true
    }

    /// Stops an internal account diff recording session without leaving recording enabled.
    pub fn stop_internal_state_diff_recording(&mut self) -> Vec<AccountAccess> {
        self.recorded_account_diffs_stack.take().unwrap_or_default().into_iter().flatten().collect()
    }

    /// Makes account accesses captured by the test runner available to the next recording session.
    pub fn set_pending_account_diffs(&mut self, accesses: Vec<AccountAccess>) {
        self.pending_account_diffs = (!accesses.is_empty()).then(|| Arc::from(accesses));
    }

    /// Starts a user account diff recording session, including pending test runner accesses.
    pub fn start_state_diff_recording(&mut self) {
        self.recorded_account_diffs_prefix = self.pending_account_diffs.take();
        self.recorded_account_diffs_stack = Some(Default::default());
    }

    /// Returns completed and active account accesses in execution order.
    pub fn recorded_account_diffs(&self) -> impl Iterator<Item = &AccountAccess> {
        self.recorded_account_diffs_prefix
            .iter()
            .flat_map(|prefix| prefix.iter())
            .chain(self.recorded_account_diffs_stack.iter().flatten().flatten())
    }

    /// Takes completed account accesses from the active user recording session.
    pub fn take_recorded_account_diffs_prefix(&mut self) -> Vec<AccountAccess> {
        self.recorded_account_diffs_prefix
            .take()
            .map(|prefix| prefix.as_ref().to_vec())
            .unwrap_or_default()
    }

    /// Sets the wallets available to this session.
    pub fn set_wallets(&mut self, wallets: Wallets) {
        self.wallets = Some(wallets);
    }

    pub fn test_runner(&mut self) -> &mut TestRunner {
        self.test_runner.get_or_insert_with(|| match self.config.seed {
            Some(seed) => TestRunner::new_with_rng(
                proptest::test_runner::Config::default(),
                TestRng::from_seed(RngAlgorithm::ChaCha, &seed.to_be_bytes::<32>()),
            ),
            None => TestRunner::new(proptest::test_runner::Config::default()),
        })
    }

    pub fn set_seed(&mut self, seed: U256) {
        self.test_runner = Some(TestRunner::new_with_rng(
            proptest::test_runner::Config::default(),
            TestRng::from_seed(RngAlgorithm::ChaCha, &seed.to_be_bytes::<32>()),
        ));
    }

    /// Returns existing or set a default `ArbitraryStorage` option.
    /// Used by `setArbitraryStorage` cheatcode to track addresses with arbitrary storage.
    pub fn arbitrary_storage(&mut self) -> &mut ArbitraryStorage {
        self.arbitrary_storage.get_or_insert_with(ArbitraryStorage::default)
    }

    /// Returns addresses explicitly marked with arbitrary storage.
    pub fn arbitrary_storage_targets(&self) -> impl Iterator<Item = Address> + '_ {
        self.arbitrary_storage.as_ref().into_iter().flat_map(ArbitraryStorage::targets)
    }

    /// Returns addresses explicitly marked with arbitrary storage and whether nonzero slots are
    /// overwritten.
    pub fn arbitrary_storage_target_overwrite_modes(
        &self,
    ) -> impl Iterator<Item = (Address, bool)> + '_ {
        self.arbitrary_storage
            .as_ref()
            .into_iter()
            .flat_map(ArbitraryStorage::target_overwrite_modes)
    }

    /// Returns addresses that copy storage from arbitrary-storage targets.
    pub fn arbitrary_storage_copied_targets(&self) -> impl Iterator<Item = Address> + '_ {
        self.arbitrary_storage.as_ref().into_iter().flat_map(ArbitraryStorage::copied_targets)
    }

    /// Returns copied arbitrary-storage targets and their source address.
    pub fn arbitrary_storage_copied_target_sources(
        &self,
    ) -> impl Iterator<Item = (Address, Address)> + '_ {
        self.arbitrary_storage
            .as_ref()
            .into_iter()
            .flat_map(ArbitraryStorage::copied_target_sources)
    }

    /// Caches a concrete replay value for a slot on an arbitrary-storage address or copied target.
    pub fn cache_arbitrary_storage_value(&mut self, address: Address, slot: U256, value: U256) {
        if let Some(storage) = &mut self.arbitrary_storage {
            storage.cache_value(address, slot, value);
        }
    }

    /// Marks a slot as explicitly written with `vm.store`.
    pub fn mark_arbitrary_storage_slot_explicit(&mut self, address: Address, slot: U256) {
        if let Some(storage) = &mut self.arbitrary_storage {
            storage.mark_explicit(address, slot);
        }
    }

    /// Returns whether a slot was explicitly written with `vm.store`.
    pub fn is_arbitrary_storage_slot_explicit(&self, address: Address, slot: U256) -> bool {
        self.arbitrary_storage.as_ref().is_some_and(|storage| storage.is_explicit(address, slot))
    }

    /// Returns a cached arbitrary-storage replay value for a slot.
    pub fn cached_arbitrary_storage_value(&self, address: Address, slot: U256) -> Option<U256> {
        self.arbitrary_storage.as_ref().and_then(|storage| storage.cached_value(address, slot))
    }

    /// Whether the given address has arbitrary storage.
    pub fn has_arbitrary_storage(&self, address: &Address) -> bool {
        match &self.arbitrary_storage {
            Some(storage) => storage.values.contains_key(address),
            None => false,
        }
    }

    /// Whether the given slot of address with arbitrary storage should be overwritten.
    /// True if address is marked as and overwrite and if no value was previously generated for
    /// given slot.
    pub fn should_overwrite_arbitrary_storage(
        &self,
        address: &Address,
        storage_slot: U256,
    ) -> bool {
        match &self.arbitrary_storage {
            Some(storage) => {
                storage.overwrites.contains(address)
                    && storage
                        .values
                        .get(address)
                        .and_then(|arbitrary_values| arbitrary_values.get(&storage_slot))
                        .is_none()
            }
            None => false,
        }
    }

    /// Whether the given address is a copy of an address with arbitrary storage.
    pub fn is_arbitrary_storage_copy(&self, address: &Address) -> bool {
        match &self.arbitrary_storage {
            Some(storage) => storage.copies.contains_key(address),
            None => false,
        }
    }

    /// Registers an SLOAD callback, replacing the existing callback for `target`.
    pub fn register_storage_load_hook(
        &mut self,
        target: Address,
        callback_target: Address,
        callback_selector: [u8; 4],
    ) {
        self.storage_load_hooks.insert(target, StorageHook { callback_target, callback_selector });
    }

    /// Registers an SSTORE callback, replacing the existing callback for `target`.
    pub fn register_storage_store_hook(
        &mut self,
        target: Address,
        callback_target: Address,
        callback_selector: [u8; 4],
    ) {
        self.storage_store_hooks.insert(target, StorageHook { callback_target, callback_selector });
    }

    /// Registers a mapping SSTORE callback. Returns false when a raw hook conflicts.
    pub fn register_mapping_storage_store_hook(
        &mut self,
        target: Address,
        root_slot: B256,
        callback_target: Address,
        callback_selector: [u8; 4],
    ) -> bool {
        if self.storage_store_hooks.contains_key(&target) {
            return false;
        }
        self.mapping_storage_store_hooks
            .entry(target)
            .or_default()
            .insert(root_slot, StorageHook { callback_target, callback_selector });
        true
    }

    /// Returns registered mapping SSTORE callbacks.
    pub fn mapping_storage_store_hooks(
        &self,
    ) -> impl Iterator<Item = (Address, B256, StorageHook)> + '_ {
        self.mapping_storage_store_hooks
            .iter()
            .flat_map(|(target, hooks)| hooks.iter().map(|(root, hook)| (*target, *root, *hook)))
    }

    /// Returns whether mapping hooks conflict with a raw store hook.
    pub fn has_mapping_storage_store_hooks(&self, target: Address) -> bool {
        self.mapping_storage_store_hooks.get(&target).is_some_and(|hooks| !hooks.is_empty())
    }

    /// Returns registered SLOAD callbacks.
    pub fn storage_load_hooks(&self) -> impl Iterator<Item = (Address, StorageHook)> + '_ {
        self.storage_load_hooks.iter().map(|(target, hook)| (*target, *hook))
    }

    /// Returns registered SSTORE callbacks.
    pub fn storage_store_hooks(&self) -> impl Iterator<Item = (Address, StorageHook)> + '_ {
        self.storage_store_hooks.iter().map(|(target, hook)| (*target, *hook))
    }
}
