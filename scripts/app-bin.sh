#!/usr/bin/env bash
# The deployed program: the pure `.#suzatary` derivation, never app-build.sh's dev binary in a
# mutable app/target/. Every script that must run the real program goes through here.
#
#   scripts/app-bin.sh            print the store path
#   scripts/app-bin.sh <verb...>  resolve and exec suzatary with these args
#   SUZATARY_APP_REV=<sha> scripts/app-bin.sh ...   the same, built from that commit's public tree
set -euo pipefail

repo="${SUZATARY_REPO:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
# Rooted, not --no-link: the daemon collects on its own when the disk is short, between this
# build and the exec. Without SUZATARY_ROOTS (set for the units) this links into `roots-dev`, so
# a dev checkout cannot repoint what the units run.
roots="${SUZATARY_ROOTS:-${XDG_STATE_HOME:-$HOME/.local/state}/suzatary/roots-dev}"
mkdir -p "$roots"

# Whether it built is read off the link (a `nix eval` first would be a second evaluation).
# Stdout stays exactly the store path.
start="$(date +%s.%N)"
# SUZATARY_APP_REV builds that commit's public tree, the store path the gate's deploy-build
# already built, never a working tree.
flake="$repo" link="$roots/app-bin"
if [[ -n "${SUZATARY_APP_REV:-}" ]]; then
  flake="$("$repo/scripts/public-tree.sh" --store "$SUZATARY_APP_REV")"
  link="$roots/app-bin-rev"
fi
before="$(readlink -f "$link" 2>/dev/null || true)"
out="$(nix build "$flake#suzatary" -o "$link" --print-out-paths)"
elapsed="$(awk -v s="$start" -v e="$(date +%s.%N)" 'BEGIN { printf "%.1f", e - s }')"
if [[ "$before" == "$out" ]]; then status="resolved"; else status="built"; fi
printf 'app-bin: %s in %ss\n' "$status" "$elapsed" >&2

if [[ "$#" -eq 0 ]]; then
  printf '%s\n' "$out"
else
  exec "$out/bin/suzatary" "$@"
fi
