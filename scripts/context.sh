#!/usr/bin/env bash
# The context budget for a person: how many tokens an agent reads to use and to develop each
# unit of the repository (context.toml), and the change against origin/master. The guard
# app/cli/tests/guards/context.rs enforces the same figures; this builds the checkout's own
# binary, so the rules are the branch's.
#
#   scripts/context.sh                              print the table, exit 1 on a finding
#   scripts/context.sh record                       write the reading set as the new ratchet
#                                                   (refused when it is above the old one)
#   scripts/context.sh external <name> <checkout>   measure a pinned library's USE again
#   scripts/context.sh calibrate --model <model>    count the sample with Claude's tokenizer
#                                                   (through the `claude` CLI) and write the ratios
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
binary="$("$root/scripts/app-build.sh" --fixture)"
exec "$binary" context --repo "$root" "$@"
