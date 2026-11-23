#!/usr/bin/env bash
# The merge queue on the box: reviewed PRs ride a train, one integration branch gated once by
# scripts/merge-gate.sh and landed as one fast-forward of master.
#
#   build <pr>...    train := origin/master, each PR merged in, then the release cut on top
#   gate [--full]    both halves of scripts/merge-gate.sh on the train (the tool's `train gate`)
#   bisect           the merge that turned a red half red
#   land             master and v<version> := the gated train, pushed as one while master has not
#                    moved; then published; then its release recorded on the roadmap
#   record [<rev>]   the release at <rev> recorded: its todos done where each is found, the
#                    roadmap's pin moved and its pictures drawn in one commit, scanned and pushed
#   nightly          both halves, full, on origin/master; a red one files a todo
#   check <pr>       the runners' jobs over <pr>'s head merged on master, posted on the PR; no lock
#   publish [--skip <sha>]... [--admit <path>]... [<rev>]
#                    core's master += the public tree of each landed commit up to <rev>
#   set-aside <half> --because <task> "<words>"
#                    the newest red row of <half> where a land reads it set aside, at a terminal,
#                    for the reason an open or completed roadmap task gives; a land prints it
#
# `--remote github` runs the tree half on core's runners (only a public tree the tool finds
# publishable leaves the box: its leak guard passed, and it holds no admission core lacks); the box keeps the steps that read this repository and the live half. Run inside
# `nix develop .#triage`, from a clean checkout; it never touches the data repository.
#
# Verdicts go to .git/train/gates/, keyed by half and tree, the live half's also by a digest of the
# owner's stores read before the run, and only the train's tool reads or writes them (app/tooling,
# `train table`). A land reads the live half at its newest row's digest; a key holding both a green
# and a red that no person set aside is a finding.
#
# `gate`, `bisect`, `nightly` and `check` are the tool's (app/tooling, `gate_run.rs`); every other
# verb runs under the train's lock, which only the tool holds (`train locked`).
#
# Test seams: the SUZATARY_TRAIN_ names of the tool's `TRAIN_READS`; a stub gate's verdicts are
# keyed by its own bytes, so a stub's green never lands a train.
set -euo pipefail

die() {
  printf 'train: %b\n' "$*" >&2
  exit 1
}
say() { printf 'train: %s\n' "$*" >&2; }
usage() {
  echo "usage: scripts/train.sh build <pr>... | gate [--full] [--remote github] | bisect [--remote github] | land | record [<rev>] | nightly | check <pr> | publish [--skip <sha>]... [--admit <path>]... [<rev>] | set-aside <tree|live> --because <task> \"<words>\"" >&2
  exit 64
}

gh="${SUZATARY_TRAIN_GH:-gh}"
# Before anything: a missing git or flock would read as no checkout or another train.
needs() {
  local tool
  for tool; do
    command -v "$tool" >/dev/null \
      || die "$tool is not on PATH; the .#triage shell (flake.nix) carries every tool this script calls. Nothing ran."
  done
}
needs git flock cargo getent
case "${1:-}" in build | land | check | nightly) needs "$gh" ;; esac
scripts="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(git rev-parse --show-toplevel)" || die "not inside a git checkout."
cd "$root"
# In .git, never in the tree.
state="$(git rev-parse --path-format=absolute --git-common-dir)/train"
mkdir -p "$state"
# Land's return to the train after a refused push; every other verb's lock and stop path is the
# tool's (`train locked`, `train gate`).
return_to=""
trap '[[ -z "$return_to" ]] || git checkout --quiet "$return_to"' EXIT

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

# PR <n>'s growth lines (`Reading-growth:`, `Use-growth:`, `Wasm-growth:`), which its merge carries
# as trailers.
growth_of() {
  local body line
  body="$("$gh" pr view "$1" --json body --jq .body)" || return 1
  while IFS= read -r line; do
    line="${line%$'\r'}"
    [[ "$line" =~ ^[[:space:]]*(Reading-growth|Use-growth|Wasm-growth):[[:space:]]*(.+)$ ]] || continue
    echo "${BASH_REMATCH[1]}: ${BASH_REMATCH[2]}"
  done <<<"$body"
}

# PR <n>'s growth lines, ready to follow its other trailers.
grown() { sed '/^$/d' <<<"${growth_lines[$1]}"; }

