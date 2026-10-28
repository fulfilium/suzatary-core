#!/usr/bin/env bash
# The web app's BEHAVIOUR gate: the real frontend, in a real browser, against
# the real backend.
#
# scripts/build-web.sh is the type gate (tsc, and the one-door grep);
# scripts/web-test.sh runs the pure modules under node. Neither renders a page.
# This does: it starts three fixture servers over temp chains, launches a headless
# Chromium per test file (web/e2e/cdp.ts — node 24's global WebSocket is the
# whole DevTools client, so there is still no npm here), and asserts on the DOM
# the app actually built.
#
# THREE SERVERS, AND THE DIFFERENCES ARE THE GATE AND THE INBOX.
#   E2E_BASE           gated to a login this deployment knows as a VIEWER and
#                      not an actor. Every write against it is the real 403, so
#                      its chain cannot move — which is why every read-only file
#                      can share it, and why the rollback test drives it. It has
#                      NOTHING waiting, which is what makes the empty Inbox a
#                      state some scene can actually see.
#   E2E_BASE_PROPOSED  the same viewer login, over a chain whose triage run left
#                      a CARD waiting (§10.6) AND whose secretary has REVIEWED
#                      it: the card is held with its reason. One page cannot be
#                      both empty and full, so the read-only Inbox is two
#                      deployments rather than one deployment two files
#                      disagree about.
#   E2E_BASE_WRITE     gated to the writer login, with a chain of its own and a
#                      card of its own. The files that complete and add todos,
#                      and the ones that tap a card, move this one only.
#
# All three are the REAL `suzatary serve` — the Rust binary, every route and
# every refusal — over a temp chain. They run a build with a whois fixture
# compiled in, so a browser on this box arrives as a named peer, which a real
# daemon never names loopback: `$SUZATARY_FIXTURE` (the `.#suzatary-fixture`
# derivation) when set, CI's path — otherwise `scripts/app-build.sh
# --fixture` builds it incrementally with the `.#triage` shell's cargo. The
# SHIPPED `.#suzatary` does not contain that code at all either way; flake.nix
# and app/cli/src/commands/fixture.rs say why it is a separate build and not a flag.
#
#   scripts/web-e2e.sh            every file
#   scripts/web-e2e.sh list       only web/e2e/list.test.ts
#   scripts/web-e2e.sh bootbench  only web/e2e/bootbench.bench.ts, a measurement
#   scripts/web-e2e.sh pagebench  only web/e2e/pagebench.bench.ts, every page measured
#   scripts/web-e2e.sh typstmeasure  only web/e2e/typstmeasure.bench.ts, the Typst list's draw timed
#                                 no gate runs any of them (`*.bench.ts` is outside every glob)
#
# Screenshots of failures land in $SUZATARY_E2E_SHOTS (a temp dir by default);
# the path is in the failure message. $SUZATARY_SHOTS is the other one — where
# `web/e2e/shots.test.ts` files the deliberate gallery.
#
# $SUZATARY_E2E_WAIT_MS (default 15000, unset only) is how long a wait may
# take, and it SCALES every timeout the scenes name rather than replacing them
# (`web/e2e/cdp.ts`). Left unset, cdp.ts reads the box's own 1-minute load
# average against its cores and raises the budget for you when the box is
# busy (see below); set it yourself to override that, and CI always does
# (60000). It is a budget, not a delay: a wait satisfied at once still costs
# nothing.
set -euo pipefail

# $SUZATARY_E2E_SCENES, a pattern, runs only the scenes whose names match it.
# $SUZATARY_E2E_PART=<k>/<n> runs only the k-th of n parts of the files (below); empty, every file.
# $SUZATARY_E2E_TRACE_REQUESTS=1 logs every signed request: the tab's stages to
# the console (a failure's trace keeps it) and the box's to each server's log.
if [[ "${SUZATARY_E2E_TRACE_REQUESTS:-}" == "1" ]]; then
  export RUST_LOG="${RUST_LOG:-info},suzatary::requests=debug"
fi

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
. "$root/scripts/nix-out.sh"

# Each scene fails after $SUZATARY_E2E_SCENE_MS (default five minutes; the slowest scene takes about two).
# Without a bound, one scene whose wait never ended held the gate for 7.7 hours (train 83, 2026-10-14).

# How many shards (below), refused before anything is built: a value that is no count is a
# mistake to name, never one shard in silence. Unset, four, or one per core on a machine with
# fewer: four shards on four cores ran the whole suite green in about 275 s against about 600 s
# in one (2026-10-13, the gate's Codespace).
shards="${SUZATARY_E2E_SHARDS:-$(( $(nproc) < 4 ? $(nproc) : 4 ))}"
if [[ ! "$shards" =~ ^[1-9][0-9]*$ ]]; then
  echo "web-e2e.sh: SUZATARY_E2E_SHARDS is a count of shards, 1 or more, not '$shards'" >&2
  exit 64
fi

# A PART, so separate machines split the suite (.github/workflows/gate.yml's e2e jobs): the files
# of part k of n (scripts/e2e-part.sh, dealt by the tree alone), in place of the files named, and
# refused before anything is built as the shards are. A part dealt no file runs nothing.
if [[ -n "${SUZATARY_E2E_PART:-}" ]]; then
  part="$(scripts/e2e-part.sh "$SUZATARY_E2E_PART" "$@")" || exit $?
  echo "==> part $SUZATARY_E2E_PART: $(paste -sd' ' <<<"$part")"
  [[ -n "$part" ]] || exit 0
  mapfile -t parted <<<"$part"
  set -- "${parted[@]}"
fi

echo "==> wait budget $(node -e 'import("./web/e2e/budget.ts").then(b => console.log(b.WAIT_MS))') ms (web/e2e/budget.ts)"

if [[ ! -f web/dist/app.js ]]; then
  echo "web-e2e.sh: web/dist/app.js is missing — run scripts/build-web.sh first" >&2
  exit 1
