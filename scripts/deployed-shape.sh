#!/usr/bin/env bash
# THE TREE AS A DEPLOY RUNS IT. A deploy hands the units a commit's tree as a
# store path: read-only, every file dated 1970, no `.git`, a basename ending
# `-source`, and a PATH holding no dev shell (nix/services.nix runtimeDeps).
# The gate itself runs from a git checkout inside `nix develop .#triage`, so a
# fault only that shape shows passed it three times on 2026-10-05: a file
# copied with the store's mode (#83), a store path handed to `nix develop` as
# a derivation (#84), the guards needing a `.git` (#86).
#
# So this puts HEAD's public tree in the store as the deploy does, and runs from it:
#   site    `suzatary web`, twice into one directory, with the code root that
#           tree and typst reached through its shell;
#   push    `<tree>/scripts/safe-push.sh --dry` over a fresh deployment's data
#           repository, as the triage pass runs it: no cargo on its PATH, so a
#           push that reached for one is red here;
#   link    a calendar connected and its secret link stored by `secretspec`,
#           through the file the binary declares its names in, since the
#           tree's own `secretspec.toml` is read-only and declares none of a
#           plugin's.
# The suite reads no deployed tree, and the gate has run it already.
#
#   nix develop .#triage -c scripts/deployed-shape.sh
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

# Under /tmp, not a dev shell's private TMPDIR, so an unprivileged user can reach it.
scratch="$(mktemp -d /tmp/deployed-shape.XXXXXX)"
# A red run's evidence (the site's pages, the state it wrote) outlives the
# scratch directory, in a directory of this step's own, the last 10 kept; its
# path is the step log's last line.
evidence="${XDG_STATE_HOME:-$HOME/.local/state}/suzatary/deployed-shape-failures"
leave() {
  local code=$?
  if ((code != 0)); then
    local kept
    kept="$evidence/$(date -u +%Y%m%dT%H%M%SZ)"
    mkdir -p "$kept"
    cp -r "$scratch/state" "$scratch/site" "$kept/" 2>/dev/null || true
    find "$evidence" -mindepth 1 -maxdepth 1 -type d -printf '%T@ %p\n' | sort -rn |
      tail -n +11 | cut -d' ' -f2- | xargs -r rm -rf
    echo "deployed-shape: FAILED; its evidence is kept in $kept" >&2
  fi
  rm -rf "$scratch"
}
trap leave EXIT

# THE TREE: HEAD's public tree, the one a deployment fetches from
# suzatary-core (scripts/public-tree.sh), added to the store and rooted for
# this run.
git diff --quiet HEAD -- || echo "deployed-shape: the working tree differs from HEAD; this runs HEAD's tree" >&2
tree="$(scripts/public-tree.sh --store HEAD)"
nix-store --realise "$tree" --add-root "$scratch/tree" --indirect >/dev/null
echo "deployed-shape: HEAD's tree is $tree" >&2

# THE BINARY: the deploy's own, `.#suzatary` of that tree, which the gate's
# public-tree step built (an evaluation here, rooted for this run), so the
# program run is the one a deployment runs, and no debug build is made for it.
suzatary="$(nix build "$tree#suzatary" --out-link "$scratch/bin" --print-out-paths)/bin/suzatary"

# THE UNITS' PATH: the packages of nix/services.nix's runtimeDeps that the
# site and the push reach (the agent's and the vault's tools left out), from
# the flake's nixpkgs, each whole as makeBinPath puts it, so neither the dev
# shell nor anything else on this shell's PATH is on it.
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

# The units' runtime directory, as systemd gives each user unit one.
mkdir -p "$scratch/run"

