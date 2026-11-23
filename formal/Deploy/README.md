# Deploy

Lean models of the Foundry code whose bugs produce a wrong transaction, a wrong deployed address
or a wrong verification verdict. Build with `../check.sh Deploy`; print the axioms of the headline
theorems with `lake env lean Deploy/Axioms.lean`.

## Scope

| Module | Rust | Question answered |
| --- | --- | --- |
| `Resume` | `completed_prefix` (`crates/script/src/sequence.rs`) | Where does `forge script --resume` restart? |
| `Offsets` | `normalize_offsets`, `matches_outside` (`crates/common/src/contracts.rs`) | Which bytes does deployed-bytecode matching compare? |
| `Metadata` | `find_metadata_start`, `ignore_metadata_hash` (`crates/common/src/utils.rs`); `match_bytecodes`, `is_partial_match`, `try_extract_and_compare_bytecode` (`crates/verify/src/utils.rs`) | What does a `verify-bytecode` match guarantee? |

## What is proved

### `Resume`

- `completedPrefix_iff`: `completed_prefix(txs, receipts) = Ok(k)` iff the first `k`
  transactions all have hashes and the multiset of those hashes equals the multiset of receipt
  hashes.
- `spec_length`, `spec_unique`: such a `k` is the number of receipts, so it is unique; an error
  means no index is consistent with the receipts.
- `completedPrefix_split`: on `Ok(k)` the transactions not re-sent are exactly `txs.take k`, whose
  hashes are a permutation of the receipts: no double-send and no skip.

### `Offsets`

- `normalize_separated`: normalized ranges are sorted, pairwise disjoint and non-adjacent
  (`a.stop < b.start` for every earlier `a` and later `b`).
- `covers_normalize`: they cover exactly the bytes the input ranges cover.
- `normalize_bounds`: every merged range ends no later than the largest input end and starts at
  an input start. The first fact makes Rust's `as u32` narrowing exact; the second keeps the
  segment loop's slicing in bounds whenever the input ranges start within the code.
- `partialMatch_iff`: the segment loop over the normalized ranges accepts iff the lengths agree
  and every byte outside the union of the input ranges decodes and equals the onchain byte.
- `exactMatch_iff`: adding the metadata range and comparing `[m..]` in full accepts iff every
  byte outside the other ranges, and every byte from `m` on, matches.

### `Metadata`

- `tryExtract_iff`: the fixed comparison accepts iff the codes are `SameUpToMetadata`: equal, or
  of equal length and equal outside a region `[m, len - 2)` that both declare with the same
  trailing length and that decodes on both sides.
- `matchBytecodes_creation_sound`: a creation-code match of `code ++ args` means the onchain code
  is `code' ++ args` with `SameUpToMetadata code code'`.
- `matchBytecodes_args`: hence `onchain.drop code.length = args`: the bytes the constructor
  decodes are exactly the arguments verify reports.
- `matchBytecodes_runtime_sound`: a runtime match means `SameUpToMetadata`.
- `old_accepts_wrong_args`: the bug this library found. Before the fix each side was stripped by
  its own trailing length. For any CBOR predicate that, like ciborium, accepts `0x60` followed by
  anything, the old code reports a partial match on a concrete input where the constructor
  decodes different arguments; the fixed code rejects it.

## Fidelity

- Transactions are `Option H` for an arbitrary `H` with decidable equality; the receipt count map
  is the list of unmatched receipts, decremented with `List.erase`. Both Rust error paths are
  `none`.
- `Offsets` uses `Nat` for `u32`; justified by `normalize_bounds` when every input end fits in
  `u32`. The local artifact is a list of `Option` bytes, `none` marking a position whose hex
  does not decode (an unlinked placeholder), which covers both `BytecodeObject` variants. Rust
  slicing panics if a range starts past the code; the model clamps.
