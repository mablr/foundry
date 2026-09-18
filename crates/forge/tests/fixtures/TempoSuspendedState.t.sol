// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.24;

interface TempoStateVm {
    struct Log {
        bytes32[] topics;
        bytes data;
        address emitter;
    }
    function snapshotState() external returns (uint256);
    function revertToState(uint256 id) external returns (bool);
    function prank(address sender) external;
    function recordLogs() external;
    function getRecordedLogs() external returns (Log[] memory);
    function getEvmVersion() external view returns (string memory);
    function envString(string calldata name) external returns (string memory);
}

interface StateToken {
    function balanceOf(address owner) external view returns (uint256);
    function allowance(address owner, address spender) external view returns (uint256);
    function approve(address spender, uint256 amount) external returns (bool);
    function transfer(address to, uint256 amount) external returns (bool);
    function transferFrom(address from, address to, uint256 amount) external returns (bool);
}

contract TempoStateLeaf {
    function pull(StateToken token, address owner, address recipient, uint256 amount) external {
        require(token.transferFrom(owner, recipient, amount), "pull failed");
    }
}

contract TempoStateChild {
    TempoStateVm constant vm = TempoStateVm(address(uint160(uint256(keccak256("hevm cheat code")))));
    uint256 public value = 7;

    function run(StateToken token, TempoStateLeaf leaf, address recipient) external returns (bytes32) {
        uint256 beforeBalance = token.balanceOf(address(this));
        uint256 beforeRecipient = token.balanceOf(recipient);
        value = 10;
        uint256 snapshot = vm.snapshotState();
        require(token.approve(address(leaf), 100), "approve failed");
        leaf.pull(token, address(this), recipient, 50);
        value = 20;
        require(token.balanceOf(address(this)) == beforeBalance - 50, "native debit");
        require(token.allowance(address(this), address(leaf)) == 50, "native allowance consumed");
        require(vm.revertToState(snapshot), "restore failed");
        require(value == 10, "EVM storage restore");
        require(token.balanceOf(address(this)) == beforeBalance, "native balance restore");
        require(token.balanceOf(recipient) == beforeRecipient, "recipient restore");
        require(token.allowance(address(this), address(leaf)) == 0, "native allowance restore");
        require(token.approve(address(leaf), 25), "second approve failed");
        leaf.pull(token, address(this), recipient, 25);
        value = 30;
        return keccak256("tempo child resumed");
    }
}

contract TempoStateParent {
    uint256 public value = 5;
    error ParentReverted(bytes32 result);
    event Resumed(bytes32 indexed result);

    function run(StateToken token, TempoStateChild child, TempoStateLeaf leaf, address recipient, uint256 outcome)
        external
    {
        value = 11;
        bytes32 result = child.run(token, leaf, recipient);
        require(result == keccak256("tempo child resumed") && value == 11 && child.value() == 30, "continuation");
        value = 12;
        emit Resumed(result);
        if (outcome == 1) revert ParentReverted(result);
        if (outcome == 2) {
            assembly { invalid() }
        }
    }
}

contract TempoSuspendedStateTest {
    TempoStateVm constant vm = TempoStateVm(address(uint160(uint256(keccak256("hevm cheat code")))));
    StateToken constant token = StateToken(0x20C0000000000000000000000000000000000000);
    address constant recipient = address(0xBEEF);
    TempoStateParent parent;
    TempoStateChild child;
    TempoStateLeaf leaf;

    function setUp() public {
        require(
            keccak256(bytes(vm.getEvmVersion())) == keccak256(bytes(vm.envString("TEMPO_EXPECTED_HARDFORK"))),
            "wrong execution hardfork"
        );
        parent = new TempoStateParent();
        child = new TempoStateChild();
        leaf = new TempoStateLeaf();
        vm.prank(0x1804c8AB1F12E6bbf3894d4083f33e07309d1f38);
        require(token.transfer(address(child), 1000), "fund child");
        require(token.balanceOf(recipient) == 0, "recipient baseline");
    }

    function check(uint256 outcome) internal {
        vm.recordLogs();
        (bool success, bytes memory output) =
            address(parent).call{gas: 3_000_000}(abi.encodeCall(parent.run, (token, child, leaf, recipient, outcome)));
        TempoStateVm.Log[] memory logs = vm.getRecordedLogs();
        require(success == (outcome == 0), "outcome");
        if (outcome == 1) {
            require(
                keccak256(output)
                    == keccak256(
                        abi.encodeWithSelector(
                            TempoStateParent.ParentReverted.selector, keccak256("tempo child resumed")
                        )
                    ),
                "revert payload"
            );
        } else {
            require(output.length == 0, "unexpected output");
        }
        require(parent.value() == (success ? 12 : 5), "parent rollback");
        require(child.value() == (success ? 30 : 7), "child rollback");
        require(token.balanceOf(address(child)) == (success ? 975 : 1000), "native child settlement");
        require(token.balanceOf(recipient) == (success ? 25 : 0), "native recipient settlement");
        require(token.allowance(address(child), address(leaf)) == 0, "native allowance settlement");
        uint256 transfers;
        bool resumed;
        for (uint256 i; i < logs.length; ++i) {
            if (
                logs[i].emitter == address(token) && logs[i].topics[0] == keccak256("Transfer(address,address,uint256)")
            ) {
                require(logs[i].topics[1] == bytes32(uint256(uint160(address(child)))), "transfer owner");
                require(logs[i].topics[2] == bytes32(uint256(uint160(recipient))), "transfer recipient");
                require(abi.decode(logs[i].data, (uint256)) == (transfers == 0 ? 50 : 25), "transfer amount");
                ++transfers;
            }
            if (logs[i].emitter == address(parent) && logs[i].topics[0] == keccak256("Resumed(bytes32)")) {
                require(logs[i].topics[1] == keccak256("tempo child resumed"), "resumption payload");
                resumed = true;
            }
        }
        require(transfers == 2 && resumed, "diagnostics lost or parent not resumed");
        // Re-enter native precompiles after the completed or rolled-back ancestor.
        uint256 before = token.balanceOf(recipient);
        parent.run(token, child, leaf, recipient, 0);
        require(token.balanceOf(recipient) == before + 25, "subsequent native call");
    }

    function testTempoSnapshotParentSuccess() public {
        check(0);
    }

    function testTempoSnapshotParentRevert() public {
        check(1);
    }

    function testTempoSnapshotParentHalt() public {
        check(2);
    }
}
