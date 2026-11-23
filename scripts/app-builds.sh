#!/usr/bin/env bash
# Everything a unit's start needs from an app tree, built and rooted under one prefix; shared by
# the roots run and scripts/deploy.sh so both build the same derivations.
#
#   scripts/app-builds.sh [--cached] <app> <prefix>
#
# <app> is the tree's store path. Links: <prefix>src (rooted first), <prefix>bin, <prefix>dist,
# <prefix>plugins, and <prefix>shell (the operator's shell as a profile, current generation only).
# --cached: the app, the web bundle and the plugins come from a binary cache (`--max-jobs 0`),
# never a build here (publication.yml fills the cache), and one no cache holds exits 75, no
# verdict; the operator's shell is not made, since `nix develop` builds its environment here and
# no cache carries its unfree package: the roots run makes it at the deploy.
set -euo pipefail

# Without --cached a failed build is the build's own red. The roots unit runs this file alone from
# the store, never with --cached, so it sources nothing.
missed() { return 1; }
fetched=()
if [[ "${1:-}" == --cached ]]; then
  fetched=(--max-jobs 0)
  # shellcheck source=scripts/nix-out.sh
  . "$(dirname "${BASH_SOURCE[0]}")/nix-out.sh"
  shift
fi
if [[ "$#" -ne 2 ]]; then
  echo "usage: scripts/app-builds.sh [--cached] <app> <prefix>" >&2
  exit 64
fi
app="$1"
at="$2"

mkdir -p "$(dirname "$at")"
nix-store --add-root "${at}src" --indirect -r "$app" >/dev/null
if [[ ! -e "$app/nix/app-input.nix" ]]; then
  echo "app-builds: $app predates deploys by lock: its scripts write under their own code root. Nothing built." >&2
  exit 1
fi
system="$(nix eval --impure --raw --expr builtins.currentSystem)"
nix build "$app#suzatary" -o "${at}bin" "${fetched[@]}" --print-out-paths || missed "$app#suzatary"
nix build "$app#suzatary-web" -o "${at}dist" "${fetched[@]}" --print-out-paths || missed "$app#suzatary-web"
nix build "$app#suzatary-plugins" -o "${at}plugins" "${fetched[@]}" --print-out-paths || missed "$app#suzatary-plugins"
((${#fetched[@]})) && exit 0
nix develop "$app#devShells.$system.triage" --profile "${at}shell" --command true
# A profile grows a generation per build and roots every one; keep the current.
nix profile wipe-history --profile "${at}shell"
