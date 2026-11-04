#!/usr/bin/env bash
# Checks that a release or docs change (app/release.toml, CHANGELOG.md, CLAUDE.md, docs/) leaves
# `.#suzatary-bin`'s derivation unchanged, else every train recompiles it, and moves
# `.#suzatary`'s, else the release no longer reaches `--version` and `GET /api/about`.
#
#   scripts/release-free.sh
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT
git -C "$root" archive --format=tar HEAD | tar -x -C "$scratch"
system="$(nix eval --impure --raw --expr builtins.currentSystem)"
drv() { nix eval --raw "path:$scratch#packages.$system.$1.drvPath"; }

bin="$(drv suzatary-bin)"
app="$(drv suzatary)"
sed -i 's/^version = .*/version = "0.0.1"/' "$scratch/app/release.toml"
printf '\nA line no build reads.\n' | tee -a "$scratch/CHANGELOG.md" "$scratch/CLAUDE.md" >/dev/null
mkdir -p "$scratch/docs" && printf 'A page no build reads.\n' >"$scratch/docs/release-free.md"
bin_after="$(drv suzatary-bin)"
app_after="$(drv suzatary)"

if [[ "$bin" != "$bin_after" ]]; then
  echo "release-free: a release or docs change moved the binary's derivation ($bin, then $bin_after): every train would compile it again" >&2
  exit 1
fi
if [[ "$app" == "$app_after" ]]; then
  echo "release-free: a new release left .#suzatary as it was ($app): the release no longer reaches the package" >&2
  exit 1
fi
echo "release-free: the binary's derivation is $bin with either release; the package moved to $app_after"
