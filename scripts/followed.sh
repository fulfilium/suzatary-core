#!/usr/bin/env bash
# The check that a revision went through the gate and was chosen by the operator, made by
# scripts/deploy.sh before it moves the lock and by the roots run before it builds a lock.
#
#   followed.sh fetch <mirror> <remote> <branch>          fetch into the bare mirror; record <remote>
#   followed.sh check <mirror> <remote> <branch> <rev>    <rev> is on <branch>'s first-parent chain
#   followed.sh pin <mirror> <rev> <branch>               record the pin; the last pin becomes `previous`
#   followed.sh admit <mirror> <remote> <branch> <rev>    <rev> was pinned here (or is `previous`) and checks
#
# Only the first-parent chain went through a gate. The mirror lives in this box's .git, so a lock
# moved by a push (a leaked token, anyone) names a revision no deploy here pinned and is refused;
# it can still name an earlier pinned one, by design (a rollback). The way back is the deploy's
# own last pin, never read from the lock, which a push can write. A branch is a name compared
# exactly (`master^2` is not one). Only `fetch` reads the network.
#
# Logs are read to the end with awk: a reader that quits at its first match (the box's grep is
# ugrep) breaks the pipe, which pipefail reads as a refusal.
set -euo pipefail

die() { echo "followed: $1" >&2; exit 1; }

# A branch name: no revision suffix, no `..`, nothing git would read as more.
named() {
  [[ "$1" =~ ^[A-Za-z0-9._/-]+$ && "$1" != *..* && "$1" != -* ]] \
    || die "\"$1\" is not a branch name this box follows"
}

on_chain() {
  local mirror="$1" remote="$2" branch="$3" rev="$4" followed
  named "$branch"
  followed="$(git -C "$mirror" config --get suzatary.remote 2>/dev/null)" \
    || die "$mirror follows no remote yet: scripts/deploy.sh fetches the branch it pins from"
  [ "$followed" = "$remote" ] \
    || die "the lock names $remote, and this box follows $followed"
  git -C "$mirror" rev-parse --quiet --verify "refs/heads/$branch^{commit}" >/dev/null \
    || die "this box has not fetched $branch of $remote"
  git -C "$mirror" rev-list --first-parent "refs/heads/$branch" \
    | awk -v r="$rev" '$0 == r { f = 1 } END { exit !f }' \
    || die "$rev is not a commit of the first-parent chain of $branch of $remote as this box last fetched it"
}

verb="${1:-}"
case "$verb" in
  fetch)
    [ "$#" -eq 4 ] || die "usage: followed.sh fetch <mirror> <remote> <branch>"
    mirror="$2" remote="$3" branch="$4"
    named "$branch"
    [ -d "$mirror" ] || git init --quiet --bare "$mirror" || die "could not make the mirror $mirror"
    git -C "$mirror" fetch --quiet "$remote" "+refs/heads/$branch:refs/heads/$branch" \
      || die "could not fetch $branch from $remote"
    git -C "$mirror" config suzatary.remote "$remote"
    ;;
  check)
    [ "$#" -eq 5 ] || die "usage: followed.sh check <mirror> <remote> <branch> <rev>"
    on_chain "$2" "$3" "$4" "$5"
    ;;
  pin)
    [ "$#" -eq 4 ] || die "usage: followed.sh pin <mirror> <rev> <branch>"
    mirror="$2" rev="$3" branch="$4"
    named "$branch"
    last="$(git -C "$mirror" config --get suzatary.last 2>/dev/null || true)"
    git -C "$mirror" config --get-all suzatary.pinned 2>/dev/null \
      | awk -v p="$rev $branch" '$0 == p { f = 1 } END { exit !f }' \
      || git -C "$mirror" config --add suzatary.pinned "$rev $branch" \
      || die "could not record the pin of $rev in $mirror"
    # A deploy of the pin already last keeps the way back it had.
    if [ -n "$last" ] && [ "$last" != "$rev $branch" ]; then
      git -C "$mirror" config suzatary.previous "$last" \
        || die "could not record the way back in $mirror"
    fi
    git -C "$mirror" config suzatary.last "$rev $branch" \
      && git -C "$mirror" config suzatary.branch "$branch" \
      || die "could not record the branch $branch in $mirror"
    ;;
  admit)
    [ "$#" -eq 5 ] || die "usage: followed.sh admit <mirror> <remote> <branch> <rev>"
    mirror="$2" remote="$3" branch="$4" rev="$5"
    named "$branch"
    following="$(git -C "$mirror" config --get suzatary.branch 2>/dev/null)" \
      || die "no deploy has pinned a revision on this box yet: scripts/deploy.sh moves the lock"
    previous="$(git -C "$mirror" config --get suzatary.previous 2>/dev/null || true)"
    if [ "$rev $branch" != "$previous" ]; then
      [ "$branch" = "$following" ] \
        || die "the lock names the branch $branch, and this box follows $following"
      git -C "$mirror" config --get-all suzatary.pinned 2>/dev/null \
        | awk -v p="$rev $branch" '$0 == p { f = 1 } END { exit !f }' \
        || die "no deploy on this box pinned $rev of $branch: only scripts/deploy.sh chooses what runs, and a rollback is a deploy of the earlier rev"
    fi
    on_chain "$mirror" "$remote" "$branch" "$rev"
    ;;
  *)
    die "usage: followed.sh fetch|check|pin|admit ..."
    ;;
esac
