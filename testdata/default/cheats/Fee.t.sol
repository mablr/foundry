// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.18;

import "utils/Test.sol";

contract FeeTest is Test {
    function testFee() public {
        vm.fee(10);
        assertEq(block.basefee, 10, "fee failed");
    }

    function testFeeFuzzed(uint64 fee) public {
        vm.fee(fee);
        assertEq(block.basefee, fee, "fee failed");
    }
}

/// `vm.fee` must remain visible to a *called* contract under `--isolate`,
/// where Foundry zeroes `block.basefee` for the synthetic inner transaction
/// used for fee accounting. Regression test for #7277.
/// forge-config: default.isolate = true
contract IsolatedFeeTest is Test {
    BaseFeeRecorder internal recorder;

    function setUp() public {
        recorder = new BaseFeeRecorder();
    }

    function test_fee_visible_in_called_contract() public {
        vm.fee(456 gwei);
        recorder.record();
        assertEq(recorder.lastBaseFee(), 456 gwei);
    }
}

contract BaseFeeRecorder {
    uint256 public lastBaseFee;

    function record() external {
        lastBaseFee = block.basefee;
    }
}
