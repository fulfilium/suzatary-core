# One owner per fact: the device's replica, and the box's readings (2026-11-02)

The owner, 2026-11-02: "when such bugs keep happening that's a sign of spaghetti
code somewhere and simplifying principles fixes things."

This is the design pass for the todo `coreskeleton`, which the owner reframed as
a simplification. It holds an audit and a plan. Nothing is built: the only
measurement is the count of production lines in §1.4, taken with the rule
`scripts/size.sh` uses. The stages in §6 complete the todo; this document does
not. Paths are relative to the repository root, read at `ce0be2f` (2.1.51).

## 0. The pattern and the principle

The device holds one fact in several places, and each of the recent bugs was two
of those places disagreeing:

- **2.1.50, "no conversation named …"** (#304, `staleconversation`). `content`'s
  memory in the core's worker counted prints as held that the core had dropped.
  `Lines.clear` emptied the core, and the JavaScript set was not emptied with it.
- **10-26, keys kept after a reseal** (2.1.24). The tab's key rows kept the keys
  of seals that a reseal had replaced, because the tab decided which keys were
  dead in a sweep of its own that ran only once every line's content was held.
  The core's worker opened every old seal and scrubbed it again on every load,
  and the secretary page took about 43 s to draw. The fix after it added a
  second implementation of the box's rule in TypeScript (`linesync.retiredBy`,
  beside `transcript::Retired`), so the rule is still computed in two places.
- **Train 173, an old-shape inbox row outlived its upgrade.** A row an earlier
  build wrote stayed on the device beside the rows of the new shape.
- **#286 nearly added a device memo table** as one more copy of the opened
  words, with its own invalidation. It was dropped because it cost more than it
  saved; it would also have been one more place to keep in step.

**The principle.** Each fact has ONE owner, and everything else is a view of it,
never a copy with a life of its own. A view is computed from the owner and
thrown away; nobody writes to it, and nothing has to keep it in step. On the
device the core's worker owns every replica (skeleton or objects, keys, the
prints it holds, the box's word and the frontier). Its persistence is the core's
own serialization, one snapshot plus appended deltas (§3.1). The JavaScript side
holds no replica state, and every reader asks the core.

