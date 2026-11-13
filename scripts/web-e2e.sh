#!/usr/bin/env bash
# The web app's behaviour gate: the real frontend in headless Chromium (web/e2e/cdp.ts, no npm)
# against real `suzatary serve` fixture servers over temp chains, asserting on the DOM.
#
# The main servers (others below serve one file each):
#   E2E_BASE           a viewer login: every write is the real 403; nothing waits in its Inbox.
#   E2E_BASE_PROPOSED  the same viewer, with a triage card waiting and held by the review.
#   E2E_BASE_WRITE     the writer login, with a chain and a card of its own.
#
# They run a build with a whois fixture compiled in (`$SUZATARY_FIXTURE`, else
# `scripts/app-build.sh --fixture`); the shipped `.#suzatary` does not contain that code.
#
#   scripts/web-e2e.sh [<file>...]  those files of web/e2e/<file>.test.ts, or every file
#   scripts/web-e2e.sh <bench>    only web/e2e/<bench>.bench.ts, a measurement no gate runs
#   SUZATARY_E2E_LIVE=<instance> scripts/web-e2e.sh pages
#                                 web/e2e/pages.test.ts against a copy of that deployment's data tree
#
# $SUZATARY_E2E_SHOTS: failure screenshots (a temp dir by default). $SUZATARY_SHOTS: the gallery
# `web/e2e/shots.test.ts` files.
# $SUZATARY_E2E_WAIT_MS (default 15000, raised by load average when unset): scales every
# timeout the scenes name (`web/e2e/cdp.ts`); a budget, not a delay.
set -euo pipefail

# $SUZATARY_E2E_SCENES, a pattern, runs only the scenes whose names match it.
# $SUZATARY_E2E_PART=<k>/<n> runs only the k-th of n parts of the files (below); empty, every file.
# $SUZATARY_E2E_TRACE_REQUESTS=1 logs every signed request.
if [[ "${SUZATARY_E2E_TRACE_REQUESTS:-}" == "1" ]]; then
  export RUST_LOG="${RUST_LOG:-info},suzatary::requests=debug"
fi

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
. "$root/scripts/nix-out.sh"

# Each scene fails after $SUZATARY_E2E_SCENE_MS (default five minutes), so a wait that never ends
# cannot hold the gate.

# Shards: default four, or one per core where fewer; a non-count is refused before any build.
shards="${SUZATARY_E2E_SHARDS:-$(( $(nproc) < 4 ? $(nproc) : 4 ))}"
if [[ ! "$shards" =~ ^[1-9][0-9]*$ ]]; then
  echo "web-e2e.sh: SUZATARY_E2E_SHARDS is a count of shards, 1 or more, not '$shards'" >&2
  exit 64
fi

# A part, so separate machines split the suite (scripts/e2e-part.sh).
if [[ -n "${SUZATARY_E2E_PART:-}" ]]; then
  part="$(scripts/e2e-part.sh "$SUZATARY_E2E_PART" "$@")" || exit $?
  echo "==> part $SUZATARY_E2E_PART: $(paste -sd' ' <<<"$part")"
  [[ -n "$part" ]] || exit 0
  mapfile -t parted <<<"$part"
  set -- "${parted[@]}"
fi

