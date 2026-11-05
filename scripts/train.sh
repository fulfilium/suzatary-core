#!/usr/bin/env bash
# The merge queue on the box: reviewed PRs ride a train, one integration branch gated once by
# scripts/merge-gate.sh and landed as one fast-forward of master.
#
#   scripts/train.sh build <pr>...   train := origin/master, each PR merged in, then the release
#                                    the project chain reads cut on top
#   scripts/train.sh gate [--full]   both halves of scripts/merge-gate.sh on the train
#   scripts/train.sh bisect          the merge that turned a red half red
#   scripts/train.sh land            master := the gated train and its tag v<version>, pushed as
#                                    one, while master has not moved and the chain reads that
#                                    version; then its todos recorded done, then publish it
#   scripts/train.sh record [<rev>]  the todos the release at <rev> (master) completed, recorded
#                                    done where they are not yet: a land's last step, again
#   scripts/train.sh nightly         both halves of the gate, full, on origin/master; a red one
#                                    files a todo on the project chain
#   scripts/train.sh publish [--skip <sha>]... [--admit <path>]... [<rev>]
#                                    core's master += one commit per landed commit up to <rev>
#                                    (master's tip), each the public tree of its source; a
#                                    skipped commit is named by the next one's `Skipped:` trailer,
#                                    an admitted path by its row's `Admitted:` trailer
#
# `--remote github` runs the tree half on core's GitHub runners. The box keeps the live half,
# which reads the owner's data.
#
# Run inside `nix develop .#triage`, from a clean checkout. It reaches GitHub through `gh`; it
# never promotes and never touches the data repository.
#
# Publication: core's master is a table, one commit per landed first-parent commit, each the
# public tree of its `Source:`. It only climbs (never rewritten), and a tree is published only
# when its leak guard passes whole. A commit that can never pass blocks every later row until
# `publish --skip <sha>`, recorded as a `Skipped:` trailer that scripts/deploy.sh refuses.
#
# Publication cannot be taken back, and the leak guard sees only what its rules know, so a path
# (or an admitted-file line, a .gitleaksignore fingerprint, a secret-rules change) no earlier
# publication held is published only after `publish --admit <path>` at a terminal, recorded as an
# `Admitted:` trailer. A land, which runs unattended, publishes nothing holding a new path.
#
# A train's PRs are a set: the merge order is a function of their heads (a stacked PR merges
# before its base, carrying its `Completes:`), so one set over one master builds one tree. A PR
# is built from the head GitHub names. Changelog lines a merge put under a released section are
# moved back under Unreleased.
#
# Every train ends with the release `suzatary release cut` writes, its version a reading of the
# project chain with the train's `Completes:` todos done. Land checks the chain reads that
# version, pushes master and `v<version>` atomically, and only then records the todos done
# (`record`, idempotent); a refused push writes nothing.
#
# The gate judges a tree in two halves, `tree` and `live`. Verdicts are appended to
# .git/train/gates/, keyed by gate, half and tree, never rewritten; a key holding both verdicts
# is a flake, and nothing lands over it. A tree lands when both halves passed it. Each half runs
# in its own worktree at the commit read once at the start, with that tree's own merge-gate.sh.
# A gate runs only in the `.#triage` shell, refusing any inherited variable its steps read.
# `--full` verdicts are a table of their own; a full red beside a selective green is a finding.
#
# `--remote github` sends only the public tree, only once its leak guard passes, to core's
# `gate/<train sha>` branch, never master. Steps that read this private repository run on the box
# (`box_before`, `box_after`). Only master's publication.yml holds the Cachix token, never a gate
# branch's workflow. The box waits for that run so Nix can substitute its builds. Only a gate's own
# red makes a tree red (a cancelled or lost run is no verdict), and a red half stops the other.
#
# `nightly` gates origin/master in both halves with `merge-gate.sh --nightly`, waiting for a running
# train and holding its lock; a red night files one todo per tree on the project chain.
#
# Test seams: SUZATARY_TRAIN_GATE (another gate script; its verdicts are keyed by its own bytes,
# so a stub's green never lands a train), SUZATARY_TRAIN_GH, SUZATARY_TRAIN_POLL (seconds, 60),
# SUZATARY_TRAIN_PUBLIC, SUZATARY_TRAIN_CACHE_TIMEOUT (seconds, 12000), SUZATARY_TRAIN_BIN.
set -euo pipefail

die() {
  printf 'train: %b\n' "$*" >&2
  exit 1
}
say() { printf 'train: %s\n' "$*" >&2; }
usage() {
  echo "usage: scripts/train.sh build <pr>... | gate [--full] [--remote github] | bisect [--remote github] | land | record [<rev>] | nightly | publish [--skip <sha>]... [--admit <path>]... [<rev>]" >&2
  exit 64
}

gh="${SUZATARY_TRAIN_GH:-gh}"
scripts="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(git rev-parse --show-toplevel)" || die "not inside a git checkout."
cd "$root"
# In .git, never in the tree.
state="$(git rev-parse --path-format=absolute --git-common-dir)/train"
mkdir -p "$state"
# One train at a time: runs share this checkout, its verdict table and its worktree. The nightly
# gate waits for a running train instead of refusing.
exec 9>>"$state/lock"
if [[ "${1:-}" == nightly ]]; then
  flock -n 9 || { say "a train is running here: the nightly gate waits for it"; flock 9; }
else
  flock -n 9 || die "another scripts/train.sh is running in this checkout ($(cat "$state/lock.holder" 2>/dev/null)); wait for it to finish."
fi
echo "${1:-}, pid $$, since $(date -u +%FT%TZ)" >"$state/lock.holder"

# What the exit trap undoes.
return_to="" there="" live="" cloud="" gate_branch="" gate_pushed=""
finish() {
  [[ -z "$live" ]] || live_stop
  [[ -z "$there" ]] || { kill "$there" 2>/dev/null; wait "$there" 2>/dev/null; } || true
  [[ -z "$gate_branch" ]] || cloud_close
  worktree_close
  [[ -z "$return_to" ]] || git checkout --quiet "$return_to"
}
trap finish EXIT

clean() {
  [[ -z "$(git status --porcelain)" ]] \
    || die "the checkout has changes; the train builds and gates what is committed, so commit or stash them first:\n$(git status --short)"
}

# Two PRs that only added CHANGELOG lines join by union, ours then theirs; a removed or rewritten
# line is a real conflict, left to its author.
join_changelog() {
  local f=CHANGELOG.md dir joined=1
  dir="$(mktemp -d)"
  if git show ":1:$f" >"$dir/base" 2>/dev/null && git show ":2:$f" >"$dir/ours" \
    && git show ":3:$f" >"$dir/theirs" \
    && only_adds "$dir/base" "$dir/ours" && only_adds "$dir/base" "$dir/theirs"; then
    git merge-file --union "$dir/ours" "$dir/base" "$dir/theirs" \
      && cp "$dir/ours" "$f" && git add -- "$f" && joined=0
  fi
  rm -rf "$dir"
  return "$joined"
}

only_adds() {
  local stat
  stat="$(git diff --no-index --numstat -- "$1" "$2")" || true
  [[ -z "$stat" || "$(cut -f2 <<<"$stat")" == 0 ]]
}

# Keep the train's lock (the versions its merges were tested with); cargo adds what the PR needs.
relock() {
  git checkout --ours -- app/Cargo.lock \
    && cargo metadata --manifest-path app/Cargo.toml --format-version 1 >/dev/null \
    && git add -- app/Cargo.lock
}

