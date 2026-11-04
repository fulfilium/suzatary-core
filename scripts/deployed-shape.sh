#!/usr/bin/env bash
# HEAD's public tree run as a deploy runs it: a read-only store path, files dated 1970, no
# `.git`, a `-source` basename, and the units' PATH with no dev shell. It typesets the decisions
# page, runs `safe-push.sh --dry` over a fresh deployment (no cargo on PATH), and stores a
# calendar's link with `secretspec` beside the read-only tree.
#
#   nix develop .#triage -c scripts/deployed-shape.sh
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

# Under /tmp, not a dev shell's private TMPDIR, so an unprivileged user can reach it.
scratch="$(mktemp -d /tmp/deployed-shape.XXXXXX)"
# A red run's evidence is kept here, the last 10.
evidence="${XDG_STATE_HOME:-$HOME/.local/state}/suzatary/deployed-shape-failures"
leave() {
  local code=$?
  if ((code != 0)); then
    local kept
    kept="$evidence/$(date -u +%Y%m%dT%H%M%SZ)"
    mkdir -p "$kept"
    cp -r "$scratch/state" "$scratch/decisions.html" "$kept/" 2>/dev/null || true
    find "$evidence" -mindepth 1 -maxdepth 1 -type d -printf '%T@ %p\n' | sort -rn |
      tail -n +11 | cut -d' ' -f2- | xargs -r rm -rf
    echo "deployed-shape: FAILED; its evidence is kept in $kept" >&2
  fi
  rm -rf "$scratch"
}
trap leave EXIT

git diff --quiet HEAD -- || echo "deployed-shape: the working tree differs from HEAD; this runs HEAD's tree" >&2
tree="$(scripts/public-tree.sh --store HEAD)"
nix-store --realise "$tree" --add-root "$scratch/tree" --indirect >/dev/null
echo "deployed-shape: HEAD's tree is $tree" >&2

# The deploy's own binary, which the gate's public-tree step built.
suzatary="$(nix build "$tree#suzatary" --out-link "$scratch/bin" --print-out-paths)/bin/suzatary"

