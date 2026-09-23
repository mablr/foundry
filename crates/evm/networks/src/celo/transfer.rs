//! Celo transfer precompile metadata.

use alloy_primitives::{Address, address};

/// Label of the Celo transfer precompile to display in traces.
pub const CELO_TRANSFER_LABEL: &str = "CELO_TRANSFER_PRECOMPILE";

/// Address of the Celo transfer precompile.
pub const CELO_TRANSFER_ADDRESS: Address = address!("0x00000000000000000000000000000000000000fd");

/// Name of the Celo transfer precompile.
pub const CELO_TRANSFER_NAME: &str = "celo transfer";