fi

# ⚠️ AND A STALE BUNDLE IS REFUSED, NOT SERVED. This script does
# not build the bundle — `build-web.sh` is the type gate and this is the
# browser gate, deliberately two — so what it serves is whatever `web/dist`
# holds, and the suite could pass against a bundle that predates the page
# under test. A source file newer than the bundle is the whole of what that
# costs to detect.
stale="$(find web/src web/static -type f -newer web/dist/app.js 2>/dev/null | head -1)"
if [[ -n "$stale" ]]; then
  echo "web-e2e.sh: web/dist/app.js is older than $stale — run scripts/build-web.sh first" >&2
  exit 1
fi

if [[ -n "${SUZATARY_FIXTURE_BIN:-}" ]]; then
  fixture_bin="$SUZATARY_FIXTURE_BIN"
elif [[ -n "${SUZATARY_FIXTURE:-}" ]]; then
  fixture_bin="$SUZATARY_FIXTURE/bin/suzatary-dev"
else
  echo "==> building the fixture binary (scripts/app-build.sh --fixture; no \$SUZATARY_FIXTURE supplied)" >&2
  fixture_bin="$(scripts/app-build.sh --fixture)"
fi

# SHARDS. $SUZATARY_E2E_SHARDS (default above) splits the files into that many shards run side by
# side, each this script over its own files with ITS OWN servers, chains, ports, state, secrets
# store and browser profiles, so a shard reads nothing another writes and a file sees what it
# sees in a run of its own; the fixture binary is the one resolved above. Each shard's output is
# printed whole when it ends, and the run fails if any shard does. `SUZATARY_E2E_SHARDS=1` runs
# the files one after another, as the gate did until the scenes that lost their races under
# shards' load were fixed.
#
# The files are dealt longest first to the shard with the least so far, by how long each took
# the last time this machine ran it: its scenes' milliseconds, kept in $durations after every
# sharded run (pagebench, now a bench outside the gate, once took 205 of the suite's 460 seconds on
# 2026-10-10, which nothing in its source says). Until every file has a time there, by size.
# Only the schedule reads that file; every file runs whatever it holds.
durations="${XDG_CACHE_HOME:-$HOME/.cache}/suzatary/e2e-durations"
names=("$@")
if [[ $# -eq 0 ]]; then
  for file in web/e2e/*.test.ts; do names+=("$(basename "$file" .test.ts)"); done
fi
if ((shards > 1 && ${#names[@]} > 1)); then
  ((shards <= ${#names[@]})) || shards=${#names[@]}
  dealt="$(mktemp -d)"
  weights() {
    local name timed timeds=()
    for name in "${names[@]}"; do
      timed="$(awk -v n="$name" '$1 == n { print $2 }' "$durations" 2>/dev/null || true)"
      if [[ -z "$timed" ]]; then
        for each in "${names[@]}"; do echo "$(wc -c <"web/e2e/$each.test.ts") $each"; done
        return
      fi
      timeds+=("$timed $name")
    done
    printf '%s\n' "${timeds[@]}"
  }
  load=() pids=()
  for ((i = 0; i < shards; i++)); do load[i]=0; : >"$dealt/$i.names"; done
  while read -r weight name; do
    least=0
    for ((i = 1; i < shards; i++)); do ((load[i] >= load[least])) || least=$i; done
    load[least]=$((load[least] + weight))
    echo "$name" >>"$dealt/$least.names"
  done < <(weights | sort -rn)
  # Every file dealt once, or nothing runs: a shard handed no file would run every file.
  if [[ "$(sort "$dealt"/*.names | paste -sd' ')" != "$(printf '%s\n' "${names[@]}" | sort | paste -sd' ')" ]]; then
    echo "web-e2e.sh: the files were not dealt to the shards whole" >&2
    exit 1
  fi
  # shellcheck disable=SC2329
  stop_shards() { for pid in "${pids[@]}"; do kill "$pid" 2>/dev/null || true; done; }
  trap 'stop_shards; exit 130' INT
  trap 'stop_shards; exit 143' TERM
  for ((i = 0; i < shards; i++)); do
    mapfile -t mine <"$dealt/$i.names"
    SUZATARY_E2E_SHARDS=1 SUZATARY_E2E_PART="" SUZATARY_FIXTURE_BIN="$fixture_bin" "$0" "${mine[@]}" >"$dealt/$i.log" 2>&1 &
    pids[i]=$!
  done
  failed=0
  for ((i = 0; i < shards; i++)); do
    code=0
    wait "${pids[i]}" || code=$?
    echo "==> shard $((i + 1))/$shards ($(paste -sd' ' "$dealt/$i.names")): exit $code"
    cat "$dealt/$i.log"
    ((code == 0)) || failed=1
  done
  # Each file's scenes, `✔ <file>: <scene> (<ms>ms)`, summed, over what was kept for the others.
  mkdir -p "$(dirname "$durations")"
  { cat "$dealt"/*.log | sed -nE 's/^(✔|✖) ([a-z]+): .* \(([0-9]+)(\.[0-9]+)?ms\)$/\2 \3/p' | sort -u |
      awk '{ ms[$1] += $2 } END { for (n in ms) print n, ms[n] }'
    cat "$durations" 2>/dev/null || true; } | awk '!seen[$1]++' >"$dealt/durations" && mv "$dealt/durations" "$durations"
  rm -rf "$dealt"
  exit "$failed"
fi

# THE FIXTURE'S SIGNING SEEDS (SPEC law 42), test values and nobody's keys: the fixture declares
# their root, and every server, the fixture's own agent run and the suite resolve them by name,
# as secretspec would, from the environment. `app/cli/src/commands/fixture.rs`'s BOX_SEED and
# AGENT_SEED are the same two, and a guard test pins that they are.
export SUZATARY_BOX_KEY="e2e0000000000000000000000000000000000000000000000000000000000b0c"
export SUZATARY_AGENT_KEY="e2e0000000000000000000000000000000000000000000000000000000000a9e"
# The mail's dedup key, a test value beside them, so a mail page reads its badges.
export SUZATARY_DEDUP_KEY="e2e0000000000000000000000000000000000000000000000000000000000d3d"

work="$(mktemp -d)"
# A runtime directory, which every box path needs and a shell outside a login session lacks.
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-$work/run}"
mkdir -p "$XDG_RUNTIME_DIR"
shots="${SUZATARY_E2E_SHOTS:-$work/shots}"
mkdir -p "$shots"
servers=()

# THE SETTINGS FIXTURE'S OWN, ISOLATED secretspec STORE:
# a tapped secret card really shells out to `secretspec set`, so a
# server started with `--settings` gets ITS OWN `$HOME` and a `dotenv`-backed
# provider under `$work` — never this box's real `pass` store. `start` below
# only applies these when `--settings` is one of its own arguments.
secrets_home="$work/secrets-home"
mkdir -p "$secrets_home"
secrets_store="$work/secrets-store.env"

# `start` (below) is called as `read_base="$(start read …)"`, a command
# substitution, which runs the function in a SUBSHELL; `servers+=("$!")`
# inside it would mutate a COPY of the array and leave this loop nothing to
# walk. So `start` drops the PID in a FILE (not subshell-scoped) and the
# caller collects it in ITS OWN scope right after the substitution returns —
# see the spot below.
#
# Each server is launched under `setsid`, so its PID is also its own
# process-group id (a leaf `setsid CMD &` execs straight into CMD rather than
# forking a wrapper — verified against `ps -o pid,pgid`). Killing the NEGATED
# pid kills that whole group, not just the axum process, which matters
# because it is what spawns typst as a subprocess at start-up (CLAUDE.md:
# "compiled as a subprocess") — a plain `kill "$pid"` would leave a
# straggling compile behind it. `INT`/`TERM` are trapped explicitly, not left
# to EXIT alone, so a caller's plain `kill` (no `-9`) is caught the same way
# a normal exit is.
#
# What this trap still cannot do anything about: a hard SIGKILL (the
# harness's 10-minute command timeout escalating past a grace period, or an
# impatient `pkill -9`) is unblockable, so no trap in this or any script runs
# when it lands, and an orphaned server keeps running under init. That path
# is operational, not this trap: run this script
# detached (`setsid nohup scripts/web-e2e.sh & disown`, poll for completion)
# so a caller-side timeout lands on a process this script does not own, and
# prefer plain `kill`/SIGTERM over `-9` when stopping it early.
cleanup() {
  local status=$?
  # A failed run says how each fixture server ended: a server that dies leaves its scenes failing
  # with "Failed to fetch" or no warm markup, and its log is gone with $work.
  if (( status != 0 )) && [[ -d "$work" ]]; then
    {
      echo "==> the fixture servers, as this run (exit $status) ends"
      for pidfile in "$work"/*.pid; do
        [[ -e "$pidfile" ]] || continue
        local name pid
        name="$(basename "$pidfile" .pid)"
        pid="$(cat "$pidfile")"
        if kill -0 "$pid" 2>/dev/null; then echo "  $name (pid $pid): running"; else echo "  $name (pid $pid): EXITED"; fi
        [[ -f "$work/$name.log" ]] && tail -n 15 "$work/$name.log" | sed 's/^/      /'
      done
      journalctl -k --since "-1h" 2>/dev/null | grep -iE "out of memory|killed process" | tail -n 5 | sed 's/^/  kernel: /' || true
    } >&2
  fi
  for pid in "${servers[@]:-}"; do
    [[ -n "$pid" ]] || continue
    kill -- "-$pid" 2>/dev/null || kill "$pid" 2>/dev/null || true
  done
  if [ -n "${SUZATARY_E2E_KEEP:-}" ]; then echo "==> kept $work (SUZATARY_E2E_KEEP)"; else rm -rf "$work"; fi
}
trap cleanup EXIT INT TERM

# A port nothing is listening on. The server is handed one rather than asking
# the kernel for any, because it announces nothing on start-up and this script
# has to know where to look; the window between the check and the bind is a
# race nobody else on this box is running into.
free_port() {
  node -e '
const net = require("net");
const server = net.createServer();
server.listen(0, "127.0.0.1", () => {
  console.log(server.address().port);
  server.close();
});
'
}

# THE SECRETARY'S TWO SUBSTITUTIONS (docs/ARCHITECTURE.md §10.3), both behind
# the same `fixture` feature the whois one is.
#
# `claude -p` cannot be part of a browser suite — it reaches the network, costs
# money and answers differently every time — so the fixture server is handed a
# SCRIPT: entries matched against the last turn (a person's words, or the name
# of the verb that just answered), each carrying the raw text a model would
# have printed. The core's own `Reply::parse` reads it, so the fake and the real
# interpreter run one parser.
#
# And a tap must not reach a transport, so the fixture server RECORDS what would
# have gone into a file instead. That is stronger than not calling it: the scene
# asserts on the message. A scene fails a send on cue by writing `not_sent` or
# `unknown` to "$sent.failures" (`commands::secretary::record`).
script="$work/model-script.json"
cat > "$script" <<'SCRIPT'
[
  {
    "when": "Title this conversation",
    "reply": "A named topic"
  },
  {
    "when": "remember that I take calls after ten",
    "reply": "{\"say\": \"Noted.\", \"calls\": [{\"verb\": \"remember\", \"args\": {\"fact\": \"Ada takes calls after ten in the morning\", \"kind\": \"preference\"}}]}"
  },
  {
    "when": "remember my accountant and my answers",
    "reply": "{\"say\": \"Noted, both.\", \"calls\": [{\"verb\": \"remember\", \"args\": {\"fact\": \"Mira Solberg is Ada's accountant\", \"kind\": \"person\"}}, {\"verb\": \"remember\", \"args\": {\"fact\": \"Answer in three sentences unless asked for more\", \"kind\": \"standing\"}}]}"
  },
  {
    "when": "remember",
    "reply": "{\"say\": \"Remembered. It is under this answer with a Forget beside it.\"}"
  },
  {
    "when": "propose the harrow reply",
    "reply": "{\"say\": \"Here is the reply I would send.\", \"calls\": [{\"verb\": \"propose_email\", \"args\": {\"account\": \"her@example.invalid\", \"to\": \"counsel@example.test\", \"subject\": \"The deed documents\", \"body\": \"Sending the documents today, the last one is scanned.\", \"why\": \"Counsel asked for the two remaining exhibits and the last scan finished today.\", \"sources\": \"mail:a1b2c3d4e5f6, todo:life/file-permit\"}}]}"
  },
  {
    "when": "propose the reply that moves away",
    "reply": "{\"say\": \"Here is the reply.\", \"calls\": [{\"verb\": \"propose_email\", \"args\": {\"account\": \"her@example.invalid\", \"to\": \"counsel@example.test\", \"subject\": \"The reply that moves away\", \"body\": \"This card goes while another card is typed in.\", \"why\": \"Testing that typing stays in its own card when a card above it goes.\", \"sources\": \"todo:life/file-permit\"}}]}"
  },
  {
    "when": "propose the reply being answered",
    "reply": "{\"say\": \"Here is the reply.\", \"calls\": [{\"verb\": \"propose_email\", \"args\": {\"account\": \"her@example.invalid\", \"to\": \"counsel@example.test\", \"subject\": \"The reply being answered\", \"body\": \"A reason is typed into this card while another card goes.\", \"why\": \"Testing that typing stays in its own card when a card above it goes.\", \"sources\": \"todo:life/file-permit\"}}]}"
  },
  {
    "when": "propose the reply opened by its question",
    "reply": "{\"say\": \"Here is the reply.\", \"calls\": [{\"verb\": \"propose_email\", \"args\": {\"account\": \"her@example.invalid\", \"to\": \"counsel@example.test\", \"subject\": \"The reply opened by its question\", \"body\": \"This card opens when Send asks its question.\", \"why\": \"Testing that a card asks its yes with every field on screen.\", \"sources\": \"todo:life/file-permit\"}}]}"
  },
  {
    "when": "propose the reply to dismiss",
    "reply": "{\"say\": \"Here is the reply.\", \"calls\": [{\"verb\": \"propose_email\", \"args\": {\"account\": \"her@example.invalid\", \"to\": \"counsel@example.test\", \"subject\": \"The reply to dismiss\", \"body\": \"This one is dismissed with a reason.\", \"why\": \"Testing that a dismissal leaves the Inbox and shows the reason.\", \"sources\": \"todo:life/file-permit\"}}]}"
  },
  {
    "when": "propose another reply meanwhile",
    "reply": "{\"say\": \"Here is another.\", \"calls\": [{\"verb\": \"propose_email\", \"args\": {\"account\": \"her@example.invalid\", \"to\": \"counsel@example.test\", \"subject\": \"Another reply\", \"body\": \"This one stands while another is dismissed.\", \"why\": \"Testing that a dismissal drops only its own row from the Inbox.\", \"sources\": \"todo:life/file-permit\"}}]}"
  },
  {
    "when": "propose the failing reply",
    "reply": "{\"say\": \"Here is the reply.\", \"calls\": [{\"verb\": \"propose_email\", \"args\": {\"account\": \"her@example.invalid\", \"to\": \"counsel@example.test\", \"subject\": \"The failing reply\", \"body\": \"The first tap on this one fails with nothing sent.\", \"why\": \"Testing that a send that sent nothing can be tried again.\", \"sources\": \"todo:life/file-permit\"}}]}"
  },
  {
    "when": "propose the uncertain reply",
    "reply": "{\"say\": \"Here is the reply.\", \"calls\": [{\"verb\": \"propose_email\", \"args\": {\"account\": \"her@example.invalid\", \"to\": \"counsel@example.test\", \"subject\": \"The uncertain reply\", \"body\": \"The first tap on this one may have gone.\", \"why\": \"Testing that a send that may have gone waits until it is checked.\", \"sources\": \"todo:life/file-permit\"}}]}"
  },
  {
    "when": "propose the reply whose answer is lost",
    "reply": "{\"say\": \"Here is the reply.\", \"calls\": [{\"verb\": \"propose_email\", \"args\": {\"account\": \"her@example.invalid\", \"to\": \"counsel@example.test\", \"subject\": \"The reply whose answer is lost\", \"body\": \"This one goes, and the page never hears back.\", \"why\": \"Testing that a card asks the box what a lost tap did.\", \"sources\": \"todo:life/file-permit\"}}]}"
  },
  {
    "when": "propose_email",
    "reply": "{\"say\": \"Proposed. Tap it when you are happy with the wording.\"}"
  },
  {
    "when": "reply with the approval notice attached",
    "reply": "{\"say\": \"Drafted the reply with the notice attached.\", \"calls\": [{\"verb\": \"propose_email\", \"args\": {\"account\": \"her@example.invalid\", \"to\": \"counsel@example.test\", \"subject\": \"The approval notice\", \"body\": \"Here is the approval notice you asked for.\", \"why\": \"Counsel asked for the approval notice, and the registry sent it.\", \"sources\": \"todo:life/file-permit\", \"attach\": [{\"mail\": \"fixture-approval-notice@example.test\", \"name\": \"approval-notice.pdf\"}]}}]}"
  },
  {
    "when": "draft the notice for the archive",
    "reply": "{\"say\": \"Drafted it for the archive.\", \"calls\": [{\"verb\": \"propose_email\", \"args\": {\"account\": \"her@example.invalid\", \"to\": \"her@example.invalid\", \"subject\": \"The approval notice, for the archive\", \"body\": \"Filed.\", \"why\": \"Ada keeps a copy of every notice.\", \"sources\": \"todo:life/file-permit\", \"attach\": [{\"mail\": \"fixture-approval-notice@example.test\", \"name\": \"approval-notice.pdf\"}]}}]}"
  },
  {
    "when": "propose an inbox tap test changeset",
    "reply": "{\"say\": \"I will propose adding a test todo.\", \"calls\": [{\"verb\": \"propose_changes\", \"args\": {\"chain\": \"life\", \"acts\": [{\"act\": \"add\", \"todo\": \"e2einboxtaptest\", \"body\": \"An inbox tap test todo\", \"rationale\": \"testing that a tap in the Inbox settles its decision\", \"priority\": \"50%\"}], \"why\": \"Testing that tapping a proposal in the Inbox drops the count.\", \"sources\": \"todo:life/file-permit\"}}]}"
  },
  {
    "when": "propose a summary changeset",
    "reply": "{\"say\": \"I will propose two todos and close one of them.\", \"calls\": [{\"verb\": \"propose_changes\", \"args\": {\"chain\": \"life\", \"acts\": [{\"act\": \"add\", \"todo\": \"e2esummaryone\", \"body\": \"A summary test todo\", \"rationale\": \"testing a changeset's summary\", \"priority\": \"40%\"}, {\"act\": \"add\", \"todo\": \"e2esummarytwo\", \"body\": \"Another summary test todo\", \"rationale\": \"testing a changeset's summary\", \"priority\": \"70%\"}, {\"act\": \"complete\", \"todo\": \"e2esummaryone\", \"note\": \"done, testing\"}], \"why\": \"Testing that a changeset opens on its summary.\", \"sources\": \"todo:life/file-permit\"}}]}"
  },
  {
    "when": "propose a test changeset",
    "reply": "{\"say\": \"I will propose adding and closing a test todo.\", \"calls\": [{\"verb\": \"propose_changes\", \"args\": {\"chain\": \"life\", \"acts\": [{\"act\": \"add\", \"todo\": \"e2echangesettest\", \"body\": \"An e2e changeset test todo\", \"rationale\": \"testing the changeset tap\", \"priority\": \"50%\"}, {\"act\": \"complete\", \"todo\": \"e2echangesettest\", \"note\": \"done, testing\"}], \"why\": \"Testing that a changeset shows as a diff and confirms.\", \"sources\": \"todo:life/file-permit\"}}]}"
  },
  {
    "when": "propose_changes",
    "reply": "{\"say\": \"Proposed the changeset. Confirm or deny it.\"}"
  },
  {
    "when": "what about that todo",
    "reply": "{\"say\": \"It is still open. See todo:life/file-permit for the wording.\"}"
  },
  {
    "when": "give me a rendered answer",
    "reply": "{\"say\": \"= A rendered answer\\n\\nThis turn is *typst*, compiled to HTML.\\n\\n#panel[docs/APP.md, typstanswers.]\"}"
  },
  {
    "when": "propose an act",
    "reply": "{\"say\": \"I will ask him to authorise the Microsoft account.\", \"calls\": [{\"verb\": \"propose_act\", \"args\": {\"act\": \"auth\", \"account\": \"her@example.invalid\", \"why\": \"The Microsoft sync has been failing for want of a grant.\", \"sources\": \"todo:life/file-permit\"}}]}"
  },
  {
    "when": "connect my Family calendar",
    "reply": "{\"say\": \"I will ask to add it.\", \"calls\": [{\"verb\": \"propose_act\", \"args\": {\"act\": \"connect\", \"template\": \"ics\", \"params\": [\"Family\"], \"why\": \"Ada asked to add the Family calendar.\", \"sources\": \"briefing:2026-09-02\"}}]}"
  },
  {
    "when": "propose_act",
    "reply": "{\"say\": \"Proposed. Tap Run and the sign-in code will land here.\"}"
  },
  {
    "when": "give me a link",
    "reply": "{\"say\": \"See https://example.test/status for the live status page.\"}"
  },
  {
    "when": "reference",
    "reply": "{\"say\": \"Read it. todo:life/file-permit is the one you mean.\"}"
  },
  {
    "when": "look up the deposit page",
    "reply": "{\"say\": \"Let me read it.\", \"calls\": [{\"verb\": \"web\", \"args\": {\"url\": \"https://www.gov.uk/tenancy-deposit-protection\", \"always\": \"*.gov.uk\", \"why\": \"Ada asked what the deposit page says.\", \"sources\": \"todo:life/file-permit\"}}]}"
  },
  {
    "when": "web",
    "reply": "{\"say\": \"I asked to read it; tap Allow and it will land here.\"}"
  },
  {
    "when": "tag the garden invoices",
    "reply": "{\"say\": \"The garden's invoices are both paid.\", \"calls\": [{\"verb\": \"add_topics\", \"args\": {\"topics\": [\"garden\", \"invoices\"]}}]}"
  },
  {
    "when": "tag the garden alone",
    "reply": "{\"say\": \"The hedge is trimmed in May.\", \"calls\": [{\"verb\": \"add_topics\", \"args\": {\"topics\": [\"garden\"]}}]}"
  },
  {
    "when": "tag the trip alone",
    "reply": "{\"say\": \"The train leaves at nine.\", \"calls\": [{\"verb\": \"add_topics\", \"args\": {\"topics\": [\"trip\"]}}]}"
  },
  {
    "when": "add_topics",
    "reply": "{\"say\": \"Filed under its topics.\"}"
  },
  {
    "when": "take your time",
    "release": "MODEL_RELEASE",
    "reply": "{\"say\": \"An answer that was stopped before it landed.\"}"
  },
  {
    "when": "",
    "reply": "{\"say\": \"Nothing has changed since the last briefing.\"}"
  }
]
SCRIPT
# The file a running "take your time" round waits for before it answers, written by a scene once it has seen the
# round running; a Stop ends the wait too (`capability::pulled`), and the box drops that reply.
release="$work/model-release"
sed -i "s|MODEL_RELEASE|$release|" "$script"
sent="$work/would-have-sent.jsonl"

# THE FAKE `claude` A SIGN-IN RUNS (`commands::serve::sign_ins`): it draws a sign-in link, reads
# one code from its terminal and prints a token for it; any other code ends it.
fake_bin="$work/fake-bin"
mkdir -p "$fake_bin"
cat >"$fake_bin/claude" <<'FAKECLAUDE'
#!/bin/sh
[ "$1" = "setup-token" ] || exit 2
[ -t 0 ] || exit 1
printf 'Browser did not open? Use the url below to sign in\r\n\r\nhttps://claude.com/cai/oauth/authorize?code=true&client_id=e2e&state=e2e\r\n\r\nPaste code here if prompted > '
read -r code
[ "$code" = "e2e-code#e2e-state" ] || { printf '\r\nOAuth error: Invalid code\r\n'; exit 1; }
printf '\r\nYour OAuth token (valid for 1 year):\r\n\r\nsk-ant-oat01-E2Efake-token-never-real\r\n\r\nStore this token securely.\r\n'
FAKECLAUDE
chmod +x "$fake_bin/claude"

# Start one fixture server and print its base URL. It writes a single json line
# and then serves, so the line is the readiness signal for the ADDRESS; `ready`
# below is the readiness signal for the SOCKET.
start() { # name [fixture-chain flags…] [-- serve flags…]
  local name="$1"; shift
  local serve_flags=()
  local chain_flags=()
  while [[ $# -gt 0 ]]; do
    if [[ "$1" == "--" ]]; then shift; serve_flags=("$@"); break; fi
    chain_flags+=("$1"); shift
  done
  set -- "${chain_flags[@]}"
  local port; port="$(free_port)"
  # The CHAIN and the instance file first — data, built by the same event
  # constructors that own the event kinds (app/cli/src/commands/fixture.rs says why),
  # over the REAL repo checkout so `instance.toml`'s `dist` finds the built
  # frontend rather than a guess.
  # `--connections lapsed` writes a sign-in into the state directory, so it is pointed at the
  # server's own.
  local chain_env=(env)
  if [[ " $* " == *" --connections lapsed "* ]]; then chain_env+=("XDG_STATE_HOME=$work/$name/state"); fi
  "${chain_env[@]}" "$fixture_bin" fixture-chain --root "$work/$name" --port "$port" --repo "$(pwd)" "$@" \
    >"$work/$name.json" 2>"$work/$name.log"
  local login; login="$(node -p "require('$work/$name.json').login")"
  # ITS OWN STATE DIRECTORY, every server: what the box keeps there (each store's
  # epoch above all, `suzatary_core::epoch`, keyed by chain name) is the box's,
  # and a fixture writing into this box's own would move the live epochs and
  # every fixture's answers would rest on another's record.
  local serve_env=(env "XDG_STATE_HOME=$work/$name/state")
  # ISOLATED secretspec, ONLY for a server this call asked `--settings` of: a
  # tapped secret card shells out to the real `secretspec set`, and what makes
  # that safe is that its `$HOME` never sees this box's own
  # `~/.config/secretspec/config.toml` (see the `secrets_home` note above).
  #
  # No name is declared here: every name a scene sets is the instance's own, which the server
  # declares in the file it writes under its own state directory, beside the fixture's empty
  # `secretspec.toml`, as a deployment's does beside its read-only code root.
  if [[ " $* " == *" --settings "* ]]; then
    local store="$secrets_store"
    # The links scenes store a link of their own and read it back, so their store is their own.
    if [[ " $* " == *" --connections lapsed "* ]]; then
      store="$work/$name/secrets-store.env"
      : >"$store"
    fi
    # A Google account's server keeps its client and its grant in a store of its own, which its
    # scene reads.
    if [[ " $* " == *" --connected google "* ]]; then
      store="$work/$name/secrets-store.env"
      : >"$store"
    fi
    serve_env+=(
      "HOME=$secrets_home"
      "XDG_CONFIG_HOME=$secrets_home/.config"
      "SECRETSPEC_PROVIDER=dotenv://$store"
    )
  fi
  # A server whose model does not answer connects one by signing in: its `claude` is the fake
  # below, which draws what `claude setup-token` draws, so no scene ever reaches a real login.
  if [[ " $* " == *" --model unanswered "* ]]; then
    serve_env+=("PATH=$fake_bin:$PATH")
  fi
  # A link stored from a card is checked at the tap by syncing its plugin, which reads the
  # reference plugins as the box's server does (scripts/serve-web.sh).
  if [[ " $* " == *" --connections lapsed "* ]]; then
    { [[ -n "${SUZATARY_PLUGINS:-}" ]] || SUZATARY_PLUGINS="$(nix_out suzatary-plugins)"; }
    serve_env+=("SUZATARY_PLUGINS=$SUZATARY_PLUGINS")
  fi
  # Then the REAL server over it, with the whois fixture compiled in, which
  # `.#suzatary` (the shipped derivation; `fixture_bin` is `suzatary-dev`)
  # does not have. See flake.nix for why that is a separate build and not a flag.
  # `setsid` gives it its own process group so cleanup's `kill -- -"$pid"`
  # reaches every subprocess it spawns, not only itself.
  setsid "${serve_env[@]}" "$fixture_bin" serve \
    --instance "$work/$name/instance.toml" \
    --whois-fixture "$login" \
    --model-fixture "$script" \
    --smtp-fixture "$sent" \
    "${serve_flags[@]}" \
    >>"$work/$name.log" 2>&1 &
  # ⚠️ THIS FUNCTION RUNS IN A SUBSHELL — the caller captures its stdout with
  # `$(start …)` — so `servers+=("$!")` here would mutate a COPY of the array
  # and leave `cleanup` an EMPTY `servers` to walk. A file is not
  # subshell-scoped; the caller collects it once the substitution returns.
  echo "$!" >"$work/$name.pid"
  node -p "require('$work/$name.json').base"
}

# READY MEANS THE MARKUP IS WARM, not merely that the socket answers. The
# server compiles the chain's typst in the background at start-up — a cold
# cache degrades to raw source for a moment, which is right for a person and
# wrong for a test that asserts on a rendered <strong>. So this waits for the
# one todo whose body carries markup to come back with its `rich.body` filled.
ready() { # name base
  local name=$1 base=$2 waited=0 started=$SECONDS
  until curl -fsS "$base/api/todo/file-permit" 2>/dev/null \
    | node -e '
let data = "";
process.stdin.on("data", chunk => { data += chunk; });
process.stdin.on("end", () => {
  try {
    const body = JSON.parse(data).entry.authored.rich.body;
    console.log(body === null ? "" : "warm");
  } catch {
    console.log("");
  }
});
' | grep -q warm; do
    sleep 0.2
    waited=$((waited + 1))
    if (( waited > 1200 )); then
      local pid
      pid=$(cat "$work/$name.pid")
      {
        echo "web-e2e.sh: $name ($base) never served warm markup in $((SECONDS - started)) s"
        if kill -0 "$pid" 2>/dev/null; then echo "  its server, pid $pid, is running"; else echo "  its server, pid $pid, has exited"; fi
        echo "  the last answer to /api/todo/file-permit: $(curl -sS -o /dev/null -w '%{http_code} in %{time_total} s' "$base/api/todo/file-permit" 2>&1)"
        echo "  the last 40 lines of $name.log:"
        tail -n 40 "$work/$name.log" | sed 's/^/    /'
      } >&2
      exit 1
    fi
  done
}

echo "==> fixture servers"
# Started together: each one compiles the chain's markup with typst before it
# listens, and doing that twice in sequence is the run's longest wait.
# `--settings` declares a calendar link so the Settings page has a row to read and, on
# `write_base`, to set — see `secrets_home`/`secrets_store` above for the
# isolation that makes running the real `secretspec` safe here.
read_base="$(start read --login e2e-reader@example.invalid --settings)"
# `--reviewed` goes on the READ-ONLY one (§10.6): its triage card is HELD
# by the secretary's review, with the reason recorded. The writer's deployment
# deliberately has NO review, so "the secretary has looked at this" and "nobody
# has" are two states on two servers, and no scene's result depends on file order.
proposed_base="$(start proposed --login e2e-reader@example.invalid --propose --reviewed)"
write_base="$(start write --propose --settings)"
# THE BOOTSTRAP'S SERVER: its model's last call failed, so the box files the card that connects
# a model, and its sign-in runs the fake `claude` above.
bootstrap_base="$(start bootstrap --settings --model unanswered)"
# A CONNECTION'S CARDS' SERVER: a calendar connected with its link unset and its removal standing,
# so their scenes move this instance file and no other server's.
connect_base="$(start connect --settings --connected calendar)"
# THE LINKS' SERVER: a calendar whose link is not stored and a
# Microsoft account that reads its mail and cannot send yet (web/e2e/links.test.ts).
links_base="$(start links --settings --connections lapsed)"
# THE FAKE GOOGLE (`suzatary-dev fake-google`, the cargo tests' own fake) AND THE SERVER THAT SIGNS
# A GOOGLE ACCOUNT IN THROUGH IT (web/e2e/google.test.ts): every request the box makes to Google,
# its consent, its token endpoint and its Calendar API, reaches the fake and nothing else.
google_port="$(free_port)"
"$fixture_bin" fake-google --port "$google_port" >"$work/fake-google.json" 2>"$work/fake-google.log" &
echo "$!" >"$work/fake-google.pid"
until [[ -s "$work/fake-google.json" ]]; do sleep 0.1; done
google_fake="$(node -p "require('$work/fake-google.json').base")"
google_base="$(start google --settings --connected google -- --google-fixture "$google_fake")"
# THE EPOCH'S SERVER (web/e2e/epoch.test.ts): a writer whose lines a scene's tab holds in an
# epoch before the box's, so what that tab resets and pulls again reaches no other file's server,
# origin or state directory.
epoch_base="$(start epoch)"
# THE LINES' SERVER (web/e2e/bigstore.test.ts): a writer whose stores have the live shape, so a scene sees a
# tab read stores of the size the owner's are: $SUZATARY_E2E_LINES generated lines (13,000) over
# $SUZATARY_E2E_CONVERSATIONS conversations (100; 0 halves them instead), $SUZATARY_E2E_RESEALS of their
# slots sealed again (1,500), so about 14,500 line objects; $SUZATARY_E2E_PROPOSALS proposals (400), so
# about 730 inbox objects; and a topic on $SUZATARY_E2E_TOPICS rounds (900).
lines_shape=(
  --lines "${SUZATARY_E2E_LINES:-13000}"
  --conversations "${SUZATARY_E2E_CONVERSATIONS:-100}"
  --reseals "${SUZATARY_E2E_RESEALS:-1500}"
  --proposals "${SUZATARY_E2E_PROPOSALS:-400}"
  --topics "${SUZATARY_E2E_TOPICS:-900}"
)
lines_base="$(start lines "${lines_shape[@]}")"
# THE RESCRUB SERVER (web/e2e/rescrub.test.ts): a writer whose generated lines were sealed before lines
# recorded their rules, $SUZATARY_E2E_RESCRUB_LINES of them (1,500 by default) over
# $SUZATARY_E2E_RESCRUB_CONVERSATIONS conversations (30), so a scene can run `migrate-scrub` under a tab.
rescrub_base="$(start rescrub --lines "${SUZATARY_E2E_RESCRUB_LINES:-1500}" --conversations "${SUZATARY_E2E_RESCRUB_CONVERSATIONS:-30}" --sealed unversioned)"
# THE WIDE SERVER (web/e2e/wide.test.ts): 400 conversations of one line each, so a tab's replica of
# the lines has 400 heads, and every pull names it by a frontier as short as one head's.
wide_base="$(start wide --wide 400)"
# THE APP SHELL'S SERVER (web/src/sw.ts): a writer, so a page there has a
# composer whose draft must outlive a new build, with the build `/api/about`
# names read from a file per request, so a scene can deploy a new build under a
# running tab. Its own server, because a worker it registers would otherwise
# serve every other file's scenes from its cache.
shell_build="$work/shell-build"
echo "e2e-1" >"$shell_build"
# Its own copy of the bundle, so a scene can deploy a build whose files differ.
shell_dist="$work/shell-dist"
cp -r web/dist "$shell_dist"
shell_base="$(start shell -- --build-fixture "$shell_build" --dist "$shell_dist")"
# Collected here, in the MAIN shell — not inside `start`'s subshell — which is
# what makes `cleanup` able to reach them at all (see the note in `start`).
servers+=("$(cat "$work/read.pid")" "$(cat "$work/proposed.pid")" "$(cat "$work/write.pid")" "$(cat "$work/shell.pid")" "$(cat "$work/bootstrap.pid")" "$(cat "$work/connect.pid")" "$(cat "$work/links.pid")" "$(cat "$work/epoch.pid")" "$(cat "$work/lines.pid")" "$(cat "$work/rescrub.pid")" "$(cat "$work/wide.pid")" "$(cat "$work/google.pid")" "$(cat "$work/fake-google.pid")")
ready read "$read_base"
ready proposed "$proposed_base"
ready bootstrap "$bootstrap_base"
ready connect "$connect_base"
ready links "$links_base"
ready google "$google_base"
ready epoch "$epoch_base"
ready lines "$lines_base"
ready rescrub "$rescrub_base"
ready wide "$wide_base"
ready write "$write_base"
ready shell "$shell_base"
echo "    reads    $read_base (a viewer, never an actor — its chain cannot move)"
echo "    proposed $proposed_base (the same viewer, with a card waiting)"
echo "    writes   $write_base"
echo "    shell    $shell_base (a writer, its build read from $shell_build)"

out="$work/bundle"
files=(web/e2e/*.test.ts)
if [[ $# -gt 0 ]]; then
  files=()
  for name in "$@"; do
    if [[ -f "web/e2e/$name.bench.ts" ]]; then files+=("web/e2e/$name.bench.ts"); else files+=("web/e2e/$name.test.ts"); fi
  done
fi

echo "==> esbuild ${#files[@]} test file(s)"
esbuild "${files[@]}" \
  --bundle \
  --platform=node \
  --format=esm \
  --target=es2022 \
  --out-extension:.js=.mjs \
  --outdir="$out"

echo "==> chromium $(chromium --version 2>/dev/null || echo '(not on PATH)')"
# One file at a time. Each launches its own browser and its own profile, and
# four Chromiums racing for one server is a source of flakiness rather than of
# speed on a run this short.
# AND THE AGENT'S WRITER, FOR ONE SCENE (docs/ARCHITECTURE.md §10.6): a scene
# files a triage card through the agent's own verbs and taps it, so it runs the
# fixture binary under node the same way this script does.
E2E_BASE="$read_base" \
E2E_BASE_PROPOSED="$proposed_base" \
E2E_BASE_WRITE="$write_base" \
E2E_BASE_SHELL="$shell_base" \
E2E_BASE_BOOTSTRAP="$bootstrap_base" \
E2E_BOOTSTRAP_INSTANCE="$work/bootstrap/instance.toml" \
E2E_BASE_CONNECT="$connect_base" \
E2E_CONNECT_INSTANCE="$work/connect/instance.toml" \
E2E_BASE_LINKS="$links_base" \
E2E_LINKS_INSTANCE="$work/links/instance.toml" \
E2E_LINKS_STORE="$work/links/secrets-store.env" \
E2E_BASE_GOOGLE="$google_base" \
E2E_GOOGLE_INSTANCE="$work/google/instance.toml" \
E2E_GOOGLE_STORE="$work/google/secrets-store.env" \
E2E_FAKE_GOOGLE="$google_fake" \
E2E_BASE_EPOCH="$epoch_base" \
E2E_EPOCH_INSTANCE="$work/epoch/instance.toml" \
E2E_BASE_LINES="$lines_base" \
E2E_LINES_INSTANCE="$work/lines/instance.toml" \
E2E_BASE_RESCRUB="$rescrub_base" \
E2E_RESCRUB_INSTANCE="$work/rescrub/instance.toml" \
E2E_BASE_WIDE="$wide_base" \
E2E_WIDE_INSTANCE="$work/wide/instance.toml" \
E2E_SECRETS_STORE="$secrets_store" \
E2E_WRITE_LOG="$work/write.log" \
E2E_SHELL_BUILD="$shell_build" \
E2E_SHELL_DIST="$shell_dist" \
E2E_SHELL_INSTANCE="$work/shell/instance.toml" \
E2E_TRIAGE_BIN="$fixture_bin" \
E2E_TRIAGE_INSTANCE="$work/write/instance.toml" \
E2E_WRITE_STATE="$work/write/state" \
E2E_SETTINGS_KEY="SUZATARY_ROUTINE_TOKEN" \
SUZATARY_E2E_SENT="$sent" \
E2E_WRITE_MAIL="$work/write/Mail/her/cur" \
E2E_MODEL_RELEASE="$release" \
SUZATARY_E2E_SHOTS="$shots" \
  node --test --test-concurrency=1 --test-timeout="${SUZATARY_E2E_SCENE_MS:-300000}" ${SUZATARY_E2E_SCENES:+"--test-name-pattern=$SUZATARY_E2E_SCENES"} "$out"/*.mjs
