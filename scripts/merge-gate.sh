#!/usr/bin/env bash
# THE MERGE GATE, AS ONE COMMAND: what a branch passes before it reaches
# master. Every step runs, even after a failure, so one run names every
# problem; each step's exit code and time is printed, the failing steps' last
# lines are repeated at the end, and the gate fails if any step failed.
#
# Each step is in a LANE: a lane's steps run one after another, and a half's
# lanes side by side (the Rust's checks; its tests, the plugin fences among
# them; the web's modules, tests and bundle; the public tree's, the web's and
# the module's Nix builds), since no lane reads what another writes. Cargo's lock on the
# target orders the lanes' cargo commands; the rest overlaps, but a step declared `heavy`
# (below) waits until no other heavy step runs, so two builds of the workspace or the app
# never share the machine's memory. The `quiet`
# lane runs after the others, alone: the browser suite, whose waits are
# budgets judged on an idle machine, lost the order of several scenes under
# the other lanes' load, and a verdict that depends on load is not a
# function of the tree.
#
# A gate that is remembered is a gate that is sometimes shorter, so the
# steps live here.
#
# THE GATE HAS TWO HALVES, and each step is in exactly one:
#   tree  a function of the checked-out tree ALONE. Its steps run with the
#         owner's data tree hidden (SUZATARY_INSTANCE names nothing), so the
#         tests that read it skip, and the half reads the same on the box and
#         on a machine that never held the data (a Codespace,
#         scripts/train.sh gate --remote).
#   live  the tests that read the owner's data tree, run against it, so on
#         the box only: `--test live` (SPEC §9 on this box's chain), `--test
#         guards` (personal terms, the agent's staging, the heartbeat's
#         instance half), the `all` modules in `live_modules` below,
#         scripts/web-test.sh (wasm.test.ts's live-chain tests), the
#         triage push as a dry run over the instance's data repository, and
#         HEAD's public tree checked whole against the data repository's lists;
#         and, beside them, the deploy's builds of HEAD's public tree for the
#         box's own system, which a deploy of the landed commit then reuses.
#         app/cli/tests/guards/merge_gate.rs fails when a test that reads the
#         data tree is outside this half.
#
# A PASS IS KEPT BY WHAT THE STEP READ. A step's verdict is a function of the
# tree, so it is a function of the part of the tree it reads: each step is
# declared with the paths it never reads (`all-but:`), the whole tree by
# default, and a pass is kept in .git/gate/<step>/<name>, <name> hashing the
# entries (mode, blob, path) of HEAD's tree outside those paths, the
# environment the steps read (the shell's PATH, cargo's, rust's, node's, the
# time zone and locale, every SUZATARY_ variable) and the one ignored input,
# proptest's saved counterexamples, by content. The date itself is not in
# it: a step whose verdict turns on the day it runs is the residual. No step may leave out scripts/, flake.nix or
# flake.lock, so the key names the gate, the shell and the step's own
# command. Passes are read and kept only for a CLEAN checkout, its working
# tree HEAD's, untracked files included, and kept only when it is still
# HEAD's when the gate ends: a dirty gate runs every step and keeps nothing,
# since the guards read HEAD and the index, the other steps the files. A
# step whose inputs have not moved since it passed is not run again; a
# failure is the absence of a pass, so a re-gate runs it; deleting the table
# costs time and changes no verdict. Steps declared `together` (one writes
# what another reads outside the tree: web/dist, web/src/wire/generated)
# run together: when one runs, every partner runs, but a step split into
# units runs only its units whose inputs moved, since a kept unit reads
# nothing. Most of the live half reads the data tree, which no name covers,
# and the resolvers answer a store path, a value and not a verdict: those
# steps are `unkept`, run every time. The live pins are kept by the code
# that reads the data (`live_reads`, below), and the web's live run by the
# web's own reads, so a docs or release commit runs neither.
#
#   nix develop .#triage -c scripts/merge-gate.sh               both halves
#   nix develop .#triage -c scripts/merge-gate.sh --no-live     the tree half
#   nix develop .#triage -c scripts/merge-gate.sh --only-live   the live half
#   nix develop .#triage -c scripts/merge-gate.sh --full ...    every step and every unit, no pass read
#   nix develop .#triage -c scripts/merge-gate.sh --no-live --steps fmt,doc   those steps alone
#   scripts/merge-gate.sh --list                                every step: half, lane, reads
#
# --steps <name>[,<name>...] runs only the named steps of the halves chosen, each in its lane as
# a whole gate would run it: the caller names a step's writers beside it (e2e reads the bundle
# build-web writes, from what web-inputs prepares). scripts/train.sh's tree half on GitHub's
# runners is the gate split this way, some steps on the box and the rest in
# .github/workflows/gate.yml's jobs, and a name that is no step of those halves is refused (64).
#
# A STEP MAY BE SPLIT INTO UNITS, each kept by what it reads: `crates`, the workspace's members,
# each reading its own directory and those of the members it depends on (cargo metadata); and
# `files:<dir>`, the test files under <dir>, each reading what its imports reach (esbuild's
# metafile) and the paths the step names as shared. The step runs only the units whose inputs
# moved, named in SUZATARY_GATE_UNITS; where the graph cannot be read, the step runs whole, as
# one unit keyed by the step's own reads. A step's reads may also be `only:<path>...`: the named
# paths, with scripts/, flake.nix and flake.lock, which every key holds.
#
# --full reads no pass and runs every step whole, unit and leak guard alike
# (SUZATARY_GUARD_FULL), and keeps the passes it makes. scripts/train.sh runs it on the first
# train gated a day after a fully passed tree last landed, and keeps its verdict apart: a
# selective pass is never a full one.
#
# SUZATARY_GATE_RUN names a program run in each step's place with the step's
# name, for the test's stub: nothing else of the machine is touched, and its
# passes are kept apart, keyed by its own bytes, so a stub's green is never
# read as a real one.
#
# Exit 0 green, 1 red, 64 usage, 75 when the gate did not start (too little
# disk, a key it could not name, or another gate running in this checkout): a
# refusal to run is no verdict on the tree.
set -uo pipefail

usage() {
  echo "usage: scripts/merge-gate.sh [--full] [--no-live | --only-live] [--steps <name>[,<name>...]] (in any order) | --list" >&2
  exit 64
}
halves=(tree live) list=0 full=0 half="" only="" given=$#
while (($#)); do
  case "$1" in
    --full) ((!full)) || usage; full=1 ;;
    --no-live | --only-live) [[ -z "$half" ]] || usage; half="$1" ;;
    --steps) [[ -z "$only" && -n "${2:-}" ]] || usage; only="$2"; shift ;;
    --list) list=1 ;;
    *) usage ;;
  esac
  shift
done
case "$half" in --no-live) halves=(tree) ;; --only-live) halves=(live) ;; esac
((!list || (given == 1))) || usage
# How a half's lanes share the machine: `weighted`, side by side with one heavy step at a time,
# or `serial`, one lane after another (SUZATARY_GATE_SERIAL=1). Read here and taken out of the
# environment, so no step sees it and no key names it: the order changes no verdict.
case "${SUZATARY_GATE_SERIAL:-}" in
  "") lanes=weighted ;;
  1) lanes=serial ;;
  *) echo "merge-gate.sh: SUZATARY_GATE_SERIAL is 1 or unset, not $SUZATARY_GATE_SERIAL" >&2; exit 64 ;;
