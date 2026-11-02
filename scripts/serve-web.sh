#!/usr/bin/env bash
# Suzatary's web app. Invoked by the systemd user unit suzatary-web.service
# (nix/nixos-module.nix) and by `nix run .#web` (nix/services.nix, which supplies
# PATH, resolves $SUZATARY_REPO and gates on a reachable substituter).
#
# WHAT RUNS IS THE RUST BINARY (docs/ARCHITECTURE.md §8.2), and it is the
# PURE `.#suzatary` derivation — scripts/app-bin.sh resolves it (building on
# demand, cached by tree) and this execs the resolved path. The deployed
# program is always this on-demand build, never scripts/app-build.sh's dev
# binary. It
# reads `instance.toml` for everything this script does not pass, and what
# this script DOES pass is what it resolves at run time and a file cannot
# know: the tailnet address, whether `tailscale cert` has minted a
# certificate yet, and the frontend bundle's
# resolved store path — `instance.toml`'s own `dist` stays a CodeRoot-relative
# `web/dist` for a developer's local `suzatary serve`, and `--dist` overrides
# it here with the immutable path `scripts/web-dist.sh` resolved.
#
# WHY IT BINDS THE TAILNET AND NOT 0.0.0.0. The app is private; "who can reach
# the socket" is the first half of its access control, and the gate (whois
# against the instance file's `web.viewers`) is the second, because the tailnet
# is shared. The app also WRITES, and the daemon's answer is what names the
# actor (`web.act_as`) — so the bind address is not merely who may look, it is
# who may append. The tailnet is the one network the owner's phone and this
# box share, so that address is the exposure this app is built for. With no
# tailnet address it REFUSES TO START: a loopback bind would hand the socket to
# whatever on this box proxies to it (a `tailscale serve`, a funnel, a reverse
# proxy), and an address is not an identity. The unit fails visibly instead.
#
# `tailscale` is deliberately looked up rather than assumed: it is not in
# nix/services.nix's runtimeDeps, so under the systemd unit's PATH this
# resolves through the system profile or not at all, and "not at all" is the
# refusal above.
set -euo pipefail

# SUZATARY_REPO is set by the systemd unit (nix/nixos-module.nix, the ONE
# place this box says where it is); the fallback resolves this script's own
# checkout, so it is runnable by hand from any clone rather than assuming
# this one's (ROADMAP.md, "a second host").
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

# A `tailscale serve` (or a funnel) pointed at this port would put a proxy
# between every reader and the gate. The gate refuses what a proxy forwards;
# this refuses to start beside one, so the mistake is a failed unit, not a log.
if served="$("$tailscale_bin" serve status 2>&1)"; then
  if grep -Eq ":${PORT}([^0-9]|$)" <<<"$served"; then
    echo "serve-web: \`tailscale serve status\` names port $PORT; refusing to start behind a proxy:" >&2
    echo "$served" >&2
    exit 1
  fi
else
  echo "serve-web: could not read \`tailscale serve status\` ($served); the gate still refuses proxied requests" >&2
fi

# THE BUNDLE IS A BUILD, LIKE THE DOCUMENT AND THE BINARY — RESOLVED, never
# built into `web/dist` in
# this checkout. That directory is gitignored and any dev command (a `cargo
# test`, a stray `build-web.sh --dev`) overwrites it in place; a running
# server holding it open would then serve whatever the checkout was mid-edit.
# `scripts/web-dist.sh` resolves and ROOTS `.#suzatary-web` under
# `$SUZATARY_ROOTS` (the unit's) exactly as `app-bin.sh` does for the
# binary — the store path it prints is immutable, so nothing this box runs
# afterwards can clobber what is being served.
DIST="$(scripts/web-dist.sh)"
BIN="$(scripts/app-bin.sh)"
# The reference plugins, as scripts/plugin-sync.sh resolves them: a link stored
# from a card is checked at its tap by syncing its plugin once
# (`commands::serve::checks`), which reads them from SUZATARY_PLUGINS.
roots="${SUZATARY_ROOTS:-${XDG_STATE_HOME:-$HOME/.local/state}/suzatary/roots-dev}"
mkdir -p "$roots"
SUZATARY_PLUGINS="$(nix build "$REPO#suzatary-plugins" -o "$roots/plugins" --print-out-paths)"
export SUZATARY_PLUGINS

# WHAT THIS PROCESS SERVES STAYS ROOTED WHILE IT RUNS. The `web-dist`/`app-bin`
# links under the state directory move whenever the roots refresh
# (`suzatary-roots.service`, on every commit), and the daemon collects whatever
# they no longer name when the disk is tight, so static files would 404 while
# `/api/about` answers from the binary in memory. So this script registers its
# OWN roots for the exact paths
# it is about to exec and serve, named for this unit and replaced only on
# the next start; a refresh behind a running server then moves the shared
# links and collects nothing the server reads. `scripts/root-serving.sh` is
# the one spelling of that registration — `scripts/mail-idle.sh` uses it too.
scripts/root-serving.sh web-dist-serving "$DIST"
scripts/root-serving.sh app-bin-serving "$BIN"
scripts/root-serving.sh plugins-serving "$SUZATARY_PLUGINS"
# TLS: the tailnet's own certificate for this box's MagicDNS name, minted by
# `tailscale cert` into $SUZATARY_TLS_DIR (cert.pem, key.pem). Without it this
# REFUSES TO START: plain http to a tailnet address carries mail in the clear
# and lets a page another name resolves here talk to the app, and
# `suzatary serve` refuses it too. The unit starts once the cert is minted.
TLS_DIR="${SUZATARY_TLS_DIR:-/var/lib/suzatary/tls}"
if ! [ -r "$TLS_DIR/cert.pem" ] || ! [ -r "$TLS_DIR/key.pem" ]; then
  echo "serve-web: no readable $TLS_DIR/{cert,key}.pem; refusing to serve plain http on $HOST. Mint it with \`tailscale cert\`." >&2
  exit 1
fi
echo "serve-web: suzatary serve https://$HOST:$PORT (reads GET, writes POST /api/act/*, gated by tailscale whois)" >&2
# TWO MALLOC ARENAS. glibc gives each thread, up to eight per core, an arena of
# its own and keeps what a request freed there, so a request that reads a whole
# store on one of the server's blocking threads leaves about that store's size
# resident after it ends. At the live shape, over the same rounds of the
# page's requests, the server settled at 1.6 GB with glibc's default and at
# 1.04 GB with two arenas (servememory, 2026-11-02).
export MALLOC_ARENA_MAX=2
# scripts/app-bin.sh resolves the `.#suzatary` derivation's store path and
# execs it with these args; the `.#triage` shell wrap stays because `serve`
# reaches typst as a subprocess for the pages it renders and notmuch for
# /api/mail/*, both of which are that shell's packages and not this binary's.
exec nix develop "$REPO#triage" --command \
  "$BIN/bin/suzatary" serve --instance "$SUZATARY_INSTANCE" \
  --host "$HOST" --port "$PORT" --dist "$DIST" \
  --cert "$TLS_DIR/cert.pem" --key "$TLS_DIR/key.pem"
