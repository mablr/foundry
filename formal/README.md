# Foundry formal models

Lean 4 models of Foundry components whose correctness users rely on but cannot easily check
themselves. Each library states the contract of one component, proves the invariants that the
Rust implementation depends on, and records how the Rust code is kept in conformance with the
model.

| Library | Component | Rust owner |
| --- | --- | --- |
| `Symbolic` | Soundness of symbolic `PASS` verdicts: expression rewrites, certificates, lowering | `crates/evm/symbolic` |
| `Deploy` | Script resume, bytecode matching, metadata stripping, library linking | `crates/script`, `crates/common`, `crates/verify`, `crates/linking` |
| `Cheatcodes` | `expectEmit`, `vm.bound`, mock calls, `expectCall`, approximate assertions | `crates/cheatcodes`, `crates/evm/fuzz` |
| `Lifecycle` | Fork, snapshot and persistent-account state machine | `crates/evm/core` |

## Building

```bash
./check.sh          # lake build of every library; fails on `sorry`
./check.sh Deploy   # one library
```

The toolchain is Lean 4.33.0 (`lean-toolchain`) with no Mathlib. `bv_decide` needs a CaDiCaL 2.x
binary next to `lean` or on `PATH`.

## Trust model

A theorem here says something about the Rust code only through the model it is stated over. Each
library documents, next to its model:

- **Fidelity**: which Rust function each Lean definition models, with its path, and every
  deliberate abstraction, such as hashing as an injective function or maps as association lists.
- **Conformance**: how a drift between the Rust code and the model is caught: shared test vectors,
  Rust tests that check the same contract, or a differential test.
- **Axioms**: `#print axioms` for each headline theorem. Proofs by `bv_decide` additionally trust
  Lean's native evaluation and the SAT solver's checked LRAT certificate (`Lean.ofReduceBool`).

## Conventions

- One directory per library; a library never imports another.
- No `sorry` and no new `axiom` except named, documented modelling assumptions.
- Comments end with periods.
