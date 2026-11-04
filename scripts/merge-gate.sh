#!/usr/bin/env bash
# The merge gate: every step a branch passes before it reaches master. Every step runs even
# after a failure; the gate fails if any step failed.
#
# A lane's steps run in order, a half's lanes side by side. A `heavy` step waits until no other
# heavy step runs. The `quiet` lane runs last, alone: the browser suite's waits are budgets
# judged on an idle machine.
#
# Two halves, each step in exactly one:
#   tree  a function of the checked-out tree alone; runs with SUZATARY_INSTANCE naming nothing.
#   live  the tests that read the owner's data tree, so on the box only.
#
# A pass is kept in .git/gate/<step>/<key>, the key hashing HEAD's tree entries outside the
# step's `all-but:` paths, the environment the steps read, and proptest's saved
# counterexamples. The date is not in the key. No step may leave out scripts/, flake.nix or
# flake.lock. Passes are read and kept only for a clean checkout still at HEAD when the gate
# ends. `together` steps run together (one writes what another reads outside the tree). `unkept`
# steps run every time.
#
#   nix develop .#triage -c scripts/merge-gate.sh               both halves
#   nix develop .#triage -c scripts/merge-gate.sh --no-live     the tree half
#   nix develop .#triage -c scripts/merge-gate.sh --only-live   the live half
#   nix develop .#triage -c scripts/merge-gate.sh --full ...    every step and every unit, no pass read
#   nix develop .#triage -c scripts/merge-gate.sh --no-live --steps fmt,doc   those steps alone
#   scripts/merge-gate.sh --list                                every step: half, lane, reads
#
# --steps runs only the named steps; the caller names a step's writers beside it. An unknown
# name is refused (64).
#
# Units: `crates` (each workspace member, reading its own and its dependencies' directories) and
# `files:<dir>` (each test file, reading what its imports reach), named in SUZATARY_GATE_UNITS.
# Where the graph cannot be read, the step runs whole.
#
# --full reads no pass and runs every step whole (SUZATARY_GUARD_FULL); its verdict is kept apart.
#
# SUZATARY_GATE_RUN names a stub run in each step's place; its passes are keyed by its bytes.
#
# Exit 0 green, 1 red, 64 usage, 75 when the gate did not start (too little disk, a key it could
# not name, another gate in this checkout).
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
# Unset after reading so no key names it: the order changes no verdict.
case "${SUZATARY_GATE_SERIAL:-}" in
  "") lanes=weighted ;;
  1) lanes=serial ;;
  *) echo "merge-gate.sh: SUZATARY_GATE_SERIAL is 1 or unset, not $SUZATARY_GATE_SERIAL" >&2; exit 64 ;;
esac
unset SUZATARY_GATE_SERIAL

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root" || exit 1

# One gate per checkout: two would race on the target, web/dist, the generated wire, the roots.
# The steps run without the lock's descriptor, so nothing a step leaves running holds it.
if ((!list)); then
  lock="$(git rev-parse --absolute-git-dir)/merge-gate.lock"
  exec 8>>"$lock"
  flock -n 8 || { echo "merge-gate.sh: another gate runs in this checkout: $(cat "$lock")" >&2; exit 75; }
  echo "pid $$, started $(date -u +%FT%TZ)" >"$lock"
  # Shared by every worktree of the repository.
  heavy_lock="$(git rev-parse --path-format=absolute --git-common-dir)/merge-gate-heavy.lock"
fi

# The `all` modules that find the owner's data tree through instance::ENV.
live_modules=(accounts context_headroom live_blocks perf_fold_chain)

logs="$(mktemp -d)"
trap 'rm -rf "$logs"' EXIT
selected() { [[ " ${halves[*]} " == *" $1 "* ]]; }
chosen() { selected "${s_half[$1]}" && { [[ -z "$only" ]] || [[ ",$only," == *",${s_name[$1]},"* ]]; }; }

