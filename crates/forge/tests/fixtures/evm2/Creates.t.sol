// SPDX-License-Identifier: MIT
pragma solidity >=0.8.18;
interface CreatesVm {
    function expectCreate(bytes calldata code, address deployer) external;
    function expectCreate2(bytes calldata code, address deployer) external;
    function prank(address sender) external;
    function expectRevert(address reverter, uint64 count) external;
    function computeCreate2Address(bytes32 salt, bytes32 hash, address deployer) external pure returns (address);
}
contract ExpectedProduct { function value() external pure returns (uint256) { return 7; } }
contract FailedProduct { constructor() { revert("constructor"); } }
contract ExpectedFactory {
    function deploy(bool salted, bool rollback) external {
        if (salted) new ExpectedProduct{salt: bytes32(uint256(1))}();
        else new ExpectedProduct();
        require(!rollback, "rollback");
    }
}
contract NativeCreatesTest {
    CreatesVm constant vm = CreatesVm(address(uint160(uint256(keccak256("hevm cheat code")))));
    function testCreateAndCreate2() public {
        vm.expectCreate(type(ExpectedProduct).runtimeCode, address(this));
        vm.expectCreate2(type(ExpectedProduct).runtimeCode, address(this));
        new ExpectedProduct();
        new ExpectedProduct{salt: bytes32(uint256(1))}();
    }
    function testNestedUnordered() public {
        ExpectedFactory factory = new ExpectedFactory();
        vm.expectCreate2(type(ExpectedProduct).runtimeCode, address(factory));
        vm.expectCreate(type(ExpectedProduct).runtimeCode, address(factory));
        factory.deploy(false, false);
        factory.deploy(true, false);
    }
    function testDuplicateExpectations() public {
        vm.expectCreate(type(ExpectedProduct).runtimeCode, address(this));
        vm.expectCreate(type(ExpectedProduct).runtimeCode, address(this));
        new ExpectedProduct(); new ExpectedProduct();
    }
    function testPrankedDeployer() public {
        vm.expectCreate(type(ExpectedProduct).runtimeCode, address(0x1234));
        vm.prank(address(0x1234));
        new ExpectedProduct();
    }
    function testMatchSurvivesEnclosingRollback() public {
        ExpectedFactory factory = new ExpectedFactory();
        vm.expectCreate(type(ExpectedProduct).runtimeCode, address(factory));
        try factory.deploy(false, true) { revert("expected rollback"); } catch {}
    }
}
contract NativeCreatesFailureTest {
    CreatesVm constant vm = CreatesVm(address(uint160(uint256(keccak256("hevm cheat code")))));
    function testMissing() public { vm.expectCreate(hex"00", address(this)); }
    function testWrongScheme() public {
        vm.expectCreate2(hex"00", address(this));
        new ExpectedProduct();
    }
    function testCollisionDoesNotMatch() public {
        new ExpectedProduct{salt: bytes32(uint256(1))}();
        vm.expectCreate2(type(ExpectedProduct).runtimeCode, address(this));
        try new ExpectedProduct{salt: bytes32(uint256(1))}() { revert("expected collision"); } catch {}
    }
    function testOriginalFailure() public {
        vm.expectCreate(hex"00", address(this));
        revert("original");
    }
}
contract FailedCreateIdentityTest {
    CreatesVm constant vm = CreatesVm(address(uint160(uint256(keccak256("hevm cheat code")))));
    function testConstructorRevertEmptyCode() public {
        vm.expectCreate(hex"", address(this));
        try new FailedProduct() { revert("expected constructor failure"); } catch {}
    }
}

contract NestedFailedProduct {
    constructor() { new FailedProduct{salt: bytes32(uint256(2))}(); }
}
contract NativeCreateRevertersTest {
    CreatesVm constant vm = CreatesVm(address(uint160(uint256(keccak256("hevm cheat code")))));
    function expectedInner() internal view returns (address) {
        address outer = vm.computeCreate2Address(bytes32(uint256(1)), keccak256(type(NestedFailedProduct).creationCode), address(this));
        return vm.computeCreate2Address(bytes32(uint256(2)), keccak256(type(FailedProduct).creationCode), outer);
    }
    function testCountedNestedReverter() public {
        vm.expectRevert(expectedInner(), 2);
        new NestedFailedProduct{salt: bytes32(uint256(1))}();
        new NestedFailedProduct{salt: bytes32(uint256(1))}();
    }
    function testSingleNestedReverter() public {
        vm.expectRevert(expectedInner(), 1);
        new NestedFailedProduct{salt: bytes32(uint256(1))}();
    }
}
contract NativeCreateReverterFailureTest is NativeCreateRevertersTest {
    function testRejectsDifferentSecondReverter() public {
        vm.expectRevert(expectedInner(), 2);
        new NestedFailedProduct{salt: bytes32(uint256(1))}();
        new FailedProduct();
    }
}
