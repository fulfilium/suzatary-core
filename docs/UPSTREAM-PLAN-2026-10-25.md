# UPSTREAM PLAN: what Suzatary built that belongs in the Prodrome, and how it moves (2026-10-25)

*Status: a design. Nothing here is built. The main session writes the roadmap
edits of §3; this document only lists them.*

Sources read: `CLAUDE.md`; `AGENTS.md`; `docs/ARCHITECTURE.md` §1, §4, §5 and
§11.1 ("Every store a replica in the tab"); `docs/LAZYSYNC-PLAN-2026-10-21.md`;
`docs/TOPICS-PLAN-2026-10-21.md` §1 to §3; `app/Cargo.toml` (the pin and the
seam comment, lines 3 to 68); every file in `app/core/src/store/`,
`web/src/prodrome/`, `app/wasm/src/`; `app/core/src/{routes,sync,epoch,sole,envelope,literal,table}.rs`;
`app/core/src/secretary/{held_lines,digest,digest_view,compaction,texts,topics/*}.rs`;
`app/core/src/signatures/{verified,trust,registry}.rs`; the open pull requests
#240, #250 and #252 (descriptions and branches) and the merged #241 and #245.
On the Prodrome side: `README.md`, `SPEC.md` (§3, §6, §9, §10), `AGENTS.md`,
`docs/roadmap.md`, `docs/design-register-types.md` §6.1 to §6.5, and
`core/src/{dag,memo,digest,store/*,fold/*}.rs` and `wasm/exports/src/lib.rs`
at `main` (`fba46ae`). `git diff de71ccc fba46ae -- core wasm` is empty, so
every Prodrome line number below holds at the pin as well. The roadmap is the
`roadmap-data` branch (`097c6db`, 2026-10-17), read with `prodrome list` and
`prodrome show` built from `main`: 73 objects, 17 open items, `verify` clean.

Line counts are `scripts/size.sh`'s rule: Rust up to a file's first
`#[cfg(test)]`, TypeScript outside `generated/`, no blank or comment lines.
Suzatary lines are at `origin/master` (`007868d2`, 2.1.17) unless a branch is
named.

## 0. The finding in one paragraph

The pin is `de71ccc` (Prodrome #44, 2026-10-06; this repository moved to it
on 2026-10-12, `7a533cf6f`). Since that move, 118 commits landed on master's
first-parent line. Over them, `web/src/prodrome/` grew from 2,120 to 3,088 lines (18 to 26 files),
`app/core/src/store/` from 859 to 920, and the count of Rust signatures
generic over a Prodrome schema (`<E: Schema>`, `<E: Store>`, `<S: Says>`) in
`app/` from 45 to 62. The Prodrome's code did not move. Most of the growth is
one subject, a replica that syncs with a store of record over a wire: what a
replica lacks, the pull cut into pieces, the era of the store's answers, the
skeleton and content split, the frontier digest, an incremental held replica,
and the browser's copy of all of it. The Prodrome has the in-process half of
that subject (`store::sync`, `store::accept`, `Replica`, law 35) and none of
the wire half. Two of its open roadmap items (lazy content by hash, mergeable
tag sets) and one more (`signature-proof-cache`) are built here. Twelve
stages below move the generic pieces up, each one deleting the local copy.

### 0.1 Where each piece goes, by repository

The owner decided (2026-10-25, relayed by the main session) that a new GitHub
organization, `fulfilium`, holds the public family. `prodrome` and
`suzatary-core` move into it. The web framework of the 2.2 split (the
algebra, the shell and the e2e harness) gets a public repository of its own
there, rather than a package inside the Prodrome. The Prodrome's roadmap
leaves its orphan branch for a repository of its own, and the Prodrome's CI
keeps reading it as a test case. No repository exists yet, and this plan
creates none. Below, "the framework repository" is that repository, whatever
the owner names it.

The rule: a store, sync or replica piece goes to `fulfilium/prodrome`, in
Rust or in TypeScript. A generic piece under `web/src` that is not a store
goes to the framework repository. Everything else stays in Suzatary.

| piece | § | destination |
|---|---|---|
| what a replica lacks, the pull and its pieces | 1.1 | `fulfilium/prodrome`, core crate |
| the era of a store's answers | 1.2 | `fulfilium/prodrome`, core crate |
| the skeleton node and content by name | 1.3 | `fulfilium/prodrome`, core crate |
| the frontier digest | 1.4 | `fulfilium/prodrome`, core crate |
| `Dag::grow` public, `memo::by_heads` | 1.5 | `fulfilium/prodrome`, core crate |
| `memo_name`, `schema!` with a composite key | 1.5, 1.8 | `fulfilium/prodrome`, `wasm/exports` |
| the literal codec, the envelope, the one-register product, the schema law kit | 1.6, 1.11 | `fulfilium/prodrome`, core crate |
| the tag sets | 1.7 | `fulfilium/prodrome`, core crate |
| the proof cache, `digest::cover` | 1.10, 1.11 | `fulfilium/prodrome`, core crate |
| the browser replica: heads, intents, pieces, the replica and row joins, content admission and eviction, `Place`, `app/idb.ts` | 1.9 | `fulfilium/prodrome`, a TypeScript package beside `wasm/exports` |
| `algebra/lo.ts`, `algebra/source.ts`, `algebra/once.ts`, `algebra/memo.ts`, and the worker's question port (`questions.Port`, `remote.over`) | 1.9 | the framework repository, by the 2.2 split |
| the registry, the tiers, sealed words, the trust registry, the prefetch budget, every reading of this application's schemas | 1.11 | Suzatary |

Two consequences. First, the Prodrome's TypeScript package may not import
from the framework repository: the database sits below the web framework,
and a dependency the other way would make every Prodrome reader take a web
framework. So stage 12 starts by replacing the replica files' imports of
`lo.Settled`, `once` and `source` with types the package declares itself.
Second, `algebra/memo.ts` is a second implementation of the Prodrome's
`memo::fold` that lands outside the Prodrome. It stays in agreement with the
Rust only through frozen vectors: the Prodrome publishes `memo::name` and
`memo::balance` vectors in `wasm/conformance/` (stage 8), and the framework
repository's tests read them.

