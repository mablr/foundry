# M2 inventory and acceptance checklist

Static audit: 2026-09-18. This document closes the **enumeration** of the current
cheatcode ABI and identifies the non-ABI lifecycle work. It does not certify native
behavior. [Transition evidence](../evm2-migration-spec.md#12-native-cheatcode-slice-2026-09-18)
remains the owner of executed results; [engine gaps](../evm2-upstream-requirements.md)
remain separate from unfinished Foundry implementation.

## Denominator and reproducibility

Run from the repository root:

```sh
python3 docs/dev/evm2-m2/inventory.py
python3 docs/dev/evm2-m2/inventory.py --check
```

The denominator is the shared `Vm` ABI; network-specific extra-address ABIs such as
Monad are deferred, and Hardhat console is covered by L1 rather than selector counts.
The script checks all 609 generated ABI IDs against the names/overload counts in
`spec/src/vm.rs`, finds reference source tokens for every ID, and requires one
milestone/family assignment per entry. It also requires an assignment for each of
68 fields in `Cheatcodes`. These checks fail on unclassified additions. Source
hashes identify the exact dirty-tree source snapshot; a Git revision alone would
not describe this audit. Regenerate after source changes, then review the diff.

- [selectors.csv](selectors.csv): one row per ABI overload, selector, owner, native
  route, reference source tokens and test-name lookup. Includes internal and deprecated ABI.
- [test-candidates.csv](test-candidates.csv): candidate files per function name.
  Lexical matches include declarations/comments and cannot distinguish overloads,
  prove execution, or demonstrate an assertion. They are navigation aids only.
- [state-fields.csv](state-fields.csv): all public/private fields, owner and open
  lifecycle-validation status. `open` means the complete lifetime matrix is unproven,
  even where selected cases already pass.
- [configuration.csv](configuration.csv): CheatsConfig and InspectorStackInner fields,
  including private state used by factory redirects and nested execution; L8 owns review.
  The 58 rows include 22 CheatsConfig and 36 stack fields; feature/test-gated fields
  remain visible and require an applicability decision.
- [native-fields.csv](native-fields.csv): native session, call context and adapter
  temporary fields; each needs an explicit lifetime and error-path assessment.
- [callbacks.csv](callbacks.csv): explicit inspector callbacks, including stack
  forwarding and frame hooks. Duplicates are definitions, not additional behavior.
- [counts.md](counts.md), [source-hashes.json](source-hashes.json): reproducible counts
  and source provenance.

Current M2 denominator: **564 overloads across 23 families**. **303** have a direct
or shared route, **2** have conditional routes (`roll`, `etch`), **259** have none.
The other **45** are 32 M3, 7 M4 and 6 deferred network overloads. Route presence
is about **54%**, not implementation effort or behavioral acceptance. Of the 303,
116 are shared assertion overloads. No family is certified end-to-end by this audit;
do not derive a completion percentage from fixture totals or lexical test matches.
The earlier 35–45% effort estimate had no closed denominator and is superseded by
these explicit counts; there is no defensible percentage of remaining effort yet.

Extraction is deliberately lexical, not a Rust type/call-graph analysis. Native
routes cover the current explicit match arms, assertion macro inputs, stateless trait implementations and shared
expectation methods. A new dispatch mechanism requires updating the extractor.
Conditional branches, transitive helpers, generated macro semantics, feature gates
and test assertions require review. ABI/spec consistency checks names/counts, not
canonical Solidity types or recomputed selectors. State enumeration covers `Cheatcodes`, plus `CheatsConfig` and `InspectorStackInner`
configuration/state fields. Nested structs and executor internals are assessed below
rather than claimed as exhaustively extracted structs.

See the [remaining assessment register](remaining-assessment.md) for audit blind spots,
ordered validation work and the evidence required to close an item.

## Scope boundary

M2 owns native cheatcode semantics and their inspector/session lifecycle in ordinary
Ethereum execution, including host-only helpers, broadcast **collection**, deployment
helpers, authorization/blob registration and assume/skip signals. These are needed
by deterministic tests too; being primarily used by scripts or fuzzing does not
remove their dispatch/cleanup obligations from M2.

M3 owns snapshot/fork replacement, persistence, `transact`/`executeTransaction`,
inherited execution and isolation. M4 owns transaction-envelope/hardfork-wide and
consumer acceptance: script simulation/publication, fuzz/invariant campaign behavior,
full tracing/debugging, coverage and Chisel. M5 owns network-only expectations and
precompile operations. Ethereum behavior of `isImplicitlyApproved` and
`assumeImplicitApproval` stays M2 (false / assume rejection). Pure signing helpers,
including keychain signing, stay M2. No existing Ethereum behavior is dropped.

A split feature needs two gates: M2 verifies local registration, state changes,
outputs and cleanup; M3/M4 verifies the deferred workflow. For example M2 verifies
broadcast transaction collection and nonce/order; M4 verifies publishing/simulation.
M2 verifies synthetic authorization registration; M4 verifies real type-4 envelopes.
Fork access permissions remain a M3 integration gate even though `allowCheatcodes`
is inventoried in M2. These boundaries must not be used to mark untested work done.

## Capability obligations

Every row is **open**. “Partial” identifies implemented slices, not acceptance.
The CSV provides the exact overload list for each family; this table specifies what
must be assessed and tested beyond finding an implementation. Fixture names below
are existing anchors under `testdata/default/cheats/`, not claims that native runs pass.

| Family | Current state | Required acceptance / remaining assessment | Existing anchors |
| --- | --- | --- | --- |
| M2-assert | Shared predicates; selected success/failure tests | All types/arrays/decimal/approximate overloads, custom messages and boundary arithmetic; revert vs legacy diagnostics/failure-slot lifetime; caught/nested failures and setup separation. Snapshot sticky failure crosses M3. | Assert; native Cheatcodes |
| M2-expect | All revert/call/emit/create registrations; selected lifecycle evidence | Emit/anonymous emit counts, topics/data/emitter, ordering, depth and reverting logs; CREATE/CREATE2 deployer/runtime matching; all revert/call overloads, internal-revert setting, dangling/count/overwrite rules; interactions with mocks/pranks/console and original failures. E2-08/09. | ExpectRevert, ExpectEmit, ExpectCreate; native Expectations/Calls |
| M2-prank | Sender/delegate slices | Origin-changing overloads, CALL/CALLCODE/DELEGATECALL/STATICCALL and CREATE identities; depth, replacement and cleanup on every outcome; introspection and broadcast conflicts; constructor/callback behavior. E2-07. | Prank, ReadCallers |
| M2-mock | Return/revert/queue/function slices | `mockFunction` execution/storage/caller identity; exact/prefix/value/selector priority, empty queues, repeat-last, clear, code injection and surviving registrations; static/value/funds/delegation/overflow paths; settlement and raw gas. E2-08/09/10. | MockCall, MockCalls, MockFunction |
| M2-account | load/store/etch/getNonce slices | deal/nonce override rollback policy; clone/allocation/dump semantics; existence, code/hash and selfdestruct/create status; account/slot warm/cold and access lists; precompile/history-address restrictions; permissions and host failures. E2-06/10. | Store, Etch, SetNonce, CloneAccount, AccessList, StorageSlotState |
| M2-env | Basic block getters/mutations | Chain/spec/difficulty/blob/slot/blockhash/gas-price setters/getters; pre/post-Prague roll/history; returned opcode values versus synthetic settlement; parent/child visibility and setup lifetime; invalid values/spec changes. E2-07. | Roll, Blobhashes, BlobBaseFee, TxGasPrice, SlotNumber, Prevrandao |
| M2-record | Basic recorded logs | Storage reads/writes, ordering/dedup rules, call/create/resume/selfdestruct account records, reverted flags and old/new data; start/stop/restart/JSON; setup-prefix handoff and created-account provenance; mapping hashes only after successful KECCAK. | Record, RecordLogs, RecordAccountAccesses, StateDiffStorageLayout, StateDiffMappings, GetStorageSlots, Mapping |
| M2-hooks | Absent | SLOAD/SSTORE/mapping registration and replacement; effective proxy storage address; trigger only after successful opcode; preserve parent stack, return data, gas and original values; enclosing rollback, callback revert/OOG/static behavior, suppression and reentrancy. | MappingStorageHooks; IN-02/G-04 in transition ledger |
| M2-gas | Absent controls; partial outcome accounting | Pause/resume/reset across nested frames; last-call versus last-frame; regions, names/overwrite, snapshot values; success/revert/halt/precompile/early failure; gas/refunds/caps/stipends and no observer-induced changes. Full fork/state-gas matrix crosses M4. | GasMetering; gas snapshot anchors T-13/T-14/T-17 and GA-01/02 |
| M2-memory | Absent | Allowed ranges, current/next-call scope, disjoint/overlapping/empty ranges; each memory-writing opcode including copies and call output; checked arithmetic, expansion, failures and cleanup. | MemSafety |
| M2-arbitrary | Absent | Seeded first-read generation, explicit stores, overwrite mode, copied-source aliasing/cache, warm/original storage and rollback; deterministic replay and test reset. Campaign/shrinking behavior crosses M4. | ArbitraryStorage, CopyStorage |
| M2-control | Absent | Assume and assumeNoRevert filters/depth/reverter, cleanup and interaction with expectRevert; skip genuine minted payload versus forged bytes, caught skip and setup; context/isolate queries and non-Tempo defaults. Consumer rejection budgets cross M4. | AssumeNoRevert, Skip; test runner classification |
| M2-broadcast | Absent | One-shot/persistent collection, caller/origin restore, prank conflicts, nested nonces, STATICCALL restrictions, CREATE redirection, wallets; signed delegation nonce/code rules and blob staging/consumption. Simulate/publish and real envelope validation stay M4. | Broadcast, AttachDelegation, AttachBlob; fork_nested_broadcast_nonces crosses M3 |
| M2-create | Absent | deployCode overloads/artifact lookup/arguments/value, failure payloads and address calculation; interceptInitcode one-shot behavior, nonce/state/gas effects; factory routing and balanced callbacks. | GetCode; intercept/create references in candidate index; IN-03 |
| M2-host-crypto | 19 stateless overloads routed | ABI encoding/errors and boundary keys/signatures/curves; wallet/signer cache lifetime; shared helper reuse without a REVM context; deterministic crypto outputs. | Sign, Derive, Addr and candidate index |
| M2-host-environment | Absent | Scalar/array/default parsing, variable overrides and missing/malformed input; context boundaries and test/process lifetime. | Candidate index by env*/setEnv/resolveEnv |
| M2-host-filesystem | Absent | Artifact/build/broadcast readers, permissions/path handling, file cursor lifetime, fs_commit behavior; FFI exit/stdout/stderr/encoding; prompts via controlled harness. No external action is needed for this audit. | GetCode, GetSelectors, Prompt; candidate index |
| M2-host-json | 32 stateless overloads routed | Typed/untyped parse/serialize, keys/path/errors, deterministic ordering and serializer persistence/reset; write permissions. | Candidate index by exact helper name |
| M2-host-toml | 30 stateless overloads routed | Same typed/path/error/permission contracts for TOML; reference-format parity. | Candidate index |
| M2-host-string | All 19 overloads routed | Parse/format, malformed and boundary values, byte/Unicode semantics and ABI output. | ToString; candidate index |
| M2-host-utilities | 12 stateless overloads routed | Bound/sort/shuffle/random and seed lifetime; base64/RLP/EIP712/address helpers; labels/cache persistence; invalid inputs and deterministic outputs. | Sort, Shuffle, Base64, GetLabel; candidate index |
| M2-host-testing | Two version overloads routed | Version comparisons, chain/RPC aliases and exact error formatting, sleep behavior and configuration lookup. | Candidate index; rpcUrl formatting exception |
| M2-host-evm | Absent | addr and RPC/header/log helpers: ABI/schema/error parity, endpoint resolution and deterministic stubbed responses; do not conflate provider access with fork switching. | Addr; candidate index |

## Non-ABI lifecycle checklist

These obligations cannot be closed by counting selectors. Each needs an explicit
reference contract, native owner and linked passing test before its checkbox closes.

- [ ] **L1 Dispatch:** malformed/unknown ABI, reserved target versus resolved code
  identity, blocked cheatcodes, caller permission (M3 fork integration), console
  handling, error wrapping (`rpcUrl` exception), deprecation collection and sticky
  unsupported-host errors even when caught. Check native dispatch enabled/disabled.
- [ ] **L2 Session ownership:** for every field in `state-fields.csv`, record
  initialization, mutation, child revert, root revert, speculative discard/commit,
  setup→test copy and independent-test reset. Describe nested structs (gas frame
  stacks, recording stacks, pending hooks, RNG, open files and serializer caches).
  Do not assume every inspector mutation follows journal rollback. M3 owns snapshot
  restoration, but each M2 field needs an explicit future snapshot policy.
- [ ] **L3 Entry/exit ordering:** ordered callbacks for ordinary/empty/precompile,
  cheatcode/console/mock short-circuit, depth/funds failure, all call schemes,
  constructors and host errors. Prank/broadcast cleanup precedes outcome rewriting;
  counts and reverter identity observe the intended original or rewritten result.
  Compare raw and reported failures separately. Native create delegates to call
  hooks; that alone does not establish reference CREATE semantics.
- [ ] **L4 Opcode continuation:** step/step_end bookkeeping is frame-local; failed
  opcodes do not publish mapping hashes or rewrite a nonexistent stack result;
  callback suppression/restoration survives nested entry and every exit. No stale
  pending opcode after parent suspension. Account for stack limits and return data.
- [ ] **L5 Settlement:** native checkpoint commit/rollback, warmth, original/current
  storage, transient storage, logs, balance/code/nonce changes and refunds remain
  correct after mocked or rewritten outcomes. Synthetic transfer errors and code
  deposit failures get explicit evidence, not success-path extrapolation.
- [ ] **L6 Creation bookkeeping:** account grant/creation provenance, failed and
  repeated deployments, CREATE2 factory rewrite, intercepted initcode, collected
  broadcast nonce and created-account recording agree; M3 owns fork/snapshot reuse.
- [ ] **L7 Host/result boundary:** database/code/storage/blockhash errors propagate;
  no panic or EVM-revert disguise; diagnostics, labels, gas snapshots, skip payloads,
  returned cheatcode state and logs survive result conversion as intended.
- [ ] **L8 Configuration:** every relevant CheatsConfig/InspectorStack option has a
  native owner or explicit milestone gate (assertion/internal-expect modes, restrictions,
  artifacts, permissions, RNG, factory, wallets, logs, gas, analysis). Default-only
  tests are insufficient. Unsupported active modes fail explicitly, not silently.
- [ ] **L9 Observers/cancellation:** logs/live console remain coherent with success
  and revert. Cancellation before/during/after execution preserves classification
  and state; record limits for blocked host/precompile calls. Full tracer/printer,
  debugger, coverage, fuzzer, Chisel and script observers remain explicit M4 gates;
  their shared hook ordering is recorded now, not implicitly certified.

## Concrete static findings to resolve first

These are source comparisons, not newly executed regressions or proven missing
engine APIs:

| Finding | Reference / native evidence | Required action |
| --- | --- | --- |
| Unknown-selector diagnostics shared; regression passes | `Cheatcodes::apply_cheatcode` augments unknown-selector errors with a Vm/Forge mismatch hint; both paths now use the same decoder. | Exact unknown-selector payload now matches in the six-case stateless fixture; malformed ABI remains open (L1). |
| Error-format exception aligned; rpcUrl still unported | Reference `apply_dispatch` excludes both assertions and `rpcUrl` from `vm.NAME:` wrapping; native dispatch now excludes both. | Preserve exception when enabling rpcUrl; add exact-output regression (L1). |
| Deprecation bookkeeping wired; setup warning regression passes | Reference dispatch populates `deprecated`; native dispatch now records and returns the same map. | Setup-time keyExists warning is preserved by a CLI snapshot; broader alias/failure cases remain (L1/L7). |
| State transfer is only a subset | Native execution explicitly round-trips block, revert/call/emit/create expectations, pranks, mocked calls, recorded logs and deprecations; new families cannot rely on all Cheatcodes state being synchronized. | Review all 68 field lifetimes before enabling dependent dispatch (L2). |
| Observer/transaction gates still constrain validation | `execute_evm2` rejects forks, isolation, tracer/printer, fuzz/coverage/Chisel/script inspectors and nonempty access/authorization/blob lists. | Keep M3/M4 boundaries explicit; do not count unreachable fixture assertions as M2 passes (L8/L9). |
| Callback inventory is larger than native hooks | Reference stack has frame_start/frame_end, log_full and selfdestruct; native adapter has eight explicit hook definitions and delegates create handling. | Assess semantic counterparts and necessary engine hooks per L3–L6; method-name mismatch alone is not an API gap. |

No new mandatory evm2 API request is established by these findings. E2-06–10
remain the confirmed engine-facing assessment list; add a new request only after
showing the required transition and why available native APIs cannot implement it.

## Completion and reporting rules

M2 may close only when:

1. Every M2 selector has a reviewed native route, restrictions and associated
   contract/test IDs. Conditional guards covering required behavior are resolved.
   Deprecated/internal aliases are included. Each M3/M4 split has a named handoff.
2. All 23 capability families and L1–L9 have reviewed transition assertions and
   passing evidence. Equivalent overloads may share a test contract only with a
   documented reason; distinct ABI decoding still needs coverage.
3. Each applicable transition covers registration/use/cleanup, success, EVM revert,
   exceptional halt and host failure; same/child/ancestor depth, setup persistence
   and test independence. Mark non-applicable combinations with reasons rather
   than requiring a meaningless full Cartesian product.
4. Run the unchanged relevant fixtures on matched reference/native inputs and add
   adversarial missing cases. Compare return/error bytes, status, visible state,
   later execution, recordings, callback order and gas/refunds where applicable.
   Record exact commands, revisions/compiler/settings, actual nonzero counts and
   skips. Candidate-file matches and successful compilation are not acceptance.
5. E2-06–10 are resolved where required by M2, with a native adapter or an adopted
   engine capability plus regression. Documenting a gap is not closing it. Newly
   discovered gaps follow the same rule; ordinary host helper ports are not engine
   requests. M3/M4-only gap portions remain explicitly assigned there.
6. No REVM live-context reconstruction, silent fallback, broad snapshot blessing
   or dropped reference assertions. Required formatting/Clippy and focused native
   regressions pass. No performance claim without a later matched benchmark.

Report three separate quantities: **enumerated** (609/609 ABI entries assigned),
**reachable** (current M2 303 + 2 conditional / 564), and **accepted contracts**
(linked transition evidence, currently not a complete per-contract tally).
No time/effort percentage follows from these quantities.

Next work: review candidate tests into exact assertion/test IDs, beginning with
expectations/mocks and dispatch L1; then host helper reuse, state/recording/creation,
and gas/hooks. Keep lifecycle and engine-capability work progressing alongside
cheap helpers so selector counts cannot create a misleading impression of completion.

## Current bounded evidence

Stateless and parser slices route 114 overloads; fixtures cover 6 stateless and 17
parser cases. Creation/reverter fixtures cover 8 positive and 5 negative cases;
events cover 8 positive and 6 negative cases. The fixture README and transition
ledger own exact commands, comparison settings and remaining behavioral limits.
The latest recorded native CLI run passes 25 tests; shared cheatcode unit tests
pass 78. These are overlapping evidence sets, not an accepted-contract count.

Event LOG errors stop the frame before settlement; frame-end mismatch rewriting
preserves already-settled state. Both have differential probes. Constructor-entry
tracking recovers failed creation identity, including nested reverter tracking.
None of these bounded results closes an entire family or L1–L9 gate.

Function-mock checkpoint: exact-calldata/selector redirection precedes expectations,
pranks and response mocks. Eight native cases and nine unchanged MockFunction tests
match reference reported gas. Setup persistence, storage/caller/value identity,
static/revert behavior and registration surviving child revert have bounded evidence.
Delegated identity remains guarded under E2-09; full M2-mock acceptance stays open.
