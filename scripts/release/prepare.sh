#!/usr/bin/env bash
# semantic-release `prepare` step. Receives the next version and updates every file that
# carries it, so the release commit and its tag are self-consistent:
#   - Cargo.toml   [package] version
#   - Cargo.lock   entry of this package
#
# The Homebrew formula is not touched here: it carries the checksums of the release
# binaries, which only exist once the release is published (scripts/release/update-formula.sh).
set -euo pipefail

version="${1:?usage: prepare.sh <version>}"
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

# Only the line-anchored `version = "..."` belongs to [package]; dependency versions live in
# inline tables (`{ version = "1" }`) and are left alone.
perl -pi -e 's/^version = "[^"]*"/version = "'"$version"'"/ if !$done && ($done = /^version = /)' Cargo.toml
cargo update --workspace --quiet

grep -q "^version = \"$version\"" Cargo.toml || { echo "Cargo.toml was not updated" >&2; exit 1; }
echo "prepared release v$version"