## 1. The inventory

Each entry names the Suzatary code, its counted lines, the Prodrome module it
would join, and what must change first. "Generic as written" means the code
names no Suzatary type and could move with a rename of its error type.

### 1.1 What a replica lacks, and the pull cut into pieces

- **Here.** `routes::lacking` (`app/core/src/routes.rs:614`), `sync::missing`
  (`app/core/src/sync.rs:536`), and the reference module's `since`
  (Prodrome `wasm/exports/src/lib.rs:1074`) are three implementations of one
  function: the objects outside the ancestry of every held head the caller
  names. Around the first: `Cut` (`routes.rs:560`, `Whole` or `Bytes(n)`,
  `PIECE` one MiB at `:572`), `Piece<T>` (`:669`), the trait `Sends<E>`
  (`:678`, what a pull sends of each object), `Literals` (`:696`),
  `stored` (`:631`) and `answered` (`:747`), which linearises the wanted
  objects, cuts them by weight and counts `rest`. About 116 lines for `Cut`
  to `Literals`, 64 for `answered`, 17 for `missing`.
- **Generic?** `lacking`, `Cut`, `Piece`, `Sends` and `Literals` are generic
  as written. `answered` names `wire::*` payloads and `crate::epoch::Epochs`
  (§1.2), and keys its lock as `"store/<name>"`; its pure part (linearise,
  cut, weigh, count) is generic and its shell (the lock, the epoch's record,
  the wire payload) stays here.
- **Joins.** A new `store/pull.rs` beside `store/replica.rs`. `Dag::closure`
  (Prodrome `core/src/dag.rs:233`) is already the ancestry; `since` becomes
  `Dag::lacking(heads)` and the reference module calls it.
- **Law.** `law_the_pieces_union_is_the_whole_and_each_is_ancestor_closed`
  (`app/core/tests/pieces.rs:207`) runs over `LineEvent`. Its statement names
  no line: a piece joined to what the caller holds is closed under parents,
  and the pieces asked one after another from the heads each left are
  together the whole pull. It moves up over a neutral schema as a new SPEC
  §9 law (47, see §3.3).

### 1.2 The era of a store's answers (the epoch)

- **Here.** `app/core/src/epoch.rs` (97): `Seen` (`:16`), `Served` and
  `Served::after` (`:27`, `:41`), the rule that an answer which extends the
  last one (the same geneses, every head answered last still held) keeps
  its epoch, and any other answer (a migration, a drop, a rollback) starts a
  later one, never below the box's clock in seconds. `Epochs` (`:62`) keeps
  the record in a `Doc` under a lock per store; `app/io/src/epochs.rs` (44)
  is its file. On the tab, `locked.join` (`web/src/prodrome/locked.ts:54`)
  and `replica.wentBack` (`web/src/prodrome/replica.ts:269`) order a row by
  generation, then by epoch, then join by union.
- **Generic?** `Seen` and `Served::after` (about 34 lines) are generic as
  written: they read names and heads only. `Epochs` names Suzatary's
  `store::Doc`, `Answered` and `capability::Datetime`, and stays as the
  effectful shell.
- **Joins.** `store/pull.rs` (§1.1), or a file `store/era.rs` beside it. The
  Prodrome has no counterpart: law 35 assumes a replica only grows, and the
  epoch is how a replica learns that the store of record holds less than
  before. That is a fact about every store of record that can be restored
  from a checkpoint, not about this application.
- **Law.** A new law (48): an answer that extends the previous answer keeps
  its epoch, any other answer gets a strictly later one, and the replica's
  join ordered by (epoch, then union) is a semilattice in which a later
  epoch drops everything of the earlier ones. Today the first half has no
  property test here (the rule is exercised through `app/cli/tests/stores.rs`
  over HTTP), and the second is `web/test/replica.test.ts` over the tab's
  types.

### 1.3 The skeleton and the content (lazy content by hash)

- **Here.** Built in #212 (stage 1) and #231 (stage 2); #240 (stage 3,
  prefetch, eviction, offline) is open. The skeleton of a store is one node
  per object: `hash`, `parents`, `size` and a `header`
  (`wire::SkeletonNode`, `app/core/src/wire.rs:901`; `NodeHeader`, `:915`).
  `skeleton_node` (`routes.rs:914`), `constructor` (`:941`) and `weight`
  (`:957`) build it (56 lines); `Nodes` (`:719`) is the `Sends` that emits it.
  Content is `GET …/content?hashes=`: `routes::content` (`routes.rs:990`, 38
  lines with `MOST_CONTENT_BYTES` at `:979`) over `Prodrome::literals`
  (`app/core/src/store/prodrome.rs:81`; on disk `app/io/src/objects.rs:208`,
  which reads one object file by name).
- **Generic?** `constructor` and `content<E>` are generic as written. The node
  is generic except its header: `NodeHeader::Line { conversation, at, kind,
  keys }` holds a line's conversation and the keys of its sealed words, and
  `weight` reads it. `Nodes` is implemented for `LineEvent` alone.
- **What must change first.** The header must become the schema's: what a
  fold keys and orders an object by, without its literal. For every schema
  that is the entity, `at`, the actor and the constructor, which is exactly
  the head that §1.6's envelope already separates; a schema may add fields
  (the lines add the names of their keys). So the header is an associated
  type of the schema with a default of that head, and this stage waits for
  §1.6.
- **A correction to the roadmap item.** `lazy-content-by-hash` proposes to
  "split each object into a small skeleton … and its content, addressed by
  its own hash", which is a change to the stored format. Suzatary did not do
  that and did not need to. The header is derived from the stored object,
  and the content is the object's own literal, addressed by the object's own
  name. No stored byte changed (LAZYSYNC §2, §3). So the item needs no new
  constructor, and its dependency on `event-header-body-split` is a
  dependency on a type, not on a stored format.
