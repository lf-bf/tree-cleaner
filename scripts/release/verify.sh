#!/usr/bin/env bash
# semantic-release `verifyConditions` step: fail early when the release job lacks a tool.
set -euo pipefail

command -v cargo >/dev/null 2>&1 || { echo "cargo is required to refresh Cargo.lock" >&2; exit 1; }
command -v perl >/dev/null 2>&1 || { echo "perl is required to edit Cargo.toml" >&2; exit 1; }
test -f Cargo.toml || { echo "Cargo.toml not found" >&2; exit 1; }
test -f Formula/tree-cleaner.rb || { echo "Formula/tree-cleaner.rb not found" >&2; exit 1; }
echo "release preconditions satisfied"
