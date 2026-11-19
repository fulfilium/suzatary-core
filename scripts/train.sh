#!/usr/bin/env bash
# The merge queue on the box: reviewed PRs ride a train, one integration branch gated once by
# scripts/merge-gate.sh and landed as one fast-forward of master.
#
#   build <pr>...    train := origin/master, each PR merged in, then the release cut on top
#   gate [--full]    both halves of scripts/merge-gate.sh on the train
#   bisect           the merge that turned a red half red
#   land             master and v<version> := the gated train, pushed as one while master has not
#                    moved; then published; then its release recorded on the roadmap
#   record [<rev>]   the release at <rev> recorded: its todos done where each is found, the
#                    roadmap's pin moved and its pictures drawn in one commit, scanned and pushed
#   nightly          both halves, full, on origin/master; a red one files a todo
#   check <pr>       the runners' jobs over <pr>'s head merged on master, posted on the PR; no lock
#   publish [--skip <sha>]... [--admit <path>]... [<rev>]
#                    core's master += the public tree of each landed commit up to <rev>
#
# `--remote github` runs the tree half on core's runners (only a public tree its leak guard passed
# leaves the box); the box keeps the steps that read this repository and the live half. Run inside
# `nix develop .#triage`, from a clean checkout; it never touches the data repository.
#
# Verdicts go to .git/train/gates/, keyed by half and tree, and only the train's tool reads or
# writes them (app/tooling, `train table`); a key holding both is a finding.
#
# Test seams: the SUZATARY_TRAIN_ names of `train_reads`; a stub gate's verdicts are keyed by its
# own bytes, so a stub's green never lands a train.
set -euo pipefail
# Bash cannot trap or reset a signal ignored at its start, and `finish` must run on HUP or TERM.
if ((0x$(awk '/^SigIgn:/ { print $2 }' /proc/$$/status) & 0x4001)); then
  exec env --default-signal=HUP,TERM "${BASH_SOURCE[0]}" "$@"
fi

die() {
  printf 'train: %b\n' "$*" >&2
  exit 1
}
say() { printf 'train: %s\n' "$*" >&2; }
usage() {
  echo "usage: scripts/train.sh build <pr>... | gate [--full] [--remote github] | bisect [--remote github] | land | record [<rev>] | nightly | check <pr> | publish [--skip <sha>]... [--admit <path>]... [<rev>]" >&2
  exit 64
}

gh="${SUZATARY_TRAIN_GH:-gh}"
# Before the lock: a missing flock or git would read as another train or no checkout.
needs() {
  local tool
  for tool; do
    command -v "$tool" >/dev/null \
      || die "$tool is not on PATH; the .#triage shell (flake.nix) carries every tool this script calls. Nothing ran."
  done
}
needs git flock
case "${1:-}" in build | land | check | nightly) needs "$gh" ;; esac
case "${1:-}" in gate | bisect | land | nightly) needs cargo getent ;; esac
scripts="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(git rev-parse --show-toplevel)" || die "not inside a git checkout."
cd "$root"
# In .git, never in the tree.
state="$(git rev-parse --path-format=absolute --git-common-dir)/train"
mkdir -p "$state"
# A coproc holds the lock until this shell and each half's (`start`) close its stdin, a pipe bash
# keeps from what it execs, so no command it runs inherits the lock.
# lock [-n]: the lock taken; with -n, 1 at once while another holds it.
lock_pid="" lock_fd=""
lock() {
  local said=""
  coproc LOCK { trap '' HUP INT TERM; exec 9>>"$state/lock" && flock "$@" 9 && echo held && read -r _; } 2>/dev/null
  lock_pid="$LOCK_PID" lock_fd="${LOCK[1]}"
  read -r said <&"${LOCK[0]}" || true
  [[ "$said" == held ]] && return
  wait "$lock_pid" 2>/dev/null || true
  return 1
}

# unlock: the lock is free when it returns.
unlock() {
  [[ -n "$lock_pid" ]] || return 0
  exec {lock_fd}>&-
  wait "$lock_pid" 2>/dev/null || true
  lock_pid="" lock_fd=""
}

# One train at a time; the nightly waits, a check takes no lock.
if [[ "${1:-}" != check ]]; then
  if [[ "${1:-}" == nightly ]]; then
    lock -n || { say "a train is running here: the nightly gate waits for it"; lock; }
  else
    lock -n || die "another scripts/train.sh is running in this checkout ($(cat "$state/lock.holder" 2>/dev/null)); wait for it to finish."
  fi
  echo "${1:-}, pid $$, since $(date -u +%FT%TZ)" >"$state/lock.holder"
fi

return_to="" cloud="" gate_branch="" gate_pushed=""
# shellcheck source=scripts/groups.sh
. "$scripts/groups.sh"
# Above the gate's, so a gate told to stop has stopped its lanes before the train kills it.
group_grace=10
# Every half is stopped before its worktree goes.
finish() {
  group_stop "${groups[@]}"
  [[ -z "$gate_branch" ]] || cloud_close
  worktree_close
  [[ -z "$return_to" ]] || git checkout --quiet "$return_to"
  unlock
}
trap finish EXIT

clean() {
  [[ -z "$(git status --porcelain)" ]] \
    || die "the checkout has changes; the train builds and gates what is committed, so commit or stash them first:\n$(git status --short)"
}

# Keep the train's lock (the versions its merges were tested with); cargo adds what the PR needs.
relock() {
  git checkout --ours -- app/Cargo.lock \
    && cargo metadata --manifest-path app/Cargo.toml --format-version 1 >/dev/null \
    && git add -- app/Cargo.lock
}

# The todos on PR <n>'s `Completes:` lines, one a line; a word that is no todo id exits 2, last.
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

# PR <n>'s `Wasm-growth:` texts, one a line, which its merge carries as trailers (wasm_budget.rs).
growth_of() {
  local body line
  body="$("$gh" pr view "$1" --json body --jq .body)" || return 1
  while IFS= read -r line; do
    line="${line%$'\r'}"
    [[ "$line" =~ ^[[:space:]]*Wasm-growth:[[:space:]]*(.+)$ ]] || continue
    echo "${BASH_REMATCH[1]}"
  done <<<"$body"
}

