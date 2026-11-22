#!/usr/bin/env bash
# What a publication run sends to the binary cache, measured in both of publication.yml's jobs.
#
#   scripts/cache-unserved.sh closure <in> <dir>   the build job, over the built closures
#   scripts/cache-unserved.sh list <in> <dir>      the push job, over the build job's list
#
# The environment names UPSTREAM (cache.nixos.org), CACHE (the cache pushed to) and LIMIT (bytes
# one publication may add). Writes <dir>/served.txt and <dir>/unserved.txt, a partition of <in>,
# the second in <in>'s order, and fails when the unserved paths' NAR sizes sum past LIMIT.
# UPSTREAM is asked about every path, CACHE about those UPSTREAM said it does not hold; a path is
# unserved only when both said so. A path either cache could not be asked about fails the run by
# name and writes no unserved.txt, so no list that may be wrong reaches the push.
#
# `closure`: served is what either cache serves.
# `list`: the list named no served path when the build job took it. A path UPSTREAM serves now is
# an error: the list was made wrongly or forged, and sending it would sign the build job's bytes
# for a path the box also trusts from upstream. A path CACHE serves now was pushed since by
# another publication run, whose tree shares it: it is skipped, since the cache already holds it.
set -euo pipefail

usage() {
  echo "usage: scripts/cache-unserved.sh closure|list <in> <dir>" >&2
  exit 64
}
[[ "$#" -eq 3 && ("$1" == closure || "$1" == list) ]] || usage
mode="$1" in="$2" dir="$3"
: "${UPSTREAM:?}" "${CACHE:?}" "${LIMIT:?}"

# The cache $1's answer about the path $2, one line: `held <path>` (status 200), `absent <path>`
# (status 404, the "no" of cache.nixos.org and of Cachix alike), or `unknown <path> <last>`.
# Anything else (a curl error such as a timeout or a refused connection, or another status) is
# asked again, five tries in all with pauses of 1, 2, 4 and 8 seconds. No try starts at or past
# the epoch second $until, and no pause ends past it; a path not asked by then is unknown at once.
ask() {
  local name="${2#/nix/store/}" pause=1 status=not-asked-before-the-cut-off try
  for try in 1 2 3 4 5; do
    ((EPOCHSECONDS < until)) || break
    status="$(curl -sI -o /dev/null --connect-timeout 5 --max-time 10 -w '%{http_code}' "$1/${name%%-*}.narinfo")" \
      || status="curl-exit-$?"
    case "$status" in
      200) echo "held $2" && return ;;
      404) echo "absent $2" && return ;;
    esac
    ((try < 5 && EPOCHSECONDS + pause < until)) || break
    sleep "$pause"
    pause=$((pause * 2))
  done
  echo "unknown $2 $status"
}
export -f ask

# The cache $1's answers about the paths in the file $2, sixteen at a time, sorted into $3.held,
# $3.absent and $3.unknown, which together hold each path of $2 once. A path is absent only on its
# cache's own "no". The cut-off is 300 seconds after the first question, and a try lasts at most
# 10, so one cache's measure ends about 310 seconds after it begins, whatever the number of paths.
answer() {
  local answer path last
  : >"$3.held" && : >"$3.absent" && : >"$3.unknown"
  until=$((EPOCHSECONDS + 300)) xargs -r -P 16 -I '{}' bash -c 'ask "$0" "$1"' "$1" '{}' <"$2" >"$3.lines" \
    || { echo "::error::a question to $1 did not run to its answer (xargs exit $?, its reason above): nothing pushed" && exit 1; }
  while read -r answer path last; do
    case "$answer" in
      held | absent) [[ -n "$path" && -z "$last" ]] && echo "$path" >>"$3.$answer" && continue ;;
      unknown) [[ -n "$path" ]] && echo "$path ($last)" >>"$3.unknown" && continue ;;
    esac
    echo "::error::an answer of $1 is none of held <path>, absent <path> and unknown <path> <last>: $answer $path $last"
    exit 1
  done <"$3.lines"
  sort "$2" >"$3.asked"
  cut -d ' ' -f 1 "$3.unknown" | sort - "$3.held" "$3.absent" >"$3.answered"
  if ! cmp -s "$3.asked" "$3.answered"; then
    echo "::error::the answers of $1 do not name each path asked once: nothing pushed"
    comm -23 "$3.asked" "$3.answered" | sed 's/^/asked, with no answer: /'
    comm -13 "$3.asked" "$3.answered" | sed 's/^/answered, and not asked once: /'
    exit 1
  fi
  if [[ -s "$3.unknown" ]]; then
    echo "::error::the $4 could not be asked about $(wc -l <"$3.unknown") paths ($1, five tries each): nothing pushed"
    head -n 5 "$3.unknown"
    exit 1
  fi
}

mkdir -p "$dir"
asked="$(mktemp -d)"
answer "$UPSTREAM" "$in" "$asked/upstream" "upstream cache"
if [[ "$mode" == list && -s "$asked/upstream.held" ]]; then
  echo "::error::the list names paths $UPSTREAM serves, so the build job made it wrongly: nothing pushed"
  cat "$asked/upstream.held"
  exit 1
fi
answer "$CACHE" "$asked/upstream.absent" "$asked/cache" "project's cache"
case "$mode" in
  closure) sort -u "$asked/upstream.held" "$asked/cache.held" >"$dir/served.txt" ;;
  list)
    sort -u "$asked/cache.held" >"$dir/served.txt"
    while read -r path; do
      echo "skipped, pushed since the list was taken: $path"
    done <"$dir/served.txt"
    ;;
esac
grep -Fx -f "$asked/cache.absent" "$in" >"$dir/unserved.txt" || (($? == 1))

bytes="$(xargs -r nix path-info --size <"$dir/unserved.txt" | awk '{ sum += $2 } END { print sum + 0 }')"
echo "the push adds $(wc -l <"$dir/unserved.txt") paths, $((bytes / 1000000)) MB uncompressed"
if ((bytes > LIMIT)); then
  echo "::error::this publication would add $((bytes / 1000000)) MB to $CACHE, over the $((LIMIT / 1000000)) MB one publication may add to its 5 GB: nothing pushed"
  exit 1
fi