# Lines HEAD's merge brought under a released section that the PR's <head> holds under Unreleased
# (git placed them by context after a release moved their neighbours) are moved to the end of
# Unreleased and the merge amended. Prints how many moved.
place_changelog() {
  local head="$1" f=CHANGELOG.md dir moved
  [[ -f "$f" ]] && git cat-file -e "$head:$f" 2>/dev/null || { echo 0; return; }
  dir="$(mktemp -d)"
  git show "$head:$f" | awk '/^## /{ un = /^## \[Unreleased\]/; next } un && NF' >"$dir/wrote"
  git diff -U0 HEAD^1 HEAD -- "$f" | awk '
    /^@@/ { split($3, at, ","); line = substr(at[1], 2); next }
    /^\+\+\+/ { next }
    /^\+/ { print line++ }' >"$dir/brought"
  awk -v count="$dir/moved" '
    FILENAME == ARGV[1] { wrote[$0]; next }
    FILENAME == ARGV[2] { brought[$0]; next }
    { line[++n] = $0 }
    END {
      section = ""
      for (i = 1; i <= n; i++) {
        if (line[i] ~ /^## /) {
          section = line[i] ~ /^## \[Unreleased\]/ ? "unreleased" : "released"
          if (section == "unreleased") at = i
        } else if (section == "released" && (i in brought) && (line[i] in wrote)) {
          move[i]
          moving[++m] = line[i]
        }
      }
      if (!at) m = 0
      last = at
      for (i = at + 1; at && i <= n && line[i] !~ /^## /; i++) if (line[i] != "") last = i
      for (i = 1; i <= n; i++) {
        if (m && (i in move)) continue
        print line[i]
        if (m && i == last) {
          if (last == at) print ""
          for (k = 1; k <= m; k++) print moving[k]
        }
      }
      print m + 0 >count
    }' "$dir/wrote" "$dir/brought" "$f" >"$dir/placed"
  moved="$(cat "$dir/moved")"
  if ((moved)); then
    cp "$dir/placed" "$f"
    git add -- "$f"
    git commit --quiet --amend --no-edit >/dev/null || { rm -rf "$dir"; return 1; }
  fi
  rm -rf "$dir"
  echo "$moved"
}

# The todos on PR <n>'s `Completes:` lines, one per line (split on spaces and commas, never
# globbed). A word that is no todo id exits 2 with it as the last line.
completes_of() {
  local body line word words=()
  body="$("$gh" pr view "$1" --json body --jq .body)" || return 1
  while IFS= read -r line; do
    line="${line%$'\r'}"
    [[ "$line" =~ ^[[:space:]]*Completes:[[:space:]]*(.*)$ ]] || continue
    IFS=$' \t,' read -r -a words <<<"${BASH_REMATCH[1]}"
    for word in "${words[@]}"; do
      [[ -n "$word" ]] || continue
      echo "$word"
      [[ "$word" =~ ^[a-z0-9_-]+$ ]] || return 2
    done
  done <<<"$body"
}

# Sets `app_bin`: SUZATARY_TRAIN_BIN, else the build of the named commit, never the checkout as
# it happens to be, since this binary writes the chain. Each commit is built once per command.
app_bin=""
declare -A app_bins=()
resolve_app() {
  local rev
  rev="$(git rev-parse --verify --quiet "$1^{commit}")" || die "\"$1\" names no commit to build the app from."
  if [[ -n "${SUZATARY_TRAIN_BIN:-}" ]]; then
    app_bin="$SUZATARY_TRAIN_BIN"
    return 0
  fi
  if [[ -z "${app_bins[$rev]:-}" ]]; then
    app_bins[$rev]="$(SUZATARY_APP_REV="$rev" "$root/scripts/app-bin.sh")/bin/suzatary" \
      || die "the app could not be built from ${rev:0:12} to read the release."
  fi
  app_bin="${app_bins[$rev]}"
}

# One paragraph of `key: value` lines, skipping empty values: git reads trailers only from the
# last paragraph, so every trailer of a commit must share it.
trailers() {
  local out=() key value
  while (($#)); do
    key="$1" value="$2"
    shift 2
    [[ -z "$value" ]] || out+=("$key: $value")
  done
  ((${#out[@]} == 0)) || printf '%s\n' "${out[@]}"
}

trailer_of() { git log -1 --format="%(trailers:key=$2,valueonly,separator= )" "$1" | sed '/^$/d'; }

# The todos the train's merges complete, each once, in merge order.
train_completes() {
  local commit
  for commit in $(git rev-list --first-parent --reverse "origin/master..HEAD"); do
    [[ -z "$(pr_of "$commit")" ]] || trailer_of "$commit" Completes | tr ' ' '\n'
  done | sed '/^$/d' | awk '!seen[$0]++'
}

# The one reader of the last release, so the patch, the record and the tag cannot disagree.
last_release() {
  local rev
  rev="$(newest_release origin/master)"
  [[ -z "$rev" ]] || trailer_of "$rev" Release
}

newest_release() {
  # awk reads to the end: one that left early would SIGPIPE git log, which pipefail and
  # errexit turn into a silent exit 141.
  git log --first-parent --format='%H %(trailers:key=Release,valueonly,separator=)' "$1" \
    | awk 'NF > 1 && !found { print $1; found = 1 }'
}

# Every command first records origin/master's newest release on the chain, so a land whose record
# failed is repaired by whatever runs next.
sync() {
  fetch_origin
  record_origin
}

fetch_origin() {
  git fetch --quiet --tags origin master || die "origin could not be fetched."
}

record_origin() {
  local rev
  rev="$(newest_release origin/master)"
  [[ -n "$rev" && ! -e "$state/recorded/$rev" ]] || return 0
  if (record_release "$rev" origin/master) >&2; then
    mkdir -p "$state/recorded" && : >"$state/recorded/$rev"
  else
    say "the release at ${rev:0:12} is still not recorded on the chain; the next command tries again (scripts/train.sh record)"
  fi
}

# The train's last commit: the release the chain reads once the train's todos are done.
cut_release() {
  [[ -f app/core/releases.toml ]] || return 0
  local todos=() args=() todo out version since last
  mapfile -t todos < <(train_completes)
  for todo in "${todos[@]}"; do args+=(--completes "$todo"); done
  last="$(last_release)"
  [[ -z "$last" ]] || args+=(--last "$last")
  # Master's changelog lets the cut move back entries written under an Unreleased since closed.
  since="$(mktemp)"
  git show origin/master:CHANGELOG.md >"$since" 2>/dev/null || : >"$since"
  resolve_app origin/master
  out="$("$app_bin" release cut --tree "$root" --since "$since" "${args[@]}")" \
    || { rm -f "$since"; die "the release could not be cut (above), so the train is not built; nothing is gated until it is."; }
  rm -f "$since"
  version="$(sed -n 's/.*"version":"\([^"]*\)".*/\1/p' <<<"$out")"
  [[ -n "$version" ]] || die "suzatary release cut printed no version: $out"
  # Else the tag would be refused only at land, after the whole gate.
  ! git ls-remote --exit-code --tags origin "refs/tags/v$version" >/dev/null \
    || die "the chain reads $version, and origin has v$version already (a tag no release on master names): the train is not built. Move or delete that tag, or complete a todo so the chain reads past it."
  # No crate compiles either file, so the release moves no gate key.
  git add -- CHANGELOG.md app/release.toml
  git commit --quiet -m "release: $version" \
    -m "$(trailers Release "$version" Completes "${todos[*]}")" \
    || die "the release $version did not commit (a hook?)."
  printf 'release %s%s\n' "$version" "$( ((${#todos[@]} == 0)) || printf ', completing %s' "${todos[*]}")"
}

build() {
  (($#)) || usage
  local n
  for n in "$@"; do [[ "$n" =~ ^[1-9][0-9]*$ ]] || die "\"$n\" is not a pull request number."; done
  command -v "$gh" >/dev/null || die "build reads each PR's title through gh, which is not on PATH."
  clean
  fetch_origin
  git checkout --quiet -B train origin/master
  record_origin
  : >"$state/skipped"
  local rows=() prs meta title url head current said code unknown ids=() checker=""
  local ready=()
  local -A head_of=() title_of=() url_of=() todos_of=() carrier_of=()
  # `Completes:` todos are checked against the chain by master's binary.
  if git cat-file -e origin/master:app/core/releases.toml 2>/dev/null; then
    resolve_app origin/master
    checker="$app_bin"
  fi
  mapfile -t prs < <(printf '%s\n' "$@" | sort -nu)
  for n in "${prs[@]}"; do
    if ! git fetch --quiet origin "refs/pull/$n/head"; then
      rows+=("#$n|skipped|origin has no refs/pull/$n/head")
      echo "#$n no head on origin" >>"$state/skipped"
      continue
    fi
    head="$(git rev-parse FETCH_HEAD)"
    if ! meta="$("$gh" pr view "$n" --json title,url,headRefOid --jq '[.title, .url, .headRefOid] | @tsv')"; then
      rows+=("#$n|skipped|gh could not read its title")
      echo "#$n gh could not read it" >>"$state/skipped"
      continue
    fi
    IFS=$'\t' read -r title url current <<<"$meta"
    # GitHub moves refs/pull/<n>/head after a push, not with it.
    [[ "$current" == "$head" ]] \
      || die "#$n: origin's refs/pull/$n/head is $head, and GitHub names $current as the PR's head, so a train of it would gate a commit that is not the PR. No PR is merged (the train is origin/master); build again once refs/pull/$n/head has moved."
    said="" code=0
    said="$(completes_of "$n")" || code=$?
    case "$code" in
      0) ;;
      2)
        rows+=("#$n|skipped|its Completes: line names \"$(tail -n 1 <<<"$said")\", which is not a todo id")
        echo "#$n Completes: not a todo id" >>"$state/skipped"
        continue
        ;;
      *)
        rows+=("#$n|skipped|gh could not read its description")
        echo "#$n gh could not read its description" >>"$state/skipped"
        continue
        ;;
    esac
    todos_of[$n]="$(paste -sd' ' <<<"$said")"
    read -r -a ids <<<"${todos_of[$n]}"
    if ((${#ids[@]})) && [[ -n "$checker" ]]; then
      code=0
      unknown="$("$checker" release known --tree "$root" "${ids[@]}" 2>&1)" || code=$?
      case "$code" in
        0) ;;
        3)
          rows+=("#$n|skipped|its Completes: line names a todo the chain does not hold: ${unknown##*: }")
          echo "#$n Completes: unknown todo" >>"$state/skipped"
          continue
          ;;
        *) die "the project chain could not be asked about #$n's Completes: line, so no PR is judged by it:\n$unknown" ;;
      esac
    fi
    head_of[$n]="$head" title_of[$n]="$title" url_of[$n]="$url"
    ready+=("$n")
  done
  # The order is a function of the heads: most other heads contained first, then head name. The
  # number only orders PRs with one head, whose merges are of one commit.
  local order=() m held
  mapfile -t order < <(for n in "${ready[@]}"; do
    held=0
    for m in "${ready[@]}"; do
      [[ "$m" == "$n" ]] || ! git merge-base --is-ancestor "${head_of[$m]}" "${head_of[$n]}" || held=$((held + 1))
    done
    echo "$held ${head_of[$n]} $n"
  done | sort -k1,1nr -k2,2 -k3,3n | cut -d' ' -f3)
  local subject trailer todos carried conflicts resolved file moved
  for n in "${order[@]}"; do
    head="${head_of[$n]}"
    if git merge-base --is-ancestor "$head" HEAD; then
      if [[ -n "${carrier_of[$n]:-}" ]]; then
        rows+=("#$n|in|carried by #${carrier_of[$n]}'s merge")
      else
        rows+=("#$n|in|already on master")
      fi
      continue
    fi
    carried=() todos="${todos_of[$n]}"
    for m in "${order[@]}"; do
      [[ "$m" != "$n" ]] && ! git merge-base --is-ancestor "${head_of[$m]}" HEAD \
        && git merge-base --is-ancestor "${head_of[$m]}" "$head" || continue
      carried+=("$m")
      todos+=" ${todos_of[$m]}"
    done
    todos="$(tr ' ' '\n' <<<"$todos" | sed '/^$/d' | awk '!seen[$0]++' | paste -sd' ')"
    subject="Merge #$n: ${title_of[$n]}"
    subject="${subject:0:72}"
    subject="${subject%"${subject##*[![:space:]]}"}"
    trailer=()
    said="$(trailers Completes "$todos" Carries "${carried[*]}")"
    [[ -z "$said" ]] || trailer=(-m "$said")
    said=""
    if ! git merge --quiet --no-ff -m "$subject" -m "${url_of[$n]}" "${trailer[@]}" "$head" >/dev/null 2>&1; then
      mapfile -t conflicts < <(git diff --name-only --diff-filter=U)
      resolved=()
      for file in "${conflicts[@]}"; do
        case "$file" in
          CHANGELOG.md) join_changelog && resolved+=("$file") ;;
          app/Cargo.lock) relock && resolved+=("$file") ;;
        esac
      done
      if ((${#conflicts[@]})) && ((${#resolved[@]} == ${#conflicts[@]})) \
        && git commit --quiet -m "$subject" -m "${url_of[$n]}" \
          -m "$(trailers Completes "$todos" Carries "${carried[*]}" Joined "${resolved[*]}" Mentions "${resolved[*]}")" \
          >/dev/null; then
        said=" (joined ${resolved[*]})"
      else
        git merge --abort 2>/dev/null || git reset --quiet --hard HEAD
        if ((${#conflicts[@]})); then
          rows+=("#$n|skipped|conflict in ${conflicts[*]}: needs its author")
          echo "#$n conflict in ${conflicts[*]}" >>"$state/skipped"
        else
          rows+=("#$n|skipped|git refused the merge (a hook?): needs a look")
          echo "#$n merge refused" >>"$state/skipped"
        fi
        continue
      fi
    fi
    if ! moved="$(place_changelog "$head")"; then
      git reset --quiet --hard HEAD
      die "#$n's merge put CHANGELOG.md lines under a released section, and moving them under Unreleased did not commit (a hook?)."
    fi
    ((moved == 0)) || said+=" (moved $moved CHANGELOG.md line$( ((moved == 1)) || echo s) under Unreleased)"
    for m in "${carried[@]}"; do carrier_of[$m]="$n"; done
    rows+=("#$n|merged|$subject$said")
  done
  local row what why
  for row in "${rows[@]}"; do
    IFS='|' read -r n what why <<<"$row"
    printf '%-6s %-8s %s\n' "$n" "$what" "$why"
  done
  printf 'train is %s on origin/master %s\n' "$(git rev-parse --short HEAD)" "$(git rev-parse --short origin/master)"
  [[ "$(git rev-parse HEAD)" != "$(git rev-parse origin/master)" ]] \
    || die "no PR merged ($(printf '#%s ' "${prs[@]}")skipped or already on master), so the train is master and there is nothing to gate."
  cut_release
}

halves=(tree live)
# `gate --full`, or `nightly`, which also runs the live web suite whole.
full=0 nightly=0
half_flag() { case "$1" in tree) echo --no-live ;; live) echo --only-live ;; esac; }
gate_flags() {
  if ((nightly)); then printf -- '--nightly '; elif ((full)); then printf -- '--full '; fi
  half_flag "$1"
}

# verdicts <half> <commit> [full]: the verdicts file. A stub gate's are keyed by its bytes.
verdicts() {
  local gate=merge-gate
  [[ -z "${SUZATARY_TRAIN_GATE:-}" ]] || gate="other-$(git hash-object "$SUZATARY_TRAIN_GATE")"
  echo "$state/gates/$gate/$1${3:+-$3}/$(git rev-parse "$2^{tree}")"
}

# ok, failed, unknown or finding (both seen), from every gate or with `full` from full gates.
verdict() {
  local seen files=()
  if [[ "${3:-}" == full ]]; then
    files=("$(verdicts "$1" "$2" full)")
  else
    files=("$(verdicts "$1" "$2")" "$(verdicts "$1" "$2" full)")
  fi
  seen="$(cat "${files[@]}" 2>/dev/null | cut -d' ' -f1 | sort -u | paste -sd' ')"
  [[ -n "$seen" ]] || { echo unknown; return; }
  case "$seen" in
    ok | failed) echo "$seen" ;;
    *) echo finding ;;
  esac
}

# The greatest of the halves' verdicts in ok < unknown < failed < finding.
verdict_all() {
  local h v all=ok
  for h in "${halves[@]}"; do
    v="$(verdict "$h" "$1")"
    (($(rank "$v") <= $(rank "$all"))) || all="$v"
  done
  echo "$all"
}
rank() { case "$1" in ok) echo 0 ;; unknown) echo 1 ;; failed) echo 2 ;; finding) echo 3 ;; esac; }

# record <half> <commit> <code>: 0 ok, 75 no verdict (did not start), else failed.
record() {
  local half="$1" commit="$2" code="$3" file
  ((code == 75)) && { say "the $half half did not start, so it says nothing of the tree"; return 2; }
  file="$(verdicts "$half" "$commit" "$( ((!full)) || echo full)")"
  mkdir -p "$(dirname "$file")"
  printf '%s %s %s\n' "$( ((code)) && echo failed || echo ok)" "$commit" "$(date -u +%FT%TZ)" >>"$file"
  return $((code ? 1 : 0))
}

log_of() { mkdir -p "$state/logs" && echo "$state/logs/$1-$(git rev-parse "$2^{tree}").log"; }

# Cargo's target is kept beside the worktree between gates.
worktree="$state/worktree"
target="$state/target"

# The worktree's path never changes, so cargo's fingerprints, which name it, still hold.
worktree_open() {
  worktree_close
  git worktree add --quiet --detach "$worktree" "$1" >/dev/null 2>&1 || return 1
  [[ ! -d "$target" ]] || { mkdir -p "$worktree/app" && mv "$target" "$worktree/app/target"; }
}

# Keeps the target. Safe from the exit trap and when there is no worktree.
worktree_close() {
  [[ -e "$worktree" ]] || { git worktree prune; return 0; }
  [[ ! -d "$worktree/app/target" || -e "$target" ]] || mv "$worktree/app/target" "$target"
  git worktree remove --force "$worktree" >/dev/null 2>&1 || rm -rf "$worktree"
  git worktree prune
}

# Gate <half> of <commit> and record the verdict: 0 green, 1 red, 2 no verdict.
run_half() {
  local half="$1" commit="$2" code=0
  if [[ "$half" == tree && -n "$cloud" ]]; then
    cloud_half "$commit"
    return
  fi
  run_here "$half" "$commit" || code=$?
  record "$half" "$commit" "$code"
}

# run_here <half> <commit> [gate args...]: in a worktree at <commit> with that tree's own gate,
# never handed train_reads. Exits as the gate did, or 75 ("did not start") with no worktree.
run_here() {
  local half="$1" commit="$2" code=0 gate unset=() name
  worktree_open "$commit" \
    || { say "no worktree could be made at ${commit:0:12}"; return 75; }
  gate="${SUZATARY_TRAIN_GATE:-$worktree/scripts/merge-gate.sh}"
  for name in "${train_reads[@]}"; do unset+=(-u "$name"); done
  # shellcheck disable=SC2046
  (cd "$worktree" && env "${unset[@]}" "$gate" $(gate_flags "$half") "${@:3}") 2>&1 \
    | tee "$(log_of "$half" "$commit")" >&2 || code=$?
  worktree_close
  return "$code"
}

# The train's own test seams and signing keys; a gate is handed none of them.
train_reads=(SUZATARY_TRAIN_BIN SUZATARY_TRAIN_GATE SUZATARY_TRAIN_GH SUZATARY_TRAIN_POLL
  SUZATARY_TRAIN_PUBLIC SUZATARY_TRAIN_CACHE_TIMEOUT SUZATARY_BOX_KEY SUZATARY_AGENT_KEY SUZATARY_DEDUP_KEY)
# Variables the gate's steps read: an inherited one makes the verdict about more than the tree.
gate_reads='^(SUZATARY_|CARGO|RUST|PROPTEST_|TYPST|NODE_OPTIONS$|LD_PRELOAD$|LD_LIBRARY_PATH$|LIBRARY_PATH$)'

# Refused unless inside `.#triage` with no `gate_reads` variable beyond the shell's own, the data
# tree's, SUZATARY_GATE_SERIAL (orders steps only) and the train's. SUZATARY_INSTANCE is made
# absolute: the worktree holds no link to the data tree.
gate_environment() {
  [[ -n "${IN_NIX_SHELL:-}" && "${SUZATARY_SHELL:-}" == triage ]] \
    || die "a gate runs inside the .#triage shell, whose tools are what it tests with: nix develop .#triage -c scripts/train.sh $*"
  local entry name found=()
  local allowed=" SUZATARY_SHELL SUZATARY_SHELL_SETS SUZATARY_INSTANCE SUZATARY_GATE_SERIAL ${train_reads[*]} ${SUZATARY_SHELL_SETS:-} "
  # From env(1): the shell's bash is built without `compgen`.
  while IFS= read -r -d '' entry; do
    name="${entry%%=*}"
    [[ "$name" =~ $gate_reads && "$allowed" != *" $name "* ]] || continue
    found+=("$name")
  done < <(env -0)
  ((${#found[@]} == 0)) \
    || die "the caller exported ${found[*]}, which the gate's steps read and the .#triage shell does not set, so the gate would test something beside the tree: unset $(printf '%s ' "${found[@]}")and gate again."
  if [[ -z "${SUZATARY_INSTANCE:-}" && -e "$root/instance.toml" ]]; then
    export SUZATARY_INSTANCE
    SUZATARY_INSTANCE="$(realpath "$root/instance.toml")"
  fi
}

# `--remote github`, or nothing.
remote_opt() {
  case "$#/${1:-}/${2:-}" in
    0//) ;;
    2/--remote/github) cloud=github ;;
    *) usage ;;
  esac
}

# The tree half's steps that read this private repository run on the box: before the push, and
# after the gate commit's publication run. Every other step is a job in gate.yml (a guard test
# holds the lists to every step of the half, each once).
box_before="fmt,changelog"
box_after="public-tree,deployed-shape"
# A run not ended in six hours (GitHub's job limit) is no verdict.
gate_workflow=.github/workflows/gate.yml
cloud_hours=6
# Seconds to wait for the publication run to appear, and to end (past its own 180 minutes).
cache_appear=600
cache_timeout="${SUZATARY_TRAIN_CACHE_TIMEOUT:-12000}"

core_repo() {
  local url
  url="$(git config --get remote.core.url)" || return 1
  [[ "$url" =~ github\.com[:/]([A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+)$ ]] || return 1
  echo "${BASH_REMATCH[1]%.git}"
}

# The tree half of <commit> on GitHub's runners, start to end: 0 green, 1 red, 2 no verdict.
cloud_half() {
  local commit="$1" pushed code=0
  pushed="$(cloud_push "$commit")" || return
  gate_branch="gate/$commit" gate_pushed="$pushed"
  cloud_build "$commit" "$pushed" || code=$?
  ((code)) || cloud_wait "$commit" "$pushed" || code=$?
  cloud_close
  return "$code"
}

# The box's steps, then the public tree pushed to core; prints the gate commit. Else 1 (box steps
# red, recorded) or 2 (no verdict: a leak, a gate that did not start, core unreachable), pushing
# nothing.
cloud_push() {
  local commit="$1" dir code=0 repo tree parent pushed
  repo="$(core_repo)" \
    || { say "the remote \`core\` is not a GitHub repository, so the tree half cannot run on GitHub's runners"; return 2; }
  dir="$(mktemp -d)" || return 2
  # The leak guard first: what fails it never leaves the box.
  if ! checked "$commit" "$dir"; then
    rm -rf "$dir"
    say "the public tree of ${commit:0:12} did not pass its leak guard (the files and lines above), so nothing went to $repo and the tree half says nothing of it; the live half's public-check judges it"
    return 2
  fi
  run_here tree "$commit" --steps "$box_before" || code=$?
  if ((code)); then
    rm -rf "$dir"
    record tree "$commit" "$code" || return
  fi
  if git ls-remote --exit-code --heads core "refs/heads/$core_branch" >/dev/null; then
    git fetch --quiet core "+refs/heads/$core_branch:refs/remotes/core/$core_branch" \
      || { rm -rf "$dir"; say "core's $core_branch could not be fetched, so nothing went to $repo"; return 2; }
    parent="$(git rev-parse "refs/remotes/core/$core_branch")"
  else
    parent=""
  fi
  tree="$(tree_of "$dir")" || { rm -rf "$dir"; say "the public tree of ${commit:0:12} could not be written as a git tree"; return 2; }
  rm -rf "$dir"
  # Stamped with the instant, so each gate's commit (and its run) has a sha of its own.
  pushed="$(public_commit "$tree" "$parent" "$(date -u +%FT%TZ)" "Gate ${commit:0:12}, $(date -u +%FT%T.%NZ)" "Gate: $commit")" \
    && [[ "$pushed" =~ ^[0-9a-f]{40}$ ]] \
    || { say "the public tree of ${commit:0:12} could not be committed"; return 2; }
  git push --quiet core "+$pushed:refs/heads/gate/$commit" \
    || { say "core refused gate/$commit, so the tree half says nothing of ${commit:0:12}"; return 2; }
  say "the tree half of ${commit:0:12} runs on GitHub's runners: $repo, branch gate/$commit, commit ${pushed:0:12}"
  echo "$pushed"
}

# The box's steps after the publication run of <pushed> answers: 0 when they pass, else the verdict
# recorded. The run is master's publication.yml: a gate branch's workflow never holds the cache
# token.
cloud_build() {
  local commit="$1" pushed="$2" repo run code=0
  repo="$(core_repo)"
  if ! timeout 60 "$gh" workflow run publication.yml --repo "$repo" --ref master -f "commit=$pushed" >&2; then
    say "could not start the publication run of ${pushed:0:12} on $repo (above): the box builds the public tree itself"
  else
    say "waiting for the publication run of ${pushed:0:12} in $repo before the box builds the public tree (at most ${cache_timeout}s)"
    run="$("$scripts/publication-run.sh" "$gh" "$repo" workflow_dispatch "$pushed" "$cache_appear" "$cache_timeout" "${SUZATARY_TRAIN_POLL:-60}")"
    case "$run" in
      success) say "the publication run of ${pushed:0:12} succeeded: the box's builds come from the binary cache" ;;
      failed*) say "the publication run of ${pushed:0:12} ended ${run#failed }: the box builds the public tree itself" ;;
      none) say "no publication run of ${pushed:0:12} appeared within ${cache_appear}s: the box builds the public tree itself" ;;
      running*) say "the publication run of ${pushed:0:12} was still ${run#running } after ${cache_timeout}s: the box builds the public tree itself" ;;
      unasked) say "could not ask GitHub for the publication run of ${pushed:0:12} (gh's words above): the box builds the public tree itself" ;;
    esac
  fi
  run_here tree "$commit" --steps "$box_after" || code=$?
  ((code == 0)) || record tree "$commit" "$code"
}

# `<id> <name>` per job of <repo>'s run <run> whose `red` step failed (its gate exited 1).
red_jobs() {
  timeout 60 "$gh" api "repos/$1/actions/runs/$2/jobs?per_page=100" \
    --jq '.jobs[] | select(any((.steps // [])[]; .name == "red" and .conclusion == "failure")) | "\(.id) \(.name)"'
}

# Records the gate run's verdict: red as soon as one job's `red` step failed; else, once it ends,
# success green and anything else none. Red jobs' logs go to the half's log.
cloud_wait() {
  local commit="$1" pushed="$2" repo log run="" status="" misses=0 red="" job name deadline
  repo="$(core_repo)"
  log="$(log_of tree "$commit")"
  deadline=$((SECONDS + cloud_hours * 3600))
  while [[ -z "$run" ]] && ((misses < 10 && SECONDS < deadline)); do
    sleep "${SUZATARY_TRAIN_POLL:-60}"
    run="$(timeout 60 "$gh" api "repos/$repo/actions/runs?head_sha=$pushed&event=push" \
      --jq ".workflow_runs[] | select(.path == \"$gate_workflow\") | .id" | awk 'NR == 1')" || run=""
    misses=$((misses + 1))
  done
  if [[ -z "$run" ]]; then
    say "no run of $gate_workflow started on $repo for ${pushed:0:12}, so the tree half says nothing of ${commit:0:12} (are Actions enabled? scripts/core-settings.sh)"
    return 2
  fi
  say "waiting on https://github.com/$repo/actions/runs/$run"
  misses=0
  # Status before jobs, so a run read as ended has every job's verdict in the jobs read after.
  while [[ -z "$red" && "$status" != completed\ * ]] && ((misses < 10 && SECONDS < deadline)); do
    sleep "${SUZATARY_TRAIN_POLL:-60}"
    if status="$(timeout 60 "$gh" api "repos/$repo/actions/runs/$run" --jq '.status + " " + (.conclusion // "")')" \
      && red="$(red_jobs "$repo" "$run")"; then
      misses=0
    else
      misses=$((misses + 1)) status="" red=""
    fi
  done
  if [[ -z "$red" && "$status" != completed\ * ]]; then
    say "the run $run on $repo did not end (${status:-unreachable}), so the tree half says nothing of ${commit:0:12}"
    return 2
  fi
  if [[ "$status" == "completed success" ]]; then
    echo "run $run: success" >>"$log"
    record tree "$commit" 0
    return
  fi
  if [[ -z "$red" ]]; then
    say "the run $run ended ${status#completed }, and no job's gate found the tree red (a runner lost, a cancel, a gate that did not start), so the tree half says nothing of ${commit:0:12}"
    return 2
  fi
  # Strip each line's timestamp, so failing_lines reads the gate's sections as it does here.
  while read -r job name; do
    say "red: $name"
    { echo "--- job $name"
      timeout 60 "$gh" api "repos/$repo/actions/jobs/$job/logs" | sed -E 's/^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9:.]+Z //' ||
        echo "(its log could not be read)"; } >>"$log"
  done <<<"$red"
  record tree "$commit" 1
}

# Cancel the gate commit's unfinished runs (a red tree's builds should not reach the cache), then
# delete the gate branch.
cloud_close() {
  local repo id
  if repo="$(core_repo)" && [[ -n "$gate_pushed" ]]; then
    while read -r id; do
      [[ -z "$id" ]] || timeout 60 "$gh" api "repos/$repo/actions/runs/$id/cancel" -X POST >/dev/null \
        || say "could not cancel the run $id on $repo: cancel it by hand (gh run cancel $id --repo $repo)"
    done < <(timeout 60 "$gh" api "repos/$repo/actions/runs?per_page=100" \
      --jq ".workflow_runs[] | select(.status != \"completed\" and (.head_sha == \"$gate_pushed\" or .display_title == \"Publication cache $gate_pushed\")) | .id" \
      || say "could not list the runs of ${gate_pushed:0:12} on $repo to cancel them")
  fi
  git push --quiet core --delete "refs/heads/$gate_branch" 2>/dev/null \
    || say "could not delete core's $gate_branch: delete it by hand (git push core --delete $gate_branch)"
  gate_branch="" gate_pushed=""
}

on_train() {
  git rev-parse --verify --quiet train >/dev/null || die "there is no train; scripts/train.sh build <pr>... first."
  git checkout --quiet train
}

# Read once: what a gate or a bisect judges, whatever the checkout does after.
train_commit() {
  git rev-parse --verify --quiet "train^{commit}" || die "there is no train; scripts/train.sh build <pr>... first."
}

# Monitor mode makes its pid a process group, so live_stop reaches every process by that pid,
# never by a pattern, which could match the caller's shell.
live_start() {
  set -m
  run_half live "$1" </dev/null &
  live=$!
  set +m
}

# TERM to the group, KILL five seconds later; no verdict is recorded.
live_stop() {
  local tries=0
  kill -TERM -- "-$live" 2>/dev/null || true
  wait "$live" 2>/dev/null || true
  while kill -0 -- "-$live" 2>/dev/null && ((tries++ < 50)); do sleep 0.1; done
  kill -KILL -- "-$live" 2>/dev/null || true
  live=""
  worktree_close
}

# beside <commit>: the live half here beside the background tree half ($there), until both end or
# one ends red, which stops the other at once.
beside() {
  local commit="$1" ended code=0
  live_start "$commit"
  wait -n -p ended "$there" "$live" || code=$?
  if [[ "$ended" == "$there" ]]; then
    there=""
    if ((code == 1)); then
      say "the tree half is red, so the live half is stopped"
      live_stop
    else
      wait "$live" || true
      live=""
    fi
  else
    live=""
    if ((code == 1)); then
      say "the live half is red, so the tree half is stopped"
      kill "$there" 2>/dev/null || true
      wait "$there" 2>/dev/null || true
      there=""
    else
      wait "$there" || true
      there=""
    fi
  fi
}

# Halves side by side when the tree half is remote, in sequence here.
gate() {
  gate_environment gate "$@"
  if [[ "${1:-}" == --full ]]; then full=1 && shift; fi
  remote_opt "$@"
  sync
  local commit
  commit="$(train_commit)"
  local code=0
  if [[ -n "$cloud" ]]; then
    # The box's steps first, so the live half's deploy-build finds their builds in the store.
    local pushed
    pushed="$(cloud_push "$commit")" || code=$?
    if ((code == 0)); then
      gate_branch="gate/$commit" gate_pushed="$pushed"
      cloud_build "$commit" "$pushed" || code=$?
      if ((code == 0)); then
        cloud_wait "$commit" "$pushed" &
        there=$!
        beside "$commit"
      fi
      cloud_close
    fi
    case "$code" in
      1) say "the tree half is red on the box, so the live half does not run" ;;
      2) run_half live "$commit" || true ;;
    esac
  else
    run_half tree "$commit" || code=$?
    if ((code == 1)); then
      say "the tree half is red, so the live half does not run"
    else
      run_half live "$commit" || true
    fi
  fi
  report "$commit"
}

