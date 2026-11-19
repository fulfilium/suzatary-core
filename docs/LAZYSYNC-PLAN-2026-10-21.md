# LAZYSYNC — a store's skeleton eagerly, its content by hash on demand

*Status: OPEN for roadmap `lazysync`, stages 4 to 6 (§12). Stages 1 to 3 (the
lines) have landed. `coreskeleton` (§16) is designed and built by
`docs/ONEOWNER-PLAN-2026-11-02.md` stage 3. Section numbers are kept because
other plans cite them.*

The owner's decision (2026-10): the tab syncs a small eager SKELETON of each
store (the DAG of object names and their headers) and fetches CONTENT on
demand, by hash. Content is immutable, so the box sends it to be kept for good
(HTTP `immutable`) and the tab keeps it in IndexedDB. The tab prefetches by
recency. Folds are memoised per node by one generic mechanism, the Prodrome's
memoised fold over a content-addressed tree (`prodrome_core::memo::fold`,
design §6.2), never by levels designed for one page.

## 1. Why

What a page reads first is small: a list (one title or one card per entry)
and a screenful of one thing. The rest of a store is needed only for structure
(which objects exist, what rests on what, which keys were erased), and
structure is a few hundred bytes per object, not kilobytes. A device should
fetch what it draws, not pull every store whole.

## 2. The skeleton

The skeleton of a store is, for every object the store holds, a NODE:

| field | what it is |
|---|---|
| `hash` | the object's name, the sha256 of its literal |
| `parents` | every object it rests on, its prodrome's genesis included, sorted (`prodrome_core::event::parents_of`) |
| `size` | the bytes of its literal as the store prints it (`Prodrome::sizes`), so content is asked for in bounded answers |
| `header` | what a fold reads of the object without its literal |

The header is a sum by what the object is. An object that carries no event (a
`Genesis`, a signature, a key) is `structure`, with its constructor. An
object carrying an event has the fields the store's folds key and order by:

- **lines** (landed): `conversation`, `at`, `kind` (the line shape's
  constructor), the line's step and writer, and `keys`, the name of the key of
  each slot of its words. A key is the mutable half and goes with the
  skeleton, which is never cached.
- **inbox** (stage 4): the proposal it is about and, for a response, its
  `Phase`. The inbox's state register is inflationary and read by phase, so
  the badge and the "Needs you" list read headers alone; a card's words are
  content.
- **chains** (stage 5): the todo, the event's constructor, `at` and the
  actor. A lifecycle (`Completed`, `Cancelled`, `Reopened`) and which
  `Authored` is a register's frontier are headers; the `Authored` record and a
  spec are content.

The skeleton answer carries what the objects answer carries beside its
objects: the store's heads, the genesis of every prodrome it serves and the
EPOCH (`suzatary_core::epoch`), read under the same lock and the same epoch
key, so one store has one epoch whichever route answered.

A skeleton is pulled exactly as objects are pulled: since heads, parents
first, in pieces (`routes::Cut`), each piece weighed by its nodes' bytes, with
`rest` saying how many objects are left. The pieces law of
`app/api/tests/pieces.rs` holds of it word for word: a piece joined to what
the tab holds holds everything it rests on, and the pieces asked one after
another are together the whole.

## 3. The routes

- **Skeleton**: `GET /api/stores/lines/skeleton?since=<heads>`
  (`routes::lines_skeleton`, `wire::SkeletonPayload`), `no-store`. Stages 4
  and 5 add one per store they move, the same way.
- **Content**: `GET /api/stores/<store>/content?hashes=<names>` and each
  chain's `…/content?hashes=` (`routes::content`, `wire::ContentPayload`).
  One to 64 names (`query::MOST_HASHES`), 64 hex digits each, ascending, none
  twice, so one set of objects has exactly one URL; the literals together at
  most a megabyte (`routes::MOST_CONTENT_BYTES`), one object answered whatever
  its size. The answer is the literals in the order asked, marked
  `immutable` (`reply::Immutable`, read by `reply::kept`; every other answer
  stays `no-store`). The lines' content is the exception: it is answered
  `no-store`, since a line is answered as its husk once its last key retires
  (`app/cli/src/commands/serve/stores.rs`).