**The shape is the page's, extended.** The page already follows the Elm
architecture: a closed `PageState`, a pure `step`, an idempotent `draw` over one
`Source`, and the law that a state is `events.reduce(step, initial)` (WEB-SPEC
§3.7). The worker and the device's storage do not: their state is spread over
JavaScript closures, IndexedDB rows and the core, each moved by its own code.
The target gives the worker ONE model (the core's replicas), ONE pure update
(in the core), and makes everything else a runtime that performs effects or a
view that holds no state of its own (§3.0). For each duplicated fact, §2.10
names the type that makes the disagreement unrepresentable, since each bug of
the list was a state the types allowed and the system never should.

Two rules follow, and the rest of the plan applies them:

1. **A form a build does not read is a miss, never a migration.** The device
   stores the core's bytes under a form number. A build that does not read that
   form pulls again from the box, as a cold load does. The only bytes that
   cannot be pulled again are this device's own unsent objects, and they are
   kept as object literals, the Prodrome's print, which never changes for a
   shipped kind (§3.1). So the device has no more upgrade branches and no more
   carries.
2. **A store addressed by content is a cache, not a copy.** A name holds one
   literal, so the `content` store and the browser's HTTP cache can never hold a
   wrong value for a name. They can disagree only about *presence*, and presence
   is the core's to answer, never a JavaScript set's.

## 1. The audit: every place device state lives today

### 1.1 The table

"Writer" and "reader" name the module that writes or reads it. "Also in" names
every other place that holds the same fact. A row marked *view* is computed from
another row and thrown away; *shadow* means a second record of what another
place holds, kept in step by hand, which is the shape of every bug in §0.

**IndexedDB `suzatary`, version 10** (`web/src/prodrome/objects.ts:84-111`)

| # | Where | Fact | Writer | Reader | Invalidated by | Also in |
|---|---|---|---|---|---|---|
| D1 | `lines`: `head` | the lines' generation, the box's words, refusals, frontier, unsent names, row format | `noderows.land`, `reset`, `reformat` | `noderows.restore`, `deviceRow`, `prime`, `settle` | a landing, a reset, a later epoch | J1 (`held`), J5 |
| D2 | `lines`: `node/<hash>` | each skeleton node | `noderows.land` (new nodes), `writeAll` | `noderows.readAll` (runs of 250) | a reset or later epoch clears the store | J1, J2 after landing, C1, D5 |
| D3 | `lines`: `key/<name>` | each key's bytes | `land`, `keepPiece` | `readAll`, `prime` and `settle` (names only) | `forget` (a purge), a reset | J1 (`keys`), C2, D5 |
| D4 | `lines`: `piece/<n>` | the nodes of a pull not yet landed | `noderows.keepPiece` | `restore` → `pull.pending` | `dropPieces` after a landing | J2 |
| D5 | `snapshots`: `lines` | the core's `Given` (nodes and keys) as postcard, keyed by head, node count and key names | `noderows.settle` after 30 s of quiet or when hidden | `noderows.prime` | deleted by every transaction that writes D1-D4 | D2, D3, C1, C2 |
| D6 | `content` | each line object's literal, by name | `content.keep` (fetches and prefetches) | `content.stored` | `content.keepOnly` (eviction) | the HTTP cache (`immutable`, a year), C3; presence also in J3 |
| D7 | `inbox`, `topics`, `topic-aliases`: head and `node/<hash>` | each store's objects with their literals, and its head | `noderows` through `objectsync` | `noderows.restore` | a landing, a reset | J1 of each store, C5 |
| D8 | `chain`: `objects:<chain>` | each chain's whole replica, as one JSON row | `holdRow` (`sync.commit`, pulls), `alterRow`, `dropRow` | `heldRow` (`restoreReplica`, `otherHeld`) | a join, a reset | P1, P2, W1, C6, C7; P3 shadows W1 |
| D9 | `chain`: `pages/<name>`, `store/<name>`, `store/conversations`, `store/rendered-owed` | rows of earlier shapes, and a mark that blocks must be cleared | old builds; the upgrade writes the mark | `objectsync.carried`, `linesync.retire`, `linesync.owedClear` | each carry deletes its row | D7, the lines |

**IndexedDB `suzatary-rendered`, version 3** (`web/src/app/rendered.ts:128-140`)

| # | Where | Fact | Writer | Reader | Invalidated by | Also in |
|---|---|---|---|---|---|---|
| R1 | `rows` | Typst's HTML by answer key, 20 MB, least recently used first | the worker's `blocks.join` | the worker's `blocks.get` | `forget` (a purge), `clear`, eviction | W3, P6, the `rich` HTML inside L1 and P5 |
| R2 | `rows`: `struck:<key>` | the marks of erased blocks, kept for good | `forget` | `join`, which refuses a struck key | nothing: never deleted | none, but it grows with every erasure |

**IndexedDB `suzatary-keys`** holds the device's signing key (`signer.ts`). It
has one owner, and it is not replica state. It stays as it is.

**localStorage and Cache Storage**

| # | Where | Fact | Writer | Reader | Invalidated by | Also in |
|---|---|---|---|---|---|---|
| L1 | `suzatary.fold.<chain>` | the box's last fold of a chain, slimmed, with when it was fetched | `sync.keep` after a fetch | `sync.restoreFold`, for the first paint | the next fetch | P5, and the core's own fold of D8 |
| L2 | `suzatary.ui.*`, `suzatary.zone`, `suzatary.pushtoken`, `suzatary.wiping` | settings, the zone, the push token, the wipe mark | `ui`, `ownerzone`, `pushtoken`, `wipe` | the same | each its own | none (each has one owner) |
| L3 | Cache Storage `suzatary-shell-<build>`, `suzatary-current` | each build's files; the build last heard | the service worker | the service worker | the sweep, a wipe | none (each keyed by build) |

**The core's worker, in memory** (`questions.serve`, `storeset`, `engine`, `pull`, `linesync`, `content`)

| # | Where | Fact | Writer | Reader | Invalidated by | Also in |
|---|---|---|---|---|---|---|
| J1 | `engine.row` per store (`locked.Row`) | the JavaScript replica: nodes or objects, keys, and the head | `engine.take` from restores, landings and purges | `linesync` (`touched`, `retiredBy`, `keep.plan`, `lines.nodesOf`, `naming`, `holds`, `lacking`), `pull` (`holding`, `sinceOf`), `purge`, `storeset.readOf` | each landing, purge or reset | D1-D3 or D7, C1, C2, C5 |
| J2 | `pull.pending` per store | the pieces of a pull not yet landed | `pull.keep` | `pull.holding` | a landing, `startOver` | D4 |
| J3 | `content.memory`, `content.moves`, `content.asked` | which prints the core holds; a counter of moves; which names a reading asked for | `content.take`, `keepOnly`, `has` | `content.has`, `holds`, `version`, `read`; `engine.over`; `linesync.keeping` | nothing tells it when the core drops a print | *shadow* of C3: the bug of #304 |
| J4 | `linesync.telling.told` | the row the core was last told, or that it holds a snapshot | `took`, `reader.load`, `holds`, `lost` | `took` (the delta it sends), `printed` | each `took` | *shadow* of C1 and C2 |
| J5 | `noderows.confirmed`, `primed`, `owed` | the row the device last confirmed holding and its printed head; the snapshot key this load found; the snapshot owed | `confirm`, `prime`, `owe` | `deviceRow`, `restore`, `settle` | a head that moved | *shadow* of D1-D3 |
| J6 | `linesync.planned`, `ahead`; `engine.purged` | the plan of one skeleton; the skeleton last prefetched; the row last purged | each landing | the eviction, the prefetch, the purge | a new skeleton | *view* of J1 |
| C1 | the core's `Outlines` (in `Lines`, `HeldReplica`) | each skeleton node, parsed | `Lines.nodes`, `load`, `clear` | `Lines.chosen`, `stream`, `excluded` | `nodes(drop)`, `clear` | D2, D5, J1 |
| C2 | the core's `KeyRing` (in `HeldLines`) | each key's bytes | `Lines.keys`, `load`, `clear` | every reading that opens words | `keys(drop)`, `clear` | D3, D5, J1 |
| C3 | the core's `HeldLines.prints` and its memo `Cache` | each print held, decoded, and the words opened from it | `Lines.hold`, `hold_admitted` | every reading | `hold(drop)`; a key let go forgets what it opened at once | D6 (bytes); J3 (presence) |
| C5 | the core's `Proposals`, `Memberships`, `TopicAliases` | the inbox and topic stores as the core reads them | `storeset.readOf`, rebuilt from every object, sent as JSON, on each move | the Inbox, the topics, a conversation's dress | each move of J1 | D7, J1 |
| W1 | `questions.serve`'s `chains` | each chain's objects as literals by name, its policy, a version | `hold` from `remote.hold` | `todosOf`, `setNow` | each holding | D8, P1, P2, C6 |
| C6 | the core's `Chains` set, and `inSet` | each chain read into the core | `setNow` | every reading that prices | a chain's version moving | W1 |
| C7 | the core's `Todos` per chain | one chain read into the core | `todosOf` | the list and a todo's page | a chain's version moving | W1 |
| W2 | `questions.readings` (four kept), `inbox` (`last.of`) | readings made, with their price memos | each reading | the same question asked again | a new version or holding | *view* of C3, C5, C6 |
| W3 | `rendered.device`'s table, 2,048 entries | rendered HTML over R1 | `join` | `get` | `forget`, eviction | R1, P6 |

**The page, in memory**

| # | Where | Fact | Writer | Reader | Invalidated by | Also in |
|---|---|---|---|---|---|---|
| P1 | `sync.source.state().replica` | the open chain's replica | `sync.commit`, `restoreReplica` | `heldNow`, `showFold`, `signedOwn`, the todo rows ("sending") | each commit, a chain switch | D8, W1 |
| P2 | `sync.others` | every other served chain, read once from D8 and joined with pulls | `otherHeld`, `pullEach` | `heldNow` | a chain switch, a reset | D8, W1 |
| P3 | `remote.sent` | the names the worker holds per chain | `remote.hold` | `remote.hold` (the next delta) | a refused holding makes the next one whole | *shadow* of W1: J3's shape again |
| P4 | `sync.kept`, `setOf`, `HeldSet`, `core.keptChains`, `chainsOf` | a page-side core `Chains` | nothing: no caller | nothing | — | dead code |
| P5 | `store` (the fold on screen), `boxFolds`, `http.kept` (24 paths), `primed` | the fold drawn; the box's folds by heads; ETag answers; a fold primed before load | each fold or fetch | the todo list | each fold or fetch | L1, the core's fold |
| P6 | `blocks.store`, 2,048 entries | rendered HTML seen from the page | `blocks` over the worker's answers | the page's drawing | the worker's `Forgot` and `Cleared` | R1, W3 |
| P7 | `intentsync.source` | the asks in flight | each `http.act` | the pages | a reload forgets it | none (LOCALWRITES replaces it) |

### 1.2 What the table says

Nine facts are held in more than one place:

| Fact | Places | Owner in the target |
|---|---|---|
| F1 the lines' skeleton | D2, D4, D5, J1, J2, C1 (J4 and J5 shadow it) | the core (C1) |
| F2 the lines' keys | D3, D5, J1, C2 (J4 shadows it) | the core (C2) |
| F3 which prints the core holds | C3, J3, and the presence of names in D6 | the core (C3) |
| F4 a store's head (generation, the box's words, frontier, refusals, unsent) | D1 and D7's heads, J1 | the core |
| F5 the inbox's and topics' objects | D7, J1, C5 | the core |
| F6 a chain's objects | D8, P1, P2, W1, C6, C7 (P3 shadows W1) | the core |
| F7 rendered HTML | R1, W3, P6, inside L1 and P5 | R1, as a cache by content name |
| F8 the box's fold of a chain | L1, P5, and the core's fold of F6 | the core's fold, a view of F6 |
| F9 the form of each of these | D9, the upgrade's branches, `noderows.Format`, `linesync.FORMAT`, `objectsync.FORMAT`, the snapshot's `FORM` | the snapshot's form alone (rule 1) |

Two shapes repeat, and each is one thing:

- **A shadow of what the worker holds.** J3 (`content.memory`) and P3
  (`remote.sent`) are the same structure: a JavaScript set that records what was
  handed to the other side, so that the next handover sends only the
  difference. Each is correct only while nothing on the other side drops
  anything without saying so. J3 broke when `Lines.clear` did. P3 survives
  because the worker never drops a chain object on its own. J4 and J5 are the
  same shape over rows. All four go for one reason: the owner answers what it
  holds, so nothing needs to remember what it was told.
- **A rule computed twice.** `linesync.retiredBy` (40 lines) re-implements
  `transcript::Retired` over the skeleton's headers, and `lines.ts` and
  `keep.ts` re-implement over `locked.Row` the questions that `Outlines` already
  answers (which conversation a node is in, its closure, its recency). The bug
  of 10-26 was the two `retired` readings disagreeing. Once the core holds the
  skeleton, each of these is one core question.

And one lifecycle repeats: **two persistence paths for one replica.**
`objects.ts` keeps a chain as one JSON row (`holdRow`, `alterRow`, `dropRow`),
and `noderows.ts` keeps every other store as a row per node, each with its own
join, reset, refusal and carry rules. Both store a replica of the same Prodrome
schema shape; in the target one path persists every store (§3.1).

### 1.3 The old-shape code

Each item below exists only to read bytes an earlier build wrote:

- `objects.ts` upgrade branches for versions 1-2, 3-7 and 8 (`carry`, `framed`,
  `carried`, `unsentNone`, 70 lines with their comments);
- `objectsync.carried`, rows of the inbox and topics from before version 9;
- `linesync.retire` (the plaintext `store/conversations` from before the lines)
  and `linesync.owedClear` (the `store/rendered-owed` mark from before version
  5);
- `noderows.reformat` and the `Format` type, and the three format numbers;
- `rendered.ts`'s upgrade, which drops its store at each version.

The upgrade tests that pin them (`web/e2e/upgrade.test.ts`, `upgrade3.test.ts`,
`upgrade8.test.ts`) are scenes of the same carries.

### 1.4 The measurement: what the device code weighs today

Production lines of TypeScript (no blank or comment lines, the rule of
`scripts/size.sh`), at `ce0be2f`. The web tree holds 20,940 such lines.

| File | Lines | Holds |
|---|---|---|
| `prodrome/noderows.ts` | 344 | D1-D5, J5 |
| `prodrome/pull.ts` | 297 | J2, the restore and landing |
| `prodrome/linesync.ts` | 278 | J4, J6, `retiredBy`, `touched`, the carries |
| `prodrome/content.ts` | 214 | J3, the fetcher, D6 |
| `prodrome/objects.ts` | 212 | D8, D9, the upgrade |
| `prodrome/storeset.ts` | 177 | J1 of each store, C5's rebuilds |
| `prodrome/engine.ts` | 142 | J1 |
| `prodrome/objectsync.ts` | 74 | D7's carry |
| `prodrome/remote.ts` | 74 | P3 |
| `prodrome/keep.ts` | 55 | the plan over J1 |
| `prodrome/locked.ts` | 50 | the row's join |
| `prodrome/pieces.ts`, `take.ts`, `purge.ts`, `handover.ts` | 112 | pieces, taking, purging, handing over |
| `prodrome/sync.ts` | 470 | P1, P2, P4, P5, L1 |
| `prodrome/replica.ts` | 341 | the replica's algebra in TypeScript |
| `app/rendered.ts`, `blocks.ts`, `richkeep.ts` | 329 | R1, R2, W3, P6 |
| `app/cache.ts`, `primed.ts` | 72 | L1, the primed fold |

The warm first draw of the capped `bigstore` scene (the whole browser on 0.67
of a core, LAZYSYNC §16) is 2.77 to 5.11 s on master, with the stream answered
1.48 to 2.70 s after it was asked. The target is under 2.5 s.

## 2. Each duplicated fact: its owner, and what deleting each copy takes

### F1, F2, F4: the lines' skeleton, keys and head

**Owner:** the core's replica of the lines, `HeldReplica` grown into the
store's whole replica: `Outlines`, `KeyRing`, the prints, and now the head
(generation, the box's words, the frontier, refusals, the unsent names).

**Deleting J1 for the lines** means moving its readers into core questions:

| Reader today (TypeScript) | Becomes (one question of `Lines`) |
|---|---|
| `linesync.touched` (which conversations a move touched) | `Lines.take` answers the conversations its change moved |
| `linesync.retiredBy` | `Lines.retired()`, read with `transcript::Retired`'s rule over the outlines; the TypeScript copy goes |
| `keep.recent`, `keep.plan` | `Lines.plan(budget)` |
| `lines.nodesOf`, `naming`, `holds` | `Lines.lacking(scope)` (the names of a scope whose prints are not held) and `Lines.holds(id)` |
| `pull.holding`, `sinceOf` | `Lines.since()` (the heads and frontier a pull asks from) |
| `locked.owed`, `purge.forgotten` | `Lines.owed()` (the keys an erasure owes, with the blocks each opened) |

**Deleting J2 and D4** means the core holds a pull's pieces: an unlanded piece is
part of the core's replica, kept apart from what readings read, and written to
the log (§3.1) so a cut pull resumes from it.

**Deleting J4 and J5** follows once nothing hands the core a row: the core is the
row. `telling`, `confirmed` and the snapshot key go.

**Deleting D1-D5** is stage 3: the snapshot and log replace the rows. The upgrade
that adds the log's store deletes the lines' rows in the same transaction. The
lines are written by the box alone today, so nothing in them is the device's
own. If LOCALWRITES stage 2 has landed first, the lines hold the tab's sealed
lines too, and the upgrade keeps each unsent one as a literal in the log
(§3.1).

**Laws:** a warm load equals a cold load (the law of `web/test/warmload.test.ts`,
widened from "skeleton, keys and prints handed" to the whole replica, head
included); two tabs converge (§3.6); `Lines.retired` equals the box's `retired`
for the same objects.

### F3: which prints the core holds

**Owner:** the core. `HeldLines.prints` is the only record.

**Deleting J3** (`content.memory`, `moves`, `asked`): `engine.over` asks
`Lines.lacking(scope)` instead of filtering by `content.has`; the count of moves
becomes the core's own version; and "what a reading asked for", which the
eviction keeps, becomes the core's own set of names a reading read, kept beside
its prints and cleared with them. The fetcher keeps its lanes and queues, which
are traffic, not state: it is handed names and hands back literals.

**Law:** for any interleaving of loads, clears, landings and evictions, a name is
fetched exactly when the core lacks its print (the #304 law, stated over the core
alone).

### F5: the inbox's and topics' objects

**Owner:** the core, one replica per store, of the same generic kind as the
lines' (§5). These stores carry their literals in their nodes, so the replica
holds objects rather than nodes.

**Deleting J1 and D7** for them: `storeset.readOf`, which rebuilds `Proposals`,
`Memberships` and `TopicAliases` from every object, sent as JSON, at each move,
becomes a reading of the replica the core already holds. `objectsync.ts` goes
whole. The topic write (`storeset.write`) seals into the core's replica, which
LOCALWRITES stage 1 already plans.

**Law:** the inbox and topics read the same from a warm load and from a cold one.

### F6: a chain's objects

**Owner:** the core, one replica per chain, of the same kind.

**Deleting P1, P2, W1, D8 and P3**: the page's `source` becomes a view of the
core's answers (the fold is already asked of the worker), and every write goes
through the core (LOCALWRITES stage 7 moves authoring into the worker). Readers
to move: `sync.commit`, `restoreReplica`, `heldNow`, `otherHeld`, `pullServed`,
`resetRow`, `signedOwn`, `showFold`'s identity check and its "fresh" test
(`unsynced`, `said`), `orderOf`, and the todo row's "sending" (which reads
`unsynced`). Each becomes a question: `Chain.unsent()`, `Chain.standing(actor,
key)`, `Chain.since()`. `remote.hold` and its delta go: the core holds the chain,
so nothing hands it over. P4 is dead today and goes in stage 0.

**Laws:** a refused object leaves every reading with what rests on it (the
existing rollback law, now over the core); an unsent object survives every
reload and every build update (§3.7).

### F7: rendered HTML

**Owner:** R1, a cache addressed by content name (`memo::name` of a function and
its source). A key names one value, so it cannot be stale. It can only outlive
an erasure, which is a privacy fact, not a staleness fact.

**What changes:**

- R1 moves into the `suzatary` database, so an erasure deletes the key, its
  blocks and the old snapshot in one transaction (§3.3) instead of three steps
  over two databases with a failure path between each.
- R2 (the `struck:` marks, kept for good) goes. A mark exists so that an answer
  asked before an erasure cannot land after it. In the target the core refuses
  to join a block whose key it has forgotten, so the mark is a question to the
  core and needs no row.
- P6, the page's tier, goes. The page asks the worker for the blocks of one
  draw in one message. W3 stays as the worker's in-memory view of R1.
  *Measure:* that the draw of the stream is no slower without P6.

### F8: the box's fold

**Owner:** the core's fold of its chain replica; the box's fold is the same
reading made elsewhere.