# Per `--- job <name>` section of a red half's <log>: the gate's "failing steps' last lines"
# block, else its last 20 lines naming an error, each failed test (`✖`, `not ok`) with its
# `Error`, `waited` and `last:` lines, once each and uncoloured.
failing_lines() {
  awk '
    function flush(  i) {
      if (!block) for (i = (n > 20 ? n - 19 : 1); i <= n; i++) print errors[i]
      n = 0
      delete seen
    }
    function keep(line) { if (!(line in seen)) { seen[line]; errors[++n] = line } }
    { gsub(/\033\[[0-9;]*m/, "") }
    /^--- job / { flush(); block = inside = under = 0; print; next }
    /^==> the failing steps. last lines$/ { block = inside = 1; print; next }
    inside && /^(size: |free after the gate: |##\[)/ { inside = 0 }
    inside { print; next }
    /✖|not ok/ { keep($0); under = 6; next }
    under > 0 && /Error|waited|last:/ { keep($0); under--; next }
    under > 0 && !/^[[:space:]]/ { under = 0 }
    /FAILED|panicked|^error|^##\[error\]/ { keep($0) }
    END { flush() }' "$1"
}

# Succeeds when <commit> may land.
report() {
  local commit="$1" h v short
  short="$(git rev-parse --short "$commit")"
  for h in "${halves[@]}"; do
    v="$(verdict "$h" "$commit")"
    say "$h half: $v"
    [[ "$v" != failed || ! -s "$(log_of "$h" "$commit")" ]] || failing_lines "$(log_of "$h" "$commit")" >&2
  done
  case "$(verdict_all "$commit")" in
    ok) say "green: the train $short may land (scripts/train.sh land)" ;;
    finding) die "red, where a half once passed this tree: the gate is not a function of the tree. The logs are in $state/logs/." ;;
    unknown) die "a half has no verdict on $short: gate again." ;;
    failed) die "red: scripts/train.sh bisect finds the merge that broke it. The logs are in $state/logs/." ;;
  esac
}

