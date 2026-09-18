# M2 remaining assessment register

Source audit: 2026-09-18. All items below remain **open**. This supplements the
[23 capability families and L1–L9](README.md), rather than defining a smaller M2.
The generated CSVs enumerate source surfaces; neither lexical extraction nor this
review proves all reachable paths. No additional engine blocker is asserted here.

## What is and is not covered by the inventory

| Surface | Enumerated evidence | Remaining assessment |
| --- | --- | --- |
| ABI | 609 IDs assigned; 564 M2 | Review canonical signatures/selectors, overload-specific decoding and guards. Names/count agreement with the spec is weaker than ABI equivalence. |
| Reference state | 68 Cheatcodes fields | Review nested types, mutation sites and ownership, including interior mutability, clone/reset behavior and host resources. A field assigned to a family is not a reviewed lifetime. |
| Configuration | 58 CheatsConfig/stack fields | Expand nested evm_opts/configuration used by M2; distinguish applied, rejected, irrelevant and silently ignored settings. |
| Native state | native-fields.csv | Specify creation, consumption and clearing on every exit; assess clone-in/result-out ownership and loss on early errors. |
| Callbacks | 41 named definitions | Trace helpers, stack forwarding and engine invocation order. Names miss hooks such as on_revert, executor postprocessing and generated dispatch. |
| Existing tests | Candidate files by function name | Identify exact test functions and assertions, overload and transition IDs, runtime prerequisites and skips. Comments/interface declarations are not coverage. |
| Executed evidence | Transition ledger and fixture README | Link evidence to individual obligations; no complete accepted-contract denominator exists yet. |

## Boundary checks from the current source

Reference roots: `crates/cheatcodes/src/inspector.rs` and
`crates/evm/evm/src/inspectors/stack.rs`. Native roots:
`crates/cheatcodes/src/native.rs` and `crates/evm/evm/src/executors/evm2.rs`.
Use symbols below rather than unstable line numbers; generated CSVs retain current
source locations and hashes.

| ID / gates | Static observation | Required closure evidence |
| --- | --- | --- |
| A01 / L1 | dispatch orders metadata/deprecation, restriction, shared helpers, expectations, mocks/pranks and state operations. | Malformed/truncated ABI, unknown selector, restricted supported/unsupported calls, alias failure and caught unsupported call; exact bytes plus sticky host classification. Verify ordering against apply_cheatcode/apply_dispatch. |
| A02 / L2 | execute_evm2 clones selected fields in and writes selected fields back; other Cheatcodes fields remain from the original clone. | For each state-fields row: native owner or explicit deferred owner; setup→test, independent tests, call vs transact, root/child revert and host-error policy. Include pending expectations/templates and mock queue consumption. |
| A03 / L2–L4 | MigrationInspector has constructor_frames, log_opcode_depth, log_errors and a single failed_frame_gas slot. | Nested/repeated depths, parent suspension, pre-interpreter rejection, constructor halt/deposit failure and callback reentry must not consume stale or wrong-frame state. Source shape is a risk to test, not proof of a bug. |
| A04 / L2/L7 | Session.signatures starts empty; diagnostics are per-dispatch; neither simply round-trips like expectation state. | Decide cache and diagnostic lifetime explicitly; exercise artifact decoding after setup, multiple errors and early exit. Audit nested reference caches, RNG, wallets, serializers and open file handles before porting them. |
| A05 / L3/L5 | finish_message restores prank state, rewrites expected failures, verifies emits, then root calls/emits/creates. Some paths return early. | Ordered trace for success/revert/halt and conflicting expectations; original vs rewritten failure, counts and gas. Include cheatcode/console/mocks, empty code, precompiles and each CALL/CREATE scheme. |
| A06 / L3–L5 | LOG errors stop at step_end; ordinary expectation mismatch may be generated after settlement. | Compare state, diagnostic recordings, EVM receipt logs and later execution for immediate versus end-of-frame failure. Include failed LOG, non-opcode logs and caught errors; establish applicability for system/precompile logs at the pinned engine. |
| A07 / L5/L6 | Native create delegates entry to call logic, then tracks constructor entry and reads settled code for matching. | Empty successful runtime, nested CREATE/CREATE2, collision/funds/depth/nonce rejection, invalid/oversized runtime, failed deposit; grants, nonce, matching order and interaction with mocks/pranks/emits/expectRevert. |
| A08 / L5/L7 | BackendReads forwards DB errors; state helpers convert access errors into cheatcode Error; Writes converts native deltas into legacy state. | Inject account/code/slot/blockhash failures; determine reference classification at each boundary, caught-error behavior and whether any writes/inspector state escape. Test absent/empty/deleted/recreated accounts and original slot values. Do not assume every reference helper uses the same error policy. |
| A09 / L7/L8 | execute_evm2 copies selected cfg fields into Version, builds a zero-price legacy envelope, then returns the original tx_env. | Account for every M2-relevant option and mutable env field in both directions. Validate contract-visible values, returned environment and next execution. Nonzero synthetic gas price is M2 work; full real envelopes remain M4. |
| A10 / L8 | roll rejects Prague+; etch rejects history storage; broadcast, access/authorization/blob input and several observers are gated. | Resolve required M2 branches or name the split handoff. Test registration/consumption for access lists, delegations/blobs and broadcast collection; an M4 envelope gate cannot excuse missing M2 registration. |
| A11 / L5 | Reference on_revert repairs deal balances and defers cleanup around expectRevert. It is absent from the named callback extraction. | Review this and other transitive cleanup helpers with native settlement; include mutations followed by child transfer and enclosing revert. E2-06 assessment owns required engine behavior. |
| A12 / L7/L9 | RawCallResult mixes native output/gas/state with cloned legacy metadata and defaults; cancellation is adapter state. | Audit each consumed result field and execution error exit. Cancellation before entry, during child execution and after writes must classify and discard/commit correctly. Explicitly bound host/precompile blocking; test console/recorded-log behavior separately from deferred full tracing. |

