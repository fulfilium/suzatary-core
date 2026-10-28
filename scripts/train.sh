#!/usr/bin/env bash
# THE MERGE QUEUE, ON THE BOX. GitHub's merge queue is for organisations'
# repositories and the heavy gate is this box's (scripts/merge-gate.sh), so
# reviewed pull requests ride a TRAIN: one integration branch, gated once,
# landed as one fast-forward of master.
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
#   scripts/train.sh publish [--skip <sha>]... [--admit <path>]... [<rev>]
#                                    core's master += one commit per landed commit up to <rev>
#                                    (master's tip), each the public tree of its source; a
#                                    skipped commit is named by the next one's `Skipped:` trailer,
#                                    an admitted path by its row's `Admitted:` trailer
#
# `gate --remote <codespace>` and `bisect --remote <codespace>` run the tree
# half in that Codespace of this repository (.devcontainer/), so the box keeps
# only the live half, the one that reads the owner's data. `--remote github`
# runs it on GitHub's runners in core instead (GITHUB'S RUNNERS, below).
#
# Run inside `nix develop .#triage`, from a clean checkout. It reads each PR's
# title, and reaches a Codespace and core's runs, through `gh`, which authenticates as itself;
# it never creates or deletes a Codespace, never promotes and never touches
# the data repository.
#
# WHAT A PUBLICATION MEANS. suzatary-core, the repository every deployment
# runs (the owner's box included), is the remote `core` of this checkout, and
# its branch `master` is a TABLE of one function, the public tree of a landed
# commit (scripts/public-tree.sh <out> <rev>), along master's first-parent
# chain: each of its commits is the public tree of the source commit its
# `Source:` trailer names, on the commit before it. A commit is made by
# nothing but its source and its parent (a fixed author, the source's date),
# so publishing a source twice is one commit, and a source already in the
# table is not published again. The table only climbs: a source that does not
# descend from the newest one published is refused, so core's history is
# never rewritten and never runs backwards. The tree is published only when
# its leak guard passes whole (`--check --strict`, the data repository's
# lists included); otherwise nothing is. No landed commit is skipped unless
# the operator says so: a publish walks every first-parent commit after the
# newest source published, oldest first, so one a land could not publish is
# published by the next. A landed commit whose tree can never pass (a term
# added to the lists later matches it) blocks every row after it until
# `publish --skip <sha>` passes over it, recorded as a `Skipped: <sha>`
# trailer on the next row, which scripts/deploy.sh refuses to deploy.
#
# A NEW PATH IS HELD. Publication cannot be taken back, and the leak guard
# reads only what its rules know to look for, so a path that no earlier
# publication held (a new file, a renamed one) is published only once a person
# has looked at it: `publish --admit <path>`, at a terminal and never from a
# script, names it, and the row that first holds it says so in an `Admitted:`
# trailer. So is every admission no earlier publication held: a line of the
# admitted files (scripts/public-admitted.sha256, a file the guard's matcher
# cannot read, by its path and digest), a fingerprint of `.gitleaksignore`, and
# any change to the secret rules (scripts/secret-rules.toml). The guard takes
# each on its word, and the word is a person's only once `--admit` names it. A land, which runs unattended, publishes nothing holding a new path
# and says which; the first publication of an empty table is the baseline the
# operator chose and holds nothing back. A path once published is known for
# good: the table only climbs.
#
# WHAT A TRAIN MEANS. The PRs are a SET, and the order they merge in is a
# function of their head commits, never of their numbers or of the order they
# were named in, so one set of PRs over one master builds one tree. A PR whose
# head contains more of the other listed heads merges first: a stacked PR,
# branched from its base PR's branch, merges before its base, its merge brings
# the base's commits, and the base is then `in`, its `Completes:` todos carried
# on the stacked PR's merge (each once) beside a `Carries:` trailer naming it.
# PRs that hold the same number of the others merge in the order of their head
# commits' names. Each PR is built from its current head: a `refs/pull/<n>/head`
# that is not the head GitHub names for the PR refuses the build. A line a PR
# wrote under the changelog's Unreleased section stays there: where its merge
# put it under a released section (a release landed after the PR branched), it
# is moved back, and the build's table says so.
#
# WHAT A RELEASE MEANS. A tree that declares a release plan (app/core/releases.toml) is
# released by every train, and its version is a reading of the project chain
# (`suzatary_core::release`): MAJOR.MINOR the last completed release's, as the plan declares
# it, and PATCH the trains since that MAJOR.MINOR was first tagged; a release's items move
# no version. A PR names the todos it completes on `Completes:` lines of its description;
# build keeps them as its merge's `Completes:` trailer and ends the train with the release
# `suzatary release cut` writes (app/release.toml, the version and the ids of what it
# completed, and the changelog's Unreleased section closed under it with those todos first),
# read as the chain will be once they are done, so the gate judges the release's own tree.
# The patch counts on from origin/master's newest release (its `Release:` trailer, the one
# reader of it); a version the tree declares that no release names folds into the release.
# Land asks `suzatary release check` whether the chain with those todos done reads at least
# that version, pushes master and the tag `v<version>` in one atomic push, and only then
# records each done as `assistant`, its note naming the PR (`record`, safe to run again): the
# tag, the changelog and the chain say one thing, and a refused push writes nothing. The
# binary is SUZATARY_TRAIN_BIN, else scripts/app-bin.sh's build of a named commit
# (resolve_app), and the chain is SUZATARY_INSTANCE's.
#
# WHAT A GATE MEANS. The gate judges a TREE (the gate script itself is in
# it), in two halves: `tree`, a function of the tree alone, and `live`, the
# tree read against the owner's data tree. Its verdicts are a table of a
# function of (half, tree): kept in .git/train/gates/, keyed by the gate, the
# half and the tree, appended to and never rewritten. Two commits with one
# tree are one gate, and a tree lands when both halves passed it. A key
# holding both verdicts is a FINDING, not a choice: the half read something
# beyond its inputs (a flake), and nothing lands over it.
#
# A GATE READS ONLY THE TREE IT NAMES. The commit is read once, when the gate
# starts, and each half here runs in a worktree of its own at that commit
# ($state/worktree, removed when the half ends), with that tree's own
# scripts/merge-gate.sh, so nothing done to this checkout meanwhile (a branch
# switched, a file edited) reaches it, and its verdict is recorded under that
# commit's tree. Cargo's target is moved into the worktree and back out, and
# the worktree's path never changes, so a gate builds as warm as it did here.
#
# A GATE RUNS IN THE `.#triage` SHELL ALONE, and only when no variable the
# gate's steps read was inherited from the caller (SUZATARY_*, CARGO*, RUST*,
# ...) beyond those the shell sets (SUZATARY_SHELL_SETS names them), the data
# tree (SUZATARY_INSTANCE, else this checkout's instance.toml) and the
# train's own (train_reads), which no gate is handed.
#
# A FULL GATE IS KEPT APART. The gate runs only the steps and units whose inputs moved; `gate
# --full` runs it with `--full` (every step and unit, no kept pass read), and its verdicts are a
# table of their own (<half>-full). A selective pass never stands for a full one: the gate runs
# full by itself on the first train gated when no tree that passed a full gate has landed in the
# last day. A tree both a full and a selective gate judged reads as one table: a full red beside
# a selective green is a FINDING, a change the selection missed.
#
# GITHUB'S RUNNERS. `gate --remote github` (and `bisect --remote github`) runs the tree half in
# core, a public repository, whose runners cost nothing. Only the train commit's PUBLIC TREE goes
# there, a function of its tree, and only once its leak guard passes whole (`--check --strict`,
# as a publication's); a tree that does not pass sends nothing, and the half says nothing of it.
# The tree half's steps that read this private repository run on the box first (`box_steps`:
# fmt, whose workspace names test targets the public tree leaves out, the public tree's own build
# and the deploy's shape, which build the public tree from the source, and the changelog, which
# reads origin's master); a red there is the half's red, and nothing goes
# to the cloud. Then the public tree is committed on core's master by the publications' fixed
# author, with a `Gate: <train sha>` trailer, and pushed to core's branch `gate/<train sha>` alone,
# never to master, where .github/workflows/gate.yml runs the rest of the half's steps, one job per
# lane, each reading the checkout of that one commit. The run's verdict is the half's: success
# green; red where a job's `red` step failed, which it does only when its gate exited 1; and
# anything else (the run cancelled or timed out, a runner lost, a job that never reached its
# gate, a gate that did not start) no verdict, as a lost Codespace is, so only a gate's own
# red makes the tree red. The branch is deleted when the half ends, and by the exit trap of a killed train.
#
# SUZATARY_TRAIN_GATE names another gate script, for the test's stub. Its
# verdicts are keyed by its own bytes, apart from the real gate's, so a
# stub's green never lands a train the real gate has not passed.
# SUZATARY_TRAIN_GH names another `gh`, for the test's stub Codespace and GitHub, and
# SUZATARY_TRAIN_POLL the seconds between reads of its log or of a run (60).
# SUZATARY_TRAIN_PUBLIC names another public-tree script, for the test's stub.
# SUZATARY_TRAIN_BIN names the app the train reads and writes the chain with.
set -euo pipefail

