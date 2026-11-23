#!/usr/bin/env bash
# Fails when a rewrite arm of the symbolic executor has no row in the Symbolic inventory.
#
# Every rewrite arm in the Rust sources below carries a comment of the form // `lhs => rhs`.
# Each such rule text must appear verbatim, in backticks, in the inventory of README.md, with its
# Lean theorem or the reason it is not modelled. Run from any directory.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
src="$here/../../crates/evm/symbolic/src/runtime"
readme="$here/README.md"
missing=0
while IFS= read -r rule; do
  if ! grep -qF -- "$rule" "$readme"; then
    echo "missing inventory row: $rule" >&2
    missing=1
  fi
done < <(grep -ho '// `[^`]*=>[^`]*`' "$src/expr/expr.rs" "$src/expr/bool.rs" \
  "$src/solver/normalize/mod.rs" | sed 's|^// ||' | sort -u)
exit "$missing"