# `Wasm-growth: <line>` for each of PR <n>'s lines, ready to follow its other trailers.
grown() { sed '/^$/d; s/^/Wasm-growth: /' <<<"${growth_lines[$1]}"; }

# Sets `app_bin`: SUZATARY_TRAIN_BIN, else the named commit's app from a cache, else the deployed one.
app_bin=""
declare -A app_bins=()
resolve_app() {
  local rev out
  rev="$(git rev-parse --verify --quiet "$1^{commit}")" || die "\"$1\" names no commit to take the app from."
  if [[ -n "${SUZATARY_TRAIN_BIN:-}" ]]; then
    app_bin="$SUZATARY_TRAIN_BIN"
    return 0
  fi
  if [[ -z "${app_bins[$rev]:-}" ]]; then
    if out="$(SUZATARY_APP_REV="$rev" "$root/scripts/app-bin.sh")"; then
      app_bins[$rev]="$out/bin/suzatary"
    else
      say "no cache holds ${rev:0:12}'s app: the deployed one runs in its place"
      app_bins[$rev]="$(deployed_app)/bin/suzatary"
    fi
  fi
  app_bin="${app_bins[$rev]}"
}

# The leak guard's matcher: no tree it reads built it, nor (this script's app-bin.sh) chose it.
deployed_app() {
  "$scripts/app-bin.sh" --deployed || die "there is no deployed app to read the tree with (scripts/deploy.sh <rev> first)."
}

# One paragraph of `key: value` lines, empty values skipped (git reads only the last paragraph).
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
  # awk reads to the end: leaving early would SIGPIPE git log into a silent 141.
  git log --first-parent --format='%H %(trailers:key=Release,valueonly,separator=)' "$1" \
    | awk 'NF > 1 && !found { print $1; found = 1 }'
}

# Records origin/master's newest release on the roadmap first, repairing a land cut short.
sync() {
  fetch_origin
  record_origin
}

fetch_origin() {
  git fetch --quiet --tags origin master || die "origin could not be fetched."
}

# The repair is the land's own last step: what the roadmap holds says what is left to do.
record_origin() {
  local rev
  rev="$(newest_release origin/master)"
  [[ -n "$rev" ]] || return 0
  (record_release "$rev") >&2 \
    || say "the release at ${rev:0:12} is still not recorded on the roadmap; the next command tries again (scripts/train.sh record)"
}

# Sets `roadmap` to the clone of the plan store <rev>'s app finds mounted, empty where the
# deployment mounts none or there is no deployment: then a train cuts no release.
roadmap=""
find_roadmap() {
  roadmap=""
  [[ -n "${SUZATARY_INSTANCE:-}" || -e "$root/instance.toml" ]] || return 0
  resolve_app "$1"
  roadmap="$("$app_bin" release store)" || die "the deployment's plan store could not be found (above)."
  [[ -z "$roadmap" ]] || git -C "$roadmap" fetch --quiet origin master \
    || die "the roadmap's origin could not be fetched, so nothing is read of it."
}

# store_at <commit> <dir>: the roadmap as <commit> holds it, written out at <dir>.
store_at() {
  mkdir -p "$2" && git -C "$roadmap" archive "$1" | tar -x -C "$2" \
    || die "the roadmap at ${1:0:12} could not be written out."
}

# The roadmap commit a cut and a check read: what its origin publishes now.
tasks_head() { git -C "$roadmap" rev-parse --verify origin/master; }

