use crate::{Cheatcode, Cheatcodes, Result, Vm::*};
use alloy_network::Network;
use alloy_primitives::{Address, B256};
use alloy_sol_types::SolValue;
use foundry_common::mapping_slots::MappingSlots;
use foundry_evm_core::EthCheatCtx;

impl<CTX: EthCheatCtx, N: Network> Cheatcode<CTX, N> for startMappingRecordingCall {
    fn apply(&self, state: &mut Cheatcodes<CTX, N>) -> Result {
        let Self {} = self;
        state.mapping_slots.get_or_insert_default();
        Ok(Default::default())
    }
}

impl<CTX: EthCheatCtx, N: Network> Cheatcode<CTX, N> for stopMappingRecordingCall {
    fn apply(&self, state: &mut Cheatcodes<CTX, N>) -> Result {
        let Self {} = self;
        state.mapping_slots = None;
        Ok(Default::default())
    }
}

impl<CTX: EthCheatCtx, N: Network> Cheatcode<CTX, N> for getMappingLengthCall {
    fn apply(&self, state: &mut Cheatcodes<CTX, N>) -> Result {
        let Self { target, mappingSlot } = self;
        let result = slot_child(state, target, mappingSlot).map(Vec::len).unwrap_or(0);
        Ok((result as u64).abi_encode())
    }
}

impl<CTX: EthCheatCtx, N: Network> Cheatcode<CTX, N> for getMappingSlotAtCall {
    fn apply(&self, state: &mut Cheatcodes<CTX, N>) -> Result {
        let Self { target, mappingSlot, idx } = self;
        let result = slot_child(state, target, mappingSlot)
            .and_then(|set| set.get(idx.saturating_to::<usize>()))
            .copied()
            .unwrap_or_default();
        Ok(result.abi_encode())
    }
}

impl<CTX: EthCheatCtx, N: Network> Cheatcode<CTX, N> for getMappingKeyAndParentOfCall {
    fn apply(&self, state: &mut Cheatcodes<CTX, N>) -> Result {
        let Self { target, elementSlot: slot } = self;
        let mut found = false;
        let mut key = &B256::ZERO;
        let mut parent = &B256::ZERO;
        if let Some(slots) = mapping_slot(state, target) {
            if let Some(key2) = slots.keys.get(slot) {
                found = true;
                key = key2;
                parent = &slots.parent_slots[slot];
            } else if let Some((key2, parent2)) = slots.seen_sha3.get(slot) {
                found = true;
                key = key2;
                parent = parent2;
            }
        }
        Ok((found, key, parent).abi_encode_params())
    }
}

fn mapping_slot<'a, CTX: EthCheatCtx, N: Network>(
    state: &'a Cheatcodes<CTX, N>,
    target: &'a Address,
) -> Option<&'a MappingSlots> {
    state.mapping_slots.as_ref()?.get(target)
}

fn slot_child<'a, CTX: EthCheatCtx, N: Network>(
    state: &'a Cheatcodes<CTX, N>,
    target: &'a Address,
    slot: &'a B256,
) -> Option<&'a Vec<B256>> {
    mapping_slot(state, target)?.children.get(slot)
}
