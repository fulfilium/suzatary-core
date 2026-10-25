#!/usr/bin/env bash
# A DEPLOY IS A COMMIT TO THE USER REPOSITORY (docs/USERREPO-PLAN-2026-10-01.md
# §4). The units run the app its committed flake.lock names (nix/services.nix),
# so moving that lock in a commit is the deploy, and the user repository's
# history says which code ran when. This script is the one thing that moves it,
# and it owns the whole deploy after the move:
#
#   scripts/deploy.sh <rev>   pin the app at <rev>, a full commit id: a
#                             commit of the declared input's branch's
#                             first-parent chain (suzatary-core's master) is
#                             pinned as it is; otherwise the commit of it
#                             whose `Source:` trailer names <rev> (the landed
#                             commit scripts/train.sh land published); a
#                             rollback is a deploy of an earlier rev, and a
#                             deploy of the rev already locked retries one
#   scripts/deploy.sh <rev> --ref <branch>
#                             the same on another branch of the input (a fix
#                             not yet landed): the first-parent check runs on
#                             <branch>, and the pin carries ?ref=<branch>
#   scripts/deploy.sh <rev> --instance <file>
#                             the same, carrying the instance file <rev> reads:
#                             <rev>'s binary checks <file>, not the live one,
#                             and the "Deploy <rev>" commit writes <file> over
#                             the instance the web serves together with the
#                             lock, so a change of config and the code that
#                             reads it land, and revert, as one
#   scripts/deploy.sh <rev> --flake <file>
#                             the same, carrying the user repository's
#                             flake.nix: <rev> is resolved in, and pinned from,
#                             the input <file> declares, and the commit writes
#                             <file> beside the lock (the one move of an input
#                             to suzatary-core is such a deploy); both options
#                             may be given together
#
# WHAT A DEPLOY PINS. The user repository's flake.nix DECLARES where the app
# comes from, a branch of published trees (suzatary-core's master, which
# scripts/train.sh publishes one commit to per landed commit, naming it in a
# `Source:` trailer); its flake.lock names WHICH commit of it runs. A deploy
# names either that commit, as a user of the published repository sees it, or
# the source commit, as land prints it, and pins exactly that commit of the
# branch; a rev that is neither is refused, naming both lookups. Only the
# branch's first-parent chain went through a gate (a merge's series never
# did), and only land publishes, from master's first-parent chain alone.
#
# A DEPLOY CARRIES ITS CONFIG. An instance format the old binary and the new
# one each refuse in the other's form cannot be edited before the deploy (the
# old units would refuse it) or after (the new ones would). With --instance,
# the units read the old file until the lock moves; the commit moves both, and
# since the build is done first, the link and the restart follow at once. A
# revert puts both back. The carried file is read once, at the start, and the
# move refuses, naming the change, when the live instance was committed again
# while the build ran (the old web saves settings into it), since the carried
# file was made from the one before. Once the web runs the new app, the way
# back is printed with the instance the previous rev read.
#
# IT BUILDS FIRST. Before the lock moves, the app's builds
# (scripts/app-builds.sh, the list the roots run builds) are built from the
# tree the new lock names and rooted under `deploy-`, for as long as the store
# takes, while every unit keeps serving the app it ran. A failed build moves
# nothing, and neither does a live instance the new binary refuses to read
# (`verify`, which writes nothing). The run the move causes then evaluates the
# same derivations, finds every output in the store and only links, so its
# wait is short, and the deploy checks that the run built exactly the outputs
# it built, path for path, before it removes its own roots. A failed deploy
# leaves them for the next, so a retry builds nothing. The previous app's
# builds are rooted under `previous-` until the deploy succeeds, so a revert's
# relink builds nothing either. And a landed commit's builds are usually made
# already: the merge gate's live half (`deploy-build`) ran this same list over
# that commit's public tree, which is the store path the published commit is
# fetched to, byte for byte, so a deploy of what the last train landed finds
# every output in the store and its build is an evaluation. When the store
# does not hold them, GitHub's runners have built them: each commit of
# suzatary-core's master runs .github/workflows/publication.yml, which pushes
# the builds to the binary cache this box substitutes from, and the deploy
# waits for that run, bounded, so its build downloads; a run that failed or
# never came leaves the build to this box, as before, and the deploy says so.
#
# THE LOG NEVER LIES, AND ONLY GROWS. The new lock is computed beside the user
# repository and committed there through an index of the deploy's own, under
# the push lock scripts/safe-push.sh holds, and only then checked out, so the
# lock suzatary-roots.path watches only ever holds a committed state and no run
# it starts meets a lock it must refuse; a failed commit wrote nothing. If the units do not move
# to the new app (the roots run fails, never comes, or links something else),
# the deploy is undone by a second commit, `Revert "Deploy <rev>"`, so the log
# says what ran, and a run of its own relinks the previous app. Both commits
# are pushed last. Only a published commit goes live.
#
# A SIGNAL IS A FAILURE. TERM, HUP and INT exit non-zero, so the deploy is
# reverted (before the units restart) or recorded (after it) as any failure
# is. Only SIGKILL skips that: then `git -C <user repository> log -1` says
# where it stopped. A "Deploy <rev>" not yet pushed is pushed if the web runs
# that rev (or simply deployed again), and reverted by deploying the previous
# rev if it does not.
#
# WHY IT WAITS ON THE RUN'S START TIME, NOT ON `Result` ALONE. A
# `Result=success` can be left over from the PREVIOUS roots run, and trusting
# it restarts the web onto a build that is failing (2026-09-26: twelve minutes
# down). So the epoch is taken once the lock is committed, the only run that
# counts is one whose ExecMainStartTimestamp is at or after it and which has
# finished, and only its `Result=success`, with the app it linked being the
# one the lock names, lets the units restart. Every other ending is an exit with
# a sentence saying what happened, and every unit keeps running what it ran.
#
# IT RESTARTS EVERY UNIT THAT RUNS THE APP FOR LONGER THAN ONE JOB. Such a unit
# goes on running the app it started with, so each is declared PartOf
# suzatary-app.target (nix/nixos-module.nix `servingUnit`), and the deploy
# try-restarts each unit the target's ConsistsOf names, once the roots run has
# linked the new app: a running unit restarts, a stopped one (a watcher with no
# account to watch) stays down and reads the new app when it next starts. A
# timer's oneshot reads the lock at each start, so it needs no restart.
#
# Environment:
#   SUZATARY_APP_URL  where /api/about answers; default: `app_url` in the
#                     instance file suzatary-web itself reads
#   SUZATARY_DEPLOY_ROOTS_TIMEOUT, SUZATARY_DEPLOY_UNITS_TIMEOUT
#                     seconds to wait for the roots run, which only links
#                     what the deploy built (default 600), and for the
#                     restarted units to run the new app (600); the build
#                     itself has no timeout
#   SUZATARY_DEPLOY_CACHE_TIMEOUT, SUZATARY_DEPLOY_CACHE_APPEAR
#                     seconds to wait for the publication run that builds
#                     the pinned commit and pushes it to the binary cache
#                     (default 1200: past it, building here is quicker), and for such a run to appear at all
#                     (120); past either the deploy builds here
#   SUZATARY_DEPLOY_PUBLICATIONS
#                     the GitHub repository (owner/name) whose publication
#                     runs are asked; default: the one the declared input's
#                     https://github.com URL names, and none for another
#   SUZATARY_DEPLOY_REVERT_TIMEOUT
#                     seconds to wait for the revert's relink (default 600),
#                     its own bound: a failed forward wait must never shorten
#                     the wait that puts the previous app back
#   XDG_RUNTIME_DIR   required: `systemctl --user` answers EMPTY without it
set -euo pipefail