**Deleting L1, `primed` and `boxFolds`** is possible once the core's warm load of
the chains draws the list within the first-paint budget. L1 exists because the
core was not warm in time. This is the last stage and depends on a
measurement: if the chains' warm load is not under the budget, L1 stays, and
the plan says so.

### F9: forms

**Owner:** the snapshot's form number alone (rule 1).

**Deleting D9 and every carry** (§1.3) is stage 7, the last upgrade of the
`suzatary` database: it keeps the unsent literals of every row (chains, topics,
and lines if LOCALWRITES stage 2 has landed) and deletes every other store and
row. From then on the database's schema never changes again. Its stores hold
opaque bytes under form numbers, and a form the build does not read is a cold
load.

### 2.10 The types that make each disagreement unrepresentable

Deleting a copy removes today's disagreement; a type keeps it from coming back.
Each row says what the bug's state was, and which type of the target cannot
hold it.

| Fact | The state the types allow today | The type in the target |
|---|---|---|
| F1, F2, F4 | `locked.Row` in JavaScript holding nodes and keys the core does not (or the other way round); `telling.told: Row \| Snapshot` and `noderows.confirmed: … \| null` recording what the other side holds | No JavaScript type holds a node, a key or a head. The worker's JavaScript holds a `Replica` handle (the wasm object) with no readable field, so there is no value to go stale; every reading is a question to it. `Told`, `confirmed` and `Primed` are deleted with the facts they shadowed. |
| F3 | `content.memory: Set<string>` naming a print the core dropped (#304) | The fetcher's input is `Lacking`, a type only the core's answer mints (`Replica.lacking(scope)`), and its output is `Admitted`, which only the core takes. JavaScript has no set of names to consult, so it cannot believe a print is held. |
| F5 | `Proposals` rebuilt from a JSON copy of `engine.row`'s objects | The inbox and topics are `Replica` handles of the same type; a reading takes a handle, never a list of objects. |
| F6 | `source.state().replica`, `others`, `remote.sent` and the worker's `chains` each holding a chain | The page holds a `Reading` of a chain, tagged with the heads it was read at, which the page draws and cannot write; the only `Replica` of a chain is the worker's. `remote.Holding` (a delta between two copies) has no type left to describe. |
| F7 | A block kept after its key was forgotten, guarded by `struck:` rows | A block enters `blocks` only as a `Kept` value the core mints from an answer whose key it still holds; after the erasure the core mints none for that key, so no row can be written. |
| F8 | A fold in `localStorage` drawn as current | A fold on screen is a `Reading` carrying its heads; the page's "fresh" is a comparison of those heads with the box's, which is a view, not a stored flag. If stage 8 deletes L1, no fold without heads exists. |
| F9 | A row of an old shape beside rows of the new one (train 173) | Storage holds only `Frame { form, kind, bytes }`, and decoding a frame answers `Decoded = Current(Delta) \| Own(Literal) \| OtherForm`. `update` takes a `Delta`; an `OtherForm` has no path into the model, and an `Own` is always readable. No TypeScript decoder of an old row shape (`Head`, `Kept`, `Spoken`, `Standing`, `replicaRow`, `carried`, `framed`) remains to be wrong. |

The erasure law gets a type too: in the core a slot already reads as
`Open(words) | Erased | Missing`, and a forgotten key's words are dropped from
the memo at once; the target adds that a compaction is the only effect that
removes bytes, so "a key or an erased slot's print deleted from memory but not
from storage" cannot happen: both are effects of one message, `Erase`, performed
in one transaction.

## 3. The target's write and read paths

### 3.0 The worker as one model

The worker becomes the page's shape one level down:

- **Model.** The core's replicas, one per store (the lines, the inbox, the two
  topic stores, each chain). This is the single source of truth on the device.
- **Messages.** A closed sum: `Restored(snapshot, frames)`, `Pulled(store,
  answer)`, `Fetched(literals)`, `Wrote(store, literal)` (the tab's own object),
  `Heard(store, seq)` (another tab appended), `Compacted(store, generation)`,
  `Erase(keys)`, `Quiet(store)`, `Budget(bytes)`.
- **Update.** One pure function in the core, `update(model, message) -> (model,
  effects)`. The effects are a closed sum too: `Append(store, frame)`,
  `Compact(store, snapshot, blocks to delete)`, `Fetch(names)`, `Ask(store,
  since)`, `Evict(names)`, `Tell(moved)` (to the page).
- **Runtime.** The worker's JavaScript performs each effect (IndexedDB, HTTP, a
  `BroadcastChannel`, `postMessage`) and turns each result back into a
  message. It holds no state of the replica: its queues and lanes (the
  fetcher's) are traffic, and dropping them loses no fact.
- **Views.** Every reading (a conversation, the stream, the Inbox, a chain's
  list), every rendered block, the page's `PageState` readings, and the plan of
  what to keep are computed from the model. W2's kept readings stay as a memo
  keyed by the model's version, which is a view.

**Laws, in WEB-SPEC's words:** the model is `frames.reduce(update, snapshot)`,
so a warm load equals a cold load whenever the frames are the cold load's
deltas; an `update` that moves nothing answers no `Append`; and two models that
took the same set of join messages in any order are equal (the joins form a
semilattice), which is the two-tab law of §3.6.

### 3.1 The one form: a snapshot and a log

Per store, the `suzatary` database holds:

- `snapshot/<store>/<form>`: the core's replica printed (`snapshot::snapshot`,
  as today, of the whole replica: the head, the nodes or objects, the keys, the
  pending pieces and the unsent names). Names are printed as 32 bytes, not 64
  hex characters, which LAZYSYNC §16 estimates halves the 6.0 MB.
- `log/<store>/<seq>`: deltas appended since that snapshot, `seq` an
  auto-increment key. Each entry is a frame: `{ form, kind, bytes }`. Every
  `kind` but one is the core's postcard of a delta (nodes or objects joined,
  keys joined, the box's word, a piece kept, a landing). The exception is
  `Own`: its bytes are an object literal this device wrote, the Prodrome's
  print, which every build reads.
- `content/<name>`: literals by name, as today (rule 2).
- `blocks/<key>`: rendered HTML, moved from `suzatary-rendered` (§2, F7).

**Why a snapshot and a log, and not one row per node.** Every write the replica
makes is a join, which is idempotent and commutative, so appending a delta is
always safe, from any tab, in any order. A down step (an erasure, a reset, a
later epoch) cannot be a delta, because the bytes it removes are in earlier
entries. So a down step is a compaction: one transaction that writes a new
snapshot and deletes the log. That is the only way bytes leave the store, and
it is the same path for an erasure, a reset, an eviction of keys and a quiet
store. The rows today need both a join path and a delete path per kind of row,
plus a snapshot path beside them. The log needs one path for joins and one for
down steps.

**Why not a snapshot alone.** A landing would print and write 3 to 6 MB. A
delta is the size of what changed.

**Compaction** runs in three cases: a down step (always, at once), a quiet store
(30 s, or the page hidden, as `settle` does today), and a log past a bound
(1,000 entries or 2 MB). It runs under the store's Web Lock (§3.6).

### 3.2 A pull

1. The worker's JavaScript asks the core `since(store)` and asks the box for a
   piece from there.
2. It hands the answer's text to `core.take(store, answer)`, which joins it
   into the replica and answers the effects: the delta's bytes, which
   conversations moved, the names to fetch (none for a skeleton), and the keys
   owed.
3. The JavaScript appends the delta to the log and does what the effects say.
   It keeps nothing of the answer.

A cut pull's pieces are deltas of kind `Piece`; the last answer's `Landing`
delta makes them readable. A restore after a crash reads them back from the log,
so `piece/<n>` and `pending` go.

### 3.3 An erasure

The box's pull names a key as erased, or the skeleton's own `Erased` node does.

1. The core answers `owed`: the keys, and for each the blocks its words were
   rendered to. It opens those words with the key, so it asks for the prints of
   those lines first; a fetch that fails keeps the key, owed, as today.