- A hash the store does not hold refuses the whole question with a 404 naming
  the hashes, in the same words whether no store holds it or another store
  does: a store's content route reads that store and nothing else, so what a
  reader may read through it is what the store's whole pull would have given
  the same reader.
- An answer is a function of the hashes alone, which is what makes
  `immutable` true. A store that no longer holds an object refuses it from
  then on; a cache that kept it holds the right bytes for a name nothing reads
  any more (§7, garbage). A revoked device is answered with the wipe's
  `Clear-Site-Data: "cache"`.
- A GET, not a POST, because only a GET is cached by the browser; 64 names are
  about 4.2 KB of URL.

## 4. How the remaining pulls map onto it

Since ONEOWNER stages 3 and 4 (2.1.59, 2.1.64) the core's worker holds the
lines, the inbox and the topics as replicas (`app/core/src/replica/`), kept on
the device as the core's snapshot and frames (`replicalog.ts`). The lines'
replica holds skeleton nodes and the content store holds literals by name;
the inbox's and the topics' replicas still hold whole objects.

| today | after |
|---|---|
| `GET /api/stores/inbox/objects` (`storereads.objects`) | stage 4: the inbox's skeleton pulled into its core replica, its cards' content fetched by name into the `content` store |
| `GET /api/<chain>/objects` (whole, `Cut::Whole`) | stage 5: the same; the tab's own sealed objects stay whole (it wrote them, and they must be posted) |
| `stores.overtaken`, a reset on a later epoch | unchanged: the skeleton answer carries the epoch. Content rows are not reset: they are right for their names whatever the epoch, and go as garbage (§7) |