pr_of() { git log -1 --format=%s "$1" | sed -n 's/^Merge #\([0-9]*\):.*/\1/p'; }

# Its own PR, then those it carried.
prs_of() { pr_of "$1" && trailer_of "$1" Carries | tr ' ' '\n' | sed '/^$/d'; }

# The longest prefix of a red train (by merges; prefix 0 is master) that the first red half
# passes. A prefix gated before is read from the table. Assumes a red prefix stays red.
bisect() {
  gate_environment bisect "$@"
  remote_opt "$@"
  sync
  local half="" h train
  train="$(train_commit)"
  case "$(verdict_all "$train")" in
    failed)
      for h in "${halves[@]}"; do
        if [[ -z "$half" && "$(verdict "$h" "$train")" == failed ]]; then half="$h"; fi
      done
      ;;
    ok) die "the train is green; there is nothing to bisect." ;;
    unknown) die "the gate has not judged the train's HEAD in both halves: scripts/train.sh gate first." ;;
    finding) die "the gate both passed and failed the train's tree: it is not a function of the tree, so no prefix's verdict can be trusted either." ;;
  esac
  local merges=() lo=0 hi mid probes=0 commit seen prs=()
  # The release commit on top is no PR; it is blamed as itself below.
  for commit in $(git rev-list --first-parent --reverse "$(git merge-base origin/master "$train")..$train"); do
    [[ -z "$(pr_of "$commit")" ]] || merges+=("$commit")
  done
  hi=${#merges[@]}
  ((hi)) || die "the train merges no PR, so there is nothing to bisect."
  if [[ "${merges[hi - 1]}" != "$train" ]]; then
    commit="${merges[hi - 1]}"
    seen="$(verdict "$half" "$commit")"
    if [[ "$seen" == unknown ]]; then
      say "probe: every merge, through #$(pr_of "$commit"), without the release on top"
      run_half "$half" "$commit" || true
      seen="$(verdict "$half" "$commit")"
    fi
    if [[ "$seen" == ok ]]; then
      printf 'green in the %s half: master and every merge of the train\n' "$half"
      printf 'broke it: %s\n' "$(git log -1 --format=%s "$train")"
      printf 'next: scripts/train.sh build %s, then gate (the release is cut again)\n' \
        "$(for commit in "${merges[@]}"; do prs_of "$commit"; done | paste -sd' ')"
      return
    fi
  fi
  while ((1 << probes < hi)); do probes=$((probes + 1)); done
  say "bisecting ${hi} merges over the $half half: at most ${probes} probes, each that half of the merge gate (a prefix gated before is read from the table)"
  while ((hi - lo > 1)); do
    mid=$(((lo + hi) / 2))
    commit="${merges[mid - 1]}"
    seen="$(verdict "$half" "$commit")"
    if [[ "$seen" == unknown ]]; then
      say "probe: the first $mid merges, through #$(pr_of "$commit")"
      run_half "$half" "$commit" || true
      seen="$(verdict "$half" "$commit")"
    fi
    case "$seen" in
      ok) lo=$mid ;;
      failed) hi=$mid ;;
      unknown) die "the $half half gave no verdict on the first $mid merges, so the search stops here." ;;
      *) die "the gate both passed and failed the first $mid merges' tree: it is not a function of the tree, so the search stops here." ;;
    esac
  done
  for commit in "${merges[@]:0:lo}"; do prs+=("#$(pr_of "$commit")"); done
  printf 'green in the %s half: master and the first %s merges %s\n' "$half" "$lo" "${prs[*]:-}"
  printf 'broke it: %s\n' "$(git log -1 --format=%s "${merges[hi - 1]}")"
  # The broken merge's own PR is left out; the PRs it carried go on, to be judged alone.
  prs=()
  for commit in "${merges[@]}"; do
    if [[ "$commit" == "${merges[hi - 1]}" ]]; then
      mapfile -t -O "${#prs[@]}" prs < <(prs_of "$commit" | tail -n +2)
    else
      mapfile -t -O "${#prs[@]}" prs < <(prs_of "$commit")
    fi
  done
  printf 'next: scripts/train.sh build %s, then gate\n' "${prs[*]:-}"
}

