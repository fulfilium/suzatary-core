# TOPICS PLAN: one stream, with topics (2026-10-21)

*Status: stages 1 to 4 are built. Stage 5 is a design. The owner named the
tags "topics" (2026-10-24), and every name below uses that word.*

Sources read: `CLAUDE.md`; `AGENTS.md`; `docs/ARCHITECTURE.md` §10.3;
`docs/WORDS-PLAN-2026-10-09.md` §0 and §1; `app/core/src/secretary/`
(`line_event`, `transcript_registers`, `store/stored.rs`, `taint.rs`,
`memory_event`, `memory_registers`, `fact_life`, `two_phase`,
`verbs/draft_note.rs`, `salted_text`, `texts`); the web's
`web/src/prodrome/stores.ts`, `objects.ts`, `replica.ts`, `chat/rounds.ts`,
`pages/secretary.ts`; `android/` for every `/api/` call; and the Prodrome at
the pinned `de71ccc`, `docs/design-register-types.md` §1 to §6.1 with
`core/src/fold/{order,register,product}.rs` and `store/{replica,memory_store}.rs`.
The brief says a roadmap names a mergeable topic sets item. I searched
`ROADMAP.md` and every file under `docs/` here, and the Prodrome's `README.md`,
`SPEC.md`, `CHANGELOG.md` and `docs/` at `de71ccc` and at its newest commit
`fba46ae`, for "mergeable", "topic set", "OR-set" and "observed-remove", and found
no such item. This plan builds the set from what the Prodrome already has.

## 0. The decision

The owner decided (2026-10): the secretary is ONE stream, not separate
conversations. Rounds and notes carry topics. One round can carry several
topics. The page leads with a topic bar, and the topics in it combine into a filter,
so the topics look like threads but are labels. The topics on one target are a
mergeable set: two devices that add and remove topics merge to the same set.
A rename is an alias, so renaming a topic keeps every round it was on. The
secretary gives a round its topics as it answers it, and the owner edits
topics by tap.

Terms used below:

- A **round** is a question and its answer. It is named by the index of the
  turn that opened it, as `web/src/chat/rounds.ts` keys a round and as a
  remembered fact's `Origin::Turn` names its turn.
- A **note** is the turn that `draft_note` writes (a `Turn::Tool` with that
  verb).
- A **target** is a round or a note. A **membership** is one topic on one
  target. An **alias** says that one topic name stands for another.

## 1. The stored shape

### 1.1 Where topics live

Three places were considered.

1. **The conversation lines** (`secretary-lines/`). Rejected. A line's one
   register is the transcript, a grow-only set ordered by causality. A topic can
   be taken off again, so the lines' schema would need a second register of a
   different type. Every topic write would also rest on, and be ordered against,
   every line of its conversation, although topics of two rounds never interact.
   The stream also crosses conversations, so a topic's natural key is not a
   conversation.
2. **The inbox** (`inbox/`). Rejected. The inbox holds proposals that wait for
   a tap and climb a state machine. A topic is not a proposal, and the inbox's
   state register is inflationary, so it cannot take a topic off.
3. **Stores of their own.** Chosen. The Prodrome is generic over a schema, and
   each schema here is small. A topic write rests only on the earlier writes to
   the same membership, so writes to different memberships commute.

There are two stores, not one, because a Prodrome store is opened at one
schema and the two registers hold values of different types:

| Store | Schema | Key | Register and its order |
|---|---|---|---|
| `secretary-topics/` | `MembershipEvent`: `Added(target, topic, at, actor)`, `Removed(target, topic, at, actor)` | `Membership { target, topic }` | `Presence`, total: `Off < On` |
| `secretary-topic-aliases/` | `AliasEvent`: `Aliased(topic, at, actor, to)`, `Unaliased(topic, at, actor)` | `Topic` | `Alias` (`Itself` or `As(topic)`), discrete |

One schema with a sum of the two keys would let a membership key carry an
alias value, which is a state that must not exist.

### 1.2 Box-local, vaulted, never pushed

A topic name is words: a person writes it, often from a conversation's title,
and `docs/WORDS-PLAN-2026-10-09.md` §1.1 classes a title as locked words. A
pushed store carries no words (that plan, §0). So both topic stores are box-local
and never pushed, like `secretary-lines-keys/`. The daily vault carries them,
`safe-push.sh` refuses them, and a guard test pins both facts. The phone
syncs them from the box over the tailnet, as it syncs `lines` today.

