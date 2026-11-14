#!/usr/bin/env bash
# Part <k> of <n> of the browser suite's files, or those that read the page's clock to bound a task
# (`timed`), one name a line (`list` for web/e2e/list.test.ts). Dealt largest first to the
# lightest part by bytes, so every machine deals the same parts.
#
#   scripts/e2e-part.sh <k>/<n> | timed [<name>...]     from the repository's root; 64 on a bad part
set -euo pipefail

part="${1:-}"
if [[ "$part" != timed ]] && { [[ ! "$part" =~ ^([1-9][0-9]*)/([1-9][0-9]*)$ ]] || ((BASH_REMATCH[1] > BASH_REMATCH[2])); }; then
  echo "e2e-part.sh: a part is <k>/<n> with 1 <= k <= n, or timed, not '$part'" >&2
  exit 64
fi
shift
names=("$@")
if ((${#names[@]} == 0)); then
  for file in web/e2e/*.test.ts; do names+=("$(basename "$file" .test.ts)"); done
fi
untimed=()
for name in "${names[@]}"; do
  if grep -qF 'performance.now()' "web/e2e/$name.test.ts"; then
    [[ "$part" != timed ]] || echo "$name"
  else
    untimed+=("$name")
  fi
done
[[ "$part" != timed ]] || exit 0
[[ "$part" =~ ^([1-9][0-9]*)/([1-9][0-9]*)$ ]]
k="${BASH_REMATCH[1]}" n="${BASH_REMATCH[2]}"
for name in "${untimed[@]}"; do echo "$(wc -c <"web/e2e/$name.test.ts") $name"; done |
  LC_ALL=C sort -k1,1rn -k2,2 |
  awk -v k="$k" -v n="$n" '{
    least = 1
    for (i = 2; i <= n; i++) if (load[i] + 0 < load[least] + 0) least = i
    load[least] += $1
    if (least == k) print $2
  }'