- **Joins.** `store/skeleton.rs` beside `store/pull.rs`, and a `Replica`
  method `literals(names)` beside `Replica::print` (Prodrome
  `core/src/store/replica.rs:55`), which already answers one print by name.
- **Law.** `law_the_skeleton_and_the_content_by_hash_are_the_whole_pull`
  (`app/core/tests/pieces.rs:344`), `a_content_answer_never_changes`
  (`:428`) and `the_content_bound_holds` (`:495`) move up over a neutral
  schema as law 49. `another_store_s_hash_is_refused_as_an_unknown_one_is`
  (`:467`) is a privacy boundary between this deployment's stores and stays.

### 1.4 The frontier digest a pull names its replica by (#252, open)

- **Here, on `claude/pull-frontier-digest`.** `app/core/src/frontier.rs` (137):
  `Digest` (`:18`) and `Digest::of(heads)` (`:28`), a 64-character name for a
  set of heads; `Frontiers` (`:90`), the store of record's bounded record of
  the frontiers it answered with (`KEPT = 512`, `:86`; `record`, `:137`); the
  `Unknown` answer when a digest is no longer held. The tab's side is
  `Frontier`, `laterFrontier` and `sinceOf` in `web/src/prodrome/replica.ts`
  on that branch. On master the digest is already spelled inline twice, as
  `memo::name(b"heads", …)`: `app/core/src/store/memoised.rs:19` and
  `app/core/src/signatures/trust.rs:380`.
- **Generic?** `Digest`, `Frontiers`' pure part and the fallback rule are
  generic. The record's file (`app/io/src/frontiers.rs` on the branch) is
  the shell.
- **Joins.** `store/pull.rs`. This is the same problem Automerge's sync
  protocol and git's negotiation solve: a replica must say what it holds in
  a bounded message. The Prodrome's answer here is a name the store of
  record recorded, which costs the store a bounded record and costs the
  replica nothing.
- **Law.** `tests/frontiers.rs` on the branch: safe (never more than the
  pull from the heads would send, never less), complete, quiet (a replica
  that holds everything gets nothing), and falls back once. Law 50.

### 1.5 The memoised fold over a store, and the held replica (#245; #250 open)

- **Here.** `app/core/src/secretary/held_lines.rs` (634) holds the lines as a
  replica that grows by prints: each print decoded once (`prints`, `:32`),
  the objects waiting on a parent (`lacking`, `:36`), the down-set kept as
  prints arrive (`place`, `:512`; `enter`, `:537`; `rebuild`), and what is
  left out (`excluded`, `:477`). About 114 counted lines from `excluded` to
  `rebuild` are that replica. The rest is the reading: a tree of three
  levels, the store, one node per conversation and one leaf per line, read
  through the Prodrome's `memo::fold` (Prodrome `core/src/memo/fold.rs:151`)
  with an algebra that opens sealed words at the leaves.
  `app/wasm/src/memo.rs` (13) exports `memo::name` to the tab, and
  `app/core/src/store/memoised.rs` (32) keys a reading of a store by its
  heads in a `memo::Cache`. `web/src/algebra/memo.ts` (99) is a second
  implementation of `memo::fold` in TypeScript, used by the typst pages
  (`web/src/typst/page.ts`, `tree.ts`).
- **Generic?** The held replica is generic in shape and not as written: its
  `Print` enum has a `Line` arm holding a `LineEvent`, and `enter` files each
  line under its conversation and its keys. The Prodrome already has the
  same walk: `Dag::grow` (Prodrome `core/src/dag.rs:148`) is
  `pub(crate)`, and its own `AGENTS.md` lists "`dag.rs` has its own Kahn
  walk to admit objects whose parents are missing" as an exception to rule
  7. `held_lines.rs` is a third copy of that walk. `memoised.rs` is generic
  as written. `memo.rs` is generic as written; the design note says an
  export is added "when one reads a cache through the module"
  (design-register-types.md §6.2, "No wasm export"), and Suzatary is that
  reader. The three-level tree is generic once its middle level is the
  set of prodromes' member key (§6.3.1) rather than a conversation id: a
  store's reading is one node per member, one leaf per object in
  linearisation order, balanced by `memo::balance`.
- **What must change first.** `Dag::grow` public, with a removal (the
  `rebuild` after prints are let go). Then `HeldLines` holds a `Dag<LineEvent>`
  and keeps only its own indices (conversation, key owners).
- **Joins.** `dag.rs` (grow made public) and `memo/` (`memo::by_heads`, the
  heads-keyed tabulation; a `memo::Nest` tree builder over a set of
  prodromes). The algebra stays the host's, as the design note says.
- **Law.** `app/core/tests/held_lines.rs` states that the held reading
  equals the plain reading of every print at once, after any sequence of
  arrivals and drops. Over a neutral schema that is law 44 read
  incrementally, and it moves up as a case of law 44, not a new law.

### 1.6 The event envelope and the one-register product (#241)

- **Here.** `app/core/src/envelope.rs` (116): `Event<S: Says>` (`:18`), an
  event printed as `Constructor(<key>, at, actor, <own fields>)`, with the
  trait `Says` (`:26`) for what a schema adds and `impl Schema for
  Event<S>` (`:127`). The memory, the digest, the decisions and both topic
  stores use it. `app/core/src/sole.rs` (59): `Only`, the trait `Sole`
  (`:18`) and `One<'a, E>` (`:41`), the `Product` of a schema whose events
  write one register. The memory, the digest, the lines and both topic
  stores use it. `app/core/src/literal.rs` (244): the trait `Literal`
  (`:18`), `record` (`:31`), `headed` (`:47`), `Fields` (`:61`), the macros
  `natural!` and `text_literal!`: a codec between a Rust value and a
  Prodrome literal, which `envelope.rs` builds on.
- **Generic?** All three are generic as written. They name no Suzatary type;
  `envelope.rs` depends on `literal.rs` and nothing else of this crate.
