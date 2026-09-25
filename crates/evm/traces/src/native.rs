//! Trace types and display safeguards for evm2 execution.

use alloy_dyn_abi::{EventExt, JsonAbiExt};
use alloy_json_abi::{Event, Function, JsonAbi};
use alloy_primitives::{B256, Bytes, Selector};
use evm2_inspectors::tracing::types::{
    DecodedCallData, DecodedCallLog, DecodedCallTrace, TraceMemberOrder,
};
use foundry_common::{ContractsByArtifact, contracts::ContractData, fmt::format_token};
use foundry_evm_core::{abi::Vm, constants::CHEATCODE_ADDRESS};
use std::collections::HashMap;

pub use evm2_inspectors::tracing::{
    CallTraceArena, TraceWriter, TracingInspector, TracingInspectorConfig,
};

/// Returns a trace arena containing only nodes visible at `depth`.
pub fn trace_arena_at_depth(arena: &CallTraceArena, depth: usize) -> CallTraceArena {
    let mut arena = arena.clone();
    let nodes = arena.nodes_mut();
    let mut reachable = vec![false; nodes.len()];
    let mut pending = vec![0];
    while let Some(node_idx) = pending.pop() {
        if reachable[node_idx] {
            continue;
        }
        reachable[node_idx] = true;
        let node = &nodes[node_idx];
        if node.trace.depth < depth {
            pending.extend(node.ordering.iter().filter_map(|item| match item {
                TraceMemberOrder::Call(child_idx) => Some(node.children[*child_idx]),
                _ => None,
            }));
        }
    }

    let mut remapped = vec![None; nodes.len()];
    for (next_idx, node) in nodes.iter_mut().filter(|node| reachable[node.idx]).enumerate() {
        remapped[node.idx] = Some(next_idx);
        if node.trace.depth >= depth {
            node.ordering.clear();
            node.children.clear();
        } else {
            let mut child_positions = vec![None; node.children.len()];
            let mut children = Vec::with_capacity(node.children.len());
            for (old_position, child) in node.children.iter().copied().enumerate() {
                if reachable[child] {
                    child_positions[old_position] = Some(children.len());
                    children.push(child);
                }
            }
            node.children = children;
            node.ordering = std::mem::take(&mut node.ordering)
                .into_iter()
                .filter_map(|item| match item {
                    TraceMemberOrder::Call(child_idx) => {
                        Some(TraceMemberOrder::Call(child_positions[child_idx]?))
                    }
                    item => Some(item),
                })
                .collect();
        }
    }

    nodes.retain(|node| reachable[node.idx]);
    for node in nodes {
        node.idx = remapped[node.idx].expect("retained trace node has a remapped index");
        node.parent = node.parent.and_then(|parent| remapped[parent]);
        for child in &mut node.children {
            *child = remapped[*child].expect("retained trace child has a remapped index");
        }
    }
    arena
}

/// ABI decoder for evm2 traces recorded by Foundry.
pub struct NativeTraceDecoder {
    contracts: ContractsByArtifact,
    functions: HashMap<Selector, Option<Function>>,
    events: HashMap<B256, Vec<Event>>,
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
        Self {
            contracts: ContractsByArtifact::default(),
            functions: HashMap::default(),
            events: HashMap::default(),
            cheatcodes,
        }
    }

    /// Registers functions from known compiled contracts.
    pub fn with_known_contracts(mut self, contracts: &ContractsByArtifact) -> Self {
        for contract in contracts.values() {
            self.with_abi(&contract.abi);
        }
        self.contracts = contracts.clone();
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
        for event in abi.events() {
            self.events.entry(event.selector()).or_default().push(event.clone());
        }
    }

    /// Decodes a display copy while keeping cheatcode inputs and outputs private.
    pub fn decode_for_display(&self, arena: &CallTraceArena) -> CallTraceArena {
        let mut display = redacted_for_display(arena);
        for node in display.nodes_mut() {
            let trace = &mut node.trace;
            let contract = if trace.kind.is_any_create() {
                self.contracts.find_by_creation_code(&trace.data).map(|(_, contract)| contract)
            } else {
                trace.bytecode.as_deref().and_then(|code| {
                    self.contracts.find_by_deployed_code(code).map(|(_, contract)| contract)
                })
            };
            self.decode_call(trace, contract);
            for log in &mut node.logs {
                let event = contract
                    .and_then(|contract| decode_event(log, contract.abi.events()))
                    .or_else(|| {
                        log.raw_log
                            .topics()
                            .first()
                            .and_then(|topic| self.events.get(topic))
                            .and_then(|events| decode_event(log, events.iter()))
                    });
                log.decoded = event.map(Box::new);
            }
        }
        display
    }

    fn decode_call(
        &self,
        trace: &mut evm2_inspectors::tracing::types::CallTrace,
        contract: Option<&ContractData>,
    ) {
        if trace.kind.is_any_create() {
            if let Some(contract) = contract {
                trace.decoded().label = Some(contract.name.clone());
            }
            return;
        }
        let Some(selector) = trace.data.get(..4).and_then(|data| Selector::try_from(data).ok())
        else {
            return;
        };
        let cheatcode = trace.address == CHEATCODE_ADDRESS;
        let function = if cheatcode {
            self.cheatcodes.get(&selector)
        } else {
            contract
                .and_then(|contract| {
                    contract.abi.functions().find(|function| function.selector() == selector)
                })
                .or_else(|| self.functions.get(&selector).and_then(Option::as_ref))
        };
        let Some(function) = function else { return };
        let args = if cheatcode {
            Vec::new()
        } else {
            let Some(input) = trace.data.get(4..) else { return };
            let Ok(decoded) = function.abi_decode_input(input) else { return };
            decoded.iter().map(format_token).collect()
        };
        trace.decoded = Some(Box::new(DecodedCallTrace {
            label: if cheatcode {
                Some("VM".to_string())
            } else {
                contract.map(|contract| contract.name.clone())
            },
            call_data: Some(DecodedCallData { signature: function.signature(), args }),
            return_data: None,
        }));
    }
}

fn decode_event<'a>(
    log: &evm2_inspectors::tracing::types::CallLog,
    events: impl Iterator<Item = &'a Event>,
) -> Option<DecodedCallLog> {
    let topic = log.raw_log.topics().first()?;
    let mut decoded = events
        .filter(|event| {
            event.selector() == *topic
                && event.inputs.iter().filter(|input| input.indexed).count() + 1
                    == log.raw_log.topics().len()
        })
        .filter_map(|event| {
            let values = event.decode_log(&log.raw_log).ok()?;
            let mut indexed = values.indexed.iter();
            let mut body = values.body.iter();
            let params = event
                .inputs
                .iter()
                .map(|input| {
                    let value = if input.indexed { indexed.next() } else { body.next() }?;
                    Some((input.name.clone(), format_token(value)))
                })
                .collect::<Option<Vec<_>>>()?;
            Some(DecodedCallLog { name: Some(event.name.clone()), params: Some(params) })
        });
    let first = decoded.next()?;
    decoded.all(|next| next == first).then_some(first)
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
