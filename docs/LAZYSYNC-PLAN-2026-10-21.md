# LAZYSYNC — a store's skeleton eagerly, its content by hash on demand

The owner's decision (2026-10): the tab syncs a small eager SKELETON of each
store (the DAG of object names and their headers) and fetches CONTENT on
demand, by hash. Content is immutable, so the box sends it to be kept for good
(HTTP `immutable`) and the tab keeps it in IndexedDB. The tab prefetches by
recency. Folds are memoised per node by one generic mechanism, the Prodrome's
memoised fold over a content-addressed tree (`prodrome_core::memo::fold`,
design §6.2), never by levels designed for one page.

This plan says what the skeleton holds, which routes serve it, how today's
whole-store pulls map onto it, what a device holding today's replica sees
after each stage, how the fold is memoised, what is prefetched, what offline
means, and the stages, each one PR.

## 1. Why

Today a tab that reads a conversation holds the whole lines store first. The
lines are the largest store: 34 MB on the box on 2026-10-13 (ARCHITECTURE,
"Every store a replica in the tab"), when it was 1,000 lines and 3.99 MB in
September (`conversation-prodrome-core-jrzwvi` report: median 702 B, 99th
percentile 61 KB, largest 196 KB); the stage-2 words fixture is 4,600 lines
and 5.5 MB. `storesync.ts` pulls it in pieces of a megabyte
(`routes::Cut::PIECE`), each asked from the heads the last one left, so a new
device opening the chat waits for 34 sequential round trips and 34 MB before
the first line draws, and every one of those requests makes the box read the
whole store. Most of those bytes are the sealed words of conversations nobody
opens on that device: the size is in a few long answers and long
conversations, not in the count of lines.

What a page reads first is small: the list of conversations (one title per
conversation) and the last screenful of one conversation. The rest of the
store is needed only for structure (which lines exist, what rests on what,
which keys were erased), and structure is a few hundred bytes per line, not
kilobytes.

## 2. The skeleton

The skeleton of a store is, for every object the store holds, a NODE:

| field | what it is |
|---|---|
| `hash` | the object's name, the sha256 of its literal |
| `parents` | every object it rests on, its prodrome's genesis included, sorted (`prodrome_core::event::parents_of`) |
| `size` | the bytes of its literal, so content is asked for in bounded answers |
| `header` | what a fold reads of the object without its literal |

The header is a sum by what the object is. An object that carries no event (a
`Genesis`, a signature, a key) is `structure`, with its constructor. An
object carrying an event has the fields the store's folds key and order by:

- **lines** (stage 1): `conversation`, `at` (the writer's clock, the recency
  key prefetch sorts by), `kind` (the line shape's constructor: `Opened`,
  `Titled`, `Said`, `Proposed`, `Erased`, …) and `keys`, the name of the key
  of each slot of its words. The keys travel beside the nodes exactly as they
  travel beside the objects today: a skeleton answer carries the key of every
  slot its nodes name, never one an erasure anywhere in the store names, and
  the name of every erased key. So erasure works unchanged: a key is the
  mutable half and goes with the skeleton, which is never cached; the literal
  is sealed ciphertext and may be cached for good, since without its key it
  opens nothing.
- **inbox** (stage 4): the proposal it is about and, for a response, its
  `Phase`. The inbox's state register is inflationary and read by phase, so
  the badge and the "Needs you" list read headers alone; a card's words are
  content.
- **chains** (stage 5): the todo, the event's constructor, `at` and the
  actor. A lifecycle (`Completed`, `Cancelled`, `Reopened`) and which
  `Authored` is a register's frontier are headers; the `Authored` record and a
  spec are content.

The skeleton answer carries what today's objects answer carries beside its
objects: the store's heads, the genesis of every prodrome it serves and the
EPOCH (`suzatary_core::epoch`), read under the same lock and the same epoch
key (`store/lines`), so one store has one epoch whichever route answered.

A skeleton is pulled exactly as objects are pulled today: since heads, parents
first, in pieces (`routes::Cut`), each piece weighed by its nodes' bytes
rather than by literals, with `rest` saying how many objects are left. The
pieces law of `app/core/tests/pieces.rs` holds of it word for word: a piece
joined to what the tab holds holds everything it rests on, and the pieces
asked one after another are together the whole.

