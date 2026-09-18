// SPDX-License-Identifier: MIT
pragma solidity >=0.8.18;

interface StatelessVm {
    function toString(uint256 value) external pure returns (string memory);
    function parseUint(string calldata value) external pure returns (uint256);
    function toBase64(bytes calldata value) external pure returns (string memory);
    function toBase64URL(bytes calldata value) external pure returns (string memory);
    function computeCreateAddress(address deployer, uint256 nonce) external pure returns (address);
    function computeCreate2Address(bytes32 salt, bytes32 initCodeHash, address deployer) external pure returns (address);
    function toRlp(bytes[] calldata values) external pure returns (bytes memory);
    function fromRlp(bytes calldata value) external pure returns (bytes[] memory);
    function foundryVersionAtLeast(string calldata version) external pure returns (bool);
    function publicKeyP256(uint256 key) external pure returns (uint256, uint256);
    function _expectCheatcodeRevert() external;
}

contract NativeStatelessTest {
    StatelessVm constant vm = StatelessVm(address(uint160(uint256(keccak256("hevm cheat code")))));

    function testStringBoundaryAndError() public {
        require(vm.parseUint(vm.toString(type(uint256).max)) == type(uint256).max);
        vm._expectCheatcodeRevert();
        vm.parseUint("-1");
    }

    function testBase64Alphabets() public pure {
        require(keccak256(bytes(vm.toBase64(hex"fbff"))) == keccak256(bytes("+/8=")));
        require(keccak256(bytes(vm.toBase64URL(hex"fbff"))) == keccak256(bytes("-_8=")));
    }

    function testCreateAddressesAndOverflow() public {
        address deployer = address(0x1234);
        require(vm.computeCreateAddress(deployer, 0) == address(uint160(uint256(keccak256(abi.encodePacked(hex"d694", deployer, hex"80"))))));
        bytes32 salt = bytes32(uint256(1));
        bytes32 codeHash = keccak256(hex"00");
        require(vm.computeCreate2Address(salt, codeHash, deployer) == address(uint160(uint256(keccak256(abi.encodePacked(hex"ff", deployer, salt, codeHash))))));
        vm._expectCheatcodeRevert();
        vm.computeCreateAddress(deployer, uint256(type(uint64).max) + 1);
    }

    function testRlpRoundTripAndError() public {
        bytes[] memory values = new bytes[](2);
        values[0] = hex"";
        values[1] = hex"1234";
        bytes memory encoded = vm.toRlp(values);
        require(keccak256(encoded) == keccak256(hex"c480821234"));
        bytes[] memory decoded = vm.fromRlp(encoded);
        require(decoded.length == 2 && decoded[0].length == 0 && keccak256(decoded[1]) == keccak256(values[1]));
        vm._expectCheatcodeRevert();
        vm.fromRlp(hex"ff");
    }

    function testCryptoAndVersionErrors() public {
        (uint256 x, uint256 y) = vm.publicKeyP256(1);
        require(x == 0x6b17d1f2e12c4247f8bce6e563a440f277037d812deb33a0f4a13945d898c296);
        require(y == 0x4fe342e2fe1a7f9b8ee7eb4a7c0f9e162bce33576b315ececbb6406837bf51f5);
        require(vm.foundryVersionAtLeast("0.0.1"));
        vm._expectCheatcodeRevert();
        vm.foundryVersionAtLeast("1.0.0-beta");
    }

    function testUnknownSelectorDiagnostic() public {
        (bool ok, bytes memory reason) = address(vm).call(hex"deadbeef");
        require(!ok);
        bytes memory expected = abi.encodeWithSignature("CheatcodeError(string)", "unknown cheatcode with selector 0xdeadbeef; you may have a mismatch between the `Vm` interface (likely in `forge-std`) and the `forge` version");
        require(keccak256(reason) == keccak256(expected));
    }
}
