#!/usr/bin/env bash
# The leak guard (`public-tree.sh --check --lists`) over every commit a range would publish to a
# public repository, its messages and the pull request's text. Prints where a hit is, never
# what it holds.
#
#   scripts/public-scan.sh [--admit <path>]... <repo> <range> [<pr>]
#   scripts/public-scan.sh ~/Documents/prodrome origin/main..land/p12 12
#
# An opaque file, or one with a finding `.gitleaksignore` admits, is a hit until `--admit
# <path>`, taken only at a terminal, admits every version of it the range holds by digest.
set -Eeuo pipefail
# 2 when the scan could not run, 1 only for a hit: a caller must never read a failure as a hit.
trap 'exit 2' ERR

usage() {
  echo "usage: public-scan.sh [--admit <path>]... <repo> <range> [<pr>]" >&2
  exit 2
}
admits=()
while [[ "${1:-}" == --admit ]]; do
  (($# >= 2)) || usage
  [[ -t 0 ]] || {
    echo "public-scan: --admit $2 is a person's word that they looked at it, so it is taken at a terminal and never from a script" >&2
    exit 2
  }
  admits+=("$2")
  shift 2
done
(($# == 2 || $# == 3)) || usage
repo=$1
range=$2
pr=${3:-}

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# Without the lists, refuse rather than pass a scan that read nothing.
[ -f "${SUZATARY_INSTANCE:-$root/instance.toml}" ] || {
  echo "public-scan: no instance file, so no lists to scan with" >&2
  exit 2
}
scan="$(mktemp -d)"
trap 'rm -rf "$scan"' EXIT
mkdir -p "$scan/tree"

# One tree per commit (a push publishes every commit, so a term added then removed still
# leaks): the files it changes and its `.gitleaksignore`. Messages and the PR text are trees of
# their own. A file that cannot be read is a scan that did not run (2), never a file skipped.
empty="$(git -C "$repo" hash-object -t tree /dev/null)"
git -C "$repo" rev-list --reverse "$range" >"$scan/commits"
n=0
while IFS= read -r commit; do
  n=$((n + 1))
  tree="$scan/files/$n-${commit:0:12}"
  mkdir -p "$tree" "$scan/text/$n-${commit:0:12}"
  parent="$(git -C "$repo" rev-parse --verify --quiet "$commit^")" || parent="$empty"
  git -C "$repo" -c core.quotePath=false diff -z --name-only --diff-filter=d "$parent" "$commit" >"$scan/paths" || {
    echo "public-scan: could not list what ${commit:0:12} changes" >&2
    exit 2
  }
  if git -C "$repo" cat-file -e "$commit:.gitleaksignore" 2>/dev/null; then
    printf '.gitleaksignore\0' >>"$scan/paths"
  fi
  while IFS= read -r -d '' path; do
    mkdir -p "$tree/$(dirname "$path")"
    git -C "$repo" show "$commit:$path" >"$tree/$path" || {
      echo "public-scan: could not read $(printf '%q' "$path") at ${commit:0:12}, so the scan did not run whole" >&2
      exit 2
    }
  done <"$scan/paths"
  git -C "$repo" log -1 --format=%B "$commit" >"$scan/text/$n-${commit:0:12}/message"
done <"$scan/commits"

if [ -n "$pr" ]; then
  slug="$(git -C "$repo" remote get-url origin | sed -E 's#.*github.com[:/]##; s#\.git$##')" || {
    echo "public-scan: could not read the repository's origin, so the pull request is unread" >&2
    exit 2
  }
  mkdir -p "$scan/text/pull-request"
  gh pr view "$pr" --repo "$slug" --json title,body --jq '.title + "\n" + .body' >"$scan/text/pull-request/pull-request" || {
    echo "public-scan: could not read pull request $pr, so the scan did not run whole" >&2
    exit 2
  }
fi

# A message or the PR text is never admitted.
for path in "${admits[@]}"; do
  touched=""
  for tree in "$scan"/files/*; do
    [ ! -f "$tree/$path" ] || touched=yes
  done
  [ -n "$touched" ] || {
    echo "public-scan: --admit $path names no file $range touches" >&2
    exit 2
  }
done

leaked=""
for tree in "$scan"/files/* "$scan"/text/*; do
  : >"$scan/admitted"
  for path in "${admits[@]}"; do
    [ ! -f "$tree/$path" ] ||
      printf '%s  %s\n' "$(sha256sum <"$tree/$path" | cut -d' ' -f1)" "$path" >>"$scan/admitted"
  done
  status=0
  case "$tree" in
    "$scan"/text/*) words=(--text) ;;
    *) words=(--admitted "$scan/admitted") ;;
  esac
  hits="$("$root/scripts/public-tree.sh" --check --lists "${words[@]}" "$tree")" || status=$?
  if [ "$status" -ne 0 ] && [ "$status" -ne 1 ]; then
    echo "public-scan: the leak guard could not run over $(basename "$tree") (it exited $status), so it is not clean" >&2
    exit 2
  fi
  if [ "$status" -eq 1 ]; then
    printf '%s\n' "$hits" | sed "s/^/public-scan: $(basename "$tree"): /" >&2
    leaked=yes
  fi
done
if [ -n "$leaked" ]; then
  echo "public-scan: a commit above would leak, or hold what a person must admit; nothing may land" >&2
  exit 1
fi
echo "public-scan: $(git -C "$repo" rev-list --count "$range") commit(s) clean" >&2
