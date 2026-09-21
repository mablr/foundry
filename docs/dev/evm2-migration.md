# Ethereum-only EVM2 preparation branch

This working branch reduces the execution surface before replacing REVM with EVM2.
It still executes Ethereum through REVM; it does not integrate EVM2 yet.

`EthEvmNetwork` is the only compiled Foundry EVM network implementation. The existing
generic executor, backend, inspector, journal, and cheatcode interfaces remain so the
engine migration can be developed separately from this preparation change.

OP, Base, and Monad feature propagation and engine dependencies are commented out.
Their feature names are retained as empty switches, so `--all-features` cannot restore
the implementations. Network-specific files remain on disk with their module declarations
disabled. Scattered integration code and dependent tests are preserved in comments marked
`EVM2 migration: disabled non-Ethereum execution`.

Tempo's unconditional factory, environment adapters, genesis helpers, label inspector,
transaction fee-token propagation, and precompile-backed cheatcodes are disabled in the
shared execution stack. Forge, local Cast execution, scripts, Chisel, and bytecode
verification reject Tempo execution explicitly. RPC transaction types, signatures, ABI
metadata, and network identification remain where they do not depend on a foreign engine.

Anvil is outside the initial migration. Its Ethereum implementation remains available as
a test dependency. Foreign feature-gated paths are disabled along with the shared crates;
Tempo startup and the execution paths dependent on removed shared adapters reject execution.
Remaining Anvil-local Tempo code is not a supported execution configuration on this branch.

The normal dependency trees of `forge` and `foundry-evm`, including all features, must not
contain `tempo-revm`, `tempo-evm`, `tempo-precompiles`, `op-revm`, `alloy-op-evm`,
`monad-revm`, `alloy-monad-evm`, or `base-common-evm`. Anvil's legacy local dependencies
remain outside that boundary. Do not interpret permissive RPC decoding as support for
executing the decoded network's transactions.

`crates/forge/tests/cli/evm2_ethereum.rs` covers Ethereum execution with snapshot and prank
cheatcodes and rejects Tempo selection. Existing Ethereum tests remain available for the
subsequent engine migration.
