//! End-to-end checks for the experimental evm2 Forge runner.

use alloy_primitives::{Address, B256, Bytes, U256};
use anvil::{NodeConfig, spawn};
use foundry_evm::fuzz::BaseCounterExample;
use foundry_test_utils::{forgetest_async, forgetest_init, str};

forgetest_init!(evm2_invariant_detects_handler_assertions, |prj, cmd| {
    prj.update_config(|config| {
        config.invariant.runs = 1;
        config.invariant.depth = 1;
    });
    prj.add_test(
        "NativeHandlerAssertion.t.sol",
        r#"
contract NativeAssertingHandler {
    uint256 public touched;

    function breakIt() external {
        touched = 1;
        assert(false);
    }
}

contract NativeAssertionInvariantTest {
    NativeAssertingHandler handler;

    function setUp() public {
        handler = new NativeAssertingHandler();
    }

    function targetContracts() public view returns (address[] memory targets) {
        targets = new address[](1);
        targets[0] = address(handler);
    }

    function invariantAlways() public pure {}
}
"#,
    );

    cmd.forge_fuse();
    cmd.args(["test", "--match-contract", "NativeAssertionInvariantTest"])
        .assert_failure()
        .stdout_eq(str![[r#"
[COMPILING_FILES] with [SOLC_VERSION]
[SOLC_VERSION] [ELAPSED]
Compiler run successful!

Ran 1 test for test/NativeHandlerAssertion.t.sol:NativeAssertionInvariantTest
Assertion Tests: 1 assertion bug(s) found
[FAIL: panic: assertion failed (0x01)] NativeAssertingHandler::breakIt
	[Sequence] (original: 1, shrunk: 1)
		sender=[..] addr=[..] calldata=0x6a1f9e19 args=[]
 invariantAlways() (runs: 1, calls: 1, reverts: 1)
Suite result: FAILED. 0 passed; 1 failed; 0 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 0 tests passed, 1 failed, 0 skipped (1 total tests)

Failing tests:
Encountered 1 failing test in test/NativeHandlerAssertion.t.sol:NativeAssertionInvariantTest
Assertion Tests: 1 assertion bug(s) found
[FAIL: panic: assertion failed (0x01)] NativeAssertingHandler::breakIt
	[Sequence] (original: 1, shrunk: 1)
		sender=[..] addr=[..] calldata=0x6a1f9e19 args=[]
 invariantAlways() (runs: 1, calls: 1, reverts: 1)

Encountered a total of 1 failing tests, 0 tests succeeded

Tip: Run `forge test --rerun` to retry only the 1 failed test

[SEED] (use `--fuzz-seed` to reproduce)

"#]]);
});

forgetest_init!(evm2_invariant_checks_stateful_handler_sequence, |prj, cmd| {
    prj.update_config(|config| {
        config.invariant.runs = 1;
        config.invariant.depth = 2;
    });
    prj.add_test(
        "NativeInvariant.t.sol",
        r#"
contract NativeCounter {
    uint256 public count;

    function increment(uint256) external {
        count++;
    }
}

contract NativeInvariantTest {
    NativeCounter counter;

    function setUp() public {
        counter = new NativeCounter();
    }

    function targetContracts() public view returns (address[] memory targets) {
        targets = new address[](1);
        targets[0] = address(counter);
    }

    function invariantCountBelowTwo() public view {
        require(counter.count() < 2, "count reached two");
    }
}
"#,
    );

    cmd.forge_fuse();
    cmd.args(["test", "--match-contract", "NativeInvariantTest"]).assert_failure().stdout_eq(str![
        [r#"
[COMPILING_FILES] with [SOLC_VERSION]
[SOLC_VERSION] [ELAPSED]
Compiler run successful!

Ran 1 test for test/NativeInvariant.t.sol:NativeInvariantTest
[FAIL: count reached two]
	[Sequence] (original: 2, shrunk: 2)
		sender=[..] addr=[..] calldata=0x[..] args=[..]
		sender=[..] addr=[..] calldata=0x[..] args=[..]
 invariantCountBelowTwo() (runs: 1, calls: 2, reverts: 0)
Suite result: FAILED. 0 passed; 1 failed; 0 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 0 tests passed, 1 failed, 0 skipped (1 total tests)

Failing tests:
Encountered 1 failing test in test/NativeInvariant.t.sol:NativeInvariantTest
[FAIL: count reached two]
	[Sequence] (original: 2, shrunk: 2)
		sender=[..] addr=[..] calldata=0x[..] args=[..]
		sender=[..] addr=[..] calldata=0x[..] args=[..]
 invariantCountBelowTwo() (runs: 1, calls: 2, reverts: 0)

Encountered a total of 1 failing tests, 0 tests succeeded

Tip: Run `forge test --rerun` to retry only the 1 failed test

[SEED] (use `--fuzz-seed` to reproduce)

"#]
    ]);
});

forgetest_init!(evm2_runs_table_rows_from_setup_fixtures, |prj, cmd| {
    prj.add_test(
        "NativeTable.t.sol",
        r#"
contract NativeTableTest {
    uint256[] values;

    function setUp() public {
        values.push(1);
        values.push(2);
    }

    function fixtureValue() public view returns (uint256[] memory) {
        return values;
    }

    function tablePass(uint256 value) public pure {
        require(value != 0, "missing value");
    }

    function tableFail(uint256 value) public pure {
        require(value != 2, "bad row");
    }
}
"#,
    );

    cmd.forge_fuse();
    cmd.args(["test", "--match-contract", "NativeTableTest"]).assert_failure().stdout_eq(str![[
        r#"
[COMPILING_FILES] with [SOLC_VERSION]
[SOLC_VERSION] [ELAPSED]
Compiler run successful!

Ran 2 tests for test/NativeTable.t.sol:NativeTableTest
[FAIL: bad row; counterexample: [..]] tableFail(uint256) (runs: 2, [AVG_GAS])
[PASS] tablePass(uint256) (runs: 2, [AVG_GAS])
Suite result: FAILED. 1 passed; 1 failed; 0 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 1 tests passed, 1 failed, 0 skipped (2 total tests)

Failing tests:
Encountered 1 failing test in test/NativeTable.t.sol:NativeTableTest
[FAIL: bad row; counterexample: [..]] tableFail(uint256) (runs: 2, [AVG_GAS])

Encountered a total of 1 failing tests, 1 tests succeeded

Tip: Run `forge test --rerun` to retry only the 1 failed test
Tip: Run `forge test --debug --match-test <TEST_NAME>` to inspect one failing test in the debugger

"#
    ]]);
});

forgetest_init!(evm2_replays_explicit_fuzz_input, |prj, cmd| {
    prj.update_config(|config| config.isolate = false);
    prj.add_test(
        "NativeFuzzReplay.t.sol",
        r#"
interface Vm {
    function assume(bool condition) external pure;
}

contract NativeFuzzReplayTest {
    Vm constant vm = Vm(address(uint160(uint256(keccak256("hevm cheat code")))));

    function testUnit() public pure {}

    function testFuzz_value(uint256 value) public pure {
        require(value != 42, "boom");
    }

    function testFuzz_payable(uint256 value) public payable {
        require(msg.value == value, "wrong replay value");
    }

    function testFuzz_assume(uint256 value) public pure {
        vm.assume(value != 42);
    }
}
"#,
    );
    let mut calldata =
        alloy_primitives::keccak256("testFuzz_value(uint256)").as_slice()[..4].to_vec();
    calldata.extend_from_slice(&U256::from(42).to_be_bytes::<32>());
    let failure = BaseCounterExample {
        warp: None,
        roll: None,
        sender: None,
        addr: None,
        calldata: calldata.into(),
        value: None,
        contract_name: None,
        func_name: None,
        signature: None,
        args: None,
        raw_args: None,
        traces: None,
        show_solidity: false,
        fuzz: Default::default(),
    };
    let input = prj.root().join("fuzz-input.json");
    std::fs::write(&input, serde_json::to_vec(&failure).unwrap()).unwrap();

    cmd.forge_fuse();
    cmd.args(["test", "--match-contract", "NativeFuzzReplayTest", "--fuzz-input-file"])
        .arg(&input)
        .assert_failure()
        .stdout_eq(str![[r#"
[COMPILING_FILES] with [SOLC_VERSION]
[SOLC_VERSION] [ELAPSED]
Compiler run successful!

Ran 4 tests for test/NativeFuzzReplay.t.sol:NativeFuzzReplayTest
[SKIP: not runnable in replay mode] testFuzz_assume(uint256) (runs: 0, [AVG_GAS])
[SKIP: not runnable in replay mode] testFuzz_payable(uint256) (runs: 0, [AVG_GAS])
[FAIL: boom; counterexample: 		sender=0x1804c8AB1F12E6bbf3894d4083f33e07309d1f38 addr=0x7FA9385bE102ac3EAc297483Dd6233D62b3e1496 calldata=0x4c39e060000000000000000000000000000000000000000000000000000000000000002a args=[42]] testFuzz_value(uint256) (runs: 0, [AVG_GAS])
[SKIP: not runnable in replay mode] testUnit() ([GAS])
Suite result: FAILED. 0 passed; 1 failed; 3 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 0 tests passed, 1 failed, 3 skipped (4 total tests)

Failing tests:
Encountered 1 failing test in test/NativeFuzzReplay.t.sol:NativeFuzzReplayTest
[FAIL: boom; counterexample: 		sender=0x1804c8AB1F12E6bbf3894d4083f33e07309d1f38 addr=0x7FA9385bE102ac3EAc297483Dd6233D62b3e1496 calldata=0x4c39e060000000000000000000000000000000000000000000000000000000000000002a args=[42]] testFuzz_value(uint256) (runs: 0, [AVG_GAS])

Encountered a total of 1 failing tests, 0 tests succeeded

Tip: Run `forge test --rerun` to retry only the 1 failed test
Tip: Run `forge test --debug --match-test <TEST_NAME>` to inspect one failing test in the debugger

[SEED] (use `--fuzz-seed` to reproduce)

"#]]);

    let mut calldata =
        alloy_primitives::keccak256("testFuzz_payable(uint256)").as_slice()[..4].to_vec();
    calldata.extend_from_slice(&U256::from(7).to_be_bytes::<32>());
    let mut success = failure;
    success.calldata = calldata.into();
    success.value = Some(U256::from(7));
    std::fs::write(&input, serde_json::to_vec(&success).unwrap()).unwrap();
    cmd.forge_fuse();
    cmd.args(["test", "--match-contract", "NativeFuzzReplayTest", "--fuzz-input-file"])
        .arg(&input)
        .assert_success()
        .stdout_eq(str![[r#"
No files changed, compilation skipped

Ran 4 tests for test/NativeFuzzReplay.t.sol:NativeFuzzReplayTest
[SKIP: not runnable in replay mode] testFuzz_assume(uint256) (runs: 0, [AVG_GAS])
[PASS] testFuzz_payable(uint256) (runs: 1, [AVG_GAS])
[SKIP: not runnable in replay mode] testFuzz_value(uint256) (runs: 0, [AVG_GAS])
[SKIP: not runnable in replay mode] testUnit() ([GAS])
Suite result: ok. 1 passed; 0 failed; 3 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 1 tests passed, 0 failed, 3 skipped (4 total tests)

"#]]);

    let mut calldata =
        alloy_primitives::keccak256("testFuzz_assume(uint256)").as_slice()[..4].to_vec();
    calldata.extend_from_slice(&U256::from(42).to_be_bytes::<32>());
    success.calldata = calldata.into();
    success.value = None;
    std::fs::write(&input, serde_json::to_vec(&success).unwrap()).unwrap();
    cmd.forge_fuse();
    cmd.args(["test", "--match-contract", "NativeFuzzReplayTest", "--fuzz-input-file"])
        .arg(&input)
        .assert_success()
        .stdout_eq(str![[r#"
No files changed, compilation skipped

Ran 4 tests for test/NativeFuzzReplay.t.sol:NativeFuzzReplayTest
[SKIP: persisted fuzz failure rejected by `vm.assume`] testFuzz_assume(uint256) (runs: 0, [AVG_GAS])
[SKIP: not runnable in replay mode] testFuzz_payable(uint256) (runs: 0, [AVG_GAS])
[SKIP: not runnable in replay mode] testFuzz_value(uint256) (runs: 0, [AVG_GAS])
[SKIP: not runnable in replay mode] testUnit() ([GAS])
Suite result: ok. 0 passed; 0 failed; 4 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 0 tests passed, 0 failed, 4 skipped (4 total tests)

"#]]);
});

forgetest_init!(evm2_reports_setup_failures_as_suite_results, |prj, cmd| {
    prj.update_config(|config| config.isolate = false);
    prj.add_test(
        "NativeSetupFailure.t.sol",
        r#"
contract ConstructorFailureTest {
    constructor() { revert("deploy failure"); }
    function testNeverRuns() public pure {}
}

contract SetupFailureTest {
    function setUp() public pure { revert("setup failure"); }
    function testNeverRuns() public pure {}
}

contract HealthyTest {
    function testRuns() public pure {}
}
"#,
    );

    cmd.forge_fuse();
    cmd.arg("test").assert_failure().stdout_eq(str![[r#"
[COMPILING_FILES] with [SOLC_VERSION]
[SOLC_VERSION] [ELAPSED]
Compiler run successful!

Ran 1 test for test/NativeSetupFailure.t.sol:ConstructorFailureTest
[FAIL: deploy failure] constructor() ([GAS])
Suite result: FAILED. 0 passed; 1 failed; 0 skipped; [ELAPSED]

Ran 1 test for test/NativeSetupFailure.t.sol:HealthyTest
[PASS] testRuns() ([GAS])
Suite result: ok. 1 passed; 0 failed; 0 skipped; [ELAPSED]

Ran 1 test for test/NativeSetupFailure.t.sol:SetupFailureTest
[FAIL: setup failure] setUp() ([GAS])
Suite result: FAILED. 0 passed; 1 failed; 0 skipped; [ELAPSED]

Ran 3 test suites [ELAPSED]: 1 tests passed, 2 failed, 0 skipped (3 total tests)

Failing tests:
Encountered 1 failing test in test/NativeSetupFailure.t.sol:ConstructorFailureTest
[FAIL: deploy failure] constructor() ([GAS])

Encountered 1 failing test in test/NativeSetupFailure.t.sol:SetupFailureTest
[FAIL: setup failure] setUp() ([GAS])

Encountered a total of 2 failing tests, 1 tests succeeded

Tip: Run `forge test --rerun` to retry only the 2 failed tests
Tip: Run `forge test --debug --match-test <TEST_NAME>` to inspect one failing test in the debugger

"#]]);
});

forgetest_init!(evm2_expect_revert_matches_external_calls, |prj, cmd| {
    prj.update_config(|config| config.isolate = false);
    prj.add_test(
        "NativeExpectRevert.t.sol",
        r#"
interface Vm {
    function expectRevert() external;
    function expectRevert(bytes4 selector) external;
    function expectRevert(bytes calldata reason) external;
}

contract RevertingConstructor {
    constructor() { revert("constructor"); }
}

contract NativeExpectRevertTest {
    error Boom();
    Vm constant vm = Vm(address(0x7109709ECfa91a80626fF3989D68f67F5b1DD12D));

    function fail() external pure returns (uint256) { revert("reason"); }
    function failCustom() external pure { revert Boom(); }

    function testAnyRevert() public {
        vm.expectRevert();
        require(this.fail() == 0, "expected dummy return data");
    }

    function testStringReason() public {
        vm.expectRevert(bytes("reason"));
        this.fail();
    }

    function testSelector() public {
        vm.expectRevert(Boom.selector);
        this.failCustom();
    }

    function testNestedRevert() public {
        vm.expectRevert();
        this.failAfterCatch();
    }

    function testConstructorRevert() public {
        vm.expectRevert(bytes("constructor"));
        RevertingConstructor deployed = new RevertingConstructor();
        require(address(deployed) == address(1), "expected dummy create address");
    }

    function failAfterCatch() external view {
        try this.fail() returns (uint256) {} catch {}
        revert("outer");
    }
}
"#,
    );

    cmd.forge_fuse();
    cmd.arg("test").assert_success().stdout_eq(str![[r#"
[COMPILING_FILES] with [SOLC_VERSION]
[SOLC_VERSION] [ELAPSED]
Compiler run successful!

Ran 5 tests for test/NativeExpectRevert.t.sol:NativeExpectRevertTest
[PASS] testAnyRevert() ([GAS])
[PASS] testConstructorRevert() ([GAS])
[PASS] testNestedRevert() ([GAS])
[PASS] testSelector() ([GAS])
[PASS] testStringReason() ([GAS])
Suite result: ok. 5 passed; 0 failed; 0 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 5 tests passed, 0 failed, 0 skipped (5 total tests)

"#]]);
});

forgetest_init!(evm2_expect_revert_rejects_successful_call, |prj, cmd| {
    prj.update_config(|config| config.isolate = false);
    prj.add_test(
        "NativeExpectRevertFailure.t.sol",
        r#"
interface Vm {
    function expectRevert() external;
    function expectRevert(bytes4 selector) external;
    function expectRevert(bytes calldata reason) external;
}

contract SuccessfulConstructor {}

contract NativeExpectRevertFailureTest {
    Vm constant vm = Vm(address(0x7109709ECfa91a80626fF3989D68f67F5b1DD12D));

    function succeeds() external pure returns (uint256) { return 1; }
    function fails() external pure { revert("actual"); }
    function failsWithoutData() external pure { revert(); }

    function testExpectedRevertMissing() public {
        vm.expectRevert();
        this.succeeds();
    }

    function testReasonMismatch() public {
        vm.expectRevert(bytes("expected"));
        this.fails();
    }

    function testDanglingExpectation() public {
        vm.expectRevert();
    }

    function testSelectorDoesNotMatchEmptyData() public {
        vm.expectRevert(bytes4(0x12345678));
        this.failsWithoutData();
    }

    function testExpectedConstructorRevertMissing() public {
        vm.expectRevert();
        new SuccessfulConstructor();
    }
}
"#,
    );

    cmd.forge_fuse();
    cmd.arg("test").assert_failure().stdout_eq(str![[r#"
[COMPILING_FILES] with [SOLC_VERSION]
[SOLC_VERSION] [ELAPSED]
Compiler run successful!

Ran 5 tests for test/NativeExpectRevertFailure.t.sol:NativeExpectRevertFailureTest
[FAIL: call didn't revert at a lower depth than cheatcode call depth] testDanglingExpectation() ([GAS])
[FAIL: next call did not revert as expected] testExpectedConstructorRevertMissing() ([GAS])
[FAIL: next call did not revert as expected] testExpectedRevertMissing() ([GAS])
[FAIL: revert data did not match the expected reason] testReasonMismatch() ([GAS])
[FAIL: revert data did not match the expected reason] testSelectorDoesNotMatchEmptyData() ([GAS])
Suite result: FAILED. 0 passed; 5 failed; 0 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 0 tests passed, 5 failed, 0 skipped (5 total tests)

Failing tests:
Encountered 5 failing tests in test/NativeExpectRevertFailure.t.sol:NativeExpectRevertFailureTest
[FAIL: call didn't revert at a lower depth than cheatcode call depth] testDanglingExpectation() ([GAS])
[FAIL: next call did not revert as expected] testExpectedConstructorRevertMissing() ([GAS])
[FAIL: next call did not revert as expected] testExpectedRevertMissing() ([GAS])
[FAIL: revert data did not match the expected reason] testReasonMismatch() ([GAS])
[FAIL: revert data did not match the expected reason] testSelectorDoesNotMatchEmptyData() ([GAS])

Encountered a total of 5 failing tests, 0 tests succeeded

Tip: Run `forge test --rerun` to retry only the 5 failed tests
Tip: Run `forge test --debug --match-test <TEST_NAME>` to inspect one failing test in the debugger

"#]]);
});

forgetest_async!(evm2_reads_resolved_fork_state, |prj, cmd| {
    let (api, handle) = spawn(NodeConfig::test()).await;
    let address = Address::with_last_byte(0x42);
    api.anvil_set_code(
        address,
        Bytes::from_static(&[0x5f, 0x54, 0x5f, 0x52, 0x60, 0x20, 0x5f, 0xf3]),
    )
    .await
    .unwrap();
    api.anvil_set_storage_at(address, U256::ZERO, B256::with_last_byte(42)).await.unwrap();
    api.anvil_mine(Some(U256::ONE), None).await.unwrap();
    prj.update_config(|config| config.isolate = false);
    prj.add_test(
        "NativeFork.t.sol",
        r#"
interface Remote {
    function value() external view returns (uint256);
}

interface Vm {
    function snapshotState() external returns (uint256);
    function revertToState(uint256 snapshotId) external returns (bool);
    function store(address target, bytes32 slot, bytes32 value) external;
}

contract NativeForkTest {
    Vm constant vm = Vm(address(0x7109709ECfa91a80626fF3989D68f67F5b1DD12D));

    function testFork() public view {
        require(block.number == 1, "wrong fork block");
        require(Remote(address(0x42)).value() == 42, "wrong fork storage");
    }

    function testForkSnapshot() public {
        uint256 id = vm.snapshotState();
        vm.store(address(0x42), bytes32(0), bytes32(uint256(99)));
        require(Remote(address(0x42)).value() == 99, "fork write did not apply");
        require(vm.revertToState(id), "snapshot missing");
        require(Remote(address(0x42)).value() == 42, "fork storage was not restored");
    }
}
"#,
    );

    cmd.forge_fuse();
    cmd.args(["test", "--fork-url", &handle.http_endpoint(), "--match-test", "testFork"])
        .assert_success()
        .stdout_eq(str![[r#"
[COMPILING_FILES] with [SOLC_VERSION]
[SOLC_VERSION] [ELAPSED]
Compiler run successful!

Ran 2 tests for test/NativeFork.t.sol:NativeForkTest
[PASS] testFork() ([GAS])
[PASS] testForkSnapshot() ([GAS])
Suite result: ok. 2 passed; 0 failed; 0 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 2 tests passed, 0 failed, 0 skipped (2 total tests)

"#]]);
});

forgetest_init!(evm2_storage_cheatcodes_follow_frame_rollback, |prj, cmd| {
    prj.update_config(|config| config.isolate = false);
    prj.add_test(
        "NativeStorage.t.sol",
        r#"
interface Vm {
    function load(address target, bytes32 slot) external view returns (bytes32);
    function store(address target, bytes32 slot, bytes32 value) external;
}

contract NativeStorageTest {
    Vm constant vm = Vm(address(0x7109709ECfa91a80626fF3989D68f67F5b1DD12D));
    uint256 public value;

    function setUp() public {
        vm.store(address(this), bytes32(0), bytes32(uint256(42)));
    }

    function testStorage() public view {
        require(value == 42, "store did not update storage");
        require(uint256(vm.load(address(this), bytes32(0))) == 42, "load returned wrong value");
    }

    function testRevertedChild() public {
        try this.changeAndRevert() {} catch {}
        require(value == 42, "child write survived revert");
    }

    function testStoreRejectsPrecompile() public {
        (bool success,) = address(vm).call(
            abi.encodeWithSignature("store(address,bytes32,bytes32)", address(1), bytes32(0), bytes32(uint256(1)))
        );
        require(!success, "precompile was modified");
    }

    function changeAndRevert() external {
        vm.store(address(this), bytes32(0), bytes32(uint256(99)));
        revert("later");
    }
}
"#,
    );

    cmd.forge_fuse();
    cmd.arg("test").assert_success().stdout_eq(str![[r#"
[COMPILING_FILES] with [SOLC_VERSION]
[SOLC_VERSION] [ELAPSED]
Compiler run successful!

Ran 3 tests for test/NativeStorage.t.sol:NativeStorageTest
[PASS] testRevertedChild() ([GAS])
[PASS] testStorage() ([GAS])
[PASS] testStoreRejectsPrecompile() ([GAS])
Suite result: ok. 3 passed; 0 failed; 0 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 3 tests passed, 0 failed, 0 skipped (3 total tests)

"#]]);
});

forgetest_init!(evm2_deploys_linked_libraries, |prj, cmd| {
    prj.update_config(|config| config.isolate = false);
    prj.add_source(
        "NativeLib.sol",
        r#"
library NativeLib {
    function identity(uint256 value) external pure returns (uint256) {
        return value;
    }
}
"#,
    );
    prj.add_test(
        "NativeLibrary.t.sol",
        r#"
import "src/NativeLib.sol";

contract NativeLibraryTest {
    uint256 value;

    constructor() {
        value = NativeLib.identity(42);
    }

    function testLibrary() public view {
        require(value == 42, "library call failed");
    }
}
"#,
    );

    cmd.forge_fuse();
    cmd.args(["test", "--match-test", "testLibrary"]).assert_success();

    prj.update_config(|config| config.create2_deployer = alloy_primitives::Address::ZERO);
    cmd.forge_fuse();
    cmd.args(["test", "--match-test", "testLibrary"]).assert_success();
});

forgetest_init!(evm2_installs_create2_factory_after_constructor, |prj, cmd| {
    prj.update_config(|config| config.isolate = false);
    prj.add_test(
        "NativeFactory.t.sol",
        r#"
contract NativeFactoryTest {
    address constant FACTORY = 0x4e59b44847b379578588920cA78FbF26c0B4956C;

    constructor() {
        require(FACTORY.code.length == 0, "factory installed before constructor");
    }

    function testFactoryInstalled() public view {
        require(FACTORY.code.length > 0, "factory not installed after constructor");
    }
}
"#,
    );

    cmd.forge_fuse();
    cmd.args(["test", "--match-test", "testFactoryInstalled"]).assert_success();
});

forgetest_init!(evm2_runs_compiled_setup_and_unit_test, |prj, cmd| {
    prj.update_config(|config| config.fuzz.runs = 8);
    prj.update_config(|config| config.isolate = false);
    prj.add_test(
        "Native.t.sol",
        r#"
interface Vm {
    function deal(address account, uint256 newBalance) external;
}

contract NativeTest {
    Vm constant vm = Vm(address(0x7109709ECfa91a80626fF3989D68f67F5b1DD12D));

    function setUp() public {
        vm.deal(address(this), 5 ether);
    }

    function testBalance() public view {
        require(address(this).balance == 5 ether, "wrong balance");
    }

    function testBroken() public pure {
        require(false, "expected failure");
    }

    function testFuzz(uint256) public pure {}
}
"#,
    );

    cmd.forge_fuse();
    cmd.args(["test", "--match-test", "testBalance"]).assert_success().stdout_eq(str![[r#"
[COMPILING_FILES] with [SOLC_VERSION]
[SOLC_VERSION] [ELAPSED]
Compiler run successful!

Ran 1 test for test/Native.t.sol:NativeTest
[PASS] testBalance() ([GAS])
Suite result: ok. 1 passed; 0 failed; 0 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 1 tests passed, 0 failed, 0 skipped (1 total tests)

"#]]);

    cmd.forge_fuse();
    cmd.args(["test", "--match-test", "testBroken"]).assert_failure().stdout_eq(str![[r#"
No files changed, compilation skipped

Ran 1 test for test/Native.t.sol:NativeTest
[FAIL: expected failure] testBroken() ([GAS])
Suite result: FAILED. 0 passed; 1 failed; 0 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 0 tests passed, 1 failed, 0 skipped (1 total tests)
...
"#]]);

    cmd.forge_fuse();
    cmd.args(["test", "--match-test", "testFuzz"]).assert_success().stdout_eq(str![[r#"
No files changed, compilation skipped

Ran 1 test for test/Native.t.sol:NativeTest
[PASS] testFuzz(uint256) (runs: 8, [AVG_GAS])
Suite result: ok. 1 passed; 0 failed; 0 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 1 tests passed, 0 failed, 0 skipped (1 total tests)

"#]]);
});

forgetest_init!(evm2_fuzz_campaign_handles_assumptions_and_failures, |prj, cmd| {
    prj.update_config(|config| {
        config.isolate = false;
        config.fuzz.runs = 8;
        config.fuzz.seed = Some(alloy_primitives::U256::ONE);
    });
    prj.add_test(
        "NativeFuzz.t.sol",
        r#"
interface Vm {
    function assume(bool condition) external;
}

contract NativeFuzzTest {
    Vm constant vm = Vm(address(uint160(uint256(keccak256("hevm cheat code")))));

    function testFuzzAssume(bool valid) public {
        vm.assume(valid);
    }

    function testFuzzFails(bool valid) public pure {
        require(!valid, "boom");
    }

    function testFuzzRejects(bool) public {
        vm.assume(false);
    }
}
"#,
    );

    cmd.forge_fuse();
    cmd.args(["test", "--match-test", "testFuzzAssume"]).assert_success().stdout_eq(str![[r#"
[COMPILING_FILES] with [SOLC_VERSION]
[SOLC_VERSION] [ELAPSED]
Compiler run successful!

Ran 1 test for test/NativeFuzz.t.sol:NativeFuzzTest
[PASS] testFuzzAssume(bool) (runs: 8, [AVG_GAS])
Suite result: ok. 1 passed; 0 failed; 0 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 1 tests passed, 0 failed, 0 skipped (1 total tests)

"#]]);

    cmd.forge_fuse();
    cmd.args(["test", "--match-test", "testFuzzFails"])
        .assert_failure()
        .stdout_eq(str![[r#"
No files changed, compilation skipped

Ran 1 test for test/NativeFuzz.t.sol:NativeFuzzTest
[FAIL: boom; counterexample: 		sender=0x1804c8AB1F12E6bbf3894d4083f33e07309d1f38 addr=0x7FA9385bE102ac3EAc297483Dd6233D62b3e1496 calldata=0xa043f3ae0000000000000000000000000000000000000000000000000000000000000001 args=[true]] testFuzzFails(bool) (runs: 1, [AVG_GAS])
Suite result: FAILED. 0 passed; 1 failed; 0 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 0 tests passed, 1 failed, 0 skipped (1 total tests)

Failing tests:
Encountered 1 failing test in test/NativeFuzz.t.sol:NativeFuzzTest
[FAIL: boom; counterexample: 		sender=0x1804c8AB1F12E6bbf3894d4083f33e07309d1f38 addr=0x7FA9385bE102ac3EAc297483Dd6233D62b3e1496 calldata=0xa043f3ae0000000000000000000000000000000000000000000000000000000000000001 args=[true]] testFuzzFails(bool) (runs: 1, [AVG_GAS])

Encountered a total of 1 failing tests, 0 tests succeeded

Tip: Run `forge test --rerun` to retry only the 1 failed test
Tip: Run `forge test --debug --match-test <TEST_NAME>` to inspect one failing test in the debugger

[SEED] (use `--fuzz-seed` to reproduce)

"#]]);

    prj.update_config(|config| config.fuzz.max_test_rejects = 2);
    cmd.forge_fuse();
    cmd.args(["test", "--match-test", "testFuzzRejects"]).assert_failure().stdout_eq(str![[r#"
No files changed, compilation skipped

Ran 1 test for test/NativeFuzz.t.sol:NativeFuzzTest
[FAIL: maximum fuzz test rejections exceeded] testFuzzRejects(bool) (runs: 0, [AVG_GAS])
Suite result: FAILED. 0 passed; 1 failed; 0 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 0 tests passed, 1 failed, 0 skipped (1 total tests)

Failing tests:
Encountered 1 failing test in test/NativeFuzz.t.sol:NativeFuzzTest
[FAIL: maximum fuzz test rejections exceeded] testFuzzRejects(bool) (runs: 0, [AVG_GAS])

Encountered a total of 1 failing tests, 0 tests succeeded

Tip: Run `forge test --rerun` to retry only the 1 failed test
Tip: Run `forge test --debug --match-test <TEST_NAME>` to inspect one failing test in the debugger

[SEED] (use `--fuzz-seed` to reproduce)

"#]]);
});

forgetest_init!(evm2_fuzz_uses_storage_fixtures_after_setup, |prj, cmd| {
    prj.update_config(|config| {
        config.isolate = false;
        config.fuzz.runs = 8;
        config.fuzz.seed = Some(alloy_primitives::U256::ONE);
        config.fuzz.max_test_rejects = 100;
    });
    prj.add_test(
        "NativeFixture.t.sol",
        r#"
interface Vm {
    function assume(bool condition) external;
}

contract NativeFixtureTest {
    Vm constant vm = Vm(address(uint160(uint256(keccak256("hevm cheat code")))));
    uint256[] public fixture_value;

    function setUp() public {
        fixture_value.push(42);
        fixture_value.push(99);
    }

    function testFuzzFixture(uint256 value) public {
        vm.assume(value == 42 || value == 99);
    }

    function fixture_key() public pure returns (bytes32[] memory keys) {
        keys = new bytes32[](1);
        keys[0] = bytes32(uint256(12345));
    }

    function testFuzzFunctionFixture(bytes32 key) public {
        vm.assume(key == bytes32(uint256(12345)));
    }
}
"#,
    );

    cmd.forge_fuse();
    cmd.args(["test", "--match-test", "testFuzzFixture"]).assert_success().stdout_eq(str![[r#"
[COMPILING_FILES] with [SOLC_VERSION]
[SOLC_VERSION] [ELAPSED]
Compiler run successful!

Ran 1 test for test/NativeFixture.t.sol:NativeFixtureTest
[PASS] testFuzzFixture(uint256) (runs: 8, [AVG_GAS])
Suite result: ok. 1 passed; 0 failed; 0 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 1 tests passed, 0 failed, 0 skipped (1 total tests)

"#]]);

    cmd.forge_fuse();
    cmd.args(["test", "--match-test", "testFuzzFunctionFixture"]).assert_success().stdout_eq(str![
        [r#"
No files changed, compilation skipped

Ran 1 test for test/NativeFixture.t.sol:NativeFixtureTest
[PASS] testFuzzFunctionFixture(bytes32) (runs: 8, [AVG_GAS])
Suite result: ok. 1 passed; 0 failed; 0 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 1 tests passed, 0 failed, 0 skipped (1 total tests)

"#]
    ]);
});

forgetest_init!(evm2_fuzz_bounds_enum_inputs, |prj, cmd| {
    prj.update_config(|config| {
        config.isolate = false;
        config.fuzz.runs = 64;
        config.fuzz.seed = Some(U256::ONE);
    });
    prj.add_source("LibEnum.sol", "enum LibEnumVal { L0, L1 }");
    prj.add_test(
        "NativeEnum.t.sol",
        r#"
import {LibEnumVal} from "src/LibEnum.sol";

contract NativeEnumTest {
    enum Choice { A, B, C }

    struct Input {
        Choice choice;
        LibEnumVal libraryChoice;
    }

    function testFuzzScalar(Choice choice) public pure {
        require(uint8(choice) < 3, "invalid choice");
    }

    function testFuzzStruct(Input memory input) public pure {
        require(uint8(input.choice) < 3, "invalid choice");
        require(uint8(input.libraryChoice) < 2, "invalid library choice");
    }
}
"#,
    );

    cmd.forge_fuse();
    cmd.args(["test", "--match-contract", "NativeEnumTest"]).assert_success().stdout_eq(str![[
        r#"
[COMPILING_FILES] with [SOLC_VERSION]
[SOLC_VERSION] [ELAPSED]
Compiler run successful!

Ran 2 tests for test/NativeEnum.t.sol:NativeEnumTest
[PASS] testFuzzScalar(uint8) (runs: 64, [AVG_GAS])
[PASS] testFuzzStruct((uint8,uint8)) (runs: 64, [AVG_GAS])
Suite result: ok. 2 passed; 0 failed; 0 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 2 tests passed, 0 failed, 0 skipped (2 total tests)

"#
    ]]);
});

forgetest_init!(evm2_reports_console_and_opcode_logs, |prj, cmd| {
    prj.update_config(|config| config.isolate = false);
    prj.add_test(
        "NativeLogs.t.sol",
        r#"
import "forge-std/console.sol";

contract NativeLogsTest {
    event log(string message);

    function setUp() public {
        emit log("setup");
    }

    function testLogs() public {
        console.log("console");
        emit log("event");
    }
}
"#,
    );

    cmd.forge_fuse();
    cmd.args(["test", "--match-test", "testLogs", "-vv"]).assert_success().stdout_eq(str![[r#"
[COMPILING_FILES] with [SOLC_VERSION]
[SOLC_VERSION] [ELAPSED]
Compiler run successful!

Ran 1 test for test/NativeLogs.t.sol:NativeLogsTest
[PASS] testLogs() ([GAS])
Logs:
  setup
  console
  event

Suite result: ok. 1 passed; 0 failed; 0 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 1 tests passed, 0 failed, 0 skipped (1 total tests)

"#]]);

    cmd.forge_fuse();
    cmd.args(["test", "--match-test", "testLogs", "-vvv"])
        .assert_success()
        .stdout_eq(str![[r#"
No files changed, compilation skipped

Ran 1 test for test/NativeLogs.t.sol:NativeLogsTest
[PASS] testLogs() ([GAS])
Logs:
  setup
  console
  event

Traces:
  [252794] → new NativeLogsTest@0x7FA9385bE102ac3EAc297483Dd6233D62b3e1496
    └─ ← [Return] 930 bytes of code
  [68137] → new <unknown>@0x4e59b44847b379578588920cA78FbF26c0B4956C
    └─ ← [Return] 69 bytes of code
  [22982] NativeLogsTest::setUp()
    ├─ emit log(message: "setup")
    └─ ← [Stop]
  [26341] NativeLogsTest::testLogs()
    ├─ [0] 0x000000000000000000636F6e736F6c652e6c6f67::41304fac(00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000007636f6e736f6c6500000000000000000000000000000000000000000000000000) [staticcall]
    │   └─ ← [Return]
    ├─ emit log(message: "event")
    └─ ← [Stop]

Suite result: ok. 1 passed; 0 failed; 0 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 1 tests passed, 0 failed, 0 skipped (1 total tests)

"#]]);
});

forgetest_init!(evm2_redacts_cheatcode_trace_io, |prj, cmd| {
    prj.update_config(|config| config.isolate = false);
    prj.add_test(
        "NativeTrace.t.sol",
        r#"
interface Vm {
    function deal(address account, uint256 balance) external;
}

contract NativeTraceTest {
    Vm constant vm = Vm(address(uint160(uint256(keccak256("hevm cheat code")))));
    event Ping(uint256 indexed id, uint256 amount);

    function testDeal() public {
        vm.deal(address(0xBEEF), 123456789);
        emit Ping(7, 11);
    }
}
"#,
    );

    cmd.forge_fuse();
    cmd.args(["test", "--match-test", "testDeal", "-vvv"]).assert_success().stdout_eq(str![[r#"
[COMPILING_FILES] with [SOLC_VERSION]
[SOLC_VERSION] [ELAPSED]
Compiler run successful!

Ran 1 test for test/NativeTrace.t.sol:NativeTraceTest
[PASS] testDeal() ([GAS])
Traces:
  [177015] → new NativeTraceTest@0x7FA9385bE102ac3EAc297483Dd6233D62b3e1496
    └─ ← [Return] 572 bytes of code
  [68137] → new <unknown>@0x4e59b44847b379578588920cA78FbF26c0B4956C
    └─ ← [Return] 69 bytes of code
  [26141] NativeTraceTest::testDeal()
    ├─ [0] VM::deal()
    │   └─ ← [Return]
    ├─ emit Ping(id: 7, amount: 11)
    └─ ← [Stop]

Suite result: ok. 1 passed; 0 failed; 0 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 1 tests passed, 0 failed, 0 skipped (1 total tests)

"#]]);
});

forgetest_init!(evm2_warp_updates_live_and_later_block_context, |prj, cmd| {
    prj.update_config(|config| config.isolate = false);
    prj.add_test(
        "NativeWarp.t.sol",
        r#"
interface Vm {
    function warp(uint256 newTimestamp) external;
}

contract NativeWarpTest {
    Vm constant vm = Vm(address(0x7109709ECfa91a80626fF3989D68f67F5b1DD12D));

    function setUp() public {
        vm.warp(123);
    }

    function testTimestampFromSetup() public view {
        require(block.timestamp == 123, "setup timestamp was lost");
    }

    function testWarpWithinCall() public {
        vm.warp(456);
        require(block.timestamp == 456, "live timestamp was not updated");
    }
}
"#,
    );

    cmd.forge_fuse();
    cmd.arg("test").assert_success().stdout_eq(str![[r#"
[COMPILING_FILES] with [SOLC_VERSION]
[SOLC_VERSION] [ELAPSED]
Compiler run successful!

Ran 2 tests for test/NativeWarp.t.sol:NativeWarpTest
[PASS] testTimestampFromSetup() ([GAS])
[PASS] testWarpWithinCall() ([GAS])
Suite result: ok. 2 passed; 0 failed; 0 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 2 tests passed, 0 failed, 0 skipped (2 total tests)

"#]]);
});

forgetest_init!(evm2_isolated_call_merges_state_and_block, |prj, cmd| {
    prj.update_config(|config| config.isolate = true);
    prj.add_test(
        "NativeIsolation.t.sol",
        r#"
interface Vm {
    function warp(uint256 newTimestamp) external;
}

contract NativeIsolationChild {
    Vm constant vm = Vm(address(0x7109709ECfa91a80626fF3989D68f67F5b1DD12D));
    uint256 public value;

    function set() external {
        value = 1;
        vm.warp(456);
    }

    function origin() external view returns (address) {
        return tx.origin;
    }
}

contract NativeIsolationTest {
    NativeIsolationChild child;

    function setUp() public {
        child = new NativeIsolationChild();
    }

    function testIsolatedCall() public {
        child.set();
        require(child.value() == 1, "child state was lost");
        require(block.timestamp == 456, "child block change was lost");
        require(child.origin() == tx.origin, "child transaction changed origin");
    }
}
"#,
    );

    cmd.forge_fuse();
    cmd.args(["test", "--match-test", "testIsolatedCall"]).assert_success().stdout_eq(str![[r#"
[COMPILING_FILES] with [SOLC_VERSION]
[SOLC_VERSION] [ELAPSED]
Compiler run successful!

Ran 1 test for test/NativeIsolation.t.sol:NativeIsolationTest
[PASS] testIsolatedCall() ([GAS])
Suite result: ok. 1 passed; 0 failed; 0 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 1 tests passed, 0 failed, 0 skipped (1 total tests)

"#]]);
});

forgetest_init!(evm2_prank_tracks_call_lifetime, |prj, cmd| {
    prj.update_config(|config| config.isolate = false);
    prj.add_test(
        "NativePrank.t.sol",
        r#"
interface Vm {
    function prank(address msgSender) external;
    function prank(address msgSender, address txOrigin) external;
    function startPrank(address msgSender) external;
    function startPrank(address msgSender, address txOrigin) external;
    function stopPrank() external;
}

contract NativePrankTest {
    Vm constant vm = Vm(address(0x7109709ECfa91a80626fF3989D68f67F5b1DD12D));
    address constant alice = address(0xA11CE);
    address constant bob = address(0xB0B);

    function sender() external view returns (address) {
        return msg.sender;
    }

    function nestedSender() external view returns (address, address) {
        return (msg.sender, this.sender());
    }

    function origin() external view returns (address) {
        return tx.origin;
    }

    function nestedOrigin() external view returns (address, address) {
        return (tx.origin, this.origin());
    }

    function originThenRevert() external view {
        require(tx.origin == bob, "origin was not changed before revert");
        revert("expected");
    }

    function testPrankThroughNestedCall() public {
        vm.prank(alice);
        (address outer, address inner) = this.nestedSender();
        require(outer == alice, "outer call was not pranked");
        require(inner == address(this), "nested caller was changed");
        require(this.sender() == address(this), "prank survived outer return");
    }

    function testSinglePrank() public {
        vm.prank(alice);
        require(this.sender() == alice, "first call was not pranked");
        require(this.sender() == address(this), "prank leaked to second call");
    }

    function testPrankOriginThroughNestedCall() public {
        address original = tx.origin;
        vm.prank(alice, bob);
        (address outer, address inner) = this.nestedOrigin();
        require(outer == bob && inner == bob, "origin was not changed in nested call");
        require(this.origin() == original, "origin survived outer return");
    }

    function testPrankOriginAfterRevert() public {
        address original = tx.origin;
        vm.prank(alice, bob);
        try this.originThenRevert() {
            revert("child did not revert");
        } catch Error(string memory reason) {
            require(keccak256(bytes(reason)) == keccak256("expected"), "unexpected child revert");
        }
        require(this.origin() == original, "origin survived revert");
    }

    function testStartPrankOrigin() public {
        address original = tx.origin;
        vm.startPrank(alice, bob);
        require(this.origin() == bob, "origin was not changed");
        require(this.origin() == bob, "origin did not persist");
        vm.stopPrank();
        require(this.origin() == original, "origin survived stop");
    }

    function testStartAndStopPrank() public {
        vm.startPrank(alice);
        require(this.sender() == alice, "first call was not pranked");
        require(this.sender() == alice, "prank did not persist");
        vm.stopPrank();
        require(this.sender() == address(this), "prank survived stop");
    }
}
"#,
    );

    cmd.forge_fuse();
    cmd.arg("test").assert_success().stdout_eq(str![[r#"
[COMPILING_FILES] with [SOLC_VERSION]
[SOLC_VERSION] [ELAPSED]
Compiler run successful!

Ran 6 tests for test/NativePrank.t.sol:NativePrankTest
[PASS] testPrankOriginAfterRevert() ([GAS])
[PASS] testPrankOriginThroughNestedCall() ([GAS])
[PASS] testPrankThroughNestedCall() ([GAS])
[PASS] testSinglePrank() ([GAS])
[PASS] testStartAndStopPrank() ([GAS])
[PASS] testStartPrankOrigin() ([GAS])
Suite result: ok. 6 passed; 0 failed; 0 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 6 tests passed, 0 failed, 0 skipped (6 total tests)

"#]]);

    prj.update_config(|config| config.isolate = true);
    cmd.forge_fuse();
    cmd.arg("test").assert_success().stdout_eq(str![[r#"
No files changed, compilation skipped

Ran 6 tests for test/NativePrank.t.sol:NativePrankTest
[PASS] testPrankOriginAfterRevert() ([GAS])
[PASS] testPrankOriginThroughNestedCall() ([GAS])
[PASS] testPrankThroughNestedCall() ([GAS])
[PASS] testSinglePrank() ([GAS])
[PASS] testStartAndStopPrank() ([GAS])
[PASS] testStartPrankOrigin() ([GAS])
Suite result: ok. 6 passed; 0 failed; 0 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 6 tests passed, 0 failed, 0 skipped (6 total tests)

"#]]);
});

forgetest_init!(evm2_snapshot_restores_live_and_accepted_state, |prj, cmd| {
    prj.update_config(|config| config.isolate = false);
    prj.add_test(
        "NativeSnapshot.t.sol",
        r#"
interface Vm {
    function snapshotState() external returns (uint256);
    function revertToState(uint256 snapshotId) external returns (bool);
    function revertToStateAndDelete(uint256 snapshotId) external returns (bool);
    function warp(uint256 timestamp) external;
}

contract NativeSnapshotTest {
    Vm constant vm = Vm(address(0x7109709ECfa91a80626fF3989D68f67F5b1DD12D));
    uint256 public value;
    uint256 setupSnapshot;

    function setUp() public {
        value = 7;
        setupSnapshot = vm.snapshotState();
        value = 9;
    }

    function restoreAndRevert(uint256 id) external {
        require(vm.revertToState(id), "snapshot missing");
        revert("expected");
    }

    function parentRestoreThenRevert(uint256 id) external {
        value = 11;
        try this.restoreAndRevert(id) {
            revert("child did not revert");
        } catch Error(string memory reason) {
            require(keccak256(bytes(reason)) == keccak256("expected"), "unexpected child revert");
        }
        require(value == 9, "child did not leave restored state");
        revert("parent");
    }

    function testRestoresSetupSnapshot() public {
        uint256 id = setupSnapshot;
        require(value == 9, "setup write was not accepted");
        value = 11;
        require(vm.revertToState(id), "setup snapshot missing");
        require(value == 7, "accepted state was not restored");
    }

    function testNestedRestoreThenRevert() public {
        uint256 id = vm.snapshotState();
        value = 11;
        try this.restoreAndRevert(id) {
            revert("child did not revert");
        } catch Error(string memory reason) {
            require(keccak256(bytes(reason)) == keccak256("expected"), "unexpected child revert");
        }
        require(value == 9, "restored state was rolled back");
    }

    function testBothActiveFramesRevertAfterRestore() public {
        uint256 id = vm.snapshotState();
        try this.parentRestoreThenRevert(id) {
            revert("parent did not revert");
        } catch Error(string memory reason) {
            require(keccak256(bytes(reason)) == keccak256("parent"), "unexpected parent revert");
        }
        require(value == 9, "parent rollback damaged restored state");
    }

    function testRestoresBlockContext() public {
        uint256 timestamp = block.timestamp;
        uint256 id = vm.snapshotState();
        vm.warp(99);
        require(vm.revertToStateAndDelete(id), "snapshot missing");
        require(block.timestamp == timestamp, "block context was not restored");
        require(!vm.revertToState(id), "deleted snapshot survived");
    }
}
"#,
    );

    cmd.forge_fuse();
    cmd.arg("test").assert_success().stdout_eq(str![[r#"
[COMPILING_FILES] with [SOLC_VERSION]
[SOLC_VERSION] [ELAPSED]
Compiler run successful!

Ran 4 tests for test/NativeSnapshot.t.sol:NativeSnapshotTest
[PASS] testBothActiveFramesRevertAfterRestore() ([GAS])
[PASS] testNestedRestoreThenRevert() ([GAS])
[PASS] testRestoresBlockContext() ([GAS])
[PASS] testRestoresSetupSnapshot() ([GAS])
Suite result: ok. 4 passed; 0 failed; 0 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 4 tests passed, 0 failed, 0 skipped (4 total tests)

"#]]);

    prj.update_config(|config| config.isolate = true);
    cmd.forge_fuse();
    cmd.arg("test").assert_success().stdout_eq(str![[r#"
No files changed, compilation skipped

Ran 4 tests for test/NativeSnapshot.t.sol:NativeSnapshotTest
[PASS] testBothActiveFramesRevertAfterRestore() ([GAS])
[PASS] testNestedRestoreThenRevert() ([GAS])
[PASS] testRestoresBlockContext() ([GAS])
[PASS] testRestoresSetupSnapshot() ([GAS])
Suite result: ok. 4 passed; 0 failed; 0 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 4 tests passed, 0 failed, 0 skipped (4 total tests)

"#]]);
});