die() {
  printf 'train: %b\n' "$*" >&2
  exit 1
}
say() { printf 'train: %s\n' "$*" >&2; }
usage() {
  echo "usage: scripts/train.sh build <pr>... | gate [--full] [--remote <codespace> | --remote github] | bisect [--remote <codespace> | --remote github] | land | record [<rev>] | publish [--skip <sha>]... [--admit <path>]... [<rev>]" >&2
  exit 64
}

gh="${SUZATARY_TRAIN_GH:-gh}"
root="$(git rev-parse --show-toplevel)" || die "not inside a git checkout."
cd "$root"
# Beside the objects, never in the tree: what the train has learned is about
# this checkout's history and must not become part of it.
state="$(git rev-parse --path-format=absolute --git-common-dir)/train"
mkdir -p "$state"
# ONE TRAIN AT A TIME. Two runs share this checkout, its verdict table and the Codespace: on
# 2026-10-04 an overlapping run tested a half-edited script, recorded two verdicts for one tree,
# and stopped the Codespace under the other run's tree half.
exec 9>"$state/lock"
flock -n 9 || die "another scripts/train.sh is running in this checkout; wait for it to finish."

# What leaving undoes: the branch to return to, the tree half running beside
# the live one, the Codespace's ref, keep-alive and machine (remote_close), and
# a gate's worktree (worktree_close).
return_to="" codespace="" ref="" keeper="" there="" cloud="" gate_branch=""
finish() {
  [[ -z "$there" ]] || { kill "$there" 2>/dev/null; wait "$there" 2>/dev/null; } || true
  [[ -z "$ref" ]] || remote_close
  [[ -z "$gate_branch" ]] || cloud_close
  worktree_close
  [[ -z "$return_to" ]] || git checkout --quiet "$return_to"
}
trap finish EXIT

clean() {
  [[ -z "$(git status --porcelain)" ]] \
    || die "the checkout has changes; the train builds and gates what is committed, so commit or stash them first:\n$(git status --short)"
}

# --- build ----------------------------------------------------------------

# The CHANGELOG's Unreleased section grows by entries, so two PRs that both
# only ADDED lines join by keeping both: the union, ours then theirs. A side
# that removed or rewrote a line is a real conflict and is left to its author.
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

# Whether <new> is <old> with lines added and none removed.
only_adds() {
  local stat
  stat="$(git diff --no-index --numstat -- "$1" "$2")" || true
  [[ -z "$stat" || "$(cut -f2 <<<"$stat")" == 0 ]]
}

# The train's lock holds the versions its merges were tested with, so it is
# kept, and cargo adds only what the PR's manifests need beside it.
relock() {
  git checkout --ours -- app/Cargo.lock \
    && cargo metadata --manifest-path app/Cargo.toml --format-version 1 >/dev/null \
    && git add -- app/Cargo.lock
}

# The lines a PR wrote under the changelog's Unreleased section belong there on the train too.
# When a release landed after the PR branched, git places each line by its context, and a line
# written below an entry the release has since moved lands under the released section. So the
# lines HEAD's merge brought (its diff from its first parent) that sit under a released section,
# and that the PR's head <head> holds under Unreleased, are moved to the end of Unreleased and the
# merge is amended. Prints how many moved; a changelog with no Unreleased section moves nothing.
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

# The todos PR <n>'s description names on its `Completes:` lines, one per line, words split on
# spaces and commas (never globbed) and a carriage return dropped; a word that is not a todo id
# fails with it as the last line.
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

# The binary a release is read, cut, checked and recorded with, set in `app_bin`: SUZATARY_TRAIN_BIN,
# else scripts/app-bin.sh's build of the NAMED COMMIT <rev> (origin/master for build, gate,
# bisect, record and publish; the train for land), never the checkout as it happens to be,
# since this binary writes the chain. Each commit is built once per command.
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