cache_timeout="${SUZATARY_DEPLOY_CACHE_TIMEOUT:-1200}"
cache_appear="${SUZATARY_DEPLOY_CACHE_APPEAR:-120}"
roots_timeout="${SUZATARY_DEPLOY_ROOTS_TIMEOUT:-600}"
units_timeout="${SUZATARY_DEPLOY_UNITS_TIMEOUT:-600}"
revert_timeout="${SUZATARY_DEPLOY_REVERT_TIMEOUT:-600}"

trap 'exit 143' TERM
trap 'exit 129' HUP
trap 'exit 130' INT

die() {
  printf 'deploy: %b\n' "$*" >&2
  exit 1
}
say() { printf 'deploy: %s\n' "$*" >&2; }
usage() {
  echo "usage: scripts/deploy.sh <rev> [--instance <file>] [--flake <file>] [--ref <branch>]   (rev: a full 40-character commit id)" >&2
  exit 64
}

(($#)) && [[ "$1" =~ ^[0-9a-f]{40}$ ]] || usage
rev="$1"
shift
carried="" carried_flake="" ref=""
while (($#)); do
  (($# >= 2)) || usage
  case "$1" in
    --instance) [[ -z "$carried" ]] || usage; carried="$2" ;;
    --flake) [[ -z "$carried_flake" ]] || usage; carried_flake="$2" ;;
    --ref) [[ -z "$ref" && -n "$2" ]] || usage; ref="$2" ;;
    *) usage ;;
  esac
  shift 2
done
# Everything this run writes outside the user repository, removed on every
# exit, signals included (each is an exit, above): the scratch directory and
# the carried file's copy beside the live instance, once it exists.
scratch="$(mktemp -d)"
beside=""
cleanup() { rm -rf "$scratch"; [[ -z "$beside" ]] || rm -f "$beside"; }
trap cleanup EXIT
# A carried file is read ONCE, here: a process substitution or a file that
# moves while the build runs is checked and committed as it read now. The
# flake is read where the new lock is computed, beside it.
if [[ -n "$carried" ]]; then
  cat "$carried" >"$scratch/instance.toml" 2>/dev/null || die "no readable instance file at $carried to carry. Nothing moved."
  carried="$scratch/instance.toml"
fi
if [[ -n "$carried_flake" ]]; then
  cat "$carried_flake" >"$scratch/flake.nix" 2>/dev/null || die "no readable flake at $carried_flake to carry. Nothing moved."
fi
# The app this script ships in, for its reader of the lock and its builds.
here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
app_input="$here/nix/app-input.nix"
lock_read() { nix eval --raw --file "$app_input" --argstr lock "$1" "${@:2}"; }
# What the flake this deploy commits declares: the scratch copy, carried or the user repository's.
# With --ref, on that branch.
declared() { nix eval --raw --file "$app_input" --argstr flake "$scratch/flake.nix" ${ref:+--argstr ref "$ref"} "$@"; }

# --- the units ------------------------------------------------------------

[[ -n "${XDG_RUNTIME_DIR:-}" ]] \
  || die "XDG_RUNTIME_DIR is unset, so systemctl --user would answer empty; export XDG_RUNTIME_DIR=/run/user/$(id -u). Nothing moved."

# What the web unit runs, read off the unit rather than assumed: the user
# repository whose lock it runs, the instance it serves, the roots it links.
unit_env="$(systemctl --user show -p Environment --value suzatary-web.service)" \
  || die "could not read suzatary-web's environment from systemctl --user. Nothing moved."
read -ra pairs <<<"$unit_env"
user="" unit_instance="" roots=""
for pair in "${pairs[@]}"; do
  case "$pair" in
    SUZATARY_USER_REPO=*) user="${pair#*=}" ;;
    SUZATARY_INSTANCE=*) unit_instance="${pair#*=}" ;;
    SUZATARY_ROOTS=*) roots="${pair#*=}" ;;
  esac
