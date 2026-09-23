// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;
contract Counter {
    uint256 public value;
    function set(uint256 next) external { value = next; }
}
contract NativeScript {
    function run() external returns (uint256) {
        Counter counter = new Counter();
        counter.set(42);
        require(counter.value() == 42);
        return counter.value();
    }
}
