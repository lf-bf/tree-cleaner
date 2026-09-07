#!/usr/bin/env bash
# semantic-release `publish` step. Hands the released version to the following GitHub
# Actions jobs (binaries are built per platform after the tag exists).
set -euo pipefail

version="${1:?usage: publish.sh <version>}"
if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
  {
    echo "released=true"
    echo "version=$version"
  } >> "$GITHUB_OUTPUT"
fi
echo "released v$version"
