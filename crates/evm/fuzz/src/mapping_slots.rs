//! REVM mapping observations used by the legacy fuzz inspector.
//!
//! TODO(evm2): Capture these observations through native opcode hooks when fuzzing migrates.

use alloy_primitives::{Address, B256, U256, map::AddressHashMap};
use foundry_common::mapping_slots::MappingSlots;
use revm::{
    bytecode::opcode,
    interpreter::{Interpreter, interpreter_types::Jumps},
};

/// A pending 64-byte Keccak operation captured before execution.
#[derive(Clone, Copy, Debug)]
pub struct PendingMappingHash {
    /// The effective storage address of the executing frame.
    pub address: Address,
    /// The memory offset containing the Keccak preimage.
    pub offset: usize,
}

/// Captures a 64-byte Keccak operation before execution.
pub fn capture_hash(interpreter: &Interpreter) -> Option<PendingMappingHash> {
    if interpreter.bytecode.opcode() != opcode::KECCAK256
        || interpreter.stack.peek(1).ok()? != U256::from(0x40)
    {
        return None;
    }
    Some(PendingMappingHash {
        address: interpreter.input.target_address,
        offset: interpreter.stack.peek(0).ok()?.try_into().ok()?,
    })
}

/// Records a successfully executed 64-byte Keccak operation after memory expansion.
pub fn record_hash(
    mapping_slots: &mut AddressHashMap<MappingSlots>,
    interpreter: &Interpreter,
    pending: PendingMappingHash,
) {
    let Ok(result) = interpreter.stack.peek(0) else { return };
    let data = interpreter.memory.slice_len(pending.offset, 0x40);
    let key = B256::from_slice(&data[..0x20]);
    let parent = B256::from_slice(&data[0x20..]);
    mapping_slots.entry(pending.address).or_default().record_hash(result.into(), key, parent);
}

/// Function to be used in `Inspector::step` to record mapping slots.
#[cold]
pub fn step(mapping_slots: &mut AddressHashMap<MappingSlots>, interpreter: &Interpreter) {
    if interpreter.bytecode.opcode() == opcode::SSTORE
        && let Some(mapping_slots) = mapping_slots.get_mut(&interpreter.input.target_address)
        && let Ok(slot) = interpreter.stack.peek(0)
    {
        mapping_slots.insert(slot.into());
    }
}
