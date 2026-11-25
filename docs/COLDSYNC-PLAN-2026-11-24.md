# COLDSYNC — a new device's first load, measured, and the lines synced by need (2026-11-24)

Status: open. Roadmap `coldsyncslow` and `lazysync`. Stage 0 landed with this plan, and stages 1 and 2
after it (§4); stages 3 and 4 are named, each with the measure that decides whether it is worth building. It continues
`docs/LAZYSYNC-PLAN-2026-10-21.md` (the skeleton, the content by name, the memoised fold), which built
most of what "sync by need" means for the lines; this plan is what the measurements say is left.

## 1. Where the minute went

`web/e2e/coldsync.bench.ts` opens `#/secretary` in a browser that holds nothing, on the bigstore
fixture (14,903 skeleton nodes over 102 conversations, 400 proposals, 900 topic objects, 600 todos),
and records every request of the page and its workers, the worker's measures and marks, and the
box's log of each skeleton piece:

    SUZATARY_COLDSYNC=<file> SUZATARY_COLDSYNC_PASSES=3 scripts/web-e2e.sh coldsync
    SUZATARY_COLDSYNC_NET=none … (the box's own speed)

The phone's network is Lighthouse's Fast 4G (165 ms a request, 1.0 MB/s down, 170 KB/s up), applied
to the page and to its workers, since the core's worker makes the stores' requests. Each pass is a
new device; the first pass of a run also warms the box (its first fold of every transcript holds the
first rendered blocks for 5 to 6 s), so the figures are passes 2 and 3, a new device against a box
that has been serving, as a phone meets the live box; after is two runs, four passes. Container: 4
cores, quiet.

| | before, Fast 4G | after, Fast 4G | before, box speed | after, box speed |
|---|---|---|---|---|
| **first round drawn** (from the document's start) | **12.9, 12.8 s** | **5.9, 6.0, 6.1, 6.4 s** | **5.2, 5.0 s** | **2.4, 2.5, 2.2, 2.3 s** |
| boot, until the stream is asked | 3.5 s | 3.5 s | 0.3 s | 0.3 s |
| lines skeleton: 7 pieces, one after another | 3.5 to 8.6 s | 3.5 to 9.7–10.0 s, behind the draw | 0.3 to 2.6 s | 0.3 to 3.8–4.4 s, behind the draw |
| inbox pull | 9.1 to 9.3 s, after the whole skeleton | 3.5 to 3.9 s, beside it | 2.9 to 3.0 s | 0.3 s |
| the stream's scope read in the worker | 9.4, 9.3 s | 4.3 to 4.4 s | 3.3, 3.1 s | 0.7 to 0.9 s |
| the stream's content (one answer) | 9.4 to 9.8 s | 4.4 to 4.9 s | 20 ms | 20 ms |
| the worker answers the stream | 9.9, 9.7 s | 4.9 to 5.0 s | 3.4, 3.2 s | 0.9 to 1.1 s |
| rendered blocks: 90 requests | 10.2 to 12.9 s, 6 at once | 0.9 to 1.1 s long, 32 at once | 3.8 to 5.1 s | 0.9 to 1.3 s long |
| the cold load ends (no API read left) | 93 s | 93 s | 11 s | 10 to 11 s |

What was pulled, the same before and after (bytes on the wire from the unthrottled run, where the
browser counts them):

| | requests | decoded | on the wire | what the first screen needed |
|---|---|---|---|---|
| lines skeleton | 7, each asked from the last one's frontier | 8.65 MB | 2.59 MB | the newest conversation and the structure: 66 KB of the nodes' 6.85 MB |
| lines content | 1 for the stream, then 232 prefetched one at a time | 60.5 MB | 41 MB (sealed words do not compress) | 574 KB of 58.8 MB |
| rendered blocks | 90 | — | 15 KB | all 90 |
| shell (bundle, modules, fonts) | 21 | 10.2 MB | 2.4 MB | all |
| inbox, topics, asks, chains | 11 | 2.9 MB | 0.3 MB | the inbox whole |

Where the worker's time went, over the whole cold load: decoding 71 MB of answers 0.15 to 0.19 s;
the core taking the 7 pieces 0.29 to 0.35 s and keeping their frames 0.05 s; verifying 233 content
answers by their names 0.27 s; writing them to IndexedDB and handing them to the core 1.3 to 1.6 s.
The box took 145 to 180 ms a piece (110 to 135 ms of it reading each line's key once; the order and
the weights are kept per head set), 352 ms for the first piece on a box that had not answered one.

So the minute was a serial waterfall, not bytes or decoding:

1. The stream waits until every store it reads has landed once (`storeset.readyOf`), the inbox among
   them, and the inbox's pull was started only after the lines' WHOLE skeleton pull had ended. The
   lines land each piece as it comes, newest conversation first, but the first draw waited for the
   last piece. On a store whose bar reads tens of megabytes that is as many pieces asked one after
   another, each a round trip, the box's time and the worker's: the minute.
2. The page asked the box for the first screen's 90 rendered blocks six at a time, as if over
   HTTP/1.1; the box speaks h2.

## 2. Stage 0 (landed here): the two waits removed

- **The inbox beside the lines** (`storeset.all`). The order was there for a reason: the box files a
  proposal's record before the line naming it, and a replica holding the line without the record
  draws it `Unheld::NotHeld`. So the inbox is pulled beside the lines and again once their pull ends;
  that second pull, begun after the lines', holds every proposal they name, and is one answer from the
  inbox's frontier. Law: `web/test/warmload.test.ts`, "the inbox is pulled beside the lines, …".
- **Blocks at the link's width** (`rendered.throughAll`): `http.atOnce` reads the protocol once for
  every caller that keeps answers out, the content's fetcher (1 over HTTP/1.1, 4 over h2) and the
  blocks (6 and 32).

Neither changes a stored form or a route. The first draw on Fast 4G went from 12.8–12.9 s to
5.9–6.4 s, and at the box's speed from 5.0–5.2 s to 2.2–2.5 s; what remains on Fast 4G is the app's
boot (3.5 s, the shell's 2.4 MB), the first skeleton piece (0.7 s), the stream's content (0.5 s) and
the blocks (0.9 to 1.1 s).

## 3. Sync by need: what the measurements leave

The lines are already synced by need in every respect but one (`LAZYSYNC` §2 to §15): the skeleton is
eager and the content is fetched by name, checked by its hash, newest first, prefetched within a
budget, and read through one memoised fold per node. **The skeleton** is, per object, its name, its
parents, its content's size and its header (conversation, instant, kind, step, writer, the names of
its slots' keys), and beside the nodes the key of each slot (the mutable half, never cached), the
heads, the geneses, the epoch and the frontier: 8.65 MB decoded and 2.59 MB on the wire for the big
store, 6.85 MB of it nodes, of which the 64-digit names are 4.1 MB. The one thing still pulled
whole before the device reads anything but the stream is that skeleton, and the readings that read
`Scope::Whole`.

- **The first screen** now pulls the first skeleton piece (the newest conversations whole, about
  2,400 nodes), the inbox, the stream's content (one answer) and its blocks.
- **An old conversation, online**: its content by name (574 KB for the largest here), then its blocks;
  its skeleton is held once the pull has ended. **Offline**: drawn from what the device holds; content
  the device lacks draws as the replica's state, "not on this device yet", with the count
  (`lines.lacking`), never as an empty conversation.
- **The conversations' list** (`#/conversations`, beside a conversation on a wide screen, and the
  palette) read `Scope::Whole`, so on a new device it waited for every content object: 60 MB here. Its
  row needs each conversation's opening, title, proposals and last words; the rest is headers and the
  inbox. Stage 1 reads it so.
- **Search** reads every word and the box has no search of the lines: on a device holding part of the
  content it can only search what it holds, and must say how much that is. Stage 2 makes it say so.
- **The memory's digest** is the box's reading (`/api/secretary/memory`); it forces no pull of lines.
- **An erasure of a line whose content the device never pulled**: nothing on the device to delete. The
  skeleton's keys stop naming the erased slot, the box answers the content as its husk, and the purge
  has nothing to forget. A content object stays `no-store` everywhere, the deployment's edge included:
  a cached literal of an erased line would be a held print, which `deviceerasure` forbids.

### The stages

Each lands alone, with its law and its measure from `coldsync.bench.ts`.

1. **The list by titles** (made, §4). `lines.Scope` gains `Titles`: each conversation's title line and last line
   with words, and what they rest on. The list and the palette read it in place of `Whole`. *Law:* the
   list read over `Titles` equals the list read over `Whole`, over generated stores and fetched
   subsets. *Measure:* the wide layout's cold list, drawn after its 200 objects instead of every
   object. *Deletes:* the list's use of `Whole`; search is then its only reader.
2. **Search says its coverage** (made, §4). Offline or partial, search answers over the held content with
   "searched N of M conversations on this device"; a full search asks for the rest by name, newest
   first, as the prefetch does. No search on the box (it would read words the device has not asked).
3. **Pieces sized to the link.** The skeleton's first piece small, each later one larger, cut only at
   a conversation's end: a piece cut inside the newest conversation would hold its oldest lines (the
   order is parents first) and the stream would draw old rounds as the newest. Generic over a store's
   order, so it is Prodrome's (`prodrome_core::store::frontier`'s cut), not this repository's.
   *Measure:* the first piece on Fast 4G is 0.7 s of 5.9 s here; the stage is worth it only where the
   live first piece is more than a tenth of the first draw.
