mod abi;
mod anvil;
mod anvil_api;
mod api;
/* EVM2 migration: disabled non-Ethereum execution.
#[cfg(feature = "base")]
mod base;
*/
mod beacon_api;
mod block_index;
mod eip2935;
mod eip4844;
mod eip6110;
mod eip7702;
mod eip7928;
mod filter;
mod fork;
mod fork_chains;
mod gas;
mod genesis;
mod ipc;
mod logs;
/* EVM2 migration: disabled non-Ethereum execution.
#[cfg(feature = "monad")]
mod monad;
*/
/* EVM2 migration: disabled non-Ethereum execution.
#[cfg(feature = "optimism")]
mod optimism;
*/
mod otterscan;
mod proof;
mod pubsub;
mod revert;
mod sign;
mod simulate;
#[cfg(feature = "cmd")]
mod state;
mod storage_values;
// mod tempo;
// mod tempo_canary;
mod traces;
mod transaction;
mod txpool;
pub mod utils;
mod wsapi;

pub use foundry_test_utils::init_tracing;
