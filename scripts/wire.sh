#!/usr/bin/env bash
# Generates the browser's wire into web/src/wire/generated/ (git-ignored): wire.ts (the types of
# app/core/src/wire.rs and wire/, via ts-rs), payload.ts (their decoders) and pages.ts (each
# page's host contract). The wire is declared once, in Rust.
#
# `$SUZATARY_WIRE_TS` names a directory holding them already (`nix build .#suzatary-web-ts`);
# without it this runs the derivation's own `cargo test` into a scratch directory.
#
#   scripts/wire.sh
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

if [[ -n "${SUZATARY_WIRE_TS:-}" ]]; then
  echo "==> the wire's TypeScript from $SUZATARY_WIRE_TS (nix build .#suzatary-web-ts)"
  wire_dir="$SUZATARY_WIRE_TS"
else
  wire_dir="$(mktemp -d)"
  trap 'rm -rf "$wire_dir"' EXIT
  echo "==> generating the wire's TypeScript (cargo test -p suzatary-core; no .#suzatary-web-ts supplied)"
  if ! ( cd app && TS_RS_EXPORT_DIR="$wire_dir" cargo test -p suzatary-core --lib -- wire::tests::generates &&
    TS_RS_EXPORT_DIR="$wire_dir" cargo test -p suzatary-pages --test boundary -- generates ) >&2; then
    echo "wire.sh: generating wire.ts failed — run inside 'nix develop .#triage', or set SUZATARY_WIRE_TS" >&2
    exit 1
  fi
fi
generated=web/src/wire/generated
rm -rf "$generated"
mkdir -p "$generated"
for file in wire.ts payload.ts pages.ts; do
  test -s "$wire_dir/$file" || {
    echo "wire.sh: $wire_dir/$file is missing or empty" >&2
    exit 1
  }
  cp "$wire_dir/$file" "$generated/$file"
  chmod u+w "$generated/$file"
done
