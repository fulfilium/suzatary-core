#!/usr/bin/env bash
# Suzatary's web app, run by suzatary-web.service and `nix run .#web`. Execs the `.#suzatary`
# binary scripts/app-bin.sh resolves, passing what only run time knows: the tailnet address, the
# TLS certificate and the resolved bundle (`--dist` overrides the instance's `web/dist`).
#
# It binds the tailnet address, never 0.0.0.0, and with none it refuses to start: a loopback
# bind would hand the socket to whatever proxies to it on this box, and an address is not an
# identity. The bind decides who may append, not only who may look.
#
# `tailscale` is not in runtimeDeps, so it resolves through the system profile or not at all.
set -euo pipefail

REPO="${SUZATARY_REPO:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
cd "$REPO"

PORT="${SUZATARY_WEB_PORT:-8787}"

tailscale_bin=""
if command -v tailscale >/dev/null 2>&1; then
  tailscale_bin="tailscale"
elif [ -x /run/current-system/sw/bin/tailscale ]; then
  tailscale_bin="/run/current-system/sw/bin/tailscale"
fi

HOST=""
if [ -n "$tailscale_bin" ]; then
  HOST="$("$tailscale_bin" ip -4 2>/dev/null | head -n1 || true)"
fi

if [ -z "$HOST" ]; then
  echo "serve-web: no tailscale IPv4 (binary or tailnet missing); refusing to start rather than bind loopback." >&2
  echo "serve-web: bring tailscaled up and the unit starts on its next try." >&2
  exit 1
fi

# A `tailscale serve` or funnel on this port would put a proxy before the gate; refuse to start
# beside one, so the mistake is a failed unit, not a log.
if served="$("$tailscale_bin" serve status 2>&1)"; then
  if grep -Eq ":${PORT}([^0-9]|$)" <<<"$served"; then
    echo "serve-web: \`tailscale serve status\` names port $PORT; refusing to start behind a proxy:" >&2
    echo "$served" >&2
    exit 1
  fi
else
  echo "serve-web: could not read \`tailscale serve status\` ($served); the gate still refuses proxied requests" >&2
fi

# A resolved store path, never this checkout's `web/dist`, which any dev command overwrites in
# place under a running server.
DIST="$(scripts/web-dist.sh)"
BIN="$(scripts/app-bin.sh)"
# A tapped link card is checked by syncing its plugin once (`commands::serve::checks`).
roots="${SUZATARY_ROOTS:-${XDG_STATE_HOME:-$HOME/.local/state}/suzatary/roots-dev}"
mkdir -p "$roots"
SUZATARY_PLUGINS="$(nix build "$REPO#suzatary-plugins" -o "$roots/plugins" --print-out-paths)"
export SUZATARY_PLUGINS

# Roots of this unit's own for what it serves: the shared links move on every roots refresh,
# and a collection would then 404 the static files under a running server.
scripts/root-serving.sh web-dist-serving "$DIST"
scripts/root-serving.sh app-bin-serving "$BIN"
scripts/root-serving.sh plugins-serving "$SUZATARY_PLUGINS"
# Minted by `tailscale cert`. Without it, refuse: plain http carries mail in the clear and lets
# a page another name resolves here talk to the app.
TLS_DIR="${SUZATARY_TLS_DIR:-/var/lib/suzatary/tls}"
if ! [ -r "$TLS_DIR/cert.pem" ] || ! [ -r "$TLS_DIR/key.pem" ]; then
  echo "serve-web: no readable $TLS_DIR/{cert,key}.pem; refusing to serve plain http on $HOST. Mint it with \`tailscale cert\`." >&2
  exit 1
fi
echo "serve-web: suzatary serve https://$HOST:$PORT (reads GET, writes POST /api/act/*, gated by tailscale whois)" >&2
# glibc keeps what a request freed in each thread's arena, so a request that reads a whole store
# leaves about its size resident; two arenas hold the server near 1 GB instead of 1.6.
export MALLOC_ARENA_MAX=2
# The `.#triage` shell supplies typst and notmuch, which `serve` runs as subprocesses.
exec nix develop "$REPO#triage" --command \
  "$BIN/bin/suzatary" serve --instance "$SUZATARY_INSTANCE" \
  --host "$HOST" --port "$PORT" --dist "$DIST" \
  --cert "$TLS_DIR/cert.pem" --key "$TLS_DIR/key.pem"