kept_dir="$(git rev-parse --path-format=absolute --git-common-dir)/gate"
[[ -z "${SUZATARY_GATE_RUN:-}" ]] || kept_dir+="/other-$(git hash-object "$SUZATARY_GATE_RUN")"

# Prints why not when it fails. Needs a Nix shell, whose PATH names the tools the keys cover.
keeping() {
  [[ -n "${IN_NIX_SHELL:-}${SUZATARY_GATE_RUN:-}" ]] || { echo "outside a nix shell"; return 1; }
  git rev-parse --verify --quiet HEAD >/dev/null || { echo "no HEAD"; return 1; }
  [[ -z "$(git status --porcelain --ignore-submodules=none 2>&1)" ]] || { echo "the checkout is not HEAD's tree"; return 1; }
}

environment() {
  env | grep -E '^(PATH|SUZATARY_|CARGO|RUST|NODE|NIX_(CFLAGS|LDFLAGS)|CC|CXX|LD_|LIBRARY_PATH|PKG_CONFIG|FONTCONFIG|TYPST|TZ|LANG|LC_)[A-Z_]*=' \
    | grep -vE '^SUZATARY_(INSTANCE|GATE_UNITS|GUARD_FULL|GUARD_PASSES)=' | LC_ALL=C sort
}

# Proptest's saved counterexamples change the cases `test` runs.
ignored_inputs() {
  git ls-files -z --others --ignored --exclude-standard -- ':(glob)**/proptest-regressions/**' |
    LC_ALL=C sort -z | xargs -0 -r sha256sum
}

# Read once, before any step runs.
snapshot() {
  git ls-tree -r --full-tree HEAD >"$logs/tree" && { environment && ignored_inputs; } >"$logs/env"
}

# kept_at <name> <unit> <spec>: where that pass is kept. A path in a spec covers what is under it.
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

# `all`, `all-but:<path>[,<path>...]`, `only:<path>[,<path>...]`, `crates`, `files:<dir>` or `unkept`.
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

