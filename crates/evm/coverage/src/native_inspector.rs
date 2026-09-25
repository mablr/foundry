//! Opcode and call hit collection for evm2 execution.

use crate::{CallData, HitMap, HitMaps};
use evm2::{
    EvmTypesHost, Inspector,
    interpreter::{Interpreter, Message, MessageResult},
};

/// Collects bytecode hits from native EVM execution.
#[derive(Clone, Debug, Default)]
pub struct NativeLineCoverageCollector {
    maps: HitMaps,
}

impl NativeLineCoverageCollector {
    /// Drains the hit maps collected so far.
    pub fn take_maps(&mut self) -> HitMaps {
        std::mem::take(&mut self.maps)
    }

    fn map(
        &mut self,
        hash: alloy_primitives::B256,
        bytecode: alloy_primitives::Bytes,
    ) -> &mut HitMap {
        self.maps.entry(hash).or_insert_with(|| HitMap::new(bytecode))
    }
}

impl<T: EvmTypesHost> Inspector<T> for NativeLineCoverageCollector {
    fn step(&mut self, interp: &mut Interpreter<'_, '_, T>) {
        self.map(interp.original_bytecode_hash(), interp.original_bytecode())
            .hit(interp.pc() as u32);
    }

    fn call(
        &mut self,
        _interp: &mut Interpreter<'_, '_, T>,
        message: &mut Message<T>,
    ) -> Option<MessageResult<T>> {
        self.map(message.code.hash_slow(), message.code.original_bytes())
            .call(CallData::new(&message.input), !message.value.is_zero());
        None
    }

    fn create_end(
        &mut self,
        _interp: &mut Interpreter<'_, '_, T>,
        message: &Message<T>,
        result: &mut MessageResult<T>,
    ) {
        if result.stop.is_success() {
            self.map(message.code.hash_slow(), message.code.original_bytes()).creation();
        }
    }
}
