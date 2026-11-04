#!/usr/bin/env bash
# Build app/'s binary INCREMENTALLY, with the cargo `.#triage` now carries
# (flake.nix's `rustToolchain`) — for a fast dev loop, never for deployment.
#
# The `.#triage` shell does not carry the release derivations (any edit under
# app/ would rebuild them before a test could run). This prints the built
# binary's path on stdout (everything else goes to stderr), and after the
# first build a no-op edit costs seconds.
#
# THIS BINARY IS NEVER DEPLOYED (docs/ARCHITECTURE.md §2, §8.2): it exists so
# scripts/web-e2e.sh and a developer's shell have something fast to iterate
# against. The deployed program is always the pure `.#suzatary` derivation —
# see scripts/app-bin.sh, which resolves and runs THAT one.
#
#   scripts/app-build.sh             cargo build --release -p suzatary into
#                                     app/target/release/, prints the binary
#                                     path
#   scripts/app-build.sh --fixture   a DEV build of suzatary-dev, the binary
#                                     with the end-to-end fixture
#                                     (--features fixture,
#                                     app/cli/Cargo.toml), incremental, into
#                                     app/target/ beside cargo test's own
#                                     artifacts, prints the binary path
#
# THE FIXTURE IS A DEV BUILD. A release build of it cost 282 s
# after a one-file change in core; the same change as an incremental dev build
# costs 18 s, and the 133 browser scenes it serves take the same two minutes
# either way. It is built in `cargo test --workspace`'s feature set, the
# fixture feature added: the workspace's (the wasm crate turns on the core's
# `wasm`) and the tests' (their dev-dependencies turn on its `nothing`, inert
# test support), so it SHARES THE TEST BUILD'S DEPENDENCIES (`-p suzatary`
# alone built wasmtime, the wit crates and anyhow again too). NOT ITS CRATES:
# `suzatary/fixture` turns on `suzatary-core/fixture` (app/cli/Cargo.toml), so
# the core and every workspace crate above it compile a second time, and so do
# bit-set and the typst crates above it, whose `default` feature proptest, a
# dev-dependency, turns on in the tests' build alone (cargo's unit graphs,
# 2026-10-11: 19 of this build's 663 units are not the tests'). CARGO_INCREMENTAL is set
# here, not in the workspace profile, which the merge gate's tests share and
# keep small (app/Cargo.toml, "THE TEST BUILD IS NOT A DEBUGGER SESSION"); a
# caller that set it keeps its own (the merge gate builds with none).
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

target_dir="$root/app/target"
if [[ "${1:-}" == "--fixture" ]]; then
  echo "==> cargo build --bin suzatary-dev --workspace --features suzatary/fixture,suzatary-core/nothing, CARGO_INCREMENTAL=${CARGO_INCREMENTAL:-1} (target-dir: $target_dir)" >&2
  CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-1}" cargo build \
    --manifest-path "$root/app/Cargo.toml" \
    --workspace \
    --bin suzatary-dev \
    --target-dir "$target_dir" \
    --features suzatary/fixture,suzatary-core/nothing >&2
  echo "$target_dir/debug/suzatary-dev"
  exit 0
fi

echo "==> cargo build --release -p suzatary (target-dir: $target_dir)" >&2
cargo build --release \
  --manifest-path "$root/app/Cargo.toml" \
  -p suzatary \
  --target-dir "$target_dir" >&2

echo "$target_dir/release/suzatary"