# One final paragraph of trailers, `key: value` per line for each pair given whose value is
# not empty: git reads trailers only from the last paragraph, so a joined merge's
# `Joined:`/`Mentions:` and its `Completes:` must share it.
trailers() {
  local out=() key value
  while (($#)); do
    key="$1" value="$2"
    shift 2
    [[ -z "$value" ]] || out+=("$key: $value")
  done
  ((${#out[@]} == 0)) || printf '%s\n' "${out[@]}"
}

# A commit's `<key>` trailer values, space-separated.
trailer_of() { git log -1 --format="%(trailers:key=$2,valueonly,separator= )" "$1" | sed '/^$/d'; }

# The todos the train's merges complete, each once, in merge order.
train_completes() {
  local commit
  for commit in $(git rev-list --first-parent --reverse "origin/master..HEAD"); do
    [[ -z "$(pr_of "$commit")" ]] || trailer_of "$commit" Completes | tr ' ' '\n'
  done | sed '/^$/d' | awk '!seen[$0]++'
}

# The version of origin/master's newest release, read off its `Release:` trailer: the one
# reader of the last release, so the patch, the record and the tag cannot disagree; nothing
# before the first.
last_release() {
  local rev
  rev="$(newest_release origin/master)"
  [[ -z "$rev" ]] || trailer_of "$rev" Release
}

# The newest release commit of <rev>'s first-parent chain, nothing where there is none.
newest_release() {
  # awk reads to the end: one that left early would SIGPIPE git log, which pipefail and
  # errexit turn into a silent exit 141.
  git log --first-parent --format='%H %(trailers:key=Release,valueonly,separator=)' "$1" \
    | awk 'NF > 1 && !found { print $1; found = 1 }'
}

# Every command first reads origin, and makes origin/master's newest release true on the
# chain: a land whose record failed is repaired by whatever runs next, and never blocks it.
# A release recorded once is remembered in the state, so no build of the app is asked for again.
sync() {
  fetch_origin
  record_origin
}

fetch_origin() {
  git fetch --quiet --tags origin master || die "origin could not be fetched."
}

# origin/master's newest release, recorded with origin/master's own binary.
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

# The train's last commit: the release the project chain reads once the train's todos are
# done. A tree with no release plan is released by no train.
cut_release() {
  [[ -f app/core/releases.toml ]] || return 0
  local todos=() args=() todo out version since last
  mapfile -t todos < <(train_completes)
  for todo in "${todos[@]}"; do args+=(--completes "$todo"); done
  last="$(last_release)"
  [[ -z "$last" ]] || args+=(--last "$last")
  # A PR branched before master's last release wrote its entries under the Unreleased that
  # release closed; the cut moves them back, reading master's changelog to tell them apart.
  since="$(mktemp)"
  git show origin/master:CHANGELOG.md >"$since" 2>/dev/null || : >"$since"
  resolve_app origin/master
  out="$("$app_bin" release cut --tree "$root" --since "$since" "${args[@]}")" \
    || { rm -f "$since"; die "the release could not be cut (above), so the train is not built; nothing is gated until it is."; }
  rm -f "$since"
  version="$(sed -n 's/.*"version":"\([^"]*\)".*/\1/p' <<<"$out")"
  [[ -n "$version" ]] || die "suzatary release cut printed no version: $out"
  # A tag origin already holds would be refused only at land, after the whole gate: say so now.
  ! git ls-remote --exit-code --tags origin "refs/tags/v$version" >/dev/null \
    || die "the chain reads $version, and origin has v$version already (a tag no release on master names): the train is not built. Move or delete that tag, or complete a todo so the chain reads past it."
  # No crate compiles either file (guards' a_release_cut_changes_no_file_a_crate_compiles), so
  # the release moves no gate key; the binary reads the release from the flake's stamp.
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
  # What each PR that may ride is: its head, title, URL and the todos it completes; and, once
  # it is in, the PR whose merge carried it.
  local ready=()
  local -A head_of=() title_of=() url_of=() todos_of=() carrier_of=()
  # Each PR's `Completes:` todos are asked of the chain by master's binary, resolved once,
  # where master declares a release plan.
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
    # GitHub moves refs/pull/<n>/head after a push, not with it: a train of the ref it has not
    # moved yet would gate a commit that is no longer the PR.
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
  # THE ORDER IS A FUNCTION OF THE HEADS. A PR whose head contains more of the other listed heads
  # merges first, so a stacked PR merges before the PRs it carries, which are then in; among PRs
  # holding as many, the head commit's name decides. Only two PRs with one head are ordered by
  # number, and their merges are of one commit, so the number names the merge and moves no tree.
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
    # The listed PRs this merge brings with it: their todos ride on its trailers, each once.
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
  # A train that is master carries nothing (every PR skipped, or on master already), and
  # gating it would say "may land" of nothing.
  [[ "$(git rev-parse HEAD)" != "$(git rev-parse origin/master)" ]] \
    || die "no PR merged ($(printf '#%s ' "${prs[@]}")skipped or already on master), so the train is master and there is nothing to gate."
  cut_release
}

# --- gate -----------------------------------------------------------------

# The halves of the gate (scripts/merge-gate.sh): `tree` reads the tree alone,
# `live` reads it against the owner's data tree, so a tree lands when both
# passed it.
halves=(tree live)
# Whether this run's gates are full (`gate --full`, or a day without a fully passed tree landing).
full=0
half_flag() { case "$1" in tree) echo --no-live ;; live) echo --only-live ;; esac; }
gate_flags() { ((!full)) || printf -- '--full '; half_flag "$1"; }

# The file holding <half>'s verdicts on <commit>'s tree, of full gates with <mode> `full`.
verdicts() {
  local gate=merge-gate
  [[ -z "${SUZATARY_TRAIN_GATE:-}" ]] || gate="other-$(git hash-object "$SUZATARY_TRAIN_GATE")"
  echo "$state/gates/$gate/$1${3:+-$3}/$(git rev-parse "$2^{tree}")"
}

# ok, failed, unknown or finding: what the table knows of <half> on <commit>'s
# tree, from every gate, or with <mode> `full` from full gates alone. The
# verdicts seen form a set, so the reading is its join: nothing seen, one
# verdict, or both (a finding).
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

# The halves' verdicts on <commit> as one: the greatest in the chain
# ok < unknown < failed < finding, so ok means every half passed.
verdict_all() {
  local h v all=ok
  for h in "${halves[@]}"; do
    v="$(verdict "$h" "$1")"
    (($(rank "$v") <= $(rank "$all"))) || all="$v"
  done
  echo "$all"
}
rank() { case "$1" in ok) echo 0 ;; unknown) echo 1 ;; failed) echo 2 ;; finding) echo 3 ;; esac; }

# Append <half>'s verdict on <commit> from the gate's exit <code>: 0 ok, 75 no
# verdict (it did not start), anything else failed.
record() {
  local half="$1" commit="$2" code="$3" file
  ((code == 75)) && { say "the $half half did not start, so it says nothing of the tree"; return 2; }
  file="$(verdicts "$half" "$commit" "$( ((!full)) || echo full)")"
  mkdir -p "$(dirname "$file")"
  printf '%s %s %s\n' "$( ((code)) && echo failed || echo ok)" "$commit" "$(date -u +%FT%TZ)" >>"$file"
  return $((code ? 1 : 0))
}

# The gate's log for <half> on <commit>'s tree.
log_of() { mkdir -p "$state/logs" && echo "$state/logs/$1-$(git rev-parse "$2^{tree}").log"; }

# The worktree a half runs in here, and cargo's target, kept beside it between gates.
worktree="$state/worktree"
target="$state/target"

# A worktree of its own at <commit>, holding the target the last gate built: the path never
# changes, so cargo's fingerprints, which name it, still hold.
worktree_open() {
  worktree_close
  git worktree add --quiet --detach "$worktree" "$1" >/dev/null 2>&1 || return 1
  [[ ! -d "$target" ]] || { mkdir -p "$worktree/app" && mv "$target" "$worktree/app/target"; }
}

# The worktree removed, its target kept for the next. Nothing where there is none, so it is
# safe from the exit trap and after a train killed outright left one behind.
worktree_close() {
  [[ -e "$worktree" ]] || { git worktree prune; return 0; }
  [[ ! -d "$worktree/app/target" || -e "$target" ]] || mv "$worktree/app/target" "$target"
  git worktree remove --force "$worktree" >/dev/null 2>&1 || rm -rf "$worktree"
  git worktree prune
}

# Gate <half> of <commit> and record the verdict: 0 green, 1 red, 2 no verdict.
# The tree half runs in the Codespace when there is one; a half here runs in a
# worktree at <commit>, with that tree's own gate script, and never reads this
# checkout. What the train reads for itself (train_reads) is not handed on.
run_half() {
  local half="$1" commit="$2" code=0
  if [[ "$half" == tree && -n "$codespace" ]]; then
    run_remote "$half" "$commit"
    return
  fi
  if [[ "$half" == tree && -n "$cloud" ]]; then
    cloud_half "$commit"
    return
  fi
  run_here "$half" "$commit" || code=$?
  record "$half" "$commit" "$code"
}

# <half> of <commit> run here, in a worktree at <commit> with that tree's own gate script, its
# output kept in the half's log; any further arguments are the gate's (`--steps ...`). Exits as
# the gate did, or 75, the gate's own "did not start", when no worktree could be made.
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

# --- the environment ----------------------------------------------------------

# What the train reads for itself: its test seams, and the keys its app signs a release's record
# with. A gate is handed none of them.
train_reads=(SUZATARY_TRAIN_BIN SUZATARY_TRAIN_GATE SUZATARY_TRAIN_GH SUZATARY_TRAIN_POLL
  SUZATARY_TRAIN_PUBLIC SUZATARY_BOX_KEY SUZATARY_AGENT_KEY SUZATARY_DEDUP_KEY)
# The variables the gate's steps read (merge-gate.sh's `environment`) that a caller sets to
# change a build or a test: an inherited one makes the verdict about something beside the tree.
gate_reads='^(SUZATARY_|CARGO|RUST|PROPTEST_|TYPST|NODE_OPTIONS$|LD_PRELOAD$|LD_LIBRARY_PATH$|LIBRARY_PATH$)'

# A gate's environment, refused unless it is the `.#triage` shell's: inside that shell, and with no
# variable of `gate_reads` beyond those it sets (SUZATARY_SHELL_SETS), the data tree's
# SUZATARY_INSTANCE, SUZATARY_GATE_SERIAL (which orders the steps and never changes what one
# tests) and the train's own. The data tree is named here, absolutely, since the
# worktree a half runs in holds no link to it.
gate_environment() {
  [[ -n "${IN_NIX_SHELL:-}" && "${SUZATARY_SHELL:-}" == triage ]] \
    || die "a gate runs inside the .#triage shell, whose tools are what it tests with: nix develop .#triage -c scripts/train.sh $*"
  local entry name found=()
  local allowed=" SUZATARY_SHELL SUZATARY_SHELL_SETS SUZATARY_INSTANCE SUZATARY_GATE_SERIAL ${train_reads[*]} ${SUZATARY_SHELL_SETS:-} "
  # The exported names, read from env(1): the shell's bash has no `compgen`, which it builds
  # without programmable completion.
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

# --- the Codespace ----------------------------------------------------------

# `--remote <codespace>`, `--remote github`, or nothing.
remote_opt() {
  case "$#/${1:-}/${2:-}" in
    0//) ;;
    2/--remote/github) cloud=github ;;
    2/--remote/?*) codespace="$2" ;;
    *) usage ;;
  esac
}