4. **The skeleton by conversation.** An eager spine (per conversation: its id, newest instant, nodes
   and bytes, about 150 bytes, so some 15 KB for the 102 conversations here) and each conversation's nodes pulled when
   it is read or prefetched. *Deletes:* nothing, and adds a second way of holding a store.

### Against it

- A device that is offline wants everything, and today's eager skeleton and budgeted prefetch already
  give it that once the first load has ended (93 s on Fast 4G here, at 41 MB of sealed words on the
  wire). Every by-need stage shortens the first screen and lengthens the time until the device holds
  what a train or a flight needs.
- Two ways of holding a store are two things to test, two upgrade paths and two answers to "is this
  conversation absent or not pulled yet". Stage 4 is that, and only a measurement on the live store
  could justify it.
- The remaining first-draw time is mostly not sync: 3.5 s of the 5.9 s is the app's boot.

### When to stop

After stage 0 the first draw on Fast 4G waits on one skeleton piece (0.7 s), one content answer
(0.5 s) and the blocks (0.9 s). After stage 2 the list waits on the boot (3.5 s), the first skeleton
piece and one or two content answers of its rows' lines, and draws every row it shows with its words
at 5.5–5.8 s on Fast 4G, beside the stream's 6.0–6.3 s: not 2 s after the stream, but before it. So stop
the lines' by-need work here unless the bench on a copy of the live store shows the first skeleton
piece above a tenth of the first draw, or the list above 2 s after the stream. Stages 3 and 4 are
named so they are not rebuilt by accident, not because the measurements ask for them.