# The train's last commit: the release the roadmap reads once the train's todos are done.
cut_release() {
  find_roadmap origin/master
  [[ -n "$roadmap" ]] || return 0
  local todos=() public=() args=() todo out version last tasks dir place placed=""
  mapfile -t todos < <(train_completes)
  tasks="$(tasks_head)"
  dir="$(mktemp -d)"
  store_at "$tasks" "$dir"
  if ((${#todos[@]})); then
    placed="$("$app_bin" release known --store "$dir" "${todos[@]}")" \
      || { rm -rf "$dir"; die "the train's Completes: todos could not be placed (above)."; }
  fi
  # A todo found only on the upkeep chain is completed at land and counts toward no version.
  while read -r place todo; do
    [[ "$place" != roadmap ]] || public+=("$todo")
  done <<<"$placed"
  for todo in "${public[@]}"; do args+=(--completes "$todo"); done
  last="$(last_release)"
  [[ -z "$last" ]] || args+=(--last "$last")
  out="$("$app_bin" release cut --tree "$root" --store "$dir" "${args[@]}")" \
    || { rm -rf "$dir"; die "the release could not be cut (above), so the train is not built; nothing is gated until it is."; }
  rm -rf "$dir"
  version="$(sed -n 's/.*"version":"\([^"]*\)".*/\1/p' <<<"$out")"
  [[ -n "$version" ]] || die "suzatary release cut printed no version: $out"
  # Else the tag would be refused only at land, after the whole gate.
  ! git ls-remote --exit-code --tags origin "refs/tags/v$version" >/dev/null \
    || die "the chain reads $version, and origin has v$version already (a tag no release on master names): the train is not built. Move or delete that tag, or complete a todo so the chain reads past it."
  # No crate compiles any of them, so the release moves no gate key. The cut removed every fragment.
  git add -- CHANGELOG.md app/release.toml && git rm -rq --cached --ignore-unmatch -- changelog.d
  git commit --quiet -m "release: $version" \
    -m "$(trailers Release "$version" Completes "${todos[*]}" Tasks "$tasks")" \
    || die "the release $version did not commit (a hook?)."
  printf 'release %s%s\n' "$version" "$( ((${#todos[@]} == 0)) || printf ', completing %s' "${todos[*]}")"
}

build() {
  (($#)) || usage
  local n
  for n in "$@"; do [[ "$n" =~ ^[1-9][0-9]*$ ]] || die "\"$n\" is not a pull request number."; done
  clean
  fetch_origin
  git checkout --quiet -B train origin/master
  record_origin
  : >"$state/skipped"
  local rows=() prs meta title url head current said code unknown ids=() checker=""
  local ready=()
  local -A head_of=() title_of=() url_of=() todos_of=() growth_lines=() carrier_of=()
  # `Completes:` todos are placed, on the roadmap's head or the upkeep chain, by master's binary.
  local tasks_dir=""
  find_roadmap origin/master
  if [[ -n "$roadmap" ]]; then
    checker="$app_bin" tasks_dir="$(mktemp -d)"
    store_at "$(tasks_head)" "$tasks_dir"
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
    if ! growth_lines[$n]="$(growth_of "$n")"; then
      rows+=("#$n|skipped|gh could not read its description")
      echo "#$n gh could not read its description" >>"$state/skipped"
      continue
    fi
    read -r -a ids <<<"${todos_of[$n]}"
    if ((${#ids[@]})) && [[ -n "$checker" ]]; then
      code=0
      unknown="$("$checker" release known --store "$tasks_dir" "${ids[@]}" 2>&1)" || code=$?
      case "$code" in
        0) ;;
        3)
          rows+=("#$n|skipped|its Completes: line names a todo neither the roadmap nor the upkeep chain holds: ${unknown##*: }")
          echo "#$n Completes: unknown todo" >>"$state/skipped"
          continue
          ;;
        *) die "the roadmap could not be asked about #$n's Completes: line, so no PR is judged by it:\n$unknown" ;;
      esac
    fi
    head_of[$n]="$head" title_of[$n]="$title" url_of[$n]="$url"
    ready+=("$n")
  done
  [[ -z "$tasks_dir" ]] || rm -rf "$tasks_dir"
  # The order is a function of the heads: most other heads contained first, then head name.
  local order=() m held
  mapfile -t order < <(for n in "${ready[@]}"; do
    held=0
    for m in "${ready[@]}"; do
      [[ "$m" == "$n" ]] || ! git merge-base --is-ancestor "${head_of[$m]}" "${head_of[$n]}" || held=$((held + 1))
    done
    echo "$held ${head_of[$n]} $n"
  done | sort -k1,1nr -k2,2 -k3,3n | cut -d' ' -f3)
  local subject trailer todos carried conflicts resolved file
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
    said="$( (trailers Completes "$todos" Carries "${carried[*]}"; grown "$n") | sed '/^$/d')"
    [[ -z "$said" ]] || trailer=(-m "$said")
    said=""
    if ! git merge --quiet --no-ff -m "$subject" -m "${url_of[$n]}" "${trailer[@]}" "$head" >/dev/null 2>&1; then
      mapfile -t conflicts < <(git diff --name-only --diff-filter=U)
      resolved=()
      for file in "${conflicts[@]}"; do
        case "$file" in
          app/Cargo.lock) relock && resolved+=("$file") ;;
        esac
      done
      if ((${#conflicts[@]})) && ((${#resolved[@]} == ${#conflicts[@]})) \
        && git commit --quiet -m "$subject" -m "${url_of[$n]}" \
          -m "$( (trailers Completes "$todos" Carries "${carried[*]}" Joined "${resolved[*]}" Mentions "${resolved[*]}"; grown "$n") | sed '/^$/d')" \
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

# reading <var> <commit> [--half <half>] [--full]: <var> := ok, failed, unknown or finding, of one
# half or, with no --half, the train's verdict (ok < unknown < failed < finding); from every gate or
# with --full from full gates. A table the tool cannot read stops the train.
reading() {
  local -n into="$1"
  into="$("$tool" table read "${@:2}")" || die "the verdict table could not be read (above)."
}

# record <half> <commit> <code>: the half's row; 0 ok, 1 failed, 2 no row (75: did not start).
record() {
  local said=0 scope=()
  ((!full)) || scope=(--full)
  "$tool" table write "$@" "${scope[@]}" || said=$?
  ((said <= 2)) || die "the verdict table could not be written (above)."
  return "$said"
}

log_of() { mkdir -p "$state/logs" && echo "$state/logs/$1-$(git rev-parse "$2^{tree}").log"; }
# A red step's screenshots and server output, `<step>-<commit12>`, beside the logs (merge-gate.sh).
evidence_dir="$state/logs/evidence"

worktree="$state/worktree"
target="$state/target"

# One worktree path, so cargo's fingerprints, which name it, hold.
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
  run_here "$half" "$commit" "${@:3}" || code=$?
  record "$half" "$commit" "$code"
}

# run_here <half> <commit> [gate args...]: its gate in a worktree, as a half; 75: no worktree.
run_here() {
  local commit="$2" code=0
  worktree_open "$commit" \
    || { say "no worktree could be made at ${commit:0:12}"; return 75; }
  half gating "$@" || code=$?
  worktree_close
  return "$code"
}

# gating <half> <commit> [gate args...]: the gate, without train_reads, its log kept.
gating() {
  local gate unset=() name
  gate="${SUZATARY_TRAIN_GATE:-$worktree/scripts/merge-gate.sh}"
  for name in "${train_reads[@]}"; do unset+=(-u "$name"); done
  # shellcheck disable=SC2046
  (cd "$worktree" && env "${unset[@]}" SUZATARY_GATE_EVIDENCE="$evidence_dir" "$gate" $(gate_flags "$1") "${@:3}") 2>&1 \
    | tee "$(log_of "$1" "$2")" >&2
}

# half <command...>: <command> as a group `finish` stops, and its status; inside a half, part of it.
in_half=""
half() {
  [[ -z "$in_half" ]] || { "$@"; return; }
  start "$@"
  group_wait "$group"
}

# start <command...>: a half in the background; sets `group`. A subshell's, `finish` would miss.
start() {
  ((BASHPID == $$)) || die "a half started in a subshell: $*"
  exec 9>&"$lock_fd"
  in_half=1
  group_start "$@"
  in_half=""
  exec 9>&-
}

# The train's own test seams and signing keys; a gate is handed none of them.
train_reads=(SUZATARY_TRAIN_BIN SUZATARY_TRAIN_GATE SUZATARY_TRAIN_GH SUZATARY_TRAIN_POLL SUZATARY_TRAIN_PIN
  SUZATARY_TRAIN_PUBLIC SUZATARY_TRAIN_CACHE_TIMEOUT SUZATARY_BOX_KEY SUZATARY_AGENT_KEY SUZATARY_DEDUP_KEY)
# Variables the gate's steps read: an inherited one would be judged beside the tree.
gate_reads='^(SUZATARY_|CARGO|RUST|PROPTEST_|TYPST|NODE_OPTIONS$|LD_PRELOAD$|LD_LIBRARY_PATH$|LIBRARY_PATH$)'

# Refused outside `.#triage` or with an inherited `gate_reads` variable; SUZATARY_INSTANCE made absolute.
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

remote_opt() {
  case "$#/${1:-}/${2:-}" in
    0//) ;;
    2/--remote/github) cloud=github && needs "$gh" ;;
    *) usage ;;
  esac
}

# The tree half's steps that read this repository, before the push and after the publication run.
box_before="fmt,changelog"
box_after="public-tree,deployed-shape"
# The live half's steps that need no publication build, then the rest (a guard holds both whole).
live_early="live,live-all,live-guards"
# gate.yml runs a `gate/nightly-` branch --nightly.
gate_prefix=gate/
live_after="web-live,deploy-build,push-dry,public-check"
# A run not ended in six hours (GitHub's job limit) is no verdict.
gate_workflow=.github/workflows/gate.yml
cloud_hours=6
# Seconds to wait for the publication run to appear, and to end (past its own 180 minutes).
cache_appear=600
cache_timeout="${SUZATARY_TRAIN_CACHE_TIMEOUT:-12000}"
box_builds="the box builds the public tree itself, about 45 minutes of release build beside the app it serves"

core_repo() {
  local url
  url="$(git config --get remote.core.url)" || return 1
  [[ "$url" =~ github\.com[:/]([A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+)$ ]] || return 1
  echo "${BASH_REMATCH[1]%.git}"
}

# The tree half of <commit> on GitHub's runners, start to end: 0 green, 1 red, 2 no verdict.
cloud_half() {
  local commit="$1" pushed code=0
  cloud_push "$commit" || return
  gate_branch="$gate_prefix$commit" gate_pushed="$pushed"
  cloud_dispatch "$pushed"
  cloud_after "$commit" "$pushed" || code=$?
  ((code)) || half cloud_wait "$commit" "$pushed" || code=$?
  cloud_close
  return "$code"
}

# Sets `pushed`: `pushing` <commit> as a half.
cloud_push() {
  local code=0
  pushed=""
  half pushing "$1" >"$state/pushed" || code=$?
  ((code)) || pushed="$(<"$state/pushed")"
  return "$code"
}

# The box's steps, then the public tree pushed; prints the gate commit, else 1 (red) or 2.
pushing() {
  local commit="$1" dir code=0 repo pushed
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
  pushed="$(gate_push "$commit" "$dir" "$gate_prefix$commit" "Gate ${commit:0:12}" "Gate: $commit")" || return 2
  say "the tree half of ${commit:0:12} runs on GitHub's runners: $repo, branch $gate_prefix$commit, commit ${pushed:0:12}"
  echo "$pushed"
}

# gate_push <commit> <dir> <branch> <subject> <trailer>: <dir>'s tree pushed to <branch> alone.
gate_push() {
  local commit="$1" dir="$2" branch="$3" tree parent="" pushed
  if git ls-remote --exit-code --heads core "refs/heads/$core_branch" >/dev/null; then
    git fetch --quiet core "+refs/heads/$core_branch:refs/remotes/core/$core_branch" \
      || { rm -rf "$dir"; say "core's $core_branch could not be fetched, so nothing went to core"; return 2; }
    parent="$(git rev-parse "refs/remotes/core/$core_branch")"
  fi
  tree="$(tree_of "$dir")" || { rm -rf "$dir"; say "the public tree of ${commit:0:12} could not be written as a git tree"; return 2; }
  rm -rf "$dir"
  # Stamped with the instant, so each commit (and its run) has a sha of its own.
  pushed="$(public_commit "$tree" "$parent" "$(date -u +%FT%TZ)" "$4, $(date -u +%FT%T.%NZ)" "$5")" \
    && [[ "$pushed" =~ ^[0-9a-f]{40}$ ]] \
    || { say "the public tree of ${commit:0:12} could not be committed"; return 2; }
  git push --quiet core "+$pushed:refs/heads/$branch" \
    || { say "core refused $branch, so it says nothing of ${commit:0:12}"; return 2; }
  echo "$pushed"
}

# Starts master's publication run of <pushed> (no gate branch's workflow holds the cache token). A
# dispatch gh gave up on (124) may have started the run, so the train looks for it all the same.
dispatched=""
cloud_dispatch() {
  local repo code=0
  repo="$(core_repo)"
  timeout 60 "$gh" workflow run publication.yml --repo "$repo" --ref master -f "commit=$1" >&2 || code=$?
  case "$code" in
    0) dispatched="$1" ;;
    124)
      dispatched="$1"
      say "the dispatch of the publication run of ${1:0:12} on $repo got no answer within 60s: looking for the run all the same"
      ;;
    *) say "could not start the publication run of ${1:0:12} on $repo (above): $box_builds" ;;
  esac
}

# The box's steps once the dispatched run of <pushed> ends: 0 when they pass, else recorded.
cloud_after() {
  local commit="$1" pushed="$2" repo run url detail code=0
  repo="$(core_repo)"
  if [[ "$dispatched" == "$pushed" ]]; then
    say "waiting for the publication run of ${pushed:0:12} in $repo before the box builds the public tree (at most ${cache_timeout}s)"
    run="$("$scripts/publication-run.sh" "$gh" "$repo" workflow_dispatch "$pushed" "$cache_appear" "$cache_timeout" "${SUZATARY_TRAIN_POLL:-60}")"
    read -r run url detail <<<"$run"
    case "$run" in
      success) say "the publication run of ${pushed:0:12} succeeded: the box's builds come from the binary cache" ;;
      failed) say "the publication run of ${pushed:0:12} ended $detail ($url): $box_builds" ;;
      none) say "no publication run of ${pushed:0:12} appeared within ${cache_appear}s: $box_builds" ;;
      running) say "the publication run of ${pushed:0:12} was still $detail after ${cache_timeout}s ($url): $box_builds" ;;
      unasked) say "could not ask GitHub for the publication run of ${pushed:0:12} (gh's words above): $box_builds" ;;
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

# Records the gate run's verdict on the tree half of <commit>.
cloud_wait() {
  local code=0
  gate_run "$1" "$2" "$(log_of tree "$1")" || code=$?
  ((code != 2)) || return 2
  record tree "$1" "$code"
}

# gate_run <commit> <pushed> <log>: the gate run of <pushed>, 1 (red) once a job's `red` step
# failed, else at its end 0 on success, 2 otherwise; red jobs' logs go to <log>, `run_url` names it.
gate_run() {
  local commit="$1" pushed="$2" log="$3" repo run="" status="" misses=0 red="" job name deadline
  run_url=""
  repo="$(core_repo)"
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
  run_url="https://github.com/$repo/actions/runs/$run"
  say "waiting on $run_url"
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
    return 0
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
  return 1
}

# Cancels the gate commit's unfinished runs (no red tree's builds reach the cache); deletes its branch.
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

train_commit() {
  git rev-parse --verify --quiet "train^{commit}" || die "there is no train; scripts/train.sh build <pr>... first."
}

# beside <tree> <commit> [gate args...]: the live half beside the half <tree>; a red one stops the
# other, which records no verdict.
beside() {
  local tree="$1" live ended code=0 name=(tree live) other
  shift
  start run_half live "$@"
  live="$group"
  wait -n -p ended "$tree" "$live" || code=$?
  group_stop "$ended"
  if [[ "$ended" == "$tree" ]]; then other="$live"; else other="$tree" name=(live tree); fi
  if ((code == 1)); then
    say "the ${name[0]} half is red, so the ${name[1]} half is stopped"
    group_stop "$other"
    worktree_close
  else
    group_wait "$other" || true
  fi
}

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
    local pushed early=""
    cloud_push "$commit" || code=$?
    if ((code == 0)); then
      gate_branch="$gate_prefix$commit" gate_pushed="$pushed"
      cloud_dispatch "$pushed"
      early=0
      run_here live "$commit" --steps "$live_early" || early=$?
      if ((early)); then
        record live "$commit" "$early" || say "the live half's early steps gave no green, so the tree half is stopped"
      else
        cloud_after "$commit" "$pushed" || code=$?
        if ((code == 0)); then
          start cloud_wait "$commit" "$pushed"
          beside "$group" "$commit" --steps "$live_after"
        fi
      fi
      cloud_close
    fi
    case "$code" in
      1) say "the tree half is red on the box, so the live half's other steps do not run" ;;
      2)
        case "$early" in
          "") run_half live "$commit" || true ;;
          0) run_half live "$commit" --steps "$live_after" || true ;;
        esac
        ;;
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