- `cbor : List UInt8 → Bool` abstracts `ciborium::from_reader::<Value, _>(_).is_ok()`. The
  soundness theorems hold for every predicate. ciborium 0.2.2 reads one item and ignores trailing
  bytes, which is the only property the counterexample assumes.
- Not modelled: how `find_by_deployed_code_exact_inner` picks among several matching artifacts
  (preference, ambiguity), Vyper handling, the collection of ignored ranges from the artifact, and
  where verify obtains the constructor arguments.

## Axioms

All headline theorems depend only on Lean's standard axioms: `propext` and `Quot.sound`, plus
`Classical.choice` for `completedPrefix_iff`, `completedPrefix_split` and the `Metadata`
theorems (`spec_unique` uses `propext` alone). No `sorry`, no `axiom`, no `native_decide`.

## Conformance

Each Rust function names its Lean theorem in its rustdoc, and a Rust test checks the same contract
directly against the Rust code:

- `completed_prefix_matches_spec` (`crates/script/src/sequence.rs`): all transaction lists of
  up to five entries over three hashes or none, against all receipt lists of up to four hashes
  (165,165 cases), compared with an independent sort-based reference.
- `normalize_offsets_matches_mask` and `matches_outside_matches_mask`
  (`crates/common/src/contracts.rs`): all lists of up to three ranges over a small window against
  a brute-force byte mask; for the segment loop, every set of differing bytes, for both linked
  and unlinked code.
- `match_bytecodes_matches_contract` (`crates/verify/src/utils.rs`): 200,000 random codes with
  real, forged and mutated metadata tails, compared as runtime code and as creation code with
  arguments, against `SameUpToMetadata`, plus the `matchBytecodes_args` consequence.

## Dropped and rescoped

- **Library linking** (`crates/linking`, `crates/script/src/library_deployments.rs`). The CREATE
  path assigns `sender.create(nonce + i)` in the same loop that orders `libs_to_deploy`, so the
  address/broadcast-order agreement holds by construction and a proof adds little. The CREATE2
  worklist is a standard topological sort whose only interesting assumption, that
  `is_unlinked()` is false exactly when every dependency has an address, lives in the compiler
  artifact format rather than in code a model can check. `replace_addresses` is sequential byte
  substitution; its equivalence to simultaneous substitution needs "no replacement creates a new
  occurrence of another address", which holds only probabilistically for 20-byte addresses.
  The suspected divergence, that `link_with_create2_partition` does not deduplicate libraries
  landing at the same CREATE2 address while `link_with_create2_detailed` does, is not harmful:
  the script runner skips a CREATE2 library whose address already has code, and the count
  mismatch then makes `equivalent_candidate` reject the candidate, falling back to the baseline.
  It only costs the optimization in that (rare, identical-bytecode) case.

## Known gap: lenient metadata detection

`SameUpToMetadata` lets the codes differ inside `[m, len - 2)` whenever that region "decodes".
With ciborium that only means "starts with one CBOR item", and the trailing length is read from
the code itself. For a local artifact that really ends in Solidity metadata this is the intended
partial match. For an artifact built with `cbor_metadata = false`, the last two code bytes can
happen to declare a short region that starts with a decodable byte, and the matchers then ignore
real code there (in `verify-bytecode`, and as the metadata range in
`find_by_deployed_code_exact`). Requiring the region to be exactly one complete CBOR map would
close this, but it would also stop stripping Vyper metadata, whose length suffix counts its own
two bytes, so it is left as a follow-up rather than changed here. The theorems are parametric in
`cbor` and carry over to a stricter predicate unchanged.

## Next steps

- Model the artifact-selection logic of `find_by_deployed_code_exact_inner` (preferred artifact,
  ambiguous partial matches).
- State and prove the `equivalent_candidate` check as a simulation relation between the baseline
  and candidate transaction lists, assuming injective CREATE/CREATE2.
- Generate test vectors with Lean `#eval` and consume them in Rust, to also pin down the
  concrete outputs, not just the contract.
