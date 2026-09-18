// SPDX-License-Identifier: MIT
pragma solidity >=0.8.18;

interface ParserVm {
    function _expectCheatcodeRevert() external;
    function _expectCheatcodeRevert(bytes4 revertData) external;
    function _expectCheatcodeRevert(bytes calldata revertData) external;
    function keyExists(string calldata json, string calldata key) external view returns (bool);
    function keyExistsJson(string calldata json, string calldata key) external view returns (bool);
    function keyExistsToml(string calldata toml, string calldata key) external view returns (bool);
    function parseJsonAddressArray(string calldata json, string calldata key) external pure returns (address[] memory);
    function parseJsonAddressArray(string calldata json, string calldata key, address[] calldata defaultValue) external pure returns (address[] memory);
    function parseJsonAddress(string calldata json, string calldata key) external pure returns (address);
    function parseJsonAddress(string calldata json, string calldata key, address defaultValue) external pure returns (address);
    function parseJsonArrayLength(string calldata json, string calldata key) external pure returns (uint256 length);
    function parseJsonBoolArray(string calldata json, string calldata key) external pure returns (bool[] memory);
    function parseJsonBoolArray(string calldata json, string calldata key, bool[] calldata defaultValue) external pure returns (bool[] memory);
    function parseJsonBool(string calldata json, string calldata key) external pure returns (bool);
    function parseJsonBool(string calldata json, string calldata key, bool defaultValue) external pure returns (bool);
    function parseJsonBytes32Array(string calldata json, string calldata key) external pure returns (bytes32[] memory);
    function parseJsonBytes32Array(string calldata json, string calldata key, bytes32[] calldata defaultValue) external pure returns (bytes32[] memory);
    function parseJsonBytes32(string calldata json, string calldata key) external pure returns (bytes32);
    function parseJsonBytes32(string calldata json, string calldata key, bytes32 defaultValue) external pure returns (bytes32);
    function parseJsonBytesArray(string calldata json, string calldata key) external pure returns (bytes[] memory);
    function parseJsonBytesArray(string calldata json, string calldata key, bytes[] calldata defaultValue) external pure returns (bytes[] memory);
    function parseJsonBytes(string calldata json, string calldata key) external pure returns (bytes memory);
    function parseJsonBytes(string calldata json, string calldata key, bytes calldata defaultValue) external pure returns (bytes memory);
    function parseJsonIntArray(string calldata json, string calldata key) external pure returns (int256[] memory);
    function parseJsonIntArray(string calldata json, string calldata key, int256[] calldata defaultValue) external pure returns (int256[] memory);
    function parseJsonInt(string calldata json, string calldata key) external pure returns (int256);
    function parseJsonInt(string calldata json, string calldata key, int256 defaultValue) external pure returns (int256);
    function parseJsonKeys(string calldata json, string calldata key) external pure returns (string[] memory keys);
    function parseJsonStringArray(string calldata json, string calldata key) external pure returns (string[] memory);
    function parseJsonStringArray(string calldata json, string calldata key, string[] calldata defaultValue) external pure returns (string[] memory);
    function parseJsonString(string calldata json, string calldata key) external pure returns (string memory);
    function parseJsonString(string calldata json, string calldata key, string calldata defaultValue) external pure returns (string memory);
    function parseJsonUintArray(string calldata json, string calldata key) external pure returns (uint256[] memory);
    function parseJsonUintArray(string calldata json, string calldata key, uint256[] calldata defaultValue) external pure returns (uint256[] memory);
    function parseJsonUint(string calldata json, string calldata key) external pure returns (uint256);
    function parseJsonUint(string calldata json, string calldata key, uint256 defaultValue) external pure returns (uint256);
    function parseTomlAddressArray(string calldata toml, string calldata key) external pure returns (address[] memory);
    function parseTomlAddressArray(string calldata toml, string calldata key, address[] calldata defaultValue) external pure returns (address[] memory);
    function parseTomlAddress(string calldata toml, string calldata key) external pure returns (address);
    function parseTomlAddress(string calldata toml, string calldata key, address defaultValue) external pure returns (address);
    function parseTomlBoolArray(string calldata toml, string calldata key) external pure returns (bool[] memory);
    function parseTomlBoolArray(string calldata toml, string calldata key, bool[] calldata defaultValue) external pure returns (bool[] memory);
    function parseTomlBool(string calldata toml, string calldata key) external pure returns (bool);
    function parseTomlBool(string calldata toml, string calldata key, bool defaultValue) external pure returns (bool);
    function parseTomlBytes32Array(string calldata toml, string calldata key) external pure returns (bytes32[] memory);
    function parseTomlBytes32Array(string calldata toml, string calldata key, bytes32[] calldata defaultValue) external pure returns (bytes32[] memory);
    function parseTomlBytes32(string calldata toml, string calldata key) external pure returns (bytes32);
    function parseTomlBytes32(string calldata toml, string calldata key, bytes32 defaultValue) external pure returns (bytes32);
    function parseTomlBytesArray(string calldata toml, string calldata key) external pure returns (bytes[] memory);
    function parseTomlBytesArray(string calldata toml, string calldata key, bytes[] calldata defaultValue) external pure returns (bytes[] memory);
    function parseTomlBytes(string calldata toml, string calldata key) external pure returns (bytes memory);
    function parseTomlBytes(string calldata toml, string calldata key, bytes calldata defaultValue) external pure returns (bytes memory);
    function parseTomlIntArray(string calldata toml, string calldata key) external pure returns (int256[] memory);
    function parseTomlIntArray(string calldata toml, string calldata key, int256[] calldata defaultValue) external pure returns (int256[] memory);
    function parseTomlInt(string calldata toml, string calldata key) external pure returns (int256);
    function parseTomlInt(string calldata toml, string calldata key, int256 defaultValue) external pure returns (int256);
    function parseTomlKeys(string calldata toml, string calldata key) external pure returns (string[] memory keys);
    function parseTomlStringArray(string calldata toml, string calldata key) external pure returns (string[] memory);
    function parseTomlStringArray(string calldata toml, string calldata key, string[] calldata defaultValue) external pure returns (string[] memory);
    function parseTomlString(string calldata toml, string calldata key) external pure returns (string memory);
    function parseTomlString(string calldata toml, string calldata key, string calldata defaultValue) external pure returns (string memory);
    function parseTomlUintArray(string calldata toml, string calldata key) external pure returns (uint256[] memory);
    function parseTomlUintArray(string calldata toml, string calldata key, uint256[] calldata defaultValue) external pure returns (uint256[] memory);
    function parseTomlUint(string calldata toml, string calldata key) external pure returns (uint256);
    function parseTomlUint(string calldata toml, string calldata key, uint256 defaultValue) external pure returns (uint256);
}

