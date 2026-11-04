#!/usr/bin/env bash
# How the publication.yml run that builds a commit and fills the binary cache ended.
#
#   scripts/publication-run.sh <gh> <owner/repo> <event> <commit> <appear> <timeout> <poll>
#
# Found by run-name `Publication cache <commit>`: a dispatched run's head commit is master's, not
# the commit it builds. Prints one of: `success`; `failed <conclusion>`; `none` (no run within
# <appear> s); `running <status>` (still running after <timeout> s); `unasked` (ten calls in a
# row failed, or one after <timeout> s). The newest matching run counts. Always exits 0.
set -euo pipefail

if [[ "$#" -ne 7 ]]; then
  echo "usage: scripts/publication-run.sh <gh> <owner/repo> <event> <commit> <appear> <timeout> <poll>" >&2
  exit 64
fi
gh="$1" repo="$2" event="$3" commit="$4" appear="$5" timeout="$6" poll="$7"
# The commit is spliced into the jq filter, so it is a commit id and nothing else.
[[ "$commit" =~ ^[0-9a-f]{40}$ ]] || { echo "publication-run: \"$commit\" is no commit id" >&2; exit 64; }

misses=0
while :; do
  if answer="$(timeout 60 "$gh" run list --repo "$repo" --workflow publication.yml --event "$event" \
    --limit 50 --json displayTitle,status,conclusion \
    --jq "[.[] | select(.displayTitle == \"Publication cache $commit\")][0] // empty | \"\(.status) \(.conclusion)\"")"; then
    misses=0
    read -r status conclusion <<<"$answer" || true
    case "${status:-}" in
      "") ((SECONDS < appear)) || { echo none; exit 0; } ;;
      completed)
        if [[ "$conclusion" == success ]]; then echo success; else echo "failed $conclusion"; fi
        exit 0
        ;;
      *) ((SECONDS < timeout)) || { echo "running $status"; exit 0; } ;;
    esac
  else
    misses=$((misses + 1))
    ((misses < 10 && SECONDS < timeout)) || { echo unasked; exit 0; }
  fi
  status=""
  sleep "$poll"
done
