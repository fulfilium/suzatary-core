#!/usr/bin/env bash
# Adopt device signatures on this box (SPEC law 42), one checked step at a time.
#
#   scripts/adopt-signatures.sh
#
# Run as the owner on the fetcher after deploying a rev that signs; every verb runs from the
# deployed binary ($SUZATARY_ROOTS/lock-bin). Each step is checked and idempotent, so a stopped
# run is run again. If anything fails after the writers stop, they are started again on exit.
set -euo pipefail

# `systemctl --user` with no runtime directory answers empty rather than failing, so every
# check below would pass while asking nothing.
[ -n "${XDG_RUNTIME_DIR:-}" ] || {
  echo "adopt-signatures: XDG_RUNTIME_DIR is unset, so systemctl --user reaches no manager:" >&2
  echo "  run this from the owner's login session" >&2
  exit 1
}
roots="${SUZATARY_ROOTS:-${XDG_STATE_HOME:-$HOME/.local/state}/suzatary/roots}"
bin="$roots/lock-bin/bin/suzatary"
instance="${SUZATARY_INSTANCE:?SUZATARY_INSTANCE names the instance file the units read}"
instance="$(realpath "$instance")"
data="$(dirname "$instance")"
# The deployed tree, whose secretspec.toml declares the keys, never a checkout that may lag it.
[ -d "$roots/app" ] || {
  echo "adopt-signatures: no deployed tree at $roots/app: deploy first (scripts/deploy.sh <rev>)" >&2
  exit 1
}
SUZATARY_REPO="$(readlink -f "$roots/app")"
export SUZATARY_REPO
# Everything that writes a chain.
writers=(suzatary-web.service suzatary-triage.timer suzatary-mail-idle.service)

step() { echo "==> $*"; }
fail() {
  echo "adopt-signatures: $*" >&2
  exit 1
}
suzatary() { "$bin" --instance "$instance" "$@"; }

step "the deployed binary signs"
[ -x "$bin" ] || fail "no deployed binary at $bin: deploy first (scripts/deploy.sh <rev>)"
suzatary device --help | grep -q sign-unsigned ||
  fail "$bin predates device signatures: deploy this branch's rev first"

declared() { suzatary instance get signatures.root 2>/dev/null || true; }

step "the box's and the agent's keys, minted into secretspec or kept"
minted="$(suzatary device init)"
echo "$minted"
root="$(echo "$minted" | grep -oE '[0-9a-f]{64}' | head -1)"
[ -n "$root" ] || fail "device init named no root"
was="$(declared)"
[ -z "$was" ] || [ "$was" = "$root" ] ||
  fail "the instance declares the root $was and SUZATARY_BOX_KEY is $root: one is not this box's"

stopped=0
restart() {
  local status=$?
  [ "$stopped" -eq 1 ] || return "$status"
  echo "adopt-signatures: starting the writers again" >&2
  systemctl --user start "${writers[@]}" || true
  local down=()
  for unit in "${writers[@]}"; do
    systemctl --user is-active --quiet "$unit" || down+=("$unit")
  done
  if [ "${#down[@]}" -gt 0 ]; then
    echo "adopt-signatures: these writers are still stopped; start them with:" >&2
    for unit in "${down[@]}"; do
      echo "  systemctl --user start $unit" >&2
    done
    [ "$status" -ne 0 ] || status=1
  fi
  exit "$status"
}
trap restart EXIT

step "stop every writer"
systemctl --user stop "${writers[@]}"
stopped=1
for _ in $(seq 1 120); do
  systemctl --user is-active --quiet suzatary-triage.service || break
  sleep 5
done
for unit in "${writers[@]}" suzatary-triage.service; do
  ! systemctl --user is-active --quiet "$unit" || fail "$unit is still running"
done

step "declare the root in $instance"
if [ -z "$was" ]; then
  suzatary instance set signatures.root "$root"
  git -C "$data" add -- "$(basename "$instance")"
  git -C "$data" commit --quiet -m "instance: declare the signing root" -- "$(basename "$instance")"
fi
[ "$(declared)" = "$root" ] || fail "the instance does not declare the root $root"

step "adopt every chain: the keys registered, the epoch taken, pending batches signed"
suzatary device init
suzatary device sign-unsigned ||
  fail "a write since the epoch is unsigned (listed above): check it, then run
  $bin --instance $instance device sign-unsigned --yes
and this script again"

step "start the writers"
systemctl --user start "${writers[@]}"
for unit in "${writers[@]}"; do
  systemctl --user is-active --quiet "$unit" || fail "$unit did not start"
done
stopped=0

step "the deployment signs"
said="$(suzatary doctor 2>&1 || true)"
grep -q "the box's and the agent's keys resolve" <<<"$said" ||
  fail "doctor does not say both keys resolve:
$said"
suzatary device list
suzatary verify || fail "verify refuses what adoption left (above): the push path would too"

step "done"
echo "Enrol the first phone: Settings → This device → Ask to trust this device, then"
echo "  $bin --instance $instance device trust <the key the page shows>"
echo "Every later device: ask, then tap its card in the Inbox on a phone already trusted."
