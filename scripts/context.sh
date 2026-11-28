#!/usr/bin/env bash
# The context budget (context.toml) by the train's tool (app/tooling, `train context`), built from
# this checkout under .git: the branch's rules, and no application built. The guard
# app/cli/tests/guards/context.rs reads the same figures.
#
#   scripts/context.sh                              print the table, exit 1 on a finding (a unit
#                                                   over the USE budget is one)
#   scripts/context.sh external <name> <checkout>   measure a pinned library's USE again
#   scripts/context.sh calibrate --model <model>    count the sample with Claude's tokenizer
#                                                   (through the `claude` CLI and the app)
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [[ "${1:-}" == calibrate ]]; then
  exec "$("$root/scripts/app-build.sh" --fixture)" context --repo "$root" "$@"
fi
# The target `train.sh check` shares: neither takes the lock.
target="$(git -C "$root" rev-parse --path-format=absolute --git-common-dir)/train/tool-check"
cargo build --quiet --locked -p suzatary-tooling --manifest-path "$root/app/Cargo.toml" --target-dir "$target"
exec "$target/debug/train" context --repo "$root" "$@"
