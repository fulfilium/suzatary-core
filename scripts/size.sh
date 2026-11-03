#!/usr/bin/env bash
# The production code a change adds or removes, in characters and in lines:
# what someone must read to run the app. Rust is `app/*/src`, each file up to
# its first `#[cfg(test)]`; TypeScript is `web/src` outside `generated/` and
# declaration files. Blank and comment lines are not counted, and a character
# is any but whitespace. Informational: the merge gate prints it, so "this
# simplified the code" is a number in every report.
#
#   scripts/size.sh [<base>]   the working tree against <base>
#                              (default: its merge-base with origin/master)
set -euo pipefail

root=$(git rev-parse --show-toplevel)
base=${1:-$(git -C "$root" merge-base HEAD origin/master)}

# The production text of each language under a tree: the lines counted, one per line.
rust() {
  local f t
  while IFS= read -r -d '' f; do
    t=$(grep -n '^#\[cfg(test)\]' "$f" | head -1 | cut -d: -f1 || true)
    if [ -n "$t" ]; then head -n $((t - 1)) "$f"; else cat "$f"; fi
  done < <(find "$1/app" -path '*/src/*' -name '*.rs' -print0) |
    { grep -vE '^\s*$|^\s*//' || true; }
}

web() {
  find "$1/web/src" -name '*.ts' ! -name '*.d.ts' ! -path '*/generated/*' -print0 |
    xargs -0 cat | { grep -vE '^\s*$|^\s*(//|\*|/\*)' || true; }
}

# Lines, and characters without whitespace: the owner's yardstick, which no formatter's line
# width moves (docs/FRAMEWORK-PLAN-2026-11-02.md §6.4). Lines are the secondary figure.
lines() { wc -l | tr -d ' '; }
chars() { tr -d '[:space:]' | wc -c | tr -d ' '; }

then=$(mktemp -d)
trap 'rm -rf "$then"' EXIT
git -C "$root" archive "$base" app web | tar -x -C "$then"

delta() { local d=$(($1 - $2)); [ "$d" -ge 0 ] && echo "+$d" || echo "$d"; }
measure() {
  local now was
  now=$("$1" "$root" | "$2") was=$("$1" "$then" | "$2")
  echo "$now ($(delta "$now" "$was"))"
}
echo "size: rust $(measure rust chars) characters, $(measure rust lines) lines;" \
  "web $(measure web chars) characters, $(measure web lines) lines, of production code against ${base:0:8}"
