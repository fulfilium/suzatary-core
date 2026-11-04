#!/usr/bin/env bash
# The key quick.yml keeps the quick-check job's pass under: the hash of the tree entries outside
# docs/, android/ and web/ (bar the web file the Rust tests read).
#
# An empty list is refused: every empty list has the same name and would pass for any commit.
#
#   scripts/ci-names.sh [<commit>]    prints rust=…
set -euo pipefail
rev=${1:-HEAD}

entries=$(git ls-tree -r "$rev" | awk -F'\t' '$2 !~ /^(docs|android)\// &&
  ($2 !~ /^web\// || $2 ~ /^web\/static\/\.well-known\//)')
[ -n "$entries" ] || { echo "ci-names.sh: no tree entries" >&2; exit 1; }
printf 'rust=%s\n' "$(printf '%s\n' "$entries" | sha256sum | cut -c1-40)"
