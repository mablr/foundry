---
forge: minor
foundry-evm: minor
foundry-cheatcodes: minor
---

Added native evm2 support for selected environment and state cheatcodes, assertions,
revert and call expectations, mocked calls, log recording, console output, and
sender/delegate pranks. The migration remains
incomplete; unsupported workflows fail explicitly.

Enabled stateless string, base64, version, selected crypto, utility and typed JSON/TOML cheatcodes in native execution, and preserved unknown-selector diagnostics.

Added native CREATE/CREATE2 expectations with settled-code matching and constructor-failure identity tracking.

Preserved innermost constructor reverter tracking and reset it between counted CREATE revert expectations.

Enabled native event expectations with shared matching, decoded mismatch diagnostics and immediate LOG-failure rollback.