The skeleton is not tiny: a node is about 300 to 450 bytes, most of it hex
names that compress to about half. That is a tenth of a line's average
literal, and it is all that is pulled eagerly. A later stage may send parents
as indices into the same answer once measured (§9, stage 6); stage 1 sends
names, because a name is what every other part of the system compares.

## 3. The routes

Stage 1 adds two reads to the box; nothing on the page uses them yet.

- **`GET /api/stores/lines/skeleton?since=<heads>`**: a piece of the lines'
  skeleton a replica holding those heads lacks (`routes::lines_skeleton`,
  `wire::SkeletonPayload`). `no-store`, like every other read: it holds the
  heads, the epoch and the keys, which move.
- **`GET /api/stores/<store>/content?hashes=<names>`** for the inbox and the
  lines, and **`GET …/content?hashes=<names>`** for each chain at the bare path
  and `/api/<chain>/content` (`routes::content`, `wire::ContentPayload`). One
  to 64 names (`query::MOST_HASHES`), 64 hex digits each, in ascending order
  with none twice, so one set of objects has exactly one URL and a cache hit
  answers any tab that asks for the same set; the literals together at most a
  megabyte (`routes::MOST_CONTENT_BYTES`), one object answered whatever its
  size (a line from before the 64 KiB stored-answer bound can be larger).
  The answer is the literals in the order asked, marked `Cache-Control:
  private, max-age=31536000, immutable` (`reply::Immutable`).

  A hash the store does not hold refuses the whole question with a 404 naming
  the hashes, in the same words whether no store holds it or another store
  does: a store's content route reads that store and nothing else, so what a
  reader may read through it is what the store's whole pull would have given
  the same reader, and a hash of the memory, of a chain asked at the lines, or
  of the agent's replica is an unknown hash there. The API's layer still
  marks every answer `no-store`, a refusal and a stream included; an
  `Immutable` answer carries a mark that one layer outside it (`reply::kept`)
  reads to mark that answer `immutable` instead, so nothing else can be kept
  by accident.

  An answer is a function of the hashes alone, which is what makes
  `immutable` true: an object's literal is what its name is the hash of, so
  the same question answered at any time answers the same bytes, after any
  write, an erasure of those very words included (the words are sealed and
  their key is gone). A store that no longer holds an object (a migration, a
  rollback) refuses it from then on; a cache that kept it holds the right
  bytes for a name nothing reads any more, which costs space and nothing
  else (§7, garbage). A revoked device is answered with the wipe's
  `Clear-Site-Data: "cache"`, which empties the browser's HTTP cache with the
  rest.

A GET, not a POST, because only a GET is cached by the browser; 64 names are
about 4.2 KB of URL, inside what every browser and axum accept.

## 4. How today's pulls map onto it

| today | after |
|---|---|
| `GET /api/stores/lines/objects?since=` in 1 MB pieces (`storesync.ts`, `pieces.pull`) | the same loop over `GET /api/stores/lines/skeleton?since=`, the pieces cut by node bytes; `pieces.ts` unchanged, `keep` and `land` keep nodes and keys instead of literals |
| a piece's `keys` and `erased` | the skeleton piece's `keys` and `erased`, the same reading (`routes::words`, shared) |
| the `store/lines` row: replica plus keys (`locked.Row`) | the same row holding nodes instead of objects, and a CONTENT object store beside it: `hash → literal`, written once per name and never rewritten |
| `stores.overtaken`, `startOver` on a later epoch | unchanged: the skeleton answer carries the epoch, and a later one resets the row. Content rows are not reset: they are right for their names whatever the epoch, and are collected as garbage (§7) |
| `GET /api/stores/inbox/objects` | stage 4, the same way |
| `GET /api/<chain>/objects` (whole, `Cut::Whole`) | stage 5; the tab's own sealed objects stay whole (it wrote them, and they must be posted) |
| `new core.Conversations(objectsOf(r.objects))`, one reading of the whole set | one memoised fold per conversation over the nodes it needs (§6) |

What a page asks of a store is unchanged: `storesync.reading` still answers a
`Secretary` reading, `storesync.proposals` still the proposals. What changes
is what a reading may answer: a conversation whose content is not all on the
device answers `Fetching` with the names it waits for, never a reading of
part of it (§6).

