// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.24;

interface NativeVm {
    function warp(uint256) external;
    function roll(uint256) external;
    function fee(uint256) external;
    function coinbase(address) external;
    function getBlockTimestamp() external view returns (uint256);
    function getBlockNumber() external view returns (uint256);
    function load(address, bytes32) external view returns (bytes32);
    function store(address, bytes32, bytes32) external;
    function etch(address, bytes calldata) external;
    function getNonce(address) external view returns (uint64);
    function assertTrue(bool) external pure;
    function assertEq(uint256, uint256) external pure;
    function assertEq(string calldata, string calldata) external pure;
    function assertEq(uint256[] calldata, uint256[] calldata) external pure;
    function assertApproxEqAbs(uint256, uint256, uint256) external pure;
}

contract NativeCheatcodesTest {
    NativeVm constant vm = NativeVm(address(uint160(uint256(keccak256("hevm cheat code")))));
    uint256 value;
    address constant TARGET = address(0x123456);

    function setUp() public {
        vm.warp(123);
        vm.fee(42);
        vm.store(address(this), bytes32(0), bytes32(uint256(11)));
        vm.etch(TARGET, hex"602a60005260206000f3");
    }

    function testSetupPersistence() public {
        vm.assertEq(vm.getBlockTimestamp(), 123);
        vm.assertEq(block.basefee, 42);
        vm.assertEq(value, 11);
        (bool ok, bytes memory data) = TARGET.staticcall("");
        vm.assertTrue(ok);
        vm.assertEq(abi.decode(data, (uint256)), 42);
        vm.warp(999);
        vm.store(address(this), bytes32(0), bytes32(uint256(99)));
    }

    function testIndependentSetup() public {
        vm.assertEq(vm.getBlockTimestamp(), 123);
        vm.assertEq(value, 11);
    }

    function mutateAndRevert() external {
        vm.store(address(this), bytes32(0), bytes32(uint256(77)));
        vm.warp(456);
        revert("rollback");
    }

    function testEnclosingRevert() public {
        (bool ok,) = address(this).call(abi.encodeCall(this.mutateAndRevert, ()));
        require(!ok);
        vm.assertEq(value, 11);
        // Environment mutations are not journaled and survive a descendant revert.
        vm.assertEq(vm.getBlockTimestamp(), 456);
    }

    function testStorageAndEnvironment() public {
        vm.store(TARGET, bytes32(uint256(8)), bytes32(uint256(19)));
        vm.assertEq(uint256(vm.load(TARGET, bytes32(uint256(8)))), 19);
        vm.roll(500);
        vm.assertEq(vm.getBlockNumber(), 500);
        vm.coinbase(TARGET);
        require(block.coinbase == TARGET);
        vm.assertTrue(vm.getNonce(address(this)) > 0);
    }

    function etchAndRevert() external {
        vm.etch(TARGET, hex"602b60005260206000f3");
        revert("rollback code");
    }

    function testEnclosingCodeRevert() public {
        bytes32 beforeHash = TARGET.codehash;
        (bool ok,) = address(this).call(abi.encodeCall(this.etchAndRevert, ()));
        require(!ok);
        require(TARGET.codehash == beforeHash, "code rollback");
    }

    function testMalformedCodeRejected() public {
        bytes32 beforeHash = TARGET.codehash;
        (bool ok,) = address(vm).call(abi.encodeCall(vm.etch, (TARGET, hex"ef01")));
        require(!ok);
        require(TARGET.codehash == beforeHash, "failed etch changed code");
    }

    function testAssertionOverloads() public {
        vm.assertEq("hello", "hello");
        uint256[] memory values = new uint256[](2);
        values[0] = 4;
        values[1] = 8;
        vm.assertEq(values, values);
        vm.assertApproxEqAbs(100, 101, 1);
    }

    function testAssertionRevertPayload() public {
        (bool ok, bytes memory data) = address(vm).call(abi.encodeWithSignature("assertEq(uint256,uint256)", 1, 2));
        require(!ok);
        require(keccak256(data) == keccak256(abi.encodeWithSignature("CheatcodeError(string)", "assertion failed: 1 != 2")));
    }

    function testPrecompileWriteRejected() public {
        (bool ok, bytes memory data) = address(vm).call(abi.encodeCall(vm.store, (address(1), bytes32(0), bytes32(uint256(1)))));
        require(!ok);
        require(keccak256(data) == keccak256(abi.encodeWithSignature("CheatcodeError(string)", "vm.store: cannot use precompile 0x0000000000000000000000000000000000000001 as an argument")));
    }
}

contract NativeAssertionFailureTest {
    NativeVm constant vm = NativeVm(address(uint160(uint256(keccak256("hevm cheat code")))));
    function testAssertionFailure() public pure { vm.assertEq(1, 2); }
}
