#!/usr/bin/env bash
# WHETHER A CHECKOUT HOLDS THE PRIVATE HISTORY: a tripwire that master's
# .github/workflows/publication.yml runs over the tree it builds, before any build. The privacy
# boundary is the box's leak guard, which passes before a tree is pushed to core; this catches
# only the gross mistake of pushing this private repository's own tree. scripts/public-tree.sh
# writes a public tree with `git archive`, which leaves out every path `.gitattributes` marks
# `export-ignore` (the private history), so this repository's checkout fails and a public tree,
# which holds no `.gitattributes`, passes whatever else it holds.
#
#   scripts/public-source.sh [<dir>]   (default: the current directory)
#
# It exits 0 when no file git tracks in <dir> is marked `export-ignore`, and 1 naming each one
# that is.
set -euo pipefail

if [[ "$#" -gt 1 ]]; then
  echo "usage: scripts/public-source.sh [<dir>]" >&2
  exit 64
fi
dir="${1:-.}"

# check-attr -z prints <path> NUL <attribute> NUL <value> NUL for each file.
held="$(git -C "$dir" ls-files -z | git -C "$dir" check-attr -z --stdin export-ignore |
  awk 'BEGIN { RS = "\0" } NR % 3 == 1 { path = $0 } NR % 3 == 0 && $0 == "set" { print path }')"
if [[ -n "$held" ]]; then
  while IFS= read -r path; do
    echo "public-source: $path is marked export-ignore, so this checkout holds the private history and is no public tree" >&2
  done <<<"$held"
  exit 1
fi
echo "public-source: no file of $dir is marked export-ignore: a public tree"
