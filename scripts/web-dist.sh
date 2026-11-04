#!/usr/bin/env bash
# The deployed frontend: the pure `.#suzatary-web` derivation, never the checkout's `web/dist`,
# which any dev command overwrites in place. Rooted, not --no-link, as in app-bin.sh.
#
#   scripts/web-dist.sh   print the store path of the built frontend
set -euo pipefail

repo="${SUZATARY_REPO:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
# `roots-dev` without SUZATARY_ROOTS: see scripts/app-bin.sh.
roots="${SUZATARY_ROOTS:-${XDG_STATE_HOME:-$HOME/.local/state}/suzatary/roots-dev}"
mkdir -p "$roots"

# Stdout stays exactly the store path.
before="$(readlink -f "$roots/web-dist" 2>/dev/null || true)"
start="$(date +%s.%N)"
out="$(nix build "$repo#suzatary-web" -o "$roots/web-dist" --print-out-paths)"
elapsed="$(awk -v s="$start" -v e="$(date +%s.%N)" 'BEGIN { printf "%.1f", e - s }')"
if [[ "$before" == "$out" ]]; then status="resolved"; else status="built"; fi
printf 'web-dist: %s in %ss\n' "$status" "$elapsed" >&2

printf '%s\n' "$out"
