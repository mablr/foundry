# Native Forge migration fixtures

Use the rebuilt migration-branch `forge`, Solc 0.8.35 and `--no-isolate`. The
checked-in configuration selects Cancun and enables the optimizer. There is no
runtime engine switch or fallback. `RUST_LOG=foundry_evm::evm2=debug` identifies
native execution. This suite does not establish full Ethereum parity or performance.

## Positive cases

```sh
forge test --root crates/forge/tests/fixtures/evm2 --use 0.8.35 --no-isolate --match-contract '^(MilestoneTest|NativeCheatcodesTest|NativeExpectationsTest|NativePranksTest|NativePrankSetupTest)$'
```

| Contract | Tests | Contract exercised |
| --- | ---: | --- |
| MilestoneTest | 7 | Constructor/setup, independent state, nested CALL success/revert/halt, CREATE/CREATE2, value, STATICCALL, transient lifetime. |
| NativeCheatcodesTest | 9 | Assertions, setup environment/storage/code, child rollback, malformed code and precompile-write errors. |
| NativeExpectationsTest | 11 | Revert matching/count/depth, cheatcode errors, CREATE/CREATE2, INVALID, refund preservation and console interposition. |
| NativePranksTest / NativePrankSetupTest | 10 + 1 | Sender/delegate overrides, cleanup, caller introspection, creation and setup persistence. |

All 38 positive cases have matched preserved REVM reported gas under this configuration.
These are gas-accounting observations, not wall-time benchmarks.

## Intentional failures

Run the same command with one of these contract filters; expect exit 1:

| Contract | Expected outcome |
| --- | --- |
| MilestoneFailureTest | Three failures: revert reason, assertion panic and INVALID. |
| NativeAssertionFailureTest | Assertion failure; with `FOUNDRY_ASSERTIONS_REVERT=false` and `-vv`, legacy failure plus diagnostic log. |
| NativeExpectationFailuresTest | Also pass `--match-test 'test(Mismatch|Dangling|CountMissing|SuccessInstead)'`: four matching failures. |
| MilestoneUnsupportedTest | Caught snapshot call still fails the native capability gate. |
| NativePrankOriginUnsupportedTest | Caught origin-changing prank still fails the native capability gate. |

Four expectation failures and the legacy assertion match reference reasons and gas.
Expected failures without captured interpreter-terminal gas remain unsupported.
Do not run the whole directory expecting an all-green suite.

## Reference-only obligation

`AccountMutation.t.sol` specifies descendant rollback behavior still missing from
the native port. Run with preserved REVM (local path below), not native Forge:

```sh
/Volumes/Stockage/dev-cache/forge-pre-evm2-milestone1 test --root crates/forge/tests/fixtures/evm2 --use 0.8.35 --no-isolate --match-contract '^MutationProbeTest$'
```

The child overrides balance/nonce to 99/8 and reverts; the parent still reads 99/8.
Native `deal`/nonce setters remain explicitly unsupported pending this contract.

## Rust regressions

Rebuild Forge before running the harness that selects its sibling binary:

```sh
cargo +nightly build -p forge
cargo +nightly test -p forge --test cli test_cmd::evm2
cargo +nightly test -p foundry-evm --lib executors::evm2::tests
```

Current results: 11 CLI snapshots and 3 executor unit tests pass. Their Solidity
cases overlap the counts above. Further contracts/gaps belong in the
[transition ledger](../../../../../docs/dev/evm2-migration-spec.md) and
[engine requirements](../../../../../docs/dev/evm2-upstream-requirements.md).