What a page asks of a store is unchanged. What changes is what a reading may
answer: one whose content is not all on the device answers `Fetching` (the
worker's `NotHere`) with what it waits for, never a reading of part of it (§5).

## 5. Consistency: a state the box could have sent

A device holding part of a store's content must always hold a state the box
could have sent. The state is a pair: the skeleton `S`, and the content `C`,
a set of literals each keyed by its name.

- `S` is built as a replica is: pieces each closed under ancestry joined to
  what is held, within one epoch, a later epoch resetting it. So `S` is always
  a down-set of the box's objects as the box served them in its latest epoch.
- Every literal in `C` is checked against its name on arrival (the tab
  rehashes), so `C` holds the box's bytes for those names and nothing else.
- The pair `(S, C ∩ S)` is a projection of `(S, all literals of S)`, which is
  what the box's whole pull at that point would have sent. Content outside `S`
  is garbage, not data.

The danger is a READING of a projection: a fold over a down-set with holes is
not a reading of any state the box was ever in. So a reading is total or
absent: a fold at a node needs the content of every node below it whose
content its algebra reads, and answers `Fetching(names)` until it has them. A
header-only fold (the list's order, the badge) reads headers alone and is
total on `S`. The law, over random stores and random subsets of fetched
content: every reading a tab answers equals the reading of the whole pull at
the same skeleton, or is `Fetching`.

Two tabs share the device's rows: the skeleton is joined (a semilattice), and
the content store only gains entries whose value is fixed by their key, so its
join is a union that cannot conflict.

## 6. The memoised fold

ONE mechanism: `prodrome_core::memo::fold`, whose cache key is the algebra's
name and a node's content name (`memo::Key`), with `memo::balance` giving any
long sequence a balanced shape, run in the core's worker. The lines are read
this way (`secretary::held_lines`, 2.1.17); a store moved by stages 4 and 5 is
read the same way, its tree built from its skeleton, never designed per page.
A new object renames only the chunks on the path to its leaf; a merge renames
the chunks from the fork on, and no others.

## 7. Prefetch, garbage and eviction

Built for the lines in stage 3 (§15: `keep.budget`, `keep.plan`). The rules a
moved store keeps:

- **Garbage**: a content row no node of the held skeleton names is dropped by
  the purge after a landing. An erased key is purged as today; its literal is
  ciphertext whose key is gone and goes only when its node does.
- **Eviction**: content of a node the skeleton still names may be evicted
  under the budget; it is fetched again by name. Eviction changes whether a
  reading can be made, never what it says.

## 8. Offline

Offline is `(S, C)` with no fetch: a list draws from headers and held
content; an entry whose content is not held says it is not on this device
yet, with the count, never an empty reading.

## 10. The box's cost

A content request reads each object it names from that object's own file,
its bytes checked against its name (`Prodrome::literals`), and nothing else of
the store. The tab keeps at most four content answers out over HTTP/2 and one
over HTTP/1.1, where a browser opens at most six connections to a host and a
page already holds three for streams.

## 11. What a device holding today's replica sees

- **After stage 4** (the inbox): the replica's form moves and the inbox is
  pulled once more as a skeleton. The tab writes inbox objects too, so the
  upgrade keeps stage 5's rule below (`docs/LOCALWRITES-PLAN-2026-11-01.md`
  §5.5): post first, drop the old form only once nothing in it is unsent.
- **After stage 5** (the chains): a chain's replica holds the tab's own sealed
  acts, which must not be lost, so the upgrade does not drop anything the box
  has not taken: it posts first (the exchange already pushes before it pulls)
  and drops the old form only once nothing in it is unsent; a refused object
  is carried, named, as today. A device offline at the upgrade keeps reading
  its old replica until it can post.
- **After stage 6** (deletion): nothing new; the routes it no longer calls are
  gone.

## 12. The stages

Each stage is one PR, a series of commits, each buildable and tested.

1. Stage 1 (the box's side): landed in 2.1.1.
2. Stage 2 (the lines in the tab): landed in 2.1.11, with the sync bar in
   bytes; the memoised fold landed in 2.1.17.
3. Stage 3 (prefetch, eviction, offline): landed in 2.1.23.
4. **The inbox.** Its skeleton (§2's inbox header), its content by name, and
   the stage-2 port onto its core replica, with §5's law over the inbox and
   §11's upgrade.
5. **The chains.** Their skeleton, the content of every register's frontier
   fetched first, and the upgrade of §11 that never drops an unsent act.
6. **Deletion and compaction.** `GET /api/stores/<store>/objects` and the
   chains' whole pull are deleted with their tests once no page calls them;
   parents sent as indices into the answer if a measurement shows the names
   dominate the skeleton.

## 14. Stage 2 as built

Landed in 2.1.11. A purge fetches the content of the conversations whose
lines name the keys it owes before it opens them, and keeps the keys owed if
that fetch fails.

## 15. Stage 3 as built

Landed in 2.1.23. The budget (`keep.budget`) is a tenth of the origin's quota,
at most 256 MiB, 64 MiB where the browser gives none. The plan (`keep.plan`)
is the newest conversations, each whole with what its lines rest on, up to the
first that does not fit; a part of a conversation reads nothing, so the plan
never holds one. Prefetch sends one answer at a time, only while no other
content answer is out and the page is shown, and stops at the first failure
until the next landing. A skeleton node's `size` is the store's own print, so
a batch sized within the bound is always answered.

## 16. A warm load from a snapshot (`warmsnapshot`, 2.2)

Landed in 2.1.43. Its next stage, `coreskeleton` (the core alone holds the
lines' skeleton, so a warm load reads no node row), is designed in
`docs/ONEOWNER-PLAN-2026-11-02.md` stage 3 and landed as 3a in 2.1.59, 3b in
2.1.63 and 3c (the wake and the compaction lock) in 2.1.76. What remains of `coreskeleton` is its measured target
(capped `bigstore` warm first draw under 2.5 s, the stream answered under 1 s
from asked), which that plan's stage 3 records.