# THE UNITS' USER. A read-only file refuses only an unprivileged writer, so
# a gate running as root (a cloud sandbox) runs the tree as nobody, through
# the nix daemon as the box's units do, with a home of its own the next run
# keeps (cargo's registry); without a daemon it cannot,
# and fails rather than pass a check it did not make.
if [[ "$(id -u)" -eq 0 ]]; then
  if [[ ! -S /nix/var/nix/daemon-socket/socket ]]; then
    echo "deployed-shape: running as root with no nix daemon, so the tree cannot run as an unprivileged user" >&2
    exit 1
  fi
  # Outside the tree, which scripts/size.sh walks, and outside /root, which
  # that user cannot enter.
  home=/var/tmp/suzatary-deployed-shape-home
  mkdir -p "$home"
  chmod 755 "$scratch"
  chown -R -h 65534:65534 "$scratch"
  # What that user wrote there is its own already: the home is handed over
  # once, never walked again (cargo's registry is large).
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

# THE UNITS' ENVIRONMENT: what systemd gives a user unit, the module's
# SUZATARY_* and what reaching the network takes here; nothing of the shell.
# Its state is this run's, so nothing it writes lands in the box's own.
units=("${as[@]}" env -i "PATH=$path" "HOME=$home" "SUZATARY_REPO=$tree"
  "XDG_STATE_HOME=$scratch/state" "XDG_RUNTIME_DIR=$scratch/run" "TMPDIR=$scratch" "${user[@]}")
for name in NIX_CONFIG HTTPS_PROXY https_proxy HTTP_PROXY http_proxy NO_PROXY no_proxy; do
  [[ -z "${!name:-}" ]] || units+=("$name=${!name}")
done
# THE BOX'S KEYS, as its secret store holds them: `init` mints the box's, the
# agent's and the dedup key through the store, and the units' PATH here carries
# no secretspec (the store is the box's own, never a gate's), so the fresh
# deployment's seeds are handed over in the environment, which the store's
# reader answers from first. Fresh each run, and this run's alone.
seed() { od -An -tx1 -N32 /dev/urandom | tr -d ' \n'; }
units+=("SUZATARY_BOX_KEY=$(seed)" "SUZATARY_AGENT_KEY=$(seed)" "SUZATARY_DEDUP_KEY=$(seed)")
# A certificate bundle a proxy needs is copied where the units' user can read it.
mkdir -m 755 "$scratch/certs"
for name in NIX_SSL_CERT_FILE SSL_CERT_FILE CARGO_HTTP_CAINFO GIT_SSL_CAINFO; do
  [[ -n "${!name:-}" ]] || continue
  install -m 644 "${!name}" "$scratch/certs/$name"
  units+=("$name=$scratch/certs/$name")
done

# A deployment to render the site from, made by the binary itself.
"${units[@]}" "$scratch/suzatary" --instance "$scratch/data/instance.toml" init \
  --name Ada --city London --zone Europe/London --actor ada \
  --box ada-box.tailnet.ts.net --login her@example.test --repo "$tree" </dev/null >/dev/null

echo "==> the site, twice into one directory, as uid $("${units[@]}" id -u)" >&2
for round in 1 2; do
  "${units[@]}" "SUZATARY_INSTANCE=$scratch/data/instance.toml" \
    "$scratch/suzatary" web "$scratch/site" ||
    { echo "deployed-shape: site round $round failed" >&2; exit 1; }
done

# The push its triage pass ends with, dry: the data repository is the fresh
# deployment's, whose chains hold their `Genesis`, so there is something to
# stage, and `suzatary verify` of every store runs. It commits nothing and
# reaches no remote.
echo "==> the data push, dry, from the tree" >&2
"${units[@]}" git -C "$scratch/data" init -q
"${units[@]}" "SUZATARY_INSTANCE=$scratch/data/instance.toml" "SUZATARY_BIN=$scratch/suzatary" \
  "$tree/scripts/safe-push.sh" --dry

# A calendar connected as AGENT.md §4.11 has it, its table in the instance file, and its link
# stored as `./suzatary secret` stores it: `secretspec set` against the file `suzatary
# secretspec` names, which must lie under the state directory, since the tree is read-only.
# secretspec is the units' (nix/services.nix runtimeDeps), on this step's PATH alone, over a
# provider of this run's own, so no store of the box's is reached.
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