esac
unset SUZATARY_GATE_SERIAL

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root" || exit 1

# ONE GATE AT A TIME IN A CHECKOUT: two would race on what the steps write outside the tree (the
# target, web/dist, the generated wire, the roots), so a second does not start, naming the first.
# The steps run without the lock's descriptor, so nothing a step leaves running holds it.
if ((!list)); then
  lock="$(git rev-parse --absolute-git-dir)/merge-gate.lock"
  exec 8>>"$lock"
  flock -n 8 || { echo "merge-gate.sh: another gate runs in this checkout: $(cat "$lock")" >&2; exit 75; }
  echo "pid $$, started $(date -u +%FT%TZ)" >"$lock"
  # The heavy steps' semaphore, one for every checkout of the repository, so the gates of two
  # worktrees share the memory as two lanes of one gate do.
  heavy_lock="$(git rev-parse --path-format=absolute --git-common-dir)/merge-gate-heavy.lock"
fi

# The `all` modules that find the owner's data tree through instance::ENV.
live_modules=(accounts context_headroom live_blocks perf_fold_chain)

logs="$(mktemp -d)"
trap 'rm -rf "$logs"' EXIT
selected() { [[ " ${halves[*]} " == *" $1 "* ]]; }
# Whether step <i> runs: its half is chosen, and --steps names it or is not given.
chosen() { selected "${s_half[$1]}" && { [[ -z "$only" ]] || [[ ",$only," == *",${s_name[$1]},"* ]]; }; }

# The kept passes: the real gate's, or a stub's apart by its bytes.
kept_dir="$(git rev-parse --path-format=absolute --git-common-dir)/gate"
[[ -z "${SUZATARY_GATE_RUN:-}" ]] || kept_dir+="/other-$(git hash-object "$SUZATARY_GATE_RUN")"

# Whether this gate reads and keeps passes, and if not why: only on a clean checkout, and under a
# Nix shell, whose PATH names the tools the keys then cover.
keeping() {
  [[ -n "${IN_NIX_SHELL:-}${SUZATARY_GATE_RUN:-}" ]] || { echo "outside a nix shell"; return 1; }
  git rev-parse --verify --quiet HEAD >/dev/null || { echo "no HEAD"; return 1; }
  [[ -z "$(git status --porcelain --ignore-submodules=none 2>&1)" ]] || { echo "the checkout is not HEAD's tree"; return 1; }
}

# The environment a step reads beside the tree, one sorted line each.
environment() {
  env | grep -E '^(PATH|SUZATARY_|CARGO|RUST|NODE|NIX_(CFLAGS|LDFLAGS)|CC|CXX|LD_|LIBRARY_PATH|PKG_CONFIG|FONTCONFIG|TYPST|TZ|LANG|LC_)[A-Z_]*=' \
    | grep -vE '^SUZATARY_(INSTANCE|GATE_UNITS|GUARD_FULL|GUARD_PASSES)=' | LC_ALL=C sort
}

# The ignored files a step reads: proptest's saved counterexamples, which change the cases
# `test` runs. Named by content, in every key.
ignored_inputs() {
  git ls-files -z --others --ignored --exclude-standard -- ':(glob)**/proptest-regressions/**' |
    LC_ALL=C sort -z | xargs -0 -r sha256sum
}

# HEAD's entries, the environment and the ignored inputs, read once, before any step runs.
snapshot() {
  git ls-tree -r --full-tree HEAD >"$logs/tree" && { environment && ignored_inputs; } >"$logs/env"
}