2. The JavaScript runs one transaction over `snapshot`, `log`, `blocks` and
   `content`: it deletes each block, deletes from `content` the print of every
   line every slot of which the erasure leaves erased, writes the snapshot the
   core prints *without* the keys, and deletes every log entry. Nothing else
   holds a key or those prints: not a row, not a piece, not a shadow.
3. The core lets the keys go (`HeldLines::keys` already forgets every word they
   opened, at once) and keeps each of those lines as its husk in place of its
   print.
4. The worker tells the page which blocks went, and the page drops any it drew.

**How the single form forgets:** the log is append-only, so it cannot remove a
key from an earlier entry. The compaction removes the whole log and writes a
snapshot that never held the key. The deleted entries are gone as IndexedDB
deletes them, which is what deleting a key row does today.

**No print whose every slot is erased** (decided by the owner, §7, and refined
for stage 3b). After an erasure no key it names is in IndexedDB, in the core, or
in any message; no word it opened is in the core's memo, in `blocks`, or in the
page; and no print of a line every slot of which is erased is in IndexedDB or in
the core. The browser's HTTP cache keeps every content answer for a year
(`immutable`) and no page can delete it, so a ciphertext print can stay there,
where without its key it opens nothing; the law covers what the device's own
code holds.

Two consequences for the core, both decided by the owner for stage 3b:

- **A line whose print is gone reads from its husk.** Its skeleton node names
  its slots' keys and its step, but not its shape (who said it, its act), which
  an erased turn is drawn by, so the node alone cannot stand for it. At the
  erasure the core keeps the line without its sealed words, its husk, in its
  snapshot and frames, and deletes the print from `content`. The husk is the
  device's alone; the snapshot's form moves to 3, and the one more download of
  the skeleton that costs is accepted. A reading reads the husk as the line read
  without its keys, and `lacking` never names a line held as one, so its print
  is never fetched again.
- **A line that seals an erased slot and a live one keeps its print.** An
  erasure names keys, not lines, and a forget erases only the copies its
  remembering wrote, so one print can seal both. The print is deleted only when
  every slot of its line is erased; until then its erased slots stay unreadable,
  since their keys are gone. This refines the earlier decision to delete every
  print of an erased slot.

### 3.4 Eviction

The core computes `plan(budget)` over its own outlines, as `keep.plan` does over
J1 today, and answers the names to drop. The JavaScript deletes them from
`content`; the core drops the prints it held. Nothing records what was evicted:
a reading that needs a dropped print finds it lacking and fetches it, from the
HTTP cache first.

### 3.5 A warm load

One read-only transaction reads `snapshot/<store>/<form>` and the range
`log/<store>/*`. The core loads the snapshot and applies the log in order,
skipping any frame of another form except `Own`. The core is then the replica.
No node row is read, no key or node is handed over, and no reader waits for a
row read behind it, so `pull.first`, `noneEarly`, `Reader`, `Primed` and `Key`
go. The prints are read from `content` by name as readings need them, as today.

**Expected cost** (from LAZYSYNC §16 and #286's phases, capped): the row read
(0.9 to 1.2 s), decoding (0.1 to 0.2 s), handing over keys (0.3 to 0.8 s) and
nodes (0.3 to 0.7 s) all leave the path. What remains is the snapshot's read (15
to 72 ms for 6 MB, less at half the size), the core's load (105 to 186 ms), the
stream's prints (about 0.29 s) and its draw (about 0.1 s). That is why the
target of under 2.5 s for the first draw is in reach; stage 3 measures it.

### 3.6 Two tabs

Each tab has its own dedicated worker and so its own core: there are two
in-memory models, and they must converge. (A `SharedWorker` would make one, but
Chrome for Android gained it only in 2026, at version 148 or 151 by the sources
the plan read, and no source confirms that the Android app's WebView runs one,
so the plan does not rely on it; the owner decided one path for all, §7.)

- **Joins** are appended by either tab at any time. After appending, a tab posts
  `{ store, seq }` on a `BroadcastChannel` named for the store. A tab that hears
  it reads the log from the last `seq` it applied and applies what it lacks.
  The message only wakes the tab; the log is the data, so a missed message is
  caught up at the next landing or load, when the tab reads the log anyway.
- **Down steps** (compaction) run under `navigator.locks.request("replica/<store>")`,
  so two tabs never compact at once. A compaction moves a generation number
  in the snapshot. A tab that hears of a compaction, or finds the generation
  moved, loads the snapshot again and keeps the prints it holds by name (the
  rule of `HeldReplica::regiven`, which becomes `snapshot::Held::regive`, #304's
  next stage).
- **Law (two tabs converge):** after any interleaving of two tabs' pulls, own
  writes, erasures, compactions and reloads, with any broadcast lost, each
  tab's core holds what a core handed the union of their objects cold holds,
  less every erased key. `web/test/warmload.test.ts` grows into it, over the
  real core and a fake IndexedDB.

Both APIs are in every browser the app supports (Chromium and its WebView, and
Safari from 15.4).

### 3.7 A build update

A new build reads `snapshot/<store>/<its form>`. If none is there, it reads the
log's `Own` frames, pulls the store cold, and appends the own literals again as
its own deltas. A page of the old build still open keeps writing frames of its
form; the new build skips them except `Own`, so nothing either build wrote that
the box lacks is lost. A compaction deletes snapshots of a lower form than its
own. The old page then loads cold in memory, and `newbuild` offers it a reload,
as today.

So the database's schema changes once more (stage 7) and then never: a form
change is a cold load of the stores whose form moved, measured once per build
that moves one.

### 3.8 The tab's own objects (`localwrites`)

LOCALWRITES §5.5 asks four things of the lines' storage, and each becomes one
step of the same owner:

- **Never drop an unsent object.** An own object is an `Own` frame in the log
  and part of the replica, so a compaction keeps it in the snapshot, and a form
  change keeps it as a literal (§3.7). A row upgrade cannot drop it, since there
  are no row upgrades.
- **Own nodes.** `core.append_own(store, literal)` seals nothing new; it computes
  the node with the store's header function (the box's `Step` for the lines),
  joins it, and answers the delta. The node the tab computes equals the box's
  (LOCALWRITES' law), since it is the same function.
- **Eviction keeps own objects** until the box's word names them: `plan` keeps
  every unsent name.
- **The warm snapshot is no longer deleted by every write.** A sealed line is a
  delta appended, so a conversation of several messages appends several deltas
  and compacts once after the quiet. LOCALWRITES stage 3's measurement of a 6
  MB rewrite per conversation goes away.

The exchange that posts unsent objects asks `core.unsent(store)` for literals.
Acceptance arrives as the box's word in the next pull (`take` marks them
synced). A refusal from the asks store marks the object refused, and `standing`
leaves it out of readings, as today.

## 4. The box: readings recomputed per request

