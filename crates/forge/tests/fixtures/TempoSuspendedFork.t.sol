// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.24;

import "./TempoSuspendedState.t.sol";

interface TempoForkVm {
    function envString(string calldata name) external returns (string memory);
    function createFork(string calldata url) external returns (uint256);
    function selectFork(uint256 id) external;
    function activeFork() external view returns (uint256);
    function makePersistent(address account) external;
    function prank(address sender) external;
}

contract TempoForkChild {
    TempoForkVm constant vm = TempoForkVm(address(uint160(uint256(keccak256("hevm cheat code")))));
    StateToken constant token = StateToken(0x20C0000000000000000000000000000000000000);
    address constant owner = address(0xCA);
    address constant recipient = address(0xBEEF);

    function run(uint256 a, uint256 b) external returns (bytes32) {
        require(vm.activeFork() == a && token.balanceOf(owner) == 1000, "initial A");
        vm.prank(owner);
        require(token.transfer(recipient, 111), "A transfer");
        vm.selectFork(b);
        require(token.balanceOf(owner) == 1000 && token.balanceOf(recipient) == 0, "initial B");
        vm.prank(owner);
        require(token.transfer(recipient, 222), "B transfer");
        vm.selectFork(a);
        require(token.balanceOf(owner) == 889 && token.balanceOf(recipient) == 111, "resumed A");
        return keccak256("native fork child resumed");
    }
}

contract TempoForkParent {
    uint256 public value = 5;
    error ParentReverted(bytes32 result);

    function run(TempoForkChild child, uint256 a, uint256 b, bool fail) external {
        value = 11;
        bytes32 result = child.run(a, b);
        require(value == 11 && result == keccak256("native fork child resumed"), "continuation");
        value = 12;
        if (fail) revert ParentReverted(result);
    }
}

contract TempoSuspendedForkTest {
    TempoForkVm constant vm = TempoForkVm(address(uint160(uint256(keccak256("hevm cheat code")))));
    StateToken constant token = StateToken(0x20C0000000000000000000000000000000000000);
    address constant owner = address(0xCA);
    address constant recipient = address(0xBEEF);
    TempoForkParent parent;
    TempoForkChild child;
    uint256 a;
    uint256 b;

    function setUp() public {
        a = vm.createFork(vm.envString("TEMPO_STATE_RPC_A"));
        b = vm.createFork(vm.envString("TEMPO_STATE_RPC_B"));
        vm.selectFork(a);
        parent = new TempoForkParent();
        child = new TempoForkChild();
        vm.makePersistent(address(parent));
        vm.makePersistent(address(child));
    }

    function check(bool fail) internal {
        (bool success, bytes memory result) = address(parent).call(abi.encodeCall(parent.run, (child, a, b, fail)));
        require(success == !fail, "outcome");
        if (fail) {
            require(
                keccak256(result)
                    == keccak256(
                        abi.encodeWithSelector(
                            TempoForkParent.ParentReverted.selector, keccak256("native fork child resumed")
                        )
                    ),
                "revert payload"
            );
        }
        require(vm.activeFork() == a, "active identity");
        require(parent.value() == (fail ? 5 : 12), "parent rollback");
        require(token.balanceOf(owner) == (fail ? 1000 : 889), "A owner settlement");
        require(token.balanceOf(recipient) == (fail ? 0 : 111), "A recipient settlement");
        vm.selectFork(b);
        require(token.balanceOf(owner) == 778 && token.balanceOf(recipient) == 222, "B publication");
        vm.selectFork(a);
        uint256 before = token.balanceOf(recipient);
        vm.prank(owner);
        require(token.transfer(recipient, 1), "subsequent transfer");
        require(token.balanceOf(recipient) == before + 1, "subsequent balance");
    }

    function testForkTempoSuspendedParentSuccess() public {
        check(false);
    }

    function testForkTempoSuspendedParentRevert() public {
        check(true);
    }
}
