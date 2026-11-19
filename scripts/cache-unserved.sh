#!/usr/bin/env bash
# What a publication run sends to the binary cache, measured in both of publication.yml's jobs.
#
#   scripts/cache-unserved.sh closure <in> <dir>   the build job, over the built closures
#   scripts/cache-unserved.sh list <in> <dir>      the push job, over the build job's list
#
# The environment names UPSTREAM (cache.nixos.org), CACHE (the cache pushed to) and LIMIT (bytes
# one publication may add). Writes <dir>/served.txt and <dir>/unserved.txt, a partition of <in>,
# the second in <in>'s order, and fails when the unserved paths' NAR sizes sum past LIMIT. A path
# is served when its cache answers its narinfo; a query that fails counts it as unserved, so the
# measure errs toward too large.
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
[[ "$#" -eq 3 ]] || usage
mode="$1" in="$2" dir="$3"
: "${UPSTREAM:?}" "${CACHE:?}" "${LIMIT:?}"

# The paths on stdin whose narinfo the cache $1 answers.
served() {
  # shellcheck disable=SC2016
  xargs -r -P 16 -I '{}' sh -c 'name="${1#/nix/store/}"; curl -sfI -o /dev/null "$0/${name%%-*}.narinfo" && echo "$1"' "$1" '{}' || true
}

mkdir -p "$dir"
case "$mode" in
  closure)
    { served "$UPSTREAM" <"$in"; served "$CACHE" <"$in"; } | sort -u >"$dir/served.txt"
    ;;
  list)
    upstream="$(served "$UPSTREAM" <"$in")"
    if [[ -n "$upstream" ]]; then
      echo "::error::the list names paths $UPSTREAM serves, so the build job made it wrongly: nothing pushed"
      echo "$upstream"
      exit 1
    fi
    served "$CACHE" <"$in" | sort -u >"$dir/served.txt"
    while read -r path; do
      echo "skipped, pushed since the list was taken: $path"
    done <"$dir/served.txt"
    ;;
  *) usage ;;
esac
grep -Fvx -f "$dir/served.txt" "$in" >"$dir/unserved.txt" || (($? == 1))

bytes="$(xargs -r nix path-info --size <"$dir/unserved.txt" | awk '{ sum += $2 } END { print sum + 0 }')"
echo "the push adds $(wc -l <"$dir/unserved.txt") paths, $((bytes / 1000000)) MB uncompressed"
if ((bytes > LIMIT)); then
  echo "::error::this publication would add $((bytes / 1000000)) MB to $CACHE, over the $((LIMIT / 1000000)) MB one publication may add to its 5 GB: nothing pushed"
  exit 1
fi