The box keeps its stores in one process, so a fact has one owner already (the
store on disk, read through `io/src/objects.rs`'s memo by heads). What it
repeats is *work*: readings that are pure functions of stores, made again on
every request.

### 4.1 The audit

In this table a path that starts with `secretary/`, `topics/`, `transcript.rs`,
`chain_set/`, `agent/`, `conversation/` or `memories.rs` is under `app/core/src/`;
`serve/` is `app/cli/src/commands/serve/`; `io/src/` is `app/io/src/`.

| Reading | Where | Made again on | Reads | A pure function of |
|---|---|---|---|---|
| The context `Reading` | `secretary/compaction.rs:59-79`, `Reading::of` over `digests.rs:101-161` | every session opening (`conversation/ask.rs:321`), the agent's `context` verb (`agent/verbs/context.rs:24-31`), the secretary's `record` (`secretary/surface.rs:122`), `GET /api/secretary/record` (`routes.rs:2344`), each round's `Compactor::take` (`compaction.rs:368`), `waiting` (`compaction.rs:528`) | every line envelope cloned, `transcript::opened` over all of them, the digests' history, one file read per summary text | the heads of lines, digests, memory, topics, topic-aliases |
| The memory read | `memories.rs:88-120`, also in `heading` and `GET /api/secretary/memory` | each of those | the memory's history, a `TwoPhase` fold, one text file per standing fact | the memory's heads |
| The topics reading | `topics/stores.rs:17`, `topics/reading.rs:52` | each session opening | both topic stores' nodes and a fold | the heads of topics and topic-aliases |
| The inbox fold | `secretary/inbox.rs:766`, `Fold::of(nodes())`, 43 call sites (`GET /api/waiting` among them) | each | the inbox's nodes | the inbox's heads |
| One conversation's transcript | `transcript.rs:952-963`, per round (`conversation/round.rs:257`) | each round | every line node, to keep one conversation | the lines' heads and the conversation |
| The fold's rows | `routes::fold` (`routes.rs:78-104`) over `chain_set::Read::moved` | every `GET /fold`, `/todo`, `/api/waiting` | each chain's reading at an instant | the chains' heads and the instant |
| The overview's checks | `serve/settings.rs:32-106` | every `GET /api/overview` | the inbox (memoised by heads), the build (memoised, but its key leaves out the instant `about` reads), health registers, git config, plugin schemas | partly heads; partly files that have none |
| Mail's maps | `io/src/mailbox.rs:160-226` (`load_messages`), `:268` (`load_state`), `state.rs:135` | every mail list, search, message, thread and queue | `notmuch show`, each file read, hashed and parsed into a `BTreeMap`; `state.json` and `local/mail.json` parsed | the notmuch database's revision and the two files' bytes |

### 4.2 The one memo

The box already has three memo mechanisms: `Graph::reading` (per graph, so
keyed by one store's heads, one user), `store::memoised` (keyed by
`memo::name(b"heads", tips())`, two users: `asking` and `built`), and
`table::Table` (used for trust, checks and rendered pages). They are one idea:
a reading's value cached under the name of what it was computed from.

**The target is `store::memoised` alone, with its key generalised from one
store's heads to a VERSION**: anything whose name is a hash of what the reading
reads. A reading declares its version as data:

- a set of stores: the name over each store's heads, in a fixed order (the
  context `Reading`: lines, digests, memory, topics, topic-aliases);
