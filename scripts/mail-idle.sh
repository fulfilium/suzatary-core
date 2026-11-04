#!/usr/bin/env bash
# IMAP IDLE watcher (`suzatary mail idle`), run by suzatary-mail-idle.service and `nix run
# .#mailIdle`. It never syncs: on a debounced wake it starts suzatary-triage.service, which
# systemd refuses to start twice, so there is one pipeline. Only Microsoft accounts are watched;
# the 5-minute timer is everyone else's path and the fallback.
set -euo pipefail

REPO="${SUZATARY_REPO:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
cd "$REPO"

# Resolved before exec (never `scripts/app-bin.sh mail idle …`) so the path can be rooted.
BIN="$(scripts/app-bin.sh)"

# Root the exact binary while it runs: the `app-bin` link moves on every roots refresh, and a
# collection would otherwise force a rebuild on a wake or restart.
scripts/root-serving.sh app-bin-idle "$BIN"

exec nix develop "$REPO#triage" --command \
  "$BIN/bin/suzatary" mail idle --instance "$SUZATARY_INSTANCE"
