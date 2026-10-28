#!/usr/bin/env bash
# HOW THE PUBLICATION RUN OF A COMMIT ENDED: the run of .github/workflows/publication.yml that
# builds a commit of suzatary-core and pushes its builds to the binary cache. scripts/deploy.sh
# asks it of the commit it pins (a `push` run on master), and scripts/train.sh of a gate commit
# (a `workflow_dispatch` run on master whose input names it), and each builds only once it has an
# answer, so no local build races the run.
#
#   scripts/publication-run.sh <gh> <owner/repo> <event> <commit> <appear> <timeout> <poll>
#
# A run is found by its event and its run-name, `Publication cache <commit>`, which publication.yml
# gives every run: a dispatched run's head commit is master's, not the commit it builds. It asks
# <gh> every <poll> seconds, each call bounded to 60, and prints one word and its detail on stdout:
# `success`; `failed <conclusion>`; `none`, when no run appeared within <appear> seconds;
# `running <status>`, when a run was still under way <timeout> seconds after the start; or
# `unasked`, when ten calls in a row failed, or a call failed after <timeout> seconds. What gh
# says on a failure goes to stderr. The newest matching run counts, so a re-run that passed
# stands. It exits 0 with every answer.
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