done
[[ -n "$user" && -n "$roots" ]] \
  || die "suzatary-web carries no SUZATARY_USER_REPO or SUZATARY_ROOTS: the box runs a module from before the lock deploys; rebuild it first. Nothing moved."

# The long-running units that run the app: each declares itself part of the
# target, so its ConsistsOf is the list.
consists="$(systemctl --user show -p ConsistsOf --value suzatary-app.target)" \
  || die "could not read suzatary-app.target from systemctl --user. Nothing moved."
read -ra app_units <<<"$consists"
((${#app_units[@]})) \
  || die "no unit is part of suzatary-app.target: the box runs a NixOS module from before a deploy restarted every unit that runs the app. The NixOS configuration pins the module by its own suzatary input, apart from the user repository's lock: update that input to this rev or later, then run nixos-rebuild switch. Nothing moved."

# --- the user repository ---------------------------------------------------

[[ -d "$user/.git" ]] || die "no user repository at $user (a clone of its own). Nothing moved."
lock="$user/flake.lock"
[[ -f "$lock" ]] || die "$user has no flake.lock naming the app. Nothing moved."
git_dir="$(git -C "$user" rev-parse --absolute-git-dir)"
exec 8>"$git_dir/suzatary-deploy.lock"
flock --nonblock 8 || die "another deploy is running in $user. Nothing moved."
pending="$(git -C "$user" status --porcelain -- flake.lock flake.nix)"
[[ -z "$pending" ]] \
  || die "$user has uncommitted changes to its flake, which a deploy commit would sweep up. Nothing moved:\n$pending"

[[ -r "$unit_instance" ]] \
  || die "suzatary-web names no readable instance file (${unit_instance:-none}). Nothing moved."
# The instance a carried file replaces is a file the user repository tracks, so
# the deploy commit can hold it and a revert can put it back.
instance_rel=""
if [[ -n "$carried" ]]; then
  instance_rel="$(realpath --relative-to="$(realpath "$user")" "$(realpath "$unit_instance")")"
  [[ "$instance_rel" != ../* ]] && git -C "$user" ls-files --error-unmatch -- "$instance_rel" >/dev/null 2>&1 \
    || die "the instance suzatary-web serves, $unit_instance, is not a file $user tracks, so a deploy cannot carry it. Nothing moved."
  pending="$(git -C "$user" status --porcelain -- "$instance_rel")"
  [[ -z "$pending" ]] \
    || die "$user has uncommitted changes to $instance_rel, which the carried file would overwrite. Nothing moved:\n$pending"
  # The instance as committed now: the build is unbounded, and a settings save
  # the old web writes meanwhile (committed by the next push) would be lost
  # under a carried file made from this one, so the move checks it again.
  instance_blob="$(git -C "$user" rev-parse "HEAD:$instance_rel")"
fi

# --- the rev ----------------------------------------------------------------

# The flake this deploy commits, beside the lock it computes: the carried one,
# or the user repository's as committed.
[[ -n "$carried_flake" ]] || cp "$user/flake.nix" "$scratch/flake.nix"
cp "$user/flake.lock" "$scratch/flake.lock"
flake_blob="$(git -C "$user" rev-parse "HEAD:flake.nix")" \
  || die "$user tracks no flake.nix declaring the app. Nothing moved."

# The commit <rev> names, asked of the declared branch itself, in a mirror in
# the user repository's .git (scripts/followed.sh, the check the roots run
# makes too): Nix's fetch answers from its own cache any rev it has ever
# fetched.
remote="$(declared remote)" || die "could not read the suzatary input the flake declares. Nothing moved."
branch="$(declared branch)" || die "could not read the branch the flake declares. Nothing moved."
mirror="$git_dir/suzatary-followed.git"
"$here/scripts/followed.sh" fetch "$mirror" "$remote" "$branch" \
  || die "could not fetch $branch from $remote. Nothing moved."
# The lookups beside the first: a commit of the chain publishes <rev>, or
# names it as a commit publish passed over. Each reads the branch's whole
# first-parent log with awk, which reads to the end (scripts/followed.sh says
# why).
first_parent() {
  "$here/scripts/followed.sh" check "$mirror" "$remote" "$branch" "$1" 2>/dev/null
}
trailer_of() {
  git -C "$mirror" log --first-parent --format="%H%x09%(trailers:key=$1,valueonly,separator=%x09)" \
    "refs/heads/$branch" | awk -F'\t' -v rev="$2" '{ for (i = 2; i <= NF; i++) if ($i == rev && !n++) print $1 }'
}
if first_parent "$rev"; then
  core="$rev"
else
  core="$(trailer_of Source "$rev")" || die "could not read the log of $branch of $remote. Nothing moved."
  skipped="$(trailer_of Skipped "$rev")" || die "could not read the log of $branch of $remote. Nothing moved."
  [[ -z "$skipped" ]] \
    || die "$rev was skipped by publish, never published (${skipped:0:12} of $branch names it in a Skipped: trailer): its tree did not pass the leak guard, so it cannot go live. Deploy a later rev. Nothing moved."
  [[ -n "$core" ]] \
    || die "$rev is neither a commit of the first-parent chain of $branch of $remote nor a source a commit of it names in a Source: trailer: only a commit the branch took, or a landed commit scripts/train.sh published, goes live (a fix not yet landed: --ref <its branch>; a user repository whose flake.nix still declares another repository: --flake). Nothing moved."
  say "${rev:0:12} is published as ${core:0:12} of $remote"
fi
# The lock interface: a tree without it predates deploys by lock, and its
# scripts would write under their own read-only code root. The roots run
# refuses one too.
git -C "$mirror" cat-file -e "$core:nix/app-input.nix" 2>/dev/null \
  || die "$rev predates deploys by lock (${core:0:12} has no nix/app-input.nix); deploy a later rev. Nothing moved."

# The new lock, computed beside the user repository: locking the repository
# itself would copy its tracked files, the stores among them, into the store.
pin="$(declared --argstr rev "$core" pinned)" || die "could not pin the input the flake declares. Nothing moved."
nix flake lock --override-input suzatary "$pin" "path:$scratch" \
  || die "nix would not pin $pin; see its message above. Nothing moved."

# --- the publication's builds -------------------------------------------------

app="$(lock_read "$scratch/flake.lock" source.outPath)" \
  || die "could not read the app the new lock names. Nothing moved."

# How the publication run of commit <2> in GitHub repository <1> ended, as a word and its
# detail: `success`; `failed <conclusion>`; `none`, when no run appeared within the appear
# bound; `running <status>`, when one was still under way at the end of the whole bound; or
# `unasked <gh's last line>`, when GitHub could not be asked. The newest run of the commit
# counts, so a re-run that passed stands.
publication_run() {
  local polls=0 answer status conclusion
  while :; do
    answer="$(gh run list --repo "$1" --workflow publication.yml --commit "$2" --limit 1 \
      --json status,conclusion --jq '.[] | "\(.status) \(.conclusion)"' 2>&1)" \
      || { echo "unasked ${answer##*$'\n'}"; return 0; }
    read -r status conclusion <<<"$answer"
    case "$status" in
      "") ((polls * 10 < cache_appear)) || { echo none; return 0; } ;;
      completed)
        if [[ "$conclusion" == success ]]; then echo success; else echo "failed $conclusion"; fi
        return 0
        ;;
      *) ((polls * 10 < cache_timeout)) || { echo "running $status"; return 0; } ;;
    esac
    polls=$((polls + 1))
    sleep 10
  done
}

# The builds of a commit of suzatary-core's master are made on GitHub's runners
# (.github/workflows/publication.yml, on each push to it) and pushed to the binary cache this
# box substitutes from. So unless the store holds them already (the merge gate's deploy-build
# made them), the deploy waits for that run, bounded: its success means the build below
# downloads them; any other ending means it compiles them here, and says which.
publications="${SUZATARY_DEPLOY_PUBLICATIONS:-}"
if [[ -z "$publications" && "$remote" =~ ^https://github\.com/([^/]+)/([^/]+)$ ]]; then
  publications="${BASH_REMATCH[1]}/${BASH_REMATCH[2]%.git}"
fi
if nix path-info "$app#suzatary" "$app#suzatary-web" "$app#suzatary-plugins" >/dev/null 2>&1; then
  say "the store holds the builds of ${rev:0:12} already"
elif [[ "$branch" != master ]]; then
  say "${core:0:12} is a commit of $branch, and publication runs build master alone: building it here"
elif [[ -z "$publications" ]]; then
  say "$remote is not on GitHub, so no publication run builds ${core:0:12}: building it here"
else
  say "waiting for the publication run of ${core:0:12} in $publications (at most ${cache_timeout}s)"
  run="$(publication_run "$publications" "$core")"
  case "$run" in
    success) say "the publication run of ${core:0:12} succeeded: its builds come from the binary cache" ;;
    failed*) say "the publication run of ${core:0:12} ended ${run#failed }: building it here" ;;
    none) say "no publication run of ${core:0:12} appeared within ${cache_appear}s: building it here" ;;
    running*) say "the publication run of ${core:0:12} was still ${run#running } after ${cache_timeout}s: building it here" ;;
    unasked*) say "could not ask GitHub for the publication run of ${core:0:12} (${run#unasked }): building it here" ;;
  esac
fi

# --- the build --------------------------------------------------------------

# No timeout: the units serve the app they ran until the lock moves.
say "building ${rev:0:12} ($app) before moving the lock"
"$here/scripts/app-builds.sh" "$app" "$roots/deploy-" \
  || die "the build of ${rev:0:12} failed; see nix's message above. Nothing moved."

# The new binary reads the LIVE instance (every plugin manifest's world
# included) and fscks the stores of its chains and its secretary, and writes
# nothing (`verify`, the push path's fsck), from the code root its units
# would have: an instance or store the new app refuses would crash-loop every
# unit the moment the web restarted. It loads no plugin component and reads
# no plugin store, which is cache.
# A carried file is checked where it will stand, beside the live one, so the paths
# it names relative to its own directory resolve alike.
checked="$unit_instance"
if [[ -n "$carried" ]]; then
  beside="$(dirname "$unit_instance")/.deploy-instance-$$.toml"
  cp "$carried" "$beside"
  checked="$beside"
fi
status=0
said="$(SUZATARY_REPO="$app" "$roots/deploy-bin/bin/suzatary" --instance "$checked" verify 2>&1)" || status=$?
[[ -z "$beside" ]] || { rm -f "$beside"; beside=""; }
((status == 0)) \
  || die "${rev:0:12} refuses the ${carried:+carried }instance ${carried:-$unit_instance} or one of the stores it names, in its own words:\n$said\nNothing moved. A refusal of the instance (a section or plugin world ${rev:0:12} does not read) means the instance must change in the same deploy: write what ${rev:0:12} reads and pass it with --instance <file>. A store that is not in the form ${rev:0:12} reads needs its migration first; a store finding needs clearing first."
say "${rev:0:12} reads ${carried:-$unit_instance} and the stores it names"

# Where /api/about answers, read by the binary just built (the deployed one may not have the
# verb yet), from the instance the web serves.
if [[ -n "${SUZATARY_APP_URL:-}" ]]; then
  url="$SUZATARY_APP_URL"
else
  url="$(SUZATARY_REPO="$app" "$roots/deploy-bin/bin/suzatary" --instance "$unit_instance" \
    instance get web.app_url)" || url=""
  [[ -n "$url" ]] || die "no app_url in $unit_instance; set SUZATARY_APP_URL. Nothing moved."
fi
about="${url%/}/api/about"

# The roots unit's state as three variables, from one `systemctl show`.
# Microseconds, because a run that began earlier in the same second as the
# commit read the OLD lock.
active="" result="" started_us=0 started=""
read_roots() {
  local out line
  out="$(systemctl --user show suzatary-roots.service --timestamp=us+utc \
    -p LoadState -p ActiveState -p Result -p ExecMainStartTimestamp)" \
    || die "could not read suzatary-roots.service from systemctl --user."
  active="" result="" started=""
  while IFS= read -r line; do
    case "$line" in
      LoadState=*) [[ "${line#*=}" == loaded ]] || die "suzatary-roots.service is ${line#*=}, not loaded." ;;
      ActiveState=*) active="${line#*=}" ;;
      Result=*) result="${line#*=}" ;;
      ExecMainStartTimestamp=*) started="${line#*=}" ;;
    esac
  done <<<"$out"
  case "$started" in
    "" | n/a) started_us=0 ;;
    *)
      started_us="$(date -d "$started" +%s%6N)" \
        || die "could not read suzatary-roots' start time \"$started\"."
      ;;
  esac
}
busy() { [[ "$active" == activating || "$active" == deactivating || "$active" == reloading ]]; }
# A run under way is let finish: a start while the unit is active is
# coalesced into a run that read some other lock.
idle() {
  local deadline=$((SECONDS + roots_timeout))
  read_roots
  while busy; do
    ((SECONDS < deadline)) || die "a suzatary-roots run was still under way after ${roots_timeout}s ($1)."
    say "a suzatary-roots run is under way; waiting for it ($1)"
    sleep 5
    read_roots
  done
}

# Wait for a run that started at or after an epoch to finish; false when none
# did within the timeout. `result` is then that run's.
await_run() {
  local deadline=$((SECONDS + roots_timeout))
  while :; do
    read_roots
    if ((started_us >= $1)) && ! busy; then
      return 0
    fi
    ((SECONDS < deadline)) || return 1
    sleep 5
  done
}

# The names of the builds rooted under a prefix (app-builds.sh's outputs); a
# profile's generation links are its profile's.
builds() {
  local link
  for link in "$roots/$1"*; do
    [[ -e "$link" || -L "$link" ]] || continue
    [[ "$link" == *-link ]] || printf '%s\n' "${link#"$roots/$1"}"
  done
}

# Pushed last, under the push lock, whatever was committed.
record() {
  flock "$git_dir/suzatary-push.lock" git -C "$user" push --quiet \
    || say "the user repository would not push; run: git -C $user push"
}

previous_rev="$(lock_read "$lock" revision)" || die "could not read the rev $lock names. Nothing moved."
previous_ref="$(lock_read "$lock" lockedRef)" || die "could not read the branch $lock pinned. Nothing moved."
# The way back names the previous commit as the lock pinned it, with its branch when that is not
# the one this deploy pinned from.
[[ "$previous_ref" == "$branch" ]] || previous_rev="$previous_rev --ref $previous_ref"
idle "before moving the lock"
for name in $(builds lock-); do
  nix-store --add-root "$roots/previous-$name" --indirect -r "$(readlink -f "$roots/lock-$name")" >/dev/null \
    || die "could not root the previous app's $name. Nothing moved."
done

# --- the move -------------------------------------------------------------

deployed=""
# What the deploy commit writes, each from where: the lock, and each carried
# file that differs from the one it replaces, with the blob that one was when
# this deploy read it.
moves=(flake.lock) froms=("$scratch/flake.lock") befores=("")
instance_moved=""
if [[ -n "$carried" ]] && ! cmp -s "$carried" "$unit_instance"; then
  moves+=("$instance_rel") froms+=("$carried") befores+=("$instance_blob")
  instance_moved=yes
fi
if ! cmp -s "$scratch/flake.nix" "$user/flake.nix"; then
  moves+=(flake.nix) froms+=("$scratch/flake.nix") befores+=("$flake_blob")
fi
# Commits `moves` as the blobs `blobs` names through an index of its own, its
# arguments `git commit`'s message options. The working tree is not written:
# the caller checks the commit out, so a file the path unit watches never holds
# a state no commit holds.
commit_moves() {
  local dir i ok=""
  dir="$(mktemp -d)" || return 1
  if GIT_INDEX_FILE="$dir/index" git -C "$user" read-tree HEAD; then
    ok=yes
    for i in "${!moves[@]}"; do
      GIT_INDEX_FILE="$dir/index" git -C "$user" update-index --add \
        --cacheinfo "100644,${blobs[i]},${moves[i]}" || ok=""
    done
    [[ -z "$ok" ]] || GIT_INDEX_FILE="$dir/index" git -C "$user" commit --quiet "$@" || ok=""
  fi
  rm -rf "$dir"
  [[ -n "$ok" ]]
}
# The roots run builds only what a deploy here pinned, from the branch it now
# follows, or the pin before (its revert's): recorded as the last step before
# the lock moves, so the run the commit starts admits it and every refusal
# above it moved nothing (scripts/followed.sh).
record_pin() {
  "$here/scripts/followed.sh" pin "$mirror" "$core" "$branch" \
    || die "could not record the pin of ${core:0:12} in $mirror. Nothing moved."
}
if [[ "$(cat "$scratch/flake.lock")" == "$(cat "$lock")" && "${#moves[@]}" -eq 1 ]]; then
  say "the lock already names ${rev:0:12}: deploying it again"
  record_pin
else
  exec 9>"$git_dir/suzatary-push.lock"
  flock 9
  for i in "${!moves[@]}"; do
    [[ -n "${befores[i]}" ]] || continue
    now_blob="$(git -C "$user" rev-parse "HEAD:${moves[i]}")"
    pending="$(git -C "$user" status --porcelain -- "${moves[i]}")"
    [[ "$now_blob" == "${befores[i]}" && -z "$pending" ]] \
      || die "${moves[i]} moved while ${rev:0:12} built, and the carried file was made from the one before it; carrying it would lose this change:\n$(git -C "$user" diff "${befores[i]}" -- "${moves[i]}")\nNothing moved. Make the carried file again from the one as it stands, and deploy again."
  done
  blobs=()
  for i in "${!moves[@]}"; do
    blob="$(git -C "$user" hash-object -w -- "${froms[i]}")" \
      || die "could not store ${moves[i]} in $user. Nothing moved."
    blobs+=("$blob")
  done
  record_pin
  commit_moves -m "Deploy $rev" \
    || die "the deploy of ${rev:0:12} would not commit; see git's message above. Nothing moved but the pin of ${rev:0:12}, which the roots run admits only once a lock names it."
  deployed="$(git -C "$user" rev-parse HEAD)"
  git -C "$user" checkout HEAD -- "${moves[@]}" \
    || die "committed Deploy ${rev:0:12} but could not check ${moves[*]} out into $user."
  flock --unlock 9
  say "committed Deploy ${rev:0:12} in $user"
fi

# Until the units run the new app, a failure undoes the deploy in a commit of
# its own, so the log never names a rev that did not run; and roots/app is
# made the previous app's again by a run of its own, since a run still
# building the new rev would link it when it ends.
live=""
# How to go back once the units run the new app: a deploy of the previous rev,
# carrying the instance the previous rev read when this deploy carried one,
# since the old binary refuses the new file. A rev that migrated a store, and
# whose app has started on it, is fixed forward: the old app writes the old
# store, which nothing carries again.
go_back() {
  if [[ -n "$instance_moved" && -n "$deployed" ]]; then
    printf 'to go back, run: git -C %s show %s^:%s >/tmp/instance-before.toml && scripts/deploy.sh %s --instance /tmp/instance-before.toml (unless this rev migrated a store: then fix forward)' \
      "$user" "${deployed:0:12}" "$instance_rel" "$previous_rev"
  else
    printf 'to go back, run: scripts/deploy.sh %s (unless this rev migrated a store: then fix forward)' "$previous_rev"
  fi
}
undo() {
  local status="$1"
  [[ "$status" -ne 0 ]] || return 0
  # A second signal must not cut the record short.
  trap '' TERM HUP INT
  # Once the units restarted, they run the new app, so the log keeps it.
  if [[ -n "$live" ]]; then
    [[ -z "$deployed" ]] || record
    say "${restarted[*]:-no unit} restarted onto ${rev:0:12}, but the deploy stopped before it was checked; re-run scripts/deploy.sh $rev to finish the about-check, or $(go_back)"
    return 0
  fi
  [[ -n "$deployed" ]] || return 0
  systemctl --user stop suzatary-roots.service \
    || say "could not stop suzatary-roots; a run of ${rev:0:12} may still link it"
  exec 9>"$git_dir/suzatary-push.lock"
  flock 9 || say "could not take the push lock; reverting without it"
  reverted=yes
  blobs=()
  for moved in "${moves[@]}"; do
    blob="$(git -C "$user" rev-parse "$deployed^:$moved")" || reverted=""
    blobs+=("$blob")
  done
  if [[ -n "$reverted" ]] \
    && commit_moves -m "Revert \"Deploy $rev\"" \
      -m "The units never ran it: the deploy stopped before restarting them."; then
    git -C "$user" checkout HEAD -- "${moves[@]}" \
      || say "could not check ${moves[*]} out into $user; they are the revert's in HEAD"
    say "reverted the deploy of ${rev:0:12} in a new commit"
  else
    say "the revert would not commit; see git's message above"
  fi
  flock --unlock 9 || true
  # The relink is waited for in a subshell, so a wait that dies there says its
  # sentence and still leaves the revert to be pushed: the record is the one
  # thing a failed deploy must not lose.
  if (
    roots_timeout="$revert_timeout"
    previous="$(lock_read "$lock" source.outPath)"
    epoch="$(date +%s%6N)"
    idle "the revert's run"
    systemctl --user start --no-block suzatary-roots.service || true
    await_run "$epoch" || die "no suzatary-roots run started after the revert finished within ${roots_timeout}s."
    [[ "$result" == success && "$(readlink -f "$roots/app")" == "$previous" ]] \
      || die "the revert's run ended Result=$result and roots/app is $(readlink -f "$roots/app"), not $previous."
  ); then
    say "roots/app is the previous app again, and every unit still runs it"
  else
    say "no run relinked the previous app; see journalctl --user -u suzatary-roots"
  fi
  record
}
trap 'status=$?; cleanup; undo "$status" || true' EXIT

# The epoch is the commit: a run that started before it read another lock.
epoch_us="$(date +%s%6N)"
idle "a run the write of the lock started"
systemctl --user start --no-block suzatary-roots.service \
  || die "the lock names ${rev:0:12} but suzatary-roots would not start; no unit was restarted."

# --- the roots run this move caused ---------------------------------------

say "waiting for a suzatary-roots run started after the move to finish"
await_run "$epoch_us" \
  || die "no suzatary-roots run started after the move finished within ${roots_timeout}s; no unit was restarted."
[[ "$result" == success ]] \
  || die "the suzatary-roots run started $started ended Result=$result; no unit was restarted. See journalctl --user -u suzatary-roots."
[[ "$(readlink -f "$roots/app")" == "$app" ]] \
  || die "the roots run succeeded but links $(readlink -f "$roots/app"), not $app, the app the lock names; no unit was restarted."
# Each build is a function of the tree, so the run's builds are the deploy's:
# the same set of outputs (the run uses the system module's copy of
# app-builds.sh, which may list one this copy does not), and the same path
# for each. A difference is a finding, never a deploy.
[[ "$(builds lock-)" == "$(builds deploy-)" ]] \
  || die "the roots run built $(builds lock- | paste -sd ' '), the deploy $(builds deploy- | paste -sd ' '): an output only one side builds is built cold; no unit was restarted."
for name in $(builds deploy-); do
  [[ "$(readlink -f "$roots/lock-$name")" == "$(readlink -f "$roots/deploy-$name")" ]] \
    || die "the roots run linked lock-$name to $(readlink -f "$roots/lock-$name"), not $(readlink -f "$roots/deploy-$name"), which the deploy built; no unit was restarted."
done
rm -f "$roots"/deploy-*
say "suzatary-roots run started $started linked $app and exactly the builds the deploy made"

# --- the units that run the app ---------------------------------------------

# try-restart, so a unit that stopped between the read and the restart stays
# down too. Every running unit is tried before a refusal fails the deploy, so
# one unit's refusal never leaves the others on the old app, and the record
# names which restarted.
restarted=() refused=()
live=yes
for unit in "${app_units[@]}"; do
  state="$(systemctl --user show -p ActiveState --value "$unit")" \
    || die "could not read $unit from systemctl --user."
  case "$state" in
    active | activating | reloading) ;;
    *)
      say "left $unit $state: it reads the new app when it next starts"
      continue
      ;;
  esac
  if systemctl --user try-restart "$unit"; then
    restarted+=("$unit")
    say "restarted $unit"
  else
    refused+=("$unit")
  fi
done
((${#refused[@]} == 0)) \
  || die "${rev:0:12} is built and linked, but ${refused[*]} would not restart, so it does not run ${rev:0:12}; see journalctl --user -u ${refused[0]}."
say "waiting for $about"
deadline=$((SECONDS + units_timeout))
code=000
while :; do
  code="$(curl --silent --output /dev/null --write-out '%{http_code}' --max-time 5 "$about")" || true
  [[ "$code" == 200 ]] && break
  ((SECONDS < deadline)) \
    || die "$about did not answer 200 within ${units_timeout}s after the restart (last: $code); see journalctl --user -u suzatary-web; $(go_back)."
  sleep 3
done

# Each unit's wrapper roots the app it runs as <unit>-app before it runs it
# (nix/services.nix `resolveApp`), so that root is the app each restarted unit
# runs.
for unit in "${restarted[@]}"; do
  until [[ "$(readlink -f "$roots/${unit%.service}-app" || true)" == "$app" ]]; do
    ((SECONDS < deadline)) \
      || die "$unit restarted but runs $(readlink -f "$roots/${unit%.service}-app" || echo nothing), not $app, the app the lock names; see journalctl --user -u $unit."
    sleep 3
  done
done

# serve-web.sh's roots of the binary and the bundle it execs and serves are
# the roots run's builds.
for pair in app-bin-serving:lock-bin web-dist-serving:lock-dist; do
  link="${pair%%:*}" built="${pair#*:}"
  [[ "$(readlink -f "$roots/$link")" == "$(readlink -f "$roots/$built")" ]] \
    || die "suzatary-web answers but serves $(readlink -f "$roots/$link"), not $(readlink -f "$roots/$built"), the roots run's build."
done

# --- the record -----------------------------------------------------------

record
rm -f "$roots"/previous-*
say "${rev:0:12} is live; $about answers 200 from the build the lock names"
