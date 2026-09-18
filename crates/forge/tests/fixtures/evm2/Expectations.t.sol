// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.24;

interface ExpectVm {
    function expectRevert() external;
    function expectRevert(bytes calldata) external;
    function expectRevert(bytes4) external;
    function expectRevert(address) external;
    function expectRevert(bytes calldata, uint64) external;
    function expectPartialRevert(bytes4) external;
    function _expectCheatcodeRevert(bytes calldata) external;
    function assertEq(uint256, uint256) external;
}

contract RevertingConstructor {
    constructor() { revert("constructor"); }
}

contract NativeExpectationsTest {
    ExpectVm constant vm = ExpectVm(address(uint160(uint256(keccak256("hevm cheat code")))));
    error Custom(uint256 n);
    uint256 public value;
    uint256 public refundable = 1;

    function fail() external { value = 99; revert("expected"); }
    function clearAndRevert() external { refundable = 0; revert("expected"); }
    function failCustom() external pure { revert Custom(7); }
    function failEmpty() external pure { revert(); }
    function failInvalid() external pure { assembly { invalid() } }
    function testStringAndRollback() public {
        vm.expectRevert(bytes("expected"));
        this.fail();
        require(value == 0);
    }
    function testAnyRevert() public { vm.expectRevert(); this.failEmpty(); }
    function testHalt() public {
        uint256 beforeGas = gasleft();
        vm.expectRevert(); this.failInvalid();
        require(beforeGas - gasleft() < 10000, "expected halt burned unused gas");
    }
    function testRevertRefund() public {
        vm.expectRevert(bytes("expected")); this.clearAndRevert();
        require(refundable == 1);
    }
    function testConstructor2() public {
        vm.expectRevert(bytes("constructor"));
        new RevertingConstructor{salt: bytes32(uint256(1))}();
    }
    function testPartial() public { vm.expectPartialRevert(Custom.selector); this.failCustom(); }
    function testAddress() public { vm.expectRevert(address(this)); this.fail(); }
    function testCount() public {
        vm.expectRevert(bytes("expected"), 2);
        this.fail();
        this.fail();
    }
    function testConstructor() public {
        vm.expectRevert(bytes("constructor"));
        new RevertingConstructor();
    }
    function testCheatcode() public {
        vm._expectCheatcodeRevert(bytes("assertion failed: 1 != 2"));
        vm.assertEq(1, 2);
    }
    function testConsoleDoesNotConsume() public {
        vm.expectRevert();
        (bool ok,) = address(0x000000000000000000636F6e736F6c652e6c6f67).staticcall(
            abi.encodeWithSignature("log(string)", "between expectation and revert")
        );
        require(ok);
        this.fail();
    }
}

contract NativeExpectationFailuresTest is NativeExpectationsTest {
    function testMismatch() public { vm.expectRevert(bytes("wrong")); this.fail(); }
    function testDangling() public { vm.expectRevert(); }
    function testCountMissing() public {
        vm.expectRevert(bytes("expected"), 2);
        this.fail();
    }
    function testSuccessInstead() public { vm.expectRevert(); this.value(); }
}