## 5. Consistency: a state the box could have sent

A device holding part of a store's content must always hold a state the box
could have sent. The state is a pair: the skeleton `S`, and the content `C`,
a set of literals each keyed by its name.

- `S` is built as today's replica is: pieces each closed under ancestry joined
  to what is held, within one epoch, a later epoch resetting the row
  (`stores.overtaken`, `locked.join`'s epoch order, PR #204's join that drops
  a store's earlier epoch). So `S` is always a down-set of the box's objects
  as the box served them in its latest epoch: exactly the structure of a pull
  the box sent.
- Every literal in `C` is checked against its name on arrival (the tab
  rehashes, as it does every object today), so `C` holds the box's bytes for
  those names and nothing else. Content is never a choice the box makes per
  answer, so two tabs, two answers or a cache can never disagree about it.
- The pair `(S, C ∩ S)` is a projection of `(S, all literals of S)`, which is
  what the box's whole pull at that point would have sent. The device never
  holds a literal the skeleton does not name and reads it as part of the
  store: content outside `S` is garbage, not data.

The danger is a READING of a projection, not the projection: a fold over a
down-set with holes is not a reading of any state the box was ever in. So the
rule, pinned in stage 2, is that a reading is total or absent: a fold at a
node needs the content of every node below it whose content its algebra
reads, and answers `Fetching(names)` until it has them. A header-only fold
(the conversation list's order, the badge) reads headers alone and is total
on `S`. The law, over random stores and random subsets of fetched content:
every reading a tab answers equals the reading of the whole pull at the same
skeleton, or is `Fetching`.

Two tabs share the IndexedDB rows: the skeleton row is joined as today
(`locked.join`, a semilattice), and the content store only gains entries whose
value is fixed by their key, so its join is a union that cannot conflict. A
second run of any fetch writes what is already there. Offline, a tab holds
`(S, C)` and reads every conversation whose content is held (§8).

## 6. The memoised fold

ONE mechanism: `prodrome_core::memo::fold` (`memoCata alg (Fix f) = memo
(name (Fix f)) (alg (fmap (memoCata alg) f))`), whose cache key is a node's
content name, with `memo::balance` giving any long sequence a balanced shape,
run in the core's worker as the readings already are (`questions.ts`).

The tree for a store is built from its skeleton, never designed per page:

- a conversation's lines in their linearisation order, as a balanced tree
  (`balance`), each leaf a line named by its object hash, each chunk named by
  `memo::name` over its children;
- the store as one node over its conversations' trees, in conversation order.

The algebra at a leaf reads that line's literal and keys and is the
transcript's one-line step; at a chunk it is the transcript reading's monoid
combine (the fold is a monoid action, `prodrome_core::fold`'s `extend`, which
is law-tested). Since a closed chunk never changes as a log grows, a new line
renames only the chunks on the path to the last leaf: one per level of one
conversation, and the store's root. A conversation nobody opened is never
entered: its top is one hit, or, when its leaves' content is not held,
`Fetching` without a fetch until a page asks for it.

The cache entry for a node is keyed by the algebra's name and the node's name
(`memo::Key`), so it is right forever and kept in IndexedDB beside the
content, evicted least recently used under a byte budget like the rendered
blocks (`rendered.ts`): an evicted entry is recomputed from content, an
evicted literal fetched again by hash, from the HTTP cache first.

A merge in a conversation (two lines resting on one line, then a line on
both) is a node of the DAG, not of the tree: the tree is over the
linearisation, which the core already reads the transcript by. A merge moves
the order of the lines after the fork, which renames the chunks from the fork
on, and no others.

## 7. Prefetch, garbage and eviction

- **On the first screen** the tab fetches the content of every `Opened` and
  `Titled` node (one per conversation, the list's titles), then the last
  screenful of the conversation the route opens, by `at` descending: one
  content request of up to 64 names each, sent at once.
- **Behind it**, while the network is idle and the tab is shown, it fetches
  the remaining lines by recency, newest conversation first, in requests of
  64 names or a megabyte, stopping at a byte budget of the device's own
  (stage 3 picks it from `navigator.storage.estimate()`, as `rendered.ts`
  does).
- **Garbage**: a content row no node of the held skeleton names is dropped by
  the purge that runs after a landing. An erased key is purged exactly as
  today; its line's literal may stay, since it is ciphertext whose key is
  gone, and goes as garbage only when its node does.
- **Eviction**: content of a node the skeleton still names may be evicted
  under the budget, oldest `at` first; it is fetched again when asked for.

## 8. Offline

Offline is `(S, C)` with no fetch: the conversation list draws from headers
and held titles; a conversation whose content is held reads whole; one that
is not says what is not on this device (`Fetching` read as "not on this
device yet", with the count), never an empty conversation. Prefetch in §7 is
what makes the recent conversations readable offline; the budget is what
bounds it.

## 9. The first screen, before and after

A new device opening the chat, a megabyte a piece. The live store's size is
known (34 MB) and its line count is not recorded here, so the skeleton is
given for the fixture's shape (4,600 lines, 5.5 MB, about 200
conversations) and scales with the count of lines, not their bytes:

| | before | after stage 3 |
|---|---|---|
| eager bytes | every literal: 34 MB live, 5.5 MB on the fixture | the skeleton: about 400 B a line, about 1.8 MB uncompressed for 4,600 lines, about 0.9 MB on the wire |
| eager round trips | one per megabyte, sequential (a piece is asked from what the last left): 34 live, 6 on the fixture | 2 for 4,600 lines, sequential |
| first-screen content | none beyond the above | titles: about 200 names in 4 parallel requests (about 150 KB); the open conversation's last 40 lines in 1 request (about 30 KB at the median, at most a megabyte) |
| total before the first line draws | 34 MB and 34 round trips live | about 1.1 MB and 3 round trips (2 + 1 parallel batch) for 4,600 lines |
| box work | one whole read of the store per piece | one whole read per skeleton piece and per content request (§10) |
| a returning device, nothing new | 1 request answering nothing | the same, and every literal it asks again is an HTTP cache hit with no round trip |

A page that reads no conversation (the Todos list, the boot) fetches nothing
of the lines, before and after, as WEB-SPEC §3.18 already requires.

These are estimates from the stores' recorded sizes; stage 2 measures them on
the fixture and on the live store's shape with `suzatary-dev bench` and the
e2e suite's network log, and replaces this table with the measured numbers.

## 10. The box's cost

Stage 1 answered a content request by reading the store's objects whole, as
every route does (about 250 ms for a chain at the live size). Since stage 2 a
content request reads each object it names from that object's own file, its
bytes checked against its name (`Prodrome::literals`), and nothing else of
the store. The live lines store holds about 6,200 objects, so a first load
that reads every line asks about 100 content requests, and each one costs its
own objects alone. The tab keeps at most four out at a time over HTTP/2, and
one over HTTP/1.1: there a browser opens at most six connections to a host,
and a chat page already holds three open for streams (its chain's, the
stores' and the conversation's), beside any pull piece in flight.

## 11. What a device holding today's replica sees

- **After stage 1** (this PR): nothing. The page is the same build and asks
  the same routes; the new routes are unused.
- **After stage 2** (the lines move): the IndexedDB version moves, and its
  upgrade drops the `store/lines` row and its pieces' rows (WEB-SPEC §3.19,
  "a replica row is read in one form": no shim reads the old form). The box
  alone writes the lines, so nothing the device sealed is in that row and
  nothing is lost: the first load pulls the skeleton (about 0.9 MB on the
  wire for 4,600 lines, against the 34 MB a device holding the live store
  held) and the content of what the page opens.
  The rendered blocks (`rendered.ts`) are content-addressed by what they
  render and stay. `gate::CLIENT_MIN` moves in the same PR, so an open page of
  the old build is told to reload rather than calling a route that no longer
  answers as it expects.
- **After stage 3** (prefetch): no row changes; the content store fills
  behind the first screen (§15).
- **After stage 4** (the inbox): the same as stage 2 for `store/inbox`.
- **After stage 5** (the chains): a chain's row holds the tab's own sealed
  acts, which must not be lost, so the upgrade does not drop a row that holds
  an object the box has not taken: it posts first (the exchange already
  pushes before it pulls) and drops the row only once nothing in it is
  unsent; a refused object is carried, named, as today. A device offline at
  the upgrade keeps reading its old row until it can post.
- **After stage 6** (deletion): nothing new; the routes it no longer calls are
  gone.

## 12. The stages

Each stage is one PR, a series of commits, each buildable and tested.

1. **The box's side** (this PR). `GET …/content?hashes=` for every store a tab
   holds (each chain, the inbox, the lines), bounded and `immutable`, refusing
   a hash the store does not hold in one set of words whoever holds it; `GET
   /api/stores/lines/skeleton?since=` sharing the objects' pull's snapshot,
   epoch, keys and erasures. Tests: a cached answer never changes after
   writes and an erasure; another store's hash is refused as an unknown one;
   the bound holds; the skeleton's pieces and the content fetched by hash
   rebuild the whole pull exactly, while the store moves (a law over random
   stores, `app/core/tests/pieces.rs`) and over HTTP (`app/cli/tests/stores.rs`).
   Nothing on the page changes.
2. **The lines in the tab.** The skeleton row and the content store
   (`web/src/prodrome/`), the memoised fold of §6 in the core's worker, the
   reading that is total or `Fetching`, the IndexedDB upgrade of §11, the
   first screen's fetch of §7, and the law of §5 over `gen.ts`. The objects
   route for the lines stays until stage 6.
3. **Prefetch, eviction, offline.** §7 and §8, with the e2e suite proving a
   conversation prefetched reads offline and one not prefetched says so.
4. **The inbox.** Its skeleton (§2's inbox header) and its stage-2 port.
5. **The chains.** Their skeleton, the content of every register's frontier
   fetched first, and the upgrade of §11 that never drops an unsent act.
6. **Deletion and compaction.** `GET /api/stores/<store>/objects` and the
   chains' whole pull are deleted with their tests once no page calls them;
   parents sent as indices into the answer if stage 2's measurement shows the
   names dominate the skeleton.

## 13. Review questions this plan answers

- *Does a device that fetched part of a store by hash always hold a state the
  box could have sent?* Yes, as a projection: §5. Its skeleton is a down-set
  the box sent in its latest epoch, its content is the box's bytes checked by
  name, and no reading is made of a part; stage 2 pins this as a law.
- *What does the first screen fetch, in bytes and round trips, before and
  after?* §9: every literal in one round trip per megabyte before (34 MB and
  34 sequential round trips live), the skeleton and one screenful after
  (about 1.1 MB in 3 round trips for 4,600 lines); stage 2 replaces the
  estimate with a measurement.

## 14. Stage 2 as built

Stage 2 holds the lines' skeleton in the tab, fetches content by name, and
makes a reading only once every line it reads is held. It differs from §6
and §7 in three ways, each left to a later stage:

- **No memoised fold yet.** A reading is the core's `Conversations` over the
  objects it needs, made once per change of the store, as before. One
  conversation's reading needs its own lines and what they rest on
  (`lines.need`), so opening a conversation fetches that conversation alone.
- **The list and a search read every line.** A list row counts turns and
  standing proposals and quotes the last line, so it reads the whole
  conversation. Until a list is read from headers and titles (§7), the
  secretary's list and the Inbox fetch the content of every line, in
  parallel answers of 64 names or a megabyte, four at once over HTTP/2 (one
  over HTTP/1.1), newest first. An
  opened conversation's names go before every whole-store batch still
  queued, taken out of that queue.
- **No measurement in §9 yet.** The table is still the estimate.

A purge fetches the content of the conversations whose lines name the keys
it owes before it opens them, and keeps the keys owed if that fetch fails.
The upgrade to version 5 leaves a mark, and the tab then clears the rendered
blocks once, since the lines they were rendered from are no longer on the
device for a purge to open.

A device holding today's replica sees this on its first load of the new
build: the database upgrade drops its lines' row (34 MB of literals on the
live store) and the pieces kept beside it, and the tab clears the blocks
rendered from them; the page pulls the skeleton,
about 400 bytes a line, with a bar under the header saying how much has
come; then it fetches the content of what the page reads. Opening one
conversation fetches that conversation; opening the list or the Inbox
fetches every line again, four answers at a time instead of one megabyte
after another. Each answer is kept for good, in the browser's cache and in
the `content` store, so a later load fetches only lines written since.


## 15. Stage 3 as built

Stage 3 prefetches the lines' content, evicts it under a budget, and says
what is not on the device when the network is gone. Since the lines moved
into the core's worker (#250), all of it runs there, in `linesync`'s
landing: the page says whether it is shown (`LinesShown`), and a
conversation the worker cannot read for want of lines answers `NotHere`
with how many it lacks.

- **The budget** (`keep.budget`) is a tenth of the origin's quota from
  `navigator.storage.estimate()`, at most 256 MiB, and 64 MiB where the
  browser gives no quota. The live store is about 54 MB in about 6,200 line
  objects, so 256 MiB holds it whole with room to grow. A tenth of the quota
  leaves the rest of the origin's storage to the chains, the rendered blocks
  (20 MB), the shell and the browser's HTTP cache.
- **The plan** (`keep.plan`) is the content of the newest conversations, each
  whole with what its lines rest on, ordered by each conversation's newest
  line, up to the first conversation that does not fit. A part of a
  conversation reads nothing, so the plan never holds one.
- **Prefetch.** After each landing of the lines, once a page has asked for
  them, the tab fetches what the plan names and the `content` store lacks. It
  sends one answer at a time, only while no other content answer is out and
  the page is shown, so a page's own fetch never waits behind more than one
  prefetch answer. A prefetch answer goes to the `content` store and not to
  the tab's memory, and the sync bar does not count it. The first failure
  stops the prefetch until the next landing.
- **Eviction.** After each landing the tab keeps the plan's content and the
  content of every line a reading of this page asked for, and drops the rest.
  A reading reads a line whole or not at all (§5), so eviction changes whether
  a reading can be made and never what it says; an evicted line is fetched
  again by name, from the HTTP cache first. The secretary's page and the
  Inbox read the list, which reads every line (§14), so while either is open
  eviction drops no line of the store, and a store past the budget is held
  whole on the device. The budget bounds what a page that reads no list
  keeps, and the prefetch; it bounds the list once the list reads headers
  and titles alone.
- **Offline.** A conversation whose lines are held reads with no network. One
  whose lines are not held, and cannot be fetched, draws "Not on this device
  yet: N lines of this conversation" with the reason, and reads again when its
  stream opens. The list, a search, the stream and what a reading left out
  still read every line (§14); offline they read only when every line is
  held, and otherwise the worker answers each of them as it answers one
  conversation (`questions.Whole`, `NotHere`), and the page says "Not on this
  device yet: N lines of the conversations" with the reason.
- **One size.** A skeleton node's `size` is what the content answer sends of
  it, the store's own print (`Prodrome::sizes`, the file's length), never its
  reprint, so a batch the tab sizes within the bound is always answered.

**The main thread, measured** (`web/e2e/linesbench.bench.ts`, 6,002 lines
and 7.6 MB, Chromium at a 6x CPU throttle, the most of one busy run each
function was on the stack for): the plan 46 to 68 ms; the eviction step 13 to
27 ms and its listing and deleting of the `content` store's keys 30 ms; the
prefetch's setup 43 ms and its queueing 39 ms; keeping one prefetch answer 68
ms and checking it 23 ms; the purge 38 ms and its search for erased keys 31 ms.
The longest busy run of each pass is not one of these: 844 ms of a first load
and 894 ms of a load are the reading of the lines (`storesync`'s reading and
`core.ts`'s), and 681 ms of a landing is the parse of the lines' row of 6,000
nodes as the landing writes it (`objects.holdRow`).

The purge still reads only the conversations whose content is held. A key the
box names erased is forgotten from every answer's `erased`, whatever is held;
a key that a closed tab writes back late is found dead only in a held
conversation.

A phone holding stage 2's replica sees this on its first load of the new
build: no database row changes, and nothing is dropped, since the live store
fits the budget. The first page that reads a conversation lands the lines, and
then the tab prefetches the content of every conversation the device lacks,
newest first, one answer of at most a megabyte at a time behind the page's own
fetches. A device whose list was opened under stage 2 already holds every
line and fetches nothing. After the prefetch every conversation reads offline.