# The runtimeDeps of nix/services.nix that the typesetter and the push reach; nothing of this
# shell's PATH.
mapfile -t packages < <(nix build --no-link --print-out-paths --inputs-from . \
  nixpkgs#bash nixpkgs#coreutils nixpkgs#gnugrep nixpkgs#gawk nixpkgs#gnused \
  nixpkgs#gnutar nixpkgs#gzip nixpkgs#git nixpkgs#nix nixpkgs#util-linux)
path=""
for package in "${packages[@]}"; do
  [[ ! -d "$package/bin" ]] || path="${path:+$path:}$package/bin"
done
for tool in typst cargo; do
  if PATH="$path" command -v "$tool" >/dev/null; then
    echo "deployed-shape: $tool is on the units' PATH, so its fallback goes untested" >&2
    exit 1
  fi
done

mkdir -p "$scratch/run"

# A read-only file refuses only an unprivileged writer, so as root this runs the tree as nobody
# through the nix daemon, and fails without one rather than pass a check it did not make.
if [[ "$(id -u)" -eq 0 ]]; then
  if [[ ! -S /nix/var/nix/daemon-socket/socket ]]; then
    echo "deployed-shape: running as root with no nix daemon, so the tree cannot run as an unprivileged user" >&2
    exit 1
  fi
  # Outside the tree (scripts/size.sh walks it) and outside /root (nobody cannot enter it).
  home=/var/tmp/suzatary-deployed-shape-home
  mkdir -p "$home"
  chmod 755 "$scratch"
  chown -R -h 65534:65534 "$scratch"
  # Handed over once, never walked again (cargo's registry is large).
  [[ "$(stat -c %u "$home")" == 65534 ]] || chown -R -h 65534:65534 "$home"
  as=(setpriv --reuid=65534 --regid=65534 --clear-groups)
  user=(NIX_REMOTE=daemon)
else
  home="$HOME"
  as=()
  user=()
  for name in USER LOGNAME XDG_CACHE_HOME; do
    [[ -z "${!name:-}" ]] || user+=("$name=${!name}")
  done
fi
install -m 755 "$suzatary" "$scratch/suzatary"

# Nothing of the shell; its state is this run's, so nothing lands in the box's own.
units=("${as[@]}" env -i "PATH=$path" "HOME=$home" "SUZATARY_REPO=$tree"
  "XDG_STATE_HOME=$scratch/state" "XDG_RUNTIME_DIR=$scratch/run" "TMPDIR=$scratch" "${user[@]}")
for name in NIX_CONFIG HTTPS_PROXY https_proxy HTTP_PROXY http_proxy NO_PROXY no_proxy; do
  [[ -z "${!name:-}" ]] || units+=("$name=${!name}")
done
# Fresh seeds in the environment, which the secret store's reader answers from first, so the
# box's own store is never reached.
seed() { od -An -tx1 -N32 /dev/urandom | tr -d ' \n'; }
units+=("SUZATARY_BOX_KEY=$(seed)" "SUZATARY_AGENT_KEY=$(seed)" "SUZATARY_DEDUP_KEY=$(seed)")
mkdir -m 755 "$scratch/certs"
for name in NIX_SSL_CERT_FILE SSL_CERT_FILE CARGO_HTTP_CAINFO GIT_SSL_CAINFO; do
  [[ -n "${!name:-}" ]] || continue
  install -m 644 "${!name}" "$scratch/certs/$name"
  units+=("$name=$scratch/certs/$name")
done

"${units[@]}" "$scratch/suzatary" --instance "$scratch/data/instance.toml" init \
  --name Ada --city London --zone Europe/London --actor ada \
  --box ada-box.tailnet.ts.net --login her@example.test --repo "$tree" </dev/null >/dev/null

echo "==> the decisions page, typeset through the binary, as uid $("${units[@]}" id -u)" >&2
"${units[@]}" "SUZATARY_INSTANCE=$scratch/data/instance.toml" \
  "$scratch/suzatary" decision view --html "$scratch/decisions.html" ||
  { echo "deployed-shape: the decisions page did not typeset" >&2; exit 1; }

# Dry: it commits nothing and reaches no remote.
echo "==> the data push, dry, from the tree" >&2
"${units[@]}" git -C "$scratch/data" init -q
"${units[@]}" "SUZATARY_INSTANCE=$scratch/data/instance.toml" "SUZATARY_BIN=$scratch/suzatary" \
  "$tree/scripts/safe-push.sh" --dry

# The file `suzatary secretspec` names must lie under the state directory: the tree is
# read-only. The provider is this run's own, so no store of the box's is reached.
echo "==> a calendar's link, stored beside the read-only tree" >&2
"${units[@]}" sh -c 'cat >>"$1"' sh "$scratch/data/instance.toml" <<'CALENDAR'

[[plugins]]
name = "ics"
component = "ics.wasm"
schema = "ics.schema"
origins = ["https://calendar.example.test"]
credentials = ["PLUGIN_ICS_WORK"]
CALENDAR
declared="$("${units[@]}" "SUZATARY_INSTANCE=$scratch/data/instance.toml" "$scratch/suzatary" secretspec)"
case "$declared" in
  "$scratch/state/"*) ;;
  *) echo "deployed-shape: the names are declared in $declared, outside the state directory" >&2; exit 1 ;;
esac
grep -q PLUGIN_ICS_WORK "$declared" ||
  { echo "deployed-shape: $declared does not declare the calendar's link" >&2; exit 1; }
secretspec_bin="$(nix build --no-link --print-out-paths --inputs-from . nixpkgs#secretspec)/bin"
"${units[@]}" mkdir -p "$scratch/secrets-home"
store=("${units[@]}" "PATH=$path:$secretspec_bin" "HOME=$scratch/secrets-home"
  "XDG_CONFIG_HOME=$scratch/secrets-home/.config" "SECRETSPEC_PROVIDER=dotenv://$scratch/secrets.env")
link="https://calendar.example.test/private-0000/basic.ics"
printf '%s\n' "$link" | "${store[@]}" secretspec set PLUGIN_ICS_WORK --file "$declared" \
  --reason "deployed-shape: a calendar's link" >/dev/null ||
  { echo "deployed-shape: secretspec refused the calendar's link" >&2; exit 1; }
[[ "$("${store[@]}" secretspec get PLUGIN_ICS_WORK --file "$declared" --reason "deployed-shape: read back" | tail -n 1)" == "$link" ]] ||
  { echo "deployed-shape: the calendar's link did not read back" >&2; exit 1; }