## 4. Stages 1 and 2 (made)

**What a row reads.** A row is read from the skeleton's outline of its conversation (what it is for, when
it opened, how many turns, when it was last spoken in, which line titles it, which lines are proposals)
and the words of a few lines (`app/lines/src/secretary/row.rs`): the title, the proposals (their objects,
which the inbox joins) and the last live turn that says words, found newest first, since an act, a model
turn with calls or a blank text says none. A conversation's kind is in its opening's unsealed shape, and
the skeleton's step carries it, so no row's kind waits on words. The walk's first step reads the newest
live turn that may say words (every turn but the box's own notes, which the skeleton marks) beside the
title and the proposals, and each later step one turn more; `HeldLines::part`
reads each step as a part of its own through the one memoised fold, a round of the stream and a row
alike, so a list read again over the same lines opens no sealed word. Until a row's lines are held it
says "Not on this device yet", with its turns, its instant and how many lines are to come (exact
wherever its newest turn says words); nothing that places it is read from its words, so it never moves
when they land. Laws: `app/api/tests/listed.rs`.

**What a row does not settle.** A skeleton piece is a prefix of the store newest conversation first, each
conversation oldest line first, cut by bytes, so a piece may end inside one conversation: that row stands
with its older turns until the next piece lands, when its turns and last instant grow and it may move
up. It is the skeleton landing, not its words; stage 3 (pieces cut only at a conversation's end) is what
removes it.

**The order on a new device.** The stream's lines (`First`), then every row's (`Titles`: in `Behind` when the
list asks, first in the prefetch otherwise), then the plan's prefetch (`Ahead`). The list and a search
answer once what the device keeps of what they read is held, and never wait on the box; the worker says,
once a frame, which conversations' words came to the core and which of them are now held whole; the list
reads again only where a row it drew waits on one of them, and a search only where one it has not read
became whole. A row drawn with its words that a new line leaves waiting is answered once that row's own
lines are held, never once every row's are. A fetch behind them that fails
is asked again after a wait that doubles from two seconds to a minute. Nothing is fetched that
`Whole` did not fetch before: the rows' lines come earlier, and only into the stores that held them.

**Search.** It reads the conversations whose every line, and every node they rest on, is held, says
"Searched N of M conversations on this device" while N < M, and "of the M this device knows of so far"
while the skeleton's pull has not ended whole; it asks for the rest by name, newest first, behind its
answer, and reads again as they land; it never says that nothing matches while it has not read them all. It is the last reader of
`Whole`, as a fetch behind its answer.

**Measured** with `web/e2e/coldsync.bench.ts` (`SUZATARY_COLDSYNC_PAGE=conversations` opens the list),
passes 2 and 3 of three, on the generated store of §1 (every figure here is of that store, none of a
deployment), in the same container before (master at 2.1.245) and after:

| | before, Fast 4G | after, Fast 4G | before, box speed | after, box speed |
|---|---|---|---|---|
| **the list, every row shown with its words** | **51.9, 51.2 s** | **5.5 to 5.8 s** | **8.9, 9.0 s** | **1.8 to 2.3 s** |
| the list's first row (its words not yet here) | with its words, above | 4.3 to 4.5 s | with its words, above | 0.8 to 1.2 s |
| content objects pulled before the list drew | 14,903 of 14,903 | 446 of 14,903 (8 answers) | 14,903 | 496 |
| the skeleton whole (rows of older conversations join below) | before the list | 10.4, 10.7 s, after it | before the list | 4.5, 5.7 s |
| **the stream's first round** | **6.0, 6.1 s** | **6.0, 6.3 s** | **2.10 to 2.67 s** | **2.33 to 2.55 s** |
| content objects pulled before the stream drew | 62 | 62 | 62 | 62 |
| the cold load ends | 93.3, 93.8 s | 93.8, 94.1 s | 11.9 to 15.5 s | 11.7 to 17.3 s |

The box-speed figures are three runs of passes 2 and 3 or 2 to 4, whose spread is the container's: the
stream's first round is not slower, and pulls the same 62 objects before it draws. A row's lines are more than the "two objects" §3 guessed: the opening
where a title replaced it, the proposals (400 here) and the reseals and erasures their words are read
through, so 446 objects rather than about 200. The list's rows shown hold no row waiting for its words
after 5.8 s on Fast 4G; the skeleton's later pieces bring older conversations, which join below the
30 rows a page shows.
