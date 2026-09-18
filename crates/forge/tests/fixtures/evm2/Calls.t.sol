// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.24;
interface CallsVm {
    struct Log { bytes32[] topics; bytes data; address emitter; }
    function expectCall(address, bytes calldata) external;
    function expectCall(address, bytes calldata, uint64) external;
    function expectCall(address, uint256, uint64, bytes calldata) external;
    function expectCallMinGas(address, uint256, uint64, bytes calldata) external;
    function expectDelegateCall(address, bytes calldata) external;
    function expectRevert(bytes calldata) external;
    function _expectCheatcodeRevert(bytes calldata) external;
    function mockCall(address, bytes calldata, bytes calldata) external;
    function mockCall(address, uint256, bytes calldata, bytes calldata) external;
    function mockCalls(address, bytes calldata, bytes[] calldata) external;
    function mockCallRevert(address, bytes calldata, bytes calldata) external;
    function etch(address, bytes calldata) external;
    function clearMockedCalls() external;
    function recordLogs() external;
    function getRecordedLogs() external returns (Log[] memory);
}
contract CallsTarget {
    event Seen(uint256 indexed n);
    function actual(uint256 n) external payable returns (uint256) { emit Seen(n); return n; }
    function forward(address to, uint256 value) external returns (bool, bytes memory) {
        return to.call{value: value}("");
    }
    function fail() external { emit Seen(99); revert("target failed"); }
}
contract NativeCallsTest {
    CallsVm constant vm = CallsVm(address(uint160(uint256(keccak256("hevm cheat code")))));
    address constant EMPTY = address(0x123456);
    CallsTarget target;
    function setUp() public { target = new CallsTarget(); }
    function testCallPrefix() public {
        vm.expectCall(address(target), abi.encodePacked(target.actual.selector));
        require(target.actual(7) == 7);
    }
    function testCount() public {
        bytes memory data = abi.encodeCall(target.actual, (7));
        vm.expectCall(address(target), data, 2);
        target.actual(7); target.actual(7);
    }
    function testDelegate() public {
        bytes memory data = abi.encodeCall(target.actual, (7));
        vm.expectDelegateCall(address(target), data);
        (bool ok,) = address(target).delegatecall(data); require(ok);
    }
    function testExactGas() public {
        bytes memory data = abi.encodeCall(target.actual, (7));
        vm.expectCall(address(target), 0, 50000, data);
        (bool ok,) = address(target).call{gas: 50000}(data); require(ok);
    }
    function testValueStipend() public {
        bytes memory data = abi.encodeCall(target.actual, (7));
        vm.expectCall(address(target), 7, 50000, data);
        (bool ok,) = address(target).call{gas: 50000, value: 7}(data); require(ok);
        require(address(target).balance == 7);
    }
    function testMinGas() public {
        bytes memory data = abi.encodeCall(target.actual, (7));
        vm.expectCallMinGas(address(target), 0, 10000, data);
        (bool ok,) = address(target).call{gas: 50000}(data); require(ok);
    }
    function testCountedOverwriteRejected() public {
        bytes memory data = abi.encodeCall(target.actual, (7));
        vm.expectCall(address(target), data, 1);
        vm._expectCheatcodeRevert(bytes("counted expected calls can only bet set once"));
        vm.expectCall(address(target), data, 2);
        target.actual(7);
    }
    function testMockPriorityAndClear() public {
        bytes memory data = abi.encodeCall(target.actual, (7));
        vm.mockCall(address(target), abi.encodePacked(target.actual.selector), abi.encode(uint256(1)));
        vm.mockCall(address(target), data, abi.encode(uint256(2)));
        vm.expectCall(address(target), data);
        require(target.actual(7) == 2);
        require(target.actual(8) == 1);
        vm.clearMockedCalls(); require(target.actual(7) == 7);
    }
    function testMockQueue() public {
        bytes[] memory outputs = new bytes[](2); outputs[0] = abi.encode(uint256(1)); outputs[1] = abi.encode(uint256(2));
        vm.mockCalls(address(target), abi.encodePacked(target.actual.selector), outputs);
        require(target.actual(7) == 1); require(target.actual(7) == 2); require(target.actual(7) == 2);
    }
    function testMockEmptyAccount() public {
        vm.mockCall(EMPTY, bytes(""), abi.encode(uint256(42)));
        require(EMPTY.code.length == 1);
        require(CallsTarget(EMPTY).actual(0) == 42);
    }
    function testMockValue() public {
        vm.mockCall(EMPTY, bytes(""), abi.encode(uint256(1)));
        vm.mockCall(EMPTY, 7, bytes(""), abi.encode(uint256(2)));
        require(CallsTarget(EMPTY).actual{value: 7}(0) == 2);
        require(EMPTY.balance == 7);
    }
    function testMockRevertRollback() public {
        vm.mockCallRevert(EMPTY, bytes(""), abi.encodeWithSignature("Error(string)", "mocked"));
        vm.expectRevert(bytes("mocked"));
        CallsTarget(EMPTY).actual{value: 7}(0);
        require(EMPTY.balance == 0);
    }
    function registerAndRevert() external {
        vm.mockCall(EMPTY, bytes(""), abi.encode(uint256(42)));
        revert();
    }
    function testMockRegistrationSurvivesRevert() public {
        (bool ok,) = address(this).call(abi.encodeCall(this.registerAndRevert, ()));
        require(!ok && EMPTY.code.length == 0);
        bytes memory result;
        (ok, result) = EMPTY.call("");
        require(ok && abi.decode(result, (uint256)) == 42);
    }
    function testInsufficientFundsRetainsQueue() public {
        bytes[] memory outputs = new bytes[](2); outputs[0] = abi.encode(uint256(1)); outputs[1] = abi.encode(uint256(2));
        vm.mockCalls(EMPTY, bytes(""), outputs);
        (bool ok,) = target.forward(EMPTY, 7); require(!ok);
        target.actual{value: 7}(0);
        bytes memory result;
        (ok, result) = target.forward(EMPTY, 7);
        require(ok && abi.decode(result, (uint256)) == 1 && EMPTY.balance == 7);
    }
    function testRecordedRevertedLogs() public {
        vm.recordLogs();
        (bool ok,) = address(target).call(abi.encodeCall(target.fail, ()));
        require(!ok);
        CallsVm.Log[] memory logs = vm.getRecordedLogs();
        require(logs.length == 1 && logs[0].emitter == address(target));
        require(logs[0].topics[1] == bytes32(uint256(99)));
        require(vm.getRecordedLogs().length == 0);
        target.actual(7); require(vm.getRecordedLogs().length == 1);
    }
}
contract NativeCallsFailuresTest {
    CallsVm constant vm = CallsVm(address(uint160(uint256(keccak256("hevm cheat code")))));
    function testMissingCall() public { vm.expectCall(address(0x123456), hex"12345678"); }
    function testTooManyCalls() public {
        vm.mockCall(address(0x123456), bytes(""), bytes(""));
        vm.expectCall(address(0x123456), hex"12345678", 1);
        (bool a,) = address(0x123456).call(hex"12345678");
        (bool b,) = address(0x123456).call(hex"12345678"); require(a && b);
    }
    function testPreservesOriginalRevert() public {
        vm.expectCall(address(0x123456), hex"12345678"); revert("original");
    }
}

contract NativeMockSetupTest {
    CallsVm constant vm = CallsVm(address(uint160(uint256(keccak256("hevm cheat code")))));
    function setUp() public { vm.mockCall(address(0x123456), bytes(""), abi.encode(uint256(42))); }
    function testSetupMockPersists() public { require(CallsTarget(address(0x123456)).actual(0) == 42); }
}
// Reference obligation at Prague: the expectation names the delegatecall operand,
// not the account to which EIP-7702 resolves its bytecode.
contract DelegatedCallIdentityTest {
    CallsVm constant vm = CallsVm(address(uint160(uint256(keccak256("hevm cheat code")))));
    function testDelegatedOperandIdentity() public {
        CallsTarget implementation = new CallsTarget();
        address authority = address(0x123456);
        vm.etch(authority, abi.encodePacked(hex"ef0100", address(implementation)));
        bytes memory data = abi.encodeCall(implementation.actual, (7));
        vm.expectDelegateCall(authority, data);
        (bool ok, bytes memory result) = authority.delegatecall(data);
        require(ok && abi.decode(result, (uint256)) == 7);
    }
}