# Per `--- job` section of <log>: the gate's failing-lines block, else its last error lines, uncoloured.
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

report() {
  local commit="$1" h v all short
  short="$(git rev-parse --short "$commit")"
  for h in "${halves[@]}"; do
    reading v "$commit" --half "$h"
    say "$h half: $v"
    [[ "$v" != failed || ! -s "$(log_of "$h" "$commit")" ]] || failing_lines "$(log_of "$h" "$commit")" >&2
  done
  reading all "$commit"
  case "$all" in
    ok) say "green: the train $short may land (scripts/train.sh land)" ;;
    finding) die "red, where a half once passed this tree: the gate is not a function of the tree. The logs are in $state/logs/." ;;
    unknown) die "a half has no verdict on $short: gate again." ;;
    failed) die "red: scripts/train.sh bisect finds the merge that broke it. The logs are in $state/logs/." ;;
  esac
}

pr_of() { git log -1 --format=%s "$1" | sed -n 's/^Merge #\([0-9]*\):.*/\1/p'; }

prs_of() { pr_of "$1" && trailer_of "$1" Carries | tr ' ' '\n' | sed '/^$/d'; }

# The longest prefix of a red train's merges its first red half passes, assuming red stays red.
bisect() {
  gate_environment bisect "$@"
  remote_opt "$@"
  sync
  local half="" h v all train
  train="$(train_commit)"
  reading all "$train"
  case "$all" in
    failed)
      for h in "${halves[@]}"; do reading v "$train" --half "$h"; if [[ -z "$half" && "$v" == failed ]]; then half="$h"; fi; done
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
    reading seen "$commit" --half "$half"
    if [[ "$seen" == unknown ]]; then
      say "probe: every merge, through #$(pr_of "$commit"), without the release on top"
      run_half "$half" "$commit" || true
      reading seen "$commit" --half "$half"
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
    reading seen "$commit" --half "$half"
    if [[ "$seen" == unknown ]]; then
      say "probe: the first $mid merges, through #$(pr_of "$commit")"
      run_half "$half" "$commit" || true
      reading seen "$commit" --half "$half"
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

