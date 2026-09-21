//! Coverage for the Ethereum-only EVM2 migration preparation branch.

use foundry_test_utils::str;

forgetest!(evm2_ethereum_execution_and_cheatcodes, |prj, cmd| {
    prj.add_test(
        "EthereumOnly.t.sol",
        r#"
pragma solidity ^0.8.0;

interface Vm {
    function deal(address account, uint256 balance) external;
    function snapshotState() external returns (uint256);
    function revertToState(uint256 snapshot) external returns (bool);
    function prank(address sender, address origin) external;
}

contract EthereumOnlyTest {
    Vm constant vm = Vm(address(uint160(uint256(keccak256("hevm cheat code")))));

    function testSnapshotAndPrank() public {
        address account = address(0x1234);
        vm.deal(account, 1 ether);
        uint256 snapshot = vm.snapshotState();
        vm.deal(account, 2 ether);
        require(vm.revertToState(snapshot));
        require(account.balance == 1 ether);

        vm.prank(account, account);
        (address sender, address origin) = this.callers();
        require(sender == account && origin == account);
    }

    function callers() external view returns (address, address) {
        return (msg.sender, tx.origin);
    }
}
"#,
    );
    cmd.args(["test", "--offline", "--network", "ethereum"]).assert_success();
});

forgetest!(evm2_ethereum_only_rejects_tempo, |prj, cmd| {
    prj.add_test(
        "EthereumOnly.t.sol",
        "pragma solidity ^0.8.0; contract EthereumOnlyTest { function testExample() public {} }",
    );
    cmd.args(["test", "--offline", "--network", "tempo"]).assert_failure().stderr_eq(str![[r#"
Error: Tempo execution is disabled on the Ethereum-only EVM2 migration branch

"#]]);
});
