#!/usr/bin/env bash
# The wasm modules <rev> ships, as its gate's run built them, fetched and never compiled: the
# Nix system on the first line, then each module's file, as `train context record` takes them
# after `--wasm`. Publication pushes a landed tree's modules to the binary cache.
#
#   scripts/wasm-modules.sh <rev>
set -euo pipefail

scripts="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
(($# == 1)) || { echo "usage: scripts/wasm-modules.sh <rev>" >&2; exit 64; }
tree="$("$scripts/public-tree.sh" --store "$1")"
system="$(nix eval --raw --impure --expr builtins.currentSystem)"
# --max-jobs 0: a module no store or cache holds is an error here, never a build.
if ! outs="$(nix build --max-jobs 0 --no-link --print-out-paths \
  "$tree#suzatary-wasm" "$tree#suzatary-views-wasm" "$tree#suzatary-typst-wasm")"; then
  echo "wasm-modules.sh: no cache holds the modules of $1 for $system; dispatch its publication run (gh workflow run publication.yml) and build again" >&2
  exit 1
fi
echo "$system"
for out in $outs; do
  printf '%s\n' "$out"/web/*_bg.wasm
done
