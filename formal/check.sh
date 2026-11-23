#!/usr/bin/env bash
# Builds every library and fails on any `sorry`.
set -euo pipefail
cd "$(dirname "$0")"
export PATH="/opt/lean4/bin:$PATH"
lake build "$@" 2>&1 | tee .lake-build.log
if grep -n "declaration uses 'sorry'" .lake-build.log; then
  echo "error: sorry found" >&2
  exit 1
fi