s_half=() s_lane=() s_name=() s_reads=() s_cmd=() s_group=()
lane=""
step() {
  local half="$1" name="$2" reads
  reads="$(reads_of "${3//,/ }")" || exit 64
  shift 3
  if ((list)); then
    # A unit step's reads, then `~` and its reads as one unit (a format the tests parse).
    local shown="$reads"
    [[ -z "${unit_fallback[$reads]:-}" ]] || shown+="~${unit_fallback[$reads]}"
    printf '%-5s %-6s %-16s %-50s %s\n' "$half" "$lane" "$name" "${shown// /,}" "$*"
    return
  fi
  s_half+=("$half") s_lane+=("$lane") s_name+=("$name") s_reads+=("$reads") s_cmd+=("$(printf '%q ' "$@")")
  s_group+=("$name")
}

together() {
  local i
  ((list)) && { echo "together $*"; return; }
  for i in "${!s_name[@]}"; do
    if [[ " $* " == *" ${s_name[i]} "* ]]; then s_group[i]="$1"; fi
  done
}

# Heavy steps take the machine one at a time, in the order named; within a lane the order must
# be the lane's own.
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

# Runs in its lane's shell so a step that exports (the resolvers) hands the value on. The heavy
# lock's descriptor is not inherited by the step, so a process it leaves running holds nothing.
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

# Heavy steps take turns because two builds side by side run the box (11 GB) out of memory.
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

# The Codespace's disk (CODESPACES=true, 32 GB) keeps the target between gates. The target is
# cleared only when no unit in it is read again (a new compiler, dev/test profile or Rust flags).
# Room is made in the order the next gate reuses least: unrooted store paths, then units no gate
# read for 7, 3, 1 days (cargo-sweep by fingerprint atime), then the whole target.
# Never sweep by size: `cargo sweep --maxsize` (0.8.0, `remove_older_until_fits`) keeps deleting
# every newer unit that fits once one does not, including the newest set.
# Sizes in GB: a warm gate needs `codespace_room` free; a gate whose locks moved (or with no
# record of them) rebuilds the dependencies beside the old units and needs `codespace_cold_room`.
codespace_room=9
codespace_cold_room=15
codespace_toolchain="app/target/.gate-toolchain"
codespace_locks="app/target/.gate-locks"
# The compiler, the dev and test profiles' settings (not their comments; crane builds the release
# profiles) and the Rust flags.
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
  # Nix never removes the scratch of builds killed with a stopped Codespace. No gate runs beside
  # this one, so no build is live.
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

# shellcheck source=scripts/nix-out.sh
. "$root/scripts/nix-out.sh"

if ((!list)) && [[ -z "${SUZATARY_GATE_RUN:-}" ]]; then
  export CARGO_PROFILE_DEV_DEBUG=line-tables-only
  export CARGO_PROFILE_TEST_DEBUG=line-tables-only

  # Always off: a flip rebuilds the workspace's crates.
  export CARGO_INCREMENTAL=0

  free_gb() { df --output=avail -BG "$root" | tail -1 | tr -dc '0-9'; }
  # Root this shell before anything collects: `nix develop` roots nothing, and a collection would
  # take its tools mid-gate.
  if ! root_environment "$(git rev-parse --path-format=absolute --git-common-dir)/gate-roots/environment"; then
    echo "==> could not root the environment this gate runs in, so it collects nothing: not starting" >&2
    exit 75
  fi
  if [ "${CODESPACES:-}" = true ]; then
    codespace_make_room
  else
    # The box: a cold gate grows the disk by about 10 GB, and a full disk stops the triage pass.
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

# What the steps never read; app/cli/tests/guards/merge_gate.rs holds the sources to it.
# `cli_only`: files every Rust and web unit skips unless crate_reads declares one for it.
cli_only=CHANGELOG.md,CLAUDE.md,app/release.toml
rust_base=web/src,web/test,web/e2e,web/typst,android,docs,memory
rust=all-but:$rust_base,$cli_only
web_app_skips=app/bench/src,app/cli/src,app/cli/tests,app/conformance,app/io/src,app/plugin/src,app/plugin/tests,app/plugin-fence/src,app/plugin-fence/wit,app/plugin-ics/src,app/plugin-ics/tests,app/wit
web=all-but:android,docs,memory,$cli_only,$web_app_skips

# Paths a member's tests read beyond its cargo dependencies; merge_gate.rs checks them.
crate_reads=(suzatary-core=app/plugin-ics suzatary=app suzatary=CLAUDE.md suzatary-io=CLAUDE.md)
# What every unit skips unless it declares the path, and the step's reads when its graph cannot
# be read.
rust_skips="${rust#all-but:}" web_skips="${web#all-but:}"
declare -A unit_base=(
  [crates]="${rust_skips//,/ }"
  [files:web/test]="${web_skips//,/ }"
  [files:web/e2e]="android docs memory ${cli_only//,/ } app/conformance app/plugin-fence/src app/plugin-fence/wit"
)
declare -A unit_fallback=([crates]="all-but:${rust_base//,/ }" [files:web/test]="${web//,/ }" [files:web/e2e]=all)
((!list)) || echo "cli_only $cli_only"
# unit_scopes: where a test file's imports can lead. unit_shared: what units read with no import
# naming it; `<test>=<path>` is read by that unit alone.
declare -A unit_scopes=([web/test]="web/src web/test web/e2e" [web/e2e]="web/src web/test web/e2e")
declare -A unit_shared=([web/test]="web/src/view/tokens.ts web/test/e2estores.test.ts=web/e2e" [web/e2e]="web/src web/e2e/typstbench.ts web/e2e/painter.test.ts=web/e2e/painterlaws.ts")

# The manifest goes right after the subcommand, never after a `--`, which would hand it to
# clippy-driver instead of cargo.
# shellcheck disable=SC2329
cargo_app() { cargo "$1" --manifest-path app/Cargo.toml "${@:2}"; }

# The live pins exist only in the private repository, and cargo refuses a `--test` it cannot find.
live_pins=()
if [ -f app/cli/tests/live.rs ]; then live_pins=(--test live); fi

# `--all-targets` leaves out the `test = false` targets, and beside it cargo drops a `--test
# <name>`, so they are linted in a second run, by name.
# shellcheck disable=SC2329
clippy_all() {
  cargo_app clippy -q --workspace --all-targets "$@" -- -D warnings &&
    cargo_app clippy -q --workspace "${live_pins[@]}" --test guards --test fence --test plugin "$@" -- -D warnings
}

# Two Rust lanes keep cargo's lock busy: the tests' runs leave it to the checks.
lane=check
step tree fmt             $rust   cargo fmt --manifest-path app/Cargo.toml --all --check
step tree clippy          $rust   clippy_all
step tree clippy-fixture  $rust   clippy_all --features suzatary/fixture
step tree doc             $rust   env RUSTDOCFLAGS="-D warnings" cargo doc --manifest-path app/Cargo.toml -q --workspace --no-deps --features suzatary/fixture
lane=tests
# Every debug executable the tree half runs, built in one step. `--workspace` everywhere: cargo
# resolves features over the packages a command selects, and a `-p` rebuilds wasmtime and the
# wit crates. Later steps are still the cargo commands that build what they run, so a kept build
# changes nothing.
# The cli's tests that need `fixture`, in the feature set app-build.sh --fixture uses.
fixture_tests=(--test all --features suzatary/fixture --features suzatary-core/nothing)
# shellcheck disable=SC2329
build_once() {
  cargo_app test -q --workspace --no-run &&
    cargo_app test -q --workspace --no-run --test guards --test fence --test plugin &&
    cargo_app test -q --workspace --no-run "${fixture_tests[@]}" &&
    scripts/app-build.sh --fixture >/dev/null
}
step tree build           $rust   build_once
# The plugin host's fences against hostile components (app/plugin/tests/fence.rs); loopback only.
# shellcheck disable=SC2329
suzatary_fence() {
  { [[ -n "${SUZATARY_FENCE_PLUGINS:-}" ]] || SUZATARY_FENCE_PLUGINS="$(nix_out suzatary-plugin-fence)"; } &&
    { [[ -n "${SUZATARY_PLUGINS:-}" ]] || SUZATARY_PLUGINS="$(nix_out suzatary-plugins)"; } &&
    export SUZATARY_FENCE_PLUGINS SUZATARY_PLUGINS
}
step tree fence-plugin    unkept  suzatary_fence
# The runner skips members SUZATARY_GATE_UNITS does not name.
# shellcheck disable=SC2329
units_test() {
  local host
  host="$(rustc -vV | sed -n 's/^host: //p')"
  cargo_app test -q --workspace --no-fail-fast "$@" \
    --config "target.$host.runner=['$root/scripts/test-units.sh']"
}
# Doc tests run apart and whole: rustdoc runs a merged doc test in one process under a runner.
# shellcheck disable=SC2329
rust_test() {
  [[ -n "${SUZATARY_GATE_UNITS:-}" ]] || { cargo_app test -q --workspace --no-fail-fast; return; }
  local code=0
  units_test --tests || code=$?
  cargo_app test -q --workspace --no-fail-fast --doc || code=$?
  return "$code"
}
step tree test            crates  rust_test
# shellcheck disable=SC2329
rust_test_fixture() {
  [[ -n "${SUZATARY_GATE_UNITS:-}" ]] || { cargo_app test -q --workspace --no-fail-fast "${fixture_tests[@]}"; return; }
  units_test "${fixture_tests[@]}"
}
step tree test-fixture    crates  rust_test_fixture
step tree guards          all     cargo_app test -q --workspace --test guards
step tree plugin-fence    $rust   cargo_app test -q --workspace --test fence
step tree plugin-sync     $rust   cargo_app test -q --workspace --test plugin
lane=web
# Prepared once in the lane's shell and exported to the steps after it. Built by Nix, outside the
# cargo lock the Rust lanes hold. The dependency builds are rooted too: no output holds them.
# shellcheck disable=SC1091,SC2329
web_inputs() {
  nix_outs suzatary-web-ts suzatary-vector-json suzatary-wasm suzatary-views-wasm suzatary-typst-wasm &&
    nix_out suzatary-wasm-deps >/dev/null && nix_out suzatary-views-wasm-deps >/dev/null && nix_out suzatary-core-deps >/dev/null &&
    SUZATARY_WIRE_TS="$(nix_out suzatary-web-ts)" &&
    SUZATARY_PRODROME_CONFORMANCE="$(nix_out suzatary-vector-json)" &&
    SUZATARY_PRODROME_WASM="$(nix_out suzatary-wasm)" &&
    SUZATARY_VIEWS_WASM="$(nix_out suzatary-views-wasm)" &&
    SUZATARY_TYPST_WASM="$(nix_out suzatary-typst-wasm)" &&
    export SUZATARY_WIRE_TS SUZATARY_PRODROME_CONFORMANCE SUZATARY_PRODROME_WASM SUZATARY_VIEWS_WASM SUZATARY_TYPST_WASM &&
    . scripts/web-inputs.sh
}
step tree web-inputs      $web    web_inputs
step tree web-test        files:web/test scripts/web-test.sh
step tree build-web       $web    scripts/build-web.sh
lane=quiet
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
# HEAD's public tree, built as the roots run builds it, so a file it needs from the private
# history is red here. It reads all but the paths `.gitattributes` marks export-ignore, keeping
# the changelog and the attributes (merge_gate.rs pins the list). First in its lane: it is the
# longest single build.
private="$(sed -n 's#^/\(.*\) export-ignore$#\1#p' .gitattributes 2>/dev/null | sed 's#/\*\*$##' |
  grep -vxF -e CHANGELOG.md -e .gitattributes | paste -sd, -)"