## Ordered validation backlog

1. **Reachability and harness:** verify generated inventory, enumerate actual test
   IDs, pin engine/reference/compiler/options and prove each selected test executes.
   Record blocked prerequisites instead of dropping tests. Add exact mappings for
   existing positive and intentional-failure fixtures first (A01, L1).
2. **Already-routed semantics:** audit all 303 routes and the two conditional routes
   against reference branches, not just happy paths. Finish assertion boundaries,
   expectation counts/ordering/identity, prank cleanup, mock priority and stateful
   setup handoff. Use A02–A07 to choose adversarial interaction tests.
3. **Missing host surfaces:** environment/filesystem/RPC/FFI/prompts, signing/cache,
   serializers, utilities and randomness. Reuse shared logic where possible; use
   temp resources and deterministic stubs for side effects/errors. Check config,
   permissions, speculative execution and independent-test lifetime.
4. **Missing state and lifecycle surfaces:** account/env mutations, mockFunction,
   access/state/mapping recording, deployment/factory helpers and broadcast
   collection. Validate native journal behavior, output conversion and registration
   handoffs before enabling routes (A08–A11).
5. **Per-opcode and control surfaces:** gas pause/reset/snapshots, memory safety,
   arbitrary storage, storage callbacks, assume/skip. Test failed opcodes, nested
   suspension/reentry, gas/refunds and callback suppression. These remain substantial
   even when selector reachability becomes high.
6. **Acceptance consolidation:** finish L1–L9, engine gaps required by M2,
   nondefault configuration and host/cancellation failures. Rerun unchanged
   applicable fixtures and adversarial regressions with recorded exclusions.
   M3/M4 handoffs remain explicit; they do not certify an M2 contract.

## Evidence required to close an obligation

Give each assertion a stable ID and record: reference symbol/branch; precondition;
transition; observable postcondition; native owner; exact test function and overloads;
matched execution settings; result/evidence location; remaining restrictions.
For every relevant dimension (same/child/ancestor depth, setup/test lifetime,
success/revert/halt/host failure, original/rewritten outcome), provide a test or a
reasoned non-applicability entry. Shared implementation is a reason to share a
semantic test, not to omit distinct ABI decoding tests.

Statuses should distinguish **unassessed**, **contract reviewed**, **implemented**,
**validated**, and **deferred with owner**. Only validated applicable contracts count
as accepted. This register currently identifies the work; it does not manufacture
per-contract coverage numbers. M2 closes only under all six completion rules in
README.md, with no unresolved required route or lifecycle gate.
