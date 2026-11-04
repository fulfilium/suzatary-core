#!/usr/bin/env bash
# `suzatary up` — converge the fetcher's services and health-check everything.
# It generates no units: they are declared in nix/nixos-module.nix; `up` enables and starts
# what the module declared, then runs the doctor.
set -euo pipefail

export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
export DBUS_SESSION_BUS_ADDRESS="${DBUS_SESSION_BUS_ADDRESS:-unix:path=$XDG_RUNTIME_DIR/bus}"
REPO="$(cd "$(dirname "$0")/.." && pwd)"
FAIL=0
UNITS=()
say() { printf '%-28s %s\n' "$1" "$2"; }
bad() { say "$1" "FAIL — $2"; FAIL=1; }

# Fetcher lock: exactly one box may run the fetcher role. A vault-restored box holds every
# credential, and activating a second fetcher races git pushes, can invalidate the first box's
# MS refresh tokens and duplicates Bridge sessions. `<data>/FETCHER` names the one box; a
# missing file refuses too.
INSTANCE="${SUZATARY_INSTANCE:-$REPO/instance.toml}"
FETCHER_FILE="$(dirname "$(realpath "$INSTANCE" 2>/dev/null || echo "$INSTANCE")")/FETCHER"
if [ ! -f "$FETCHER_FILE" ]; then
  echo "up: no fetcher is declared ($FETCHER_FILE is missing)."
  echo "    Name the one box allowed to fetch in the data repository's FETCHER, commit, re-run."
  exit 1
fi
if [ "$(uname -n)" != "$(tr -d '[:space:]' < "$FETCHER_FILE")" ]; then
  echo "up: this box ($(uname -n)) is not the declared fetcher ($(cat "$FETCHER_FILE"))."
  echo "    Refusing to enable services — a second live fetcher races git pushes and"
  echo "    can invalidate the real fetcher's OAuth and Bridge sessions."
  echo "    To take over the role: edit the data repository's FETCHER, commit, push, re-run."
  exit 1
fi

# The dev shell can shadow enough env that systemctl misreports "no units", so tell "cannot
# reach systemd" from "module absent". Reload first: after a nixos-rebuild the user manager
# holds a stale unit view.
systemctl --user daemon-reload 2>/dev/null || true
if ! systemctl --user show-environment >/dev/null 2>&1; then
  bad "systemd" "cannot reach the user manager (env problem — run outside nix develop or check XDG_RUNTIME_DIR)"
elif systemctl --user list-unit-files 2>/dev/null | grep -q "suzatary-triage"; then
  # Asked of the manager: the bridge and the vault exist only where their options do.
  mapfile -t UNITS < <(systemctl --user list-unit-files --no-legend \
    proton-bridge.service 'suzatary-*.timer' 2>/dev/null | awk '{print $1}')
  for u in "${UNITS[@]}"; do
    systemctl --user enable --now "$u" 2>/dev/null || true
    if systemctl --user is-active --quiet "$u"; then say "$u" "active"; else bad "$u" "not active"; fi
  done
else
  say "systemd units" "not installed (module absent)"
  echo "  -> reboot-persistent services come from the NixOS module:"
  echo "     services.suzatary.enable = true;  (see nix/nixos-module.nix), then nixos-rebuild switch"
  echo "  -> everything below still works hand-run from 'nix develop .#triage'"
fi

AVAIL_MB=$(df -Pm "$REPO" | awk 'NR==2 {print $4}')
[ "$AVAIL_MB" -ge 1024 ] && say "disk" "${AVAIL_MB}MB free" || bad "disk" "only ${AVAIL_MB}MB free"

if (cd "$REPO" && SECRETSPEC_REASON="up: doctor" secretspec check >/dev/null 2>&1); then
  say "secretspec" "all declared secrets present"
else
  bad "secretspec" "missing secrets — run: SECRETSPEC_REASON=check secretspec check"
fi

if (cd "$REPO" && secretspec get CLAUDE_CODE_OAUTH_TOKEN --reason "up: doctor" >/dev/null 2>&1); then
  say "triage auth" "long-lived token present"
else
  say "triage auth" "no long-lived token — triage rides the interactive session (claude setup-token to fix)"
fi

declared() { [[ " ${UNITS[*]:-} " == *" $1 "* ]]; }

if ! declared proton-bridge.service; then
  say "proton bridge" "not declared (services.suzatary.protonBridge.enable)"
elif timeout 5 bash -c 'exec 3<>/dev/tcp/127.0.0.1/1143' 2>/dev/null; then
  say "proton bridge" "IMAP answering on 1143"
else
  bad "proton bridge" "nothing on 127.0.0.1:1143"
fi

if ! declared suzatary-vault.timer; then
  say "proton drive" "no vault declared (services.suzatary.vault.enable)"
elif [ -x "$HOME/bin/proton-drive" ] && "$HOME/bin/proton-drive" filesystem list / >/dev/null 2>&1; then
  say "proton drive" "authed (vault + documents reachable)"
else
  bad "proton drive" "not authed — the declared vault will fail (proton-drive auth login)"
fi

LAST_SYNC="${XDG_STATE_HOME:-$HOME/.local/state}/suzatary/last-sync.json"
if [ -f "$LAST_SYNC" ]; then
  AGE_MIN=$(( ( $(date +%s) - $(stat -c %Y "$LAST_SYNC") ) / 60 ))
  [ "$AGE_MIN" -le 120 ] && say "last sync" "${AGE_MIN}min ago" || bad "last sync" "${AGE_MIN}min ago — hourly triage may be failing"
else
  say "last sync" "no record yet (first run pending)"
fi

if loginctl show-user "$USER" 2>/dev/null | grep -q "Linger=yes"; then
  say "linger" "enabled (services survive logout/reboot)"
else
  say "linger" "disabled — services stop at logout (module sets this; or: loginctl enable-linger $USER)"
fi

exit $FAIL