# Both halves run whatever the first says, so one night names every red step.
nightly_gate() {
  (($# == 0)) || usage
  gate_environment nightly
  full=1 nightly=1
  sync
  local commit h code failed=() unknown=()
  commit="$(git rev-parse --verify origin/master)"
  for h in "${halves[@]}"; do
    code=0
    run_half "$h" "$commit" || code=$?
    case "$code" in
      0) ;;
      1) failed+=("$h") ;;
      *) unknown+=("$h") ;;
    esac
  done
  code=0
  report_nightly "$commit" "${failed[@]}" || code=$?
  ((${#unknown[@]} == 0)) || die "the ${unknown[*]} half gave no verdict on origin/master ${commit:0:12}; the logs are in $state/logs/."
  return "$code"
}

# The failing steps of <commit>'s red halves, from the gate's summary rows, filed once per tree.
report_nightly() {
  local commit="$1" h steps=() logs=() id
  shift
  for h in "${halves[@]}"; do say "$h half: $(verdict "$h" "$commit" full)"; done
  (($#)) || { say "the nightly gate passed origin/master ${commit:0:12}"; return 0; }
  for h; do
    logs+=(".git/train/logs/${h}-$(git rev-parse "$commit^{tree}").log")
    mapfile -t -O "${#steps[@]}" steps < <(sed -n 's/^\([a-z][a-z0-9-]*\) \{1,\}FAILED.*/\1/p' "$(log_of "$h" "$commit")" | awk '!seen[$0]++')
  done
  ((${#steps[@]})) || steps=("$@")
  id="nightly$(git rev-parse --short=8 "$commit^{tree}")"
  resolve_app origin/master
  if "$app_bin" todo show --chain suzatary "$id" >/dev/null 2>&1; then
    say "red: $id already names this tree's failing steps on the project chain"
  else
    "$app_bin" todo add --chain suzatary --actor assistant --kind suzateryTodo --priority 55 "$id" \
      --body "The nightly gate is red on master ${commit:0:12}: ${steps[*]}." \
      --detail "The gate's logs are in ${logs[*]}." \
      --rationale "A step red on master either let a broken change land or no longer reads the data, and every later train inherits it." >&2 \
      || die "the nightly gate is red (${steps[*]}), and the todo naming it could not be filed (above). The logs are in $state/logs/."
    say "red: filed todo:suzatary/$id for ${steps[*]}"
  fi
  return 1
}

# HEAD's release version, once the chain with its todos done reads at least it and origin has no
# such tag. Asked before anything is pushed or written.
checked_release() {
  [[ -f app/core/releases.toml ]] || return 0
  local version
  version="$(trailer_of HEAD Release)"
  [[ -n "$version" ]] \
    || die "the train's HEAD is not its release (no Release: trailer), so it has no version to tag: scripts/train.sh build again. Nothing landed."
  # Only origin's tags count; a local one is a refused land's leftover, replaced on tagging.
  ! git ls-remote --exit-code --tags origin "refs/tags/v$version" >/dev/null \
    || die "origin has v$version already, so this train's release would be a second $version: scripts/train.sh build again, which reads the chain afresh. Nothing landed."
  resolve_app HEAD
  "$app_bin" release check --tree "$root" >/dev/null \
    || die "the project chain, with this train's todos done, reads below $version (above): scripts/train.sh build again. Nothing landed."
  echo "$version"
}

# record_release <rev> <from>: records <rev>'s release on the chain with <from>'s binary, each todo
# noted with its PR. Idempotent; the release files are extracted, so no checkout moves.
record_release() {
  local rev="$1" from="$2" version tree subject release completes todo out notes=()
  version="$(trailer_of "$rev" Release)"
  [[ -n "$version" ]] || die "${rev:0:12} is not a release (no Release: trailer); nothing to record."
  # The PRs since the previous release, in one pass of the log (a call per commit is slow).
  while IFS=$'\x1f' read -r subject release completes; do
    [[ -z "$release" ]] || break
    [[ "$subject" =~ ^Merge\ \#([0-9]+): ]] || continue
    for todo in $completes; do notes+=(--note "$todo=PR #${BASH_REMATCH[1]}"); done
  done < <(git log --first-parent \
    --format='%s%x1f%(trailers:key=Release,valueonly,separator= )%x1f%(trailers:key=Completes,valueonly,separator= )' \
    "$rev^")
  tree="$(mktemp -d)"
  if ! git archive "$rev" app/core/releases.toml app/release.toml \
    | tar -x -C "$tree"; then
    rm -rf "$tree"
    die "${rev:0:12}'s release files could not be read: scripts/train.sh record ${rev:0:12}"
  fi
  resolve_app "$from"
  out="$("$app_bin" release record --tree "$tree" "${notes[@]}")" \
    || { rm -rf "$tree"; die "$version is not recorded on the project chain (above): scripts/train.sh record ${rev:0:12}"; }
  rm -rf "$tree"
  printf 'recorded: %s for %s\n' "$(sed -n 's/.*"recorded":\[\([^]]*\)\].*/\1/p' <<<"$out" | tr -d '"' | sed 's/^$/nothing more/')" "$version"
}

# `record [<rev>]`: only a release on origin/master's first-parent chain (what landed) is recorded.
record_done() {
  (($# <= 1)) || usage
  fetch_origin
  local asked rev
  asked="${1:-$(newest_release origin/master)}"
  [[ -n "$asked" ]] || die "origin/master holds no release; nothing to record."
  rev="$(git rev-parse --verify --quiet "$asked^{commit}")" || die "\"$asked\" is no commit."
  # grep -c reads to the end: -q would leave early and SIGPIPE rev-list under pipefail.
  (($(git rev-list --first-parent origin/master | grep -cxF "$rev"))) \
    || die "${rev:0:12} is not on origin/master's first-parent chain, so it never landed; nothing recorded."
  record_release "$rev" origin/master
  mkdir -p "$state/recorded" && : >"$state/recorded/$rev"
}

land() {
  (($# == 0)) || usage
  clean
  sync
  on_train
  local short
  short="$(git rev-parse --short HEAD)"
  local h missing=()
  for h in "${halves[@]}"; do [[ "$(verdict "$h" HEAD)" == ok ]] || missing+=("$h: $(verdict "$h" HEAD)"); done
  case "$(verdict_all HEAD)" in
    ok) ;;
    unknown) die "the gate has not passed the train's HEAD ($short) in both halves (${missing[*]}): scripts/train.sh gate first. Nothing landed." ;;
    failed) die "the gate failed on the train's HEAD ($short) (${missing[*]}): scripts/train.sh bisect, build without the merge it names, gate again. Nothing landed." ;;
    finding) die "the gate both passed and failed the train's tree ($short) (${missing[*]}), so neither verdict lands: it read something beyond the tree. Nothing landed." ;;
  esac
  # A master that moved since makes a different tree, however disjoint (git merges text, not
  # types), so the train is built and gated again.
  git fetch --quiet origin master
  if ! git merge-base --is-ancestor origin/master HEAD; then
    local prs=() commit
    for commit in $(git rev-list --first-parent --reverse "$(git merge-base origin/master HEAD)..HEAD"); do
      mapfile -t -O "${#prs[@]}" prs < <(prs_of "$commit")
    done
    ((${#prs[@]})) || die "master moved since the train was gated, and the train merges no PR to build again on it. Nothing landed."
    say "master moved to $(git rev-parse --short origin/master) since the train was gated, so its tree is not the one that would land; building the train again on it"
    build "${prs[@]}" >&2
    die "the train is built again: scripts/train.sh gate, then land. Nothing landed."
  fi
  local version tag=()
  [[ ! -f app/core/releases.toml ]] || resolve_app HEAD
  # A refusal exits this substitution, and its failure stops the land here.
  version="$(checked_release)"
  # Pushed from a detached HEAD, so a refused push leaves the gated train as it was. The tag rides
  # the same atomic push: master and v<version> move together or neither does.
  return_to=train
  git checkout --quiet --detach
  if [[ -n "$version" ]]; then
    git tag --force --annotate --message "Suzatary $version" "v$version" HEAD >/dev/null
    tag=("refs/tags/v$version")
  fi
  if ! git push --quiet --atomic origin HEAD:refs/heads/master "${tag[@]}"; then
    [[ -z "$version" ]] || git tag --delete "v$version" >/dev/null
    die "origin refused master at $(git rev-parse --short HEAD) (did it move again?); the train is as it was, so land again. Nothing landed."
  fi
  git branch --quiet --force train HEAD
  git fetch --quiet . HEAD:master 2>/dev/null || say "the local master did not move (checked out elsewhere?)"
  local landed
  landed="$(git rev-parse HEAD)"
  printf 'landed: master is %s%s\n' "$landed" "${version:+, tagged v$version}"
  # Recorded only once origin holds the release; a failure still lets the publication run.
  local recorded=0
  if [[ -n "$version" ]]; then
    if (record_release "$landed" "$landed"); then
      mkdir -p "$state/recorded" && : >"$state/recorded/$landed"
    else
      recorded=1
    fi
  fi
  (publish "$landed") \
    || die "master landed at ${landed:0:12}, and its public tree was not published, so no deploy can run it yet; scripts/train.sh publish, once the cause above is mended."
  ((recorded == 0)) \
    || die "master landed and v$version is tagged, and its todos are not all recorded done: scripts/train.sh record ${landed:0:12}"
  printf 'next, by the operator (this script deploys nothing):\n'
  printf '  scripts/deploy.sh %s\n' "$landed"
}

core_branch=master
# A line of it is a person's word that they looked at those bytes: a new one is held as a new
# path is.
admitted=scripts/public-admitted.sha256
# A change to the secret rules changes what passes, so it is admitted as a line is.
rules=scripts/secret-rules.toml

# The core commit whose Source is <rev>, or nothing. awk reads the whole log, so no closed pipe
# reads as a failure under pipefail.
published() {
  git rev-parse --verify --quiet "refs/remotes/core/$core_branch" >/dev/null || return 0
  git log --first-parent --format='%H%x09%(trailers:key=Source,valueonly,separator=%x2C)' \
    "refs/remotes/core/$core_branch" | awk -F'\t' -v rev="$1" '$2 == rev && !n++ { print $1 }'
}

skipped_by() {
  git rev-parse --verify --quiet "refs/remotes/core/$core_branch" >/dev/null || return 0
  git log --first-parent --format='%H%x09%(trailers:key=Skipped,valueonly,separator=%x09)' \
    "refs/remotes/core/$core_branch" | awk -F'\t' -v rev="$1" '{ for (i = 2; i <= NF; i++) if ($i == rev && !n++) print $1 }'
}

# The newest source published, or nothing.
newest() {
  git rev-parse --verify --quiet "refs/remotes/core/$core_branch" >/dev/null || return 0
  git log --first-parent --format='%(trailers:key=Source,valueonly,separator=%x2C)' \
    "refs/remotes/core/$core_branch" | awk 'NF && !n++'
}

# Publishes the rows core lacks up to <rev> (default origin/master), oldest first, each only once
# its leak guard passes whole; the first that fails stops the walk.
publish() {
  local rev heads last source sources=() skips=() skipped=() at
  # Read by `row`.
  local admits=()
  while [[ "${1:-}" == --skip || "${1:-}" == --admit ]]; do
    (($# >= 2)) || usage
    if [[ "$1" == --admit ]]; then
      [[ -t 0 ]] \
        || die "--admit $2 is a person's word that they looked at it, so it is taken at a terminal and never from a script. Nothing published."
      admits+=("$2")
    else
      at="$(git rev-parse --verify --quiet "$2^{commit}")" || die "--skip ${2:-\"\"} names no commit. Nothing published."
      skips+=("$at")
    fi
    shift 2
  done
  (($# <= 1)) || usage
  # An empty <rev> is an upstream mistake, never "the tip": publishing the tip instead would leave
  # the meant rev below the table forever.
  (($# == 0)) || [[ -n "$1" ]] || die "publish was given an empty <rev>. Nothing published."
  git remote get-url core >/dev/null 2>&1 \
    || die "this checkout has no remote \`core\`, the repository a deployment runs: git remote add core <its url>. Nothing published."
  git fetch --quiet origin master || die "could not fetch origin's master. Nothing published."
  rev="$(git rev-parse --verify --quiet "${1:-origin/master}^{commit}")" || die "${1:-origin/master} names no commit. Nothing published."
  on_chain "$rev" origin/master \
    || die "${rev:0:12} is not on master's first-parent chain: only a landed commit is published. Nothing published."
  heads="$(git ls-remote --heads core "refs/heads/$core_branch")" || die "could not reach core. Nothing published."
  if [[ -n "$heads" ]]; then
    git fetch --quiet core "+refs/heads/$core_branch:refs/remotes/core/$core_branch" \
      || die "could not fetch core's $core_branch. Nothing published."
  else
    git update-ref -d "refs/remotes/core/$core_branch" 2>/dev/null || true
  fi
  source="$(published "$rev")"
  if [[ -n "$source" ]]; then
    say "core already publishes ${rev:0:12}, as ${source:0:12}"
    return 0
  fi
  source="$(skipped_by "$rev")"
  [[ -z "$source" ]] \
    || die "${rev:0:12} was skipped by publish, never published (${source:0:12} of core's $core_branch names it in a Skipped: trailer): its tree did not pass the leak guard. Nothing published."
  last="$(newest)"
  if [[ -z "$last" ]]; then
    sources=("$rev")
  else
    git merge-base --is-ancestor "$last" "$rev" 2>/dev/null \
      || die "core publishes ${last:0:12}, and ${rev:0:12} does not descend from it: the table only climbs. Nothing published."
    mapfile -t sources < <(git rev-list --first-parent --reverse "$last..$rev")
  fi
  ((${#sources[@]})) || die "found no commit between ${last:0:12} and ${rev:0:12} to publish. Nothing published."
  for at in "${skips[@]}"; do
    printf '%s\n' "${sources[@]}" | awk -v r="$at" '$0 == r { f = 1 } END { exit !f }' \
      || die "--skip ${at:0:12} is not a commit this publish would make a row of. Nothing published."
    [[ "$at" != "${sources[-1]}" ]] \
      || die "--skip ${at:0:12} is the last commit to publish, so no row after it would record the skip; publish again once another commit lands. Nothing published."
    # A skip is for a tree that can never pass: a skipped commit is never deployed.
    ! checked "$at" \
      || die "--skip ${at:0:12}: ${at} passes its leak guard; publish it instead. Nothing published."
  done
  # Read by `row`. An admit must name one of these paths.
  local pending=() fresh paths
  for source in "${sources[@]}"; do
    printf '%s\n' "${skips[@]}" | awk -v r="$source" '$0 == r { f = 1 } END { exit !f }' || pending+=("$source")
  done
  fresh="$(fresh_rows "${pending[@]}")" || die "could not write the public tree to list the paths it would publish. Nothing published."
  # A here-string, not a pipe: `grep -q` exits early and the writer's SIGPIPE reads as no match.
  paths="$(cut -d' ' -f2- <<<"$fresh")"
  for at in "${admits[@]}"; do
    grep -qxF -- "$at" <<<"$paths" \
      || die "--admit $at names no path this publish would hold: admit only what a held row names. Nothing published."
  done
  for source in "${sources[@]}"; do
    if printf '%s\n' "${skips[@]}" | awk -v r="$source" '$0 == r { f = 1 } END { exit !f }'; then
      say "skipping ${source:0:12}, as asked: the next row names it"
      skipped+=("$source")
      continue
    fi
    row "$source" "${skipped[@]}" || return 1
    skipped=()
  done
}

# on_chain <commit> <branch>. awk reads to the end: a reader quitting at its first match (the
# box's grep is ugrep) breaks the pipe, which pipefail reads as "not on it".
on_chain() {
  git rev-list --first-parent "$2" | awk -v r="$1" '$0 == r { f = 1 } END { exit !f }'
}

# checked <source> [<dir>]: the leak guard on <source>'s public tree (kept in <dir>/tree if given):
# 0 passes, 1 hits (on stderr). A guard that could not run dies, so a skip is never taken on a
# check that did not run.
checked() {
  local source="$1" dir="${2:-}" own="" code=0 public
  public="${SUZATARY_TRAIN_PUBLIC:-$root/scripts/public-tree.sh}"
  if [[ -z "$dir" ]]; then
    dir="$(mktemp -d)" || die "could not make a scratch directory to check ${source:0:12}. Nothing published."
    own=yes
  fi
  if ! "$public" "$dir/tree" "$source" >&2; then
    rm -rf "$dir"
    die "could not write the public tree of ${source:0:12}, so its leak guard did not run. Nothing published."
  fi
  "$public" --check --strict "$dir/tree" >&2 || code=$?
  [[ -z "$own" ]] || rm -rf "$dir"
  case "$code" in
    0 | 1) return "$code" ;;
    *)
      rm -rf "$dir"
      die "the leak guard could not run over the public tree of ${source:0:12} (it exited $code; see above). Nothing published."
      ;;
  esac
}

# "<source> <path>" for each path, or admission (admitted-file line, .gitleaksignore fingerprint,
# secret rules digest), of each <source> that no core row or earlier source holds. None while core
# is empty: the first publication is the operator's baseline. Admissions are read by
# `suzatary leaks --admissions`.
fresh_rows() {
  local public scratch s c bin
  git rev-parse --verify --quiet "refs/remotes/core/$core_branch" >/dev/null || return 0
  public="${SUZATARY_TRAIN_PUBLIC:-$root/scripts/public-tree.sh}"
  bin="${SUZATARY_TRAIN_BIN:-}"
  [ -n "$bin" ] || bin="$(SUZATARY_APP_REV="$(git rev-parse origin/master)" "$root/scripts/app-bin.sh")/bin/suzatary" || return 1
  scratch="$(mktemp -d)" || return 1
  # `<path>\t<atom>` per line.
  admissions_of() {
    local dir="$1" manifest="$1/$admitted" ignore="$1/.gitleaksignore"
    [ -f "$manifest" ] || manifest=/dev/null
    [ -f "$ignore" ] || ignore=/dev/null
    "$bin" leaks --terms /dev/null --words /dev/null --links /dev/null --admissions \
      --admitted "$manifest" --findings "$ignore" || return 1
    [ ! -f "$dir/$rules" ] || printf '%s\trules %s\n' "$rules" "$(sha256sum <"$dir/$rules" | cut -d' ' -f1)"
  }
  # NUL-separated end to end, so no path is quoted and every one compares as `find` prints it.
  git log -z --format= --name-only "refs/remotes/core/$core_branch" | grep -zv '^$' |
    LC_ALL=C sort -zu >"$scratch/known" || { rm -rf "$scratch"; return 1; }
  # Once published, an admission stands.
  : >"$scratch/admitted"
  git log --format=%H "refs/remotes/core/$core_branch" -- "$admitted" .gitleaksignore "$rules" >"$scratch/rows" ||
    { rm -rf "$scratch"; return 1; }
  while IFS= read -r c; do
    rm -rf "$scratch/row" && mkdir -p "$scratch/row/$(dirname "$admitted")" "$scratch/row/$(dirname "$rules")"
    for f in "$admitted" .gitleaksignore "$rules"; do
      git show "$c:$f" >"$scratch/row/$f" 2>/dev/null || rm -f "$scratch/row/$f"
    done
    admissions_of "$scratch/row" >>"$scratch/admitted" || { rm -rf "$scratch"; return 1; }
  done <"$scratch/rows"
  LC_ALL=C sort -u "$scratch/admitted" -o "$scratch/admitted"
  for s in "$@"; do
    rm -rf "$scratch/tree"
    if ! "$public" "$scratch/tree" "$s" >/dev/null 2>&1; then
      rm -rf "$scratch"
      say "could not write the public tree of ${s:0:12} to list its paths"
      return 1
    fi
    (cd "$scratch/tree" && find . -type f -printf '%P\0' | LC_ALL=C sort -z) >"$scratch/paths"
    admissions_of "$scratch/tree" | LC_ALL=C sort -u >"$scratch/lines" ||
      { rm -rf "$scratch"; say "could not read the admissions of ${s:0:12}"; return 1; }
    {
      LC_ALL=C comm -z -23 "$scratch/paths" "$scratch/known" | tr '\0' '\n'
      LC_ALL=C comm -23 "$scratch/lines" "$scratch/admitted" | cut -f1
    } | LC_ALL=C sort -u | sed "s#^#$s #"
    LC_ALL=C sort -zu "$scratch/known" "$scratch/paths" -o "$scratch/known"
    LC_ALL=C sort -u "$scratch/admitted" "$scratch/lines" -o "$scratch/admitted"
  done
  rm -rf "$scratch"
}

# Writes <dir>/tree as a git tree and prints its id, through its own index so the checkout's is
# untouched; forced, since a published file is published even where a `.gitignore` names it.
tree_of() {
  local gitdir
  gitdir="$(git rev-parse --absolute-git-dir)" &&
    (cd "$1/tree" && export GIT_DIR="$gitdir" GIT_WORK_TREE=. GIT_INDEX_FILE="$1/index" &&
      git add --all --force . && git write-tree)
}

# public_commit <tree> <parent> <when> <subject> <trailers>, by a fixed author: no person's name.
public_commit() {
  GIT_AUTHOR_NAME=Suzatary GIT_AUTHOR_EMAIL=publish@suzatary.invalid GIT_AUTHOR_DATE="$3" \
    GIT_COMMITTER_NAME=Suzatary GIT_COMMITTER_EMAIL=publish@suzatary.invalid GIT_COMMITTER_DATE="$3" \
    git commit-tree "$1" ${2:+-p "$2"} -m "$4" -m "$5"
}

# One row: <source>'s public tree, checked, committed on core's master and pushed. Nothing is
# pushed unless it is a commit: pushing an empty name would delete core's master.
row() {
  local source="$1" dir tree parent commit when trailers s new held every path line
  # One paragraph, so git reads every line as a trailer.
  trailers="Source: $source"
  for s in "${@:2}"; do trailers+=$'\n'"Skipped: $s"; done
  dir="$(mktemp -d)" || die "could not make a scratch directory. Nothing published."
  if ! checked "$source" "$dir"; then
    rm -rf "$dir"
    say "the public tree of ${source:0:12} did not pass its leak guard (the files and lines above). Nothing published from it on. If a term added to the lists since it landed matches it, it can never pass: scripts/train.sh publish --skip ${source}"
    return 1
  fi
  new="$(printf '%s\n' "$fresh" | sed -n "s#^$source ##p")"
  held="$(printf '%s\n' "$new" | grep -vxF -f <(printf '%s\n' "${admits[@]}" "") | grep . || true)"
  if [[ -n "$held" ]]; then
    rm -rf "$dir"
    # Every row's held paths, so one look admits them all; a path keeps its spaces.
    every="$(while IFS= read -r line; do
      s="${line%% *}" path="${line#* }"
      grep -qxF -- "$path" <<<"$(printf '%s\n' "${admits[@]}")" || printf '%s %s\n' "${s:0:12}" "$path"
    done <<<"$fresh")"
    printf '%s\n' "$every" | sed 's/^\([^ ]*\) \(.*\)$/held: \2 (in \1)/' >&2
    say "the public tree of ${source:0:12} holds a path no publication has held; every row's are above. Nothing published from it on. Look at each, then at a terminal: scripts/train.sh publish$(while IFS= read -r line; do printf " --admit %q" "${line#* }"; done <<<"$every")"
    return 1
  fi
  while IFS= read -r s; do
    [[ -z "$s" ]] || trailers+=$'\n'"Admitted: $s"
  done <<<"$new"
  tree="$(tree_of "$dir")" ||
    { rm -rf "$dir"; die "could not write the public tree of ${source:0:12} as a git tree. Nothing published."; }
  rm -rf "$dir"
  parent="$(git rev-parse --verify --quiet "refs/remotes/core/$core_branch")" || parent=""
  when="$(git log -1 --format=%cI "$source")" && [[ -n "$when" ]] \
    || die "could not read ${source:0:12}'s date. Nothing published."
  commit="$(public_commit "$tree" "$parent" "$when" "Publish ${source:0:12}" "$trailers")" \
    || die "could not commit the public tree of ${source:0:12}. Nothing published."
  [[ "$commit" =~ ^[0-9a-f]{40}$ ]] \
    || die "committing the public tree of ${source:0:12} named no commit (\"$commit\"), and pushing it would delete core's $core_branch. Nothing published."
  git push --quiet core "$commit:refs/heads/$core_branch" \
    || die "core refused $core_branch at ${commit:0:12} (did it move?); scripts/train.sh publish again."
  git update-ref "refs/remotes/core/$core_branch" "$commit" \
    || die "core took ${commit:0:12}, but the local ref of it did not move; scripts/train.sh publish again reads it afresh."
  printf 'published: core %s is %s (Source: %s)\n' "$core_branch" "$commit" "$source"
}

case "${1:-}" in
  build) shift && build "$@" ;;
  gate) shift && gate "$@" ;;
  bisect) shift && bisect "$@" ;;
  land) shift && land "$@" ;;
  record) shift && record_done "$@" ;;
  nightly) shift && nightly_gate "$@" ;;
  publish) shift && sync && publish "$@" ;;
  *) usage ;;
esac
