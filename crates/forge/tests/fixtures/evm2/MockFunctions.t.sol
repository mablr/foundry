// SPDX-License-Identifier: MIT
pragma solidity ^0.8.18;

interface MockFunctionVm {
    function mockFunction(address callee, address target, bytes calldata data) external;
    function clearMockedCalls() external;
    function mockCall(address callee, bytes calldata data, bytes calldata output) external;
    function expectCall(address callee, bytes calldata data) external;
    function prank(address caller) external;
}

contract FunctionModel {
    uint256 public stored;
    uint256 immutable offset;

    constructor(uint256 value) { offset = value; }

    function write(uint256 value) external payable returns (address, address, uint256, uint256) {
        stored = value + offset;
        return (address(this), msg.sender, msg.value, stored);
    }

    function fail() external {
        stored = 99;
        revert("model failure");
    }
}

contract NativeMockFunctionsTest {
    MockFunctionVm constant vm = MockFunctionVm(address(uint160(uint256(keccak256("hevm cheat code")))));
    FunctionModel original;
    FunctionModel model;
    FunctionModel alternate;

    function setUp() public {
        original = new FunctionModel(1);
        model = new FunctionModel(10);
        alternate = new FunctionModel(100);
        vm.mockFunction(address(original), address(model), abi.encodePacked(FunctionModel.write.selector));
    }

    function testSetupStorageAndIdentity() public {
        (address self, address caller, uint256 value, uint256 result) = original.write{value: 7}(2);
        require(self == address(original) && caller == address(this) && value == 7 && result == 12);
        require(original.stored() == 12 && model.stored() == 0);
        require(address(original).balance == 7 && address(model).balance == 0);
    }

    function testExactPrecedesSelector() public {
        vm.mockFunction(address(original), address(alternate), abi.encodeCall(FunctionModel.write, (2)));
        original.write(2);
        require(original.stored() == 102);
        original.write(3);
        require(original.stored() == 13);
    }

    function testClearRetainsFunctionAndSelfResets() public {
        vm.clearMockedCalls();
        original.write(2);
        require(original.stored() == 12);
        vm.mockFunction(address(original), address(original), abi.encodePacked(FunctionModel.write.selector));
        original.write(2);
        require(original.stored() == 3);
    }

    function testRedirectBeforeExpectationsPrankAndMocks() public {
        bytes memory input = abi.encodeCall(FunctionModel.write, (2));
        vm.expectCall(address(model), input);
        vm.prank(address(123));
        (, address caller,,) = original.write(2);
        require(caller == address(123));
        vm.mockCall(address(model), input, abi.encode(address(1), address(2), uint256(3), uint256(4)));
        (address self,,,uint256 value) = original.write(2);
        require(self == address(1) && value == 4 && original.stored() == 12);
    }

    function testRedirectRevertRollsBack() public {
        vm.mockFunction(address(original), address(model), abi.encodePacked(FunctionModel.fail.selector));
        (bool ok, bytes memory output) = address(original).call(abi.encodeCall(FunctionModel.fail, ()));
        require(!ok && keccak256(output) == keccak256(abi.encodeWithSignature("Error(string)", "model failure")));
        require(original.stored() == 0 && model.stored() == 0);
    }

    function testRedirectPreservesStatic() public {
        (bool ok,) = address(original).staticcall(abi.encodeCall(FunctionModel.write, (2)));
        require(!ok && original.stored() == 0);
    }

    function testOnlyExactOrFourByteSelector() public {
        vm.mockFunction(address(original), address(original), abi.encodePacked(FunctionModel.write.selector));
        bytes memory input = abi.encodeCall(FunctionModel.write, (2));
        bytes memory prefix = new bytes(5);
        for (uint256 i; i < 5; ++i) prefix[i] = input[i];
        vm.mockFunction(address(original), address(model), prefix);
        original.write(2);
        require(original.stored() == 3);
    }

    function testRegistrationSurvivesChildRevert() public {
        (bool ok,) = address(this).call(abi.encodeCall(this.registerAndRevert, ()));
        require(!ok);
        original.write(2);
        require(original.stored() == 102);
    }

    function registerAndRevert() external {
        vm.mockFunction(address(original), address(alternate), abi.encodePacked(FunctionModel.write.selector));
        revert("registered");
    }
}
