#!/usr/bin/env bash
# Replay the commit-msg hook over every non-merge commit in a range.
#
#   scripts/check-commits.sh <range>        e.g. origin/master..HEAD
set -euo pipefail
range="${1:?usage: check-commits.sh <range>}"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
hook="$here/hooks/commit-msg"
failed=0
for sha in $(git rev-list --reverse --no-merges "$range"); do
  msg=$(mktemp)
  git log -1 --format=%B "$sha" > "$msg"
  if ! COMMIT_MSG_COMMIT="$sha" "$hook" "$msg"; then
    echo "check-commits: $(git log -1 --format='%h %s' "$sha")" >&2
    failed=1
  fi
  rm -f "$msg"
done
exit "$failed"
