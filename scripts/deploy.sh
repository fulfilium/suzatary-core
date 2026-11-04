#!/usr/bin/env bash
# A deploy is a commit to the user repository moving its flake.lock, which the units run. This
# script is the one thing that moves it.
#
#   scripts/deploy.sh <rev>                     pin the app at <rev> (full id: a first-parent commit of the input's branch, or the one whose `Source:` trailer names it)
#   scripts/deploy.sh <rev> --ref <branch>      the same on another branch of the input; the pin carries ?ref=<branch>
#   scripts/deploy.sh <rev> --instance <file>   also commit <file> over the served instance, so config and code land and revert as one
#   scripts/deploy.sh <rev> --flake <file>      also commit <file> as the user repository's flake.nix; <rev> resolves in its input
#
# Only the branch's first-parent chain went through a gate. A carried instance is read once; the
# move refuses if the live instance was committed again while the build ran.
#
# It builds first (scripts/app-builds.sh, rooted under `deploy-`), waiting bounded for the
# publication run to fill the binary cache. A failed build, or a live instance the new binary
# refuses to `verify`, moves nothing. The previous app stays rooted under `previous-` until success.
#
# The lock is committed through the deploy's own index under safe-push's push lock, then checked
# out, so suzatary-roots.path only ever sees a committed state. If the units do not move to the
# new app, a `Revert "Deploy <rev>"` commit undoes it. Both commits are pushed last.
#
# TERM, HUP and INT exit non-zero, so the deploy is reverted or recorded as any failure. After a
# SIGKILL, `git -C <user repository> log -1` says where it stopped.
#
# Wait on the roots run's start time, not `Result` alone: a `Result=success` can be left over
# from the previous run. Only a run started at or after the commit, finished with success and
# linking the locked app, lets the units restart.
#
# It try-restarts every unit in suzatary-app.target's ConsistsOf; a stopped one stays down. A
# timer's oneshot reads the lock at each start.
#
# Environment (seconds unless named):
#   SUZATARY_APP_URL                 where /api/about answers; default: `app_url` in suzatary-web's instance
#   SUZATARY_DEPLOY_ROOTS_TIMEOUT    the roots run (600); the build itself has no timeout
#   SUZATARY_DEPLOY_UNITS_TIMEOUT    the restarted units to run the new app (600)
#   SUZATARY_DEPLOY_CACHE_TIMEOUT    the publication run filling the binary cache (1200)
#   SUZATARY_DEPLOY_CACHE_APPEAR     such a run to appear (120); past either it builds here
#   SUZATARY_DEPLOY_CACHE_POLL       between two questions about that run (10)
#   SUZATARY_DEPLOY_PUBLICATIONS     owner/name whose publication runs are asked; default: the input's github.com URL
#   SUZATARY_DEPLOY_REVERT_TIMEOUT   the revert's relink (600), its own bound so a failed forward wait never shortens it
#   XDG_RUNTIME_DIR                  required: `systemctl --user` answers EMPTY without it
set -euo pipefail

cache_timeout="${SUZATARY_DEPLOY_CACHE_TIMEOUT:-1200}"
cache_appear="${SUZATARY_DEPLOY_CACHE_APPEAR:-120}"
cache_poll="${SUZATARY_DEPLOY_CACHE_POLL:-10}"
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
# Removed on every exit, signals included.
scratch="$(mktemp -d)"
beside=""
cleanup() { rm -rf "$scratch"; [[ -z "$beside" ]] || rm -f "$beside"; }
trap cleanup EXIT
# A carried file is read once, here: what is checked is what is committed.
if [[ -n "$carried" ]]; then
  cat "$carried" >"$scratch/instance.toml" 2>/dev/null || die "no readable instance file at $carried to carry. Nothing moved."
  carried="$scratch/instance.toml"
fi
if [[ -n "$carried_flake" ]]; then
  cat "$carried_flake" >"$scratch/flake.nix" 2>/dev/null || die "no readable flake at $carried_flake to carry. Nothing moved."
fi
here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
app_input="$here/nix/app-input.nix"
lock_read() { nix eval --raw --file "$app_input" --argstr lock "$1" "${@:2}"; }
declared() { nix eval --raw --file "$app_input" --argstr flake "$scratch/flake.nix" ${ref:+--argstr ref "$ref"} "$@"; }

[[ -n "${XDG_RUNTIME_DIR:-}" ]] \
  || die "XDG_RUNTIME_DIR is unset, so systemctl --user would answer empty; export XDG_RUNTIME_DIR=/run/user/$(id -u). Nothing moved."

# Read off the unit rather than assumed.
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

consists="$(systemctl --user show -p ConsistsOf --value suzatary-app.target)" \
  || die "could not read suzatary-app.target from systemctl --user. Nothing moved."