nightly_gate() {
  (($# == 0)) || usage
  gate_environment nightly
  full=1 nightly=1
  sync
  local commit h v code failed=() unknown=()
  commit="$(git rev-parse --verify origin/master)"
  if core_repo >/dev/null; then
    cloud=github
    nightly_remote "$commit"
  else
    for h in "${halves[@]}"; do run_half "$h" "$commit" || true; done
  fi
  for h in "${halves[@]}"; do
    reading v "$commit" --half "$h" --full; case "$v" in
      ok) ;;
      failed) failed+=("$h") ;;
      *) unknown+=("$h") ;;
    esac
  done
  code=0
  report_nightly "$commit" "${failed[@]}" || code=$?
  ((${#unknown[@]} == 0)) || die "the ${unknown[*]} half gave no verdict on origin/master ${commit:0:12}; the logs are in $state/logs/."
  return "$code"
}

# check <pr>: the gate run of <pr>'s head merged on origin/master, posted on the PR; a conflict or a
# refused guard is posted instead. No verdict is recorded.
check() {
  (($# == 1)) && [[ "$1" =~ ^[1-9][0-9]*$ ]] || usage
  local n="$1" meta current head master tree merge dir pushed code=0 log what verdict
  trap '[[ -z "$gate_branch" ]] || cloud_close' EXIT
  core_repo >/dev/null || die "the remote \`core\` is not a GitHub repository, so the tree half cannot run on GitHub's runners."
  fetch_origin
  git fetch --quiet origin "+refs/pull/$n/head:refs/check/pr$n" || die "origin has no refs/pull/$n/head."
  head="$(git rev-parse "refs/check/pr$n")" master="$(git rev-parse origin/master)"
  meta="$("$gh" pr view "$n" --json title,url,headRefOid --jq '[.title, .url, .headRefOid] | @tsv')" \
    || die "gh could not read #$n."
  IFS=$'\t' read -r _ _ current <<<"$meta"
  [[ "$current" == "$head" ]] || die "#$n: refs/pull/$n/head is not GitHub's head $current yet."
  what="The tree half of ${head:0:12} on master ${master:0:12}"
  if ! tree="$(git merge-tree --write-tree --name-only --no-messages "$master" "$head")"; then
    post "$n" "$what conflicts with master in: $(tail -n +2 <<<"$tree" | sed '/^$/d' | paste -sd' ')." && return 1
  fi
  merge="$(GIT_AUTHOR_NAME=Suzatary GIT_AUTHOR_EMAIL=publish@suzatary.invalid GIT_COMMITTER_NAME=Suzatary \
    GIT_COMMITTER_EMAIL=publish@suzatary.invalid git commit-tree "${tree%%$'\n'*}" -p "$master" -p "$head" -m "Check #$n")"
  dir="$(mktemp -d)" || die "could not make a scratch directory."
  # The guard's hits stay on the box: a comment says only that it refused.
  if ! (checked "$merge" "$dir"); then
    rm -rf "$dir"
    post "$n" "$what did not pass the leak guard on the box, or it could not run: nothing went to GitHub." && return 1
  fi
  pushed="$(gate_push "$merge" "$dir" "gate/pr$n-${head:0:12}" "Check #$n ${head:0:12}" "Check: $head")" \
    || die "nothing went to core for #$n (above)."
  gate_branch="gate/pr$n-${head:0:12}" gate_pushed="$pushed"
  log="$(mkdir -p "$state/logs" && echo "$state/logs/check-pr$n-$head.log")"
  : >"$log"
  gate_run "$merge" "$pushed" "$log" || code=$?
  cloud_close
  case "$code" in
    0) verdict="$what is green on GitHub's runners: $run_url" ;;
    1) verdict="$what is red on GitHub's runners: $run_url"$'\n\n```\n'"$(failing_lines "$log")"$'\n```' ;;
    *) verdict="$what gave no verdict on GitHub's runners: ${run_url:-no run}. Check again." ;;
  esac
  post "$n" "$verdict"
  return "$code"
}

# post <pr> <text>: a comment on the pull request, and the same words here.
post() {
  say "#$1: ${2%%$'\n'*}"
  "$gh" pr comment "$1" --body "$2" >/dev/null || die "could not comment on #$1 (above)."
}

# nightly_remote <commit>: the tree half on the runners, the live half here; neither stops the
# other, and the lock is let go while only the runners work.
nightly_remote() {
  local commit="$1" pushed code=0 a=0 b=0
  gate_prefix=gate/nightly-
  if cloud_push "$commit"; then
    gate_branch="$gate_prefix$commit" gate_pushed="$pushed"
    cloud_dispatch "$pushed"
    run_here live "$commit" --steps "$live_early" || a=$?
    cloud_after "$commit" "$pushed" || code=$?
    run_here live "$commit" --steps "$live_after" || b=$?
  else
    code=1
    run_here live "$commit" || a=$?
  fi
  # Red over silent over green.
  ((a == 0 || a == 75 && b && b != 75)) && a=$b
  record live "$commit" "$a" || true
  # Unlocked, a train may own the worktree.
  trap '[[ -z "$gate_branch" ]] || cloud_close' EXIT
  unlock
  ((code)) || cloud_wait "$commit" "$pushed" || true
  [[ -z "$gate_branch" ]] || cloud_close
  lock
  trap finish EXIT
}

# The failing steps of <commit>'s red halves, from the gate's summary rows, filed once per tree on
# the instance's [upkeep] chain, or on none when it names none.
report_nightly() {
  local commit="$1" h v steps=() logs=() scenes=() step detail id upkeep
  shift
  for h in "${halves[@]}"; do reading v "$commit" --half "$h" --full; say "$h half: $v"; done
  (($#)) || { say "the nightly gate passed origin/master ${commit:0:12}"; return 0; }
  for h; do
    logs+=(".git/train/logs/${h}-$(git rev-parse "$commit^{tree}").log")
    mapfile -t -O "${#steps[@]}" steps < <(sed -n 's/^\([a-z][a-z0-9-]*\) \{1,\}FAILED.*/\1/p' "$(log_of "$h" "$commit")" | awk '!seen[$0]++')
    # Each red scene's name (`<file>: <scene>`), never its assertion, which can quote the page.
    mapfile -t -O "${#scenes[@]}" scenes < <(sed -E 's/\x1b\[[0-9;]*m//g' "$(log_of "$h" "$commit")" |
      sed -nE 's/^[[:space:]]*✖ ([a-z]+: .+) \([0-9.]+ms\)$/\1/p' | tr -d '`' | awk '!seen[$0]++')
  done
  ((${#steps[@]})) || steps=("$@")
  detail="The gate's logs are in ${logs[*]}."
  ((${#scenes[@]} == 0)) || detail+=" The red scenes: $(printf '`%s`, ' "${scenes[@]}" | sed 's/, $//')."
  for step in "${steps[@]}"; do
    [[ ! -d "$evidence_dir/$step-${commit:0:12}" ]] \
      || detail+=" The $step step's screenshots and servers' output are on the box in \`${evidence_dir#"$root/"}/$step-${commit:0:12}\`."
  done
  id="nightly$(git rev-parse --short=8 "$commit^{tree}")"
  resolve_app origin/master
  upkeep="$("$app_bin" instance get --or-empty upkeep.chain)" \
    || die "the nightly gate is red (${steps[*]}), and its [upkeep] chain could not be read (above)."
  if [[ -z "$upkeep" ]]; then
    say "red: ${steps[*]}; the instance names no [upkeep] chain, so no todo is filed"
  elif "$app_bin" todo show --chain "$upkeep" "$id" >/dev/null 2>&1; then
    say "red: $id already names this tree's failing steps on $upkeep"
  else
    "$app_bin" todo add --chain "$upkeep" --actor assistant --priority 55 "$id" \
      --body "The nightly gate is red on master ${commit:0:12}: ${steps[*]}." \
      --detail "$detail" \
      --rationale "A step red on master either let a broken change land or no longer reads the data, and every later train inherits it." >&2 \
      || die "the nightly gate is red (${steps[*]}), and the todo naming it could not be filed (above). The logs are in $state/logs/."
    say "red: filed todo:$upkeep/$id for ${steps[*]}"
  fi
  return 1
}

# HEAD's release version, once the roadmap's head reads at least it and origin has no such tag.
checked_release() {
  find_roadmap HEAD
  [[ -n "$roadmap" ]] || return 0
  local version dir
  version="$(trailer_of HEAD Release)"
  [[ -n "$version" ]] \
    || die "the train's HEAD is not its release (no Release: trailer), so it has no version to tag: scripts/train.sh build again. Nothing landed."
  # Only origin's tags count; a local one is a refused land's leftover, replaced on tagging.
  ! git ls-remote --exit-code --tags origin "refs/tags/v$version" >/dev/null \
    || die "origin has v$version already, so this train's release would be a second $version: scripts/train.sh build again, which reads the roadmap afresh. Nothing landed."
  dir="$(mktemp -d)"
  store_at "$(tasks_head)" "$dir"
  "$app_bin" release check --tree "$root" --store "$dir" >/dev/null \
    || { rm -rf "$dir"; die "the roadmap, with this train's todos done, reads below $version (above): scripts/train.sh build again. Nothing landed."; }
  rm -rf "$dir"
  echo "$version"
}

# Whether a commit of the roadmap at <ref> names <rev> in its `Shipped:` trailer (awk reads to
# the end, so git log never dies of a closed pipe).
shipped() {
  git -C "$roadmap" log --format='%(trailers:key=Shipped,valueonly,separator=%x2C)' "$2" \
    | awk -v rev="$1" '$0 == rev { f = 1 } END { exit !f }'
}

# record_release <rev>: by <rev>'s app, each step skipped where the roadmap holds it: its Completes:
# todos done where each is found, the roadmap's pin moved to core's publication of <rev>, then one
# roadmap commit naming <rev> in `Shipped:` (roadmap-push.sh). No `Tasks:`: nothing to record.
# The roadmap learns nothing of the private repository: only the version and the landed commit,
# which core's publication names as its `Source:`.
record_release() {
  local rev="$1" version tasks subject release completes todo core notes=() todos=()
  version="$(trailer_of "$rev" Release)"
  [[ -n "$version" ]] || die "${rev:0:12} is not a release (no Release: trailer); nothing to record."
  tasks="$(trailer_of "$rev" Tasks)"
  [[ -n "$tasks" ]] || return 0
  find_roadmap "$rev"
  [[ -n "$roadmap" ]] \
    || die "${rev:0:12} names the roadmap commit ${tasks:0:12}, and the deployment mounts no plan store to record $version on."
  if shipped "$rev" origin/master; then
    return 0
  fi
  if ! shipped "$rev" HEAD; then
    while IFS=$'\x1f' read -r subject release completes; do
      [[ -z "$release" ]] || break
      [[ "$subject" =~ ^Merge\ \#([0-9]+): ]] || continue
      for todo in $completes; do
        todos+=("$todo")
        # The PR is an upkeep completion's note alone: the roadmap's names the release.
        notes+=(--note "$todo=PR #${BASH_REMATCH[1]}, $version")
      done
    done < <(git log --first-parent \
      --format='%s%x1f%(trailers:key=Release,valueonly,separator= )%x1f%(trailers:key=Completes,valueonly,separator= )' \
      "$rev^")
    if ((${#todos[@]})); then
      "$app_bin" release complete --release "$version" "${todos[@]}" "${notes[@]}" >&2 \
        || die "the todos $version completed are not all recorded done (above): scripts/train.sh record ${rev:0:12}"
    fi
    git fetch --quiet core "+refs/heads/$core_branch:refs/remotes/core/$core_branch" \
      || die "core could not be fetched, so the roadmap's pin has nothing to move to: scripts/train.sh record ${rev:0:12}"
    core="$(published "$rev")"
    [[ -n "$core" ]] \
      || die "core does not publish ${rev:0:12} yet, and the roadmap pins the published code: scripts/train.sh publish, which records it after."
    pin "$core"
  fi
  SUZATARY_BIN="$app_bin" "$scripts/roadmap-push.sh" "$roadmap" "Ship $version" \
    "Release: $version" "Shipped: $rev" >&2 \
    || die "$version is recorded in the roadmap's clone and not pushed (above): scripts/train.sh record ${rev:0:12}"
  printf 'recorded: %s on the roadmap\n' "$version"
}

# pin <core commit>: the roadmap's flake.lock moved to that commit of the application.
pin() {
  if [[ -n "${SUZATARY_TRAIN_PIN:-}" ]]; then
    "$SUZATARY_TRAIN_PIN" "$roadmap" "$1"
  else
    (cd "$roadmap" && nix flake lock --update-input suzatary)
  fi || die "the roadmap's pin could not be moved (above)."
  grep -q "\"rev\": \"$1\"" "$roadmap/flake.lock" \
    || die "the roadmap's flake.lock does not pin core's ${1:0:12}, its publication of the release: core moved since, so record again."
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
  record_release "$rev"
}

land() {
  (($# == 0)) || usage
  clean
  sync
  on_train
  local short
  short="$(git rev-parse --short HEAD)"
  local h v all missing=()
  for h in "${halves[@]}"; do reading v HEAD --half "$h"; [[ "$v" == ok ]] || missing+=("$h: $v"); done
  reading all HEAD
  case "$all" in
    ok) ;;
    unknown) die "the gate has not passed the train's HEAD ($short) in both halves (${missing[*]}): scripts/train.sh gate first. Nothing landed." ;;
    failed) die "the gate failed on the train's HEAD ($short) (${missing[*]}): scripts/train.sh bisect, build without the merge it names, gate again. Nothing landed." ;;
    finding) die "the gate both passed and failed the train's tree ($short) (${missing[*]}), so neither verdict lands: it read something beyond the tree. Nothing landed." ;;
  esac
  # A moved master makes another tree, so the train is built and gated again.
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
  # A refusal exits this substitution, and its failure stops the land here.
  version="$(checked_release)"
  # One atomic push of master and v<version>; a refusal leaves the train as it was.
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
  # In order: code, core, roadmap. The roadmap pins core's row, so it is recorded last.
  (publish "$landed") \
    || die "master landed at ${landed:0:12}, and its public tree was not published, so no deploy can run it yet; scripts/train.sh publish, once the cause above is mended, then records its release."
  [[ -z "$version" ]] || (record_release "$landed") \
    || die "master landed, v$version is tagged and published, and the roadmap does not record it yet: scripts/train.sh record ${landed:0:12}"
  printf 'next, by the operator (this script deploys nothing):\n'
  printf '  scripts/deploy.sh %s\n' "$landed"
}

core_branch=master
# Each line a person's word on those bytes, so a new line is held as a new path is.
admitted=scripts/public-admitted.sha256
# A change to the secret rules changes what passes, so it is admitted as a line is.
rules=scripts/secret-rules.toml

# The core commit whose Source is <rev>, or nothing (awk reads the whole log: no SIGPIPE).
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

newest() {
  git rev-parse --verify --quiet "refs/remotes/core/$core_branch" >/dev/null || return 0
  git log --first-parent --format='%(trailers:key=Source,valueonly,separator=%x2C)' \
    "refs/remotes/core/$core_branch" | awk 'NF && !n++'
}

# Publishes core's missing rows up to <rev> (origin/master), oldest first, until a leak guard fails.
publish() {
  local rev heads last source sources=() skips=() skipped=() at
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
  # An empty <rev> is a mistake, never "the tip", which would leave the meant rev unpublished.
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

# on_chain <commit> <branch>, read to the end (a reader quitting early breaks the pipe: pipefail).
on_chain() {
  git rev-list --first-parent "$2" | awk -v r="$1" '$0 == r { f = 1 } END { exit !f }'
}

# checked <source> [<dir>]: the leak guard on <source>'s public tree (kept in <dir>/tree): 0 passes,
# 1 hits (on stderr); a guard that could not run dies, so no check is skipped unrun.
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
  SUZATARY_BIN="${SUZATARY_TRAIN_BIN:-$(deployed_app)/bin/suzatary}" "$public" --check --strict "$dir/tree" >&2 || code=$?
  [[ -z "$own" ]] || rm -rf "$dir"
  case "$code" in
    0 | 1) return "$code" ;;
    *)
      rm -rf "$dir"
      die "the leak guard could not run over the public tree of ${source:0:12} (it exited $code; see above). Nothing published."
      ;;
  esac
}

# "<source> <path>" for each path or admission (`suzatary leaks --admissions`) of each <source> that
# no core row or earlier source holds; none while core is empty (the operator's baseline).
fresh_rows() {
  local public scratch s c bin
  git rev-parse --verify --quiet "refs/remotes/core/$core_branch" >/dev/null || return 0
  public="${SUZATARY_TRAIN_PUBLIC:-$root/scripts/public-tree.sh}"
  bin="${SUZATARY_TRAIN_BIN:-}"
  [ -n "$bin" ] || bin="$(deployed_app)/bin/suzatary"
  scratch="$(mktemp -d)" || return 1
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

# <dir>/tree as a git tree, its id printed, through its own index, forced past any `.gitignore`.
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

# One row: <source>'s checked public tree, committed on core's master and pushed.
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

# The train's tool, built from this checkout before anything else into its own target under .git,
# so a train that changes the tool is gated and landed by it; under a test seam, apart, with seams.
case "${1:-}" in
  gate | bisect | land | nightly)
    tool="$(git -C "$scripts" rev-parse --path-format=absolute --git-common-dir)/train/tool" features=()
    if [[ -n "${!SUZATARY_TRAIN_*}" ]]; then tool+=-seams && features=(--features seams); fi
    # The account's cargo home, whatever HOME the train was handed: another registry path makes every
    # dependency dirty, and the next build compiles them all again.
    account="$(getent passwd "$(id -u)" | cut -d: -f6)" || account="$HOME"
    CARGO_HOME="${CARGO_HOME:-$account/.cargo}" cargo build --quiet --locked -p suzatary-tooling \
      --manifest-path "$scripts/../app/Cargo.toml" --target-dir "$tool" "${features[@]}" \
      || die "the train's tool (app/tooling) did not build (above). Nothing ran."
    tool+=/debug/train
    ;;
esac

case "${1:-}" in
  build) shift && build "$@" ;;
  gate) shift && gate "$@" ;;
  bisect) shift && bisect "$@" ;;
  land) shift && land "$@" ;;
  record) shift && record_done "$@" ;;
  nightly) shift && nightly_gate "$@" ;;
  check) shift && check "$@" ;;
  publish) shift && fetch_origin && publish "$@" && record_origin ;;
  *) usage ;;
esac
