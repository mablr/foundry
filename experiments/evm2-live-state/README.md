# Historical live-state differential evidence

The retained `evidence.json` records the original 54-case runs at Foundry reference
`295647678` and evm2 `2c8b67f0`: checkpoint-only adapter 30 matches/12 candidate
panics; incomplete native-copy adapter 39 matches/6 candidate panics. With reference
tracing enabled, the reference itself panicked in 36 cross-depth cases. These are
historical results, not the latest checkpoint prototype or a parity allowlist.

The obsolete standalone Cargo workspace, incomplete patch and REVM reference example
were removed from the active Foundry tree after milestone 1. Their source files,
lockfile, generator and original instructions were copied and byte-verified at:

`/Volumes/Stockage/dev-cache/evm2-live-state-pre-milestone1-20260918/`

This is a local archive, not a portable checked-in reproduction package. Build its
reference example against pre-migration Foundry `295647678`, not this branch's native
Executor; otherwise the reference ceases to be a REVM oracle. The original evidence
records binary/input hashes. Generated native sources/results were moved into the archive’s `generated/`
directory; the active tree retains only this note and historical evidence.

The newer replacement-aware checkpoint prototype and its independent runner live at
`/Volumes/Stockage/dev-cache/evm2-live-state-agent/`. It reported 45/54 matches and no
candidate panics. See [requirements](../../docs/dev/evm2-upstream-requirements.md) for
remaining limitations. Foundry continues to depend on the unpatched evm2 pin.
