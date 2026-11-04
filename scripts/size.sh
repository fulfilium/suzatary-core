#!/usr/bin/env bash
# The production code a change adds or removes, in non-whitespace characters and in lines,
# blank and comment lines excluded. Rust is `app/*/src` up to each file's first `#[cfg(test)]`;
# TypeScript is `web/src` outside `generated/` and `.d.ts`. Informational.
#
#   scripts/size.sh [<base>]   the working tree against <base>
#                              (default: its merge-base with origin/master)
set -euo pipefail

root=$(git rev-parse --show-toplevel)
base=${1:-$(git -C "$root" merge-base HEAD origin/master)}

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

# Characters without whitespace are the primary figure: no formatter's line width moves them.
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
