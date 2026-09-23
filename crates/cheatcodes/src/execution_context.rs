//! Process-wide Forge execution context.

use crate::ForgeContext;
use std::sync::OnceLock;

static FORGE_CONTEXT: OnceLock<ForgeContext> = OnceLock::new();

/// Returns the active Forge execution context.
pub fn current_execution_context() -> Option<ForgeContext> {
    FORGE_CONTEXT.get().copied()
}

/// Initializes the Forge execution context once for the process.
pub fn set_execution_context(context: ForgeContext) {
    let _ = FORGE_CONTEXT.set(context);
}
