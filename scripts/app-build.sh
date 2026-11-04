#!/usr/bin/env bash
# Builds app/'s binary incrementally for a dev loop; this binary is never deployed (the deployed
# one is `.#suzatary`, via scripts/app-bin.sh). Prints the binary's path on stdout, all else on
# stderr.
#
#   scripts/app-build.sh             release build of suzatary into app/target/release/
#   scripts/app-build.sh --fixture   incremental dev build of suzatary-dev (the e2e fixture)
#
# The fixture is a dev build (a release build is far slower for no faster e2e), built with
# `cargo test --workspace`'s features so it shares the test build's dependencies.
# CARGO_INCREMENTAL is set here, not in the workspace profile the gate's tests share; a caller's
# own value wins.
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
