# Who pays a first read of the conversations (`firstread`), 2026-11-16

A design note with measurements. It closes the kept-fold series, whose plan this note replaces.
The question is no longer how to make the first read cheaper to repeat, but which processes
repeat it, and which of them need not.

## The kept-fold series, closed

- **Step 0 landed** (#523, release 2.1.151). An ask of a husked seal's words prints no binding.
  On the deployment, a new process's first read fell from a median of 5,409 ms to 4,635 ms.
  What is left is spread: objects 800, words 800, nodes 570, hold 500, close 480, dag 450,
  fold 240 ms, and no part is over a second.
- **The skeleton was measured and closed** (#524, unmerged). After #523 the kept seal plan
  saves about 2% of a cold read, and reading a kept skeleton back (680–840 ms) costs more than
  the parts it replaces (580–590 ms). The read-back also needs the graph read anyway, for the
  seals. On a scratch store at the deployment's shape, the cold read is 2.73–2.79 s and the
  skeleton path is 2.9–3.1 s.
- **Decided:** no reading of a store is kept on disk. A later design that holds the lines by
  position, with seals opened lazily, would start from #524's branch.
- **Still open** from the old plan, and not this note's subject: the server's start spends
  5.8 s in `notice` (every declared secret resolved afresh) and 1.6–5.0 s in `erase.texts`
  (every text swept). Each is a pull request of its own: name its parts until they sum to the
  whole, then remove what a start does not need.

## The measurements

All figures were taken in this container (4 cores) on a scratch store shaped like the
deployment's, built as for #524: `suzatary-dev fixture-chain --lines 15000 --conversations 143
--reseals 10400 --depth 3`, then `suzatary secretary migrate-scrub --apply` ("the sweep
destroyed 31200 key(s)"). That gives 46,201 line objects, 31,200 of them husks. The binary is
a release build of this branch. Each verb was run three times as a new process, except
`secretary review` and `mcp`, which were run twice. Processor time is user plus system. A
first read on the deployment is about 1.7 times this container's (4.6 s against 2.7 s).

| process | wall | processor | its first read (the line) |
| --- | --- | --- | --- |
| `secretary list` | 2.80–2.96 s | 3.4–3.5 s | conversations, 2,614–2,768 ms |
| `secretary review --json`, no card standing | 2.80–2.93 s | 3.4–3.5 s | conversations, 2,583–2,731 ms |
| `mcp`, its first `context` call | 3.31–3.33 s (the call) | 4.0–4.1 s (the process) | digest, 3,202–3,234 ms |
| `verify` (every store) | 12.8–13.1 s | 13.8–14.2 s | stores, 12,433–12,784 ms |
| `verify`, the Prodrome's rule fixed (below) | 3.40–3.70 s | 4.5–5.0 s | stores, 3,109–3,430 ms |

**`verify` is the largest single cost, and it is not the fold.** It reads every store whole
from the files, and the Prodrome's fsck of the lines (`Dag::verify`) takes 9.7–10.2 s of its
12.6–13.1 s. All of that is one rule, `Dag::deps_findings`: 9.3 s over the lines' 46,200
changes. For each dependency of a change it builds the whole ancestor closure of that
dependency's parents, then asks whether another dependency lies beneath it. A change with
one dependency has no other to compare, yet still builds the closure, so the cost grows with
the square of a conversation's length. Building the closures only for a change of two or more
dependencies (measured in a throwaway copy of the Prodrome, not committed) brings the rule to
15 ms and `verify` to 3.4–3.7 s, with the same findings (all 11 stores OK). The Prodrome is
not changed here. One caveat for its owners: in a store with a cycle, the old rule also
reported each dependency as beneath itself, and the frozen conformance vectors must say
whether that finding is expected.

The parts of `verify` with the rule fixed: objects 320–419 ms, dag 297–332, the fsck's
re-read and history 490–580, `dag.verify` 330–400, the transcripts' check 157–176, and
`stray` (the husks no key opens) 885–918.

**`mcp` pays more than the fold.** Its first `context` call reads the digest, which checks
that the lines read whole before folding them, so the graph is read outside the fold. The
digest's reading is now under the same line, so the line says what the agent's process paid.
A second `context` call in the same process takes 70–80 ms.

## Who opens the store

Every unit, from `nix/services.nix` and the scripts it runs, and every verb, down to the first
read of the lines. "Needs" is what the process uses from the store. The last column says
whether it could (a) not open the store at all, (b) ask the running server, or (c) must read
the files itself.

| process | run by, how often | needs | first read | (a), (b) or (c) |
| --- | --- | --- | --- | --- |
| `heartbeat`, `sync`, `gate` | every triage pass, 288 a day | nothing | none | (a): they never open it |
| `safe-push.sh`, quiet | every pass with nothing to commit | nothing | none | (a): it exits before `verify` |
| `verify` | `safe-push.sh` on every pass that commits | every store whole, as files | stores | (c) |
| `verify --husked` | the push after a sweep husked lines | the husked lines and the committed wholes | husked lines | (c) |
| `mcp` | `claude -p` on every busy pass | the digest (`context`, `recall`), and the `triage` conversation when it files a card | digest, or conversations | (b) for the digest; (c) to file a card |
| `ran` | every busy pass | nothing | none | (a) |
| `secretary review` | every busy pass | every conversation (`note_unreadable`), today's review round, the standing cards | conversations | (a) when no card stands; else (b), with (c) as fallback |
| `secretary sweep` | once a UTC day | today's sweep round, the open list | conversations | (b), with (c) as fallback |
| `serve` | always on | the whole reading, held from its start (3.7 s) | conversations | the holder |
| `mail idle` | always on | nothing | none | (a) |
| `plugin sync` | every 15 minutes | nothing | none | (a) |
| `vault-backup.sh` | daily | runs no binary | none | (a) |
| the nightly gate's live half | daily | the data tree, in test processes; `push-dry` runs `verify` | stores | (c) |
| `secretary list`, `show`, `readings`, `digest census`, `reviews` | a person, rarely | the whole reading or one conversation | conversations | (b), with (c) as fallback |
| `secretary dismiss`, `brief`, `topics`; `coding poll` | a person, rarely | one conversation, and a write | conversations | (b), with (c) as fallback |
| `secretary migrate-scrub`, `prune-lines` | a person, after a change of rules | the store and its keys, rewritten | conversations | (c) |

Who must work when the server is down:

- **The triage pass**, whole. Mail triage is the box's main job, and the server is down for
  every deploy and every crash. A caller that asks the server keeps its own read as the
  fallback.
- **`verify`**, always from the files. It vouches for the bytes that git commits, and a
  server's reading is no evidence of what the files hold.
- **The recovery verbs** (`migrate-scrub`, `prune-lines`, `verify`). They change keys and files
  under every reader, and they are what a person runs when the server will not start.

## What a day costs

On the deployment's figures (each first read about 4.6 s), a day costs:

- one `verify` per commit, at about 13 s on a store of this shape;
- two first reads per busy pass (`mcp` and `review`), about 9 s;
- one per sweep, about 4.6 s;
- one per server start, 3.7 s;
- one per person's command.

The quiet passes (most of the 288) pay none. The deployment's journal now gives the counts
(below).

## The line that says it

Under a unit, a process's first read is one journal entry when it ends. Systemd sets
`JOURNAL_STREAM` for a unit and its children. The entry names the verb, what was read and the
milliseconds, as `MESSAGE` and in fields of its own:

```
secretary review: first read of the conversations, 2731 ms
SUZATARY_VERB=secretary review  SUZATARY_READ=conversations  SUZATARY_MS=2731
```

- The verb is the subcommands' names, never an argument's value. No command of the tree takes
  an external subcommand (`verb::tests::the_journal_names_the_verb_and_no_argument`).
- What was read is one of four constants: `conversations`, `digest`, `stores`,
  `husked lines`.
- It is said on the journal's native socket, not on stderr, because the pass captures or
  redirects its verbs' stderr: `gate`'s goes to `sync.log`, `review`'s is cut to three lines,
  and `mcp`'s goes to the model CLI.
- It is said when the first read ends, not when the process ends. `gate`, `ran`, a paused
  round and every failure leave by `process::exit`, which runs nothing after it. `serve` and
  `mail idle` never end.
- A process a person runs says nothing: the journal records the sender's command line, and a
  person's can hold words (`secretary dismiss --reason …`).
- A process that reads nothing says nothing, so a missing line is the answer for it.

Who paid today, and how many seconds in all:

```
journalctl --user -t suzatary --since today -o cat \
  | awk -F'[:,]' '{ms=$NF; sub(/ ms/, "", ms); t[$1]+=ms; n[$1]++}
      END {for (v in t) printf "%s: %d reads, %.1f s\n", v, n[v], t[v]/1000}'
```

One thing to check on the first busy pass: whether an `mcp:` entry appears. It does only if the
model CLI passes `JOURNAL_STREAM` on to the MCP server it starts. If `hourly-triage: …` shows
verbs called and no `mcp:` entry appears, the environment was not passed on.

## The server's first fold, alone

The server is the holder, so its first reading should be ready soon after it starts. At
release 2.1.152 (#527) the start itself takes 2.9 s. Then three pieces of work started at once
on the deployment's two cores:

- the conversations' first fold (`waiting`);
- the notice, whose secret-store call alone is 4.4 s;
- the summaries' sweep, which folds the same lines.

The fold took 7,538 ms there (`waiting.hold` 2,774 against about 500 in a quiet read), so the
first reading was ready about 10.4 s after the process started.

**The order now** (this pull request): after the listener, the first fold runs alone. The
summaries' and the uploads' sweeps, and the first notice, start once it holds. The law it
keeps: every card is filed and every text swept as before, only later.

- The sweeps find the fold warm, rather than folding again or waiting on its lock.
- `moved` (a model moved) is a `Notify` fired with `notify_one`, which keeps a permit, so a
  move during the first fold still wakes the notice once it listens.
- The hourly notice's first tick is the one that runs once the fold holds.
- The compactor's workers (`compaction::started`) are not moved. They start before the
  listener, as before, and fold through the same lock.

On the scratch store, with `serve` pinned to two cores (`taskset -c 0,1`), three runs of each
order were made. The fixture declares no secret, so the notice is 0–5 ms here and the
deployment's contention cannot be shown.

| | the first fold (`waiting`) | the summaries' sweep |
| --- | --- | --- |
| before | 1,959–2,175 ms | 2,463–2,611 ms |
| after | 1,869–1,927 ms | 419–461 ms |

The deployment's next start says what the order gives against the store call: `waiting`'s
`whole` and its `hold`.

## The recommendation, in order of what it saves a day

1. **The Prodrome: answer `deps_findings` without a closure for a change of one dependency**
   (or from the ancestry bitsets of `prodrome_core::registers`). It saves about 9 s per commit
   on a store of this shape, and every commit runs `verify`. It is a change to the Prodrome,
   so it is a request to its owners, with the figures above. It is the first item, and it is
   not a change in this repository, so this pull request stops at the note and the line.
2. **`secretary review` stops reading when no card stands.** It saves one first read (about
   4.6 s) per busy pass with nothing standing; the review's JSON already prints `standing`, so
   the journal says how many such passes there are. Today the CLI runs `note_unreadable`, which
   folds every conversation, before the review asks the inbox whether any card stands. The
   change is to ask the inbox first, and to return before any read of the lines when nothing
   stands. `Reviewed.stored` becomes a sum (`Nothing` or a round), since there is no round to
   show then. The existing review laws (`app/cli/tests/review.rs`) hold it: the same lines are
   written, and the same counts are printed. One behaviour moves: the page that names
   unreadable conversations waits for a pass with a card standing. The owner decides whether
   that is acceptable. It is about 60 to 100 lines.
3. **`mcp` asks the server for the digest.** It saves 3.2 s here (about 5 s on the deployment)
   per busy pass. The agent's record would gain one field: a holder of the digest, whose only
   interpreter asks `GET /api/secretary/record` over loopback as `Local`, and folds the files
   when that does not answer. This changes the fence's type, though not what the agent can
   learn, since it reads the same record today. So it is the owner's decision, with the
   security review below. Filing a card still reads and writes the lines in the agent's own
   process.
4. **The server runs the review and the sweep when it answers.** The triage script would ask
   it as `call` does, signed by the root key and recorded as the operator's (→ `localnode`). If
   the server does not answer, the script runs the verb as today. This is a new write route
   and model rounds in the server's process, so it is larger. It saves the review's first read
   on every busy pass that item 2 does not, and the sweep's once a day.
5. **A person's read-only verbs ask the server** (`secretary list`, `show`, `readings`,
   `digest census`, `reviews`), each falling back to its own read. This saves seconds of a
   person's wait, not of the box's day.

## The law for a caller that asks

**The answer is the one the files give.** The server's reading is taken at a store's heads.
`Objects::with` reads the store's record on every call and folds again when the heads moved.
`Opened::read` feeds what the graph holds that the fold does not, and drops every key a line
killed, in any process. So an answer at heads `H` is the files' reading at `H`.

- **The answer names its heads.** The caller reads the store's record first, which is cheap
  (`record 0`). It takes the answer only when the answer's heads are the ones it read.
  Otherwise it reads the files itself. A write between the two reads costs one read of the
  caller's own, never a stale answer.
- **When the server does not answer, the caller reads the files**, with the same answer and
  the cold read's cost. That covers a refused connection, a bounded wait that ends, a 5xx,
  and a 426 from a server of another build. A caller never fails because the server is down.
- **What a caller may assume across a restart of the server:** nothing. Each answer stands
  alone and names its heads. A restarted server reads cold on its first request, and that
  request's caller may give up waiting and read the files itself.
- **The standing test** (AGENTS.md §8: a security boundary and a law), over the fixture
  server. Generated writes and erasures happen between asks, by the server and by another
  process, and a restart of the server comes between two asks. For every ask, the caller's
  answer equals the plain reading of the files: the `secretary readings` fingerprint
  (`routes::lines_readings`). With the server stopped, the answer is the same, read locally.

**The security engineer.** A read goes over loopback, unsigned, and the gate answers it as
`Local` (→ `localnode`, `addressnotidentity`), which reads and never writes. Nothing here adds
a door that writes as `Local`. A write (item 4) is the operator's signed request, as `call`
makes today. Item 3 gives the agent's process a reader of what it already reads. It gains no
key, no write and no route but one fixed GET. The journal line holds no word and no name.

## The one general piece

A process takes a reading from whoever already holds it, else computes it. In Prodrome terms
this is a store's reading served by its holder. It belongs in the Prodrome, beside `memo`, as
`prodrome_core::store::served`, free of any Suzatary type:

```rust
/// What the holder of a store's reading answers for the heads a caller read.
pub enum Held<R> { At(R), Moved(Heads), Unreachable }

/// The process that holds a store's reading.
pub trait Holder<R> { fn reading(&self, at: &Heads) -> Held<R>; }

/// Where an answer came from.
pub enum Source { Holder, Read }

/// The holder's answer where it is at `at`, else `read()`.
pub fn taken_or_read<R>(holder: &impl Holder<R>, at: Heads, read: impl FnOnce() -> R) -> (R, Source);
```

Its one law is the law above: for every holder, `taken_or_read(h, at, read).0 == read()`. The
Prodrome would test it with an in-process holder (a fold that lags, moves and forgets). This
repository keeps only the interpreter that asks over HTTP, and the readings it serves. Nothing
in the Prodrome changes in this pull request.

## Order of the work

1. This note and the line, in this pull request, so the deployment's journal gives the counts.
2. The Prodrome's `deps_findings`, asked of its owners with the figures above.
3. After a day of the journal: item 2 if the review passes with no card standing are many,
   then items 3 and 4 in the order the journal's seconds put them.