# A live run names its scenes: every other scene asks for a fixture server it does not start.
if [[ -n "${SUZATARY_E2E_LIVE:-}" && $# -eq 0 ]]; then
  echo "web-e2e.sh: SUZATARY_E2E_LIVE runs only the scenes named, and none was" >&2
  exit 64
fi

echo "==> wait budget $(node -e 'import("./web/e2e/budget.ts").then(b => console.log(b.WAIT_MS))') ms (web/e2e/budget.ts)"
# What a time bound was measured under, so a log read later can tell a slow run from a slow change.
echo "==> run: SUZATARY_LINES_RATE=${SUZATARY_LINES_RATE:-1} SUZATARY_E2E_BROWSER_CPU=${SUZATARY_E2E_BROWSER_CPU:-all} shards=$shards cores=$(nproc) load=$(cut -d' ' -f1-3 /proc/loadavg)"

if [[ ! -f web/dist/app.js ]]; then
  echo "web-e2e.sh: web/dist/app.js is missing — run scripts/build-web.sh first" >&2
  exit 1
fi

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

# Each shard is this script over its own files, servers, chains, ports, state and profiles, so no
# shard reads what another writes; the run fails if any shard does.
#
# Files are dealt longest first to the least-loaded shard, by each file's last time in $durations
# (by size until every file has one).
durations="${XDG_CACHE_HOME:-$HOME/.cache}/suzatary/e2e-durations"
names=("$@")
if [[ $# -eq 0 ]]; then
  for file in web/e2e/*.test.ts; do names+=("$(basename "$file" .test.ts)"); done
fi
# A file that observes the page's long tasks measures the page's thread, which another shard's
# browser preempts, so it runs after the shards, alone.
alone=() shared=()
for name in "${names[@]}"; do
  if grep -q '"longtask"' "web/e2e/$name.test.ts"; then alone+=("$name"); else shared+=("$name"); fi
done
if ((shards > 1 && ${#shared[@]} > 0 && ${#names[@]} > 1)); then
  ((shards <= ${#shared[@]})) || shards=${#shared[@]}
  dealt="$(mktemp -d)"
  weights() {
    local name timed timeds=()
    for name in "${shared[@]}"; do
      timed="$(awk -v n="$name" '$1 == n { print $2 }' "$durations" 2>/dev/null || true)"
      if [[ -z "$timed" ]]; then
        for each in "${shared[@]}"; do echo "$(wc -c <"web/e2e/$each.test.ts") $each"; done
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
  printf '%s\n' "${alone[@]}" >"$dealt/alone.names"
  if [[ "$(sort "$dealt"/*.names | sed '/^$/d' | paste -sd' ')" != "$(printf '%s\n' "${names[@]}" | sort | paste -sd' ')" ]]; then
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
  if ((${#alone[@]})); then
    code=0
    SUZATARY_E2E_SHARDS=1 SUZATARY_E2E_PART="" SUZATARY_FIXTURE_BIN="$fixture_bin" "$0" "${alone[@]}" >"$dealt/alone.log" 2>&1 || code=$?
    echo "==> alone, after the shards (${alone[*]}): exit $code"
    cat "$dealt/alone.log"
    ((code == 0)) || failed=1
  fi
  # Each file's scenes, `✔ <file>: <scene> (<ms>ms)`, summed; older times kept for the others.
  mkdir -p "$(dirname "$durations")"
  { cat "$dealt"/*.log | sed -nE 's/^(✔|✖) ([a-z]+): .* \(([0-9]+)(\.[0-9]+)?ms\)$/\2 \3/p' | sort -u |
      awk '{ ms[$1] += $2 } END { for (n in ms) print n, ms[n] }'
    cat "$durations" 2>/dev/null || true; } | awk '!seen[$1]++' >"$dealt/durations" && mv "$dealt/durations" "$durations"
  rm -rf "$dealt"
  exit "$failed"
fi

# The fixture's signing seeds, test values and nobody's keys; they equal fixture.rs's BOX_SEED and
# AGENT_SEED (pinned by a test).
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

# Each server runs under `setsid`, so its PID is its process group, and cleanup kills the whole
# group (its typst subprocesses too). A SIGKILL runs no trap and orphans the servers: run this
# detached (`setsid nohup scripts/web-e2e.sh & disown`) and stop it with SIGTERM.
cleanup() {
  local status=$?
  # A failed run reports how each fixture server ended; its log is gone with $work.
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
  # Server logs go beside the failure screenshots, one directory per run: the shards share $shots.
  if (( status != 0 )) && [[ -d "$work" ]]; then
    local kept; kept="$shots/servers/$(printf '%s-' "${names[@]}" | head -c 120)$(basename "$work")"
    mkdir -p "$kept"
    for log in "$work"/*.log; do
      [[ -e "$log" ]] && cp "$log" "$kept/"
    done
    echo "==> the fixture servers' output is kept in $kept" >&2
  fi
  for pid in "${servers[@]:-}"; do
    [[ -n "$pid" ]] || continue
    kill -- "-$pid" 2>/dev/null || kill "$pid" 2>/dev/null || true
  done
  if [ -n "${SUZATARY_E2E_KEEP:-}" ]; then echo "==> kept $work (SUZATARY_E2E_KEEP)"; else rm -rf "$work"; fi
}
trap cleanup EXIT INT TERM

# The server announces no port, so it is handed one. The check-to-bind window is a race.
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

# The model is a script: entries matched against the last turn (a person's words, or the verb
# that just answered), each the raw text a model would print, read by the core's `Reply::parse`.
# A tap reaches no transport: the server records what would have been sent. A scene fails a send
# by writing `not_sent` or `unknown` to "$sent.failures" (`commands::secretary::record`).
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
    "when": "give me an answer to erase",
    "reply": "{\"say\": \"= An answer to erase\\n\\nThese words are *said once*, then erased.\"}"
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
    "when": "ada@example.test",
    "reply": "{\"say\": \"I will ask to connect it.\", \"calls\": [{\"verb\": \"propose_act\", \"args\": {\"act\": \"connect\", \"template\": \"google\", \"params\": [\"ada@example.test\"], \"why\": \"Ada asked to connect this Google account.\", \"sources\": \"briefing:2026-09-02\"}}]}"
  },
  {
    "when": "put the dentist in my calendar",
    "reply": "{\"say\": \"Here is the event.\", \"calls\": [{\"verb\": \"propose_act\", \"args\": {\"act\": \"calendar_event\", \"account\": \"her@example.invalid\", \"title\": \"Dentist\", \"start\": \"2026-12-01T14:00:00Z\", \"end\": \"2026-12-01T14:45:00Z\", \"why\": \"Ada asked for it.\", \"sources\": \"briefing:2026-09-02\"}}]}"
  },
  {
    "when": "her@example.invalid",
    "reply": "{\"say\": \"I will ask to sign it in with Google.\", \"calls\": [{\"verb\": \"propose_act\", \"args\": {\"act\": \"connect\", \"template\": \"google\", \"params\": [\"her@example.invalid\"], \"why\": \"Ada asked to connect this Google account.\", \"sources\": \"briefing:2026-09-02\"}}]}"
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
# A "take your time" round waits for this file, which a scene writes; a Stop ends the wait too.
release="$work/model-release"
sed -i "s|MODEL_RELEASE|$release|" "$script"
sent="$work/would-have-sent.jsonl"

# The fake `claude` a sign-in runs (`commands::serve::sign_ins`).
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
  # `--connections lapsed` writes a sign-in into the state directory: point it at the server's.
  local chain_env=(env)
  if [[ " $* " == *" --connections lapsed "* ]]; then chain_env+=("XDG_STATE_HOME=$work/$name/state"); fi
  "${chain_env[@]}" "$fixture_bin" fixture-chain --root "$work/$name" --port "$port" --repo "$(pwd)" "$@" \
    >"$work/$name.json" 2>"$work/$name.log"
  local login; login="$(node -p "require('$work/$name.json').login")"
  # Every server has its own state directory: writing into this box's would move the live epochs.
  local serve_env=(env "XDG_STATE_HOME=$work/$name/state")
  # A tapped secret card really runs `secretspec set`, so each `--settings` server gets its own
  # `$HOME` and dotenv store under `$work`, never this box's `pass`: secretspec rewrites a dotenv
  # file unlocked, so two servers sharing one can leave it unparseable.
  if [[ " $* " == *" --settings "* ]]; then
    local store="$work/$name/secrets-store.env" home="$work/$name/home"
    : >"$store"
    mkdir -p "$home"
    serve_env+=(
      "HOME=$home"
      "XDG_CONFIG_HOME=$home/.config"
      "SECRETSPEC_PROVIDER=dotenv://$store"
    )
  fi
  # The fake `claude` on PATH, so no scene ever reaches a real login.
  if [[ " $* " == *" --model unanswered "* ]]; then
    serve_env+=("PATH=$fake_bin:$PATH")
  fi
  # A stored link is checked at the tap by syncing its plugin.
  if [[ " $* " == *" --connections lapsed "* ]]; then
    { [[ -n "${SUZATARY_PLUGINS:-}" ]] || SUZATARY_PLUGINS="$(nix_out suzatary-plugins)"; }
    serve_env+=("SUZATARY_PLUGINS=$SUZATARY_PLUGINS")
  fi
  # The bash between `setsid` and the server logs how it ended (137 is SIGKILL, e.g. the OOM
  # killer; 134 an abort). Cleanup kills the group, bash too, so a normal stop logs nothing.
  # shellcheck disable=SC2016
  setsid "${serve_env[@]}" bash -c '"$@"; echo "==> suzatary serve ended: exit $?"' serve \
    "$fixture_bin" serve \
    --instance "$work/$name/instance.toml" \
    --whois-fixture "$login" \
    --model-fixture "$script" \
    --smtp-fixture "$sent" \
    "${serve_flags[@]}" \
    >>"$work/$name.log" 2>&1 &
  # A subshell: `servers+=` here would be lost, so the PID goes to a file.
  echo "$!" >"$work/$name.pid"
  node -p "require('$work/$name.json').base"
}

# Ready means the markup is warm: the server compiles typst in the background and serves raw
# source until then, so this waits for file-permit's `rich.body`.
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

# The scene files, one at a time (Chromiums racing for one server are flaky), against `e2e_env`.
e2e_env=()
run_scenes() {
  local out="$work/bundle" files=(web/e2e/*.test.ts) name
  if [[ ${#names_given[@]} -gt 0 ]]; then
    files=()
    for name in "${names_given[@]}"; do
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
  env "${e2e_env[@]}" SUZATARY_E2E_SHOTS="$shots" \
    node --test --test-concurrency=1 --test-timeout="${SUZATARY_E2E_SCENE_MS:-300000}" ${SUZATARY_E2E_SCENES:+"--test-name-pattern=$SUZATARY_E2E_SCENES"} "$out"/*.mjs
}
names_given=("$@")

# THE LIVE SMOKE: one server over a COPY of the deployment's data tree (`fixture-live`), as a viewer
# that acts as nobody, with its own state, home and an empty secrets store, so nothing a scene does
# reaches the data tree.
if [[ -n "${SUZATARY_E2E_LIVE:-}" ]]; then
  [[ -f "$SUZATARY_E2E_LIVE" ]] || { echo "web-e2e.sh: SUZATARY_E2E_LIVE names no instance file: $SUZATARY_E2E_LIVE" >&2; exit 64; }
  echo "==> a copy of the data tree of $SUZATARY_E2E_LIVE"
  port="$(free_port)"
  "$fixture_bin" fixture-live --from "$SUZATARY_E2E_LIVE" --root "$work/live" --port "$port" --repo "$(pwd)" \
    >"$work/live.json" 2>"$work/live.log"
  live_home="$work/live-home"
  mkdir -p "$live_home/.config"
  : >"$work/live-secrets.env"
  # shellcheck disable=SC2016
  setsid env -u SUZATARY_BOX_KEY -u SUZATARY_AGENT_KEY -u SUZATARY_DEDUP_KEY \
    "XDG_STATE_HOME=$work/live-state" "XDG_CACHE_HOME=$work/live-cache" "HOME=$live_home" \
    "XDG_CONFIG_HOME=$live_home/.config" "SECRETSPEC_PROVIDER=dotenv://$work/live-secrets.env" \
    bash -c '"$@"; echo "==> suzatary serve ended: exit $?"' serve \
    "$fixture_bin" serve --instance "$work/live/instance.toml" --dist "$(pwd)/web/dist" \
    --whois-fixture "$(node -p "require('$work/live.json').login")" \
    --model-fixture "$script" --smtp-fixture "$sent" \
    >>"$work/live.log" 2>&1 &
  echo "$!" >"$work/live.pid"
  servers+=("$(cat "$work/live.pid")")
  live_base="$(node -p "require('$work/live.json').base")"
  until curl -fsS "$live_base/api/tip" >/dev/null 2>&1; do
    kill -0 "$(cat "$work/live.pid")" 2>/dev/null || { echo "web-e2e.sh: the live copy's server ended:" >&2; tail -n 40 "$work/live.log" >&2; exit 1; }
    sleep 0.2
  done
  echo "    live     $live_base (a viewer of the copy)"
  e2e_env=("E2E_BASE=$live_base" "E2E_BASE_PROPOSED=$live_base" "E2E_BASE_WRITE=$live_base")
  run_scenes
  exit
fi

echo "==> fixture servers"
# Started together: each compiles the chain's markup before it listens.
read_base="$(start read --login e2e-reader@example.invalid --settings)"
# Only this one has a review, so reviewed and unreviewed are two servers' states.
proposed_base="$(start proposed --login e2e-reader@example.invalid --propose --reviewed)"
write_base="$(start write --propose --settings)"
# Its model's last call failed, so the box files the card that connects a model.
bootstrap_base="$(start bootstrap --settings --model unanswered)"
connect_base="$(start connect --settings --connected calendar)"
# web/e2e/links.test.ts: an unstored calendar link and a Microsoft account that cannot send yet.
links_base="$(start links --settings --connections lapsed)"
# web/e2e/google.test.ts: every request the box makes to Google reaches the fake and nothing else.
google_port="$(free_port)"
"$fixture_bin" fake-google --port "$google_port" >"$work/fake-google.json" 2>"$work/fake-google.log" &
echo "$!" >"$work/fake-google.pid"
until [[ -s "$work/fake-google.json" ]]; do sleep 0.1; done
google_fake="$(node -p "require('$work/fake-google.json').base")"
google_base="$(start google --settings --connected google -- --google-fixture "$google_fake")"
epoch_base="$(start epoch)"
# web/e2e/bigstore.test.ts: stores of the live size (each count overridable; conversations 0
# halves them instead).
lines_shape=(
  --lines "${SUZATARY_E2E_LINES:-13000}"
  --conversations "${SUZATARY_E2E_CONVERSATIONS:-100}"
  --reseals "${SUZATARY_E2E_RESEALS:-1500}"
  --proposals "${SUZATARY_E2E_PROPOSALS:-400}"
  --topics "${SUZATARY_E2E_TOPICS:-900}"
  --todos "${SUZATARY_E2E_TODOS:-600}"
)
lines_base="$(start lines "${lines_shape[@]}")"
# web/e2e/rescrub.test.ts: lines sealed before lines recorded their rules, for `migrate-scrub`.
rescrub_base="$(start rescrub --lines "${SUZATARY_E2E_RESCRUB_LINES:-500}" --conversations "${SUZATARY_E2E_RESCRUB_CONVERSATIONS:-30}" --sealed unversioned)"
# web/e2e/wide.test.ts: 400 heads.
wide_base="$(start wide --wide 400)"
# The service worker's server: its build is read from a file per request, so a scene can deploy
# under a running tab. Its own, or its worker would serve other files' scenes from cache.
shell_build="$work/shell-build"
echo "e2e-1" >"$shell_build"
# Its own copy of the bundle, so a scene can deploy a build whose files differ.
shell_dist="$work/shell-dist"
cp -r web/dist "$shell_dist"
shell_base="$(start shell -- --build-fixture "$shell_build" --dist "$shell_dist")"
# Collected in the main shell, so `cleanup` can reach them.
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


# E2E_TRIAGE_BIN lets a scene file a triage card through the agent's own verbs.
e2e_env=(
  "E2E_BASE=$read_base"
  "E2E_BASE_PROPOSED=$proposed_base"
  "E2E_BASE_WRITE=$write_base"
  "E2E_BASE_SHELL=$shell_base"
  "E2E_BASE_BOOTSTRAP=$bootstrap_base"
  "E2E_BOOTSTRAP_INSTANCE=$work/bootstrap/instance.toml"
  "E2E_BASE_CONNECT=$connect_base"
  "E2E_CONNECT_INSTANCE=$work/connect/instance.toml"
  "E2E_CONNECT_STORE=$work/connect/secrets-store.env"
  "E2E_BASE_LINKS=$links_base"
  "E2E_LINKS_INSTANCE=$work/links/instance.toml"
  "E2E_LINKS_STORE=$work/links/secrets-store.env"
  "E2E_BASE_GOOGLE=$google_base"
  "E2E_GOOGLE_INSTANCE=$work/google/instance.toml"
  "E2E_GOOGLE_STORE=$work/google/secrets-store.env"
  "E2E_FAKE_GOOGLE=$google_fake"
  "E2E_BASE_EPOCH=$epoch_base"
  "E2E_EPOCH_INSTANCE=$work/epoch/instance.toml"
  "E2E_BASE_LINES=$lines_base"
  "E2E_LINES_INSTANCE=$work/lines/instance.toml"
  "E2E_BASE_RESCRUB=$rescrub_base"
  "E2E_RESCRUB_INSTANCE=$work/rescrub/instance.toml"
  "E2E_BASE_WIDE=$wide_base"
  "E2E_WIDE_INSTANCE=$work/wide/instance.toml"
  "E2E_WRITE_LOG=$work/write.log"
  "E2E_SHELL_BUILD=$shell_build"
  "E2E_SHELL_DIST=$shell_dist"
  "E2E_SHELL_INSTANCE=$work/shell/instance.toml"
  "E2E_TRIAGE_BIN=$fixture_bin"
  "E2E_TRIAGE_INSTANCE=$work/write/instance.toml"
  "E2E_WRITE_STATE=$work/write/state"
  "E2E_SETTINGS_KEY=SUZATARY_ROUTINE_TOKEN"
  "SUZATARY_E2E_SENT=$sent"
  "E2E_WRITE_MAIL=$work/write/Mail/her/cur"
  "E2E_MODEL_RELEASE=$release"
)
run_scenes
