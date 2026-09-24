//! End-to-end checks for the experimental evm2 Forge runner.

use foundry_test_utils::{forgetest_init, str};

forgetest_init!(evm2_runs_compiled_setup_and_unit_test, |prj, cmd| {
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
    cmd.env("FOUNDRY_EVM2_NATIVE", "1");
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
    cmd.env("FOUNDRY_EVM2_NATIVE", "1");
    cmd.args(["test", "--match-test", "testBroken"]).assert_failure().stdout_eq(str![[r#"
No files changed, compilation skipped

Ran 1 test for test/Native.t.sol:NativeTest
[FAIL: EVM execution stopped: Revert] testBroken() ([GAS])
Suite result: FAILED. 0 passed; 1 failed; 0 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 0 tests passed, 1 failed, 0 skipped (1 total tests)
...
"#]]);

    cmd.forge_fuse();
    cmd.env("FOUNDRY_EVM2_NATIVE", "1");
    cmd.args(["test", "--match-test", "testFuzz"]).assert_failure().stderr_eq(str![[
        r#"Error: native execution does not yet support fuzz tests

"#
    ]]);
});