read -ra app_units <<<"$consists"
((${#app_units[@]})) \
  || die "no unit is part of suzatary-app.target: the box runs a NixOS module from before a deploy restarted every unit that runs the app. The NixOS configuration pins the module by its own suzatary input, apart from the user repository's lock: update that input to this rev or later, then run nixos-rebuild switch. Nothing moved."

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
# Must be tracked, so the deploy commit can hold it and a revert can put it back.
instance_rel=""
if [[ -n "$carried" ]]; then
  instance_rel="$(realpath --relative-to="$(realpath "$user")" "$(realpath "$unit_instance")")"
  [[ "$instance_rel" != ../* ]] && git -C "$user" ls-files --error-unmatch -- "$instance_rel" >/dev/null 2>&1 \
    || die "the instance suzatary-web serves, $unit_instance, is not a file $user tracks, so a deploy cannot carry it. Nothing moved."
  pending="$(git -C "$user" status --porcelain -- "$instance_rel")"
  [[ -z "$pending" ]] \
    || die "$user has uncommitted changes to $instance_rel, which the carried file would overwrite. Nothing moved:\n$pending"
  # The old web may save settings into it during the build; the move checks this blob again.
  instance_blob="$(git -C "$user" rev-parse "HEAD:$instance_rel")"
fi

[[ -n "$carried_flake" ]] || cp "$user/flake.nix" "$scratch/flake.nix"
cp "$user/flake.lock" "$scratch/flake.lock"
flake_blob="$(git -C "$user" rev-parse "HEAD:flake.nix")" \
  || die "$user tracks no flake.nix declaring the app. Nothing moved."

# Asked of the branch itself in a mirror: Nix's fetch answers from its own cache any rev it has
# ever fetched.
remote="$(declared remote)" || die "could not read the suzatary input the flake declares. Nothing moved."
branch="$(declared branch)" || die "could not read the branch the flake declares. Nothing moved."
mirror="$git_dir/suzatary-followed.git"
"$here/scripts/followed.sh" fetch "$mirror" "$remote" "$branch" \
  || die "could not fetch $branch from $remote. Nothing moved."
# awk reads the whole log to the end (scripts/followed.sh says why).
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
# A tree without it predates deploys by lock; its scripts would write under a read-only code root.
git -C "$mirror" cat-file -e "$core:nix/app-input.nix" 2>/dev/null \
  || die "$rev predates deploys by lock (${core:0:12} has no nix/app-input.nix); deploy a later rev. Nothing moved."

# Computed beside the user repository: locking it in place would copy its tracked files, the
# stores among them, into the Nix store.
pin="$(declared --argstr rev "$core" pinned)" || die "could not pin the input the flake declares. Nothing moved."
nix flake lock --override-input suzatary "$pin" "path:$scratch" \
  || die "nix would not pin $pin; see its message above. Nothing moved."

app="$(lock_read "$scratch/flake.lock" source.outPath)" \
  || die "could not read the app the new lock names. Nothing moved."

# .github/workflows/publication.yml pushes master's builds to the binary cache this box
# substitutes from.
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
  run="$("$here/scripts/publication-run.sh" gh "$publications" push "$core" "$cache_appear" "$cache_timeout" "$cache_poll")"
  case "$run" in
    success) say "the publication run of ${core:0:12} succeeded: its builds come from the binary cache" ;;
    failed*) say "the publication run of ${core:0:12} ended ${run#failed }: building it here" ;;
    none) say "no publication run of ${core:0:12} appeared within ${cache_appear}s: building it here" ;;
    running*) say "the publication run of ${core:0:12} was still ${run#running } after ${cache_timeout}s: building it here" ;;
    unasked) say "could not ask GitHub for the publication run of ${core:0:12} (gh's words above): building it here" ;;
  esac
fi

# No timeout: the units serve the app they ran until the lock moves.
say "building ${rev:0:12} ($app) before moving the lock"
"$here/scripts/app-builds.sh" "$app" "$roots/deploy-" \
  || die "the build of ${rev:0:12} failed; see nix's message above. Nothing moved."

# `verify` writes nothing. An instance or store the new app refuses would crash-loop every unit
# once the web restarted. A carried file is checked beside the live one, so its relative paths
# resolve alike.
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

# Read by the binary just built: the deployed one may not have the verb yet.
if [[ -n "${SUZATARY_APP_URL:-}" ]]; then
  url="$SUZATARY_APP_URL"
else
  url="$(SUZATARY_REPO="$app" "$roots/deploy-bin/bin/suzatary" --instance "$unit_instance" \
    instance get web.app_url)" || url=""
  [[ -n "$url" ]] || die "no app_url in $unit_instance; set SUZATARY_APP_URL. Nothing moved."
fi
about="${url%/}/api/about"

# Microseconds, because a run that began earlier in the same second as the commit read the OLD lock.
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
# A start while the unit is active is coalesced into a run that read some other lock.
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

# await_run <epoch-us>: false when no run started at or after it finished in time.
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

# A profile's generation links (`*-link`) are its profile's, not builds.
builds() {
  local link
  for link in "$roots/$1"*; do
    [[ -e "$link" || -L "$link" ]] || continue
    [[ "$link" == *-link ]] || printf '%s\n' "${link#"$roots/$1"}"
  done
}

record() {
  flock "$git_dir/suzatary-push.lock" git -C "$user" push --quiet \
    || say "the user repository would not push; run: git -C $user push"
}

previous_rev="$(lock_read "$lock" revision)" || die "could not read the rev $lock names. Nothing moved."
previous_ref="$(lock_read "$lock" lockedRef)" || die "could not read the branch $lock pinned. Nothing moved."
[[ "$previous_ref" == "$branch" ]] || previous_rev="$previous_rev --ref $previous_ref"
idle "before moving the lock"
for name in $(builds lock-); do
  nix-store --add-root "$roots/previous-$name" --indirect -r "$(readlink -f "$roots/lock-$name")" >/dev/null \
    || die "could not root the previous app's $name. Nothing moved."
done

deployed=""
# Each path, its source, and the blob it was when read (to refuse a lost change).
moves=(flake.lock) froms=("$scratch/flake.lock") befores=("")
instance_moved=""
if [[ -n "$carried" ]] && ! cmp -s "$carried" "$unit_instance"; then
  moves+=("$instance_rel") froms+=("$carried") befores+=("$instance_blob")
  instance_moved=yes
fi
if ! cmp -s "$scratch/flake.nix" "$user/flake.nix"; then
  moves+=(flake.nix) froms+=("$scratch/flake.nix") befores+=("$flake_blob")
fi
# Through an index of its own; the working tree is not written, so a file the path unit watches
# never holds a state no commit holds. Arguments are `git commit`'s message options.
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
# The roots run builds only a recorded pin (scripts/followed.sh). Recorded last before the lock
# moves, so every refusal above moved nothing.
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

# Until the units run the new app, a failure reverts in a commit of its own, and the roots run
# is stopped, since a run still building the new rev would link it when it ends.
live=""
# Carries the previous instance when this deploy carried one: the old binary refuses the new
# file. A rev that migrated a store is fixed forward.
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
  # In a subshell, so a wait that dies still leaves the revert to be pushed: the record is the
  # one thing a failed deploy must not lose.
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

say "waiting for a suzatary-roots run started after the move to finish"
await_run "$epoch_us" \
  || die "no suzatary-roots run started after the move finished within ${roots_timeout}s; no unit was restarted."
[[ "$result" == success ]] \
  || die "the suzatary-roots run started $started ended Result=$result; no unit was restarted. See journalctl --user -u suzatary-roots."
[[ "$(readlink -f "$roots/app")" == "$app" ]] \
  || die "the roots run succeeded but links $(readlink -f "$roots/app"), not $app, the app the lock names; no unit was restarted."
# The run uses the system module's copy of app-builds.sh, which may list another set. A
# difference is a finding, never a deploy.
[[ "$(builds lock-)" == "$(builds deploy-)" ]] \
  || die "the roots run built $(builds lock- | paste -sd ' '), the deploy $(builds deploy- | paste -sd ' '): an output only one side builds is built cold; no unit was restarted."
for name in $(builds deploy-); do
  [[ "$(readlink -f "$roots/lock-$name")" == "$(readlink -f "$roots/deploy-$name")" ]] \
    || die "the roots run linked lock-$name to $(readlink -f "$roots/lock-$name"), not $(readlink -f "$roots/deploy-$name"), which the deploy built; no unit was restarted."
done
rm -f "$roots"/deploy-*
say "suzatary-roots run started $started linked $app and exactly the builds the deploy made"

# Every running unit is tried before a refusal fails the deploy, so one refusal never leaves the
# others on the old app.
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

# Each unit's wrapper roots the app it runs as <unit>-app (nix/services.nix `resolveApp`).
for unit in "${restarted[@]}"; do
  until [[ "$(readlink -f "$roots/${unit%.service}-app" || true)" == "$app" ]]; do
    ((SECONDS < deadline)) \
      || die "$unit restarted but runs $(readlink -f "$roots/${unit%.service}-app" || echo nothing), not $app, the app the lock names; see journalctl --user -u $unit."
    sleep 3
  done
done

for pair in app-bin-serving:lock-bin web-dist-serving:lock-dist; do
  link="${pair%%:*}" built="${pair#*:}"
  [[ "$(readlink -f "$roots/$link")" == "$(readlink -f "$roots/$built")" ]] \
    || die "suzatary-web answers but serves $(readlink -f "$roots/$link"), not $(readlink -f "$roots/$built"), the roots run's build."
done

record
rm -f "$roots"/previous-*
say "${rev:0:12} is live; $about answers 200 from the build the lock names"
