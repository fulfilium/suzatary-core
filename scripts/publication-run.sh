#!/usr/bin/env bash
# How the publication.yml run that builds a commit and fills the binary cache ended.
#
#   scripts/publication-run.sh <gh> <owner/repo> <event> <commit> <appear> <timeout> <poll>
#
# Found by run-name `Publication cache <commit>`: a dispatched run's head commit is master's, not
# the commit it builds. Prints one of: `success <url>`; `unbuilt <url>` (its build job's build
# step failed: the tree's builds failed); `failed <url> <conclusion>` (it ended otherwise: an
# install, a fetch or the push); `running
# <url> <status>` (still running after <timeout> s); `none` (no run within <appear> s); `unasked`
# (ten calls in a row failed, or one after <timeout> s). The newest matching run counts. Always
# exits 0. Its seconds are those slept, <poll> a question, and the 60 of one `timeout` cut short.
set -euo pipefail

if [[ "$#" -ne 7 ]]; then
  echo "usage: scripts/publication-run.sh <gh> <owner/repo> <event> <commit> <appear> <timeout> <poll>" >&2
  exit 64
fi
gh="$1" repo="$2" event="$3" commit="$4" appear="$5" timeout="$6" poll="$7"
# The commit is spliced into the jq filter, so it is a commit id and nothing else.
[[ "$commit" =~ ^[0-9a-f]{40}$ ]] || { echo "publication-run: \"$commit\" is no commit id" >&2; exit 64; }

# The build job's build step, by its name in publication.yml.
build_step="Build what a deploy builds and what the gate's live half reads"
# built <url>: how that step of the run at <url> concluded, or nothing where gh does not say.
built() {
  timeout 60 "$gh" run view --repo "$repo" "${1##*/}" --json jobs \
    --jq ".jobs[] | select(.name == \"build\") | .steps[] | select(.name == \"$build_step\") | .conclusion" \
    2>/dev/null || true
}

misses=0 slept=0
while :; do
  if answer="$(timeout 60 "$gh" run list --repo "$repo" --workflow publication.yml --event "$event" \
    --limit 50 --json displayTitle,url,status,conclusion \
    --jq "[.[] | select(.displayTitle == \"Publication cache $commit\")][0] // empty | \"\(.url) \(.status) \(.conclusion)\"")"; then
    misses=0
    read -r url status conclusion <<<"$answer" || true
    case "${status:-}" in
      "") ((slept < appear)) || { echo none; exit 0; } ;;
      completed)
        if [[ "$conclusion" == success ]]; then
          echo "success $url"
        elif [[ "$(built "$url")" == failure ]]; then
          echo "unbuilt $url"
        else
          echo "failed $url $conclusion"
        fi
        exit 0
        ;;
      *) ((slept < timeout)) || { echo "running $url $status"; exit 0; } ;;
    esac
  else
    (($? != 124)) || slept=$((slept + 60))
    misses=$((misses + 1))
    ((misses < 10 && slept < timeout)) || { echo unasked; exit 0; }
  fi
  status=""
  sleep "$poll"
  slept=$((slept + poll))
done
