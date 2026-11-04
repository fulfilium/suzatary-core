#!/usr/bin/env bash
# `suzatary plugin sync`: every `[[plugins]]` source into the cache tier, and every actor's
# proposals filed in the Inbox. Run by suzatary-plugins.service. Binary and components are
# rooted, so a collection cannot take them between build and read.
#
# No lock: a sync is monotone and idempotent, under each plugin store's `store.lock` and the
# Inbox's own lock.
set -euo pipefail

REPO="${SUZATARY_REPO:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
cd "$REPO"

roots="${SUZATARY_ROOTS:-${XDG_STATE_HOME:-$HOME/.local/state}/suzatary/roots-dev}"
mkdir -p "$roots"

BIN="$(scripts/app-bin.sh)"
PLUGINS="$(nix build "$REPO#suzatary-plugins" -o "$roots/plugins" --print-out-paths)"

exec "$BIN/bin/suzatary" plugin sync --components "$PLUGINS" "$@"
