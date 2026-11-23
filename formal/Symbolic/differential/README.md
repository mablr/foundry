# Symbolic differential fuzzer

`fuzz.rs` compares the symbolic smart constructors and solver shortcuts with a reference EVM
evaluator (see the conformance section of `../README.md`). `crates/evm/symbolic/AGENTS.md` keeps
crate-level test harnesses out of the crate, so this file is not compiled by default. It reaches
crate-private items and runs as a temporary child module of the solver:

```bash
cp formal/Symbolic/differential/fuzz.rs crates/evm/symbolic/src/runtime/solver/fuzz_tmp.rs
echo '#[cfg(test)] mod fuzz_tmp;' >> crates/evm/symbolic/src/runtime/solver/mod.rs
FUZZ_SEED=1 FUZZ_ITERS=20000 cargo +nightly test -p foundry-evm-symbolic --lib fuzz_tmp -- --nocapture
git checkout crates/evm/symbolic/src/runtime/solver/mod.rs
rm crates/evm/symbolic/src/runtime/solver/fuzz_tmp.rs
```

`FUZZ_DEPTH`, `FUZZ_VARS` and `FUZZ_CONS` set the expression depth, the number of variables and
the number of constraints per case. A run prints its statistics and fails on any mismatch.
