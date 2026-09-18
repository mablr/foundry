// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.24;

import "utils/Test.sol";

contract SnapshotLeaf {
    uint256 public value = 9;

    function write() external {
        value = 99;
        assembly { tstore(0, 99) }
    }

    function transientValue() external view returns (uint256 result) {
        assembly { result := tload(0) }
    }
}

contract SnapshotChild is Test {
    uint256 public value = 7;
    event Phase(uint256 indexed phase);

    function probe() public view returns (uint256 used, uint256 result) {
        assembly {
            let before := gas()
            result := sload(100)
            used := sub(before, gas())
        }
    }

    function run(SnapshotLeaf leaf) external returns (bytes32) {
        value = 10;
        assembly { tstore(0, 10) }
        vm.coolSlot(address(this), bytes32(uint256(100)));
        uint256 snapshot = vm.snapshotState();
        (uint256 coldBefore,) = probe();
        value = 20;
        assembly { tstore(0, 20) }
        leaf.write();
        emit Phase(1);
        require(vm.revertToState(snapshot), "restore failed");
        (uint256 coldAfter, uint256 slotValue) = probe();
        (uint256 warmAfter,) = probe();
        require(coldBefore == coldAfter && coldAfter - warmAfter == 2000, "warmth not restored");
        require(slotValue == 0 && value == 10 && leaf.value() == 9, "storage not restored");
        uint256 restoredTransient;
        assembly { restoredTransient := tload(0) }
        require(restoredTransient == 10 && leaf.transientValue() == 0, "transient not restored");
        value = 30;
        assembly { tstore(0, 30) }
        emit Phase(2);
        return keccak256("child resumed");
    }

    function transientValue() external view returns (uint256 result) {
        assembly { result := tload(0) }
    }
}

contract SnapshotParent {
    uint256 public value = 5;
    error ParentReverted(bytes32 childResult);
    event Resumed(bytes32 indexed childResult);

    function run(SnapshotChild child, SnapshotLeaf leaf, uint256 outcome) external returns (bytes32) {
        value = 11;
        bytes32 result = child.run(leaf);
        require(result == keccak256("child resumed") && value == 11, "parent continuation");
        require(child.value() == 30 && child.transientValue() == 30, "child settlement");
        value = 12;
        emit Resumed(result);
        if (outcome == 1) revert ParentReverted(result);
        if (outcome == 2) {
            assembly { invalid() }
        }
        return result;
    }
}

/// Snapshot replacement occurs with both the test and parent frames suspended.
abstract contract SuspendedSnapshotTestBase is Test {
    function isolated() internal pure virtual returns (bool);
    SnapshotParent parent;
    SnapshotChild child;
    SnapshotLeaf leaf;

    function setUp() public {
        parent = new SnapshotParent();
        child = new SnapshotChild();
        leaf = new SnapshotLeaf();
    }

    function check(uint256 outcome) internal {
        vm.recordLogs();
        (bool success, bytes memory result) =
            address(parent).call{gas: 1_000_000}(abi.encodeCall(parent.run, (child, leaf, outcome)));
        Vm.Log[] memory logs = vm.getRecordedLogs();
        assertEq(success, outcome == 0);
        if (outcome == 0) assertEq(abi.decode(result, (bytes32)), keccak256("child resumed"));
        if (outcome == 1) {
            assertEq(result, abi.encodeWithSelector(SnapshotParent.ParentReverted.selector, keccak256("child resumed")));
        }
        if (outcome == 2) assertEq(result.length, 0);
        assertEq(parent.value(), outcome == 0 ? 12 : 5);
        assertEq(child.value(), outcome == 0 ? 30 : 7);
        assertEq(child.transientValue(), outcome == 0 && !isolated() ? 30 : 0);
        assertEq(leaf.value(), 9);
        assertEq(leaf.transientValue(), 0);
        // Inspector diagnostics retain logs even across snapshot and enclosing rollback.
        require(logs.length == 3, "diagnostic log count");
        for (uint256 i; i < 2; ++i) {
            assertEq(logs[i].emitter, address(child));
            assertEq(logs[i].topics[0], keccak256("Phase(uint256)"));
            assertEq(logs[i].topics[1], bytes32(i + 1));
        }
        // Prove the halt case reached the parent continuation before INVALID.
        assertEq(logs[2].emitter, address(parent));
        assertEq(logs[2].topics[0], keccak256("Resumed(bytes32)"));
        assertEq(logs[2].topics[1], keccak256("child resumed"));
        // A later call must work with the restored journal after ancestor settlement.
        assertEq(parent.run(child, leaf, 0), keccak256("child resumed"));
        assertEq(parent.value(), 12);
    }

    function testSuspendedSnapshotParentSuccess() public {
        check(0);
    }

    function testSuspendedSnapshotParentRevert() public {
        check(1);
    }

    function testSuspendedSnapshotParentHalt() public {
        check(2);
    }
}

/// forge-config: default.evm_version = "cancun"
/// forge-config: default.isolate = false
contract SuspendedSnapshotTest is SuspendedSnapshotTestBase {
    function isolated() internal pure override returns (bool) {
        return false;
    }
}

/// forge-config: default.evm_version = "cancun"
/// forge-config: default.isolate = true
contract SuspendedSnapshotIsolatedTest is SuspendedSnapshotTestBase {
    function isolated() internal pure override returns (bool) {
        return true;
    }
}