public_reads="all-but:${private:+$private,}docs,memory,android"
# Exports SUZATARY_DEPLOY_BIN by store path, never `gate-roots/public-bin`, a link every
# checkout's gate moves. Unset until the build succeeds, so a reader after a failed one fails.
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
# shellcheck disable=SC2329
nix_web() { nix_out suzatary-web >/dev/null; }
step tree nix-web         $web    nix_web
# shellcheck disable=SC2329
nix_module() {
  nix build --no-link ".#checks.$(nix eval --impure --raw --expr builtins.currentSystem).suzatary-module"
}
step tree nix-module      all     nix_module
step tree release-free    $rust   scripts/release-free.sh
# Unkept: it reads the store and the daemon and writes under the cache.
step tree deployed-shape  unkept  scripts/deployed-shape.sh
lane=history
# Unkept: its answer is a function of origin/master, not of the tree; without it, it passes.
# shellcheck disable=SC2329
changelog_entry() {
  local base head=HEAD unreleased='/^## \[Unreleased\]/,/^## \[[0-9]/p'
  base="$(git merge-base HEAD origin/master 2>/dev/null)" ||
    { echo "changelog: skipped: no origin/master to compare with"; return 0; }
  # A release commit (`Release:` trailer) closes Unreleased, so read the commit before it.
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
# The live tests are kept by the code that reads the data, not by the data; the daily full gate
# reruns them. merge_gate.rs fails when a path their sources name is outside this list.
live_reads=(CLAUDE.md app/Cargo.toml app/Cargo.lock app/conformance app/wit app/core app/io app/plugin app/bench
  app/plugin-ics/Cargo.toml app/plugin-ics/ics.schema app/plugin-fence/Cargo.toml app/wasm/Cargo.toml
  app/cli/Cargo.toml app/cli/src app/cli/tests/common app/cli/tests/this_box.rs app/cli/tests/all.rs
  app/cli/tests/live.rs app/testkit app/viewtree/Cargo.toml app/pages/Cargo.toml)
for module in "${live_modules[@]}"; do live_reads+=("app/cli/tests/$module.rs"); done
live_only="only:$(IFS=,; echo "${live_reads[*]}")"
if ((${#live_pins[@]})); then
  step live live            $live_only cargo_app test -q --workspace --test live
  heavy live
fi
step live live-all        $live_only cargo_app test -q --workspace --test all -- "${live_modules[@]/%/::}"
# Unkept: the leak guard must read every tracked file, docs included.
step live live-guards     unkept  cargo_app test -q --workspace --test guards
# Kept by the web's reads, as the live pins are by the code; the full gate runs it daily.
# shellcheck disable=SC2329
with_flake_wire() {
  SUZATARY_WIRE_TS="$(nix_out suzatary-web-ts)" &&
    SUZATARY_PRODROME_CONFORMANCE="$(nix_out suzatary-vector-json)" &&
    export SUZATARY_WIRE_TS SUZATARY_PRODROME_CONFORMANCE &&
    "$@"
}
step live web-live        $web    with_flake_wire scripts/web-test.sh
lane=deploy
# The deploy's build, done here so the deploy of this commit finds every output in the store (the
# same `-source` path). The two steps after it run $SUZATARY_DEPLOY_BIN, each in a subshell, so a
# missing binary fails the step, not the lane.
step live deploy-build    unkept  public_tree
# Dry: it commits nothing and reaches no remote.
# shellcheck disable=SC2329
push_dry() (
  instance="${SUZATARY_INSTANCE:-$root/instance.toml}"
  [[ -f "$instance" ]] || { echo "push-dry: skipped: no instance file at $instance"; exit 0; }
  SUZATARY_INSTANCE="$instance" SUZATARY_BIN="${SUZATARY_DEPLOY_BIN:?deploy-build made no binary}" \
      scripts/safe-push.sh --dry
)
step live push-dry        unkept  push_dry
# As a publish checks it, so a leaking tree never lands. SUZATARY_BIN avoids a second release build.
# shellcheck disable=SC2329
public_check() (
  instance="${SUZATARY_INSTANCE:-$root/instance.toml}"
  [[ -f "$instance" ]] || { echo "public-check: skipped: no instance file at $instance"; exit 0; }
  bin="${SUZATARY_DEPLOY_BIN:?deploy-build made no binary}"
  SUZATARY_INSTANCE="$instance" SUZATARY_BIN="$bin" \
    scripts/public-tree.sh --check --strict "${SUZATARY_DEPLOY_TREE:?deploy-build made no tree}"
)
step live public-check    unkept  public_check

# In the order they take the machine: the one with the most work behind it in its lane first.
heavy build web-inputs public-tree clippy clippy-fixture doc live-all live-guards deploy-build

((list)) && exit 0

for name in ${only//,/ }; do
  for i in "${!s_name[@]}"; do
    if [[ "${s_name[i]}" == "$name" ]] && selected "${s_half[i]}"; then continue 2; fi
  done
  echo "merge-gate.sh: --steps names $name, which is no step of the ${halves[*]} half" >&2
  exit 64
done

# One `<unit>\t<the paths it skips>` line each, or a failure when the graph cannot be read. The
# esbuild graph needs the generated wire from an earlier gate.
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

# Sets `kept`/`units` (all) and `todo_kept`/`todo_units` (those with no pass). An empty
# `todo_units` means the step runs whole.
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

# Decided once, before any step runs. A `together` partner runs whole when another runs, but a
# unit step runs only its moved units: a kept unit reads nothing.
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
# Set after the snapshot, so it is in no key.
((!full)) || export SUZATARY_GUARD_FULL=1

failures=()
for half in "${halves[@]}"; do run_half "$half"; done
# Keep what a Codespace's next gate reuses: sweep units unread for two days, root the shell and
# crane's dependency build.
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
  # A test run's last lines are its passing tail, so the lines naming a failure come first.
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
