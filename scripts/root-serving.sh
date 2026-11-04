#!/usr/bin/env bash
# Roots a store path a long-running caller is about to run, under a name of its own. The shared
# `app-bin`/`web-dist` links move on every roots refresh, and a collection would then take what
# the process opens later (a subprocess, a static file), though its mapped binary survives.
#
#   scripts/root-serving.sh <root-name> <store-path>
set -euo pipefail

if [[ "$#" -ne 2 ]]; then
  echo "usage: scripts/root-serving.sh <root-name> <store-path>" >&2
  exit 64
fi

name="$1"
path="$2"

roots="${SUZATARY_ROOTS:-${XDG_STATE_HOME:-$HOME/.local/state}/suzatary/roots-dev}"
mkdir -p "$roots"
nix-store --add-root "$roots/$name" --indirect -r "$path" >/dev/null
