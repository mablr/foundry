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