# Every derived figure of the merged tree recorded and staged by the tool, the wasm limits from
# origin/master's modules as its publication cached them (AGENTS.md, "Derived figures"): no build.
# On stdout, the figures that moved, for the commit to say.
figures_line() {
  [[ -e context.toml ]] || return 0
  local out modules=() files=(context.toml)
  if [[ -e web/wasm-budget ]]; then
    files+=(web/wasm-budget)
    out="$("${SUZATARY_TRAIN_WASM:-$scripts/wasm-modules.sh}" origin/master)" \
      || die "origin/master's wasm modules could not be fetched (above), so the train is not built."
    mapfile -t modules <<<"$out"
  fi
  out="$("$tool" context --repo "$root" record ${modules[0]:+--wasm "${modules[@]}"})" && git add -- "${files[@]}" \
    || die "the derived figures of the merged tree could not be measured (above), so the train is not built."
  echo "The derived figures of the merged tree: ${out:-none moved}"
}

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

# The app the box runs, which no tree the train judges built.
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

# The train's last commit: the release the roadmap reads once the train's todos are done, with the
# derived figures.
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
  # The cut removed every fragment. The figures are recorded last, over exactly what commits.
  git add -- CHANGELOG.md app/release.toml && git rm -rq --cached --ignore-unmatch -- changelog.d
  local said
  said="$(figures_line)" || exit 1
  git commit --quiet -m "release: $version" ${said:+-m "$said"} \
    -m "$(trailers Release "$version" Completes "${todos[*]}" Tasks "$tasks" Figures "$figures")" \
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
    said="$( (trailers Completes "$todos" Carries "${carried[*]}"; for m in "$n" "${carried[@]}"; do grown "$m"; done) | sed '/^$/d')"
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
          -m "$( (trailers Completes "$todos" Carries "${carried[*]}" Joined "${resolved[*]}" Mentions "${resolved[*]}"; for m in "$n" "${carried[@]}"; do grown "$m"; done) | sed '/^$/d')" \
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
  local said cut
  # The train's commit's `Figures:` trailer, where context.toml stands.
  [[ ! -e context.toml ]] || figures="over origin/master $(git rev-parse --short=12 origin/master)"
  cut="$(git rev-parse HEAD)"
  cut_release
  # With no release to say them in, the figures ride a commit of their own: the train's last commit
  # records them either way, which the growth check reads.
  if [[ -n "$figures" && "$(git rev-parse HEAD)" == "$cut" ]]; then
    said="$(figures_line)" || exit 1
    git commit --quiet --allow-empty -m "figures: the derived figures of the merged tree" -m "$said" \
      -m "$(trailers Figures "$figures")" \
      || die "the derived figures did not commit (a hook?)."
  fi
}

figures=""
halves=(tree live)

# reading <var> <commit> [--half <half>] [--full]: <var> := ok, failed, unknown or finding, of one
# half or, with no --half, the train's verdict (ok < unknown < failed < finding); from every gate or
# with --full from full gates. A table the tool cannot read stops the train.
reading() {
  local -n into="$1"
  into="$("$tool" table read "${@:2}")" || die "the verdict table could not be read (above)."
}

log_of() { mkdir -p "$state/logs" && echo "$state/logs/$1-$(git rev-parse "$2^{tree}").log"; }
# A red step's screenshots and server output, `<step>-<commit12>`, beside the logs (merge-gate.sh).
evidence_dir="$state/logs/evidence"

on_train() {
  git rev-parse --verify --quiet train >/dev/null || die "there is no train; scripts/train.sh build <pr>... first."
  git checkout --quiet train
}

pr_of() { git log -1 --format=%s "$1" | sed -n 's/^Merge #\([0-9]*\):.*/\1/p'; }

prs_of() { pr_of "$1" && trailer_of "$1" Carries | tr ' ' '\n' | sed '/^$/d'; }

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
    git fetch --quiet core "+refs/heads/master:refs/remotes/core/master" \
      || die "core could not be fetched, so the roadmap's pin has nothing to move to: scripts/train.sh record ${rev:0:12}"
    core="$("$tool" published "$rev")" || die "core's rows could not be read (above): scripts/train.sh record ${rev:0:12}"
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
  # Each half's reading and every set-aside and lapsed red it prints, from one read of the table;
  # a refusal is the tool's own words.
  "$tool" table land HEAD || exit 1
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
  "$tool" publish "$landed" \
    || die "master landed at ${landed:0:12}, and its public tree was not published, so no deploy can run it yet; scripts/train.sh publish, once the cause above is mended, then records its release."
  [[ -z "$version" ]] || (record_release "$landed") \
    || die "master landed, v$version is tagged and published, and the roadmap does not record it yet: scripts/train.sh record ${landed:0:12}"
  printf 'next, by the operator (this script deploys nothing):\n'
  printf '  scripts/deploy.sh %s\n' "$landed"
}