- **Joins.** `envelope.rs` is the roadmap's `event-header-body-split` for any
  application schema: one header (the entity, `at`, `actor`) and a body.
  The Prodrome's `TodoEvent` (Prodrome `core/src/event.rs`) still repeats
  the three fields in each variant; the envelope is the shape it would take
  under that item. `literal.rs` is the roadmap's `one-literal-codec`
  (SIMPLIFY-PLAN stage 4 plans "a trait `literal::codec` with the functions
  `to_value` and `from_value`", which is `Literal` here). `sole.rs` joins
  `fold/product.rs` beside the todo schema's product (Prodrome
  `core/src/fold.rs:106`) and the declared schema's (`declared/event.rs:230`).
- **Law.** `Literal`'s round trip (`from_value(&x.to_value()) == Ok(x)`, the
  doc at `literal.rs:17`) moves up as a property test over neutral types.
  The envelope and `One` are covered by the schema laws the Prodrome runs
  already (laws 31, 33, 37) once a neutral `Says` schema is among their
  cases.

### 1.7 Mergeable tag sets (the topic stores)

- **Here.** `app/core/src/secretary/topics/`: `membership.rs` (18, `Presence`
  with `Off < On` and its `Order`, `:18`, `:23`), `membership_event.rs` (92,
  the membership register under `Sole`), `alias_event.rs` (77, the alias
  register, discrete), `dispute.rs` (15), and `reading.rs` (225:
  `Reading::of`, `:35`; `follow`, `:237`, which follows aliases and reads a
  ring as its least name). The merge law is TOPICS-PLAN §2: an
  observed-remove set with add-wins, read as the greatest value of the
  membership's frontier. `app/wasm/src/topics.rs` (158) carries a `replica!`
  macro (`:25`) that copies part of the Prodrome's `schema!`
  (`wasm/exports/src/lib.rs:224`) because `schema!` cannot name a key that
  is a pair.
- **Generic?** The two registers and the reading are generic in shape and not
  as written: they name `Topic` and `Target`. A `Topic`'s rules (one to three
  lowercase words, at most 24 characters, `topic.rs`) are this
  application's vocabulary and stay. `plan.rs`, `carry.rs`, `backfill.rs`,
  `stores.rs`, `target.rs` and `targets.rs` (777 lines together) are the
  secretary's use of the set and stay.
- **What must change first.** The registers become generic over a target
  type and a tag type, each a `Literal` (§1.6). The schemas use §1.6's
  envelope, so this stage waits for it.
