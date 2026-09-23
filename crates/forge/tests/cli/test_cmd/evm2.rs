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
[FAIL: evm2: unsupported cheatcode (target 0x7109709ECfa91a80626fF3989D68f67F5b1DD12D, input 0x9cd23835)] testCaughtCheatcodeStillFails() ([GAS])
Suite result: FAILED. 0 passed; 1 failed; 0 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 0 tests passed, 1 failed, 0 skipped (1 total tests)

Failing tests:
Encountered 1 failing test in test/Milestone.t.sol:MilestoneUnsupportedTest
[FAIL: evm2: unsupported cheatcode (target 0x7109709ECfa91a80626fF3989D68f67F5b1DD12D, input 0x9cd23835)] testCaughtCheatcodeStillFails() ([GAS])

Encountered a total of 1 failing tests, 0 tests succeeded

Tip: Run `forge test --rerun` to retry only the 1 failed test
Tip: Run `forge test --debug --match-test <TEST_NAME>` to inspect one failing test in the debugger

"#]]);
});

forgetest!(evm2_native_basic_cheatcodes, |prj, cmd| {
    prj.add_test("Basic.t.sol", include_str!("../../fixtures/evm2/Basic.t.sol"));
    cmd.args([
        "test",
        "--no-isolate",
        "--evm-version",
        "cancun",
        "--match-contract",
        "^BasicTest$",
    ])
    .assert_success()
    .stdout_eq(str![[r#"
...
Ran 4 tests for test/Basic.t.sol:BasicTest
[PASS] testCaughtErrors() ([GAS])
[PASS] testEnvironment() ([GAS])
[PASS] testState() ([GAS])
[PASS] testStorageRollback() ([GAS])
Suite result: ok. 4 passed; 0 failed; 0 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 4 tests passed, 0 failed, 0 skipped (4 total tests)

"#]]);
});

forgetest!(evm2_native_assertion_failure, |prj, cmd| {
    prj.add_test("Basic.t.sol", include_str!("../../fixtures/evm2/Basic.t.sol"));
    cmd.args([
        "test",
        "--no-isolate",
        "--evm-version",
        "cancun",
        "--match-contract",
        "^BasicFailureTest$",
    ])
    .assert_failure()
    .stdout_eq(str![[r#"
...
Ran 1 test for test/Basic.t.sol:BasicFailureTest
[FAIL: assertion failed: 1 != 2] testAssertionFails() ([GAS])
Suite result: FAILED. 0 passed; 1 failed; 0 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 0 tests passed, 1 failed, 0 skipped (1 total tests)

Failing tests:
Encountered 1 failing test in test/Basic.t.sol:BasicFailureTest
[FAIL: assertion failed: 1 != 2] testAssertionFails() ([GAS])

Encountered a total of 1 failing tests, 0 tests succeeded

Tip: Run `forge test --rerun` to retry only the 1 failed test
Tip: Run `forge test --debug --match-test <TEST_NAME>` to inspect one failing test in the debugger

"#]]);
});