The alternative is to name each topic by a salted `TextName` and keep its words
in a text store, as the memory does. That alternative makes a topic's words
erasable. It also has two costs. A salted name has a random salt, so the same
words written on two devices give two names, and only the box can mint a name
under a lock. So a phone offline could not make a new topic. And the merge law
would hold for names, while two topics with the same words could still exist.
The owner chose the box-local stores (Q1, answered 2026-10-22).

### 1.3 One spelling for each value

- A `Topic` is one to three words of lowercase letters and digits, in any
  script, joined by single hyphens, at most 24 characters, in Unicode NFC.
  `Topic::new` is its one constructor, and it refuses every other text, so
  `Travel`, `travel ` and `travel` cannot be three topics, and a long name
  cannot be a topic. The refusal says which rule the text breaks. A topic
  is short so that the bar stays readable; keeping the topics few is the
  secretary's job (stage 3), not a rule of the type.
- A `Target` prints and parses as `round:<conversation>/<turn>` or
  `note:<conversation>/<turn>`, with the index in decimal and no leading zero.
  The same text is the object's literal and, from stage 3, a `Reference`.
- A `Membership` is the pair, and it is the membership store's key, so a pair
  has exactly one register.
- `Alias::As(t)` is never the topic's own name: `AliasEvent::aliased` refuses
  it, and the parse goes through that constructor. "This topic is its own name"
  has one spelling, `Unaliased`.

## 2. The merge law

