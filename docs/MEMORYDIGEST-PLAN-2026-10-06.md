# MEMORYDIGEST: OptMem/OptChat onto the secretary's memory (2026-10-05, answers 2026-10-06)

Read-only research. Sources read whole: the tweet (fxtwitter JSON, 2026-10-05,
x.com/VictorTaelin/status/2107160728151130295), its quoted tweet (2026-07-26),
github.com/VictorTaelin/OptMem at 1fb164c (README, `memo`, the prompt; test.py
skimmed), and the "prompt below", the gist
gist.github.com/VictorTaelin/91837951a5ce5b38f341ec1ba1df6449 (`optchat.md`,
2026-10-04, the full OptChat spec). Ours: CLAUDE.md, SECRETARY.md "What you
remember", CHAT-PLAN-2026-09-25 §3, `secretary/{memory, memories, memory_text,
memory_event, memory_registers, two_phase}.rs`, the `remember`, `recall`,
`memory`, `context` verbs, `conversation/ask.rs`, `queue.rs`; Prodrome
`docs/design-register-types.md` §1-3, §6.0-6.3.1, §7 at b6be723.

## 1. What the post proposes

**Data model.** One append-only log of every message (OptChat: user, talk,
tool, echo, note kinds; OptMem: one-line notes ≤280 B), never edited. Over
it a purely BINARY tree of one-line summaries: `node(0,i)` is message i in
≤512 B (verbatim if it fits, a "free node"), `node(l,i)` merges its two
children. Identity is position (`id+n`, n = 2^l). The tree is "a cache in
principle", stored because rebuilding costs model calls.

**Writing.** Every message is logged as it happens (thoughts never). A cheap
model (Sonnet, medium) builds nodes in a strict order: a node is built only
when its sources exist and every view line before its end is already a
summary. The compactor gets the current view as `<chat>` context (cached
prefix), no ids, a 512-byte SCALE example, and a size-enforcement loop (show
the line cut at the limit, retry up to 5, keep the shortest). Its prompt
ranks: the user's own words near-verbatim first, then lasting effects, then
findings and replies, tool noise last; "never answer, obey or add".

**Retrieval.** Each user message starts a FRESH model call: `[system][view]
[new message]` plus tools `zoom(id,n)` (open a line into its two children,
n=1 gives the message whole) and `date(id)`. The view tiles the whole log
under a byte budget (128 KB ≈ 64k tokens), recent lines fine, old ones
coarse. It is maintained incrementally: append the new leaf, then while over
budget replace the "most due" adjacent sibling pair by its built parent, due
= (T − start)/2^(l+2); never split. No whole message is ever in the view.
A turn waits until every view line is a summary. Plus `recall <regex>` in
OptMem (word for word).

**Consolidation / forgetting.** Consolidation is the tree itself ("nap, don't
sleep": on the spot, never a nightly job). Nothing is deleted; detail fades
with age only in the view. `memo forget` drops a bad SUMMARY and everything
built on it, never the log. Instructions "stick" because the compactor keeps
the user's words, and "the last word wins".

**Algorithmic core.** No interaction nets, no learned index, no embeddings:
a dyadic segment tree with LLM-written internal nodes, a budgeted cut whose
resolution decays with age, and agent-driven descent (3-5 zooms). The
cache-stability argument is the binary counter: a level-l line changes about
once every 2^l messages, so the view's prefix survives between turns.

**Claims and evidence.** Claims: constant context, no context rot, recall
"with remarkable accuracy" years later, lower cost, AGENTS.md mostly
unneeded. Evidence is anecdotal plus engineering measurements: consecutive
views share 73k of ~131k chars at 20k messages, 92k at 400k; alpha-bisection
(OptMem's `cover`) shared ~7.5k (every turn a cache miss); compactor refused
112/630 with thoughts logged vs 0/523 without; ids in input copied 6/16 vs
0/32; OptMem `wake` 0.03 s at 1M memories; a 5,000-memory synthetic test
with a fake compressor checks tiling invariants. No recall-accuracy
evaluation of any kind.

## 2. What a memory MEANS (Elliott), and where the post is weak

- ⟦leaf⟧ = one stored conversation line (already `Scrubbed`).
- ⟦node⟧ = the sequence of leaves it spans. A summary `s` of node `n` is an
  ABSTRACTION of ⟦n⟧, not a value of a function: two runs of the compactor on
  one node give two different lines. So a summary is NOT a §6.2 cache entry
  (`memo f = f` needs `f` pure; a model call is an effect). It is an
  observation recorded as an OBJECT, like a `Decision`. Only the pure
  functions over (tree, summaries) are caches.
- ⟦view⟧ = concat of ⟦part⟧ = the whole history (tiling law), at a resolution
  per part. `zoom` moves up the information order toward the leaves; the tree
  is lossless, only the view is lossy.
- ⟦memory⟧ (facts) = the live set of the two-phase history, each with its
  provenance. A fact is authority-free background with an owner-confirmed
  origin; a summary is never a fact and never becomes one without `remember`.

The post's weak points, in these terms: identity by position is not a
function of the set (an earlier-dated line arriving late renames everything
to its right, O(T) model calls); the incremental view passes over unbuilt
parents, so it depends on build timing, not only on the log; "last word
wins" is succession by position inside lossy prose, i.e. by clock.

## 3. The mapping

| Post | Ours | Kind |
|---|---|---|
| append-only log | the conversation lines (`ConversationEvent`), already a prodrome | SCHEMA (exists) |
| summary node | `DigestEvent::Summarized` keyed by the node's content name, one discrete register per node | SCHEMA (new store `secretary-digests/`) |
| summary words | salted text in a box-local erasable store, named by `TextName` | TEXT STORE (as `memory_text`) |
| the tree shape | `memo::balance` over the lines in one causal order: a prolly tree, chunk boundaries from line names | READING |
| `pending` (what to compress next) | nodes whose children read summarized and that do not | READING |
| the view (budgeted cut) | `digest::view(tree, lines, budget)` | READING, memoised by `memo::fold` = CACHE |
| node label (taint, kinds, bytes) | a measure monoid folded up the tree | READING / CACHE |
| compactor call | `Compact` capability, host rule "pending non-empty ⇒ call, append outcome" | EFFECT |
| zoom / recall | verbs answering readings | READING (verb) |
| `memo forget <node>` (bad summary) | `Retracted { node }` event (the owner taps; the compactor rebuilds) | SCHEMA |
| facts / AGENTS.md "sticks" | the existing fact memory, unchanged in kind | SCHEMA (exists) |

## 4. Types (Kmett: the algebra first)

In the PUBLIC Prodrome, `prodrome_core::digest` (generic, no private term;
scan before merge as always):

```rust
/// What every node carries up the tree: a commutative monoid, so a node's
/// measure is the fold of its children's and memoCata computes it.
pub trait Measure: Clone + Eq { fn empty() -> Self; fn join(&self, o: &Self) -> Self; }
pub struct Bytes(pub u64);                           // the line's rendered size
pub struct Leaf<M> { pub name: Hash, pub line_bytes: u32, pub measure: M }
pub struct Node<M> { pub name: Hash, pub span: Range<usize>, pub children: Vec<NodeRef>, pub measure: M }
pub struct Tree<M>;                                  // memo::balance over leaves, `Node::name` = memo::name
pub fn tree<M: Measure>(leaves: Vec<Leaf<M>>) -> Option<Tree<M>>;
/// Nodes buildable now, smallest-first then leftmost: every child a leaf or summarized,
/// and every part of the view before its end summarized (OptChat's rule 3).
pub fn pending<M>(t: &Tree<M>, summarized: impl Fn(&Hash) -> Option<u32>) -> Vec<Hash>;
pub struct Part { pub node: Hash, pub span: Range<usize> }
pub struct View { pub parts: Vec<Part>, pub bytes: u64, pub waiting: Vec<Hash> }
/// The scan: for each leaf in order, append it, then while over `budget` replace the
/// full run of siblings with the greatest due = age/size by their summarized parent.
pub fn view<M>(t: &Tree<M>, summarized: impl Fn(&Hash) -> Option<u32>, budget: u64) -> View;
pub fn zoom<M>(t: &Tree<M>, node: &Hash) -> Option<Vec<Part>>;
```

Why `memo::balance` and not the dyadic tree: its chunk boundaries are a
function of the line names (one in four, or at sixteen), so the shape is a
function of the set, a late arrival renames one logarithmic spine, and two
replicas build one tree. A completed chunk is closed by its own last element,
so every node is prefix-determined and the scan over leaves is well defined.
Fan-out 2..16 means the compactor merges k lines, not two; OptMem's first
version did exactly that.

In Suzatary:

```rust
// secretary/digest_event.rs: keyed by the node's content name, one register.
pub enum Said {
    Summarized { text: TextName, over: Vec<Hash>, model: String, bytes: u32 },
    Retracted,                                   // the owner's tap: this line is wrong
}
pub struct DigestEvent { node: Hash, at: Datetime, actor: Actor, said: Said }
// secretary/digest.rs
pub struct Label { pub taint: Taint, pub kinds: KindSet, pub bytes: Bytes }  // Measure
pub struct Digest { tree: Tree<Label>, lines: BTreeMap<Hash, Summary> }      // the reading
pub fn digest(convs: &dyn Prodrome<ConversationEvent>, hist: &dyn Prodrome<DigestEvent>,
              words: impl Fn(&TextName) -> Option<Line>) -> Digest;
