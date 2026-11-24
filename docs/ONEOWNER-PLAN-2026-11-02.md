# One owner per fact: the device's replica, and the box's readings (2026-11-02)

Status: open. Roadmap `coreskeleton` (stage 3's timing target). Stages 6 to 9
have no roadmap task of their own yet; stage 6's writes meet `localwrites`.

The decision is `oneowner`. Each fact has ONE owner, and everything else is a
view of it, never a copy with a life of its own. On the device the core's
worker owns every replica (skeleton or objects, keys, the prints it holds, the
box's word and the frontier), persisted as the core's own snapshot plus
appended frames (`app/store/src/snapshot.rs`, `web/src/prodrome/replicalog.ts`).
The JavaScript side holds no replica state, and every reader asks the core.

Two rules follow:

1. **A form a build does not read is a miss, never a migration.** The device
   stores the core's bytes under a form number. A build that does not read that
   form pulls again from the box, as a cold load does. The only bytes that
   cannot be pulled again are this device's own unsent objects, kept as object
   literals (an `Own` frame), which every build reads.
2. **A store addressed by content is a cache, not a copy.** A name holds one
   literal, so the `content` store and the browser's HTTP cache can never hold a
   wrong value for a name. They can disagree only about *presence*, and presence
   is the core's to answer, never a JavaScript set's.

## 1. What is still held twice

The names below are the audit's (2026-11-02); the rows not listed here are gone.

| # | Where | Fact | Also in |
|---|---|---|---|
| D8 | IndexedDB `chain`: `objects:<chain>` | each chain's whole replica, as one JSON row (`holdRow`, `heldRow`) | P1, P2, W1, C6, C7 |
| D9 | IndexedDB `chain`: rows of earlier shapes | old builds' rows | none read them |
| L1 | localStorage `suzatary.fold.<chain>` | the box's last fold of a chain, for the first paint (`web/src/app/cache.ts`) | P5, the core's fold |
| W1 | the worker's `chains` (`questions.serve`) | each chain's objects, its policy, a version, from `remote.hold` | D8, P1, P2 |
| C6, C7 | the core's `Chains` set and `Todos` per chain | each chain read into the core | W1 |
| P1 | `sync.source.state().replica` | the open chain's replica | D8, W1 |
| P2 | `sync.others` | every other served chain | D8, W1 |
| P3 | `remote.sent` | the names the worker holds per chain | a *shadow* of W1 |
| P5 | `store`, `boxFolds`, `http.kept`, `primed` | the fold drawn, the box's folds by heads, a fold primed before load | L1, the core's fold |

| Fact | Places | Owner in the target |
|---|---|---|
| F6 a chain's objects | D8, P1, P2, W1, C6, C7 (P3 shadows W1) | the core |
| F8 the box's fold of a chain | L1, P5, and the core's fold of F6 | the core's fold, a view of F6 |
| F9 the form of each store | D9 | the snapshot's form alone (rule 1) |

### F6: a chain's objects

**Owner:** the core, one replica per chain, of the same generic kind as the
lines', the inbox's and the topics' (`app/store/src/replica/`).

**Deleting P1, P2, W1, D8 and P3**: the page's `source` becomes a view of the
core's answers (the fold is already asked of the worker), and every write goes
through the core (LOCALWRITES stage 7 moves authoring into the worker). Readers
to move: `sync.commit`, `restoreReplica`, `heldNow`, `otherHeld`, `pullServed`,
`resetRow`, `signedOwn`, `showFold`'s identity check and its "fresh" test
(`unsynced`, `said`), `orderOf`, and the todo row's "sending" (which reads
`unsynced`). Each becomes a question: `Chain.unsent()`, `Chain.standing(actor,
key)`, `Chain.since()`. `remote.hold` and its delta go: the core holds the chain,
so nothing hands it over.

**Laws:** a refused object leaves every reading with what rests on it (the
existing rollback law, now over the core); an unsent object survives every
reload and every build update (§2.3).

**The type:** the page holds a `Reading` of a chain, tagged with the heads it
was read at, which the page draws and cannot write; the only `Replica` of a
chain is the worker's. `remote.Holding` (a delta between two copies) has no
type left to describe.

### F8: the box's fold

**Owner:** the core's fold of its chain replica; the box's fold is the same
reading made elsewhere.

**Deleting L1, `primed` and `boxFolds`** is possible once the core's warm load of
the chains draws the list within the first-paint budget. L1 exists because the
core was not warm in time. If the chains' warm load is not under the budget, L1
stays, and the plan says so.

**The type:** a fold on screen is a `Reading` carrying its heads; the page's
"fresh" is a comparison of those heads with the box's, a view, not a stored
flag. If stage 8 deletes L1, no fold without heads exists.

### F9: forms

**Owner:** the snapshot's form number alone (rule 1). Storage holds only
frames of a form, and a frame of another form has no path into the model but
`Own`, which is always readable. Deleting D9 is stage 7: the last upgrade of
the `suzatary` database keeps the unsent literals of every row and deletes every
other store and row. From then on the database's schema never changes again; a
form change is a cold load of the stores whose form moved.

## 2. The target, for the chains

### 2.1 The worker as one model

- **Model.** The core's replicas, one per store (the lines, the inbox, the two
  topic stores, each chain): the single source of truth on the device.
