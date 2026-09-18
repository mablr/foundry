//! End-to-end acceptance for the first native evm2 execution milestone.

use foundry_test_utils::str;

const FIXTURE: &str = include_str!("../../fixtures/evm2/Milestone.t.sol");

forgetest!(evm2_native_lifecycle, |prj, cmd| {
    prj.add_test("Milestone.t.sol", FIXTURE);
    cmd.args([
        "test",
        "--no-isolate",
        "--evm-version",
        "cancun",
        "--match-contract",
        "^MilestoneTest$",
    ])
    .assert_success()
    .stdout_eq(str![[r#"
...
Ran 7 tests for test/Milestone.t.sol:MilestoneTest
[PASS] testIndependentSetup() ([GAS])
[PASS] testNestedCreateAndCreate2() ([GAS])
[PASS] testNestedHalt() ([GAS])
[PASS] testNestedRevert() ([GAS])
[PASS] testSetupAndStorageOnlyCommit() ([GAS])
[PASS] testStaticCallRejectsWrite() ([GAS])
[PASS] testTransientResetBetweenTransactions() ([GAS])
Suite result: ok. 7 passed; 0 failed; 0 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 7 tests passed, 0 failed, 0 skipped (7 total tests)

"#]]);
});

forgetest!(evm2_native_failure_reporting, |prj, cmd| {
    prj.add_test("Milestone.t.sol", FIXTURE);
    cmd.args([
        "test",
        "--no-isolate",
        "--evm-version",
        "cancun",
        "--match-contract",
        "^MilestoneFailureTest$",
    ])
    .assert_failure()
    .stdout_eq(str![[r#"
...
Ran 3 tests for test/Milestone.t.sol:MilestoneFailureTest
[FAIL: EvmError: InvalidFEOpcode] testHalt() ([GAS])
[FAIL: panic: assertion failed (0x01)] testPanic() ([GAS])
[FAIL: milestone failure] testRevert() ([GAS])
Suite result: FAILED. 0 passed; 3 failed; 0 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 0 tests passed, 3 failed, 0 skipped (3 total tests)

Failing tests:
Encountered 3 failing tests in test/Milestone.t.sol:MilestoneFailureTest
[FAIL: EvmError: InvalidFEOpcode] testHalt() ([GAS])
[FAIL: panic: assertion failed (0x01)] testPanic() ([GAS])
[FAIL: milestone failure] testRevert() ([GAS])

Encountered a total of 3 failing tests, 0 tests succeeded

Tip: Run `forge test --rerun` to retry only the 3 failed tests
Tip: Run `forge test --debug --match-test <TEST_NAME>` to inspect one failing test in the debugger

"#]]);
});

forgetest!(evm2_native_rejects_caught_cheatcodes, |prj, cmd| {
    prj.add_test("Milestone.t.sol", FIXTURE);
    cmd.args(["test", "--no-isolate", "--evm-version", "cancun", "--match-contract", "^MilestoneUnsupportedTest$"])
        .assert_failure()
        .stdout_eq(str![[r#"
...
Ran 1 test for test/Milestone.t.sol:MilestoneUnsupportedTest
[FAIL: evm2 cheatcode/console call is not migrated (target 0x7109709ECfa91a80626fF3989D68f67F5b1DD12D, input 0x9cd23835)] testCaughtCheatcodeStillFails() ([GAS])
Suite result: FAILED. 0 passed; 1 failed; 0 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 0 tests passed, 1 failed, 0 skipped (1 total tests)

Failing tests:
Encountered 1 failing test in test/Milestone.t.sol:MilestoneUnsupportedTest
[FAIL: evm2 cheatcode/console call is not migrated (target 0x7109709ECfa91a80626fF3989D68f67F5b1DD12D, input 0x9cd23835)] testCaughtCheatcodeStillFails() ([GAS])

Encountered a total of 1 failing tests, 0 tests succeeded

Tip: Run `forge test --rerun` to retry only the 1 failed test
Tip: Run `forge test --debug --match-test <TEST_NAME>` to inspect one failing test in the debugger

"#]]);
});

forgetest!(evm2_native_cheatcode_transitions, |prj, cmd| {
    prj.add_test("Cheatcodes.t.sol", include_str!("../../fixtures/evm2/Cheatcodes.t.sol"));
    cmd.args([
        "test",
        "--no-isolate",
        "--evm-version",
        "cancun",
        "--match-contract",
        "^NativeCheatcodesTest$",
    ])
    .assert_success()
    .stdout_eq(str![[r#"
...
Ran 9 tests for test/Cheatcodes.t.sol:NativeCheatcodesTest
[PASS] testAssertionOverloads() ([GAS])
[PASS] testAssertionRevertPayload() ([GAS])
[PASS] testEnclosingCodeRevert() ([GAS])
[PASS] testEnclosingRevert() ([GAS])
[PASS] testIndependentSetup() ([GAS])
[PASS] testMalformedCodeRejected() ([GAS])
[PASS] testPrecompileWriteRejected() ([GAS])
[PASS] testSetupPersistence() ([GAS])
[PASS] testStorageAndEnvironment() ([GAS])
Suite result: ok. 9 passed; 0 failed; 0 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 9 tests passed, 0 failed, 0 skipped (9 total tests)

"#]]);
});

forgetest!(evm2_native_assertion_failure, |prj, cmd| {
    prj.add_test("Cheatcodes.t.sol", include_str!("../../fixtures/evm2/Cheatcodes.t.sol"));
    cmd.args([
        "test",
        "--no-isolate",
        "--evm-version",
        "cancun",
        "--match-contract",
        "^NativeAssertionFailureTest$",
    ])
    .assert_failure()
    .stdout_eq(str![[r#"
...
Ran 1 test for test/Cheatcodes.t.sol:NativeAssertionFailureTest
[FAIL: assertion failed: 1 != 2] testAssertionFailure() ([GAS])
Suite result: FAILED. 0 passed; 1 failed; 0 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 0 tests passed, 1 failed, 0 skipped (1 total tests)

Failing tests:
Encountered 1 failing test in test/Cheatcodes.t.sol:NativeAssertionFailureTest
[FAIL: assertion failed: 1 != 2] testAssertionFailure() ([GAS])

Encountered a total of 1 failing tests, 0 tests succeeded

Tip: Run `forge test --rerun` to retry only the 1 failed test
Tip: Run `forge test --debug --match-test <TEST_NAME>` to inspect one failing test in the debugger

"#]]);
});

forgetest!(evm2_native_expectations, |prj, cmd| {
    prj.add_test("Expectations.t.sol", include_str!("../../fixtures/evm2/Expectations.t.sol"));
    cmd.args([
        "test",
        "--no-isolate",
        "--evm-version",
        "cancun",
        "--match-contract",
        "^NativeExpectationsTest$",
    ])
    .assert_success()
    .stdout_eq(str![[r#"
...
Ran 11 tests for test/Expectations.t.sol:NativeExpectationsTest
[PASS] testAddress() ([GAS])
[PASS] testAnyRevert() ([GAS])
[PASS] testCheatcode() ([GAS])
[PASS] testConsoleDoesNotConsume() ([GAS])
[PASS] testConstructor() ([GAS])
[PASS] testConstructor2() ([GAS])
[PASS] testCount() ([GAS])
[PASS] testHalt() ([GAS])
[PASS] testPartial() ([GAS])
[PASS] testRevertRefund() ([GAS])
[PASS] testStringAndRollback() ([GAS])
Suite result: ok. 11 passed; 0 failed; 0 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 11 tests passed, 0 failed, 0 skipped (11 total tests)

"#]]);
});

forgetest!(evm2_native_expectation_failures, |prj, cmd| {
    prj.add_test("Expectations.t.sol", include_str!("../../fixtures/evm2/Expectations.t.sol"));
    cmd.args([
        "test",
        "--no-isolate",
        "--evm-version",
        "cancun",
        "--match-contract",
        "^NativeExpectationFailuresTest$",
        "--match-test",
        "test(Mismatch|Dangling|CountMissing|SuccessInstead)",
    ])
    .assert_failure()
    .stdout_eq(str![[r#"
...
Ran 4 tests for test/Expectations.t.sol:NativeExpectationFailuresTest
[FAIL: next call did not revert as expected] testCountMissing() ([GAS])
[FAIL: call didn't revert at a lower depth than cheatcode call depth] testDangling() ([GAS])
[FAIL: Error != expected error: expected != wrong] testMismatch() ([GAS])
[FAIL: next call did not revert as expected] testSuccessInstead() ([GAS])
Suite result: FAILED. 0 passed; 4 failed; 0 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 0 tests passed, 4 failed, 0 skipped (4 total tests)

Failing tests:
Encountered 4 failing tests in test/Expectations.t.sol:NativeExpectationFailuresTest
[FAIL: next call did not revert as expected] testCountMissing() ([GAS])
[FAIL: call didn't revert at a lower depth than cheatcode call depth] testDangling() ([GAS])
[FAIL: Error != expected error: expected != wrong] testMismatch() ([GAS])
[FAIL: next call did not revert as expected] testSuccessInstead() ([GAS])

Encountered a total of 4 failing tests, 0 tests succeeded

Tip: Run `forge test --rerun` to retry only the 4 failed tests
Tip: Run `forge test --debug --match-test <TEST_NAME>` to inspect one failing test in the debugger

"#]]);
});

forgetest!(evm2_native_legacy_assertion, |prj, cmd| {
    prj.add_test("Cheatcodes.t.sol", include_str!("../../fixtures/evm2/Cheatcodes.t.sol"));
    cmd.env("FOUNDRY_ASSERTIONS_REVERT", "false");
    cmd.args([
        "test",
        "--no-isolate",
        "--evm-version",
        "cancun",
        "--match-contract",
        "^NativeAssertionFailureTest$",
        "-vv",
    ])
    .assert_failure()
    .stdout_eq(str![[r#"
...
Ran 1 test for test/Cheatcodes.t.sol:NativeAssertionFailureTest
[FAIL] testAssertionFailure() ([GAS])
Logs:
  assertion failed: 1 != 2

Suite result: FAILED. 0 passed; 1 failed; 0 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 0 tests passed, 1 failed, 0 skipped (1 total tests)

Failing tests:
Encountered 1 failing test in test/Cheatcodes.t.sol:NativeAssertionFailureTest
[FAIL] testAssertionFailure() ([GAS])

Encountered a total of 1 failing tests, 0 tests succeeded

Tip: Run `forge test --rerun` to retry only the 1 failed test
Tip: Run `forge test --debug --match-test <TEST_NAME>` to inspect one failing test in the debugger

"#]]);
});

forgetest!(evm2_native_pranks, |prj, cmd| {
    prj.add_test("Pranks.t.sol", include_str!("../../fixtures/evm2/Pranks.t.sol"));
    cmd.args([
        "test",
        "--no-isolate",
        "--evm-version",
        "cancun",
        "--match-contract",
        "^NativePranksTest$",
    ])
    .assert_success()
    .stdout_eq(str![[r#"
...
Ran 10 tests for test/Pranks.t.sol:NativePranksTest
[PASS] testCannotOverwriteUnused() ([GAS])
[PASS] testCreate() ([GAS])
[PASS] testCreate2() ([GAS])
[PASS] testDelegate() ([GAS])
[PASS] testDelegateFromEOARejected() ([GAS])
[PASS] testNested() ([GAS])
[PASS] testOneShot() ([GAS])
[PASS] testPersistent() ([GAS])
[PASS] testReadCallers() ([GAS])
[PASS] testRevertCleanup() ([GAS])
Suite result: ok. 10 passed; 0 failed; 0 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 10 tests passed, 0 failed, 0 skipped (10 total tests)

"#]]);
});

forgetest!(evm2_native_setup_prank, |prj, cmd| {
    prj.add_test("Pranks.t.sol", include_str!("../../fixtures/evm2/Pranks.t.sol"));
    cmd.args([
        "test",
        "--no-isolate",
        "--evm-version",
        "cancun",
        "--match-contract",
        "^NativePrankSetupTest$",
    ])
    .assert_success()
    .stdout_eq(str![[r#"
...
Ran 1 test for test/Pranks.t.sol:NativePrankSetupTest
[PASS] testSetupPrankPersists() ([GAS])
Suite result: ok. 1 passed; 0 failed; 0 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 1 tests passed, 0 failed, 0 skipped (1 total tests)

"#]]);
});

forgetest!(evm2_native_rejects_origin_prank, |prj, cmd| {
    prj.add_test("Pranks.t.sol", include_str!("../../fixtures/evm2/Pranks.t.sol"));
    cmd.args(["test", "--no-isolate", "--evm-version", "cancun", "--match-contract", "^NativePrankOriginUnsupportedTest$"])
        .assert_failure().stdout_eq(str![[r#"
...
Ran 1 test for test/Pranks.t.sol:NativePrankOriginUnsupportedTest
[FAIL: evm2 cheatcode/console call is not migrated (target 0x7109709ECfa91a80626fF3989D68f67F5b1DD12D, input 0x47e50cce00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000002)] testCaughtOriginOverrideFails() ([GAS])
Suite result: FAILED. 0 passed; 1 failed; 0 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 0 tests passed, 1 failed, 0 skipped (1 total tests)

Failing tests:
Encountered 1 failing test in test/Pranks.t.sol:NativePrankOriginUnsupportedTest
[FAIL: evm2 cheatcode/console call is not migrated (target 0x7109709ECfa91a80626fF3989D68f67F5b1DD12D, input 0x47e50cce00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000002)] testCaughtOriginOverrideFails() ([GAS])

Encountered a total of 1 failing tests, 0 tests succeeded

Tip: Run `forge test --rerun` to retry only the 1 failed test
Tip: Run `forge test --debug --match-test <TEST_NAME>` to inspect one failing test in the debugger

"#]]);
});

forgetest!(evm2_native_calls, |prj, cmd| {
    prj.add_test("Calls.t.sol", include_str!("../../fixtures/evm2/Calls.t.sol"));
    cmd.args([
        "test",
        "--no-isolate",
        "--evm-version",
        "cancun",
        "--match-contract",
        "^NativeCallsTest$",
    ])
    .assert_success()
    .stdout_eq(str![[r#"
...
Ran 15 tests for test/Calls.t.sol:NativeCallsTest
[PASS] testCallPrefix() ([GAS])
[PASS] testCount() ([GAS])
[PASS] testCountedOverwriteRejected() ([GAS])
[PASS] testDelegate() ([GAS])
[PASS] testExactGas() ([GAS])
[PASS] testInsufficientFundsRetainsQueue() ([GAS])
[PASS] testMinGas() ([GAS])
[PASS] testMockEmptyAccount() ([GAS])
[PASS] testMockPriorityAndClear() ([GAS])
[PASS] testMockQueue() ([GAS])
[PASS] testMockRegistrationSurvivesRevert() ([GAS])
[PASS] testMockRevertRollback() ([GAS])
[PASS] testMockValue() ([GAS])
[PASS] testRecordedRevertedLogs() ([GAS])
[PASS] testValueStipend() ([GAS])
Suite result: ok. 15 passed; 0 failed; 0 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 15 tests passed, 0 failed, 0 skipped (15 total tests)

"#]]);
});

forgetest!(evm2_native_setup_mock, |prj, cmd| {
    prj.add_test("Calls.t.sol", include_str!("../../fixtures/evm2/Calls.t.sol"));
    cmd.args([
        "test",
        "--no-isolate",
        "--evm-version",
        "cancun",
        "--match-contract",
        "^NativeMockSetupTest$",
    ])
    .assert_success()
    .stdout_eq(str![[r#"
...
Ran 1 test for test/Calls.t.sol:NativeMockSetupTest
[PASS] testSetupMockPersists() ([GAS])
Suite result: ok. 1 passed; 0 failed; 0 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 1 tests passed, 0 failed, 0 skipped (1 total tests)

"#]]);
});

forgetest!(evm2_native_call_failures, |prj, cmd| {
    prj.add_test("Calls.t.sol", include_str!("../../fixtures/evm2/Calls.t.sol"));
    cmd.args(["test", "--no-isolate", "--evm-version", "cancun", "--match-contract", "^NativeCallsFailuresTest$"])
        .assert_failure().stdout_eq(str![[r#"
...
Ran 3 tests for test/Calls.t.sol:NativeCallsFailuresTest
[FAIL: expected call to 0x0000000000000000000000000000000000123456 with data 0x12345678 to be called 1 time, but was called 0 times] testMissingCall() ([GAS])
[FAIL: original] testPreservesOriginalRevert() ([GAS])
[FAIL: expected call to 0x0000000000000000000000000000000000123456 with data 0x12345678 to be called 1 time, but was called 2 times] testTooManyCalls() ([GAS])
Suite result: FAILED. 0 passed; 3 failed; 0 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 0 tests passed, 3 failed, 0 skipped (3 total tests)

Failing tests:
Encountered 3 failing tests in test/Calls.t.sol:NativeCallsFailuresTest
[FAIL: expected call to 0x0000000000000000000000000000000000123456 with data 0x12345678 to be called 1 time, but was called 0 times] testMissingCall() ([GAS])
[FAIL: original] testPreservesOriginalRevert() ([GAS])
[FAIL: expected call to 0x0000000000000000000000000000000000123456 with data 0x12345678 to be called 1 time, but was called 2 times] testTooManyCalls() ([GAS])

Encountered a total of 3 failing tests, 0 tests succeeded

Tip: Run `forge test --rerun` to retry only the 3 failed tests
Tip: Run `forge test --debug --match-test <TEST_NAME>` to inspect one failing test in the debugger

"#]]);
});

forgetest!(evm2_native_delegated_identity_guard, |prj, cmd| {
    prj.add_test("Calls.t.sol", include_str!("../../fixtures/evm2/Calls.t.sol"));
    cmd.args(["test", "--no-isolate", "--evm-version", "prague", "--match-contract", "^DelegatedCallIdentityTest$"])
        .assert_failure().stdout_eq(str![[r#"
...
Ran 1 test for test/Calls.t.sol:DelegatedCallIdentityTest
[FAIL: evm2 delegated call identity for expectations/mocks is not migrated] testDelegatedOperandIdentity() ([GAS])
Suite result: FAILED. 0 passed; 1 failed; 0 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 0 tests passed, 1 failed, 0 skipped (1 total tests)

Failing tests:
Encountered 1 failing test in test/Calls.t.sol:DelegatedCallIdentityTest
[FAIL: evm2 delegated call identity for expectations/mocks is not migrated] testDelegatedOperandIdentity() ([GAS])

Encountered a total of 1 failing tests, 0 tests succeeded

Tip: Run `forge test --rerun` to retry only the 1 failed test
Tip: Run `forge test --debug --match-test <TEST_NAME>` to inspect one failing test in the debugger

"#]]);
});

forgetest!(evm2_native_stateless, |prj, cmd| {
    prj.add_test("Stateless.t.sol", include_str!("../../fixtures/evm2/Stateless.t.sol"));
    cmd.args([
        "test",
        "--no-isolate",
        "--evm-version",
        "cancun",
        "--match-contract",
        "^NativeStatelessTest$",
    ])
    .assert_success()
    .stdout_eq(str![[r#"
...
Ran 6 tests for test/Stateless.t.sol:NativeStatelessTest
[PASS] testBase64Alphabets() ([GAS])
[PASS] testCreateAddressesAndOverflow() ([GAS])
[PASS] testCryptoAndVersionErrors() ([GAS])
[PASS] testRlpRoundTripAndError() ([GAS])
[PASS] testStringBoundaryAndError() ([GAS])
[PASS] testUnknownSelectorDiagnostic() ([GAS])
Suite result: ok. 6 passed; 0 failed; 0 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 6 tests passed, 0 failed, 0 skipped (6 total tests)

"#]]);
});

forgetest!(evm2_native_parsers, |prj, cmd| {
    prj.add_test("Parsers.t.sol", include_str!("../../fixtures/evm2/Parsers.t.sol"));
    cmd.args([
        "test",
        "--no-isolate",
        "--evm-version",
        "cancun",
        "--match-contract",
        "^NativeParserTest$",
    ])
    .assert_success()
    .stdout_eq(str![[r#"
...
Ran 16 tests for test/Parsers.t.sol:NativeParserTest
[PASS] testJsonAddress() ([GAS])
[PASS] testJsonBool() ([GAS])
[PASS] testJsonBytes() ([GAS])
[PASS] testJsonBytes32() ([GAS])
[PASS] testJsonInt() ([GAS])
[PASS] testJsonString() ([GAS])
[PASS] testJsonUint() ([GAS])
[PASS] testKeysAndLength() ([GAS])
[PASS] testMalformedAndMissing() ([GAS])
[PASS] testTomlAddress() ([GAS])
[PASS] testTomlBool() ([GAS])
[PASS] testTomlBytes() ([GAS])
[PASS] testTomlBytes32() ([GAS])
[PASS] testTomlInt() ([GAS])
[PASS] testTomlString() ([GAS])
[PASS] testTomlUint() ([GAS])
Suite result: ok. 16 passed; 0 failed; 0 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 16 tests passed, 0 failed, 0 skipped (16 total tests)

"#]]);
});

forgetest!(evm2_native_setup_deprecation, |prj, cmd| {
    prj.add_test("Parsers.t.sol", include_str!("../../fixtures/evm2/Parsers.t.sol"));
    cmd.args([
        "test",
        "--no-isolate",
        "--evm-version",
        "cancun",
        "--match-contract",
        "^NativeParserDeprecatedTest$",
    ])
    .assert_success()
    .stdout_eq(str![[r#"
...
Ran 1 test for test/Parsers.t.sol:NativeParserDeprecatedTest
[PASS] testDeprecatedSetupRetained() ([GAS])
Suite result: ok. 1 passed; 0 failed; 0 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 1 tests passed, 0 failed, 0 skipped (1 total tests)

"#]])
    .stderr_eq(str![[r#"
Warning: the following cheatcode(s) are deprecated and will be removed in future versions:
  keyExists(string,string): replaced by `keyExistsJson`

"#]]);
});

forgetest!(evm2_native_creates, |prj, cmd| {
    prj.add_test("Creates.t.sol", include_str!("../../fixtures/evm2/Creates.t.sol"));
    cmd.args([
        "test",
        "--no-isolate",
        "--evm-version",
        "cancun",
        "--match-contract",
        "^NativeCreatesTest$",
    ])
    .assert_success()
    .stdout_eq(str![[r#"
...
Ran 5 tests for test/Creates.t.sol:NativeCreatesTest
[PASS] testCreateAndCreate2() ([GAS])
[PASS] testDuplicateExpectations() ([GAS])
[PASS] testMatchSurvivesEnclosingRollback() ([GAS])
[PASS] testNestedUnordered() ([GAS])
[PASS] testPrankedDeployer() ([GAS])
Suite result: ok. 5 passed; 0 failed; 0 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 5 tests passed, 0 failed, 0 skipped (5 total tests)

"#]]);
});

forgetest!(evm2_native_constructor_identity, |prj, cmd| {
    prj.add_test("Creates.t.sol", include_str!("../../fixtures/evm2/Creates.t.sol"));
    cmd.args([
        "test",
        "--no-isolate",
        "--evm-version",
        "cancun",
        "--match-contract",
        "^FailedCreateIdentityTest$",
    ])
    .assert_success()
    .stdout_eq(str![[r#"
...
Ran 1 test for test/Creates.t.sol:FailedCreateIdentityTest
[PASS] testConstructorRevertEmptyCode() ([GAS])
Suite result: ok. 1 passed; 0 failed; 0 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 1 tests passed, 0 failed, 0 skipped (1 total tests)

"#]]);
});

forgetest!(evm2_native_creation_failures, |prj, cmd| {
    prj.add_test("Creates.t.sol", include_str!("../../fixtures/evm2/Creates.t.sol"));
    cmd.args(["test", "--no-isolate", "--evm-version", "cancun", "--match-contract", "^NativeCreatesFailureTest$"])
        .assert_failure()
        .stdout_eq(str![[r#"
...
Ran 4 tests for test/Creates.t.sol:NativeCreatesFailureTest
[FAIL: expected CREATE2 call by address 0x7fa9385be102ac3eac297483dd6233d62b3e1496 for bytecode 0x[..] but not found] testCollisionDoesNotMatch() ([GAS])
[FAIL: expected CREATE call by address 0x7fa9385be102ac3eac297483dd6233d62b3e1496 for bytecode 0x00 but not found] testMissing() ([GAS])
[FAIL: original] testOriginalFailure() ([GAS])
[FAIL: expected CREATE2 call by address 0x7fa9385be102ac3eac297483dd6233d62b3e1496 for bytecode 0x00 but not found] testWrongScheme() ([GAS])
Suite result: FAILED. 0 passed; 4 failed; 0 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 0 tests passed, 4 failed, 0 skipped (4 total tests)

Failing tests:
Encountered 4 failing tests in test/Creates.t.sol:NativeCreatesFailureTest
[FAIL: expected CREATE2 call by address 0x7fa9385be102ac3eac297483dd6233d62b3e1496 for bytecode 0x[..] but not found] testCollisionDoesNotMatch() ([GAS])
[FAIL: expected CREATE call by address 0x7fa9385be102ac3eac297483dd6233d62b3e1496 for bytecode 0x00 but not found] testMissing() ([GAS])
[FAIL: original] testOriginalFailure() ([GAS])
[FAIL: expected CREATE2 call by address 0x7fa9385be102ac3eac297483dd6233d62b3e1496 for bytecode 0x00 but not found] testWrongScheme() ([GAS])

Encountered a total of 4 failing tests, 0 tests succeeded

Tip: Run `forge test --rerun` to retry only the 4 failed tests
Tip: Run `forge test --debug --match-test <TEST_NAME>` to inspect one failing test in the debugger

"#]]);
});

forgetest!(evm2_native_create_reverters, |prj, cmd| {
    prj.add_test("Creates.t.sol", include_str!("../../fixtures/evm2/Creates.t.sol"));
    cmd.args([
        "test",
        "--no-isolate",
        "--evm-version",
        "cancun",
        "--match-contract",
        "^NativeCreateRevertersTest$",
    ])
    .assert_success()
    .stdout_eq(str![[r#"
...
Ran 2 tests for test/Creates.t.sol:NativeCreateRevertersTest
[PASS] testCountedNestedReverter() ([GAS])
[PASS] testSingleNestedReverter() ([GAS])
Suite result: ok. 2 passed; 0 failed; 0 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 2 tests passed, 0 failed, 0 skipped (2 total tests)

"#]]);
});

forgetest!(evm2_native_create_reverter_reset, |prj, cmd| {
    prj.add_test("Creates.t.sol", include_str!("../../fixtures/evm2/Creates.t.sol"));
    cmd.args([
        "test",
        "--no-isolate",
        "--evm-version",
        "cancun",
        "--match-contract",
        "^NativeCreateReverterFailureTest$",
        "--match-test",
        "testRejectsDifferentSecondReverter",
    ])
    .assert_failure()
    .stdout_eq(str![[r#"
...
Ran 1 test for test/Creates.t.sol:NativeCreateReverterFailureTest
[FAIL: Reverter != expected reverter: 0x[..] != 0x[..]] testRejectsDifferentSecondReverter() ([GAS])
Suite result: FAILED. 0 passed; 1 failed; 0 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 0 tests passed, 1 failed, 0 skipped (1 total tests)

Failing tests:
Encountered 1 failing test in test/Creates.t.sol:NativeCreateReverterFailureTest
[FAIL: Reverter != expected reverter: 0x[..] != 0x[..]] testRejectsDifferentSecondReverter() ([GAS])

Encountered a total of 1 failing tests, 0 tests succeeded

Tip: Run `forge test --rerun` to retry only the 1 failed test
Tip: Run `forge test --debug --match-test <TEST_NAME>` to inspect one failing test in the debugger

"#]]);
});

forgetest!(evm2_native_emits, |prj, cmd| {
    prj.add_test("Emits.t.sol", include_str!("../../fixtures/evm2/Emits.t.sol"));
    cmd.args([
        "test",
        "--no-isolate",
        "--evm-version",
        "cancun",
        "--match-contract",
        "^NativeEmitsTest$",
    ])
    .assert_success()
    .stdout_eq(str![[r#"
...
Ran 8 tests for test/Emits.t.sol:NativeEmitsTest
[PASS] testAnonymousOverloads() ([GAS])
[PASS] testCaughtEndMismatchKeepsSettledState() ([GAS])
[PASS] testCounts() ([GAS])
[PASS] testDefaultAndEmitter() ([GAS])
[PASS] testFlagsAndCountFlags() ([GAS])
[PASS] testRevertedChildLogStillMatches() ([GAS])
[PASS] testZeroCountExpectedRevert() ([GAS])
[PASS] testZeroCountRollsBackAndStopsFrame() ([GAS])
Suite result: ok. 8 passed; 0 failed; 0 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 8 tests passed, 0 failed, 0 skipped (8 total tests)

"#]]);
});

forgetest!(evm2_native_emit_failures, |prj, cmd| {
    prj.add_test("Emits.t.sol", include_str!("../../fixtures/evm2/Emits.t.sol"));
    cmd.args(["test", "--no-isolate", "--evm-version", "cancun", "--match-contract", "^NativeEmitsFailureTest$", "--match-test", "test(MissingEmit|WrongData|WrongEmitter|ZeroCountViolation|AnonymousTemplateRejected|OriginalFailure)"])
        .assert_failure()
        .stdout_eq(str![[r#"
...
Ran 6 tests for test/Emits.t.sol:NativeEmitsFailureTest
[FAIL: use vm.expectEmitAnonymous to match anonymous events] testAnonymousTemplateRejected() ([GAS])
[FAIL: log != expected log] testMissingEmit() ([GAS])
[FAIL: original] testOriginalFailure() ([GAS])
[FAIL: E param mismatch at value: expected=2, got=3] testWrongData() ([GAS])
[FAIL: log emitter mismatch: expected=0x0000000000000000000000000000000000001234, got=0x7fa9385be102ac3eac297483dd6233d62b3e1496] testWrongEmitter() ([GAS])
[FAIL: log emitted but expected 0 times] testZeroCountViolation() ([GAS])
Suite result: FAILED. 0 passed; 6 failed; 0 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 0 tests passed, 6 failed, 0 skipped (6 total tests)

Failing tests:
Encountered 6 failing tests in test/Emits.t.sol:NativeEmitsFailureTest
[FAIL: use vm.expectEmitAnonymous to match anonymous events] testAnonymousTemplateRejected() ([GAS])
[FAIL: log != expected log] testMissingEmit() ([GAS])
[FAIL: original] testOriginalFailure() ([GAS])
[FAIL: E param mismatch at value: expected=2, got=3] testWrongData() ([GAS])
[FAIL: log emitter mismatch: expected=0x0000000000000000000000000000000000001234, got=0x7fa9385be102ac3eac297483dd6233d62b3e1496] testWrongEmitter() ([GAS])
[FAIL: log emitted but expected 0 times] testZeroCountViolation() ([GAS])

Encountered a total of 6 failing tests, 0 tests succeeded

Tip: Run `forge test --rerun` to retry only the 6 failed tests
Tip: Run `forge test --debug --match-test <TEST_NAME>` to inspect one failing test in the debugger

"#]]);
});
