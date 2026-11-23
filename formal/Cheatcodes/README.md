# Cheatcodes

Lean models of the matching and arithmetic contracts behind `expectEmit`, `vm.bound`, mock calls,
`expectCall` and the approximate-equality assertions. Build with `../check.sh Cheatcodes`.

## Scope and fidelity

| Lean | Rust | Abstractions |
| --- | --- | --- |
| `ExpectEmit.observeWith`, `observe`, `checkOne` | `observe_log` in `crates/cheatcodes/src/expected_emit.rs` | `VecDeque` as `List` (`eraseIdx`, `insertIdx`, cons, append); log bodies and check flags abstract, with `checks_topics_and_data` as `Matcher.ck` and its concrete model `checksTopicsAndData`; the per-emitter `LogCountMap`s as the list `seen` of counted emitters; `mismatch_error` not modelled |
| `ExpectEmit.relCount` | `matched_count` | the count of the emitter, or the sum over emitters |
| `ExpectEmit.checkCall` | `check_call_emits` | depth and static-call gating not modelled: the model is the check once it applies |
| `ExpectEmit.rootUnmet` | `first_unmet_root_emit` | returns only whether an expectation is unmet |
| `ExpectEmit.run` | `log_hook` over the logs of a call | stops at the first failing log, whose frame reverts |
| `Bound.uBound`, `Bound.iBound` | `BoundMutator::bound` and `bound_u256`, `bound_i256` in `crates/evm/fuzz/src/strategies/mutators.rs` | the RNG as an oracle (`bits`, `word`, `flip`); `I256` as `Int` with its raw word via `toRaw`/`fromRaw` |
| `Bound.oldCandidate` | the pre-fix `U256` sampler | one loop iteration |
| `Mock.findMock` | `find_mock_returns` in `crates/cheatcodes/src/evm/mock.rs` | the `BTreeMap` as its sorted iteration order; `range_mut(ctx..)` as the keys `≥ ctx` |
| `ExpectCall.register`, `observe`, `passes` | `expect_call`, `observe_call`, `first_unmet_call` in `crates/cheatcodes/src/test/expect.rs` | one callee; the gas stipend not modelled |
| `Approx.getDeltaInt`, `calcDeltaFull`, `uintRel`, `intRel`, `intAbs` | `get_delta_int`, `calc_delta_full`, `{uint,int}_assert_approx_eq_{abs,rel}` in `crates/cheatcodes/src/test/assert.rs` | `U256`/`U512` wrapping as `%`, so the theorems show nothing wraps |

## Proved theorems

- `ExpectEmit.run_spec`: from a freshly declared queue, the logs of the asserted call trip a
  `count = 0` expectation iff one of them matches it, and otherwise the call-end check passes and
  clears the queue iff the positive expectations, in declaration order, are matched by consecutive
  blocks of the logs holding at least `count` matching logs each.
- `ExpectEmit.Sat_count_one_iff`: with `count = 1`, that is ordered-subsequence matching with
  extra logs allowed.
- `ExpectEmit.step_found_iff`: a checked expectation becomes found iff the log matches and the
  matching logs, summed over all emitters when no emitter is set, reach `count`.
- `ExpectEmit.checkCall_iff`, `rootUnmet_iff`: the call-end and test-end checks fail iff a
  positive expectation is pending; the queue is cleared only when all are met.
- `ExpectEmit.old_misses_two_emitters`: the pre-fix rule rejects `expectEmit(2)` with matching
  logs from two emitters for every `HashMap` iteration order.
- `Bound.uBound_some`, `iBound_some`: every result is in `[min, max]` and differs from `current`.
- `Bound.uBound_surjective`, `iBound_surjective`: every other value of the range is produced by
  some draw. The fixed sampler draws once, so it terminates.
- `Bound.oldUBound_unreachable`: the pre-fix sampler could not produce `max` on the full range,
  nor any value at or above `min + 2^128`.