# The Codespace gets each commit it gates as a git bundle sent down the ssh pipe, so it needs no
# credentials (a Codespace's ssh session has no token for origin, and one with a token could push).
# It is kept awake from here to remote_close.
remote_open() {
  [[ -n "$codespace" && -z "$ref" ]] || return 0
  ref="bundle"
  say "the tree half runs in $codespace, each commit sent as a bundle (gh codespace ssh starts it if it is stopped)"
  # A Codespace stops itself once idle, the backstop for a train that could not stop it, and its
  # idle timeout is fixed when it is made: scripts/gate-codespace.sh makes one of 10 minutes.
  local idle
  idle="$("$gh" codespace view -c "$codespace" --json idleTimeoutMinutes -q .idleTimeoutMinutes 2>/dev/null)" || idle=""
  if [[ "$idle" =~ ^[0-9]+$ ]] && ((idle > 10)); then
    say "$codespace stops itself after $idle idle minutes, not 10: scripts/gate-codespace.sh makes one that does"
  fi
  keep_awake </dev/null >/dev/null 2>&1 9>&- &
  keeper=$!
}

# One interactive session in the Codespace, a pty printing the date once a minute, held open until
# this process is stopped and opened again a poll after it drops. It holds none of the train's
# files (its output, the lock), and a session that cannot connect costs a retry, never the gate.
#
# IT NEVER OUTLIVES THE TRAIN. A train killed with no chance to run its trap (SIGKILL, a dropped
# terminal's process group) would leave this loop reopening sessions, and the Codespace up and
# billed, for good. So it looks for the train every few seconds and ends with it, stopping the
# Codespace as the train's trap would have (remote_close ends it with a TERM, which stops
# nothing), and each session ends itself after an hour, so even an orphaned `gh` lets the
# Codespace idle out.
keep_awake() {
  local session kept="" train=$$
  session="$(printf '%q codespace ssh -c %q -- -t %q' "$gh" "$codespace" 'for minute in $(seq 60); do date; sleep 60; done')"
  # Each child is waited on in the background, so a stop is heard at once and ends it.
  trap 'kill "$kept" 2>/dev/null; wait "$kept" 2>/dev/null; exit 0' TERM
  while kill -0 "$train" 2>/dev/null; do
    script -qec "$session" /dev/null </dev/null >/dev/null 2>&1 &
    kept=$!
    while kill -0 "$kept" 2>/dev/null; do
      kill -0 "$train" 2>/dev/null || { kill "$kept" 2>/dev/null; break 2; }
      sleep 5 &
      wait $! || true
    done
    wait "$kept" || true
    sleep "${SUZATARY_TRAIN_POLL:-60}" &
    wait $! || true
  done
  "$gh" codespace stop -c "$codespace" >/dev/null 2>&1 || true
}