contract NativeParserTest {
    ParserVm constant vm = ParserVm(address(uint160(uint256(keccak256("hevm cheat code")))));
    function testJsonUint() public pure {
        string memory document = "{\"value\":42,\"values\":[42]}";
        require(keccak256(abi.encode(vm.parseJsonUint(document, ".value"))) == keccak256(abi.encode(uint256(42))));
        require(keccak256(abi.encode(vm.parseJsonUint(document, ".value", 0))) == keccak256(abi.encode(uint256(42))));
        require(keccak256(abi.encode(vm.parseJsonUint(document, ".missing", 42))) == keccak256(abi.encode(uint256(42))));
        uint256[] memory values = vm.parseJsonUintArray(document, ".values");
        require(values.length == 1 && keccak256(abi.encode(values[0])) == keccak256(abi.encode(uint256(42))));
        require(keccak256(abi.encode(vm.parseJsonUintArray(document, ".values", new uint256[](0)))) == keccak256(abi.encode(values)));
        require(keccak256(abi.encode(vm.parseJsonUintArray(document, ".missing", values))) == keccak256(abi.encode(values)));
    }
    function testJsonInt() public pure {
        string memory document = "{\"value\":-7,\"values\":[-7]}";
        require(keccak256(abi.encode(vm.parseJsonInt(document, ".value"))) == keccak256(abi.encode(int256(-7))));
        require(keccak256(abi.encode(vm.parseJsonInt(document, ".value", 0))) == keccak256(abi.encode(int256(-7))));
        require(keccak256(abi.encode(vm.parseJsonInt(document, ".missing", -7))) == keccak256(abi.encode(int256(-7))));
        int256[] memory values = vm.parseJsonIntArray(document, ".values");
        require(values.length == 1 && keccak256(abi.encode(values[0])) == keccak256(abi.encode(int256(-7))));
        require(keccak256(abi.encode(vm.parseJsonIntArray(document, ".values", new int256[](0)))) == keccak256(abi.encode(values)));
        require(keccak256(abi.encode(vm.parseJsonIntArray(document, ".missing", values))) == keccak256(abi.encode(values)));
    }
    function testJsonBool() public pure {
        string memory document = "{\"value\":true,\"values\":[true]}";
        require(keccak256(abi.encode(vm.parseJsonBool(document, ".value"))) == keccak256(abi.encode(bool(true))));
        require(keccak256(abi.encode(vm.parseJsonBool(document, ".value", false))) == keccak256(abi.encode(bool(true))));
        require(keccak256(abi.encode(vm.parseJsonBool(document, ".missing", true))) == keccak256(abi.encode(bool(true))));
        bool[] memory values = vm.parseJsonBoolArray(document, ".values");
        require(values.length == 1 && keccak256(abi.encode(values[0])) == keccak256(abi.encode(bool(true))));
        require(keccak256(abi.encode(vm.parseJsonBoolArray(document, ".values", new bool[](0)))) == keccak256(abi.encode(values)));
        require(keccak256(abi.encode(vm.parseJsonBoolArray(document, ".missing", values))) == keccak256(abi.encode(values)));
    }
    function testJsonAddress() public pure {
        string memory document = "{\"value\":\"0x0000000000000000000000000000000000001234\",\"values\":[\"0x0000000000000000000000000000000000001234\"]}";
        require(keccak256(abi.encode(vm.parseJsonAddress(document, ".value"))) == keccak256(abi.encode(address(address(0x1234)))));
        require(keccak256(abi.encode(vm.parseJsonAddress(document, ".value", address(0)))) == keccak256(abi.encode(address(address(0x1234)))));
        require(keccak256(abi.encode(vm.parseJsonAddress(document, ".missing", address(0x1234)))) == keccak256(abi.encode(address(address(0x1234)))));
        address[] memory values = vm.parseJsonAddressArray(document, ".values");
        require(values.length == 1 && keccak256(abi.encode(values[0])) == keccak256(abi.encode(address(address(0x1234)))));
        require(keccak256(abi.encode(vm.parseJsonAddressArray(document, ".values", new address[](0)))) == keccak256(abi.encode(values)));
        require(keccak256(abi.encode(vm.parseJsonAddressArray(document, ".missing", values))) == keccak256(abi.encode(values)));
    }
    function testJsonString() public pure {
        string memory document = "{\"value\":\"hello\",\"values\":[\"hello\"]}";
        require(keccak256(abi.encode(vm.parseJsonString(document, ".value"))) == keccak256(abi.encode(string("hello"))));
        require(keccak256(abi.encode(vm.parseJsonString(document, ".value", ""))) == keccak256(abi.encode(string("hello"))));
        require(keccak256(abi.encode(vm.parseJsonString(document, ".missing", "hello"))) == keccak256(abi.encode(string("hello"))));
        string[] memory values = vm.parseJsonStringArray(document, ".values");
        require(values.length == 1 && keccak256(abi.encode(values[0])) == keccak256(abi.encode(string("hello"))));
        require(keccak256(abi.encode(vm.parseJsonStringArray(document, ".values", new string[](0)))) == keccak256(abi.encode(values)));
        require(keccak256(abi.encode(vm.parseJsonStringArray(document, ".missing", values))) == keccak256(abi.encode(values)));
    }
    function testJsonBytes() public pure {
        string memory document = "{\"value\":\"0x1234\",\"values\":[\"0x1234\"]}";
        require(keccak256(abi.encode(vm.parseJsonBytes(document, ".value"))) == keccak256(abi.encode(bytes(hex"1234"))));
        require(keccak256(abi.encode(vm.parseJsonBytes(document, ".value", hex""))) == keccak256(abi.encode(bytes(hex"1234"))));
        require(keccak256(abi.encode(vm.parseJsonBytes(document, ".missing", hex"1234"))) == keccak256(abi.encode(bytes(hex"1234"))));
        bytes[] memory values = vm.parseJsonBytesArray(document, ".values");
        require(values.length == 1 && keccak256(abi.encode(values[0])) == keccak256(abi.encode(bytes(hex"1234"))));
        require(keccak256(abi.encode(vm.parseJsonBytesArray(document, ".values", new bytes[](0)))) == keccak256(abi.encode(values)));
        require(keccak256(abi.encode(vm.parseJsonBytesArray(document, ".missing", values))) == keccak256(abi.encode(values)));
    }
    function testJsonBytes32() public pure {
        string memory document = "{\"value\":\"0x0000000000000000000000000000000000000000000000000000000000001234\",\"values\":[\"0x0000000000000000000000000000000000000000000000000000000000001234\"]}";
        require(keccak256(abi.encode(vm.parseJsonBytes32(document, ".value"))) == keccak256(abi.encode(bytes32(bytes32(uint256(0x1234))))));
        require(keccak256(abi.encode(vm.parseJsonBytes32(document, ".value", bytes32(0)))) == keccak256(abi.encode(bytes32(bytes32(uint256(0x1234))))));
        require(keccak256(abi.encode(vm.parseJsonBytes32(document, ".missing", bytes32(uint256(0x1234))))) == keccak256(abi.encode(bytes32(bytes32(uint256(0x1234))))));
        bytes32[] memory values = vm.parseJsonBytes32Array(document, ".values");
        require(values.length == 1 && keccak256(abi.encode(values[0])) == keccak256(abi.encode(bytes32(bytes32(uint256(0x1234))))));
        require(keccak256(abi.encode(vm.parseJsonBytes32Array(document, ".values", new bytes32[](0)))) == keccak256(abi.encode(values)));
        require(keccak256(abi.encode(vm.parseJsonBytes32Array(document, ".missing", values))) == keccak256(abi.encode(values)));
    }
    function testTomlUint() public pure {
        string memory document = "value = 42\nvalues = [42]\n";
        require(keccak256(abi.encode(vm.parseTomlUint(document, ".value"))) == keccak256(abi.encode(uint256(42))));
        require(keccak256(abi.encode(vm.parseTomlUint(document, ".value", 0))) == keccak256(abi.encode(uint256(42))));
        require(keccak256(abi.encode(vm.parseTomlUint(document, ".missing", 42))) == keccak256(abi.encode(uint256(42))));
        uint256[] memory values = vm.parseTomlUintArray(document, ".values");
        require(values.length == 1 && keccak256(abi.encode(values[0])) == keccak256(abi.encode(uint256(42))));
        require(keccak256(abi.encode(vm.parseTomlUintArray(document, ".values", new uint256[](0)))) == keccak256(abi.encode(values)));
        require(keccak256(abi.encode(vm.parseTomlUintArray(document, ".missing", values))) == keccak256(abi.encode(values)));
    }
    function testTomlInt() public pure {
        string memory document = "value = -7\nvalues = [-7]\n";
        require(keccak256(abi.encode(vm.parseTomlInt(document, ".value"))) == keccak256(abi.encode(int256(-7))));
        require(keccak256(abi.encode(vm.parseTomlInt(document, ".value", 0))) == keccak256(abi.encode(int256(-7))));
        require(keccak256(abi.encode(vm.parseTomlInt(document, ".missing", -7))) == keccak256(abi.encode(int256(-7))));
        int256[] memory values = vm.parseTomlIntArray(document, ".values");
        require(values.length == 1 && keccak256(abi.encode(values[0])) == keccak256(abi.encode(int256(-7))));
        require(keccak256(abi.encode(vm.parseTomlIntArray(document, ".values", new int256[](0)))) == keccak256(abi.encode(values)));
        require(keccak256(abi.encode(vm.parseTomlIntArray(document, ".missing", values))) == keccak256(abi.encode(values)));
    }
    function testTomlBool() public pure {
        string memory document = "value = true\nvalues = [true]\n";
        require(keccak256(abi.encode(vm.parseTomlBool(document, ".value"))) == keccak256(abi.encode(bool(true))));
        require(keccak256(abi.encode(vm.parseTomlBool(document, ".value", false))) == keccak256(abi.encode(bool(true))));
        require(keccak256(abi.encode(vm.parseTomlBool(document, ".missing", true))) == keccak256(abi.encode(bool(true))));
        bool[] memory values = vm.parseTomlBoolArray(document, ".values");
        require(values.length == 1 && keccak256(abi.encode(values[0])) == keccak256(abi.encode(bool(true))));
        require(keccak256(abi.encode(vm.parseTomlBoolArray(document, ".values", new bool[](0)))) == keccak256(abi.encode(values)));
        require(keccak256(abi.encode(vm.parseTomlBoolArray(document, ".missing", values))) == keccak256(abi.encode(values)));
    }
    function testTomlAddress() public pure {
        string memory document = "value = \"0x0000000000000000000000000000000000001234\"\nvalues = [\"0x0000000000000000000000000000000000001234\"]\n";
        require(keccak256(abi.encode(vm.parseTomlAddress(document, ".value"))) == keccak256(abi.encode(address(address(0x1234)))));
        require(keccak256(abi.encode(vm.parseTomlAddress(document, ".value", address(0)))) == keccak256(abi.encode(address(address(0x1234)))));
        require(keccak256(abi.encode(vm.parseTomlAddress(document, ".missing", address(0x1234)))) == keccak256(abi.encode(address(address(0x1234)))));
        address[] memory values = vm.parseTomlAddressArray(document, ".values");
        require(values.length == 1 && keccak256(abi.encode(values[0])) == keccak256(abi.encode(address(address(0x1234)))));
        require(keccak256(abi.encode(vm.parseTomlAddressArray(document, ".values", new address[](0)))) == keccak256(abi.encode(values)));
        require(keccak256(abi.encode(vm.parseTomlAddressArray(document, ".missing", values))) == keccak256(abi.encode(values)));
    }
    function testTomlString() public pure {
        string memory document = "value = \"hello\"\nvalues = [\"hello\"]\n";
        require(keccak256(abi.encode(vm.parseTomlString(document, ".value"))) == keccak256(abi.encode(string("hello"))));
        require(keccak256(abi.encode(vm.parseTomlString(document, ".value", ""))) == keccak256(abi.encode(string("hello"))));
        require(keccak256(abi.encode(vm.parseTomlString(document, ".missing", "hello"))) == keccak256(abi.encode(string("hello"))));
        string[] memory values = vm.parseTomlStringArray(document, ".values");
        require(values.length == 1 && keccak256(abi.encode(values[0])) == keccak256(abi.encode(string("hello"))));
        require(keccak256(abi.encode(vm.parseTomlStringArray(document, ".values", new string[](0)))) == keccak256(abi.encode(values)));
        require(keccak256(abi.encode(vm.parseTomlStringArray(document, ".missing", values))) == keccak256(abi.encode(values)));
    }
    function testTomlBytes() public pure {
        string memory document = "value = \"0x1234\"\nvalues = [\"0x1234\"]\n";
        require(keccak256(abi.encode(vm.parseTomlBytes(document, ".value"))) == keccak256(abi.encode(bytes(hex"1234"))));
        require(keccak256(abi.encode(vm.parseTomlBytes(document, ".value", hex""))) == keccak256(abi.encode(bytes(hex"1234"))));
        require(keccak256(abi.encode(vm.parseTomlBytes(document, ".missing", hex"1234"))) == keccak256(abi.encode(bytes(hex"1234"))));
        bytes[] memory values = vm.parseTomlBytesArray(document, ".values");
        require(values.length == 1 && keccak256(abi.encode(values[0])) == keccak256(abi.encode(bytes(hex"1234"))));
        require(keccak256(abi.encode(vm.parseTomlBytesArray(document, ".values", new bytes[](0)))) == keccak256(abi.encode(values)));
        require(keccak256(abi.encode(vm.parseTomlBytesArray(document, ".missing", values))) == keccak256(abi.encode(values)));
    }
    function testTomlBytes32() public pure {
        string memory document = "value = \"0x0000000000000000000000000000000000000000000000000000000000001234\"\nvalues = [\"0x0000000000000000000000000000000000000000000000000000000000001234\"]\n";
        require(keccak256(abi.encode(vm.parseTomlBytes32(document, ".value"))) == keccak256(abi.encode(bytes32(bytes32(uint256(0x1234))))));
        require(keccak256(abi.encode(vm.parseTomlBytes32(document, ".value", bytes32(0)))) == keccak256(abi.encode(bytes32(bytes32(uint256(0x1234))))));
        require(keccak256(abi.encode(vm.parseTomlBytes32(document, ".missing", bytes32(uint256(0x1234))))) == keccak256(abi.encode(bytes32(bytes32(uint256(0x1234))))));
        bytes32[] memory values = vm.parseTomlBytes32Array(document, ".values");
        require(values.length == 1 && keccak256(abi.encode(values[0])) == keccak256(abi.encode(bytes32(bytes32(uint256(0x1234))))));
        require(keccak256(abi.encode(vm.parseTomlBytes32Array(document, ".values", new bytes32[](0)))) == keccak256(abi.encode(values)));
        require(keccak256(abi.encode(vm.parseTomlBytes32Array(document, ".missing", values))) == keccak256(abi.encode(values)));
    }
    function testKeysAndLength() public view {
        require(vm.keyExistsJson('{"a":1}', ".a"));
        require(!vm.keyExistsJson('{"a":1}', ".b"));
        require(vm.keyExistsToml("a=1", ".a"));
        require(!vm.keyExistsToml("a=1", ".b"));
        string[] memory keys = vm.parseJsonKeys('{"a":1}', ".");
        require(keys.length == 1 && keccak256(bytes(keys[0])) == keccak256("a"));
        keys = vm.parseTomlKeys("a=1", ".");
        require(keys.length == 1 && keccak256(bytes(keys[0])) == keccak256("a"));
        require(vm.parseJsonArrayLength('{"a":[1,2]}', ".a") == 2);
    }
    function testMalformedAndMissing() public {
        vm._expectCheatcodeRevert();
        vm.parseJsonUint("{", ".a", 9);
        vm._expectCheatcodeRevert();
        vm.parseTomlUint("a=[", ".a", 9);
        vm._expectCheatcodeRevert();
        vm.parseJsonUint("{}", ".a");
        vm._expectCheatcodeRevert();
        vm.parseTomlUint("", ".a");
    }
}
contract NativeParserDeprecatedTest {
    ParserVm constant vm = ParserVm(address(uint160(uint256(keccak256("hevm cheat code")))));
    function setUp() public view { require(vm.keyExists('{"a":1}', ".a")); }
    function testDeprecatedSetupRetained() public pure {}
}
