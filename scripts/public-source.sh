#!/usr/bin/env bash
# Whether a checkout holds the private history: a tripwire publication.yml runs before any build.
# The privacy boundary is the box's leak guard; this only catches pushing this private tree
# itself. A public tree (scripts/public-tree.sh, `git archive`) drops every `export-ignore` path.
#
#   scripts/public-source.sh [<dir>]   (default: the current directory)
#
# Exits 0 when no tracked file in <dir> is marked `export-ignore`, else 1 naming each.
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