# End the keep-alive and stop the Codespace, so it spends no core-hours waiting.
remote_close() {
  ref=""
  { kill "$keeper" && wait "$keeper"; } 2>/dev/null || true
  keeper=""
  "$gh" codespace stop -c "$codespace" >/dev/null || say "could not stop $codespace: stop it by hand (gh codespace stop -c $codespace)"
}

# A bundle of `commit` over what the Codespace's checkout already holds (all of it, if the box does
# not know that commit; nothing, if it holds the commit already), written to a temporary file the
# caller removes.
remote_bundle() {
  local commit="$1" out="$2" base
  base="$("$gh" codespace ssh -c "$codespace" -- 'git -C "${CODESPACE_VSCODE_FOLDER:-/workspaces/Suzatary}" rev-parse HEAD' 2>/dev/null | tail -1)" || base=""
  [[ "$base" != "$commit" ]] || return 0
  git branch --quiet --force train-bundle "$commit"
  if [[ "$base" =~ ^[0-9a-f]{40}$ ]] && git merge-base --is-ancestor "$base" "$commit" 2>/dev/null; then
    git bundle create --quiet "$out" "$base..train-bundle"
  else
    git bundle create --quiet "$out" train-bundle
  fi
  git branch --quiet -D train-bundle
}

# The tree half of <commit> in the Codespace, its whole output
# kept in the log. The gate runs DETACHED there and the box polls its log once
# a minute, so a dropped connection costs one poll, not the run (ssh dropped
# mid-gate twice on 2026-10-04). Its exit code comes back as a line of the log;
# a gate whose process is gone without one says nothing of the tree.
#
# The Codespace stays up through it because keep_awake holds an interactive
# session open beside it. A Codespace stops itself after its idle timeout (30
# minutes on one made before the account's default rose to 240), and only an
# interactive session counts as activity: on 2026-10-06 a cold tree half ran
# past 30 minutes and the Codespace stopped mid-gate, its lastUsedAt never
# moved by the polls' short `gh codespace ssh -- <command>` calls, while a
# session with a pty printing once a minute held it up through a 50-minute gate.
run_remote() {
  local half="$1" commit="$2" log code gate bundle chunk errs inner misses=0
  log="$(log_of "$half" "$commit")"
  gate="nix develop .#triage -c scripts/merge-gate.sh"
  [[ -z "${SUZATARY_TRAIN_GATE:-}" ]] || gate="$(printf %q "$SUZATARY_TRAIN_GATE")"
  inner="$gate $(gate_flags "$half"); code=\$?; echo train-gate-exit: \$code"
  bundle="$(mktemp)" chunk="$(mktemp)" errs="$(mktemp)"
  remote_bundle "$commit" "$bundle"
  : >"$log"
  "$gh" codespace ssh -c "$codespace" -- "set -e
    export PATH=/nix/var/nix/profiles/default/bin:\$PATH
    cd \"\${CODESPACE_VSCODE_FOLDER:-/workspaces/Suzatary}\"
    cat > .git/train.bundle
    [ ! -s .git/train.bundle ] || git fetch --quiet .git/train.bundle train-bundle
    git checkout --quiet --force --detach $commit
    git clean --quiet -fd
    setsid nohup bash -c $(printf %q "$inner") >.git/train-gate.log 2>&1 </dev/null &
    echo \$! >.git/train-gate.pid" <"$bundle" >"$errs" 2>&1 \
    || { cat "$errs" >>"$log"; misses=10; }
  rm -f "$bundle"
  # Liveness is read BEFORE the log, so a gate that ends between the two has its last line read.
  # The polls end with the train (this may be its background half): a poll's ssh starts a
  # stopped Codespace, so an orphan's would wake one the idle timeout had let go.
  while ((misses < 10)) && kill -0 $$ 2>/dev/null && ! grep -q '^train-gate-exit:' "$log"; do
    sleep "${SUZATARY_TRAIN_POLL:-60}"
    if "$gh" codespace ssh -c "$codespace" -- "cd \"\${CODESPACE_VSCODE_FOLDER:-/workspaces/Suzatary}\"
      kill -0 \$(cat .git/train-gate.pid) 2>/dev/null && alive=1 || alive=0
      tail -c +$(($(stat -c %s "$log") + 1)) .git/train-gate.log
      [ \$alive = 1 ] || echo train-gate-gone >&2" >"$chunk" 2>"$errs"; then
      cat "$chunk" >>"$log"
      misses=0
      if grep -q train-gate-gone "$errs" && ! grep -q '^train-gate-exit:' "$log"; then misses=10; fi
    else
      misses=$((misses + 1))
    fi
  done
  rm -f "$chunk" "$errs"
  code="$(sed -n 's/^train-gate-exit: \([0-9]*\)$/\1/p' "$log" | tail -1)"
  if [[ -z "$code" ]]; then
    say "the $half half in $codespace did not finish, so it says nothing of the tree; its last lines ($log):"
    tail -n 20 "$log" >&2
    return 2
  fi
  record "$half" "$commit" "$code"
}