- **Messages.** A closed sum: `Restored(snapshot, frames)`, `Pulled(store,
  answer)`, `Fetched(literals)`, `Wrote(store, literal)` (the tab's own object),
  `Heard(store, seq)` (another tab appended), `Compacted(store, generation)`,
  `Erase(keys)`, `Quiet(store)`, `Budget(bytes)`.
- **Update.** One pure function in the core, `update(model, message) -> (model,
  effects)`. The effects are a closed sum too: `Append(store, frame)`,
  `Compact(store, snapshot, blocks to delete)`, `Fetch(names)`, `Ask(store,
  since)`, `Evict(names)`, `Tell(moved)` (to the page).
- **Runtime.** The worker's JavaScript performs each effect (IndexedDB, HTTP, a
  `BroadcastChannel`, `postMessage`) and turns each result back into a message.
  It holds no state of the replica: its queues and lanes are traffic.
- **Views.** Every reading, rendered block, page reading and plan of what to
  keep is computed from the model. Kept readings are a memo keyed by the
  model's version.

**Laws:** the model is `frames.reduce(update, snapshot)`, so a warm load equals
a cold load whenever the frames are the cold load's deltas; an `update` that
moves nothing answers no `Append`; two models that took the same set of join
messages in any order are equal.

### 2.2 Two tabs

Each tab has its own worker and so its own core (the owner, §4). Joins are
appended by either tab at any time and announced on a `BroadcastChannel` named
for the store, which only wakes; the log is the data. Compactions run under a
Web Lock of the same name. **Law:** after any interleaving of two tabs' pulls,
own writes, erasures, compactions and reloads, with any broadcast lost, each
tab's core holds what a core handed the union of their objects cold holds, less
every erased key (`web/test/warmload.test.ts`).

### 2.3 A build update

A new build reads `snapshot/<store>/<its form>`. If none is there, it reads the
log's `Own` frames, pulls the store cold, and appends the own literals again as
its own deltas. A page of the old build still open keeps writing frames of its
form; the new build skips them except `Own`, so nothing either build wrote that
the box lacks is lost. A compaction deletes snapshots of a lower form than its
own.

### 2.4 The tab's own objects (`localwrites`)

- **Never drop an unsent object.** An own object is an `Own` frame in the log
  and part of the replica, so a compaction keeps it in the snapshot, and a form
  change keeps it as a literal.
- **Own nodes.** `core.append_own(store, literal)` computes the node with the
  store's header function and joins it; the node the tab computes equals the
  box's, since it is the same function.
- **Eviction keeps own objects** until the box's word names them: `plan` keeps
  every unsent name.

The exchange that posts unsent objects asks `core.unsent(store)` for literals.
Acceptance arrives as the box's word in the next pull. A refusal marks the
object refused, and `standing` leaves it out of readings.

## 3. The box: one memo by version (stage 9)

The box keeps its stores in one process, so each fact has one owner already
(the store on disk). What it repeats is *work*: readings that are pure functions
of stores, made again on every request.

