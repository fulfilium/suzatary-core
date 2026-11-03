#!/usr/bin/env bash
# The wire for the browser, into web/src/wire/generated/ (output, git-ignored
# like web/dist/): wire.ts, the types app/core/src/wire.rs declares, written by
# ts-rs; and payload.ts, a decoder for every type a route answers, folded from
# the same text (`suzatary_core::wire::shape`); and pages.ts, what a host
# hands each page of app/pages and what the page asks of it, the runtime's
# effects and inputs among them (no decoders: the page's module reads its own
# inputs). THE WIRE IS DECLARED ONCE, IN
# RUST (docs/ARCHITECTURE.md §2): a field the Rust adds is in both files, and a
# caller that reads it wrong is a tsc error rather than an `undefined` on a page.
#
# `$SUZATARY_WIRE_TS` names a directory holding all four already (`nix build
# .#suzatary-web-ts`, CI). Without it (the ordinary `.#triage` shell, which
# does not carry that derivation) this runs the SAME `cargo test` the
# derivation runs, with the shell's cargo, into a scratch directory.
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