# --- GitHub's runners --------------------------------------------------------

# The tree half's steps that read this private repository, run on the box before anything goes to
# the cloud: public-tree builds the public tree from the source (scripts/public-tree.sh --store),
# deployed-shape runs the binary it built, and changelog compares with origin's master. Every
# other step of the half is a job's in .github/workflows/gate.yml (a guard test holds the two to
# every step of the half, each once).
box_steps="fmt,public-tree,deployed-shape,changelog"
# The workflow a gate branch runs, and how long the train waits for its run: a run that has not
# ended in six hours (GitHub's own limit on a job) says nothing of the tree.
gate_workflow=.github/workflows/gate.yml
cloud_hours=6

# core's owner/name on GitHub, read off the remote's configured URL.
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
  gate_branch="gate/$commit"
  cloud_wait "$commit" "$pushed" || code=$?
  cloud_close
  return "$code"
}

# The box's part of <commit>'s tree half, then the public tree pushed to core: prints the gate
# commit and succeeds once it is pushed; otherwise 1 when the box's steps are red (recorded) and
# 2 when there is no verdict (a leak, a gate that did not start, core out of reach), recording
# nothing and pushing nothing.
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
  run_here tree "$commit" --steps "$box_steps" || code=$?
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
  # Named by the instant it is made, so each gate pushes a commit of its own and its run is the
  # only one of that sha.
  pushed="$(public_commit "$tree" "$parent" "$(date -u +%FT%TZ)" "Gate ${commit:0:12}, $(date -u +%FT%T.%NZ)" "Gate: $commit")" \
    && [[ "$pushed" =~ ^[0-9a-f]{40}$ ]] \
    || { say "the public tree of ${commit:0:12} could not be committed"; return 2; }
  git push --quiet core "+$pushed:refs/heads/gate/$commit" \
    || { say "core refused gate/$commit, so the tree half says nothing of ${commit:0:12}"; return 2; }
  say "the tree half of ${commit:0:12} runs on GitHub's runners: $repo, branch gate/$commit, commit ${pushed:0:12}"
  echo "$pushed"
}

# Wait for the gate workflow's run on <pushed>, the gate commit of <commit>, and record its
# verdict: success green, a job's `red` step failed red, anything else none. Each job's log is
# kept in the half's log; a red job's ends with the gate's failing steps.
cloud_wait() {
  local commit="$1" pushed="$2" repo log run="" status="" misses=0 red job name deadline
  repo="$(core_repo)"
  log="$(log_of tree "$commit")"
  deadline=$((SECONDS + cloud_hours * 3600))
  while [[ -z "$run" ]] && ((misses < 10 && SECONDS < deadline)); do
    sleep "${SUZATARY_TRAIN_POLL:-60}"
    run="$("$gh" api "repos/$repo/actions/runs?head_sha=$pushed&event=push" \
      --jq ".workflow_runs[] | select(.path == \"$gate_workflow\") | .id" | awk 'NR == 1')" || run=""
    misses=$((misses + 1))
  done
  if [[ -z "$run" ]]; then
    say "no run of $gate_workflow started on $repo for ${pushed:0:12}, so the tree half says nothing of ${commit:0:12} (are Actions enabled? scripts/core-settings.sh)"
    return 2
  fi
  say "waiting on https://github.com/$repo/actions/runs/$run"
  misses=0
  while [[ "$status" != completed\ * ]] && ((misses < 10 && SECONDS < deadline)); do
    sleep "${SUZATARY_TRAIN_POLL:-60}"
    if status="$("$gh" api "repos/$repo/actions/runs/$run" --jq '.status + " " + (.conclusion // "")')"; then
      misses=0
    else
      misses=$((misses + 1)) status=""
    fi
  done
  if [[ "$status" != completed\ * ]]; then
    say "the run $run on $repo did not end (${status:-unreachable}), so the tree half says nothing of ${commit:0:12}"
    return 2
  fi
  if [[ "$status" == "completed success" ]]; then
    echo "run $run: success" >>"$log"
    record tree "$commit" 0
    return
  fi
  # Each job whose `red` step failed: its gate exited 1, a finding about the tree.
  red="$("$gh" api "repos/$repo/actions/runs/$run/jobs?per_page=100" \
    --jq '.jobs[] | select(any((.steps // [])[]; .name == "red" and .conclusion == "failure")) | "\(.id) \(.name)"')" || {
    say "the jobs of run $run could not be read, so the tree half says nothing of ${commit:0:12}"
    return 2
  }
  if [[ -z "$red" ]]; then
    say "the run $run ended ${status#completed }, and no job's gate found the tree red (a runner lost, a cancel, a gate that did not start), so the tree half says nothing of ${commit:0:12}"
    return 2
  fi
  # A job's log stamps each line with its time, which comes off, so the gate's own sections read
  # as they do here.
  while read -r job name; do
    say "red: $name"
    { echo "--- job $name"
      "$gh" api "repos/$repo/actions/jobs/$job/logs" | sed -E 's/^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9:.]+Z //' ||
        echo "(its log could not be read)"; } >>"$log"
  done <<<"$red"
  record tree "$commit" 1
}

# Delete the gate branch. Nothing waits on it any more, and a failure leaves only a branch.
cloud_close() {
  git push --quiet core --delete "refs/heads/$gate_branch" 2>/dev/null \
    || say "could not delete core's $gate_branch: delete it by hand (git push core --delete $gate_branch)"
  gate_branch=""
}

on_train() {
  git rev-parse --verify --quiet train >/dev/null || die "there is no train; scripts/train.sh build <pr>... first."
  git checkout --quiet train
}

# The train's commit, read once: what a gate or a bisect judges, whatever the checkout does after.
train_commit() {
  git rev-parse --verify --quiet "train^{commit}" || die "there is no train; scripts/train.sh build <pr>... first."
}

# When a tree that passed a full gate in both halves last landed, in seconds since the epoch.
full_at="$state/full-at"

