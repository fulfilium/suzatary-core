#!/usr/bin/env bash
# Everything a unit's start needs from an app tree, built and rooted under one prefix; shared by
# the roots run and scripts/deploy.sh so both build the same derivations.
#
#   scripts/app-builds.sh <app> <prefix>
#
# <app> is the tree's store path. Links: <prefix>src (rooted first), <prefix>bin, <prefix>dist,
# <prefix>plugins, and <prefix>shell (the operator's shell as a profile, current generation only).
set -euo pipefail

if [[ "$#" -ne 2 ]]; then
  echo "usage: scripts/app-builds.sh <app> <prefix>" >&2
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
nix build "$app#suzatary" -o "${at}bin" --print-out-paths
nix build "$app#suzatary-web" -o "${at}dist" --print-out-paths
nix build "$app#suzatary-plugins" -o "${at}plugins" --print-out-paths
nix develop "$app#devShells.$system.triage" --profile "${at}shell" --command true
# A profile grows a generation per build and roots every one; keep the current.
nix profile wipe-history --profile "${at}shell"