- **Joins.** A Prodrome module `tags/` (two schemas, `Membership<T, G>` and
  `Alias<G>`, and the reading), or two declared schemas (`declared/`) with
  the reading in the library. The roadmap item `mergeable-tag-sets` asks for
  exactly this, and its text ("an add and a remove are events, an
  observed-remove set … a rename or a merge of two tags is an alias event")
  matches what was built.
- **Law.** From `app/core/tests/topics.rs`:
  `merging_is_commutative_associative_and_idempotent` (`:251`),
  `an_add_and_a_remove_apart_merge_the_same_way_in_either_order` (`:279`),
  and the four alias tests named in TOPICS-PLAN §3. Law 51.

### 1.8 The wasm exports

- **Here.** `app/wasm/src/lib.rs` instantiates the Prodrome's `schema!` for
  `Todos` and `Proposals`. Generic beyond the Prodrome's exports: `memo_name`
  (§1.5, 13 lines) and the `replica!` macro (§1.7). `signatures.rs` (197) has
  one near-generic function, `signed` (`:19`). The rest (`chains.rs`,
  `todos.rs`, `evaluator.rs`, `lifecycle.rs`, `secretary.rs`, `rendering.rs`,
  `prose.rs`, `tripwire.rs`, `unrendered.rs`) is this application's.
- **What must change first.** `schema!` must take a key that is not one
  field. Then `replica!` is deleted and `Memberships` is a `schema!` class.
- **Joins.** `wasm/exports/src/lib.rs`; the module classes stay here.
- **Law.** None new: a composite key is covered by the exports' existing
  tests once one schema in `wasm/exports/src/review.rs`'s style has a pair
  key.

### 1.9 The browser replica (`web/src/prodrome/`, 3,088 lines)

Generic as written, with no Suzatary name: `heads.ts` (12), `intents.ts`
(71, a two-phase set of asks retracted when the history holds them),
`pieces.ts` (31, the pull loop), `readings.ts` (9). Generic once a type
parameter replaces `StoredObject | SkeletonNode` (the wire's types):
`replica.ts` (288) less `unsentTodos` (`:467`), `locked.ts`'s join (46; the
order is §1.2's epoch rule), `take.ts` (25), `content.ts`'s `admit` (`:66`),
`fetcher` (`:119`) and `keepOnly` (`:172`) (142 in all), the closure in
`lines.ts`, `excluded.ts` (79, its messages say "chain"), and `verify.ts`
(137, whose `isRoot` matches the legacy `Sealed(prev='',`). On #250's
branch, `pull.ts` (274; `Place`, the storage a row lives in) and
`linerows.ts` (214, one IndexedDB row per node) are generic in shape and
name `stores.Name`. `web/src/algebra/memo.ts` (99) is §1.5's second `memo`.
The helpers they lean on are `app/idb.ts` (55), which goes with the replica,
and `algebra/lo.ts` (38), `algebra/source.ts` (24) and `algebra/once.ts`
(27), which go to the framework repository (§0.1).

Application code that stays: `asks.ts`, `intentsync.ts`, `core.ts`,
`prodrome.d.ts`, `questions.ts`, `stores.ts`, `progress.ts`, `folds.ts`,
`vouch.ts`, `purge.ts`, `sync.ts`, `storesync.ts`, and on the branches
`linesync.ts` and `keep.ts`. The keys of sealed words in `locked.ts` and
`purge.ts` stay with §1.11.

- **What must change first.** `replica.ts`'s `parentsOf`, `NAMED_GENESIS` and
  `SIGNS` (`:271`, `:348`, `:370`) read parents and geneses out of a
  literal with regular expressions. That is a second reader of the
  Prodrome's grammar ("A hand parser for somebody else's language is a
  second reading of it", CLAUDE.md Log 2026-09-07). Before the module moves,
  those reads go through the wasm module, which already answers `tips` and
  `since`.
- **Joins.** The Prodrome has no browser replica: `viewer/src/main.ts:67`
  fetches a static `index.json` and `objects/<hash>.py`, and no file under
  `viewer/` keeps a replica, heads or a join. So this is a new package in
  `fulfilium/prodrome`, a TypeScript module beside `wasm/exports`, the way
  `automerge-repo` sits beside `automerge`: not the core crate, and not the
  framework repository, which holds no store (§0.1).
- **Law.** `web/test/heads.test.ts`, `intents.test.ts`, `pieces.test.ts`,
  `replica.test.ts`, `replicas.test.ts`, `excluded.test.ts`, `verify.test.ts`
  and `memo.test.ts` move with their modules, over `gen.ts`-style generated
  neutral data. The vectors that pin TypeScript to Rust (`memo::name`,
  `memo::balance`, a pull's pieces) become frozen files in the Prodrome's
  `wasm/conformance/`, which the package's tests read.

### 1.10 The digest and the compactor

- **Here.** `app/core/src/secretary/digest.rs` (363), `digest_view.rs` (273),
  `compaction.rs` (386), `compactor.rs` (46), `texts.rs` (68). They read a
  conversation through the Prodrome's `digest` (Prodrome
  `core/src/digest.rs:219` `tree`, `:426` `view`, `:350` `pending`, `:375`
  `zoom`).
- **Generic?** One function is generic in shape: `Digest::cover`
  (`digest.rs:343`), the largest nodes that tile a span, beside the
  Prodrome's `zoom`. It is written over `digest::Node<Label>`, and `Label` is
  this application's measure. Everything else is this application's: the
  `DigestEvent` schema holds summaries as salted words, `digest_view::fit`
  trims text to a byte budget, the `Paced`/`Claim`/`Round` pacing in
  `compaction.rs` (`:281`) is tied to the model's budget, and `texts::dead`
  (`:107`) is the sweep of erasable words (§1.11).
- **Joins.** `cover` joins `digest.rs` once it takes any `Measure`.
- **The roadmap item `digest-objects`** waits for "the first program that
  reads a log through a digest". Suzatary is that program, and its
  `DigestEvent` defines a line and a summary as objects. The schema is not
  generic as written (its summary names salted words), so the item is
  unblocked, not built.

### 1.11 What stays in Suzatary, and why

- **The store registry** (`app/core/src/store/kind.rs`, 99; `stores!` at
  `:71`, rows at `:117`). Each row's `Tier` (`Pushed`, `Local`, `Cache`) and
  `Files` are this deployment's policy on where data may go. The Prodrome's
  answer to "several stores" is a set of prodromes (design §6.3.1), which
  needs no registry. One piece of #241 is generic: `app/io/tests/store_laws.rs`
  runs one law suite over every prodrome row. The Prodrome could export that
  suite as a test-support feature, so that every application schema is
  checked by the laws the reference schemas are checked by (stage 3).
- **`Database`, `Area`, the in-memory `Doc` and `Log`**
  (`store/database.rs`, `doc.rs`, `log.rs`, `in_memory_*.rs`). These are
  key-value and log stores, not prodromes.
- **Sealed words and erasure** (`secretary/locked_text.rs` 207,
  `salted_text.rs` 78, `words.rs` 100, `key_ring.rs`, `texts.rs`, and the
  keys in the tab's `locked.ts` and `purge.ts`). The mechanism is generic
  (each slot of an object's words sealed under its own key, kept beside the
  store; erasure destroys the key and the object stays). But a library that
  holds keys takes on key custody and a threat model, and SPEC §1 states
  that the Prodrome rewrites no object; it says nothing about secrecy.
  Automerge leaves encryption to the application for the same reason. It
  stays here until a second program needs it.
- **The signing decorator and the trust registry**
  (`store/signing.rs` 90, `signatures/registry.rs` 358, `custody.rs`). The
  decorator is generic and would sit beside `store/overlay.rs`; the registry
  decides which actor a key may sign for, which is this deployment's
  policy. Only `signatures/verified.rs` (31) moves (stage 11): it is the
  roadmap's `signature-proof-cache`, built.
- **The prefetch budget and plan** (`keep.ts` on #240's branch) and the
  IndexedDB names (`objects.ts`: `DB_NAME = "suzatary"`, the row keys and the
  version history). A device's budget is the application's choice.

## 2. Reviewed as two reviewers would

**The author of a CRDT library such as Automerge.** The core of such a library
holds the data type, its merge and the sync protocol; Automerge's sync
protocol (a bounded summary of what each side has, and the changes the other
lacks) is in the core, and its storage adapters and network adapters are in
`automerge-repo`, a separate package. By that line:

- The core crate takes §1.1 (what a replica lacks, and the pull), §1.2 (the
  era), §1.3 (the skeleton, content by name), §1.4 (the frontier digest),
  §1.5's `Dag::grow` and the heads-keyed tabulation, §1.6 and §1.7 (a data
  type with its merge law). Each is a function of objects and heads, with
  laws.
- A separate package takes §1.9 (the browser's rows in IndexedDB, the
  fetcher's lanes, the pull loop's effects), as `automerge-repo` does. The
  core crate must not learn about IndexedDB. The package stays in
  `fulfilium/prodrome`, and the generic web pieces that are not a store go
  to the framework repository (§0.1), so the database never depends on the
  framework.
- The application keeps the registry, the tiers, the prefetch budget, the
  sealed words and the trust registry.

The reviewer would also ask whether the pull should be a protocol rather than
functions: two messages and a state per peer. Here the pull is a question
the replica asks (its frontier) and an answer the store of record gives (a
piece and `rest`). That is a protocol with one side holding state (the
`Frontiers` record), and it stays one, because the store of record is the
one side that is always there. A peer-to-peer sync between two tabs is not
asked for, and SPEC §10 excludes a hosted service, not peers; the stage
leaves the door open by keeping the pure functions apart from the HTTP.

**Rich Hickey: is each piece moved up one simple idea, with no application
concept carried along?**

- "What a replica lacks" is one idea, and today it is three functions.
  Moving it makes it one.
- "A pull cut into pieces" is one idea: a linearised down-set cut by weight.
  `Sends` must not carry the box's lock or the wire payload with it.
- "The era" is one idea: an answer that does not extend the last starts a
  later era. It must arrive without the word "box", without the `Doc` it is
  kept in and without the clock floor's source (the floor is an argument).
- "A node of the skeleton" is one idea: an object without its literal. The
  lines' `keys` and `conversation` are the application's header fields and
  stay out; the Prodrome's header is the schema's head.
- "A name for a set of heads" is one idea. "A bounded record of the names a
  store answered with" is a second idea, and the two ship as two types.
- Tags are two ideas, a presence register and an alias register, and they
  are two schemas, as TOPICS-PLAN §1.1 already chose. A `Topic`'s spelling
  rule is a third idea and the application's.
- The held replica is "a `Dag` that grows by prints". The conversation index
  beside it is the application's and does not move.

What would complect if moved as written: `answered` (pull, lock, epoch
record and wire payload in one function), `held_lines.rs` (a replica, an
index and sealed words in one struct), `locked.ts` (a replica, an epoch order
and keys of words in one row), and `replica.ts` (a replica and a regular
expression reader of the grammar). Each stage below moves the simple part and
leaves the rest.

## 3. The roadmap

### 3.1 Items already built downstream

| item | where it is built here | what is left for the Prodrome |
|---|---|---|
| `lazy-content-by-hash` (75) | #212, #231 (merged); #240 (open). §1.3 | the generic node, content by name and the law (stages 4, 6). The proposed stored-format split is unnecessary |
| `mergeable-tag-sets` (85) | #229, #234, #239 (merged). §1.7 | the two schemas generic over target and tag, and the laws (stage 9) |
| `signature-proof-cache` (90) | `app/core/src/signatures/verified.rs` (31) | move the table beside `Proof::of` (stage 11) |

Partly built here:

| item | what is built here |
|---|---|
| `event-header-body-split` (90) | `envelope::Event<S: Says>`, the split for every application schema (§1.6); `TodoEvent`'s own split is not done |
| `one-literal-codec` (88) | `literal::Literal`, the codec SIMPLIFY-PLAN stage 4 plans, for application values (§1.6) |
| `digest-objects` (90) | `DigestEvent`, the first program's objects for lines and summaries; not generic (§1.10) |

### 3.2 The edits, for the main session to write

`prodrome revise` takes a price and no new detail, and `prodrome add` writes a
detail only on a new item. So an edit to an existing item's text is a
`revise` with the reason in `--note`, unless the main session first adds a
verb that records a new detail (itself an edit to the CLI, outside this plan).
Every edit is written on a checkout of the roadmap store, by pull request, as
`docs/roadmap.md` says: today the `roadmap-data` branch of
`bmabsout/prodrome`, and after the move the roadmap's own repository in
`fulfilium` (§0.1). The objects are content-addressed, so the same `prodrome`
commands write the same bytes in either place. Prices use `PRICING.md`'s scale (70 is "a
clearly useful feature that someone is waiting on"; a dependency is priced as
a reference to what it waits on).

Edits to existing items:

1. `lazy-content-by-hash`: `prodrome revise lazy-content-by-hash --priority 65
   --note "Built downstream in an application without a stored-format change: the skeleton node (hash, parents, size, header) is derived from each object, and content is the object's own literal fetched by the object's own name. The work left is the generic node, a Replica method that answers literals by name, and the law that skeleton and content rebuild the whole pull. An application is waiting on it."`
2. `mergeable-tag-sets`: `prodrome revise mergeable-tag-sets --priority 70
   --note "Built downstream as two schemas: a membership register ordered Off < On and read as the greatest value of its frontier (an observed-remove set where an add wins over a concurrent remove), and an alias register read by following aliases, with a ring read as its least name. The work left is to make both generic over the target type and the tag type."`
3. `signature-proof-cache`: `prodrome revise signature-proof-cache --priority 75
   --note "Built downstream: a memo table keyed by the content name of each Signed object, holding the result of Signed::verifies. A program now holds enough signatures to need it."`
4. `event-header-body-split`: `prodrome revise event-header-body-split
   --priority 80 --note "An application generalised this for its own schemas: one generic event holding a key, at, actor and a body type. The skeleton header of lazy-content-by-hash is this head. TodoEvent can take the same shape."`
5. `one-literal-codec`: `prodrome revise one-literal-codec --priority 80
   --note "An application has a trait Literal with to_value and from_value, record and Fields helpers, and two macros, for its own payload values. It is the codec of SIMPLIFY-PLAN stage 4."`
6. `digest-objects`: `prodrome revise digest-objects --priority 85 --note "The first program that reads a log through a digest exists. Its lines and summaries are objects of its own schema, whose summaries are erasable text, so the generic schema still needs a design for where a summary's words are kept."`

New items (`prodrome add <id> --body … --detail … --priority …`):

7. `pull-protocol`, priority 65. Body: "a pull over a wire: what a replica
   lacks, cut into pieces closed under parents". Detail: "Three
   implementations of the objects outside the ancestry of a replica's heads
   exist: the wasm export since, and two in an application. Add
   Dag::lacking, a Cut (whole, or a byte bound), and a piece that answers the
   linearised objects within the bound and the count left. A law: each
   piece joined to what the caller holds is closed under parents, and the
   pieces asked one after another are together the whole."
8. `store-era`, priority `Ref(pull-protocol)`. Body: "an era for the answers of
   a store of record, so a replica learns when the store holds less".
   Detail: "Sync is a join, so a replica cannot tell a store restored from a
   checkpoint from an older answer. The store of record keeps an era: an
   answer that extends the previous one (the same geneses, every earlier
   head still held) keeps the era, and any other gets a later one. A
   replica orders what it holds by era, then by union. A law states both."
9. `frontier-digest`, priority `Ref(pull-protocol)`. Body: "name a replica's
   heads by a digest the store of record recorded". Detail: "A pull that
   lists every head in its URL is refused past a few hundred heads. The
   store of record records each frontier it answers with under its digest,
   keeping a bounded number, and a replica asks from the digest. An unknown
   digest is answered once from nothing. A law: the answer is the pull from
   those heads, and a replica that holds everything gets nothing."
10. `incremental-dag`, priority 75. Body: "make Dag::grow public, with
    removal, so a host holds a replica that grows by prints". Detail:
    "Dag::grow is pub(crate). An application copies the same Kahn walk to
    keep a down-set as prints arrive. Law 44 read incrementally: after any
    sequence of arrivals and removals the Dag equals Dag::from_prints of
    what is held."
11. `memo-exports`, priority 80. Body: "export memo::name to WebAssembly, and
    tabulate a reading by a store's heads". Detail: "A client reads a cache
    through the module, which design 6.2 names as the condition for an
    export. Add memo_name to the exports and memo::by_heads, the key of a
    reading of a store at its heads."
12. `schema-key-of-fields`, priority 80. Body: "let schema! take a key of more
    than one field". Detail: "schema! cannot name a key that is a pair, so an
    application copies part of it in a macro of its own for a membership
    keyed by target and tag."
13. `envelope-schema`, priority `Ref(one-literal-codec)`. Body: "one generic
    event for application schemas: a key, at, actor and a body". Detail:
    "Applications write the same event struct, vocabulary and Schema impl
    for every small schema. A generic event over a trait for the body, and a
    one-register product for schemas that write one register, remove those
    copies."
14. `schema-law-kit`, priority 80. Body: "export the schema laws as a test
    kit an application runs over its own schemas". Detail: "An application
    runs one law suite over each of its schemas. The suite belongs beside
    the laws it checks, behind a test-support feature."
15. `replica-package`, priority `Ref(pull-protocol)`. Body: "a TypeScript
    package for a browser replica: heads, the join, the pull loop, content
    by name". Detail: "The viewer reads a static index and keeps no replica.
    An application has a browser replica of about two thousand generic
    lines. A package beside the wasm exports, tested in node over generated
    neutral data and against frozen vectors shared with the Rust, removes
    every second copy."
16. `digest-cover`, priority 90. Body: "the largest nodes of a digest's tree
    that tile a span". Detail: "zoom opens one node. A reader that holds a
    span (a page of a log) needs the nodes that cover it. Generic over the
    Measure."
17. `spec-law-numbers`, priority 50. Body: "two laws in SPEC section 9 have the
    number 40". Detail: "SPEC.md lines 1343 and 1414 both start a law 40: the
    largest down-set, and a declared schema. Renumber one and every test
    doc comment that cites it." (Found while reading for this plan; it is a
    spec ambiguity that bites a reader who cites a law by number.)

## 4. The stages

Each stage is one Prodrome pull request and one Suzatary pull request that
moves the pin to it and deletes the local copy, so Suzatary shrinks at every
stage. A Prodrome pull request is public: its code, tests, CHANGELOG entry,
SPEC text and commit messages name no person, deployment, store of this
application ("lines", "inbox", "topics", "conversation", "secretary") or
private repository, and its tests use neutral data (the actor `ada`, a schema
of notes or reviews, generated histories). Each follows the Prodrome's
`AGENTS.md`: a SPEC §9 law with a property test for what a program can
observe, a CHANGELOG entry, `PRODROME_MAX_CASES` through `cases.rs`. The pin
bump touches the five places `app/Cargo.toml:48` names (the two crates'
`rev`, `app/Cargo.lock`, `flake.nix`'s `inputs.prodrome.url`, `flake.lock`).
Once the Prodrome moves to `fulfilium/prodrome`, the first pin bump after the
move also changes the `git` URL in those places, and the seam comment at
`app/Cargo.toml:3` names the new repository. Every stage below is a pull
request on `fulfilium/prodrome` (on `bmabsout/prodrome` until it moves); none
is on the framework repository, whose pieces move with the 2.2 split.

The order puts first what the others build on: the codec and the envelope
(the header is their head), then the pull (the skeleton, the era and the
frontier are parts of a pull), then the replica pieces, then the data type,
then the package.

| # | Prodrome PR | its laws and tests | Suzatary deletes | about |
|---|---|---|---|---|
| 1 | `literal::Literal`, `Fields`, `record`, `headed`, the two macros | round trip over neutral types (no new SPEC law: a codec is not observable) | `app/core/src/literal.rs` | −244 |
| 2 | `schema::envelope::Event<S: Says>` and `fold::sole::{Sole, One}` | laws 31, 33, 37 over a neutral one-register `Says` schema | `envelope.rs`, `sole.rs` | −175 |
| 3 | the schema law kit, behind a `laws` feature | the kit runs over the reference schemas in the crate's own tests | the suite body in `app/io/tests/store_laws.rs`, which then calls the kit per row | test code |
| 4 | `store/pull.rs`: `Dag::lacking`, `Cut`, `Piece`, `Sends`, the cut by weight; the wasm `since` calls `Dag::lacking` | law 47 (from `pieces.rs:207`, over neutral histories) | `routes::lacking`, `Cut`, `Piece`, `Sends`, `Literals`, `stored`, the pure half of `answered`; `sync::missing` | about −190 |
| 5 | `store/era.rs`: `Seen`, `Served::after` with the floor as an argument | law 48 (an extending answer keeps the era; the replica's join by era, then union, is a semilattice) | `epoch.rs`'s `Seen`, `Served` (the `Epochs` shell stays) | about −34 |
| 6 | `store/skeleton.rs`: the node, `Schema::Header` defaulting to the envelope's head, `Replica::literals(names)` with a byte bound | law 49 (from `pieces.rs:344`, `:428`, `:495`) | `constructor`, the generic half of `skeleton_node` and `Nodes`, `routes::content`'s body; `NodeHeader::Line` becomes the lines' `Header` | about −80 |
| 7 | `store/frontier.rs`: `Digest::of`, `Frontiers` (pure, bounded), the `Unknown` fallback | law 50 (from #252's `tests/frontiers.rs`) | `app/core/src/frontier.rs` (after #252 lands); the inline `memo::name(b"heads", …)` at `store/memoised.rs:19` and `signatures/trust.rs:380` | about −150 |
| 8 | `Dag::grow` public with removal; `memo::by_heads`; `memo_name` in the exports | law 44 read incrementally (a case of law 44, cited in the test's doc); frozen `memo::name` and `memo::balance` vectors in `wasm/conformance/`, which the framework repository's `memo.ts` tests read | the down-set half of `held_lines.rs` (`prints`, `lacking`, `place`, `enter`, `rebuild`, `excluded`), `store/memoised.rs`, `app/wasm/src/memo.rs` | about −160 |
| 9 | `tags/`: `Membership<T, G>` and `Alias<G>` over the envelope, and the reading with `follow` | law 51 (from `topics.rs:251`, `:279` and the four alias tests) | `topics/membership.rs`, `membership_event.rs`, `alias_event.rs`, `dispute.rs`, the generic half of `reading.rs` | about −320 |
| 10 | `schema!` with a key of more than one field | the exports' tests over a pair-keyed review schema | the `replica!` macro in `app/wasm/src/topics.rs` | about −40 |
| 11 | `sign`: the proof cache keyed by the `Signed` object's content name; `digest::cover` over any `Measure` | the cache means `Signed::verifies` (a cold table answers the same); `cover` tiles exactly the span | `signatures/verified.rs`, `Digest::cover` | about −55 |
| 12 | a TypeScript package `replica/` beside `wasm/exports`: heads, intents, pieces, the replica join, the era-ordered row join, content admission and eviction, the `Place` of #250, `app/idb.ts`; it imports nothing from the framework repository | the moved node tests, over generated neutral data; frozen vectors for a pull's pieces in `wasm/conformance/` | the generic replica files of §1.9 | about −1,000 web |

The counts are of the code each Suzatary pull request deletes and do not
subtract the few lines each adds to call the Prodrome. Rust totals about
−1,450 and web about −1,000, against today's `app/core/src/store/` (920),
`web/src/prodrome/` (3,088) and the generic files named above.

Notes on the order:

- Stage 3 can go any time after stage 2, and stages 10 and 11 depend on
  nothing after stage 2; they can ride beside any later stage.
- Stage 7 waits for #252 to land here; stage 8 for #250 (its row layout
  changes what `held_lines.rs` is given); stage 12 for #240 and #250, so
  that the package takes the last shape of `content.ts` and `pull.ts`.
- Stage 12 starts by routing `replica.ts`'s regular-expression reads
  (`parentsOf`, `NAMED_GENESIS`, `SIGNS`) through the wasm module, in a
  Suzatary pull request of its own before the Prodrome one, since that is a
  fix here regardless of the move.
- No stage changes a stored byte. Stage 6 changes the wire's header type
  only in this repository's own `wire.rs`, and the tab reads the same JSON.

## 5. The process gap

**Why this drifted.** Every brief here carries a tail of review questions
(the brief's numbered checklist, `AGENTS.md`'s rules and review questions).
Item 10 of the current tail asks for one generic piece instead of a copy,
and #253 (open) adds "move the code to a higher structure when one appears"
to `AGENTS.md` rule 7. Both ask the question inside this repository. Nothing
in the train, the merge gate or a brief asks whether a piece is generic over
a Prodrome schema and so belongs upstream. `app/Cargo.toml:4` says "nothing
flows the other way", which a reader takes as "do not depend on Suzatary
from the Prodrome" and not as "move what is generic up". So #212, #231,
#241, #245 and the topic stages each made a correct generic piece and put it
here, and three of them closed roadmap items without anyone editing the
roadmap.

**Three candidate checks.**

1. *A brief-tail item.* "Is any code in this change generic over a Prodrome
   schema (it names no type of this application)? If so, name it under
   'Upstream' in the pull request with its lines and the roadmap item it
   builds or implies, or say there is none." Cost: one sentence in every
   brief, and one line in every pull request. Weakness: it relies on the
   author's judgement, and the judgement is exactly what drifted.
2. *A review question in `AGENTS.md`.* The same question as review question
   6. Cost: one line, once. Weakness: the same, and review questions are
   read once per change at most.
3. *A size report.* `scripts/size.sh` already runs in the merge gate and
   prints production lines. Add one line: the lines of `web/src/prodrome/`,
   `app/core/src/store/` and the count of signatures generic over a Prodrome
   schema in `app/` (the measure of §0: `<E: Schema>`, `<E: Store>`,
   `<S: Says>` and their spellings), each with its change against the base.
   Cost: about ten lines of shell. Weakness: a number does not say what to
   do, and a generic piece outside those paths (`topics/`, `sole.rs`) shows
   only in the third count.

**The recommendation: 1 and 3 together, with 3 as the trigger.** The size
line is mechanical, so it is in every gate report whatever the author
thinks (CLAUDE.md's core principle: nothing load-bearing is left to
discipline). The brief-tail item says what a moved number obliges: when the
size line shows the generic count or either directory growing, the pull
request's "Upstream" line must name the pieces or say why they are not
generic. Neither is a gate that fails; an upstream move is a judgement and
a public pull request, and a failing step for it would be bypassed. Had the
size line existed on 2026-10-12, #212 would have shown `store` and the
generic count growing, and its "Upstream" line would have named
`lazy-content-by-hash`.

A second, smaller fix: when a pull request here builds what a roadmap item
asks for, its `Completes:`-style trailer has no Prodrome equivalent, so the
roadmap does not learn of it. The "Upstream" line names the roadmap item, and
the main session's train report lists those lines so that the roadmap edits
of §3.2 are written once per train instead of once a quarter.
