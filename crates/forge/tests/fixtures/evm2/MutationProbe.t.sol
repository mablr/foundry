// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;
interface MutationVm {
    function deal(address, uint256) external;
    function setNonceUnsafe(address, uint64) external;
    function getNonce(address) external view returns (uint64);
}
contract MutationProbeTest {
    MutationVm constant vm = MutationVm(address(uint160(uint256(keccak256("hevm cheat code")))));
    address constant target = address(0x777777);
    function mutate() external {
        vm.deal(target, 9);
        vm.setNonceUnsafe(target, 9);
        revert("child");
    }
    function testDirectAccountMutationSurvivesChildRevert() public {
        vm.deal(target, 7);
        vm.setNonceUnsafe(target, 7);
        (bool ok,) = address(this).call(abi.encodeCall(this.mutate, ()));
        require(!ok);
        require(target.balance == 9, "balance");
        require(vm.getNonce(target) == 9, "nonce");
    }
}