# Where a pass of step <name>'s unit <unit> reading <spec> is kept: the hash of the entries of
# HEAD's tree the spec reads and of the environment. A path in a spec names itself and what is
# under it; `all-but:` reads every entry but those, and `only:` those alone, with the gate's own
# scripts and the flake.
kept_at() {
  local name="$1" unit="$2" spec="$3" key mode=all paths=""
  case "$spec" in
    all-but:*) mode=skip paths="${spec#all-but:}" ;;
    only:*) mode=only paths="${spec#only:} scripts flake.nix flake.lock" ;;
  esac
  key="$({
    awk -F'\t' -v mode="$mode" -v paths="$paths" '
      BEGIN { n = split(paths, p, " "); for (k = 1; k <= n; k++) set[p[k]] }
      function named(path,   d) {
        if (path in set) return 1
        d = path
        while (sub(/\/[^\/]*$/, "", d)) if (d in set) return 1
        return 0
      }
      mode == "all" || (mode == "skip") != named($2)
    ' "$logs/tree" && cat "$logs/env"
  } | sha256sum | cut -c1-40)" && printf '%s/%s/%s%s\n' "$kept_dir" "$name" "${unit:+${unit//\//%}/}" "$key"
}

# A step's reads as declared: `all`, `all-but:<path>[,<path>...]`, `only:<path>[,<path>...]`,
# `crates`, `files:<dir>` or `unkept`. A path no step may leave out is refused before anything
# runs: the gate, the shell and the scripts are in every key.
reads_of() {
  local p
  case "$1" in
    all | unkept | crates | files:web/test | files:web/e2e) echo "$1" ;;
    only:?*) echo "$1" ;;
    all-but:?*)
      for p in ${1#all-but:}; do
        p="${p%/}"
        case "$p" in
          "" | . | :* | scripts | scripts/* | flake.nix | flake.lock)
            echo "merge-gate.sh: a step may not leave out $p" >&2
            exit 64
            ;;
        esac
      done
      echo "$1"
      ;;
    *) echo "merge-gate.sh: reads is all, all-but:<paths>, only:<paths>, crates, files:<dir> or unkept, not $1" >&2; exit 64 ;;
  esac
}

# The plan, in the order the summary prints it: each step's half, lane, name, reads and command.
s_half=() s_lane=() s_name=() s_reads=() s_cmd=() s_group=()
lane=""
step() {
  local half="$1" name="$2" reads
  reads="$(reads_of "${3//,/ }")" || exit 64
  shift 3
  if ((list)); then
    # A unit step's reads, then `~` and its reads as one unit.
    local shown="$reads"
    [[ -z "${unit_fallback[$reads]:-}" ]] || shown+="~${unit_fallback[$reads]}"
    printf '%-5s %-6s %-16s %-50s %s\n' "$half" "$lane" "$name" "${shown// /,}" "$*"
    return
  fi
  s_half+=("$half") s_lane+=("$lane") s_name+=("$name") s_reads+=("$reads") s_cmd+=("$(printf '%q ' "$@")")
  s_group+=("$name")
}

# The named steps are kept or run together: one writes what another reads, outside the tree.
together() {
  local i
  ((list)) && { echo "together $*"; return; }
  for i in "${!s_name[@]}"; do
    if [[ " $* " == *" ${s_name[i]} "* ]]; then s_group[i]="$1"; fi
  done
}

# The named steps are heavy: each needs the machine's memory to itself, so it waits until no
# other heavy step runs, in this gate or another checkout's. A light step runs beside one. In a
# weighted gate the heavy steps of a half take the machine in the order they are named, each
# after the one named before it that runs (`s_after`, set by run_half), so the step with the
# most work behind it in its lane goes first. Within a lane the order must be the lane's own.
s_heavy=() heavy_order=() s_after=()
heavy() {
  local name i
  ((list)) && { echo "heavy $*"; return; }
  for name; do
    for i in "${!s_name[@]}"; do
      if [[ "${s_name[i]}" == "$name" ]]; then s_heavy[i]=1 heavy_order+=("$i"); fi
    done
  done
}

# Step <i>, run in its lane's shell so a step that exports (the resolvers below) hands the
# value to the steps after it. A step whose passes are all kept (`hit`, decided before any step
# ran) is not run; a step split into units runs those named in `todo_units`, handed to it in
# SUZATARY_GATE_UNITS, or whole when that is empty. Its passes are noted beside its log and kept
# at the end. Its output goes to its log, printed whole when it ends so two lanes never
# interleave lines; its summary row and any failure go to files the lanes share. A heavy step
# holds the heavy lock while it runs, on a descriptor the step itself does not inherit, so a
# process it leaves running holds nothing; its row names the time it waited.
run_step() {
  local i="$1" name="${s_name[$1]}" start=$SECONDS code took of=""
  if [[ -n "${hit[i]:-}" ]]; then
    echo "==> $name: kept, its inputs passed on $(head -1 <<<"${kept[i]}" | xargs tail -1 | cut -d' ' -f2-)" >&2
    printf '%-16s ok  kept\n' "$name" >"$logs/$name.row"
    return
  fi
  if [[ -n "${s_heavy[i]:-}" ]]; then
    if [[ -n "${s_after[i]:-}" && ! -e "$logs/${s_after[i]}.row" ]]; then
      echo "==> $name waits for its turn, after ${s_after[i]}" >&2
      until [[ -e "$logs/${s_after[i]}.row" ]]; do sleep 1; done
    fi
    exec 7>>"$heavy_lock"
    flock -n 7 || { echo "==> $name waits for the heavy step $(cat "$heavy_lock")" >&2; flock 7; }
    echo "$name" >"$heavy_lock"
    (((SECONDS - start) == 0)) || of=" (waited $((SECONDS - start))s)"
    start=$SECONDS
  fi
  echo "==> $name (${s_lane[i]} lane)" >&2
  if [[ "${s_half[i]}" == tree ]]; then local -x SUZATARY_INSTANCE="$logs/no-data-tree/instance.toml"; fi
  if [[ -n "${todo_units[i]:-}" ]]; then
    local -x SUZATARY_GATE_UNITS="${todo_units[i]}"
    of+=" ($(wc -w <<<"${todo_units[i]}") of $(wc -w <<<"${units[i]}") units)"
    echo "==> $name runs the units whose inputs moved: ${todo_units[i]}" >&2
  fi
  if [[ -n "${SUZATARY_GATE_RUN:-}" ]]; then "$SUZATARY_GATE_RUN" "$name"; else eval "${s_cmd[i]}"; fi >"$logs/$name" 2>&1 8>&- 7>&-
  code=$? took=$((SECONDS - start))
  [[ -z "${s_heavy[i]:-}" ]] || exec 7>&-
  printf '%-16s %s  %4ds%s\n' "$name" "$([ "$code" -eq 0 ] && echo ok || echo "FAILED ($code)")" "$took" "$of" >"$logs/$name.row"
  if [ "$code" -eq 0 ]; then
    [[ -z "${todo_kept[i]:-}" ]] || echo "${todo_kept[i]}" >"$logs/$name.pass"
  else
    : >"$logs/$name.failed"
  fi
  {
    flock 9
    echo "==> $name: $(cut -c18- "$logs/$name.row")" >&2
    cat "$logs/$name" >&2
  } 9>"$logs/.print"
}

# A lane's steps one after another; a half's lanes side by side, then its quiet lane alone; and
# the halves one after the other, so the live half runs alone as it always has. On the box (11
# GB, beside the server) the lanes' builds side by side ran it out of memory (train 90,
# 2026-10-16: a Nix build of the app beside clippy), so the heavy steps take turns; `serial`
# runs a half's lanes one after another too. No lane reads what another writes, so the order
# changes no verdict.
run_lane() {
  local i
  for i in "${!s_name[@]}"; do
    if [[ "${s_half[i]}" == "$1" && "${s_lane[i]}" == "$2" ]] && chosen "$i"; then run_step "$i"; fi
  done
}
run_half() {
  local l i turn=""
  s_after=()
  if [[ "$lanes" == weighted ]]; then
    for i in "${heavy_order[@]}"; do
      if [[ "${s_half[i]}" == "$1" && -z "${hit[i]:-}" ]] && chosen "$i"; then s_after[i]="$turn" turn="${s_name[i]}"; fi
    done
  fi
  for l in $(for i in "${!s_name[@]}"; do [[ "${s_half[i]}" != "$1" ]] || echo "${s_lane[i]}"; done | awk '!seen[$0]++'); do
    [[ "$l" == quiet ]] && continue
    case "$lanes" in
      serial) run_lane "$1" "$l" ;;
      weighted) run_lane "$1" "$l" & ;;
    esac
  done
  wait
  run_lane "$1" quiet
}

# THE CODESPACE'S DISK (CODESPACES=true). A Codespace does nothing but gate, and its 32 GB hold
# the system, the Nix store and the target, which it keeps when scripts/train.sh stops it (its
# /tmp, a disk of its own, is wiped then), so each gate starts from the last one's build. Until
# 2026-10-10 the gate cleared the target whenever it was short of room, which on 32 GB was
# nearly every gate: each paid the cold build, a third of an hour of the four cores
# (the gate-warm-fast report). Now the target is cleared only when no unit in it is read again:
# for a new compiler, and for a move of what every unit is built with (app/Cargo.toml's dev and
# test profiles, the shell's Rust flags), which builds every unit again beside the old ones. Room
# is made in the order the next gate reuses least: the store's paths no root holds (the gate
# roots what it builds, runs in and builds from), then the target's units by age alone, those no
# gate read for a week, then three days, then one (cargo-sweep, by the last read of each unit's
# fingerprint, which relatime records to within a day), and the whole target only if that is not
# enough. NEVER BY SIZE: `cargo sweep --maxsize` (0.8.0, `remove_older_until_fits`) walks the
# units from oldest to newest and deletes each one that still fits in the bytes to free, so once
# one does not fit it goes on deleting every newer one that does, the newest set's small units
# among them, and every unit above a deleted one builds again (on 2026-10-11 it took the set the
# gate before had just built, and that gate rebuilt every dependency). After the gate the units no gate has read for two days go (a lock's move leaves
# the old ones behind), so the target stays the size of what the gates read. A warm gate needs
# `codespace_room` GB beside its target: the store's new outputs, the units the moved sources
# rebuild beside the old, and the Nix builds' scratch. A gate whose locks moved since the last
# one (app/Cargo.lock, flake.lock: a dependency, the Prodrome pin) builds the moved dependencies
# again beside the units it holds, in cargo and in crane's three dependency builds, as a cold
# gate does, so it needs `codespace_cold_room`; a gate with no record of what the target was
# built from, a cleared one among them, is read as one whose locks moved. Measured on
# 2026-10-11: a move of the profiles (every dependency again, in cargo and in all three crane
# builds) took the disk from 13 GB free to 1.1, so 13 was too few.
codespace_room=9
codespace_cold_room=15
codespace_toolchain="app/target/.gate-toolchain"
codespace_locks="app/target/.gate-locks"
# What every unit is built with: the compiler, the dev and test profiles (their settings, not
# their comments; the release profiles build nothing here, crane builds them) and the shell's
# Rust flags. A move of any of them leaves no unit of the target read again.
codespace_toolchain_now() {
  { rustc -vV && cargo -V &&
    awk '/^\[/ { profile = /^\[profile\.(dev|test)[].]/ } profile && !/^[[:space:]]*(#|$)/' app/Cargo.toml &&
    { env | grep -E '^CARGO_[A-Z0-9_]*RUSTFLAGS=|^RUSTFLAGS=' || true; } | LC_ALL=C sort; } |
    sha256sum | cut -c1-40
}
codespace_locks_now() { cat app/Cargo.lock flake.lock | sha256sum | cut -c1-40; }
codespace_make_room() {
  local toolchain days room=$codespace_room
  toolchain="$(codespace_toolchain_now)"
  if [[ -e "$codespace_toolchain" && "$(cat "$codespace_toolchain")" != "$toolchain" ]]; then
    echo "==> the compiler, a dev or test profile or the Rust flags moved since the target was built, so no unit in it is read again: clearing it" >&2
    rm -rf app/target
  fi
  if [[ "$(cat "$codespace_locks" 2>/dev/null)" != "$(codespace_locks_now)" ]]; then
    room=$codespace_cold_room
    echo "==> the locks moved since the target was built: this gate builds the dependencies again beside it, and needs $room GB" >&2
  fi
  # What an incremental build left (the gate builds without), which no gate reads, and the
  # scratch of Nix builds killed with their gate (a stopped Codespace), which Nix never removes:
  # 0.9 GB of the root disk on 2026-10-10. No gate runs beside this one, so no build is live.
  rm -rf app/target/debug/incremental
  sudo -n find /nix/var/nix/builds -mindepth 1 -maxdepth 1 -mmin +60 -exec rm -rf {} +
  if [ "$(free_gb)" -lt "$room" ]; then
    echo "==> $(free_gb) GB free: collecting the Nix store's unrooted paths" >&2
    nix-collect-garbage >/dev/null 2>&1
  fi
  for days in 7 3 1; do
    if [[ ! -d app/target ]] || (($(free_gb) >= room)); then break; fi
    echo "==> $(free_gb) GB free: sweeping the target's units no gate read in ${days}d" >&2
    cargo sweep --time "$days" app >/dev/null
  done
  if [ "$(free_gb)" -lt "$room" ]; then
    echo "==> $(free_gb) GB free: clearing app/target for a cold build" >&2
    rm -rf app/target
    if [ "$(free_gb)" -lt "$room" ]; then
      echo "==> only $(free_gb) GB free with no target: not starting a gate that needs $room" >&2
      exit 75
    fi
  fi
  mkdir -p app/target && echo "$toolchain" >"$codespace_toolchain"
}

# A flake output, built and rooted under the checkout's gate-roots, and the environment the gate
# runs in, rooted (scripts/nix-out.sh).
# shellcheck source=scripts/nix-out.sh
. "$root/scripts/nix-out.sh"

if ((!list)) && [[ -z "${SUZATARY_GATE_RUN:-}" ]]; then
  # Line tables keep a backtrace's files and lines and drop the rest of the debug
  # info, which is most of a debug target's bytes.
  export CARGO_PROFILE_DEV_DEBUG=line-tables-only
  export CARGO_PROFILE_TEST_DEBUG=line-tables-only

  # NO INCREMENTAL CACHES, ANYWHERE. They were a third of a Codespace's target
  # (6 of 15 GB), and the gate turned them on and off with the free disk, and a
  # flip rebuilds the workspace's crates: one setting, the box's.
  export CARGO_INCREMENTAL=0

  free_gb() { df --output=avail -BG "$root" | tail -1 | tr -dc '0-9'; }
  # THE SHELL THIS GATE RUNS IN IS ROOTED BEFORE ANYTHING COLLECTS: the room below collects the
  # store's unrooted paths, and the box's daemon does short of disk, and `nix develop` roots
  # nothing. Its paths stay rooted until the next gate roots its own.
  if ! root_environment "$(git rev-parse --path-format=absolute --git-common-dir)/gate-roots/environment"; then
    echo "==> could not root the environment this gate runs in, so it collects nothing: not starting" >&2
    exit 75
  fi
  if [ "${CODESPACES:-}" = true ]; then
    codespace_make_room
  else
    # THE BOX: a cold gate grew its disk by 10.2 GB on 2026-10-03 (the target
    # 8.9 GB, the Nix store 1.3), and on 2026-10-01 a gate ran it to 512 MB and
    # the triage pass refused to run, so it keeps that headroom: short of 16 GB
    # free the debug target goes, and short of 12 after that the gate does not start.
    if [ "$(free_gb)" -lt 16 ]; then
      echo "==> $(free_gb) GB free: clearing app/target/debug for a cold build" >&2
      rm -rf app/target/debug
      if [ "$(free_gb)" -lt 12 ]; then
        echo "==> only $(free_gb) GB free after clearing the debug target: not starting a cold gate that needs 12" >&2
        exit 75
      fi
    fi
  fi
fi



# What the steps never read, so a change there runs none of them. The Rust reads no web source,
# test or view, no Android and no docs/ (app/cli/tests/guards/merge_gate.rs holds every Rust
# source but the guards' to that). The web's modules, unit tests and bundles read no Android,
# no docs/ and none of the app's Rust beyond what the four flake builds they take read (the
# same test holds those builds' sources to that): the core, the browser module, the manifests
# and the files the unit tests read from app/core/tests and app/io/tests.
#
# THE FILES ONLY A DECLARING UNIT READS, which most changes move: the changelog, CLAUDE.md and
# the release file. Every Rust and web unit skips them unless crate_reads declares one for it;
# the guards, the public tree, the changelog step and the module's evaluation read the whole
# tree or their own list. merge_gate.rs takes a source's leave to name one from crate_reads.
cli_only=CHANGELOG.md,CLAUDE.md,app/release.toml
rust_base=web/src,web/test,web/e2e,web/typst,android,docs
rust=all-but:$rust_base,$cli_only
web_app_skips=app/bench/src,app/cli/src,app/cli/tests,app/conformance,app/io/src,app/plugin/src,app/plugin/tests,app/plugin-fence/src,app/plugin-fence/wit,app/plugin-ics/src,app/plugin-ics/tests,app/wit
web=all-but:android,docs,$cli_only,$web_app_skips

# THE UNITS (see the head of this file). Each member's tests read what cargo metadata says it
# depends on, and the paths declared here for a member whose tests read another member's files:
# the core's tests include the calendar's schema, and the CLI's tests walk every source of the
# workspace; the CLI's live pins and the manuals' reader name CLAUDE.md. merge_gate.rs fails when
# a path a member's sources resolve is one its unit skips.
crate_reads=(suzatary-core=app/plugin-ics suzatary=app suzatary=CLAUDE.md suzatary-io=CLAUDE.md)
# What every unit of a unit step skips unless it declares the path, and the reads of the step as
# one unit when its graph cannot be read.
rust_skips="${rust#all-but:}" web_skips="${web#all-but:}"
declare -A unit_base=(
  [crates]="${rust_skips//,/ }"
  [files:web/test]="${web_skips//,/ }"
  [files:web/e2e]="android docs ${cli_only//,/ } app/conformance app/plugin-fence/src app/plugin-fence/wit"
)
declare -A unit_fallback=([crates]="all-but:${rust_base//,/ }" [files:web/test]="${web//,/ }" [files:web/e2e]=all)
# The list as merge_gate.rs reads it.
((!list)) || echo "cli_only $cli_only"
# Where a test file's unread files can be: the test files and the modules, whichever directory
# imports them, so the import graph alone decides. And what every unit reads although no import
# names it: the token check reads web/src/view/tokens.ts; a scene drives the whole app bundle,
# so it reads all of web/src; and the typst scene bundles web/e2e/typstbench.ts for the page.
declare -A unit_scopes=([web/test]="web/src web/test web/e2e" [web/e2e]="web/src web/test web/e2e")
declare -A unit_shared=([web/test]="web/src/view/tokens.ts" [web/e2e]="web/src web/e2e/typstbench.ts")

# Invoked through `step`. The manifest goes right after the subcommand, never
# after a `--`, which would hand it to clippy-driver instead of cargo.
# shellcheck disable=SC2329
cargo_app() { cargo "$1" --manifest-path app/Cargo.toml "${@:2}"; }

# The live pins are the private repository's alone (`.gitattributes` keeps them out of the
# public tree), and cargo refuses a `--test` it cannot find, so they are named only where they are.
live_pins=()
if [ -f app/cli/tests/live.rs ]; then live_pins=(--test live); fi

# `--all-targets` leaves out the `test = false` targets, and beside it cargo drops a `--test
# <name>`, so they are linted in a second run, by name.
# shellcheck disable=SC2329
clippy_all() {
  cargo_app clippy -q --workspace --all-targets "$@" -- -D warnings &&
    cargo_app clippy -q --workspace "${live_pins[@]}" --test guards --test fence --test plugin "$@" -- -D warnings
}

# THE RUST IS TWO LANES, SO CARGO'S LOCK IS NEVER IDLE. The checks hold it from start to end; the
# tests hold it only while they build, and their runs, five minutes of a warm gate, leave it to
# the checks. One lint, doc, check and debug build per feature set.
lane=check
step tree fmt             $rust   cargo fmt --manifest-path app/Cargo.toml --all --check
step tree clippy          $rust   clippy_all
step tree clippy-fixture  $rust   clippy_all --features suzatary/fixture
step tree doc             $rust   env RUSTDOCFLAGS="-D warnings" cargo doc --manifest-path app/Cargo.toml -q --workspace --no-deps --features suzatary/fixture
lane=tests
# EVERY DEBUG EXECUTABLE THE TREE HALF RUNS, IN ONE STEP: the workspace's test harnesses, one
# feature set, the `test = false` ones run by name, and the browser suite's fixture binary, which
# scripts/app-build.sh builds in the tests' feature set with the fixture added (`--workspace`
# everywhere: cargo resolves features over the packages a command selects, and a `-p` built
# wasmtime and the wit crates again too). So the fixture shares the tests' dependencies, not the
# workspace's crates: `suzatary/fixture` turns on `suzatary-core/fixture` (app/cli/Cargo.toml),
# and the core and every crate above it compile a second time, as do bit-set and the typst crates
# above it, whose `default` feature only the tests' proptest turns on. The steps after it run what
# it built; each is still the cargo command that would build it, so a step run without it (a kept
# build) builds what it needs.
# shellcheck disable=SC2329
build_once() {
  cargo_app test -q --workspace --no-run &&
    cargo_app test -q --workspace --no-run --test guards --test fence --test plugin &&
    scripts/app-build.sh --fixture >/dev/null
}
step tree build           $rust   build_once
# The plugin host's fences, against the hostile components the flake builds
# and the app's reference plugins, held to the same fences
# (app/plugin/tests/fence.rs); loopback only.
# shellcheck disable=SC2329
suzatary_fence() {
  { [[ -n "${SUZATARY_FENCE_PLUGINS:-}" ]] || SUZATARY_FENCE_PLUGINS="$(nix_out suzatary-plugin-fence)"; } &&
    { [[ -n "${SUZATARY_PLUGINS:-}" ]] || SUZATARY_PLUGINS="$(nix_out suzatary-plugins)"; } &&
    export SUZATARY_FENCE_PLUGINS SUZATARY_PLUGINS
}
step tree fence-plugin    unkept  suzatary_fence
# The workspace's tests, or only the members named in SUZATARY_GATE_UNITS: every executable is
# built in the workspace's feature set, and the runner skips those of the other members. The doc
# tests run apart and whole, since rustdoc runs a merged doc test in one process under a runner.
# shellcheck disable=SC2329
rust_test() {
  [[ -n "${SUZATARY_GATE_UNITS:-}" ]] || { cargo_app test -q --workspace --no-fail-fast; return; }
  local host code=0
  host="$(rustc -vV | sed -n 's/^host: //p')"
  cargo_app test -q --workspace --no-fail-fast --tests \
    --config "target.$host.runner=['$root/scripts/test-units.sh']" || code=$?
  cargo_app test -q --workspace --no-fail-fast --doc || code=$?
  return "$code"
}
step tree test            crates  rust_test
step tree guards          all     cargo_app test -q --workspace --test guards
step tree plugin-fence    $rust   cargo_app test -q --workspace --test fence
step tree plugin-sync     $rust   cargo_app test -q --workspace --test plugin
lane=web
# What the web steps read, prepared once in the lane's shell (scripts/web-inputs.sh): the static
# checks, the browser's modules, exported to the steps after it, and web/src/wire/generated. The
# wire's TypeScript and the conformance vectors are the flake's builds of them, as CI's are (the
# wire's the very derivation nix-web and public-tree build the bundle from, so it is built once a
# gate), outside the cargo lock the Rust lanes hold: web-inputs waited behind clippy for 20
# minutes of a cold gate, and cargo built the core twice more for them, in its own feature set.
# The four are one `nix build`, so Nix builds them side by side. Each is keyed by the sources
# its crates compile (the browser module's by app/core/src and app/wasm, the wire's and the
# vectors' by the core's library and examples), so a change to the CLI, the server's crates or a
# test file builds none of them again. The dependency builds they start from are rooted beside
# them, on the box as on a Codespace: no output holds them, and a collection that took them cost
# the wasm32 dependencies again, five minutes.
# shellcheck disable=SC1091,SC2329
web_inputs() {
  nix_outs suzatary-wire-ts suzatary-vector-json suzatary-wasm suzatary-typst-wasm &&
    nix_out suzatary-wasm-deps >/dev/null && nix_out suzatary-core-deps >/dev/null &&
    SUZATARY_WIRE_TS="$(nix_out suzatary-wire-ts)" &&
    SUZATARY_PRODROME_CONFORMANCE="$(nix_out suzatary-vector-json)" &&
    SUZATARY_PRODROME_WASM="$(nix_out suzatary-wasm)" &&
    SUZATARY_TYPST_WASM="$(nix_out suzatary-typst-wasm)" &&
    export SUZATARY_WIRE_TS SUZATARY_PRODROME_CONFORMANCE SUZATARY_PRODROME_WASM SUZATARY_TYPST_WASM &&
    . scripts/web-inputs.sh
}
step tree web-inputs      $web    web_inputs
step tree web-test        files:web/test scripts/web-test.sh
step tree build-web       $web    scripts/build-web.sh
lane=quiet
# The browser suite, alone, in scripts/web-e2e.sh's own shards (four, or one per core on fewer):
# the whole suite passed 20 of 20 runs in four shards on four cores, 253-271 s each, against
# about 600 s in one (2026-10-17, the e2e-shards report), once the scenes that lost their races
# under load waited on what the page draws. The scenes named in SUZATARY_GATE_UNITS, or every scene.
# shellcheck disable=SC2329
e2e_scenes() {
  local scenes=() unit
  for unit in ${SUZATARY_GATE_UNITS:-}; do scenes+=("$(basename "$unit" .test.ts)"); done
  scripts/web-e2e.sh "${scenes[@]}"
}
step tree e2e             files:web/e2e e2e_scenes
# web-inputs writes web/src/wire/generated, which web-test and build-web read, and e2e serves the
# bundle build-web writes to web/dist: none of it is in the tree.
together web-inputs web-test build-web e2e
lane=nix
# THE PUBLIC TREE OF HEAD STANDS ON ITS OWN: what a deployment runs is that tree, published to
# suzatary-core, so it is built here as the roots run builds it (scripts/app-builds.sh: the app,
# the web, the plugins, the shell), from the store path it would be fetched to, and a file it
# needs from the private history is red here and not after it lands. Its leak guard is the guards'
# (public_tree_holds_nothing_of_this_deployment) and its run the deployed-shape step's. It reads
# HEAD's tree but the private history `git archive` leaves out, as `.gitattributes` declares it,
# all but the changelog the public one is dated from and the attributes themselves (merge_gate.rs
# pins the list). First in its lane: the release build is the gate's longest single build, mostly
# one core (`codegen-units = 1`), so it starts with the gate and ends inside it.
private="$(sed -n 's#^/\(.*\) export-ignore$#\1#p' .gitattributes 2>/dev/null | sed 's#/\*\*$##' |
  grep -vxF -e CHANGELOG.md -e .gitattributes | paste -sd, -)"
# Nor does it read docs/ or android/: its builds' sources hold neither (merge_gate.rs).
public_reads="all-but:${private:+$private,}docs,android"
# The binary it built is exported to the steps after it in its lane, as SUZATARY_DEPLOY_BIN: this
# gate's own, named by the store path its build answered, where `gate-roots/public-bin` is a link
# every checkout's gate moves (the gate's lock is per checkout) and a failed build leaves pointing
# at the last gate's. Unset until the build succeeds, so a reader after a failed one fails too.
# shellcheck disable=SC2329
public_tree() {
  local tree
  unset SUZATARY_DEPLOY_BIN SUZATARY_DEPLOY_TREE
  tree="$(scripts/public-tree.sh --store HEAD)" &&
    scripts/app-builds.sh "$tree" "$(git rev-parse --path-format=absolute --git-common-dir)/gate-roots/public-" &&
    SUZATARY_DEPLOY_BIN="$(nix build "$tree#suzatary" --no-link --print-out-paths)/bin/suzatary" &&
    SUZATARY_DEPLOY_TREE="$tree" &&
    export SUZATARY_DEPLOY_BIN SUZATARY_DEPLOY_TREE
}
step tree public-tree     "$public_reads" public_tree
# The web's Nix build: what a deploy's roots run builds, so a branch that breaks it is red here and
# not at the deploy. Its inputs are rooted beside it, so an unchanged derivation costs an evaluation.
# shellcheck disable=SC2329
nix_web() { nix_out suzatary-web >/dev/null; }
step tree nix-web         $web    nix_web
# The NixOS module evaluated into a host, with the facts a deploy rests on (nix/module-check.nix);
# an evaluation, seconds, so it runs every time.
# shellcheck disable=SC2329
nix_module() {
  nix build --no-link ".#checks.$(nix eval --impure --raw --expr builtins.currentSystem).suzatary-module"
}
step tree nix-module      all     nix_module
# The binary's derivation does not move with the release, the changelog, CLAUDE.md or docs/, and
# the package's does (scripts/release-free.sh): so a train compiles no binary. An evaluation.
step tree release-free    $rust   scripts/release-free.sh
# HEAD's tree as a deploy runs it: a read-only store path with no .git and the units' PATH, from
# which the site and the data push run as the triage pass runs them (scripts/deployed-shape.sh),
# by the deploy's own binary, the one public-tree built. Never kept: it reads the store and the
# daemon and writes under the cache.
step tree deployed-shape  unkept  scripts/deployed-shape.sh
lane=history
# An app/ or web/ change says what it changed: the `## [Unreleased]` section of CHANGELOG.md
# moved since HEAD left origin/master. Never kept: its answer is a function of that base, not of
# the tree, and without the base (a Codespace handed a bundle) it says so and passes.
# shellcheck disable=SC2329
changelog_entry() {
  local base head=HEAD unreleased='/^## \[Unreleased\]/,/^## \[[0-9]/p'
  base="$(git merge-base HEAD origin/master 2>/dev/null)" ||
    { echo "changelog: skipped: no origin/master to compare with"; return 0; }
  # A train's release commit closes Unreleased under its version, so the entries are read on
  # the commit before the cut (scripts/train.sh's `Release:` trailer marks it).
  if [[ -n "$(git log -1 --format='%(trailers:key=Release,valueonly)' HEAD | sed '/^$/d')" ]]; then
    head=HEAD^
  fi
  if git diff --quiet "$base" "$head" -- app/ web/; then
    echo "changelog: no app/ or web/ change since $(git rev-parse --short "$base")"
    return 0
  fi
  if [[ "$(git show "$base:CHANGELOG.md" | sed -n "$unreleased")" == "$(git show "$head:CHANGELOG.md" | sed -n "$unreleased")" ]]; then
    echo "changelog: app/ or web/ changed since $(git rev-parse --short "$base") and CHANGELOG.md's" \
      "## [Unreleased] section did not; add an entry" >&2
    return 1
  fi
  echo "changelog: an ## [Unreleased] entry rides the change"
}
step tree changelog       unkept  changelog_entry

lane=live
# THE LIVE PINS AND THE LIVE MODULES ARE KEPT BY THE CODE THAT READS THE DATA: the files the
# tests and the code they compile are made of (the store, its readers, the wire and every
# migration are in the crates they link), and no file of the data tree. A data change that a
# reader of unchanged code refuses is a finding of the data, not of a change: the full gate, once
# a day, reads no pass and runs them. merge_gate.rs fails when a module these tests compile, or a
# path their sources name, is outside this list.
live_reads=(CLAUDE.md app/Cargo.toml app/Cargo.lock app/conformance app/wit app/core app/io app/plugin app/bench
  app/plugin-ics/Cargo.toml app/plugin-ics/ics.schema app/plugin-fence/Cargo.toml app/wasm/Cargo.toml
  app/cli/Cargo.toml app/cli/src app/cli/tests/common app/cli/tests/this_box.rs app/cli/tests/all.rs
  app/cli/tests/live.rs app/testkit)
for module in "${live_modules[@]}"; do live_reads+=("app/cli/tests/$module.rs"); done
live_only="only:$(IFS=,; echo "${live_reads[*]}")"
# The public tree holds no live pins, so it has no live step.
if ((${#live_pins[@]})); then
  step live live            $live_only cargo_app test -q --workspace --test live
  heavy live
fi
step live live-all        $live_only cargo_app test -q --workspace --test all -- "${live_modules[@]/%/::}"
# The guards read every tracked file against the data tree's lists, so they run every time: a
# docs change is a file the leak guard must read (it keeps each clean file by its hash itself).
step live live-guards     unkept  cargo_app test -q --workspace --test guards
# web-test.sh reads the live chain beside its usual inputs, the wire's TypeScript and the vectors
# taken from the flake as the tree half's are: the very derivations the deploy's bundle is built
# from (the wire), where cargo built the core twice more for them in its own feature set. Kept,
# as the live pins are, by the code that reads the data: the web's reads, which no docs, release
# or changelog change moves (it ran 452 s of a docs-only train); the full gate runs it daily.
# shellcheck disable=SC2329
with_flake_wire() {
  SUZATARY_WIRE_TS="$(nix_out suzatary-wire-ts)" &&
    SUZATARY_PRODROME_CONFORMANCE="$(nix_out suzatary-vector-json)" &&
    export SUZATARY_WIRE_TS SUZATARY_PRODROME_CONFORMANCE &&
    "$@"
}
step live web-live        $web    with_flake_wire scripts/web-test.sh
lane=deploy
# THE DEPLOY'S BUILD, ONCE, ON THE BOX. A deploy builds HEAD's public tree with
# scripts/app-builds.sh before it moves the lock, from the store path that tree is fetched to,
# and a landed commit's published tree is the very bytes `public_tree` adds (the same
# `-source` path: checked against the box's deployed app on 2026-10-10). So the live half builds
# it here, beside its tests, for the box's own system (the tree half's Codespace builds another),
# rooted under gate-roots/public-, and the deploy of the commit this gate lands finds every
# output in the store and only links (the gate-build-once report). The two steps after it run
# the binary it linked, the program the deploy runs, as $SUZATARY_DEPLOY_BIN, which it exports in
# this lane's shell; each runs in a subshell, so a build that made none fails the step, not the lane.
step live deploy-build    unkept  public_tree
# The triage push over the instance's data repository (on the box, the migrated scratch copy the
# operator names), dry: a path it cannot stage, a file its allowlist refuses or a store this tree's
# program does not verify is red here and not on the box. It commits nothing and reaches no remote.
# shellcheck disable=SC2329
push_dry() (
  instance="${SUZATARY_INSTANCE:-$root/instance.toml}"
  [[ -f "$instance" ]] || { echo "push-dry: skipped: no instance file at $instance"; exit 0; }
  SUZATARY_INSTANCE="$instance" SUZATARY_BIN="${SUZATARY_DEPLOY_BIN:?deploy-build made no binary}" \
      scripts/safe-push.sh --dry
)
step live push-dry        unkept  push_dry
# HEAD's public tree, checked whole against the data repository's lists as a publish checks it
# (`--check --strict`), so a commit whose tree leaks never lands and never blocks the publications
# after it. Without an instance file there are no lists to check against, and it says so. Its
# matcher is `suzatary leaks` of the deploy's binary: without one named, the check built the
# private tree's release binary for itself through scripts/app-bin.sh (1769 of the 1791 s it took
# on the box in a train of 2026-10-10), a second release build beside the deploy's.
# shellcheck disable=SC2329
public_check() (
  instance="${SUZATARY_INSTANCE:-$root/instance.toml}"
  [[ -f "$instance" ]] || { echo "public-check: skipped: no instance file at $instance"; exit 0; }
  bin="${SUZATARY_DEPLOY_BIN:?deploy-build made no binary}"
  SUZATARY_INSTANCE="$instance" SUZATARY_BIN="$bin" \
    scripts/public-tree.sh --check --strict "${SUZATARY_DEPLOY_TREE:?deploy-build made no tree}"
)
step live public-check    unkept  public_check

# THE HEAVY STEPS, in the order they take the machine: the workspace's Cargo builds and the Nix
# builds of the app and the browser module, the one with the most work behind it in its lane
# first (build: the tests; web-inputs: the web's tests and bundle). Measured on 2026-10-18 on
# four cores, as memory in use above the idle machine at the step's peak, in a cold gate and in
# a warm one after a change to the core: web-inputs 5.3 and 6.8 GB (four Nix builds side by
# side), build 4.0 and 3.4, public-tree 3.4 and 3.1, clippy 2.3 and 2.2, doc 1.4 and 1.8,
# clippy-fixture 1.6 and 1.6; every light step under 1 GB but e2e (1.9 and 2.6, alone in its
# lane). The live half's Cargo steps (and `live`, above, where the live pins are) compile the
# workspace when that half runs alone, beside deploy-build's release build.
heavy build web-inputs public-tree clippy clippy-fixture doc live-all live-guards deploy-build

((list)) && exit 0

# Every name --steps gives is a step of the halves chosen, or nothing runs.
for name in ${only//,/ }; do
  for i in "${!s_name[@]}"; do
    if [[ "${s_name[i]}" == "$name" ]] && selected "${s_half[i]}"; then continue 2; fi
  done
  echo "merge-gate.sh: --steps names $name, which is no step of the ${halves[*]} half" >&2
  exit 64
done

# A unit step's units, one `<unit>\t<the paths it skips>` line each, or a failure when the graph
# cannot be read (the step then runs whole). The crates come from cargo metadata of the checkout,
# which is HEAD's tree; the test files from esbuild's metafile of their bundles, which needs the
# generated wire from an earlier gate (the import graph of the generated files is read as all of
# web/src/wire, which a change to app/core moves in every unit's key anyway).
units_for() {
  local i="$1" dir
  case "${s_reads[i]}" in
    crates)
      cargo metadata --manifest-path app/Cargo.toml --no-deps --format-version 1 --offline \
        >"$logs/metadata" 2>"$logs/metadata.err" &&
        node scripts/gate-units.mjs crates "$logs/metadata" "$root" "$logs/tree" \
          --base ${unit_base[crates]} -- "${crate_reads[@]}"
      ;;
    files:*)
      dir="${s_reads[i]#files:}"
      esbuild "$dir"/*.test.ts --bundle --platform=node --format=esm --log-level=silent \
        --external:./prodrome.js --external:./typst.js --metafile="$logs/$dir.meta" \
        --outdir="$logs/$dir.bundle" >/dev/null 2>&1 &&
        node scripts/gate-units.mjs files "$logs/$dir.meta" "$logs/tree" "$dir" \
          --scope ${unit_scopes[$dir]} --shared ${unit_shared[$dir]:-} --base ${unit_base[${s_reads[i]}]}
      ;;
  esac
}

# Step <i>'s units and where each one's pass is kept: `kept` and `units` hold them all, and
# `todo_kept` and `todo_units` those with no pass (every one under --full). A step with no units,
# or whose graph could not be read, is one unit keyed by `unit_fallback` or its own reads. When
# every unit is to run, `todo_units` is empty, and the step runs whole.
plan_step() {
  local i="$1" lines unit skips path n=0 todo=0 k specs=()
  if [[ "${s_reads[i]}" == crates || "${s_reads[i]}" == files:* ]]; then
    if lines="$(units_for "$i")" && [[ -n "$lines" ]]; then
      while IFS=$'\t' read -r unit skips; do
        specs+=("$unit" "all-but:$skips")
      done <<<"$lines"
    else
      echo "==> ${s_name[i]}: its units could not be read, so it runs whole" >&2
    fi
  fi
  ((${#specs[@]})) || specs=("" "${unit_fallback[${s_reads[i]}]:-${s_reads[i]}}")
  kept[i]="" units[i]="" todo_kept[i]="" todo_units[i]=""
  for ((k = 0; k < ${#specs[@]}; k += 2)); do
    unit="${specs[k]}"
    path="$(kept_at "${s_name[i]}" "$unit" "${specs[k + 1]}")" || return 1
    n=$((n + 1)) kept[i]+="$path"$'\n' units[i]+="${unit:+$unit }"
    if ((full)) || [[ ! -s "$path" ]]; then
      todo=$((todo + 1)) todo_kept[i]+="$path"$'\n' todo_units[i]+="${unit:+$unit }"
    fi
  done
  kept[i]="${kept[i]%$'\n'}" todo_kept[i]="${todo_kept[i]%$'\n'}"
  units[i]="${units[i]% }" todo_units[i]="${todo_units[i]% }"
  ((todo < n)) || todo_units[i]=""
}

# Where each kept step's passes are kept, and whether they are: decided once, from one snapshot,
# before any step runs. A step kept `together` with a partner runs whenever the partner runs,
# whole, but a step split into units runs only its own units whose inputs moved: the partners
# write what a running reader reads, and a kept unit reads nothing.
kept=() hit=() units=() todo_kept=() todo_units=()
eligible() { [[ "${s_reads[$1]}" != unkept ]] && chosen "$1"; }
if why="$(keeping)"; then
  head="$(git rev-parse HEAD)"
  snapshot || { echo "==> could not read HEAD's tree to name the steps' inputs: not starting" >&2; exit 75; }
  declare -A active=()
  for i in "${!s_name[@]}"; do
    eligible "$i" || continue
    plan_step "$i" || { echo "==> could not name ${s_name[i]}'s inputs: not starting" >&2; exit 75; }
    [[ -z "${todo_kept[i]}" ]] || active["${s_group[i]}"]=1
  done
  for i in "${!kept[@]}"; do
    [[ -z "${todo_kept[i]}" ]] || continue
    if [[ "${s_reads[i]}" == crates || "${s_reads[i]}" == files:* || -z "${active[${s_group[i]}]:-}" ]]; then
      hit[i]=1
    else
      todo_kept[i]="${kept[i]}"
    fi
  done
else
  head=""
  echo "==> no pass is read or kept: ${why}" >&2
fi
# The leak guard reads no pass under --full either; set after the snapshot, it is in no key.
((!full)) || export SUZATARY_GUARD_FULL=1

failures=()
for half in "${halves[@]}"; do run_half "$half"; done
# A Codespace's next gate reuses what this one built and ran in: the target loses only the units
# no gate has read for two days, and the dev shell and crane's dependency build of the app are
# rooted beside the outputs the steps rooted (web-inputs roots the browser module's and the
# core's), so the next collection keeps them. An unrooted path only costs a rebuild.
if [[ "${CODESPACES:-}" == true && -z "${SUZATARY_GATE_RUN:-}" ]]; then
  [[ ! -d app/target ]] || cargo sweep --time 2 app >/dev/null 2>&1 || echo "==> could not sweep app/target" >&2
  [[ ! -d app/target ]] || codespace_locks_now >"$codespace_locks"
  roots="$(git rev-parse --path-format=absolute --git-common-dir)/gate-roots"
  # A profile grows a generation per build and roots every one; the current is kept.
  mkdir -p "$roots" && nix develop .#triage --profile "$roots/triage" -c true >/dev/null 2>&1 &&
    nix profile wipe-history --profile "$roots/triage" >/dev/null 2>&1
  deps="$(nix eval --raw ".#suzatary-deps.outPath" 2>/dev/null)"
  [[ -z "$deps" || ! -e "$deps" ]] || nix-store --add-root "$roots/deps" -r "$deps" >/dev/null 2>&1
fi

# A pass is kept only if the checkout is still the tree its key named.
if [[ -n "$head" ]]; then
  if [[ "$(git rev-parse HEAD)" == "$head" ]] && keeping >/dev/null; then
    for pass in "$logs"/*.pass; do
      [[ -e "$pass" ]] || continue
      while read -r file; do
        mkdir -p "${file%/*}" && printf 'ok %s %s\n' "$head" "$(date -u +%FT%TZ)" >>"$file"
      done <"$pass"
    done
  else
    echo "==> the checkout moved during the gate: no pass is kept" >&2
  fi
fi
echo >&2
for i in "${!s_name[@]}"; do
  if ! selected "${s_half[i]}"; then
    printf '%-16s skipped (%s half)\n' "${s_name[i]}" "${s_half[i]}" >&2
  elif ! chosen "$i"; then
    printf '%-16s skipped (not named by --steps)\n' "${s_name[i]}" >&2
  else
    cat "$logs/${s_name[i]}.row" >&2 2>/dev/null || printf '%-16s FAILED (no row)\n' "${s_name[i]}" >&2
    [[ -e "$logs/${s_name[i]}.row" && ! -e "$logs/${s_name[i]}.failed" ]] || failures+=("${s_name[i]}")
  fi
done
case "${halves[*]}" in
  tree) echo "halves: tree ran; live skipped (it reads the owner's data tree: --only-live on the box)" >&2 ;;
  live) echo "halves: live ran; tree skipped (--no-live, here or on a Codespace)" >&2 ;;
  *) echo "halves: tree and live ran" >&2 ;;
esac
if ((${#failures[@]})); then
  # A test run's last lines are its passing tail, so each step's lines that
  # name a failure come first, and its last lines only when none does.
  echo "==> the failing steps' last lines" >&2
  for name in "${failures[@]}"; do
    echo "--- $name" >&2
    grep -E 'FAILED|panicked|^error|not ok|✖' "$logs/$name" | tail -n 20 >&2 \
      || tail -n 20 "$logs/$name" >&2
  done
fi
scripts/size.sh >&2 || echo "size: not measured" >&2
df -h --output=avail "$root" | tail -1 | sed 's/^ */free after the gate: /' >&2
exit $((${#failures[@]} ? 1 : 0))
