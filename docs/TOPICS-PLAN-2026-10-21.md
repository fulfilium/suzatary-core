# TOPICS PLAN: one stream, with topics (2026-10-21)

*Status: stages 1 to 4 are built, with provenance. Stage 5 is open. Git holds the text of the
built stages.*

## 0. The decision

The owner decided (2026-10): the secretary is ONE stream. Rounds and notes carry topics, a round
can carry several, and the page leads with a topic bar whose topics combine into a filter. The
topics on one target are a mergeable set, and a rename is an alias. The owner named them "topics"
(2026-10-24).

- A **round** is a question and its answer, named by the index of the turn that opened it.
- A **note** is the turn that `draft_note` writes.
- A **target** is a round or a note: `round:<conversation>/<turn>` or `note:<conversation>/<turn>`.
- A **topic** is one to three words of lowercase letters and digits joined by single hyphens, at
  most 24 characters, in NFC. `Topic::new` is its one constructor.

## 1. The stores

Two box-local stores, vaulted and never pushed, because a topic name is words (Q1, answered
2026-10-22): `secretary-topics/` (`Added` and `Removed` of a target and a topic) and
`secretary-topic-aliases/` (`Aliased` and `Unaliased` of a topic). They are the Prodrome's tag
sets (`prodrome_core::tags`, SPEC law 48). A write rests only on the earlier writes to its own
membership, so writes to different memberships commute.

## 2. The laws

- A membership reads its frontier, the greatest value under `Off < On`: an observed-remove set
  where a concurrent add wins. No clock decides.
- Every device that holds the same objects reads the same topics, in any order of arrival
  (`the_box_door_merges_as_the_union_does`, `a_second_run_writes_nothing`).
- A rename writes aliases and no membership, so it keeps every round. A rename to a topic in use
  merges the two; renaming back undoes it. Concurrent renames that disagree read by the least
  object name and are listed as a dispute until the next rename settles them.
- The filter shows exactly the rounds that carry every chosen topic
  (`a_filter_shows_exactly_the_rounds_holding_every_chosen_topic`).

## 3. Who set a topic

The owner (2026-11-01): "a topic the owner set is kept; a topic the secretary inferred is a
suggestion it may revise or drop". A topic's standing on a target is the join, over the frontier
writes of each of its names there, of (presence, provenance), presence first, with
`Inferred < Owner` (`topics::standing::Standing`):

| Join | Standing | The secretary may |
|---|---|---|
| on, by the owner | `Kept` | nothing |
| on, by the secretary only | `Suggested` | drop it, or rename it on this target |
| off, by the owner | `Declined` | nothing: it may not put it back |
| off by the secretary, or no write | `Open` | suggest it |

The secretary's writes go through the tokens that only `Suggested` and `Open` carry
(`topics::revision::plan`), so a write over the owner's choice does not type-check. The owner's
add of a suggestion keeps it. The law is `the_secretary_never_undoes_the_owners_choice`. The
secretary's actor is `assistant`, so the topics the carry and the backfill wrote read as
suggestions. A round that has read words not the owner's may only use topics in use.

## 4. Stage 5: one stream (open)

Every new round goes to `default`. `POST /api/secretary/conversations`, the conversation list,
the New topic button, the carry verb and the backfill verb are deleted with their tests, once the
owner has run the two verbs. `CLIENT_MIN` moves to 6, because an old page would offer New topic.
A tab's unsent `topics.Write` is kept and planned again against a new epoch's reading, so a write
made offline across a migration of the box's topic stores is not lost.

## 5. Questions for the owner

- **Q2. Does erasing a conversation erase its title's topic?** With box-local stores the topic
  stays on the stream's other rounds.
- **Q4.** Answered by a rule at the verb: the session opening tells every topic in use with its
  round count.
