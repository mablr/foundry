// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.24;

interface BenchVm {
    function store(address, bytes32, bytes32) external;
    function load(address, bytes32) external view returns (bytes32);
}

contract ExecutionBenchTest {
    BenchVm constant vm = BenchVm(address(uint160(uint256(keccak256("hevm cheat code")))));

    function testCompute() public pure {
        bytes32 value = bytes32(uint256(1));
        for (uint256 i; i < 50000; ++i) {
            assembly {
                mstore(0, value)
                value := keccak256(0, 32)
            }
        }
        require(value != bytes32(0));
    }

    function testCheatcodeReads() public {
        vm.store(address(this), bytes32(0), bytes32(uint256(42)));
        uint256 sum;
        for (uint256 i; i < 1000; ++i) {
            sum += uint256(vm.load(address(this), bytes32(0)));
        }
        require(sum == 42000);
    }
}