- a set of stores and an argument (one conversation's transcript: the lines'
  heads and the conversation; the fold: the chains' heads and the instant,
  which fixes the overview's build key too);
- an outside source with a revision of its own (mail: notmuch's database UUID
  and `lastmod`, from `notmuch count --lastmod`, and the SHA-256 of each JSON
  file).

The same `memoised(version, f)` serves all of them. `Graph::reading` becomes one
instance (a version of one store), and `Table` stays only where its key is not a
version (the rendered pages, keyed by content).

**Two rules the memo must keep:**

- **What has no version is not memoised.** The overview's health registers, git
  reflog and plugin schemas are files with no heads; they stay per request until
  one becomes a store. The memo never guesses a key.
- **Erasure reaches the memo.** The context `Reading` holds opened words. An
  erasure or a forget appends an object (`Erased`, `Forgotten`), which moves the
  heads, so a stale entry is never served; but it would stay in memory until
  evicted. So a reading that holds opened words keeps one entry (the newest
  version only), and the sweep that deletes a key file drops every entry of the
  readings that opened it in the same lock. *Law:* after an erasure, no memo
  entry holds a word the erased key opened.

## 5. What is generic, and where it belongs

Three pieces of the target name no Suzatary concept, and belong in the Prodrome
(fulfilium/prodrome), free of any Suzatary type so that they can move:

- **The replica**, generic over a schema with a header function: heads,
  frontier, the box's word, refusals, unsent names, nodes or objects, keys,
  prints held, pieces, `since`, `take`, `append_own`, `owed`, `lacking`,
  `plan`, `unsent`. Today it is split between TypeScript (`replica.ts`,
  `locked.ts`, `pull.ts`, `engine.ts`) and the core (`HeldReplica`, `Outlines`,
  `KeyRing`). It would join `prodrome_core` as `replica`, beside
  `store::frontier`. `HeldReplica`, the inbox's and the topics' replicas and the
  chains' become instances, each given its schema.
- **The persistence form**: `snapshot` (which already says it can join the
  Prodrome beside `memo`) with `regive` (#304's next stage), the log frame, and
  the delta type of the replica. It joins `prodrome_core::snapshot`.
- **The memo by version** of §4.2: `store::memoised` with a version in place of
  heads. It joins `prodrome_core::memo`, beside `Cache`.

The IndexedDB adapter of the log (about 150 lines of TypeScript) is generic too,
but the Prodrome ships no TypeScript, so it stays in `web/src/prodrome/` and
names no store.

## 6. Stages

Each stage is one PR, a series of commits, each buildable and tested. Each
DELETES a copy or a recompute, names its law, and measures the lines it removes
(`scripts/size.sh`) and, from stage 3 on, the capped `bigstore` warm first draw
(`web/e2e/bigstore.test.ts`'s last scene with `SUZATARY_E2E_BROWSER_CPU=0.67`,
three warm loads after the snapshot is written; the target is under 2.5 s).

0. **Dead code and the yardstick.** Delete P4 (`setOf`, `HeldSet`, `kept`,
   `told` and `tellLeft` in `sync.ts`, `keptChains` and `chainsOf` in
   `core.ts`: no caller in `web/src` or `web/test`), about 50 lines with their
   comments. `bigstore`'s last scene prints the capped first
   draw as one number the later stages report. *Law:* none new. *Measure:* the
   baseline.
1. **The core answers what the tab counted.** Delete J3 (`content.memory`,
   `moves`, `asked`, `holds`, `has`, `read`, `version`): `engine.over` asks
   `Lines.lacking`, the eviction asks the core what readings read. *Law:* a name
   is fetched exactly when the core lacks its print, under any interleaving of
   loads, clears, landings and evictions (`warmload.test.ts`). *Measure:* lines
   removed (about 40 in `content.ts`, 15 in `engine.ts`).

   *As built (stages 0 and 1, one PR).* The core answers `Lines.held` and a
   count of moves, `Lines.version`, which moves with every change of the prints
   it holds. `core.lackingOf` reads `held` again whenever the count moves, a
   view keyed by the core's version that nothing else writes; a `Lines.lacking`
   that took names as JSON cost about 150 ms more per warm load at the
   `bigstore` shape, since the purge asks it of the whole skeleton after each
   reading. Stage 0 removed 56 production lines of TypeScript; stage 1 removed
   the record and its six helpers and added the view, net +2 lines of
   TypeScript and +19 of Rust. The capped warm first draw (`bigstore`, 0.67 of
   a core, nine loads each, alternating): master median 3,149 ms (2,553 to
   3,893), this stage 2,895 ms (2,547 to 3,472).
2. **The core reads its own skeleton.** Delete `linesync.retiredBy`, `touched`,
   `keep.ts`, and `lines.nodesOf`, `naming`, `holds` in favour of `Lines.retired`,
   the moved conversations `take` answers, `Lines.plan`, `Lines.lacking` and
   `Lines.holds`. J1 for the lines is still written (the rows need it) but read
   by nothing but the persistence. *Law:* `Lines.retired` equals the box's
   `retired` over generated stores with reseals of reseals and erasures (the
   rule of 10-26, one implementation). *Measure:* about 150 lines of TypeScript
   out, about 60 of Rust in.

   *As built.* The core answers over its skeleton and any nodes it was not
   told yet (`beside`): a purge reads the row joined with a pull's pieces
   before the core holds them, so `Lines.retired(beside, held)` and
   `Lines.lacking(scope, beside)` take them, and `linesync.telling` names
   them (`untold`). A first version asked of the core's skeleton alone, and
   the erasure law of `onewriter.test.ts` caught a cut pull whose erasure
   found no line to open. The law is the core's `web_vectors`
   `law_the_core_s_retired_keys_are_the_box_s` (128 generated stores, the
   device pulling, starting over or away); it catches a rule that does not
   follow a reseal of a reseal. Production lines: TypeScript −100, Rust +337,
   far past the estimate, since the scope, the plan and the rule each read
   nodes held and nodes beside them, and the module parses a scope at its
   edge. J1 is still read by `pull` (`holding`, the frontier) and by `untold`,
   until stage 3. The capped warm first draw did not move measurably: two
   sets of three alternating runs, nine loads each side a set, gave master
   medians of 3,381 and 3,801 ms and this stage 3,887 and 3,078 ms; the sets
   disagree in direction, and the runs vary by about 0.8 s.
3. **One form for the lines (`coreskeleton`).** It is split in three PRs:
   3a, the core holds the lines' replica and the device keeps the core's
   snapshot and log; 3b, the erased slots' prints deleted at an erasure; 3c, a
   `BroadcastChannel` wake and a Web Lock for compaction.

   *As built (3a).* The core holds the landed row, a pull's pieces, the box's
   words, the frontier and the generation (`app/core/src/replica/`, generic
   over a node and its keys, a port of `replica.ts` and `locked.ts` checked
   step by step against them by `linereplica.test.ts`); every move answers a
   delta, and each delta but a purge's is a join. `linesync.ts` keeps each
   delta as a frame (`replicalog.ts`) and reads the log back on a load and
   before each pull, so two tabs converge without a lock: a compaction is one
   IndexedDB transaction that writes nothing where another tab's snapshot came
   first. Version 11 drops the lines' rows and the snapshots store with no
   converter. Deleted: J1, J2, J4 and J5 for the lines, `telling`,
   `untold`, `Reader`, `Primed`, `Key`, `pull.first`, `pull.loaded`, the
   `Loaded` scope, and `noderows`' snapshot (`Memo`, `prime`, `settle`, its
   key). Laws: `warmload.test.ts` (warm equals cold and two tabs converge, each
   reload read with the box gone; a fetch hands the core exactly what it
   lacks), `onewriter.test.ts` (no key of an erasure in any row, message or
   reload), the core's `line_replica` laws. The bigstore snapshot is 3,517,325
   bytes for 13,000 lines with names as 32 bytes, against 6.0 MB before. The
   capped warm first draw, three alternating pairs of three loads: master
   median 2,197 ms (mean 2,420, 1,893 to 3,575), 3a median 1,981 ms (mean
   2,113, 1,689 to 2,909); in the worker the lines are restored 0.4 to 0.8 s
   after the stream is asked, where master read its rows 3.3 to 3.8 s after.
   Production lines: Rust +1,186, TypeScript +77. The Rust is the replica
   algebra the core now owns; the TypeScript copy of it stays until the inbox,
   the topics (stage 4) and the chains (stage 6) move to the core, which is
   where its lines go.

   *As built (3b).* A purge's forget husks every line the rows read, held with
   words each key of which no row holds (`HeldReplica::bare`): the line with
   each slot's words dropped (`Dropped(key, rules)`, which no store's
   vocabulary reads), held where its print stood and never let go by a drop.
   The core's form is 3: the snapshot is the rows and the husks
   (`LineReplica`), each frame the rows' delta and the lines it husked. A purge
   also runs when a line is to be husked with no key owed (a device that took
   the line after its keys were erased); it keeps its step as a frame before it
   compacts, so a tab that compacts first takes the forget and the husks
   instead of writing a snapshot without them; and every compaction deletes, in
   its transaction over the log and `content`, the print of each line the core
   holds as a husk. A content answer is no longer kept beside the reads, since
   it carried the ciphertext past the erasure. The fake IndexedDB ran two
   transactions over one store at once and lost the first one's writes;
   it now runs them in IndexedDB's order. Stage 7's deletions came forward (the
   owner's direction of 2026-11-03): the upgrade carries nothing (`carry`,
   `framed`, `carried`, `objectsync.carried` and its hook, `linesync.retire`,
   `RENDERED_OWED` with `owedClear` and the worker's `Cleared`, the old upgrade
   scenes), keeps a device's chains' rows from version 8, since they hold what
   the tab wrote, and the rendered store's version 4 drops every block once.
   Laws: the core's `held_lines` "a line whose keys are gone reads the same as
   its husk"; `onewriter.test.ts` (no row, message or kept answer holds an erased
   line's ciphertext, and a reload reads without it); `warmload.test.ts` "a husk
   one tab's purge kept stands through another tab's compaction…";
   `objects.test.ts` (the upgrade keeps a chain's unsent act, from version 8).

   *As built (3c).* Each append is told on a `BroadcastChannel` named
   `replica/<store>`, and a tab that hears it reads the log from where it
   stands; the log is the data, so a lost message is caught up at the next
   pull. A compaction runs under a Web Lock of the same name, where the browser
   keeps locks; the IndexedDB transaction stays what makes it safe. Waking a
   tab mid-pull showed that taking another tab's frames between a pull's pieces
   and its landing loses the pieces: a landing of the other tab, at a later
   count of landings, replaces the pending pieces, and the pull then lands
   nothing. A tab's pull, its compactions and its reads of the log now take
   turns, and a compaction its own purge asks for during a pull runs once the
   pull ends. The generation-moved check of §3.6 was not needed: a compaction
   that finds another tab's later snapshot already reads it instead. Law:
   `warmload.test.ts` "a tab another tab's landing wakes reads it with no pull
   of its own…", which fails without the wake, and the convergence law with
   the wake on. The snapshot and log of §3.1
   replace D1-D5 for the lines; the core holds the head and the pieces; J1, J2,
   J4 and J5 for the lines go, with `telling`, `Reader`, `Primed`, `Key`,
   `pull.first` and the snapshot's key. Erasure by compaction (§3.3); two tabs by
   the log, a `BroadcastChannel` and a Web Lock (§3.6). The database's version
   11 adds `snapshot`, `log` and `blocks`, and deletes the lines' rows and
   snapshot in the same transaction (keeping any unsent literal as an `Own`
   frame). *Laws:* a warm load equals a cold load; two tabs converge; an erasure
   leaves no key in IndexedDB, the core or any message, no opened word in the
   core or `blocks`, and no print of an erased slot in IndexedDB or the core,
   with each such line read from its node as erased. *Measure:* the capped warm first draw (target under 2.5
   s), the stream's answer time, the snapshot's size with 32-byte names, and
   lines removed (most of `noderows.ts` for the lines, `linesync.telling`).
4. **The inbox and the topics into the same form.** Their replicas become
   instances of the generic replica in the core; `objectsync.ts` and the rest of
   `noderows.ts` go, with `storeset.readOf`'s rebuilds (C5 becomes a reading).
   *Law:* warm equals cold for each store; two tabs converge on each. *Measure:*
   lines removed (`objectsync.ts` 74, `noderows.ts` 344 with stage 3's share,
   `locked.ts` 50), the warm draw of the Inbox.

   *As built.* The inbox and the two topic stores are each a replica of
   objects the core holds (`app/core/src/replica/objects.rs`): the generic
   rows of 3a (`Pulling`) with no keys, beside the objects the tab wrote,
   each with the box's answer (unsent, refused until trusted or for good,
   taken), which only climbs, so every move is a join. What the tab wrote
   stands through a reset or a later epoch of the rows. The rows' form is one
   form for every store (`replica/form.rs`, the lines' bytes unchanged at
   form 3); the objects' form is 1. In the worker one store (`object_rows.rs`,
   one macro for the three classes) answers each move as a frame, and its
   readings read the Prodrome's replica of the standing objects, made once per
   move of them and counted by `version`; `replicasync.ts` drives each store
   as `linesync.ts` drives the lines, minus content and keys. Deleted:
   `objectsync.ts`, `noderows.ts`, `engine.ts`, `pull.ts`, `locked.ts`,
   `take.ts`, `purge.ts`, `storeset.readOf`'s rebuilds and the stores' rows;
   version 12 drops the rows with no converter. Laws: `warmload.test.ts` (the
   inbox and the topics read from a warm load what they read cold, and two
   tabs converge, whatever they write, each reload read with the box gone);
   the core's `object_replica` (the moves are joins, shuffled and repeated;
   the snapshot and frames read back; what the tab wrote stands until the box
   answers; the later-epoch law of `epochdrop.test.ts`, now over the core's
   rows). Production code against 3b: Rust +17,254 characters (+791 lines),
   web −21,923 characters (−592 lines), net −4,669 characters. The Inbox's
   first draw in a fresh tab of a warm device (`pagebench`, the Inbox alone,
   `SUZATARY_E2E_BROWSER_CPU=0.67` and throttling 4, nine loads each, three
   alternating pairs): 3b medians 3,408, 3,324 and 3,494 ms, this stage 3,578,
   3,397 and 3,122 ms; no measurable change, with about 0.4 s between runs.
   Next: `linesync.ts` restores, pulls, lands and compacts as
   `replicasync.ts` does, and becomes its instance once 3c's wake and lock
   land in one place for all four stores.
