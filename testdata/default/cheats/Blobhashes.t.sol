// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.25;

import "utils/Test.sol";

contract BlobhashesTest is Test {
    function testSetAndGetBlobhashes() public {
        bytes32[] memory blobhashes = new bytes32[](2);
        blobhashes[0] = bytes32(0x0000000000000000000000000000000000000000000000000000000000000001);
        blobhashes[1] = bytes32(0x0000000000000000000000000000000000000000000000000000000000000002);
        vm.blobhashes(blobhashes);

        bytes32[] memory gotBlobhashes = vm.getBlobhashes();
        assertEq(gotBlobhashes[0], blobhashes[0]);
        assertEq(gotBlobhashes[1], blobhashes[1]);
    }
}

/// `vm.blobhashes` must remain visible to a *called* contract under
/// `--isolate`, where the synthetic inner transaction would otherwise be
/// rejected (left over EIP-4844 type + zero gas price) and `BLOBHASH` would
/// return zero. Regression test for #7277.
/// forge-config: default.isolate = true
contract IsolatedBlobhashesTest is Test {
    BlobhashRecorder internal recorder;

    function setUp() public {
        recorder = new BlobhashRecorder();
    }

    function test_blobhashes_visible_in_called_contract() public {
        bytes32[] memory hashes = new bytes32[](2);
        hashes[0] = bytes32(uint256(0xdeadbeef));
        hashes[1] = bytes32(uint256(0xcafebabe));
        vm.blobhashes(hashes);

        recorder.record();

        assertEq(recorder.hash(0), hashes[0]);
        assertEq(recorder.hash(1), hashes[1]);
    }
}

contract BlobhashRecorder {
    mapping(uint256 => bytes32) public hash;

    function record() external {
        hash[0] = blobhash(0);
        hash[1] = blobhash(1);
    }
}
