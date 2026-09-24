//! End-to-end checks for the experimental evm2 Forge runner.

use foundry_test_utils::{forgetest_init, str};

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
    cmd.env("FOUNDRY_EVM2_NATIVE", "1");
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
    cmd.env("FOUNDRY_EVM2_NATIVE", "1");
    cmd.args(["test", "--match-test", "testLibrary"]).assert_success();

    prj.update_config(|config| config.create2_deployer = alloy_primitives::Address::ZERO);
    cmd.forge_fuse();
    cmd.env("FOUNDRY_EVM2_NATIVE", "1");
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
    cmd.env("FOUNDRY_EVM2_NATIVE", "1");
    cmd.args(["test", "--match-test", "testFactoryInstalled"]).assert_success();
});

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
[FAIL: expected failure] testBroken() ([GAS])
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