| Reading | Made again on | A pure function of |
|---|---|---|
| The context `Reading` (`Reading::of` over the digests) | every session opening, the agent's `context` verb, the secretary's `record`, `GET /api/secretary/record`, each round's `Compactor::take`, `waiting` | the heads of lines, digests, memory, topics, topic-aliases |
| The memory read (also `heading`, `GET /api/secretary/memory`) | each of those | the memory's heads |
| The topics reading | each session opening | the heads of topics and topic-aliases |
| The inbox fold (`Fold::of(nodes())`, `GET /api/waiting` among its callers) | each | the inbox's heads |
| One conversation's transcript, per round | each round | the lines' heads and the conversation |
| The fold's rows (`routes::fold`) | every `GET /fold`, `/todo`, `/api/waiting` | the chains' heads and the instant |
| The overview's checks (`serve/settings.rs`) | every `GET /api/overview` | partly heads; partly files that have none |
| Mail's maps (`load_messages`, `load_state`) | every mail list, search, message, thread and queue | the notmuch database's revision and the two files' bytes |

The lines' fold and the digest fold are already memoised over their
content-addressed objects (`memoisedfold`; 2.1.106, 2.1.111); the table is what
the plan found per request on 2026-11-02, to check against the code before
stage 9 starts.

**The target is `store::memoised` alone** (`app/store/src/store/memoised.rs`),
**with its key generalised from one store's heads to a VERSION**: a hash of
what the reading reads, declared as data:

- a set of stores: the name over each store's heads, in a fixed order;
- a set of stores and an argument (a transcript: the lines' heads and the
  conversation; the fold: the chains' heads and the instant, which fixes the
  overview's build key too);
- an outside source with a revision of its own (mail: notmuch's database UUID
  and `lastmod`, from `notmuch count --lastmod`, and the SHA-256 of each JSON
  file).

`Graph::reading` becomes one instance (a version of one store), and `Table`
stays only where its key is not a version (the rendered pages, keyed by
content). Two rules:

- **What has no version is not memoised.** The overview's health registers, git
  reflog and plugin schemas stay per request until one becomes a store. The
  memo never guesses a key.
- **Erasure reaches the memo.** A reading that holds opened words keeps one
  entry (the newest version only), and the sweep that deletes a key file drops
  every entry of the readings that opened it in the same lock. *Law:* after an
  erasure, no memo entry holds a word the erased key opened.

**Where the generic pieces belong.** The replica, the persistence form
(`snapshot`, the frame, the delta) and the memo by version name no Suzatary
concept and belong in the Prodrome, free of any Suzatary type: the replica
beside `store::frontier`, the form in `prodrome_core::snapshot`, the memo in
`prodrome_core::memo` beside `Cache`. The IndexedDB adapter of the log stays in
`web/src/prodrome/`, since the Prodrome ships no TypeScript, and names no store.

## 4. The owner's answers (2026-11-02)

1. **An erasure also deletes the ciphertext prints of the erased slots** from the
   `content` store, in the same transaction as the keys, the blocks and the
   snapshot, and the core drops them too. The HTTP cache, which no page can
   reach, is outside the law. A print that seals an erased slot and a live one
   is kept until every slot of its line is erased, and a line whose print is
   gone reads from its husk.
2. **Each upgrade downloads the skeleton again once**, with no converter of old
   rows (rule 1).
3. **One path for all browsers: one core per tab**, converging through the log.
   There is no `SharedWorker` path.

## 5. Stages

Each stage is one PR, a series of commits, each buildable and tested. Each
DELETES a copy or a recompute, names its law, and measures the lines it removes
(`scripts/size.sh`) and the capped `bigstore` warm first draw
(`web/e2e/bigstore.test.ts`'s last scene with `SUZATARY_E2E_BROWSER_CPU=0.67`,
three warm loads after the snapshot is written; the target is under 2.5 s).

- Stage 0 (dead code and the yardstick): landed in 2.1.55.
- Stage 1 (the core answers what the tab counted): landed in 2.1.55.
- Stage 2 (the core reads its own skeleton): landed in 2.1.56.
- Stage 3a (the core holds the lines' replica; snapshot and log): landed in 2.1.59.
- Stage 3b (the erased slots' prints deleted at an erasure; husks): landed in 2.1.63.
- Stage 3c (the `BroadcastChannel` wake and the Web Lock): landed in 2.1.76.
- Stage 4 (the inbox and the topics into the same form): landed in 2.1.64.
- Stage 5 (rendered blocks beside the replica): landed in 2.1.81.

3. **The lines' timing (`coreskeleton`).** The structure is built (3a to 3c): a
   warm load reads no node row and the tab keeps no copy of the skeleton's rows.
   The target is open: the capped warm first draw under 2.5 s, and the stream
   answered under 1 s from asked.

   *Measured at 2.1.83* (`bigstore`, `SUZATARY_E2E_BROWSER_CPU=0.67`, the wasm
   without `wasm-opt`, three warm loads a run):

   | | first draw | the stream answered, from asked |
   |---|---|---|
   | alone | 3.30, 3.46, 4.10 s | 1.59, 1.89, 2.19 s |
   | in a four-shard run | 3.46, 3.56, 4.00 s | 1.80, 1.99, 2.10 s |

   Where the time goes, from the stream asked: the four stores' log reads end by
   0.11 to 0.20 s; the lines' `Lines.load` takes 0.63 to 0.80 s more (the topics
   wait behind it); reading the stream's prints takes 0.55 to 0.66 s; reading
   the stream, 0.03 to 0.09 s. The page then draws 1.0 to 1.4 s after the
   answer. The prints' share, split at 2.1.84 (marks `lines First scoped`,
   `content First read`, `content First held`): the core's scope of the stream,
   0.18 to 0.28 s; the content's one transaction, 0.09 to 0.50 s, whose first
   request waits on the worker, not on IndexedDB: the worker answers the inbox's
   `Proposals`, asked after the stream, while the stream waits on the read,
   holding it 0.21 to 0.29 s. So the core dominates: its scope of the stream and
   the inbox's reading answered in the stream's wait.

   *Measured at 2.1.239* (the same scene, two runs of three warm loads, the wasm
   as the web build ships it): the first draw 2.61, 2.47, 2.31, 2.27, 2.42, 2.42 s
   (five of six under 2.5 s); the stream answered 1.59, 1.52, 1.50, 1.40, 1.51,
   1.59 s from asked. Not met: from the stream asked, the lines are restored by
   0.53 to 0.73 s, the stream's content is read by 0.82 to 1.11 s, the worker has
   read the stream by 0.92 to 1.29 s, and its answer goes 0.3 s later.
6. **The chains into the core.** The chains become replicas of the generic kind
   in the worker; the page's `source` becomes a view; `remote.hold` and P3, P1,
   P2 and W1 go; writes seal through the core (with or after LOCALWRITES stage
   7). *Laws:* the rollback law over the core; an unsent act survives every
   reload and build update. *Measure:* lines removed (most of `sync.ts`,
   `remote.ts`'s delta), the todo list's warm draw.
7. **The last upgrade.** The next database version (12 and 13 went to stages 4
   and 5) keeps every unsent literal of the `chain` rows and deletes the `chain`
   store and every D9 row. The other upgrade branches went in stages 3a and 3b,
   by the owner's direction of 2026-11-03: each stage deletes the old shapes it
   leaves unused, and a device on an old format takes its stores again. From
   here the schema is fixed.
8. **The local fold goes, if measured.** With the chains warm from the core,
   measure the todo list's first draw from the core alone. If it is within the
   first-paint budget, delete L1, `primed` and `boxFolds`. If not, L1 stays, and
   the PR records the numbers instead.
9. **The box's memo by version.** `store::memoised` takes a version; the context
   `Reading`, the memory and topics readings, the inbox fold, a round's
   transcript and the fold's rows go through it; `Graph::reading` becomes an
   instance; mail's maps are keyed by notmuch's revision and the two files'
   hashes. *Laws:* a memoised reading equals the reading made fresh, over
   generated store histories; after an erasure no memo entry holds a word the
   erased key opened. *Measure:* a session opening's time on the live shape
   before and after; lines removed (the per-call rebuilds).

Stages 6 to 8 follow stage 4's pattern for the chains, in order. Stage 9 is
independent of the rest and can go in any order.
