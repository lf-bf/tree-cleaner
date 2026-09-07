#!/usr/bin/env bash
# semantic-release `prepare` step. Receives the next version and updates every file that
# carries it, so the release commit and its tag are self-consistent:
#   - Cargo.toml   [package] version
#   - Cargo.lock   entry of this package
#   - Formula/tree-cleaner.rb   `tag:` (Homebrew builds from the release tag)
set -euo pipefail

version="${1:?usage: prepare.sh <version>}"
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

# Only the line-anchored `version = "..."` belongs to [package]; dependency versions live in
# inline tables (`{ version = "1" }`) and are left alone.
perl -pi -e 's/^version = "[^"]*"/version = "'"$version"'"/ if !$done && ($done = /^version = /)' Cargo.toml
cargo update --workspace --quiet

perl -pi -e 's/tag: "v[^"]*"/tag: "v'"$version"'"/' Formula/tree-cleaner.rb

grep -q "^version = \"$version\"" Cargo.toml || { echo "Cargo.toml was not updated" >&2; exit 1; }
grep -q "tag: \"v$version\"" Formula/tree-cleaner.rb || { echo "Formula was not updated" >&2; exit 1; }
echo "prepared release v$version"