pub struct Line(String);  // smart constructor: one line, ≤ NODE_BYTES, no URL, via redact::Scrubbed
pub const NODE_BYTES: usize = 512;
pub struct ContextBudget { pub facts: usize /* MAX_LIVE*MAX_FACT_CHARS */, pub view: usize }
```

`Taint` is the existing `secretary::taint::Taint` read off each line by
`taint::turn`, made a join-semilattice (`Clean < Tainted`), so a node's taint
is the join of its leaves': computed by the reading, never by the model.

The node key names its LIVE inputs: `memo::name` over the leaf names that
read live. Nothing erases a conversation line today, so today that is every
leaf; the day one can be erased, the spine above it renames, the old
summaries' keys stop being named by the live tree, and the existing sweep
rule ("delete every text something names and nothing live names",
`memories::dead`) deletes their words. Forgetting never needs a special
case.

## 5. Retrieval inside the context budget

Today: a new session is told every live fact (≤200 × 280 chars ≈ 56 KB),
then deltas via `MemorySeen`; earlier conversations reach the model only
through `recall` (substring, 280-char excerpts) or a Claude Code session's
own lossy compaction; `Ask::Window` sends the last 60 turns.

Proposed, as one reading `context_of(memory, digest, budget) -> Told`:

1. Facts first, whole, ordered by kind then id (standing, preference, person,
   owner). They change rarely, so they are the stable prefix. Facts do NOT
   decay with age: a standing instruction from two years ago binds as much as
   yesterday's. This is the one place the post's design is wrong for us.
2. Then the CLEAN view: the cut over the tree restricted to clean spans,
   under `budget.view`. A tainted part renders as its handle and kinds only
   ("`c/4120+16` 16 lines: answers, mail reads; zoom to read"), never its
   words, so the always-on context stays clean and "remember FIRST" keeps
   working.
3. `zoom(handle)` answers a node's children (a leaf whole). Its taint class
   in `taint.rs`'s total match is the node's label: zooming into a clean node
   is `Clean`, into a tainted one is `Tainted`, so a later `remember` in that
   round becomes a card, exactly as after `recall` today.
4. `recall` stays (word for word, as OptMem keeps `recall`).

Handles shown to the model are positional (`c/<first index>+<leaves>`),
computed by the reading for the current tree; identity stays the content
name, and `zoom` resolves a handle against the reading it was printed from.
Determinism: everything above is a function of (objects, budget); no model
call, no clock read.

## 6. Consolidation, compaction, forgetting: new objects only

- **Compaction** appends `Summarized`; the text goes to the text store first,
  then the event, the text removed if the event is refused (the
  `Held::remember` pattern). Actor `compactor`, a capability record holding
  only `&dyn Prodrome<DigestEvent>`, the text store and a `Compact` model
  with no tools: it cannot express a fact write, a chain write or anything
  outward. Its input is rendered from `Entry` lines, which hold no memory
  words (a remember card names a `TextName`), so a card's words never reach a
  summary.
- **Two summaries of one node** (two replicas, a retry) are a discrete
  register in conflict. The view takes the least object name and says so; the
  digest page shows both. Never picked by time.
- **Retraction**: the owner taps "wrong" on a line: `Retracted` supersedes it,
  the node is pending again, the parent's key is unchanged (keys name leaves,
  not summaries) but its summary was built from the bad line, so retraction
  marks every summarized ancestor's event `stale` by a reading (an ancestor
  whose `over` names a retracted summary) and the compactor redoes the spine.
- **Fact consolidation**: `Said::Consolidated { from: Vec<FactId>, text, kind }`
  in `MemoryEvent`, replacing SECRETARY.md's "forget both, remember the merged
  line". Provenance of the new fact is the union of its sources' `Origin`s
  (`Origin` becomes a set for this case). Same gate as `remember` and `forget`
  (a clean round the owner opened).
- **Forgetting** is unchanged: the history keeps that fact N existed, the
  words go. If conversations ever become erasable (open question 6), §4's key
  rule erases every summary above the erased line with no new code in the
  sweep.

## 7. Laws (each a property test; the Prodrome ones as SPEC law 45)

Prodrome `digest`, over generated sequences and summary maps:
1. **Tiling.** A view's parts partition `[0, T)` in order, no gap, no overlap.
2. **Budget.** `waiting` empty ⇒ `bytes ≤ budget` or every part is a root.
3. **Decay.** Part sizes are non-increasing toward the present.
4. **Refinement.** Zooming every part down to leaves yields exactly the leaf
   sequence: the tree is lossless.
5. **Free.** `tree`, `pending`, `view` are invariant under permutation and
   duplication of the input objects (law 33's reading).
6. **Stability.** Appending one leaf changes at most `c` parts of the view
   (the merged run and the tail), `c` stated and tested; a late insertion
   renames at most `O(log T)` nodes.
7. **Memo.** `view`, `pending` and the label fold through `memo::fold` equal
   the plain ones (law 8's instance).

Suzatary:
8. **Never in an object.** No `DigestEvent` print contains its summary's
   words; a sweep after a key is no longer live leaves no text for it.
9. **Taint is the reading's.** A node's taint is the join of its leaves';
   the clean view never prints a tainted part's words; `remember` after
   `zoom` into a tainted node is a card.
10. **Conflicts.** Two `Summarized` for one node read as a conflict; the view
    is the same whatever their arrival order.
11. **Consolidation.** `live(h + Consolidated{from, into})` =
    `live(h) − from + {into}`; `origins(into) = ⋃ origins(from)`; the sources'
    words are dead after the sweep.
12. **Determinism.** `context_of` is a function of the objects and the budget:
    two replicas holding the same objects send the model byte-identical
    context.

## 8. Invariants kept

Erasable words: summaries live in a box-local salted text store, names only
in objects. Redaction door: `Line::new` goes through `Scrubbed`, input lines
already are. The tap: nothing here acts outward; the compactor has no tools.
The triage agent gets no memory write. Since 2026-10-24 (the owner) it reads
the facts and the view in the owner's words alone, as the secretary's opening
does, in one budget with its manual, and its `recall` and `zoom` read the same
clean set (the facts, the owner's own lines, and summaries of nothing but
those, `Label::is_owners`): never the secretary's answers, a verb's answer or a
review round, which can quote mail or a batch triage once wrote, so nothing an
email put in front of one run is read back by a later one through the memory. Provenance: every fact keeps its `Origin`(s); every summary names its
node, whose leaves name conversation and turn. Conflicts: shown, least name for
a single-record reading. Replicas: everything is a prodrome or a reading of
one, so union converges (§6.1); summaries are objects, so a phone never
recomputes one.

## 9. What not to copy

1. **Facts that decay.** Standing instructions and preferences stay a flat set
   read whole; the tree is for the episodic record only.
2. **"Instructions stick" through the compactor.** Lossy, ranked by the
   compactor, settled by position: succession by clock in prose. Our facts
   are explicit, gated and forgettable.
3. **Tool output in the always-on context.** OptChat logs `echo` (mail, web)
   and every later turn reads its summary: SpAIware persistence by design.
   Ours keeps tainted spans out of the always-on view.
4. **Nothing is ever deleted.** We erase words; the history stays verifiable.
5. **Position as identity** and the build-timing-dependent incremental view.
6. **A manager with spawn and computer use** acting from memory ("buy my
   groceries"): outward acts stay proposals the owner taps.
7. Vendor tuning (cache marks at 50/80/100k, 5-minute entries) until we
   actually send fresh calls per turn (stage 6).

Worth copying: the ranking in the COMPACT prompt (owner's words first,
describe tool output, never make anything look further along than it was),
the SCALE line and cut-at-limit retry, no ids in compactor input, no thoughts
logged, "zoom before you act, guess or ask", and the stability-first view.

## 10. Stages (each one PR)

1. **Prodrome `digest`** (public): `Measure`, `tree`, `pending`, `view`,
   `zoom`; laws 1-7 as SPEC law 45. ~500 lines + ~400 tests. Deletes nothing.
2. **Salted text store, generalised**: `memory_text::Text` becomes
   `salted_text::Text<T>` for facts and lines; `memories::dead` becomes
   `texts::dead` over both histories. ~250 lines, mostly moved. Deletes the
   fact-only text type. BUILT (2026-10-06, the
   `salted-text-generalisation-b460rb` report): `Text<T: Words>`, and each
   history answers a `texts::Naming` (a map into `Past < Live`, joined
   pointwise) that the one sweep joins; the files' bytes did not move.
3. **Digest schema and reading**: `digest_event.rs`, `digest.rs`, `Label`,
   the store under `[secretary]`, the sweep extended; law 8, 10. ~450 lines. BUILT
   with 3b (2026-10-06, the `memorydigest-stage-3-st4ksm` report): the
   store is `digests/` and its words `secretary-digest-texts/`; `Label` is
   taint and kinds, the bytes being the tree's own; a naming is typed by
   its store (`Naming<T>`); `Erased` (3b) is a digest event keyed by the
   line, made only from a `Tap` and believed only from the deployment's
   writers. IT EXCLUDES THE LINE FROM THE DIGEST AND ERASES NO WORDS: the
   line's words stay in its pushed conversation object, the data
   repository's history and every tab, and no page or verb offers it.
   Answer 6's erasure (conversation words box-local and salted like facts,
   a migration, and the data repository's history) is a stage of its own.
4. **Compactor**: `Compact` capability, the adapted COMPACT prompt, the size
   loop, the host rule in the existing job loop, a fake compactor (join and
   truncate, as OptMem's test) for every test; fixtures in scratch only.
   Law 9's first half. ~500 lines. BUILT (2026-10-06, the
   `memorydigest-stage-4-compactor-na3yqb` report): `capability::Compact`,
   `compact_ask::Ask` and the prompt `secretary/compact.md`,
   `compactor::compact` (the size loop, `Unfit` when nothing fits),
   `compaction::Compaction` in the engine and `Paced` at the budget's pace;
   `Summarized` records the node's taint, every call is an `Asked`
   recorded before it is made, and the budget (`[secretary]
   compact_calls_per_hour`, `compact_bytes_per_day`, the switch `compact`,
   off by default, the model `compactor`) is a fold over those objects. The import is the same rule at
   the same pace. Since 2026-10-24 `compact_workers` steps run at once, each on a node
   `Paced::take` claimed for it alone, the budget checked under the digest
   store's lock (`workers_write_the_tree_one_worker_writes_within_one_budget`).
5. **Retrieval**: `context_of`, the clean view as the heading a new session
   gets (beside the facts), the `zoom` verb and its taint class, SECRETARY.md's
   "What you remember" rewritten; laws 9, 12. ~350 lines. KEEPS
   `MODEL_WINDOW`'s 60-turn window (review of #111: a stateless model or a
   session opening still needs this conversation's own last turns, after
   the record, which leaves them out); stage 7 decides whether it goes.
   BUILT (2026-10-07, the `memorydigest-stage-5-retrieval-uo5kgg` report):
   `told::context_of` and `told::zoom` over one rendering, `digest_view`;
   the handle is the Prodrome's `Part::handle`, `first+lines`, resolved to
   the deepest node spanning it; a zoom answer carries the node's taint,
   which `taint::turn` reads, so its class is the node's with no second
   verb; the told view keeps its newest parts when over budget, the
   compactor's log its oldest. Review round 1 narrowed and corrected it:
   the record shows the owner's own words alone (`Label::is_owners`),
   everything else by kind, runs of those merged into one line that `zoom`
   opens through `Digest::cover`; a model's answer joins what its model
   still holds (`queue::held`: what `for_model` sends and, for a kept
   session, every live turn from the first its opening sent, recorded on its
   `Kept` line; every turn for a transcript ask, recorded on a `Scoped`
   line; rounds 2 to 6), as does remember's gate; a line's `by`
   is a typed `Stamp`, an actor's or the box's notes read clean; the window
   stays after the record, which leaves its lines out; a question larger
   than a day's cap is recorded `Unfit`. Also the compactor's three follow-ups: an
   `Asked` per call at the call's instant, a call held when it would cross
   a cap by its own size, and an empty `CLAUDE_CONFIG_DIR`.
   A SUMMARY RECORDS HOW IT WAS ASKED (2026-10-25): `compact_ask::Asking`
   is a `rule_version::Versioned` rule with a version per class of node
   (`Beside`: the owner's words alone, or every clean part), the SHA-256 of
   the prompt, the question's text and what that class's log shows on a
   fixed probe of every shape of part (`digest_view::shown_as`, the one rule
   a rendering shows a part by), the sizes and a `REVISION` raised by hand
   for what the probe cannot reach; `the_asking_moves_when_the_questions_move`
   pins both versions beside a hash of a whole compaction's questions, and
   every `SummarizedUnder` records its node's class's version (a `Summarized` from before reads as
   unversioned). A summary is CURRENT when it was asked under the current
   version over what its children read now (`digest::Standing`), and
   `Digest::pending` reads a stale one as none, so the compactor asks it
   again only over current children, lowest level first, the owner's nodes
   first (`Compaction::owed`), at the budget's pace, its new summary
   appended over the old one. A stale summary of a node labelled the
   owner's alone is shown to no reader until it is redone, since it may
   have been asked beside the clean log (before #236). `suzatary secretary
   digest census` counts current, stale and unsummarized nodes by group and
   level.
6. **Fact consolidation**: `Consolidated`, the `consolidate` verb, the page's
   "merged from"; law 11. ~250 lines. Deletes the forget-both procedure.
7. **(Owner-gated) A fresh call per turn**: `Ask::Digest` replaces
   `Ask::Session` for conversations; a migration verb drops `MemorySeen` and
   `Session` lines, then deletes `memory::Change`, `queue::heading`,
   `Stored::memory_seen`, the resume path and its tests. Roughly +400/−900.
   Here, and only here, the post's caching layout applies.

## 11. Open questions for the owner

1. Always-on view: clean only (owner's turns, recommended) or the secretary's
   answers too (then every round starts tainted and every `remember` is a
   card)?
2. Stage 7: give up Claude Code session resume for a fresh call per turn?
3. One view across all conversations (recommended: one tree, lines in causal
   order, as "your life") or one per conversation?
4. Summary words: box-local and erasable (recommended) or pushed with the
   conversations?
5. Compactor model and cost: OptChat spends about two calls per line with
   ~64k tokens of cached context each. Same provider as the secretary? The
   compactor reads everything the conversations hold.
6. Should a conversation, or a line, become erasable? The key rule supports
   it at no cost to the digest.
7. Import past conversations into the tree at once (OptChat imported its
   whole history), or start from now?
8. Budgets: `NODE_BYTES` 512 and a view budget of what (OptChat 128 KB, with
   SECRETARY.md and the facts beside it)?

Review lenses: Kmett (is every label a monoid, is every merge a join, is any
law missing?), Elliott (does each reading have a meaning stated without the
model?), Pijul (is every result a function of the set of objects, and does a
late object rename only what rests on it?).

## 12. The owner's answers (2026-10-06)

1. Always-on view: clean only (the owner's turns), as recommended.
2. Stage 7 (a fresh call per turn instead of session resume): decided after
   stages 1-6 are live, from measurements of the view against the resumed
   session (cache share, cost per turn, recall on the owner's real questions).
3. One tree across all conversations, lines in causal order.
4. Summary words box-local and erasable.
5. The compactor runs on the latest Sonnet (Sonnet 5.5 today), named in
   config, never in code.
6. Erasable: yes, by the owner only. He erases a conversation or a single
   line from the page; the secretary may propose an erasure as a card he
   taps, never perform one. Built as stage 3b (after the digest schema):
   an `Erased { line }` event the owner's actor alone may write, the line's
   words dropped from the text store by the sweep, and §4's key rule
   renaming the spine above it so every summary that read it dies in the
   same sweep, with no new digest code. The history keeps that a line
   existed, never its words.
7. Import past conversations: yes. The tree is a function of the set of lines,
   so an import is the same build over lines that already exist (about 4,100
   objects today), run in the background at the compactor's pace.
8. Budgets: NODE_BYTES 512, a view of 128 KB.
