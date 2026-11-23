#!/usr/bin/env bash
# Each wasm module against its limit in <budget> for <system> (`<module> <system> <measured>
# <limit>`, AGENTS.md, "Derived figures"); exit 1 naming each module over its limit, or with no
# line where the budget has lines for <system>. A system the budget names nowhere is printed, not
# judged.
#
#   scripts/wasm-limits.sh <budget> <system> <module>...
set -euo pipefail

(($# >= 3)) || { echo "usage: scripts/wasm-limits.sh <budget> <system> <module>..." >&2; exit 64; }
budget="$1" system="$2"
shift 2
judged="$(grep -v '^#' "$budget" | awk -v s="$system" '$2 == s' | wc -l)"
over=0
for wasm; do
  # A module named by its bytes is judged by its plain name.
  name=$(basename "$wasm" | sed -E 's/-[0-9a-f]{64}\././')
  size=$(stat -c %s "$wasm")
  read -r measured limit < <(grep -v '^#' "$budget" | awk -v n="$name" -v s="$system" '$1 == n && $2 == s { print $3, $4 }') || true
  if [[ -n "${limit:-}" ]] && ((size > limit)); then
    echo "wasm-limits.sh: $name is $size bytes, over its limit of $limit on $system ($measured at the train's base): declare \`Wasm-growth: $name +$((size - limit)): <reason>\`" >&2
    over=1
  elif [[ -n "${limit:-}" ]]; then
    echo "    $name: $size bytes, limit $limit, $measured at the train's base"
  elif ((judged > 0)); then
    echo "wasm-limits.sh: $name has no line for $system in $budget ($size bytes): declare \`Wasm-growth: $name +$size: <reason>\`" >&2
    over=1
  else
    echo "    $name: $size bytes; $budget names no line for $system"
  fi
  measured="" limit=""
done
exit "$over"
