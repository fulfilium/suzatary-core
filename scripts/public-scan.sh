#!/usr/bin/env bash
# The leak guard (`public-tree.sh --check --lists`) over every commit a range would publish to a
# public repository, its messages and the pull request's text. Prints where a hit is, never
# what it holds.
#
#   scripts/public-scan.sh [--admit <path>]... <repo> <range> [<pr>]
#
# An opaque file, or one with a finding `.gitleaksignore` admits, is a hit until `--admit
# <path>`, taken only at a terminal, admits every version of it the range holds by digest. A
# store's pictures (`suzatary prodrome draw`) are admitted where drawing the store at their
# commit again gives their bytes, and are a hit, by name, where it does not. Each object a
# chartered commit changes is also read as door 1 reads it (`suzatary prodrome texts`): its
# strings and what its markup draws, beside it as `<object>.texts`; an object whose markup does
# not draw is a hit by its path.
set -Eeuo pipefail
# 2 when the scan could not run, 1 only for a hit.
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
bin="${SUZATARY_BIN:-}"
[ -n "$bin" ] || bin="$("$root/scripts/app-bin.sh")/bin/suzatary"
scan="$(mktemp -d)"
trap 'rm -rf "$scan"' EXIT
mkdir -p "$scan/tree"

# One tree per commit (a push publishes every commit, so a term added then removed still
# leaks): the files it changes and its `.gitleaksignore`. Messages, authors and committers
# (`%an`, not the mailmap's `%aN`) and the PR text are trees of their own. An unreadable file is
# a scan that did not run (2), never a file skipped.
empty="$(git -C "$repo" hash-object -t tree /dev/null)"
git -C "$repo" rev-list --reverse "$range" >"$scan/commits"
n=0
while IFS= read -r commit; do
  n=$((n + 1))
  tree="$scan/files/$n-${commit:0:12}" said="$scan/text/$n-${commit:0:12}"
  mkdir -p "$tree" "$said"
  parent="$(git -C "$repo" rev-parse --verify --quiet "$commit^")" || parent="$empty"
  git -C "$repo" -c core.quotePath=false diff -z --name-only --diff-filter=d "$parent" "$commit" >"$scan/paths" || {
    echo "public-scan: could not list what ${commit:0:12} changes" >&2
    exit 2
  }
  if git -C "$repo" cat-file -e "$commit:.gitleaksignore" 2>/dev/null; then
    printf '.gitleaksignore\0' >>"$scan/paths"
  fi
  # Door 2's redraw: the binary draws the store at this commit again and prints each picture's
  # digest, which admits that picture at that path with exactly those bytes; it prints nothing
  # for a tree whose charter is not public. Each picture is read at every commit, changed or
  # not, so one drawn before the commit's objects is refused.
  mkdir -p "$scan/drawn" "$scan/linked" "$scan/undrawn"
  : >"$scan/drawn/$n-${commit:0:12}"
  : >"$scan/linked/$n-${commit:0:12}"
  : >"$scan/undrawn/$n-${commit:0:12}"
  chartered=""
  if git -C "$repo" cat-file -e "$commit:charter.toml" 2>/dev/null; then
    chartered=yes
    store="$scan/store/$n"
    mkdir -p "$store"
    # Blob by blob from the commit's tree, so its own `.gitattributes` (`export-ignore`,
    # `export-subst`) changes nothing the redraw reads, and nothing outside the commit is read: an
    # entry that is not a file (a link, a submodule) is a hit by its path.
    git -C "$repo" ls-tree -r -z --full-tree "$commit" >"$scan/entries" || {
      echo "public-scan: could not list the tree of ${commit:0:12}" >&2
      exit 2
    }
    while IFS= read -r -d '' entry; do
      path="${entry#*$'\t'}"
      read -r mode _ oid <<<"${entry%%$'\t'*}"
      case "$mode" in
        100644 | 100755) ;;
        *)
          printf '%s:0\n' "$path" >>"$scan/linked/$n-${commit:0:12}"
          continue
          ;;
      esac
      case "$path" in
        charter.toml | objects/*)
          mkdir -p "$store/$(dirname "$path")"
          git -C "$repo" cat-file blob "$oid" >"$store/$path" || {
            echo "public-scan: could not write $(printf '%q' "$path") at ${commit:0:12} out" >&2
            exit 2
          }
          ;;
      esac
    done <"$scan/entries"
    "$bin" prodrome draw --store "$store" --admissions >"$scan/drawn/$n-${commit:0:12}" || {
      echo "public-scan: the store at ${commit:0:12} did not draw again, so its pictures are unread" >&2
      exit 2
    }
    rm -rf "$store"
    while IFS= read -r line; do
      path="${line#*  }"
      if git -C "$repo" cat-file -e "$commit:$path" 2>/dev/null; then
        printf '%s\0' "$path" >>"$scan/paths"
      fi
    done <"$scan/drawn/$n-${commit:0:12}"
  fi
  objects=()
  while IFS= read -r -d '' path; do
    mkdir -p "$tree/$(dirname "$path")"
    git -C "$repo" show "$commit:$path" >"$tree/$path" || {
      echo "public-scan: could not read $(printf '%q' "$path") at ${commit:0:12}, so the scan did not run whole" >&2
      exit 2
    }
    case "$path" in
      objects/*/*) ;;
      objects/*.py) [ -z "$chartered" ] || objects+=("$tree/$path") ;;
    esac
  done <"$scan/paths"
  # Door 1's reading of each object, the same function over the same bytes: an object another
  # clone wrote, or one copied in by hand, never passed door 1, and the Pages workflow draws it.
  if ((${#objects[@]})); then
    "$bin" prodrome texts "${objects[@]}" >"$scan/undrawn/$n-${commit:0:12}" || {
      echo "public-scan: the objects of ${commit:0:12} could not be read as door 1 reads them" >&2
      exit 2
    }
    sed -i "s#^$tree/##" "$scan/undrawn/$n-${commit:0:12}"
  fi
  git -C "$repo" log -1 --format=%B "$commit" >"$said/message"
  git -C "$repo" log -1 --format='%an <%ae>%n%cn <%ce>' "$commit" >"$said/identity"
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
for linked in "$scan"/linked/*; do
  [ -s "$linked" ] || continue
  sed "s/^/public-scan: $(basename "$linked"): /; s/\$/ is not a file, and a chartered tree holds files alone/" "$linked" >&2
  leaked=yes
done
for undrawn in "$scan"/undrawn/*; do
  [ -s "$undrawn" ] || continue
  sed "s/^/public-scan: $(basename "$undrawn"): /; s/\$/:0 holds markup that does not draw in the closed world/" "$undrawn" >&2
  leaked=yes
done
for tree in "$scan"/files/* "$scan"/text/*; do
  : >"$scan/admitted"
  for path in "${admits[@]}"; do
    [ ! -f "$tree/$path" ] ||
      printf '%s  %s\n' "$(sha256sum <"$tree/$path" | cut -d' ' -f1)" "$path" >>"$scan/admitted"
  done
  status=0
  case "$tree" in
    "$scan"/text/*) words=(--text) ;;
    *)
      cat "$scan/drawn/$(basename "$tree")" >>"$scan/admitted"
      words=(--admitted "$scan/admitted")
      ;;
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
