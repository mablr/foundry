//! Trace types and display safeguards for evm2 execution.

use alloy_primitives::Bytes;
use foundry_evm_core::constants::CHEATCODE_ADDRESS;

pub use evm2_inspectors::tracing::{
    CallTraceArena, TraceWriter, TracingInspector, TracingInspectorConfig,
};

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
}