# Both halves, side by side when the tree half is in a Codespace: neither
# reads the other. The Codespace is let go as soon as its half is done, not
# when the live half here is: an idle machine waiting on the box is billed.
gate() {
  gate_environment gate "$@"
  if [[ "${1:-}" == --full ]]; then full=1 && shift; fi
  remote_opt "$@"
  sync
  local commit
  commit="$(train_commit)"
  if ((!full && $(date +%s) - $(cat "$full_at" 2>/dev/null || echo 0) >= 86400)); then
    full=1 && say "no tree that passed a full gate has landed in the last day, so this gate runs full"
  fi
  remote_open
  if [[ -n "$codespace" ]]; then
    { run_half tree "$commit" || true; remote_close; } &
    there=$!
    run_half live "$commit" || true
    wait "$there" || true
    there="" ref="" keeper=""
  elif [[ -n "$cloud" ]]; then
    # The box's steps first, in the worktree the live half then uses; the cloud's run is waited
    # on beside the live half.
    local pushed
    if pushed="$(cloud_push "$commit")"; then
      gate_branch="gate/$commit"
      { cloud_wait "$commit" "$pushed" || true; } &
      there=$!
    fi
    run_half live "$commit" || true
    if [[ -n "$there" ]]; then
      wait "$there" || true
      there=""
      cloud_close
    fi
  else
    run_half tree "$commit" || true
    run_half live "$commit" || true
  fi
  # A red full gate is no full pass, and until a fully passed tree lands every gate is full.
  if ((full)) && [[ "$(verdict tree "$commit" full)/$(verdict live "$commit" full)" != ok/ok ]]; then rm -f "$full_at"; fi
  report "$commit"
}

# Say what the table knows of <commit>, the train gated, half by half; succeeds
# when it may land.
report() {
  local commit="$1" h v short
  short="$(git rev-parse --short "$commit")"
  for h in "${halves[@]}"; do
    v="$(verdict "$h" "$commit")"
    say "$h half: $v"
    [[ "$v" != failed || ! -s "$(log_of "$h" "$commit")" ]] \
      || sed -n "/^==> the failing steps' last lines/,\$p" "$(log_of "$h" "$commit")" >&2
  done
  case "$(verdict_all "$commit")" in
    ok) say "green: the train $short may land (scripts/train.sh land)" ;;
    finding) die "red, where a half once passed this tree: the gate is not a function of the tree. The logs are in $state/logs/." ;;
    unknown) die "a half has no verdict on $short: gate again." ;;
    failed) die "red: scripts/train.sh bisect finds the merge that broke it. The logs are in $state/logs/." ;;
  esac
}

# --- bisect ---------------------------------------------------------------

# The PR a train's merge brought, read off its subject.
pr_of() { git log -1 --format=%s "$1" | sed -n 's/^Merge #\([0-9]*\):.*/\1/p'; }

# Every PR a train's merge brought, one a line: its own, then those it carried (`Carries:`).
prs_of() { pr_of "$1" && trailer_of "$1" Carries | tr ' ' '\n' | sed '/^$/d'; }

# The longest prefix of a red train that <half> passes, for the first half
# red on HEAD. Prefix k is the train after its first k merges; prefix 0 is
# master, green because it landed through a gate. Each probe gates that one
# half and its verdict joins the table, so a prefix gated before is read, not
# run. Like every bisect, the search assumes a red prefix stays red as it
# grows.
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
  # The merges alone: the release commit on top is no PR, and is blamed as itself below.
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
      [[ "$half" != tree ]] || remote_open
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
  [[ "$half" != tree ]] || remote_open
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

# --- land -----------------------------------------------------------------

# The version the release on the checkout's HEAD declares, once the chain with its todos done
# reads at least that version and origin holds no tag of it; nothing for a tree with no release
# plan. Asked before anything is pushed or written: a refusal lands nothing and records nothing.
checked_release() {
  [[ -f app/core/releases.toml ]] || return 0
  local version
  version="$(trailer_of HEAD Release)"
  [[ -n "$version" ]] \
    || die "the train's HEAD is not its release (no Release: trailer), so it has no version to tag: scripts/train.sh build again. Nothing landed."
  # Only origin's tags say what was released; a local one of that name is a leftover of a land
  # origin refused, replaced when this one tags.
  ! git ls-remote --exit-code --tags origin "refs/tags/v$version" >/dev/null \
    || die "origin has v$version already, so this train's release would be a second $version: scripts/train.sh build again, which reads the chain afresh. Nothing landed."
  resolve_app HEAD
  "$app_bin" release check --tree "$root" >/dev/null \
    || die "the project chain, with this train's todos done, reads below $version (above): scripts/train.sh build again. Nothing landed."
  echo "$version"
}