# set-aside <half> --because <task> "<words>": the task read on the roadmap's head by master's app, as
# a build reads a `Completes:` todo, and taken only open or completed; the tool sets the train's
# newest red row of <half> aside, and refuses unless its stdin is a terminal.
set_aside() {
  (($# == 4)) && [[ "$2" == --because ]] || usage
  local half="$1" task="$3" words="$4" dir shown failed=0
  # As completes_of takes a todo id, before the id reaches a pattern.
  [[ "$task" =~ ^[a-z0-9_-]+$ ]] || die "\"$task\" is not a todo id. Nothing is set aside."
  find_roadmap origin/master
  [[ -n "$roadmap" ]] || die "the deployment mounts no roadmap, so no task can say why a red is set aside. Nothing is set aside."
  dir="$(mktemp -d)"
  # In a subshell, so the directory goes whatever stops the read.
  shown="$(store_at "$(tasks_head)" "$dir" && "$app_bin" todo show --store "$dir" "$task" 2>&1)" || failed=$?
  rm -rf "$dir"
  ((failed == 0)) || die "the roadmap could not be read, or holds no task $task ($shown). Nothing is set aside."
  # `todo show` prints the todo's id, then its state, in one JSON object.
  shown="$(sed -n "s/.*\"todo\":\"$task\",\"state\":\"\\([^\"]*\\)\".*/\\1/p" <<<"$shown")"
  case "$shown" in
    open | completed) ;;
    *) die "the roadmap's $task is ${shown:-unreadable}, and only an open or completed task says why a red is set aside. Nothing is set aside." ;;
  esac
  "$tool" table set-aside "$half" "$task" "$words"
}

# The train's tool, built from this checkout before anything else into its own target under .git,
# so a train that changes the tool is gated and landed by it; under a test seam, apart, with seams.
case "${1:-}" in
  build | gate | bisect | land | record | nightly | check | publish | sync | nightly-report | set-aside)
    tool="$(git -C "$scripts" rev-parse --path-format=absolute --git-common-dir)/train/tool" features=()
    if [[ -n "${!SUZATARY_TRAIN_*}" ]]; then tool+=-seams && features=(--features seams); fi
    # `check` takes no lock, so it never builds into the target whose binary a locked train execs.
    [[ "$1" != check ]] || tool+=-check
    # The account's cargo home, whatever HOME the train was handed: another registry path makes every
    # dependency dirty, and the next build compiles them all again.
    account="$(getent passwd "$(id -u)" | cut -d: -f6)" || account="$HOME"
    build=(env CARGO_HOME="${CARGO_HOME:-$account/.cargo}" cargo build --quiet --locked -p suzatary-tooling
      --manifest-path "$scripts/../app/Cargo.toml" --target-dir "$tool" "${features[@]}")
    # A child of the tool runs the tool its parent built. Every other locked verb builds under the
    # train's lock, so no build reads a checkout a running train is merging; the nightly waits.
    # `-o`: the build is not handed the lock's descriptor, so nothing it leaves behind holds it.
    built=0
    if [[ -n "${SUZATARY_LOCK_HOLDER:-}" ]]; then
      :
    elif [[ "$1" == check ]]; then
      "${build[@]}" || built=$?
    else
      flock -o -n -E 75 "$state/lock" "${build[@]}" || built=$?
      if ((built == 75)) && [[ "$1" == nightly ]]; then
        say "a train is running here: the nightly gate waits for it"
        built=0 && flock -o "$state/lock" "${build[@]}" || built=$?
      fi
      ((built != 75)) \
        || die "another scripts/train.sh is running in this checkout ($(cat "$state/lock.holder" 2>/dev/null)); wait for it to finish."
    fi
    ((built == 0)) || die "the train's tool (app/tooling) did not build (above). Nothing ran."
    tool+=/debug/train
    ;;
esac

# A verb of this script runs as a child of the tool, which holds the lock; a gate's `sync` and
# `nightly-report` are the tool's own children, under its lock.
case "${1:-}" in
  build | land | record | publish | sync | nightly-report | set-aside)
    [[ -n "${SUZATARY_LOCK_HOLDER:-}" ]] || exec "$tool" locked "$1" -- "$scripts/train.sh" "$@"
    ;;
esac

case "${1:-}" in
  build) shift && build "$@" ;;
  land) shift && land "$@" ;;
  record) shift && record_done "$@" ;;
  publish) shift && fetch_origin && "$tool" publish "$@" && record_origin ;;
  sync) shift && { (($# == 0)) || usage; } && sync ;;
  nightly-report) shift && report_nightly "$@" ;;
  set-aside) shift && set_aside "$@" ;;
  gate | bisect | nightly | check) SUZATARY_SCRIPTS="$scripts" exec "$tool" "$@" ;;
  *) usage ;;
esac
