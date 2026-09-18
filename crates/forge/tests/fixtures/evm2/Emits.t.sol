// SPDX-License-Identifier: MIT
pragma solidity >=0.8.18;
interface EmitsVm {
    function expectRevert(bytes calldata reason) external;
    function expectEmit() external;
    function expectEmit(address emitter) external;
    function expectEmit(uint64 count) external;
    function expectEmit(address emitter, uint64 count) external;
    function expectEmit(bool, bool, bool, bool) external;
    function expectEmit(bool, bool, bool, bool, address) external;
    function expectEmit(bool, bool, bool, bool, uint64) external;
    function expectEmit(bool, bool, bool, bool, address, uint64) external;
    function expectEmitAnonymous() external;
    function expectEmitAnonymous(address) external;
    function expectEmitAnonymous(bool, bool, bool, bool, bool) external;
    function expectEmitAnonymous(bool, bool, bool, bool, bool, address) external;
}
contract NativeEmitsTest {
    EmitsVm constant vm = EmitsVm(address(uint160(uint256(keccak256("hevm cheat code")))));
    event E(uint256 indexed key, uint256 value);
    event A(uint256 indexed key, uint256 value) anonymous;
    event Z(uint256 value) anonymous;
    uint256 public value;
    function fire(uint256 key, uint256 val, uint256 count) external {
        for (uint256 i; i<count; ++i) emit E(key, val);
    }
    function anon() external { emit A(1, 2); }
    function zeroViolation() external { value = 1; emit E(1, 2); value = 2; }
    function emitAndRevert() external { value = 1; emit E(1, 2); revert("child"); }
    function writeWrongEvent() external { value = 1; emit E(1, 3); }
    function testRevertedChildLogStillMatches() public {
        vm.expectEmit(); emit E(1, 2);
        (bool ok,) = address(this).call(abi.encodeCall(this.emitAndRevert, ()));
        require(!ok && value == 0);
    }
    function testCaughtEndMismatchKeepsSettledState() public {
        vm.expectEmit(); emit E(1, 2);
        (bool ok,) = address(this).call(abi.encodeCall(this.writeWrongEvent, ()));
        require(!ok && value == 1);
        this.fire(1, 2, 1);
    }
    function testZeroCountExpectedRevert() public {
        vm.expectEmit(uint64(0)); emit E(1, 2);
        vm.expectRevert(bytes("log emitted but expected 0 times"));
        this.zeroViolation();
        require(value == 0);
    }
    function testDefaultAndEmitter() public {
        vm.expectEmit(); emit E(1, 2); this.fire(1, 2, 1);
        vm.expectEmit(address(this)); emit E(1, 2); this.fire(1, 2, 1);
    }
    function testCounts() public {
        vm.expectEmit(uint64(2)); emit E(1, 2); this.fire(1, 2, 2);
        vm.expectEmit(address(this), uint64(2)); emit E(1, 2); this.fire(1, 2, 2);
    }
    function testFlagsAndCountFlags() public {
        vm.expectEmit(false, false, false, true); emit E(1, 2); this.fire(99, 2, 1);
        vm.expectEmit(true, false, false, false, address(this)); emit E(1, 2); this.fire(1, 99, 1);
        vm.expectEmit(false, false, false, true, uint64(2)); emit E(1, 2); this.fire(99, 2, 2);
        vm.expectEmit(true, false, false, false, address(this), uint64(2)); emit E(1, 2); this.fire(1, 99, 2);
    }
    function testAnonymousOverloads() public {
        vm.expectEmitAnonymous(); emit A(1, 2); this.anon();
        vm.expectEmitAnonymous(address(this)); emit A(1, 2); this.anon();
        vm.expectEmitAnonymous(true, false, false, false, true); emit A(1, 2); this.anon();
        vm.expectEmitAnonymous(true, false, false, false, true, address(this)); emit A(1, 2); this.anon();
    }
    function testZeroCountRollsBackAndStopsFrame() public {
        vm.expectEmit(uint64(0)); emit E(1, 2);
        (bool ok, bytes memory reason) = address(this).call(abi.encodeCall(this.zeroViolation, ()));
        require(!ok && value == 0);
        require(keccak256(reason) == keccak256(abi.encodeWithSignature("CheatcodeError(string)", "log emitted but expected 0 times")));
    }
}
contract NativeEmitsFailureTest is NativeEmitsTest {
    function testMissingEmit() public { vm.expectEmit(); emit E(1, 2); this.fire(1, 2, 0); }
    function testWrongData() public { vm.expectEmit(); emit E(1, 2); this.fire(1, 3, 1); }
    function testWrongEmitter() public { vm.expectEmit(address(0x1234)); emit E(1, 2); this.fire(1, 2, 1); }
    function testZeroCountViolation() public { vm.expectEmit(uint64(0)); emit E(1, 2); this.fire(1, 2, 1); }
    function testAnonymousTemplateRejected() public { vm.expectEmit(); emit Z(2); }
    function testOriginalFailure() public { vm.expectEmit(); emit E(1, 2); revert("original"); }
}
