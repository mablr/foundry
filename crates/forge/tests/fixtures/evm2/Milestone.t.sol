// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.24;

contract Child {
    uint256 public value;
    constructor(uint256 initial) payable { value = initial; }
    function write(uint256 next) public returns (uint256) { value = next; return value; }
    function writeThenRevert() public { value = 99; revert("child rollback"); }
    function writeThenHalt() public { value = 99; assembly { invalid() } }
    function transientWrite(uint256 next) public { assembly { tstore(0, next) } }
    function transientRead() public view returns (uint256 result) { assembly { result := tload(0) } }
}

contract MilestoneTest {
    uint256 public value;
    Child public child;
    address public immutable deployedBy;
    event Changed(uint256 value);

    constructor() { value = 7; deployedBy = msg.sender; }

    function setUp() public {
        require(value == 7, "constructor storage");
        require(deployedBy != address(0), "constructor caller");
        value = 11;
        child = new Child(22);
        child.transientWrite(88);
    }

    function testSetupAndStorageOnlyCommit() public {
        require(value == 11, "setup storage");
        require(child.value() == 22, "child constructor storage");
        require(address(child).code.length > 0, "child runtime code");
        value = 100;
        require(child.write(33) == 33, "return data");
        require(child.value() == 33, "child storage");
        emit Changed(33);
    }

    function testIndependentSetup() public {
        require(value == 11 && child.value() == 22, "test state leaked");
        value = 200;
        child.write(44);
    }

    function testNestedRevert() public {
        (bool ok, bytes memory reason) = address(child).call(abi.encodeCall(Child.writeThenRevert, ()));
        require(!ok, "expected revert");
        require(keccak256(reason) == keccak256(abi.encodeWithSignature("Error(string)", "child rollback")), "revert data");
        require(child.value() == 22, "reverted child write");
        require(child.write(34) == 34, "parent resume");
    }

    function testNestedHalt() public {
        (bool ok, bytes memory reason) = address(child).call{gas: 100000}(abi.encodeCall(Child.writeThenHalt, ()));
        require(!ok && reason.length == 0, "expected halt");
        require(child.value() == 22, "halted child write");
        child.write(35);
    }

    function testNestedCreateAndCreate2() public {
        Child first = new Child{value: 1}(31);
        Child second = new Child{salt: bytes32(uint256(42))}(32);
        require(first.value() == 31 && second.value() == 32, "created state");
        require(address(first).balance == 1, "creation value");
        require(address(first) != address(second) && address(first) != address(child), "create nonce");
    }

    function testTransientResetBetweenTransactions() public {
        require(child.transientRead() == 0, "setup transient state leaked");
        child.transientWrite(66);
        require(child.transientRead() == 66, "transaction transient state");
    }

    function testStaticCallRejectsWrite() public {
        (bool ok,) = address(child).staticcall(abi.encodeCall(Child.write, (55)));
        require(!ok && child.value() == 22, "static write escaped");
    }
}

contract MilestoneFailureTest {
    function testRevert() public pure { revert("milestone failure"); }
    function testPanic() public pure { assert(false); }
    function testHalt() public pure { assembly { invalid() } }
}

contract MilestoneUnsupportedTest {
    function testCaughtCheatcodeStillFails() public {
        address vm = address(uint160(uint256(keccak256("hevm cheat code"))));
        (bool ok,) = vm.call(abi.encodeWithSignature("snapshotState()"));
        // Catching the unsupported call must not turn this into a passing test.
        require(!ok, "unexpected success");
    }
}
