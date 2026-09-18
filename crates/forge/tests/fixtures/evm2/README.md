# Native Forge milestone 1

Run the rebuilt migration-branch Forge with:

```sh
forge test --root crates/forge/tests/fixtures/evm2 --no-isolate --match-contract '^MilestoneTest$'
```

Seven tests exercise constructor and setup state, independent test state, nested
CALL success/revert/INVALID, CREATE/CREATE2, value transfer, STATICCALL write
rejection, and transient storage lifetime. `MilestoneFailureTest` must exit 1
with three failures (revert reason, assertion panic, INVALID).
`MilestoneUnsupportedTest` must exit 1 even though Solidity catches the unsupported
cheatcode call. The Rust CLI tests snapshot all three command outcomes.

There is no engine-selection flag or fallback in the normal Executor call/deploy
path. `RUST_LOG=foundry_evm::evm2=debug` identifies native transaction execution.
Use `--no-isolate`: isolation, cheatcodes, tracing/debugging/coverage, forks, and
fuzz/invariant inspection are subsequent migration work. This fixture does not
establish full Ethereum parity or performance.

The preserved pre-migration Forge and native Forge both passed the seven positive
cases with Solc 0.8.35/Cancun/optimizer enabled. Reported gas matched for each test.
These are gas-accounting observations, not wall-time benchmarks.
