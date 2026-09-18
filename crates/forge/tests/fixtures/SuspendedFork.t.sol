// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.24;

interface SuspendedForkVm {
    function envString(string calldata name) external returns (string memory);
    function createFork(string calldata url) external returns (uint256);
    function selectFork(uint256 id) external;
    function activeFork() external view returns (uint256);
    function makePersistent(address account) external;
    function etch(address account, bytes calldata code) external;
}

contract ForkCell {
    uint256 public value;

    function write(uint256 next) external {
        value = next;
    }
}

contract SuspendedForkChild {
    SuspendedForkVm constant vm = SuspendedForkVm(address(uint160(uint256(keccak256("hevm cheat code")))));

    function run(ForkCell cell, uint256 a, uint256 b) external returns (bytes32) {
        require(vm.activeFork() == a && cell.value() == 11, "initial A");
        cell.write(111);
        vm.selectFork(b);
        require(cell.value() == 22, "initial B");
        cell.write(222);
        vm.selectFork(a);
        require(cell.value() == 111, "resumed A");
        return keccak256("fork child resumed");
    }
}

contract SuspendedForkParent {
    uint256 public value;
    error ParentReverted(bytes32 childResult);

    function run(SuspendedForkChild child, ForkCell cell, uint256 a, uint256 b, bool fail) external {
        value = 1;
        bytes32 result = child.run(cell, a, b);
        require(result == keccak256("fork child resumed") && value == 1, "parent continuation");
        value = 2;
        if (fail) revert ParentReverted(result);
    }
}

// Executed by the local-node CLI harness, not by the remote testdata sweep.
/// forge-config: default.evm_version = "cancun"
contract SuspendedForkTest {
    SuspendedForkVm constant vm = SuspendedForkVm(address(uint160(uint256(keccak256("hevm cheat code")))));
    SuspendedForkParent parent;
    SuspendedForkChild child;
    ForkCell cell;
    uint256 a;
    uint256 b;

    function setUp() public {
        a = vm.createFork(vm.envString("SUSPENDED_FORK_RPC_A"));
        b = vm.createFork(vm.envString("SUSPENDED_FORK_RPC_B"));
        vm.selectFork(a);
        parent = new SuspendedForkParent();
        child = new SuspendedForkChild();
        cell = new ForkCell();
        vm.makePersistent(address(parent));
        vm.makePersistent(address(child));
        cell.write(11);
        bytes memory code = address(cell).code;
        vm.selectFork(b);
        vm.etch(address(cell), code);
        cell.write(22);
        vm.selectFork(a);
    }

    function check(bool fail) internal {
        (bool success, bytes memory result) =
            address(parent).call(abi.encodeCall(parent.run, (child, cell, a, b, fail)));
        require(success == !fail, "outcome");
        if (fail) {
            require(
                keccak256(result)
                    == keccak256(
                        abi.encodeWithSelector(
                            SuspendedForkParent.ParentReverted.selector, keccak256("fork child resumed")
                        )
                    ),
                "revert payload"
            );
        }
        require(vm.activeFork() == a, "active identity");
        require(parent.value() == (fail ? 0 : 2), "parent settlement");
        require(cell.value() == (fail ? 11 : 111), "A settlement");
        vm.selectFork(b);
        require(cell.value() == 222, "source B publication");
        vm.selectFork(a);
        cell.write(33);
        require(cell.value() == 33, "subsequent call");
    }

    function testForkSuspendedParentSuccess() public {
        check(false);
    }

    function testForkSuspendedParentRevert() public {
        check(true);
    }
}
