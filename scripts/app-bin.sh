#!/usr/bin/env bash
# The deployed program, `.#suzatary`, never app-build.sh's dev binary; every script that runs the
# real program goes through here.
#
#   scripts/app-bin.sh            print the store path
#   scripts/app-bin.sh <verb...>  resolve and exec suzatary with these args
#   SUZATARY_APP_REV=<sha> scripts/app-bin.sh ...   that commit's public tree's, fetched, never built
#   scripts/app-bin.sh --deployed [<verb...>]       the deployed app ($SUZATARY_ROOTS/lock-bin)
set -euo pipefail

repo="${SUZATARY_REPO:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"

if [[ "${1:-}" == --deployed ]]; then
  shift
  out="$(readlink -f "${SUZATARY_ROOTS:-${XDG_STATE_HOME:-$HOME/.local/state}/suzatary/roots}/lock-bin")" &&
    [[ -x "$out/bin/suzatary" ]] || { echo "app-bin: no deployed app (scripts/deploy.sh <rev> first)" >&2; exit 1; }
  if [[ "$#" -eq 0 ]]; then printf '%s\n' "$out"; exit 0; fi
  exec "$out/bin/suzatary" "$@"
fi

# Rooted, not --no-link: the daemon collects on its own when the disk is short, between this
# build and the exec. Without SUZATARY_ROOTS (set for the units) this links into `roots-dev`, so
# a dev checkout cannot repoint what the units run.
roots="${SUZATARY_ROOTS:-${XDG_STATE_HOME:-$HOME/.local/state}/suzatary/roots-dev}"
mkdir -p "$roots"

# Whether it built is read off the link; stdout is exactly the store path.
start="$(date +%s.%N)"
# A commit's is its public tree's, which deploy-build and publication.yml build.
flake="$repo" link="$roots/app-bin" fetched=()
if [[ -n "${SUZATARY_APP_REV:-}" ]]; then
  flake="$("$repo/scripts/public-tree.sh" --store "$SUZATARY_APP_REV")"
  link="$roots/app-bin-rev" fetched=(--max-jobs 0)
fi
before="$(readlink -f "$link" 2>/dev/null || true)"
out="$(nix build "$flake#suzatary" "${fetched[@]}" -o "$link" --print-out-paths)"
# Its dependency build, kept: no cache serves it, and the next binary built here reuses it.
deps="$(nix eval --raw "$flake#suzatary-deps.outPath" 2>/dev/null)" || deps=""
[[ -z "$deps" || ! -e "$deps" ]] || nix-store --add-root "$link-deps" --indirect -r "$deps" >/dev/null
elapsed="$(awk -v s="$start" -v e="$(date +%s.%N)" 'BEGIN { printf "%.1f", e - s }')"
if [[ "$before" == "$out" ]]; then status="resolved"; else status="built"; fi
printf 'app-bin: %s in %ss\n' "$status" "$elapsed" >&2

if [[ "$#" -eq 0 ]]; then
  printf '%s\n' "$out"
else
  exec "$out/bin/suzatary" "$@"
fi
