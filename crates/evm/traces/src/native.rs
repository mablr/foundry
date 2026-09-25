//! Trace types and display safeguards for evm2 execution.

use alloy_dyn_abi::JsonAbiExt;
use alloy_json_abi::{Function, JsonAbi};
use alloy_primitives::{Bytes, Selector};
use evm2_inspectors::tracing::types::{DecodedCallData, DecodedCallTrace};
use foundry_common::{ContractsByArtifact, fmt::format_token};
use foundry_evm_core::{abi::Vm, constants::CHEATCODE_ADDRESS};
use std::collections::HashMap;

pub use evm2_inspectors::tracing::{
    CallTraceArena, TraceWriter, TracingInspector, TracingInspectorConfig,
};

/// ABI decoder for evm2 traces recorded by Foundry.
pub struct NativeTraceDecoder {
    functions: HashMap<Selector, Option<Function>>,
    cheatcodes: HashMap<Selector, Function>,
}

impl Default for NativeTraceDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl NativeTraceDecoder {
    /// Creates a decoder with Foundry's cheatcode signatures.
    pub fn new() -> Self {
        let cheatcodes = Vm::abi::functions()
            .into_values()
            .flatten()
            .map(|function| (function.selector(), function))
            .collect();
        Self { functions: HashMap::default(), cheatcodes }
    }

    /// Registers functions from known compiled contracts.
    pub fn with_known_contracts(mut self, contracts: &ContractsByArtifact) -> Self {
        for contract in contracts.values() {
            self.with_abi(&contract.abi);
        }
        self
    }

    /// Registers functions from an ABI.
    pub fn with_abi(&mut self, abi: &JsonAbi) {
        for function in abi.functions() {
            self.functions
                .entry(function.selector())
                .and_modify(|known| {
                    if known.as_ref().is_some_and(|known| known.signature() != function.signature())
                    {
                        *known = None;
                    }
                })
                .or_insert_with(|| Some(function.clone()));
        }
    }

    /// Decodes a display copy while keeping cheatcode inputs and outputs private.
    pub fn decode_for_display(&self, arena: &CallTraceArena) -> CallTraceArena {
        let mut display = redacted_for_display(arena);
        for node in display.nodes_mut() {
            let trace = &mut node.trace;
            let Some(selector) = trace.data.get(..4).and_then(|data| Selector::try_from(data).ok())
            else {
                continue;
            };
            let cheatcode = trace.address == CHEATCODE_ADDRESS;
            let function = if cheatcode {
                self.cheatcodes.get(&selector)
            } else {
                self.functions.get(&selector).and_then(Option::as_ref)
            };
            let Some(function) = function else { continue };
            let args = if cheatcode {
                Vec::new()
            } else {
                let Some(input) = trace.data.get(4..) else { continue };
                let Ok(decoded) = function.abi_decode_input(input) else { continue };
                decoded.iter().map(format_token).collect()
            };
            trace.decoded = Some(Box::new(DecodedCallTrace {
                label: cheatcode.then(|| "VM".to_string()),
                call_data: Some(DecodedCallData { signature: function.signature(), args }),
                return_data: None,
            }));
        }
        display
    }
}

/// Returns a display copy that does not expose cheatcode arguments or return values.
pub fn redacted_for_display(arena: &CallTraceArena) -> CallTraceArena {
    let mut display = arena.clone();
    for node in display.nodes_mut() {
        if node.trace.address == CHEATCODE_ADDRESS {
            node.trace.data =
                Bytes::copy_from_slice(&node.trace.data[..node.trace.data.len().min(4)]);
            node.trace.output = Bytes::new();
            node.trace.decoded = None;
        }
    }
    display
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_redacts_cheatcode_io_without_mutating_recording() {
        let mut arena = CallTraceArena::default();
        arena.nodes_mut()[0].trace.address = CHEATCODE_ADDRESS;
        arena.nodes_mut()[0].trace.data = Bytes::from_static(&[1, 2, 3, 4, 5, 6]);
        arena.nodes_mut()[0].trace.output = Bytes::from_static(&[7, 8]);

        let display = redacted_for_display(&arena);

        assert_eq!(display.nodes()[0].trace.data.as_ref(), &[1, 2, 3, 4]);
        assert!(display.nodes()[0].trace.output.is_empty());
        assert_eq!(arena.nodes()[0].trace.data.as_ref(), &[1, 2, 3, 4, 5, 6]);
        assert_eq!(arena.nodes()[0].trace.output.as_ref(), &[7, 8]);
    }

    #[test]
    fn native_decoder_names_calls_without_exposing_cheatcode_arguments() {
        let mut decoder = NativeTraceDecoder::new();
        decoder.with_abi(&JsonAbi::parse(["function check(uint256) external"]).unwrap());
        let mut arena = CallTraceArena::default();
        let function = decoder.functions.values().next().unwrap().as_ref().unwrap();
        arena.nodes_mut()[0].trace.data =
            [function.selector().as_slice(), &[0; 31], &[7]].concat().into();
        let display = decoder.decode_for_display(&arena);
        let decoded = display.nodes()[0].trace.decoded.as_ref().unwrap();
        assert_eq!(decoded.call_data.as_ref().unwrap().signature, "check(uint256)");
        assert_eq!(decoded.call_data.as_ref().unwrap().args, ["7"]);

        let deal = decoder.cheatcodes.values().find(|function| function.name == "deal").unwrap();
        let mut arena = CallTraceArena::default();
        arena.nodes_mut()[0].trace.address = CHEATCODE_ADDRESS;
        arena.nodes_mut()[0].trace.data = [deal.selector().as_slice(), &[0xaa; 64]].concat().into();
        arena.nodes_mut()[0].trace.output = Bytes::from_static(&[0xbb; 32]);
        let display = decoder.decode_for_display(&arena);
        let trace = &display.nodes()[0].trace;
        assert_eq!(trace.data.as_ref(), deal.selector().as_slice());
        assert!(trace.output.is_empty());
        assert_eq!(trace.decoded.as_ref().unwrap().label.as_deref(), Some("VM"));
        assert_eq!(trace.decoded.as_ref().unwrap().call_data.as_ref().unwrap().args.len(), 0);
        assert_eq!(arena.nodes()[0].trace.output.len(), 32);
    }
}
