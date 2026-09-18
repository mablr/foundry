# Native Forge migration fixtures

Use the rebuilt migration-branch `forge`, Solc 0.8.35 and `--no-isolate`. The
checked-in configuration selects Cancun and enables the optimizer. There is no
runtime engine switch or fallback. `RUST_LOG=foundry_evm::evm2=debug` identifies
native execution. This suite does not establish full Ethereum parity or performance.

## Positive cases

```sh
forge test --root crates/forge/tests/fixtures/evm2 --use 0.8.35 --no-isolate --match-contract '^(MilestoneTest|NativeCheatcodesTest|NativeExpectationsTest|NativePranksTest|NativePrankSetupTest|NativeCallsTest|NativeMockSetupTest|NativeStatelessTest|NativeParserTest|NativeParserDeprecatedTest|NativeCreatesTest|FailedCreateIdentityTest|NativeCreateRevertersTest|NativeEmitsTest|NativeMockFunctionsTest)$'
```

| Contract | Tests | Contract exercised |
| --- | ---: | --- |
| MilestoneTest | 7 | Constructor/setup, independent state, nested CALL success/revert/halt, CREATE/CREATE2, value, STATICCALL, transient lifetime. |
| NativeCheatcodesTest | 9 | Assertions, setup environment/storage/code, child rollback, malformed code and precompile-write errors. |
| NativeExpectationsTest | 11 | Revert matching/count/depth, cheatcode errors, CREATE/CREATE2, INVALID, refund preservation and console interposition. |
| NativeCallsTest / NativeMockSetupTest | 15 + 1 | Call expectations, mock responses/value/rollback/setup and reverted log recording. |
| NativeParserTest / NativeParserDeprecatedTest | 16 + 1 | All 62 typed JSON/TOML overloads, defaults, malformed inputs and setup deprecation. |
| NativeMockFunctionsTest | 8 | Function redirection, exact/selector priority, setup persistence, identity, static/revert behavior and mock interactions. |
| NativeEmitsTest | 8 | All event expectation overloads, immediate rollback, caught end mismatch and reverted-child logs. |
| NativeCreateRevertersTest | 2 | Innermost constructor reverter and repeated CREATE2 revert matching. |
| NativeCreatesTest / FailedCreateIdentityTest | 5 + 1 | Creation matching, nested/duplicate/pranked deployments, enclosing rollback and constructor-revert identity. |
| NativeStatelessTest | 6 | String boundaries, base64 alphabets, CREATE addresses, RLP/crypto/version errors and unknown-selector diagnostics. |
| NativePranksTest / NativePrankSetupTest | 10 + 1 | Sender/delegate overrides, cleanup, caller introspection, creation and setup persistence. |

All 101 positive cases have matched preserved REVM reported gas under this configuration.
These are gas-accounting observations, not wall-time benchmarks.

## Intentional failures

Run the same command with one of these contract filters; expect exit 1:

| Contract | Expected outcome |
| --- | --- |
| MilestoneFailureTest | Three failures: revert reason, assertion panic and INVALID. |
| NativeAssertionFailureTest | Assertion failure; with `FOUNDRY_ASSERTIONS_REVERT=false` and `-vv`, legacy failure plus diagnostic log. |
| NativeExpectationFailuresTest | Also pass `--match-test 'test(Mismatch|Dangling|CountMissing|SuccessInstead)'`: four matching failures. |
| NativeCreateReverterFailureTest | With `--match-test testRejectsDifferentSecondReverter`: second constructor must fail with reverter mismatch. |
| NativeEmitsFailureTest | Filter `--match-test 'test(MissingEmit|WrongData|WrongEmitter|ZeroCountViolation|AnonymousTemplateRejected|OriginalFailure)'`: six failures with matching gas/diagnostics. |
| NativeCreatesFailureTest | Four failures: missing creation, wrong scheme, collision and preserved original failure. |
| NativeCallsFailuresTest | Three failures: missing call, excess call count and preserved original revert. |
| DelegatedCallIdentityTest | Use `--evm-version prague`: REVM passes; native fails the E2-09 guard. |
| MilestoneUnsupportedTest | Caught snapshot call still fails the native capability gate. |
| NativePrankOriginUnsupportedTest | Caught origin-changing prank still fails the native capability gate. |

Eighteen expectation failures and the legacy assertion match reference reasons and gas.
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
cargo +nightly test -p foundry-cheatcodes --lib native::tests
```

Current results: 25 CLI snapshots, 3 executor unit tests and the mock-overflow guard test pass. Their Solidity
cases overlap the counts above. Further contracts/gaps belong in the
[transition ledger](../../../../../docs/dev/evm2-migration-spec.md) and
[engine requirements](../../../../../docs/dev/evm2-upstream-requirements.md).

## Unchanged stateless helper fixtures

The following isolated selection passes 41 deterministic tests on both preserved
REVM and native evm2, with matching reported gas. Fuzz cases remain excluded by name
because the native fuzz inspector is an M4 gate. Sources and assertions are unchanged.

```sh
python3 - <<'PYFIX'
from pathlib import Path
import shutil
root = Path('/tmp/evm2-m2-stateless-original')
(root / 'test').mkdir(parents=True, exist_ok=True)
shutil.copytree('testdata/utils', root / 'utils', dirs_exist_ok=True)
for name in ['ToString', 'Base64', 'StringUtils', 'Parse', 'Ed25519', 'SignP256', 'Rlp', 'Derive']:
    shutil.copyfile(f'testdata/default/cheats/{name}.t.sol', root / 'test' / f'{name}.t.sol')
(root / 'foundry.toml').write_text('[profile.default]\nsrc="test"\ntest="test"\nevm_version="cancun"\noptimizer=true\nremappings=["utils/=utils/"]\n')
PYFIX
# Run once with each binary: the newly built native Forge and preserved REVM.
<forge> test --root /tmp/evm2-m2-stateless-original --use 0.8.35 --no-isolate --no-match-test Fuzz
```

This selection does not certify all 52 newly routed stateless overloads or close
any whole M2 capability family.

## Existing constructor-reverter regressions

Copy unchanged `testdata/default/cheats/ExpectRevert.t.sol` and `testdata/utils/`
into an isolated project configured identically to the helper project above.
Run both binaries with `--no-isolate --use 0.8.35 --match-test 'Reverter.*Create'`.
Seven tests execute and match reported gas: top-level/nested CREATE, partial/bytes4
reason matching, CREATE2 and counted top-level/nested CREATE2. The checked-in
`NativeCreateReverterFailureTest` additionally prevents stale reverter identity
from accepting a different second constructor. These seven existing tests are
separate from the 85-case native fixture total.

## Existing event-expectation regressions

Copy unchanged `testdata/default/cheats/ExpectEmit.t.sol` and `testdata/utils/` into
an isolated project configured like the helper project above. Run both binaries
with `--no-isolate --use 0.8.35 --no-match-test '^testExpectEmit(Nested)?\('`.
This executes 22 deterministic tests with matching reported gas and explicitly
excludes two fuzz tests. These cases are separate from the native fixture total.
