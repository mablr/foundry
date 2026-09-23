// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.24;

interface BasicVm {
    function warp(uint256) external;
    function roll(uint256) external;
    function fee(uint256) external;
    function coinbase(address) external;
    function prevrandao(bytes32) external;
    function getBlockTimestamp() external view returns (uint256);
    function getBlockNumber() external view returns (uint256);
    function load(address, bytes32) external view returns (bytes32);
    function store(address, bytes32, bytes32) external;
    function etch(address, bytes calldata) external;
    function getNonce(address) external view returns (uint64);
    function assertEq(uint256, uint256) external;
}

contract BasicTest {
    BasicVm constant vm = BasicVm(address(uint160(uint256(keccak256("hevm cheat code")))));
    address constant target = address(0x12345);

    function setUp() public {
        vm.warp(1234);
        vm.roll(42);
        vm.store(target, bytes32(0), bytes32(uint256(77)));
        vm.etch(target, hex"602a60005260206000f3");
    }

    function testEnvironment() public {
        vm.assertEq(vm.getBlockTimestamp(), 1234);
        vm.assertEq(vm.getBlockNumber(), 42);
        vm.warp(2345);
        vm.assertEq(vm.getBlockTimestamp(), 2345);
        vm.fee(17);
        vm.coinbase(address(0x555));
        vm.prevrandao(bytes32(uint256(99)));
        require(block.basefee == 17 && block.coinbase == address(0x555) && block.prevrandao == 99);
    }

    function testCaughtErrors() public {
        (bool ok, bytes memory error) = address(vm).call(abi.encodeCall(vm.store, (address(1), bytes32(0), bytes32(0))));
        require(!ok);
        require(keccak256(error) == keccak256(abi.encodeWithSignature("CheatcodeError(string)", "vm.store: cannot use precompile 0x0000000000000000000000000000000000000001 as an argument")));
        (ok, error) = address(vm).call(abi.encodeWithSelector(bytes4(0x12345678)));
        require(!ok);
        require(keccak256(error) == keccak256(abi.encodeWithSignature("CheatcodeError(string)", "unknown cheatcode with selector 0x12345678; you may have a mismatch between the `Vm` interface (likely in `forge-std`) and the `forge` version")));
    }

    function testState() public {
        vm.assertEq(uint256(vm.load(target, bytes32(0))), 77);
        vm.assertEq(vm.getNonce(address(this)), 1);
        (bool ok, bytes memory result) = target.call("");
        require(ok && abi.decode(result, (uint256)) == 42);
        vm.store(target, bytes32(0), bytes32(uint256(88)));
        vm.assertEq(uint256(vm.load(target, bytes32(0))), 88);
    }

    function mutateAndRevert() external {
        vm.store(target, bytes32(0), bytes32(uint256(99)));
        revert("rollback");
    }

    function testStorageRollback() public {
        (bool ok,) = address(this).call(abi.encodeCall(this.mutateAndRevert, ()));
        require(!ok);
        vm.assertEq(uint256(vm.load(target, bytes32(0))), 77);
    }
}

contract BasicFailureTest {
    BasicVm constant vm = BasicVm(address(uint160(uint256(keccak256("hevm cheat code")))));
    function testAssertionFails() public { vm.assertEq(1, 2); }
}

contract BasicConsoleTest {
    BasicVm constant vm = BasicVm(address(uint160(uint256(keccak256("hevm cheat code")))));
    event log_string(string);
    function testConsole() public {
        emit log_string("before");
        (bool ok,) = address(0x000000000000000000636F6e736F6c652e6c6f67).staticcall(abi.encodeWithSignature("log(string)", "native console"));
        require(ok);
        emit log_string("after");
    }
    function testAssertionLogOrder() public {
        emit log_string("before assertion");
        vm.assertEq(1, 2);
        emit log_string("after assertion");
    }
}