5. **Rendered blocks beside the replica.** R1 moves into `blocks`; the erasure's
   transaction deletes blocks with the keys; R2's marks and P6 go;
   `suzatary-rendered` is deleted at the worker's start. *Law:* an erasure
   leaves no block of its words, with an answer asked before it landing after
   it. *Measure:* lines removed; the stream's draw without P6, no slower.

   *As built.* The blocks are the `suzatary` database's `blocks` store
   (version 13; a device at 12 adds the store and keeps the rest). The core
   answers which answer keys the words its held keys open feed
   (`HeldLines::fed`), and `Lines.admits` keeps of a set of keys every key of
   another template and each fed answer key. A join runs in one transaction
   with the replicas: where another tab wrote frames this tab has not taken, it
   takes them and asks again; else it writes what the core admits. Every
   compaction sweeps, in its transaction, the blocks the core no longer admits
   and those of another build, so the erasure's own compaction deletes its
   blocks, and one that failed is healed by the next; no list of owed blocks is
   kept. R2's marks, P6, the `Forgot` and `BlocksOwn` messages and the separate
   database go (deleted at the worker's start). A block is kept while any held
   key opens the words it renders, so the same words said in two conversations
   keep their block until both are erased. Laws: the core's `held_lines` "the
   blocks fed are those a held key's words render"; `onewriter.test.ts` (no
   block of erased words after the erasure, and no join after it writes one);
   `rendered.test.ts` (a join writes what the core admits, takes the log first
   where it is behind, and a sweep takes the rest). The warm first draw,
   uncapped: 988 ms, median of three loads, against 1,011 ms in 3a.
6. **The chains into the core.** The chains become replicas of the generic kind
   in the worker; the page's `source` becomes a view; `remote.hold` and P3, P1,
   P2 and W1 go; writes seal through the core (with or after LOCALWRITES stage
   7). *Laws:* the rollback law over the core; an unsent act survives every
   reload and build update. *Measure:* lines removed (most of `sync.ts`'s 470,
   `remote.ts`'s delta), the todo list's warm draw.
7. **The last upgrade.** Version 12 keeps every unsent literal of the `chain`
   rows and deletes the `chain` store and every D9 row. The upgrade's branches,
   `carry`, `framed`, `carried`, `unsentNone`, `objectsync.carried`,
   `linesync.retire`, `owedClear` and the old upgrade scenes went in stages 3a
   and 3b, by the owner's direction of 2026-11-03: each stage deletes the old
   shapes it leaves unused, and a device on an old format takes its stores
   again. From here the schema is fixed.
8. **The local fold goes, if measured.** With the chains warm from the core,
   measure the todo list's first draw from the core alone. If it is within the
   first-paint budget, delete L1, `primed` and `boxFolds`. If not, L1 stays, and
   the PR records the numbers instead.
9. **The box's memo by version.** `store::memoised` takes a version; the context
   `Reading`, the memory and topics readings, the inbox fold, a round's
   transcript and the fold's rows go through it; `Graph::reading` becomes an
   instance; mail's maps are keyed by notmuch's revision and the two files'
   hashes. *Laws:* a memoised reading equals the reading made fresh, over
   generated store histories (`app/core/tests/`); after an erasure no memo entry
   holds a word the erased key opened. *Measure:* a session opening's time on the
   live shape before and after; lines removed (the per-call rebuilds).

Stages 1 and 2 are small and safe, and they remove the two bug shapes of §0 on
their own. Stage 3 is the todo `coreskeleton`. Stages 4 to 7 follow its pattern
store by store. Stage 9 is independent of the rest and can go in any order.

LOCALWRITES can land in parallel. Its stage 2 (the tab seals lines) is simpler
after stage 3 here: `append_own` is one core call, and the "never drop an unsent
object" rule is the log's, not a rule each upgrade must remember.

## 7. The owner's answers (2026-11-02)

1. **An erasure also deletes the ciphertext prints of the erased slots** from the
   `content` store, in the same transaction as the keys, the blocks and the
   snapshot, and the core drops them too (§3.3). The HTTP cache, which no page
   can reach, is outside the law. The one edge left open is a print that seals
   an erased slot and a live one; stage 3 counts them and brings the count back.
2. **Each upgrade downloads the skeleton again once**, with no converter of old
   rows (rule 1). The `content` store and the HTTP cache keep every print, so
   only the skeleton (about 6 MB for the lines at the live shape) comes again.
3. **One path for all browsers: one core per tab**, converging through the log
   (§3.6). There is no `SharedWorker` path.

## 8. The plan read three ways

**As the Elm architecture reads it (one model, one update, views).**

- *Is there one model?* On the device today there are four places that move
  replica state: the core, the worker's JavaScript (`engine.row`, `pending`,
  `content.memory`, `telling`, `confirmed`), the page (`source`, `others`,
  `remote.sent`) and the IndexedDB rows, each moved by its own code. In the
  target there is one: the core's replicas (§3.0). The rows become the model's
  history (a snapshot and the frames since), not a second state.
- *Is every change a message through one pure update?* Today a landing is
  `engine.take`, then `place.land` joining with what another tab wrote, then
  `telling.took` handing the difference to the core, then a snapshot owed:
  four writers of one change. In the target a landing is `Pulled(answer)`
  through `update`, whose effects the runtime performs.
- *Is everything drawn or cached a view?* The readings, the plan, the rendered
  blocks (addressed by content, reached by erasure in the model's own
  transaction) and the page's readings are. What is not a view today is deleted:
  the shadows J3, J4, J5 and P3, the page's block tier P6, and, if measured,
  the local fold L1.
- *Is any mutable state hidden on the side?* What remains outside the model is
  traffic (the fetcher's queues, pull retries, the lock and channel handles),
  the device's signing key, and the UI registers, each with one owner and none
  a fact about a store.

**As "make impossible states impossible" reads it.** §2.10 takes each bug of §0
in turn. A print counted held that the core lacks needs a JavaScript set of
names, and none exists (`Lacking` is minted by the core). An old-shape row
beside a new one needs a decoder of an old shape, and none exists (`Frame` and
`Decoded`). Keys kept after a reseal need a second implementation of the rule,
and stage 2 deletes it. A device memo table would be a second model, and §3.0
leaves no room for one.

**As a storage engineer reads it: does every fact end with exactly one owner?**
F1 to F6: the core's replicas, persisted as one snapshot and one log per store.
F7: the `blocks` store, addressed by content and reached by an erasure in the
same transaction as the keys. F8: the core's fold. F9: the snapshot's form.
`content` and the HTTP cache hold literals by name, which a name decides, and
presence is the core's answer. The device's signing key, the UI registers and
the shell's caches each had one owner already. On the box (§4) every fact
already has one owner, the store on disk; what repeats is work, and the one memo
keyed by version replaces the per-request rebuilds.