- `Mock.findMock_spec` with `eq_of_not_better`: on a well-formed map the lookup returns the
  unique matching mock with the longest calldata prefix, value-specific before value-agnostic,
  and returns nothing iff no mock matches. `matches_in_range` is the reason the reversed-order
  range trick is complete: every prefix of the input sorts at or after the input, so interleaving
  keys cannot hide one. `findMock_exact` and `findMock_no_value` are corollaries.
- `ExpectCall.observe_counts`: each expectation counts exactly the calls that pass its filters.
- `ExpectCall.repeated_filters_dropped`: see below.
- `Approx.getDeltaInt_eq`, `intAbs_iff`: the signed delta is `|l - r|` over the integers.
- `Approx.intRel_eq_spec`, `uintRel_eq_spec`: the relative delta is
  `⌊|l - r| * 10^18 / |r|⌋`, an overflow is reported iff that is at least `2^256`, and with
  `r = 0` the assertion passes iff `l = 0`.

## Axioms

`Axioms.lean` prints the axioms of every headline theorem: each depends only on `propext`,
`Quot.sound` and, for most, `Classical.choice`. No theorem uses `sorry`, `native_decide`,
`bv_decide` or a modelling axiom.

## Conformance

- `expected_emit.rs`: `matcher_conforms_to_specification` runs 20,000 pseudo-random queues and
  log sequences through `observe_log` and `check_call_emits` and compares them with a brute-force
  implementation of `Sat` and the `count = 0` rule. `counts_matching_logs_across_emitters` is the
  regression test of the fixed bug.
- `mutators.rs`: `u256_bound_reaches_every_other_value` and `i256_bound_reaches_every_other_value`
  check the draw-to-value maps exhaustively on small ranges (every draw, every `current`, both
  flips) and at the ends of the full ranges.
- `mock.rs`: `lookup_matches_longest_prefix_reference` checks `find_mock_returns` against the
  brute-force longest-prefix rule on 2,000 maps over all 21 keys of calldata length at most 2 and
  three values, for every input of length at most 3 and four values.
- `assert.rs`: `approx_eq_matches_lean_vectors` checks the `#eval vectors` output of
  `Approx.lean`, including the `I256::MIN`/`I256::MAX` extremes.
- Solidity: `testdata/default/cheats/ExpectEmit.t.sol` (`testCountEmitsAcrossEmitters`) and
  `testdata/default/cheats/Bound.t.sol`.

The Rust functions carry a rustdoc pointer to the theorem they implement.

## Findings

- `expectEmit` with a count and no emitter decided `found` from the count of one arbitrary
  emitter, while `check_call_emits` summed over emitters, so matching logs from two emitters
  failed with `log != expected log`. Fixed by `matched_count`, shared by both.
- `vm.bound` drew its candidate from 128 bits and saturated the range size, so values at or above
  `min + 2^128` and, on the full range, `max` were unreachable. Fixed by drawing 256 bits and
  mapping the draw onto the range without `current`, without rejection sampling.
- `expectCall` added the 2300 gas stipend with `+=`, which overflows for gas near `u64::MAX`
  (a panic in debug builds, a wrong expectation in release). Fixed with `saturating_add`, as in the
  symbolic engine.

## Not changed

`expectCall` keys expectations by calldata and call scheme only, so a repeated non-counted
`expectCall` with a different value or gas only raises the call count of the first one and drops
its own filters (`repeated_filters_dropped`): a test expecting one call with value 1 and one with
value 2 passes on two calls with value 1. This keyed-additive behaviour is documented on
`expect_call` and mirrored on purpose by the symbolic engine (`register_expected_call`), so
changing it is a user-visible semantics change for both engines and the Book, not a local fix.

The mock lookup was correct; the model proves it rather than changing it.

## Gaps and next steps

- The `expectEmit` theorems cover the check phase: all expectations filled before the asserted
  call. Filling (`fill` branch), expectations added while earlier ones are found, and the depth at
  which `check_call_emits` fires are modelled or not modelled but not proved about.
- `checks_topics_and_data` is abstract in the theorems; only its reflexivity is used.
- The `vm.bound` theorems are about reachability, not the distribution.
- Conformance is by Rust tests against references of the same contracts; Lean `#eval` vectors are
  copied by hand into `approx_eq_matches_lean_vectors`, not generated.
