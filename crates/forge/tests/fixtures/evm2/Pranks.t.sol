// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.24;

interface PrankVm {
    function prank(address) external;
    function prank(address, bool) external;
    function prank(address, address) external;
    function startPrank(address) external;
    function stopPrank() external;
    function readCallers() external returns (uint8, address, address);
    function getNonce(address) external returns (uint64);
    function expectRevert(bytes calldata) external;
    function _expectCheatcodeRevert(bytes calldata) external;
}
contract PrankTarget {
    address public creator;
    constructor() { creator = msg.sender; }
    function sender() external view returns (address) { return msg.sender; }
    function fail() external pure { revert("child"); }
    function relay(PrankTarget other) external view returns (address) { return other.sender(); }
    function delegated() external view returns (address, address) { return (address(this), msg.sender); }
}
contract NativePranksTest {
    PrankVm constant vm = PrankVm(address(uint160(uint256(keccak256("hevm cheat code")))));
    address constant ALICE = address(0x123456);
    PrankTarget target;
    function setUp() public { target = new PrankTarget(); }
    function testOneShot() public {
        vm.prank(ALICE);
        vm.getNonce(ALICE);
        require(target.sender() == ALICE);
        require(target.sender() == address(this));
    }
    function testReadCallers() public {
        vm.prank(ALICE);
        (uint8 mode, address sender, address origin) = vm.readCallers();
        require(mode == 3 && sender == ALICE && origin == tx.origin);
        require(target.sender() == ALICE);
        (mode, sender, origin) = vm.readCallers();
        require(mode == 0 && sender == tx.origin && origin == tx.origin);
    }
    function testDelegateFromEOARejected() public {
        vm._expectCheatcodeRevert(bytes("cannot `prank` delegate call from an EOA"));
        vm.prank(ALICE, true);
        require(target.sender() == address(this));
    }
    function testPersistent() public {
        vm.startPrank(ALICE);
        require(target.sender() == ALICE);
        require(target.sender() == ALICE);
        vm.stopPrank();
        require(target.sender() == address(this));
    }
    function testNested() public {
        vm.prank(ALICE);
        require(target.relay(target) == address(target));
        require(target.sender() == address(this));
    }
    function testRevertCleanup() public {
        vm.prank(ALICE);
        vm.expectRevert(bytes("child"));
        target.fail();
        require(target.sender() == address(this));
    }
    function testCreate() public {
        vm.prank(ALICE);
        PrankTarget created = new PrankTarget();
        require(created.creator() == ALICE);
        require(vm.getNonce(ALICE) == 1);
        require(address(created) == address(uint160(uint256(keccak256(abi.encodePacked(hex"d694", ALICE, hex"80"))))));
        require(target.sender() == address(this));
    }
    function testCreate2() public {
        bytes32 salt = bytes32(uint256(7));
        address expected = address(uint160(uint256(keccak256(abi.encodePacked(bytes1(0xff), ALICE, salt, keccak256(type(PrankTarget).creationCode))))));
        vm.prank(ALICE);
        PrankTarget created = new PrankTarget{salt: salt}();
        require(address(created) == expected);
        require(created.creator() == ALICE);
    }
    function testDelegate() public {
        vm.prank(address(target), true);
        (bool ok, bytes memory data) = address(target).delegatecall(abi.encodeCall(target.delegated, ()));
        require(ok);
        (address context, address sender) = abi.decode(data, (address, address));
        require(context == address(target) && sender == address(target));
    }
    function testCannotOverwriteUnused() public {
        vm.prank(ALICE);
        vm._expectCheatcodeRevert(bytes("cannot overwrite a prank until it is applied at least once"));
        vm.prank(address(0x234567));
        require(target.sender() == ALICE);
    }
}
contract NativePrankSetupTest {
    PrankVm constant vm = PrankVm(address(uint160(uint256(keccak256("hevm cheat code")))));
    PrankTarget target;
    function setUp() public { target = new PrankTarget(); vm.startPrank(address(0x123456)); }
    function testSetupPrankPersists() public { require(target.sender() == address(0x123456)); }
}
contract NativePrankOriginUnsupportedTest {
    PrankVm constant vm = PrankVm(address(uint160(uint256(keccak256("hevm cheat code")))));
    function testCaughtOriginOverrideFails() public {
        try vm.prank(address(1), address(2)) {} catch {}
    }
}
