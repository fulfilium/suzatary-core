#!/usr/bin/env bash
# Part <k> of <n> of the browser suite's files, one name a line (`list` for web/e2e/list.test.ts).
# Dealt largest first to the lightest part by bytes, so every machine deals the same parts.
#
#   scripts/e2e-part.sh <k>/<n> [<name>...]     from the repository's root; 64 on a bad part
set -euo pipefail

if [[ ! "${1:-}" =~ ^([1-9][0-9]*)/([1-9][0-9]*)$ ]] || ((BASH_REMATCH[1] > BASH_REMATCH[2])); then
  echo "e2e-part.sh: a part is <k>/<n> with 1 <= k <= n, not '${1:-}'" >&2
  exit 64
fi
k="${BASH_REMATCH[1]}" n="${BASH_REMATCH[2]}"
shift
names=("$@")
if ((${#names[@]} == 0)); then
  for file in web/e2e/*.test.ts; do names+=("$(basename "$file" .test.ts)"); done
fi
for name in "${names[@]}"; do echo "$(wc -c <"web/e2e/$name.test.ts") $name"; done |
  LC_ALL=C sort -k1,1rn -k2,2 |
  awk -v k="$k" -v n="$n" '{
    least = 1
    for (i = 2; i <= n; i++) if (load[i] + 0 < load[least] + 0) least = i
    load[least] += $1
    if (least == k) print $2
  }'