**A membership reads its frontier** (the Prodrome's design §2): the writes to
that pair that no later write to it descends from. The reading is the greatest
value among them, with `Off < On`. A write rests on the pair's heads in the
writer's replica, so it supersedes exactly the writes its writer saw.

- An add, then a removal on a device that saw the add: the frontier is the
  removal. The topic is off.
- A removal on one device and an add on another that the removal did not see:
  the frontier holds both, and the greatest is `On`. The topic is on.

This is the observed-remove set (the OR-set of Shapiro and others), with "add
wins" for concurrent writes. The Prodrome's causal deps are the set of
observed adds, so the store needs no unique topics of its own and no tombstones.

Why this law and no other:

- **Last writer wins by a clock** is refused by the constitution: no clock
  decides who wins (CLAUDE.md, "Order is causal, time is data").
- **A two-phase set** cannot put a topic back on once it is taken off.
- **Remove wins** loses a topic that a device added without seeing the removal,
  so an offline phone's tap would vanish without a sign.
- **A state-based set per target** (each write carries the whole set) makes two
  concurrent edits of one target a conflict that a person must settle. The
  OR-set settles it by the rule above.

**Every device computes the same reading, offline included.** The reading is
a function of the two object sets (`Reading::of`), and a merge is the union of
object sets (`prodrome_core::store::sync`). Union is commutative, associative
and idempotent, so devices that hold the same objects read the same topics,
whatever order the objects arrived in. A phone writes in its own replica with
the same planner the box uses (stage 2 compiles it to the wasm module) and
syncs later.

The membership register is not inflationary, because a removal goes down. No
append is refused, and the store needs no lock: every topic write is monotone in
the CALM sense (design §6), so no write needs a decision.

The tests in `app/core/tests/topics.rs` state each law over real in-memory
replicas: `merging_is_commutative_associative_and_idempotent`,
`an_add_and_a_remove_apart_merge_the_same_way_in_either_order` (against the
OR-set's model of each pair), and `a_second_run_writes_nothing`.

## 3. Aliases

A rename writes aliases and no membership. A membership keeps the name its
writer wrote, and the reading follows each name's alias to the topic it stands
for. So a rename keeps every round the topic was on, and a device that added
the topic under the old name while another device renamed it lands in the renamed topic
after the merge (`a_topic_under_the_old_name_lands_in_the_renamed_topic`).

**The rule.** `Reading::rename(from, to)` makes every name of `from`'s topic,
and every name of the topic `to` stood for, an alias of `to`, and makes `to` its
own name. So:

- a rename to an unused name renames;
- a rename to a name of another topic merges the two topics under that name;
- a rename of `b` back to `a`, after `a` was renamed to `b`, undoes it;
- the same rename again writes nothing.

**Taking a topic off** writes `Removed` for every name of the topic that the
writer sees on the target, so a membership written under an old name comes
off too (`a_second_run_writes_nothing`).

**Disputes.** Concurrent renames can disagree in two ways. The reading always
answers one topic for each name, by a rule every device applies alike, and lists
the dispute so the page can show it:

- *Two aliases of one topic* (`Dispute::Aliases`): device A renames `travel` to
  `trips` while device B renames it to `work`. The topic's register holds both,
  and the reading follows the alias whose write has the least object name.
  This is the house rule for a reading that can show only one record.
- *A ring* (`Dispute::Ring`): A merges `travel` into `trips` while B merges
  `trips` into `travel`. Every name of the ring reads as the least name of the
  ring.

The next rename of the topic settles either dispute, because each of its writes
supersedes what its writer held for that name
(`two_renames_into_each_other_read_as_the_least_name_until_one_settles_them`,
`two_renames_of_one_topic_apart_are_a_dispute_every_device_reads_alike`,
`a_rename_keeps_every_membership`).

## 4. From conversations to one stream

**The stream is a reading, and no line is rewritten.** The lines stay in the
conversations they were written in, with their words locked under their keys.
The WORDS plan keeps every pushed object immutable (§0), and a carried copy of
every line would need new keys for every word. The page reads the stream as
every live round of every conversation, ordered by the instant of the turn
that opened it. From stage 5, every new round is asked in the `default`
conversation, so the stream grows in one place.

**The carry verb.** `suzatary secretary topics carry --map <file>` gives the
rounds and notes of each conversation that a hand-written map names the topics
the map chooses for it. It has the project's usual shape for a one-time
migration: an explicit verb, run once on the box with the owner's yes, then
deleted with its tests. It writes only into the new membership store, and it
changes no stored format, so no old reader or old store needs deleting.

- The map is a TOML file with one table per conversation id, and each table
  names 1 to 3 topics:

  ```toml
  ["2026-10-20-1"]
  topics = ["trip", "coast"]

  [default]
  topics = ["daily"]
  ```

- A conversation that the map does not name gets no topic. No topic is
  derived from a title: the owner chooses a short list of topics by hand.
- `Map::parse` refuses the whole map, before anything is written, when one
  table names an id that is not a conversation, an invalid topic, a topic
  twice, or no topic or more than three. The refusal lists every problem.
- It writes `Added(round:<conversation>/<turn>, topic)` for each live round
  (`Stored::live`) and `Added(note:<conversation>/<turn>, topic)` for each
  `draft_note` turn, for each topic of its conversation, as the actor
  `assistant`, through `Reading::add`. A membership that the store already
  reads as on plans no write, so a second run writes nothing.
- `--dry-run` without `--map` lists every conversation with its id, date,
  round count and title, so that a person can write the map. `--dry-run` with
  `--map` prints, for each conversation, its topics and how many memberships
  it would write, and writes nothing.

**Run order.** Deploy the rename. Run the listing, and let the owner write the
map. Show the owner the dry run with the map. With his yes, run the verb once
on the box and check that a second run writes nothing. The PR of stage 5
deletes the verb.

**The tests** prove it on a fixture instance under the test's scratch
directory with four conversations: the listing and a refused map writing
nothing (`the_carry_lists_the_conversations_and_refuses_a_wrong_map_whole`),
and the dry run's text, the writes and the second run writing nothing
(`the_carry_gives_the_conversations_a_map_names_their_topics_once`).

## 5. The secretary's verb

The `Secretary` record gains the membership store's writer, and the verb list
gains `add_topics`:

- `add_topics { topics: [text] }` puts at most three topics on the round the
  secretary is answering. The target is the current round, derived by the box
  and never an argument, so the model cannot give another round a topic. Each
  text goes through `Topic::new` at the edge, and a refusal names the rule the
  text breaks, so the model learns the one spelling.
- The verb takes nothing off and renames nothing: removal and rename are the
  owner's, by tap.
- The session opening tells the model the topics in use, so it reuses a name and
  does not make a near twin. A topic is at most three words and 24 characters of letters,
  digits and hyphens, but it is still text that a round that read mail can choose.
  So the opening tells only the topics that carry a membership by the owner or
  by a round with no taint (`secretary::taint`), and tells the others by count.
  **Question Q4** asks whether that rule is right.
- `SECRETARY.md` gains a short section on when to add a topic, and on
  keeping the topics few. The verb snapshot
  (`app/core/tests/secretary-surface.json`) moves with it.

## 6. The page

- **The topic bar** stands above the stream: the topics in use, the most recent
  first. Tapping a topic adds it to the filter or takes it out. The filter holds
  one or more topics and reads as "every topic of the filter": a round shows when it
  carries all of them. With no topic chosen, the page shows the whole stream.
- The filter is a register of the route (`#/secretary?topics=a,b`), so a deep
  link prints it and Back moves it, as the Prodrome's design §6.0 says of
  navigation.
- **Each round shows its topics** as chips. A tap on a chip takes the topic off;
  an add control puts one on, offering the topics in use first. A rename is a
  sheet on a topic in the bar, and it shows the merge when the new name is a
  topic already.
- **Asking with a filter** topics the new round with the filter's topics, as the
  owner's writes, so a question asked inside a topic stays in it.
- **A dispute** shows a small mark on the topic in the bar, with the names in
  dispute and a Rename that settles it.
- The page folds the two stores with the wasm module's `Reading`, so it
  computes the filter offline and writes a topic offline. Every law lands in
  `docs/WEB-SPEC.md` with the node test that pins it, over `web/test/gen.ts`,
  and the Rust writes vectors under `app/core/tests/web/` that the page's
  filter must agree with. The page is written in `docs/TS-STYLE.md`'s style.

## 7. Stages

Each stage is one pull request, a series of commits, each commit buildable
and tested. Android needs nothing in any stage: the APK calls only
`/api/author`, `/api/act/device` and `/api/fold`, and no stage changes them.

1. **The core** (this pull request). `secretary::topics`: `Topic`, `Target`,
   `Membership` and `Presence`, the two schemas and their registers, `Dispute`,
   and `Reading` with its planners. The laws are proptests over in-memory
   replicas. No store on disk, no route and no page. No stored format changes,
   so this stage has no migration verb.
   *A phone on its first load after stage 1:* nothing changes. The box serves
   the same routes, the same bundle and the same contract (`CLIENT_MIN` stays
   5), and the phone's replicas (the chains, `inbox`, `lines`) read as before.
2. **The stores.** `[secretary] topics` in `instance.toml` names the two
   directories; `suzatary init` begins them; the box opens them; the vault
   carries them; `safe-push.sh` refuses them, with a guard test. The tab gets
   two more store names in `web/src/prodrome/stores.ts`, served by
   `/api/stores/{topics,topic-aliases}/objects` and `…/events`, and a write route
   that takes a replica's objects under the same gate as `POST /api/objects`
   (a named tailnet peer in `web.act_as`, the CSRF triple, a trusted device
   key, the actor the peer maps to). The wasm module gains the two schemas and
   `Reading`. The carry verb ships here, and the owner runs it (§4).
   *First load:* the box offers the new bundle (`newbuild.ts`). Before Reload,
   the old bundle never asks for the new stores, and its routes are unchanged,
   so it works as before. After Reload, the tab pulls the two stores into its
   database the first time a page reads the conversations. The database
   stays at version 4: each store is a row under its own key in the one object
   store, so a new store is a new key, every version 4 row is read as it is,
   and a tab of the old bundle still open beside it is not blocked by an
   upgrade. The page draws nothing new.
   *As built:* the tab only pulls the topic stores in this stage; its write path
   comes with the page (stage 4). Before it writes, `locked.join`'s rule that a
   later epoch of the box drops what a row holds must keep the tab's unsent
   topics, which no other store's row has. The per-store event streams became
   one, `GET /api/stores/events`: two more streams put a tab at six
   long-lived connections, Chrome's limit per host over HTTP/1.1, and every
   later fetch waited for good. The write route takes a replica's changes
   under the act guard over a box route (a named peer `web.act_as` maps, the
   CSRF triple, a request signed by a device key every chain trusts for that
   actor), and each object must be a `Change` whose event is that actor's
   (`sync::receive_changes`). The browser module's topic replicas are classes of
   their own, because the Prodrome's `schema!` exports read registers by a
   key that is one string, and a membership's key is a pair.
3. **The secretary adds topics.** The `add_topics` verb, the session opening's list of
   topics, `SECRETARY.md`, and `Target` in the `Reference` grammar.
   *First load:* nothing new is drawn. The replica of `secretary-topics/`
   grows as the secretary answers.
   *As built:* Q4 is answered by a rule at the verb instead of a filter on the
   opening: a round whose model holds words that are not the owner's
   (`taint::tainted`) may only use a topic in use, so every topic is made by
   the owner, by a carry or backfill he ran, or by a round of his words alone,
   and the opening tells every topic in use with its round count. `add_topics`
   also takes `on: "note"` for the note the round drafted last, refuses a
   fourth topic on a target and a near twin of a topic in use
   (`Topic::twins`, the same words in any order with a plural `-s`, `-es` or
   `-ies` dropped), and counts as a read for a rewind, since a membership on a
   round set aside is on no live round. Its target is the first owner-stamped
   person's or act's turn of `queue::delta`, so a message sent before the
   answer takes nothing. A one-time backfill gives the rounds no topic is on
   their topics (`topics::backfill`): batches of the owner's words alone,
   through `ForModel`, to the compactor's model under its reserve; a new topic
   stands only on 3 answered rounds or more and twins none made before it.
   `--dry-run --plan <file>` writes the plan the owner reads, and `--apply
   <file>` writes exactly it, asking no model (`topics::plan::Plan`). Like the
   carry, it is deleted in stage 5.
4. **The page.** The topic bar, the filter, the chips, the rename sheet and the
   stream view of every conversation's rounds. The New topic button stays for this stage,
   so nothing an old bundle sends is refused.
   *First load:* the box offers the new bundle. After Reload, `#/secretary`
   opens on the stream with the topic bar, every round with the topics the carry
   gave it. An old bundle still lists conversations, because its
   routes are unchanged.
   *As built:* the core reads the stream (`secretary::stream`, the live rounds
   of every conversation a filter shows, ordered by the instant of each
   round's opening turn) and the bar (`Reading::bar`, `in_use` with each
   topic's disputed names) from the tab's replicas, through the wasm module's
   `TopicReading`, which the tab reads once per change of either topic store.
   The page draws the newest 30 rounds and asks the core for 30 more on
   Earlier. The filter is the core's `Reading::shows`, and its law is
   `a_filter_shows_exactly_the_rounds_holding_every_chosen_topic`. The bar
   orders topics by round count, as `in_use` does, not by recency. A write is
   an ask (`asks.Topic`): the tab plans it with the same reading, appends it
   to its replica, keeps the row on the device, and posts it to
   `POST /api/stores/<store>/objects`; a post that gets no answer stays unsent
   and goes before the next pull. Asking with a filter is not built: the
   stream has no composer, and a round is asked in its conversation, which
   each round links to. `locked.join` is unchanged: when the box answers in a
   later epoch while the tab holds an unsent topic write, the reset that epoch
   asks for is refused (`holdRow` never drops an unsent object it was not
   shown), so the write is kept and that topic store stops pulling for good:
   the new epoch lacks the objects the write rests on, so the box refuses it and
   it is never taken. A tab posts each write at once, so this needs a write made
   offline across a migration of the box's topic stores. Stage 5 fixes it by
   keeping each unsent `topics.Write` and planning it again against the new
   epoch's reading, never by sending the old objects again.
   The reading, the stream and the write planner run in the core's worker,
   beside the lines (`questions` `HoldTopics`, `Stream`, `PlanTopic`), so there
   is one `TopicReading`, and a topic write moves no line: the worker's memoised
   lines reading is a cache hit. An object the box refused leaves the reading
   (`replica.standing`).
5. **One stream.** Every new round goes to `default`; `POST
   /api/secretary/conversations` and the conversation list are deleted, with
   their tests; the carry verb is deleted. `CLIENT_MIN` moves to 6, because an
   old page would offer New topic, which the box no longer has.
   *First load:* an old bundle's next request gets 426, and the page says
   "Suzatary <version> is ready" with Reload; the act it tried was not sent.
   After Reload, the page is the stream.

## 8. Questions for the owner

- **Q1. Box-local topic stores, or salted names?** Answered 2026-10-22:
  box-local and vaulted, never pushed. Topics merge by their words, a phone
  makes a topic offline, and a topic's words cannot be erased.
- **Q2. Does erasing a conversation erase its title's topic?** With box-local
  stores the topic stays on the stream's other rounds. A topic carried from a
  title is a copy of the title's words, as the WORDS plan §4.2 counts copies.
- **Q3. Should the `default` conversation's rounds get a topic in the carry?**
  Answered 2026-10-24: the owner writes the map, so the map decides.
- **Q4. Which topics does the session opening tell the model?** The plan says
  the topics with a membership by the owner or by an untainted round.