# The release at <rev> made true on the chain: `suzatary release record` over the release's
# own files (extracted, so no checkout moves), each todo noted with the PR that completed it.
# Safe to run again: a todo already done is not written twice. The binary that writes is built
# from <from>, a named commit.
record_release() {
  local rev="$1" from="$2" version tree subject release completes todo out notes=()
  version="$(trailer_of "$rev" Release)"
  [[ -n "$version" ]] || die "${rev:0:12} is not a release (no Release: trailer); nothing to record."
  # The PRs since the release before this one, read in ONE pass of the log (a call per commit
  # cost most of a minute over the history before the first release).
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

# `record [<rev>]`: record_release for <rev> (origin/master's newest release), which must be a
# release of origin/master's first-parent chain: only what landed is recorded.
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
  # A gate judged this tree; a master that moved since is a different tree, however disjoint
  # its files (git merges text, not types), so the train is built again on it and gated
  # again, its kept steps making that mostly hits.
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
  # The landing push is of a detached HEAD, and the train moves to it only once origin has
  # taken it, so a refused push leaves the gated train as it was, to land again. The tag
  # rides the same atomic push: master and v<version> move together or neither does.
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
  [[ "$(verdict tree HEAD full)/$(verdict live HEAD full)" != ok/ok ]] || date +%s >"$full_at"
  # Recorded only now that origin holds the release; a failure names its retry and still lets
  # the publication run, then fails the land.
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

# --- publish ----------------------------------------------------------------

core_branch=master
# The public tree's admitted files (scripts/public-tree.sh): a line of it is a person's word that
# they looked at those bytes, so a publication holding a new one is held as a new path is.
admitted=scripts/public-admitted.sha256
# The secret scanner's rules: a change to them changes what passes, so it is admitted as a line is.
rules=scripts/secret-rules.toml

# The commit of core's master whose Source is <rev>, or nothing: the table's entry for <rev>.
# git reads the whole log, so no closed pipe reads as a failure under pipefail.
published() {
  git rev-parse --verify --quiet "refs/remotes/core/$core_branch" >/dev/null || return 0
  git log --first-parent --format='%H%x09%(trailers:key=Source,valueonly,separator=%x2C)' \
    "refs/remotes/core/$core_branch" | awk -F'\t' -v rev="$1" '$2 == rev && !n++ { print $1 }'
}

# The commit of core's master that names <rev> as skipped, or nothing.
skipped_by() {
  git rev-parse --verify --quiet "refs/remotes/core/$core_branch" >/dev/null || return 0
  git log --first-parent --format='%H%x09%(trailers:key=Skipped,valueonly,separator=%x09)' \
    "refs/remotes/core/$core_branch" | awk -F'\t' -v rev="$1" '{ for (i = 2; i <= NF; i++) if ($i == rev && !n++) print $1 }'
}

# The newest source core's master publishes, or nothing (a master that predates the table).
newest() {
  git rev-parse --verify --quiet "refs/remotes/core/$core_branch" >/dev/null || return 0
  git log --first-parent --format='%(trailers:key=Source,valueonly,separator=%x2C)' \
    "refs/remotes/core/$core_branch" | awk 'NF && !n++'
}

# The rows the table lacks up to <rev> (origin/master's tip by default), oldest first, one core
# commit each: every first-parent commit of master after the newest source published, or <rev>
# alone where core publishes none yet; nothing when core publishes <rev> already. A row is
# published only once its tree's leak guard passes whole, and the first row that fails stops the
# walk, so a later land publishes it and every row after it.
publish() {
  local rev heads last source sources=() skips=() skipped=() at
  # Read by `row`: the new paths a person has looked at.
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
  # An empty <rev> is a mistake upstream of this (a variable that read nothing), never "the tip":
  # publishing the tip in its place would leave the rev meant below the table forever.
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
    # A skip is for a tree that can never pass, so one that passes is refused before any row is
    # pushed: a skipped commit is never deployed, and deploy calls it a leak.
    ! checked "$at" \
      || die "--skip ${at:0:12}: ${at} passes its leak guard; publish it instead. Nothing published."
  done
  # Read by `row`: the paths each row would publish first. An admit names one of them, or it is a
  # word about nothing this publish holds.
  local pending=() fresh paths
  for source in "${sources[@]}"; do
    printf '%s\n' "${skips[@]}" | awk -v r="$source" '$0 == r { f = 1 } END { exit !f }' || pending+=("$source")
  done
  fresh="$(fresh_rows "${pending[@]}")" || die "could not write the public tree to list the paths it would publish. Nothing published."
  # grep reads a here-string, not a pipe: `grep -q` leaves at its first match, and a writer
  # into a pipe longer than its buffer then dies of SIGPIPE, which pipefail reads as no match.
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

# Whether <commit> is on <branch>'s first-parent chain. awk reads to the end: a reader that quit
# at its first match (the box's grep is ugrep, which does so with its output going nowhere) would
# close the pipe, and under pipefail rev-list's broken pipe would read as "not on it".
on_chain() {
  git rev-list --first-parent "$2" | awk -v r="$1" '$0 == r { f = 1 } END { exit !f }'
}

# The leak guard's verdict on the public tree of <source>, written into <dir>/tree when a <dir>
# is given (and removed with the scratch directory otherwise): 0 when it passes whole, 1 when
# the guard ran and found hits, their files and lines only on stderr. A guard that could not
# run (no scratch directory, the tree not written, the check's own 2) is no verdict, and stops
# the publish naming itself, so a skip is never taken on a check that did not run. The one check
# both a row and a skip are judged by.
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

# For each <source>, in order, the paths of its public tree that no row of core's master and no
# source before it holds, and the paths its ADMISSIONS name that none of theirs did: a line of the
# admitted files (a path and its digest), a fingerprint of `.gitleaksignore`, and the secret
# rules, admitted whole by their digest under their own path. Each is a person's word, so a new
# one is held as a new path is. As "<source> <path>" lines; none where core publishes nothing
# yet, since the first publication is the baseline the operator chose. Both are unions over the
# rows, so the order they were published in decides nothing. Each tree is written, never
# checked, and its admissions are read by the binary's own parsers (`suzatary leaks
# --admissions`).
fresh_rows() {
  local public scratch s c bin
  git rev-parse --verify --quiet "refs/remotes/core/$core_branch" >/dev/null || return 0
  public="${SUZATARY_TRAIN_PUBLIC:-$root/scripts/public-tree.sh}"
  bin="${SUZATARY_TRAIN_BIN:-}"
  # A named commit's app, as resolve_app's: origin/master, which a publish follows.
  [ -n "$bin" ] || bin="$(SUZATARY_APP_REV="$(git rev-parse origin/master)" "$root/scripts/app-bin.sh")/bin/suzatary" || return 1
  scratch="$(mktemp -d)" || return 1
  # The admissions of the three files under <dir>, `<path>\t<atom>` a line.
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
  # Every admission a row of core has published: once made, it stands.
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

# <dir>/tree written into this repository's objects as a git tree, its id printed: through an
# index of its own (<dir>/index), so neither the checkout nor its index is touched, and forced,
# since a published file is published even where a `.gitignore` names it.
tree_of() {
  local gitdir
  gitdir="$(git rev-parse --absolute-git-dir)" &&
    (cd "$1/tree" && export GIT_DIR="$gitdir" GIT_WORK_TREE=. GIT_INDEX_FILE="$1/index" &&
      git add --all --force . && git write-tree)
}

# A commit of <tree> on <parent> (none when empty), dated <when>, with the subject <subject> and the
# trailers <trailers>, by the fixed author every commit pushed to core carries: no person's name.
public_commit() {
  GIT_AUTHOR_NAME=Suzatary GIT_AUTHOR_EMAIL=publish@suzatary.invalid GIT_AUTHOR_DATE="$3" \
    GIT_COMMITTER_NAME=Suzatary GIT_COMMITTER_EMAIL=publish@suzatary.invalid GIT_COMMITTER_DATE="$3" \
    git commit-tree "$1" ${2:+-p "$2"} -m "$4" -m "$5"
}

# One row: the public tree of <source>, checked whole, committed on core's master and pushed.
# Every step that fails stops it, naming itself, and nothing is pushed unless it is a commit:
# pushing an empty name would delete core's master.
row() {
  local source="$1" dir tree parent commit when trailers s new held every path line
  # One paragraph, so git reads every line of it as a trailer.
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
    # Every row's held paths, this one's and those after it, so one look admits them all.
    # Each line is "<source> <path>", the path whole: a space or an edge blank in it is kept.
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
  publish) shift && sync && publish "$@" ;;
  *) usage ;;
esac
