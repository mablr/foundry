// SPDX-License-Identifier: MIT OR Apache-2.0
// Reference-proven obligation; native deal/setNonce remain explicitly unsupported.
pragma solidity ^0.8.24;
interface VmProbe {
    function deal(address, uint256) external;
    function setNonce(address, uint64) external;
    function getNonce(address) external returns (uint64);
}
contract MutationProbeTest {
    VmProbe constant vm = VmProbe(address(uint160(uint256(keccak256("hevm cheat code")))));
    address constant target = address(0x123456);
    function changeAndRevert() external {
        vm.deal(target, 99);
        vm.setNonce(target, 8);
        revert();
    }
    function testUnjournaledAccountMutations() public {
        vm.deal(target, 11);
        vm.setNonce(target, 7);
        (bool ok,) = address(this).call(abi.encodeCall(this.changeAndRevert, ()));
        require(!ok);
        require(target.balance == 99, "deal was reverted");
        require(vm.getNonce(target) == 8, "nonce was reverted");
    }
}
