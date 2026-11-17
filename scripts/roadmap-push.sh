#!/usr/bin/env bash
# The push of a public store's clone: what the box wrote there committed with its pictures drawn
# again, every commit origin lacks read by door 2 (`public-scan.sh`), then pushed. A push that
# lost a race merges origin and goes round again; a range the scan refuses is never pushed.
#
#   scripts/roadmap-push.sh <clone> [<subject> [<trailer>...]]
#
# Without a subject the commit is the box's ordinary writes, under a fixed subject of its own: a
# public commit's message holds nothing a private caller wrote. Commits as a fixed identity, never
# a person's, since door 2 reads each commit's author. Pushes with the credential git already
# holds for the clone's `origin`. A clone holding nothing new and no commit origin lacks returns at
# once, with no fetch and no draw. Exits 1 when the scan refused, after one upkeep todo names each
# hit's `file:line` (never the word), filed once for those hits so a pass after it adds nothing;
# 2 when something else stopped it, the filing included.
set -euo pipefail

say() { printf 'roadmap-push: %s\n' "$*" >&2; }
die() {
  say "$1"
  exit "${2:-2}"
}
(($# >= 1)) || die "usage: roadmap-push.sh <clone> [<subject> [<trailer>...]]" 64
clone="$1" subject="The box's writes"
(($# < 2)) || subject="$2"
shift $(($# >= 2 ? 2 : 1))
scripts="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
bin="${SUZATARY_BIN:-}"
[[ -n "$bin" ]] || bin="$("$scripts/app-bin.sh" --deployed)/bin/suzatary" \
  || die "there is no deployed app to draw the pictures with."
export GIT_AUTHOR_NAME=Suzatary GIT_AUTHOR_EMAIL=publish@suzatary.invalid \
  GIT_COMMITTER_NAME=Suzatary GIT_COMMITTER_EMAIL=publish@suzatary.invalid
git=(git -C "$clone")

# One pusher at a time per clone: the land and the data push both come here.
exec 9>"$("${git[@]}" rev-parse --path-format=absolute --git-dir)/suzatary-push.lock"
flock 9

# The pictures as the store now draws them, staged.
draw() {
  "$bin" prodrome draw --store "$clone" >/dev/null || die "the store did not draw (above)."
  "${git[@]}" add -- roadmap.svg roadmap-dark.svg
}

# Commits what the box wrote (new objects, a moved pin) with the pictures; nothing when nothing.
commit() {
  local path
  for path in objects flake.lock; do
    [[ ! -e "$clone/$path" ]] || "${git[@]}" add -- "$path"
  done
  "${git[@]}" diff --cached --quiet && return 0
  draw
  local message="$subject"
  (($# == 0)) || message+=$'\n\n'"$(printf '%s\n' "$@")"
  "${git[@]}" commit --quiet -m "$message" || die "the clone did not commit (a hook?)."
}

# Origin's commits joined in: objects only ever add, so only the pictures can conflict, and the
# draw of the joined store settles them.
join() {
  "${git[@]}" merge-base --is-ancestor origin/master HEAD && return 0
  if ! "${git[@]}" merge --quiet --no-edit origin/master >/dev/null 2>&1; then
    local conflicted
    conflicted="$("${git[@]}" diff --name-only --diff-filter=U | grep -vxE 'roadmap(-dark)?\.svg' || true)"
    if [[ -n "$conflicted" ]]; then
      "${git[@]}" merge --abort
      die "origin's roadmap conflicts with the clone in $(paste -sd' ' <<<"$conflicted"), which a store never does: look at the clone."
    fi
  fi
  draw
  "${git[@]}" diff --cached --quiet && [[ ! -e "$("${git[@]}" rev-parse --git-path MERGE_HEAD)" ]] && return 0
  "${git[@]}" commit --quiet --no-edit -m "Join origin's roadmap" || die "the join did not commit."
}

# Read against the local origin/master, so a quiet pass reaches no remote and draws nothing.
if [[ -z "$("${git[@]}" status --porcelain --untracked-files=all -- objects flake.lock)" &&
  -z "$("${git[@]}" rev-list origin/master..HEAD)" ]]; then
  say "nothing to push"
  exit 0
fi
# refused <door 2's stderr>: the range stays unpushed until a person mends the clone, and the
# instance's upkeep chain says where, once for the same hits.
refused() {
  local hits upkeep id
  hits="$(sed -nE 's/^public-scan: [0-9]+-[0-9a-f]{12}: ([^ ]+:[0-9]+)$/\1/p' "$1" | LC_ALL=C sort -u)"
  upkeep="$("$bin" instance get --or-empty upkeep.chain)" || die "door 2 refused, and the upkeep chain could not be read."
  [[ -n "$upkeep" ]] || die "door 2 refused, and the instance names no [upkeep] chain to say so on."
  id="doortwo$(printf '%s\n' "$hits" | sha256sum | cut -c1-8)"
  if "$bin" todo show --chain "$upkeep" "$id" >/dev/null 2>&1; then
    die "door 2 still refuses origin/master..HEAD; todo:$upkeep/$id says where." 1
  fi
  "$bin" todo add --chain "$upkeep" --actor assistant --priority 45 "$id" \
    --body "Door 2 refused the push of a public store's clone." \
    --detail "The clone at \`$clone\` holds commits its origin lacks, and door 2 refused them at $(sed 's/.*/`&`/' <<<"$hits" | paste -sd, - | sed 's/,/, /g'). Nothing is pushed from it until they are mended there." \
    --rationale "A public store that cannot be pushed goes stale: what a visitor reads stops being true." >&2 ||
    die "door 2 refused, and its todo could not be filed (above)."
  die "door 2 refused a commit of origin/master..HEAD (above), so nothing is pushed; filed todo:$upkeep/$id." 1
}

"${git[@]}" fetch --quiet origin master || die "the roadmap's origin could not be fetched."
commit "$@"
for _ in 1 2 3; do
  join
  [[ "$("${git[@]}" rev-parse HEAD)" != "$("${git[@]}" rev-parse origin/master)" ]] \
    || { say "nothing to push"; exit 0; }
  code=0 said="$(mktemp)"
  SUZATARY_BIN="$bin" "$scripts/public-scan.sh" "$clone" origin/master..HEAD 2>"$said" || code=$?
  cat "$said" >&2
  case "$code" in
    0) ;;
    1) refused "$said" ;;
    *) die "door 2 could not read origin/master..HEAD (above), so nothing is pushed." ;;
  esac
  if "${git[@]}" push --quiet origin HEAD:master; then
    say "pushed $("${git[@]}" rev-parse --short HEAD)"
    exit 0
  fi
  "${git[@]}" fetch --quiet origin master || die "the roadmap's origin could not be fetched after a refused push."
done
die "origin moved under three pushes in a row; push again."
