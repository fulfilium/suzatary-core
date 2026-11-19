# MEMORYDIGEST: OptMem/OptChat onto the secretary's memory (2026-10-05, answers 2026-10-06)

*Status: OPEN for roadmap `memorydigest`, stage 7 only (§10): the owner's
decision on a fresh call per turn, taken from measurements (§12, answer 2).
Stages 1 to 6 have landed; the digest's rules live in the decisions `digests`,
`compactasking`, `secretarymemory` and `agentreadsmemory`, and in the module
docs of `app/digest/src/secretary/{digest,told}.rs` and
`app/memory/src/secretary/two_phase.rs`. Section numbers are kept because other
plans cite them.*

Source: github.com/VictorTaelin/OptMem at 1fb164c and the OptChat spec,
gist.github.com/VictorTaelin/91837951a5ce5b38f341ec1ba1df6449 (2026-10-04).

## 1. What the post proposes

**Data model.** One append-only log of every message, never edited. Over it
a tree of one-line summaries (≤512 B a node), each internal node merging its
children. The tree is "a cache in principle", stored because rebuilding costs
model calls.

**Retrieval.** Each user message starts a FRESH model call: `[system][view]
[new message]` plus a `zoom` tool that opens a line into its children. The
view tiles the whole log under a byte budget (128 KB), recent lines fine, old
ones coarse, maintained incrementally: append the new leaf, then while over
budget replace the most due run of siblings by its built parent; never split.

**The cache argument.** A level-l line changes about once every 2^l messages,
so the view's prefix survives between turns and a fresh call per turn reads
most of its context from the provider's prompt cache. The post's measurement:
consecutive views shared 73k of ~131k chars at 20k messages and 92k at 400k.
There is no recall-accuracy evaluation of any kind.

## 5. Retrieval inside the context budget (landed in stage 5)

A session opening is told `told::context_of(memory, digest, budget)`: every
live fact whole, standing first (facts do not decay with age), then the topics
in use, then the clean view under the budget, the owner's own words as said
and every other part by its handle and kinds alone; `zoom(handle)` opens a
part with that part's taint. `context_of` is a function of the objects and
the budget: two replicas holding the same objects send the model
byte-identical context (law 12). Today a conversation is asked as
`conversation::ask::Ask::Session` (a Claude Code session resumed, told only
what changed through `MemorySeen` lines), falling back to `Ask::Window` (the
same context and the conversation's last 60 turns, `MODEL_WINDOW`).

## 9. What not to copy

7. Vendor tuning (cache marks at 50/80/100k, 5-minute entries) until we
   actually send fresh calls per turn (stage 7).

## 10. Stages (each one PR)

1. Stage 1 (Prodrome `digest`: `Measure`, `tree`, `pending`, `view`, `zoom`,
   laws 1-7): landed in 1.11.0, through the Prodrome pin.
2. Stage 2 (salted text store, generalised): landed in 1.11.0.
3. Stage 3 and 3b (digest schema and reading; `Erased`): landed in 1.11.0.
4. Stage 4 (compactor): landed in 1.11.0.
5. Stage 5 (retrieval: `context_of`, `zoom`): landed in 1.11.0; a summary
   records how it was asked landed in 2.1.23.
6. Stage 6 (fact consolidation and retraction): landed in 2.1.36, completed
   in 2.1.80 (a `consolidate` from a tainted round files a card; the
   secretary proposes a retraction as `Act::Retract`).
7. **(Owner-gated) A fresh call per turn**: `Ask::Digest` replaces
   `Ask::Session` for conversations; a migration verb drops `MemorySeen` and
   `Session` lines, then deletes `memory::Change`, `queue::heading`,
   `Stored::memory_seen`, the resume path and its tests. Roughly +400/−900.
   Here, and only here, the post's caching layout applies (§9, item 7). It
   also decides whether `MODEL_WINDOW`'s 60-turn window stays: a stateless
   model still needs this conversation's own last turns after the record,
   which leaves them out. Law 12 is what makes the per-turn prefix cacheable.

## 12. The owner's answers (2026-10-06)

2. Stage 7 (a fresh call per turn instead of session resume): decided after
   stages 1-6 are live, from measurements of the view against the resumed
   session (cache share, cost per turn, recall on the owner's real questions).
8. Budgets: NODE_BYTES 512, a view of 128 KB.
