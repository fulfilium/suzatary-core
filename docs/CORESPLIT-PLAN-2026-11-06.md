# The core split, measured (2026-11-06)

The plan for the todo `coresplit`. Delete this file once the split lands.

## What was measured, and how

- **Today** (`scripts/context.sh` at `20035df`): core USE 485.1k, DEVELOP 1.50M. Core's USE is
  why io (880k), cli (1.08M), wasm (693k), pages (701k) and web (1.08M) are over DEVELOP. Every
  crate also pays the constitution (the root manuals and two docs, 60k) and prodrome-core's USE
  (109k), because every proposed crate calls `prodrome_core`.
- **The graph** comes from the `use` trees and the inline `crate::`/`super::` paths of each
  production file (before `#[cfg(test)]`). A node is a top module of `app/core/src`, or a
  submodule of the twelve largest. That gives 350 nodes over 491 files and 1.26M tokens, and
  each edge carries the items it names.
- **USE** is the same line reading as `commands/context/interface.rs`. Ported, it gives 485.1k
  for core, the binary's figure. A crate's USE is the interface of each `pub` item that some file
  outside the crate names, plus every file's `//!`, plus a 0.5k README. A name is matched as a
  word, and a method counts only where its type is also named. Name collisions make every USE
  below an upper bound.
- **DEVELOP** has two bounds. The low bound is own source + 60k + 109k + the interface of the
  dependency items the crate names. The high bound replaces the last term with the whole USE of
  each direct dependency, which is what the measure counts. Cited docs are left out (core's come
  to 34k).
- **Churn** counts the commits that touched the crate's files since 2026-11-01, the clone's
  depth (58 commits touched core).

## The cycles today

Of core's 81 top modules, 48 form one strongly connected component (1.11M tokens). Inside
`secretary`, 68 of its 118 submodules form another (384k). Every cycle has one of these causes,
and each fix is a named move:

| # | Cycle | Edges | Break |
|---|---|---|---|
| C1 | `store/kind` and `store/database` name every store's event type (secretary, mail, decisions, devices, plugin, upload) | 33 | Done in #436: `Kind`, `Tier`, `Files`, the `Store` trait, its rows and `Database` are the top module `stores`, in `api`; `Hold` stays in `store`, whose code was already generic over `Schema`. The impls stay in `api`'s one list: most schemas are aliases of the Prodrome's generics (`headed::Event<Said>`), which the orphan rule lets only the trait's crate implement. |
| C2 | `wire.rs` holds every route's types, and low modules use them (`FailedChain`, `SinceRead`, `DevicePayload`, `ErrorPayload`, `MailWatch`, `Card*`) | 50 | Split by concept: each wire type moves to the lowest crate that names it (table below). The ts-rs generator test moves to `api`. Done in #447: each type is in `wire/<cut>.rs` of the lowest cut that names it or names a type that names it (97 types, #440's `Content` among them), `wire.rs` keeps the 71 only the routes and pages name, and the generated `wire.ts` and `pages.ts` hold the same declarations in an order the types' ids decide; `payload.ts` is byte-identical. |
| C3 | `capability/*` defines every port and record over every domain's types (`mail` 27, `secretary` 22) | 49 | Each port file moves beside its domain (`corpus`, `mailbox`, `mail_sync`, `junk`, `older_mail` to mail and triage; `model`, `agent`, `secretary` to `secretary`; `server` to `api`). `capability/mod` keeps `Secret`, `Datetime`, `Login` and the tokens in `base`. Split in three by the reviewer's call. First part done in #438: `mailbox`, `mail_sync`, `corpus`, `junk` (as `mail/junk_folder.rs`) and `older_mail` are `mail/`'s, and so is `heartbeat` (from `secretary`), so `Corpus` no longer names `secretary`'s `PassOutcome`. Second part done in #441: the closed `VerbName` (each verb's name and powers, which `ToolName` holds and `SecretaryVerb` maps onto, a law keeping them one), the model port with its laws and `model_sessions` in the conversations' cut, `answers` in the topics', `compact` in the digest's, `MailError` in held's (`held_value` named it under the old re-export), and the binding `apply` beside `Gate`. Third part done in #442: the role records and their ports leave `capability`: `agent::Agent`, `secretary::Secretary` with `answering` and `briefings`, and the top module `server`. Fourth part done in #444: the last twelve ports move beside their domains, and `capability/mod.rs` re-exports only the base's effects (`Clock`, `Entropy`, `Manuals`, `RenderError`, `Typeset`, `Whois`) and the test fakes; `RenderError` stays there because `narrowing.rs`, a base module whose text is hashed into the rendering functions, names it. |
| C4 | `connect::Failure` ↔ `capability::Limited` ↔ `health`, used by mail, plugin and drive | 27 | Done in #405: the `health` module; mail fails with `mail::fault::Fault`; `Lacking` is mail's. |
| C5 | `secretary/tap::Tap` is used by `mail/oauth` (`Consent::given_by_tap`) and by digest | 3 | Done in #421: `Tap`, `Seat`, `Admitted` and the CSRF triple are `signatures`'; `performs` is `inbox::sending`'s. |
| C6 | `secretary/store` (`Entry`, `Proposal`) ↔ `acts`/`inbox`/`changes`/`responded` | 36 | Done in #430. `Proposal`, `Reasons`, `Draft` and `Forward` (with `Style`) move to `inbox` as `secretary/proposal`. The writers of an `Entry` in `acts.rs` move to `secretary/act_lines.rs`, in `lines` with the store's `append` (C7). `store::stamp` moves beside `Stamp`; `act_vocabulary.rs` (only `SIGNATURES`) is `inbox`'s; `Daily` moved to `base` in C8. `diff.rs` (`changes` names `diff::shown`) is `inbox`'s, and `agent/priced.rs` (`changes` names `Priced`) is the top module `priced`, in `todo`. |
| C7 | `secretary/mod.rs` is both the store's operations (`read`, `append`, `list`, `start`) and the turn; and `conversations` ↔ `lines` (the store names `LineEvent`, the lines name `Entry`, `Stored` and `Conversations`) | 22 | Corrected on #430 (option b). `conversations` is an entry and its fold (`store/*`, `turn_shape`, `outcome`, `queue`, `taint`, `tool_name`, and nothing that names `LineEvent`). `lines` holds the store: `conversations.rs` with the operations (`read`, `start`, `list`, `append`, `read_or_open_default`), `conversation_json.rs`, `transcript`, `line_*`, `held_*`, `daily.rs` and `act_lines.rs`; every edge points from `lines` down. `writes_as()` was `transcript::writer()`. `SecretaryVerb` stays in `secretary` (`verbs!` builds it from the verb structs); its low users (`tool_name`, `taint`, `targets`, `caused`, and `store/stored.rs` through `Effect`) need each verb's powers, not only its name, which the C7 pull request's comment proposes to settle. `Bound` and the cards' `propose` are the cards ↔ secretary cycle below. |
| C8 | Ids defined high and named low | 28 | Done in #388: each moved to the lowest crate that names it (`conversation_id`, `topic_target`, `writer`, `spelling`, `drive_path`, `repo_ids`, `coding_report`, `answer`). |
| C9 | `deployment` and `settings` name `coding::{Coding, RoutineId, ROUTINE_TOKEN}` | 4 | Done in #434: the three and `parsed!` are `connect/routine.rs`'s; `coding` (the Task card) is `inbox`'s. |
| C10 | `held_value`/`file_read` ↔ `mail` | 6 | Done in #388: `message_part`. |

With these moves applied, the graph over the cut below has **no edge that points up**, except
these, found while doing C6 and C7, each named with its proposed move in the C7 pull request:

- **cards ↔ secretary.** `secretary` names `filed.rs` (`Filing`, `file`, `TriageCards`, `Carding`,
  `triage_card`) and `afresh::Request`; `cards` names `secretary`'s `Bound`, `WRITES_AS` and
  `verbs::propose`, and `digest`'s `memories.rs` names `filed.rs`.
- **Re-exports hide edges.** `capability/mod.rs` re-exports every port, so a path such as
  `crate::capability::Turn` reads as `base` while `Turn` is `capability/model.rs`'s; low
  modules name `Turn` and `ModelSessions` this way. C3 must move what the low modules use.
- **The test stores name `stores::Store`.** `store`'s `InMemory` (`tests/support/in_memory.rs`,
  compiled under `nothing`) takes its hold from `E::HOLD`; its crate PR moves it to `api`'s test
  support or passes the hold in.

## The crates, bottom up

In the table, "use" is the crate's USE after the wire split and "deps" counts its direct
dependencies. Every figure is in thousands of tokens.

| # | Crate (`app/<name>`) | What it means | own | use | deps | DEVELOP lo / hi | churn |
|---|---|---|---|---|---|---|---|
| 1 | `base` | The names every crate speaks: ids, references, chains, zones, refusals, secrets, the markup escape, and the clock and entropy ports. | 51 | 22.0 | 0 | 220 / 220 | 5 |
| 2 | `record` | What one todo record is: its kinds, fields and markup sources. | 28 | 11.2 | 1 | 199 / 218 | 2 |
| 3 | `store` | A content-addressed, signed object store and its replica, for any schema. | 44 | 19.7 | 2 | 214 / 244 | 8 |
| 4 | `signatures` | Who may write to a chain: devices, keys, trust, epochs, and the owner's `Tap`. | 55 | 24.0 | 3 | 234 / 274 | 2 |
| 5 | `todo` | One todo as the folds read it: authoring, views, trees, decisions, releases and renders. | 88 | 28.0 | 4 | 274 / 330 | 12 |
| 6 | `held` | Values that a model sees only by placeholder: held files, attachments, redaction and the leak guard. | 63 | 22.7 | 2 | 241 / 272 | 2 |
| 7 | `memory` | Words the owner can erase, sealed per slot, and the facts the secretary keeps in them. | 37 | 16.5 | 3 | 214 / 268 | 3 |
| 8 | `mail` | One account's mail as the protocols carry it: address, IMAP, SMTP, OAuth and the message. | 48 | 27.0 | 2 | 226 / 260 | 8 |
| 9 | `health` | What went wrong with a source, and what remedy fixes it. | 17 | 9.2 | 3 | 193 / 254 | 3 |
| 10 | `triage` | What the box decided about each message: junk, older mail, watches, rules and state. | 35 | 18.5 | 3 | 223 / 271 | 3 |
| 11 | `services` | The other sources by grant: Drive, repositories, usage, web, weather and the plugin manifest. | 42 | 18.9 | 3 | 226 / 262 | 8 |
| 12 | `google` | Google's sign-in and calendar as a source. | 32 | 12.9 | 4 | 213 / 287 | 2 |
| 13 | `connect` | The deployment's declared connections and settings, their secrets and their checks. | 48 | 17.7 | 8 | 246 / 363 | 7 |
| 14 | `inbox` | The machine that only climbs: proposals, acts, taps and phases. | 80 | 28.3 | 11 | 299 / 460 | 10 |
| 15 | `conversations` | A conversation's entries and their fold: ids, queues, searches and taint. | 56 | 19.8† | 10 | 271 / 427 | 10 |
| 16 | `lines` | The conversation store: its operations, its lines as stored and replicated, shapes, transcripts and held replicas. | 81 | 28.0† | 9 | 299 / 425 | 10 |
| 17 | `topics` | The readings of a conversation that the browser shares: topics, readings and the stream. | 39 | 11.0 | 12 | 265 / 447 | 6 |
| 18 | `digest` | A conversation compacted into a digest within its budget, and the memories it recalls. | 68 | 22.9 | 9 | 294 / 412 | 5 |
| 19 | `cards` | The cards that the box files on its own: setup, failures, coding sessions and notifications. | 65 | 18.0 | 13 | 319 / 489 | 14 |
| 20 | `secretary` | One turn of the secretary or the triage agent: its verbs, surface, sweep and review, over the capability records. | 170 | 30.3 | 19 | 470 / **699** | 14 |
| 21 | `api` | The routes, the gate and the wire that the browser and the server share. | 115 | 26.1 | 18 | 446 / **652** | 20 |
| 22 | `server` | cli's `serve` (115k): the axum shell that answers the routes. | 115 | — | ~20 | ≥ 414 / — | — |

† Measured after C7 (#430's decision) at `d2615b1`, every `pub` item of the cut counted (an upper bound of the measure above); the same count gives 21.9 and 25.0 for the two rows before the C7 moves.

Churn is highest at the top (cards and secretary 14, api 20). Among crates 1–12, only todo (12)
changes that often. record, signatures, held, memory, health, triage and google (2–3 commits each) are
the first candidates for their own repositories, pinned by rev.

### The cut: each crate's modules and its USE

`s/` is `secretary/`. A crate's USE is the `pub` items of its modules that a file outside the
crate names. PR 1 narrows the rest. The last column names the main exported types.

| Crate | Modules | Main exports |
|---|---|---|
| base | wire/base, capability (mod, clock, entropy, manuals, typeset, whois), cast, chains, content_id, escape, finite, model_name, narrowing, nonempty, persona, reference, refusal, rule_version, seat, table, term_json, timings, tree, tripwire, url, words, zone; the C8 ids | `Refusal`, `Reference`, `ChainName`, `Zone`, `Secret`, `Datetime`, `Clock`, `Entropy`, `Typeset` |
| record | wire/record, decoration, record, todo | `TodoEvent`, `Record`, `MarkupSource` |
| store | wire/store, replica/\*, snapshot, store/\* (with `Hold`) | `Prodrome`, `Reads`, `Log`, `Doc`, `StoreError`, `Objects`, `Frontier` |
| signatures | wire/signatures, believed, chain_set, devices, epoch, signatures/\*, sync; `Tap` | `Trust`, `Enrolment`, `ChainSet`, `Tap` |
| todo | wire/todo, authoring, render, render_cache, decisions/\*, priced, reading, release, rendering, template, todos, verb, view | `Authored`, `Fulfillment`, `Decisions`, `Released` |
| held | attachment, pdf_text, upload_index, enclosed, mail_error, excerpt, file_read, for_model, held_file, held_value, leak, redact, s/{attached, attachment_source, upload_standing}, upload, vault | `HeldFile`, `AttachmentId`, `Redacted`, `Enclosure` |
| memory | s/{carried, erasure, fact_life, key_ring, line_words, locked_text, memory, memory_event, opened, provenance, salted_text, scrubbed_under, slots, texts, two_phase, words} | `Words`, `Fact`, `KeyName`, `Erasure` |
| mail | mail, mail/{mail_sync, mailbox, account, account_email, address, cache, declared, dedup_key, folders, grants, hash_prefix, idle, imap, mailroom, oauth, outbox, puller, removal, send, sync_now, transport, verbs} | `MailMessage`, `MailAccount`, `EmailAddress`, `Consent`, `mail::Fault` |
| health | capability/limited, connect/{failure, label, lacking, refused}, health, plugin/{credential, name, origin, refusal, said}, source_key | `Failure`, `Remedy`, `Healths`, `SourceKey`, `PluginName` |
| triage | wire/triage, mail/{corpus, junk_folder, older_mail, heartbeat, gone, junk, known, local_state, older, state, triage, watch}, rule | `Corpus`, `TriageState`, `Rule` |
| services | repositories, repository, drive (with `effect`), plugin, plugin/{bundle_path, fetch, item, manifest, run, url}, repos, usage, weather (with `effect`), web (with `effect`) | `DrivePath`, `RepoName`, `Manifest`, `Allowance` |
| google | calendar/\* (with `effect`), google/\* | `Occurrence`, `Scope`, `Held` |
| connect | secrets, conformance (but not the model's laws), connect and its other submodules, deployment, settings (but not overview) | `Connection`, `Standing`, `Secrets`, `Settable` |
| inbox | wire/inbox, coding, s/{act_vocabulary, acts, card, diff, proposal, changes, decide, inbox, inbox_event, inbox_json, inbox_reading, inbox_registers, own_round, phase, proposer, responded, sending, sign_in, stamp, tap, tapped}; `Proposal`, `Reasons`, `Style` | `Act`, `Fold`, `ProposalId`, `InboxEvent`, `Proposal` |
| conversations | s/{about, answer, found, model (with `model/laws`), model_sessions, outcome, queue, search, store, taint, tool_name, turn_shape, verb_name} | `Entry`, `Stored`, `Kind`, `ConversationState` |
| lines | wire/lines, s/{act_lines, conversation_json, conversations, held_lines, held_replica, holder, line_event, line_form, line_shape, object_stores, outline, outlines, rescrub, step, targets, transcript} | `Conversations`, `LineEvent`, `HeldReplica`, `LineReplica` |
| topics | wire/topics, s/{answers, readings, rules, stream, title, topics} | `Topic`, `Target`, `Dress`, `Window` |
| digest | s/{caused, compact, compact_\*, compaction, digest\*, indexed_tree, memories, recollection, shown_record, told} | `Digest`, `DigestEvent`, `Memories` |
| cards | wire/cards, notify (with `effect`), s/{afresh, bootstrap, build, coding, connections, done_elsewhere, failing, filed, fixes, google_sign_in, mail_health, merged_patches, noticed, request, undo}, settings/overview | `Filing`, `Noticed`, `Notify` |
| secretary | agent (with `record`), conversation, s/{answering, briefings, record, briefs, call, daily, file_query, file_search, gate, live_shape, token_expiry, opening, read_holders, review, surface, sweep, verb_answer, verbs}, secretary (mod) | `Agent`, `Secretary`, `Model` (with their `compile_fail` tests) |
| api | server, clienterror, gate, portal, query, routes, s/routes, stores (`Kind`, `Store`, `Database`), wire (and `wire/shape`) | the route table, 75 wire types |

The wire split moves 26k of `wire.rs`: base 1.0k, record 1.1k, store 0.3k, signatures 0.9k,
todo 2.3k, held 1.1k, memory 0.1k, triage 0.3k, inbox 4.4k, lines/topics 5.2k and cards 1.3k.
The remaining 8.9k (75 types that only cli and pages name) stays in `api`.

## Where the cut is still over budget

- **USE over 25k**: todo 28.0, mail 27.0, inbox 28.3, conversations 26.7, secretary 30.3, api
  26.1, all upper bounds. The `//!` of each crate's private modules accounts for 2.4–3.9k of
  these figures. Cutting narration from those comments (AGENTS.md, rule 2) is the first remedy.
  If `todo` stays over, `release` (4.5k USE) becomes its own crate.
- **DEVELOP over 500k at the high bound**: secretary 699 and api 652. Their low bounds (470 and
  446) fit. The difference is dependency USE that they do not name, and the cause is their
  fan-in of 18–19 crates, not their size: splitting `verbs` out of `secretary` measured 541 and
  591, because both halves keep the fan-in and `verbs` ↔ `agent` is a cycle. Two remedies are
  measured: prodrome-core's USE down to 25k cuts 84k from every crate (secretary 615, api 568),
  and narrowing each dependency to what its dependents name. Stage 3 measures both again once the
  crates exist.
- **Outside core**: io (own 222k, names 160k of core) is 555k even at the low bound, cli (own
  355k, names 231k) is about 810k, and `server` is at least 414k. All three split along the same
  seams in a second plan: io by interpreter beside its port's crate (io-mail, io-store, io-model,
  io-sources), and cli into `server` plus its verbs by group. This plan does not cover them.

## The pull requests, in order

Each PR compiles and passes the gate alone. "Paths" counts the `crate::`/`suzatary_core::`
references that the PR rewrites, as measured.

**Stage 1: narrow (core stays one crate).**

| PR | Change | Size |
|---|---|---|
| 1 | Narrow the `pub` surface. Done in #372. | ~700 lines |

**Stage 2: break the cycles inside core.** One PR per row of the cycles table. Files and items
move to their future crate's modules, and no crate exists yet.

| PR | Change | Size |
|---|---|---|
| 2 | C8 and C10: ids and message parts move down | ~45 items, ~250 paths |
| 3 | C4: the `health` cluster, `mail::Fault`, the `Healths` builders up | ~12 files, ~120 paths |
| 4 | C5: `Tap` beside `Trusted`; `performs` to `inbox` | 3 files, ~40 paths |
| 5 | C6 and C7: `Proposal`/`Reasons`/`Style` to `inbox`, the `Entry` writers up, `mod.rs` split | ~10 files, ~200 paths |
| 6 | C9: the coding session's configuration to `connect`. Done in #434. | ~20 paths |
| 7 | C3 (in four parts; mail and triage in #438, the secretary's ports in #441, the role records in #442, the rest in #444): the `capability` ports beside their domains; `capability/mod.rs`'s 35 `pub use` lines shrink to `base`'s | ~30 files moved, ~400 paths |
| 8 | C1: `store` generic over `Schema`; `Kind` to the top. Done in #436. | `store/kind.rs` (162 lines), `store/database.rs` (99) |
| 9 | C2: the wire split by the table above; generator unchanged. Done in #447: 97 types to `wire/<cut>.rs`, 71 stay. | ~93 types out of `wire.rs` (2,795 lines) |
| 10 | Pin the layers until the crates exist. Done in #449: `app/cli/tests/guards/layers.toml` lists the cuts bottom first and their modules (beside its guard, since a table in `context.toml` would grow the reading set and the context tool reads that file strictly), and `guards/layers.rs` refuses a `crate::` or `super::` path that reaches a higher cut's module, judging a path by the item it reaches through any module a `use` binds. The sixteen edges still up are listed by name in `[layers] allowed` and the list only shrinks: the cards ↔ secretary cycle (`Bound`, `WRITES_AS`, `propose`, `filed`), `memory` → `lines` and `digest` (`erasure`, `opened`), `held_lines` → `digest` (#437), `held_replica` and `prune` → `routes` (#440), `readings` and `title` → `secretary` (`typst_prose_line`, `state`), and `todo`'s `reading` → `calendar`. The guard pins the eight sources `rendering.rs` hashes by SHA-256. First, `decide::performed` and `connections::{grant, connect, retire}` take the fields they write to instead of the `Server` record, and `DriveFailure` is `drive_path`'s. | one guard |

**Stage 3: the crates, bottom up, one PR each.** Each PR moves the crate's files with `git mv`,
adds its `Cargo.toml`, rewrites every path into it (core, io, cli, wasm, pages, plugin and the
tests), adds its `[[unit]]` to `context.toml` and a `CHANGELOG.md` line, and runs `scripts/context.sh
record`. No `suzatary_core` re-export is left behind (AGENTS §10).

| PR | Crate: files moved / paths rewritten | PR | Crate: files / paths |
|---|---|---|---|
| 11 | base 27 / 1,329 | 22 | google 15 / 146 |
| 12 | record 13 / 221 | 23 | connect 23 / 335 |
| 13 | store 29 / 263 | 24 | inbox 19 / 370 |
| 14 | signatures 21 / 164 | 25 | conversations 38 / 537 |
| 15 | todo 32 / 238 | 26 | lines 11 / 117 |
| 16 | held 16 / 239 | 27 | topics 13 / 75 |
| 17 | memory 16 / 237 | 28 | digest 20 / 108 |
| 18 | mail 23 / 352 | 29 | cards 19 / 57 |
| 19 | health 12 / 80 | 30 | secretary 92 / 200 |
| 20 | triage 16 / 93 | 31 | api 9 / 305; `suzatary-core` is deleted |
| 21 | services 22 / 234 | 32 | server: `cli/src/commands/serve/` (38 files) |

**PR 11 (base).** It moves 24 modules and `literals.rs`. Eight modules of the cut table's base stay
in core, each under the cut of its lowest user in `guards/layers.toml`: `cast` (signatures),
`coding_report` (memory), `outcome` (mail), `url` (google), `content_id` (inbox), `nonempty`
(conversations), `timings` (lines) and `model_name` (digest). With them, base's USE was 28.9k, and
a new unit joins no ratchet; without them it is 23.9k. Measured with `cargo check --workspace
--all-targets` on four cores, base as a crate does not make an edit faster: one line in
`secretary` took 46.0 and 43.6 s against 47.0 and 42.2 s. Core's library is still checked
(about 17 s), then core's unit tests (about 21 s), then cli's (about 17 s).

**The order after PR 11.** An edit is cheap only once its crate stops carrying what is under it,
and bottom-up that comes last for the files edited most, `routes.rs` and `secretary/routes.rs`. So
PR 12 is `api`, the top cut. It first removes the two edges into `routes` (`line_header` and
`husks` move into the lines' cut, as row 26 planned), then measures the same three edit cases
plus one line in `routes.rs`. If that case is not clearly faster, the order is bottom-up as in the
table. If it is, the rest go bottom-up from `record`, and `secretary` comes off the top once 29a
has removed the edges into it.

**PR 12 (api).**
- **First:** `line_header` and `husks` move into the lines' cut. The layer guard now reads a file up
  to its `#[cfg(test)]` module, not to its first `#[cfg(test)]` item. That found `settings ->
  notify`, which `channel_secrets` moving into `connect` removes.
- **Then `app/api`:** it takes `clienterror`, `gate`, `portal`, `query`, `routes`,
  `secretary/routes`, `server` and wire.rs's 71 types, with the generator and six integration
  tests.
- **`stores` stays in core under the api cut,** because the store's `InMemory` takes its hold from
  `Store`. The store crate's PR settles that.

Measured with `cargo check --workspace --all-targets`, one line edited, the second run of each:

| one-line edit in | before | after |
|---|---|---|
| `routes.rs` | 42.7 s | 23.2 s |
| `secretary` | 44.3 s | 39.4 s |
| base | 41.5 s | 40.5 s |

An edit in `routes.rs` now rechecks `api` (about 2 s) and the CLI, not core. Core's library check
fell from 17.3 to 15.1 s. The rest goes bottom-up from `record`, and `secretary` comes off the
top once 29a has removed the edges into it.

**What PR 12 left to revisit.**
- **Items made `pub` so that `api` can call them.** Any crate that depends on core can now call
  them too. When the cut that owns each one becomes a crate, see whether it can be `pub(crate)`
  there again, or whether `api` should reach it through a narrower door:

  | items | owning cut |
  |---|---|
  | `secretary::verbs::propose`, `secretary::bound_of`, `secretary::gate` | secretary |
  | `undo::withdrawn`, `done_elsewhere::{Seen, sweep}` | cards |
  | `readings::{drawn, proposal, turn, Place, blocks_of}` | topics |

- **The row-10 guard's hole changed the reading of two files.** In `settings.rs`, 292 lines of
  production code went unread and hid `settings -> notify`. In `secretary/held_lines.rs`, 8 lines
  of a test-only helper went unread and hid no edge. The sixteen allowed edges were counted with
  the hole; the guard now reads up to the `#[cfg(test)]` module.
- **The browser's modules link `api`.** The 71 wire types move down only after the bottom cuts they
  would move into exist.

**What a crate cut costs the pages' module.** `views_bg.wasm`, brotli, on aarch64-linux:

| release | the cut | bytes |
|---|---|---|
| 2.1.119 | base | 618,462 |
| 2.1.120 | api | 619,307 (+845, no code changed) |

The line is now 620,000 (613,000 on x86_64). Earlier cuts cost several hundred bytes each.
- **What grows is not measured yet.** This container has no wasm32 target and no `twiggy`. The
  module is built with fat LTO, one codegen unit, opt-level "s", symbols stripped and
  `panic = abort`. The candidates to tell apart:
  - panic and format strings that carry a module path or a file path (`#[track_caller]`
    locations, `expect` messages)
  - generic code instantiated once per crate before LTO and not merged after it, such as the
    serde impls of a type named from two crates
  - inlining that changes when a function crosses a crate boundary (a callee without `#[inline]`
    is inlined only by LTO, under a different cost model)
  - brotli's window over a reordered data section
- **The measure:** `twiggy diff` of `views_bg.wasm` between consecutive train builds, by function
  and by data segment, run on a machine with the wasm32 target before each bottom cut. A diff that
  is mostly data points at paths; one that is mostly code points at instantiation or inlining.

**PR 13 (record).** `decoration`, `record`, `todo` and `wire/record` move to `app/record`; USE
11.0k. The one item the cuts above call while it is `pub(crate)`, `todo::fields::plain_kind`,
becomes `pub` first.

**While PR 14 waits** for the feature trains on `store/`, three edges went: `reading -> calendar`
(the calendar's page is drawn in `calendar::page`), and `readings` and `title -> secretary`
(`typst_prose_line` is `title`'s; `state` is named at `store`, not through the re-export).

**PR 14 (store).** `store`, `replica`, `snapshot` and `wire/store` move to `app/store`, with
their test support and `husk`; USE 19.8k. The crate depends on base and the Prodrome alone.
- **Where the in-memory stores live.** The plan's open question on `InMemory`'s hold is settled
  by where it lives. `InMemory` reads its hold from `stores::Store`, which stays in core above the
  store cut, so it lives beside `Store` in `stores`. `InMemoryKeys` moves beside
  `secretary::locked_text`.
- **The store laws that hold an `InMemory`** (`signing`, `memoised`, and one of `learnt`'s) move to
  `core/tests/store_*.rs`. `learnt`'s law over `Knowledge` stays in the crate, with the doors'
  schema as `store::doors`.
- **The inert stores are generic.** The orphan rule refuses core an impl of the store's traits for
  base's `Nothing` at a schema core does not own: `TodoEvent` is record's, and five schemas are
  aliases of the Prodrome's `Event`. So `app/store/tests/support/nothing.rs` implements them once
  over any schema, and an inert chain now keeps an append nowhere instead of refusing it.
- **The layer guard** skips a module file that is only `#[cfg(test)] mod tests;`.
  `secretary::live_shape`, test and bench support that holds `InMemory`, takes the api cut.

**PR 15 (signatures).** `believed`, `chain_set`, `devices`, `epoch`, `signatures/*` and
`wire/signatures` move to `app/signatures`; USE 23.6k.
- **Two modules stay in core**, as PR 11 kept base's: nothing in the crate names them, and with
  them its USE was 28.5k. `cast` takes the todo cut, and `sync` takes the api cut, the cut of its
  one user (`api`'s routes). `receipt`, which #462 added to the cut, stays too, in the api cut
  beside `asks`, its one user in core.
- **What holds an `InMemory`** stays beside it, as in PR 14. `scene`, the test support that builds
  a `Trusted` over an in-memory chain, moves from `signatures` to `stores`. The tests that hold an
  `InMemory` move to core's integration tests: `signatures/tests.rs`, `chain_set`'s (with
  `Read::shares` public in the builds that compile the inert stores) and `tap`'s two. The `sooner`
  law stays with `sooner`.
- **The inert device log** for base's `Nothing` moves with `Device`: the orphan rule gives its impl
  to the type's crate.

**PR 16 (todo).** `authoring`, `decisions`, `reading`, `render`, `render_cache`, `rendering`
(with its typst sources), `template`, `todos`, `view` and `wire/todo` move to `app/todo`; USE
22.4k.
- **Four modules stay in core**, since nothing in the crate names them: `cast`, `priced` and `verb`
  take the held cut, and `release` the connect cut, where `deployment` reads it. Core calls
  `verbs!` and `Priced`, which are crate-private, so moving `verb` and `priced` would have widened
  them. Keeping `release` also keeps the train's `app/core/releases.toml` where it is.
- **One item widens:** `Reversal::of_note` becomes `pub`, as its printer `note()` already is,
  because `secretary::act_lines` in core reads a reversal's note.
- **The inert renderer** for base's `Nothing` moves with `Render`: the orphan rule gives its impl to
  the trait's crate. The rendering guard reads `rendering.rs` in `app/todo/src`; its include paths
  resolve alike from there, so no kept render is renamed.

**PR 17 (held).** `attachment`, `enclosed`, `excerpt`, `file_read`, `for_model`, `held_file`,
`held_value`, `leak`, `mail_error`, `message_part`, `pdf_text`, `redact`, `upload`,
`upload_index`, `vault` and `secretary/{attached, attachment_source, upload_standing}` move to
`app/held`, the last three under `suzatary_held::secretary`, so no path but the crate's changes.
- **`cast`, `priced` and `verb` stay in core** for PR 16's reason, and take the memory cut, now
  the lowest in core.
- **`first_read`** (#497, a process's timing accumulator) stays in core in the memory cut too:
  in base it measured 24.9k of base's 25k USE, and it keeps process state where base keeps names.
- **Nothing widens.** Every crate-private item of the crate is read only inside it.
- **Two test modules move to core's integration tests**, because they read higher cuts:
  `attachment`'s two transcript tests (`secretary::model::Turn`) and `vault`'s (`drive::allowed`).
- **The inert upload index and PDF reader** for base's `Nothing` move with their traits.

**PR 18 (memory).** `coding_report` and `secretary/{carried, erasure, fact_life, key_ring,
line_words, locked_text, memory, memory_event, provenance, salted_text, scrubbed_under, slots,
texts, two_phase, words}` move to `app/memory`, the secretary's under
`suzatary_memory::secretary`.
- **`holder` moves down with them.** `erasure` names `Holder` and `Fate`, and `words` names
  `erasure`, so the edge `erasure -> holder` was a cycle once the cut is a crate. `holder` names
  nothing of core's, so it moves to the lowest crate that names it, as C8 moved the ids.
- **`opened` stays in core** and takes the lines cut, the cut of its lowest user
  (`conversations`): nothing in the crate names it, and it names `digest`, `held_lines` and
  `line_event`. Two allowed edges go (`opened -> held_lines`, `opened -> line_event`), with
  `erasure -> holder`; `opened -> digest` stays.
- **The four that joined the cut stay in core**, each in its lowest user's cut, since nothing in
  the crate names them: `first_read` in lines (`conversations`, `held_lines`, `opened`), `cast` in
  connect (`deployment`), and `priced` and `verb` in inbox (`changes`).
- **Nothing widens.** Two test modules move to core's integration tests because they read higher
  cuts: `holder`'s two sweep laws (`transcript`, `rescrub`) and `line_words`' bounded tool answer
  (`turn_shape`). `InMemoryKeys` moves with `locked_text`.

**Rows 18 to 20 (mail, health, triage), one pull request.** Each cut is its own crate, and
each module keeps its path under the crate's name: `suzatary_mail::{mail, outcome}`,
`suzatary_health::health` and `suzatary_triage::{mail::*, rule, wire::triage}`.
- **mail** takes `mail.rs` and its 23 protocol modules, with `outcome` (base's table put it under
  mail). Its USE is 24.8k after the module docs lost the lists that their items' docs repeat.
  - One edge went first: `mail.rs` read triage's `state::Decision` through a relative path,
    which the layer guard does not read. The reading is now `From<&Decision> for Disposed` in
    `state.rs`.
  - The device flow's tap test reads core's test chain, so it moved to `core/tests/oauth_tap.rs`.
    The consent guard in `cli/tests/writes.rs` skips that file and the constructors' file.
- **health** moves whole, with its USE at 8.6k. `UNTOLD` and `RATE_WAIT` stay public, since public
  docs link them as the documented waits.
- **triage** takes the thirteen modules of core's `mail` that read triage's decisions, with `rule`
  and `wire/triage`, so core no longer has a `mail` module. Its USE is 23.8k. The
  `junk_seed` example moves with `normalize_seed_file`.
  - `heartbeat::freshness` becomes `pub`, because `secretary::verbs::briefing_delta` reads it.
  - `junk`'s bounded-sweep law names its account as the module's other sweeps do, so the
    law no longer reads `cast`.
- **What narrowed:** `SHOWN_HEADERS`, `LINK_SCHEMES`, `HEADER_CHARS` and `Domain::of_address`
  became private, because only their own file reads them.
- **The inert impls** for base's `Nothing` of `Mailbox`, `MailSync`, `Register`, `Corpus`, `Junk`,
  `OlderMail` and the watch log move with their traits.

Measured with `cargo check --workspace --all-targets` on four cores, one line edited, the
second run of each:

| one-line edit in | before | after |
|---|---|---|
| `mail` (`address.rs`) | 46.0 s | 47.4 s |
| `secretary` (`review.rs`) | 50.8 s | 44.0 s |
| `routes.rs` | 32.2 s | 31.4 s |

An edit in `mail` still rechecks core and everything above it. An edit in `secretary` is faster
because core is smaller: core's USE fell from 308.1k to 250.3k.

The pages' module, `views_bg.wasm` on x86_64-linux by `nix build`, grew by 227 bytes after
`wasm-opt` (3,602,822 to 3,603,049) and by 815 bytes brotli (644,785 to 645,600). No line of
`web/wasm-budget` rises.

**Rows 21 to 23 (services, google, connect), one pull request.** Each module keeps its path
under the crate's name: `suzatary_services::{drive, plugin, repos, repositories, repository,
usage, weather, web}`, `suzatary_google::{calendar, google, url}` and `suzatary_connect::{cast,
conformance, connect, deployment, secrets, settings}`.
- **services** moves whole; USE 14.1k. `repos::MAX_SHOWN_BYTES` becomes `pub`, because
  `secretary::verbs::propose_patch` reads it.
- **google** takes `url` from base's table; USE 14.7k. The calendar reading's two store laws hold
  an `InMemory`, so they moved to `core/tests/calendar_reading.rs`.
- **connect** takes `cast` from base's table; USE 24.3k.
  - `release` stays in core and takes the api cut. With it the crate's USE was 30.8k, and every
    `//!` of the crate comes to 1.4k, so no trim could pay for it. Its one reader in the cut was
    `Deployment::builder`, which since #534 takes the chain that holds the release plan from its
    callers instead of reading the plan.
  - `settings::overview` is the cards cut's, so core keeps it as an inline module,
    `suzatary_core::settings::overview`.
  - `conformance::hold` becomes `pub` (the model's laws read it), and `parsed!` is exported, at
    `suzatary_connect::parsed` (`coding` declares its ids with it).
- **Nothing narrows.** Each unnamed `pub` item is in a public signature or linked from a public
  doc (`MAX_TEXT_BYTES`, `CALENDARS`).
- **The flake names the library's crates once**, and each fileset names the parts it reads, so a
  crate cut adds one word to `flake.nix`.

Measured with `cargo check --workspace --all-targets` on four cores, one line edited, the second
run of each, master and branch interleaved on one target directory:

| one-line edit in | master | branch |
|---|---|---|
| `connect` (`check.rs`) | 29.6, 34.0 s | 34.7, 30.7, 34.6 s |
| `secretary` (`review.rs`) | 30.5, 34.5 s | 33.5, 37.5, 33.8 s |
| `routes.rs` | 23.1, 28.1 s | 23.7, 22.7, 23.1 s |

The spread between runs is larger than the change. Cargo's per-unit timings show it: core's
library and test checks take 18.7 s after a `secretary` edit (21.1 s on master) and 19.5 s after
a `connect` edit (22.4 s), while the CLI's test binary takes 16.7 s on both. Core's USE fell from
250.3k to 197.7k.

The pages' module, `views_bg.wasm` on x86_64-linux by `nix build`, grew by 452 bytes after
`wasm-opt` (3,603,049 to 3,603,501) and shrank by 1,137 bytes brotli (645,600 to 644,463). No line
of `web/wasm-budget` rises.

**Rows 24 to 26 (inbox, conversations, lines), one pull request.** Each module keeps its path
under the crate's name, the secretary's under `secretary`: `suzatary_inbox::{coding, content_id,
priced, secretary::*, verb, wire::inbox}`, `suzatary_conversations::{nonempty, secretary::*}` and
`suzatary_lines::{first_read, parts, timings, secretary::*, wire::lines}`.
- **inbox** takes `content_id` from base's table; USE 33.6k.
  - Two inherent methods moved first, since only a type's crate may declare one: `need.rs`
    (`Proposal::need`) left the conversations' `store/` and `request.rs` (`changes::Act::request`)
    left the cards cut. Neither layer guard nor table had seen them: an `impl` block names no path.
  - `verb` splits: `parse` and `Caught`, which `changes` and `priced` read, are
    `suzatary_inbox::verb`; the `Verb` trait, `verbs!` and `Chained` stay in core under the
    secretary cut. `Priced`, `Priced::price`, `PRICE_SPELLINGS` and `day_of` become `pub` (the
    agent reads them); its fields are `pub(crate)`.
  - Six inbox laws and the tap's law hold `stores::InMemory`, so they are core's integration tests.
- **conversations** takes `nonempty`; USE 26.7k. `search::{fold, around}` become `pub` (the file
  verbs read them). Core no longer re-exports `store::{ConversationState, state}`. `about`'s
  fixture moves to `app/conversations/tests`, where the web tests read it too.
- **lines** takes `timings`; USE 37.7k.
  - The two edges into `digest` went first: the box's gathered lines kept the digest's `Shape`, and
    `Opened` its `Kept`. Both are a value a reader derives from the lines' words, so
    `secretary/derived.rs` keeps one value per type, and the digest names its own types there
    (`Shape::over`). A forgetting still drops them all at once.
  - The in-memory conversations hold `stores::InMemory`, so they are `stores::conversations`, and
    the test modules of `act_lines`, `held_lines`, `opened` and `transcript` are core's
    integration tests. `transcript::{erased_lines, causal}` become `pub` (the digest reads them),
    and `Opened::says` and `held_lines::says` are test support under `nothing`.
  - Core has no `wasm` feature any more: `inbox_json` and `conversation_json` are their crates'.
- **What a further cut would take,** since no trim of `//!` reaches 25k (each crate's module docs
  come to 1.2–2k, and they state orders and boundaries):
  - inbox: what a proposal asks (`acts`, `changes`, `coding`, `decide`, `diff`, `fix`, `need`,
    `priced`, `proposal/*`, `request`, `sign_in`, `verb`, `wire/inbox`, about 22k) below the
    machine that holds it (`inbox`, `inbox_event`, `inbox_reading`, `inbox_registers`, `phase`,
    `responded`, `tapped`, `sending`, `stamp`, `own_round`, `proposer`, `card`, about 11k). Three
    names point up: `ProposalId` (read by `sign_in`, `fix`), `inbox_reading::Words` (`proposal`)
    and `responded::{Done, Performed, Tried}` (`decide`).
  - conversations: the model's port (`model` with its laws, `model_sessions`, 5.6k) below the
    entries.
  - lines: the replicas (`held_replica`, `own_line`, `husks`, `prune`, `outline`, `outlines`,
    `rescrub`, `object_stores`, about 11k) above the rest; one edge points down into them,
    `line_form -> outlines`. `timings`, `parts` and `first_read` (2k, a process's measures) could
    leave too.

Measured with `cargo check --workspace --all-targets` on four cores, one line edited, the second
run of each:

| one-line edit in | master | branch |
|---|---|---|
| `lines` (`transcript.rs`) | 37.3 s | 34.8 s |
| `secretary` (`review.rs`) | 38.7 s | 35.2 s |
| `routes.rs` | 27.3 s | 27.0 s |

Core's library check fell from 9.8 to 4.6 s and its unit tests' from 11.0 to 5.2 s. The largest
unit of every case is now the CLI's test binary, `all` (19.5 to 22.1 s), so `server` and the CLI's
split decide what an edit costs from here, more than the order of the remaining cuts. Core's USE
fell from 202.8k to 106.8k.

The browser's module, `prodrome_bg.wasm` on x86_64-linux by `nix build`, grew by 13,352 bytes after
`wasm-opt` (4,850,323 to 4,863,675): inbox 9,172, conversations 2,490, lines 1,690. Its strings
grew by about 640 bytes, so the rest is code. Its line moves from 4,860,000 to 4,870,000. The
pages' module grew by 400 bytes (3,602,402 to 3,602,802).

**Rows 27 to 29 (topics, digest, cards), one pull request.** Each module keeps its path under the
crate's name, the secretary's under `secretary`: `suzatary_topics::{secretary::*, wire::topics}`,
`suzatary_digest::{model_name, secretary::*}` and `suzatary_cards::{notify, secretary::*,
settings::overview, wire::cards}`. `[layers] allowed` is empty: no edge below the secretary cut
points up.
- **The six upward edges went first,** each by moving what was owned by the wrong cut or by
  passing a narrower value in:
  - `build`, `filed` and `undo` named the secretary's `Bound` only for the coding session's
    admission, which reads each chain's name, untrusted actors and todos. They take
    `build::Chain`, the `(&ChainPolicy, &dyn Prodrome<TodoEvent>)` pair that
    `chain_set::Read::of_stores` already read, and the secretary passes `Bound::chain`.
  - `mail_health` takes the secretary cut: `propose_persistent` proposes through the secretary's
    own `verbs::propose`, as `WRITES_AS`, over the whole `Secretary` record, and the CLI's brief
    is its one caller.
  - `filed`, `build` and `coding` take the topics cut: `readings` reads back the cards the door
    files, and the door names `readings::state`. So `memories -> filed` points down.
- **Two edges no guard saw:** `notify`'s law over every push read `secretary::answer_push` from a
  test module, so `answer_push` moved to `notify`, beside `decide` and `needs_you`. And the inert
  `Memories::nothing` is an inherent impl, so it moved to the digest crate's test support.
- **Nothing widens.** PR 12's `undo::withdrawn` and `done_elsewhere::{Seen, sweep}` stay `pub`,
  since `api` reads them; `readings::{drawn, proposal, turn, Place, blocks_of}` too.
- **The laws that hold `stores::InMemory`** are core's integration tests: `filed`, `rules`,
  `set_up`, `title`, `memories`, `failing`, `fixes`, `undo`, most of `noticed`, and the
  overview's draft law. Five laws read private state and hold no store, so they stay beside their
  code; what both halves build is test support under `nothing` (`overview::world`,
  `noticed::owing`). `InMemory` cannot move below the cuts that test with it: it takes each
  schema's hold from `stores::Store`, whose table names every schema up to the api cut's
  `AskEvent`, and the orphan rule refuses a lower trait's impl for the five schemas that are
  aliases of the Prodrome's `Event`. At row 31 `stores` moves to `api`, and these laws become
  `api`'s integration tests.
- **USE:** topics 20.5k, digest 28.4k, cards 13.3k; core falls from 106.8k to 44.4k. The digest
  is over the line, and a further cut would take the memory's reading (`memories`,
  `recollection`, `told`, `shown_record`, about 6.6k) above the summary tree; three names point up
  across it and move first: `memories::Held` (read by `compaction`, `compact`, `digests`),
  `memories::Memories` (`compaction`) and `memories::whole` (`digests`).

Measured with `cargo check --workspace --all-targets` on four cores, one line edited, the second
run of each, master and branch on one target directory:

| one-line edit in | master | branch |
|---|---|---|
| `lines` (`transcript.rs`) | 38.5 s | 33.3 s |
| `secretary` (`review.rs`) | 35.6 s | 31.7 s |
| `routes.rs` | 28.0 s | 28.1 s |

Core's library check fell from 4.8 to 1.9 s and its unit tests' from 6.2 to 2.1 s. What a session
that works in one crate waits for, `cargo check -p <crate> --all-targets` after one line there (on
master the three cuts were core's, so their edit checked core):

| one-line edit in | master | branch |
|---|---|---|
| `lines` (`transcript.rs`) | 3.5 s | 3.7 s |
| `topics` (`readings.rs`) | 9.9 s | 3.3 s |
| `digest` (`digest.rs`) | ≈ 10 s, as core | 1.6 s |
| `cards` (`noticed.rs`) | ≈ 10 s, as core | 1.8 s |
| core (`review.rs`) | 10.6 s | 4.7 s |

The CLI's test binary `all` is about 21.8 s of every case. 48 of its 71 modules each declare
`mod common;`, so the shared test support (about 5,000 lines) is checked 48 times, about 0.5 s
each; leaving out any one of the six largest test modules moves the figure by no more than the
spread between runs. Declared once in `all.rs`, with each file naming `crate::common`, the check
measured 4.0 s. Done in the next pull request: after a one-line edit in a CLI test module, the
workspace check fell from 21.1 s to 4.7 s and the binary's build from 35.6 s to 11.0 s.

The browser's module, `prodrome_bg.wasm` on x86_64-linux by `nix build`, shrank by 582 bytes after
`wasm-opt` (4,863,675 to 4,863,093), and the pages' module grew by 46 (3,602,802 to 3,602,848). No
line of `web/wasm-budget` moves.

**Rows 30 and 31 (secretary, and the api cut's remainder), one pull request.** No crate is named
core. Each module keeps its path under its crate's name: `suzatary_secretary::{agent, asks,
conversation, secretary::*, stores, verb}`, `suzatary_api::sync` and `suzatary_release::release`.
- **The api cut's remainder went first,** each module to the crate that owns its meaning:
  - `release` is a crate of its own, over `record`, against the crate table, which gave it to
    `todo` for `releases.toml`. That file is no longer in this tree (it is beside the roadmap
    store), and `train.sh` reads `app/release.toml`, which does not move. Nothing in `todo`
    names `release`, and it names only `record` and the Prodrome; in `todo` it took that crate's
    USE from 24.9k to 31.5k and gave every crate above it 6.6k more to read, none of which names
    a release. Its readers are `api`, `pages` and the CLI, its changelog guard among them.
  - `sync` decides what the box takes from a replica, and its one reader is `routes.rs`, so it is
    `api`'s, with its counterexamples and the two laws that post through it (`lines_rule`,
    `topics`).
  - `wire` held no module since #447, and `receipt` was already the store crate's.
  - `stores` and `asks` stay with the secretary, against the crate table. Nothing below the
    secretary names them, and `io`, `api`, `wasm` and the CLI, which do, already depend on it. In
    `api`, `io` would wait for `api` (2.3 s of library check) and recheck after each edit to
    `routes.rs`. And the secretary's own unit tests hold the in-memory stores: the `remembered`
    law reads its crate-private function over a `live_shape` memory, so it would need a
    dev-dependency cycle through `api` or a widened `remembered`. `live_shape` stays with them.
- **The vectors** the browser checks are `api`'s, at `app/api/tests/web`, beside their writer,
  `web_vectors.rs`. `vector_json`, which reads the Prodrome's conformance files, is `todo`'s
  example.
- **Then core is renamed** by `git mv` to `app/secretary`, the package `suzatary-secretary`. A
  trial rename first found no tie the compiler did not; three compile-fail doc tests in
  `signatures` had named `suzatary_core::signatures`, a crate `signatures` never depended on, and
  now refuse for the private field.
- **Core's test binary split.** The 18 files that name nothing of the secretary went to the
  highest crate each names: `todo` (`tree`, `decisions_schema`), `memory` (`memory_schema`),
  `services` (`vault`), `connect` (`connect`, `drive_port`, `puller_port`, `transport_port`),
  `inbox` (`act_literal`, `card_stages`), `conversations` (`attachment`, `held`, `line_words`)
  and `lines` (`conversation_id`, `line_replica`, `sequence`, `snapshot`). The other 40 hold
  the secretary's items, most of them the in-memory stores, and stay. `laws.rs`, which three
  crates' schema tests call and which names only `prodrome_core::fold::Order`, is
  `suzatary_store::laws` under `nothing`. `release` and `fragment` are the release crate's tests.
  `#[test]` functions: 2,222 on master and 2,221 here, the one gone being the layer guard's.
- **The layer guard** held nothing once core was one cut, so it is gone; what is left is the pin
  of the sources `rendering.rs` hashes, `guards/rendering.rs`.
- **Nothing widens.** `suzatary_store::laws` is test support under `nothing`.
- **USE:** secretary 35.4k, `api` 25.6k with `sync`, `release` 6.7k; `todo` stays at 24.9k.
  The two over the line are in `[ratchet.use]` at their figures. A further cut of each:
  - api: `wire` with `wire/shape` (10.2k), which `routes`, `query`, the pages and the CLI read.
    It names `release` and `stores` from below; the one name that points across is
    `From<&portal::Portal> for PortalPayload`, which would move beside `Portal`.
  - secretary: the triage agent (`agent`, 6.4k of USE) below the secretary's turn. The
    secretary's record and surface name `Agent`, `Holds` and `Says`, and its verbs reuse the
    agent's (`agent::verbs`, `reads`, `answered`, `refused`); all point down.

Measured with `cargo check --workspace --all-targets` on four cores, one line edited, the second
run of each, master (2.1.168, where the CLI's test binary checks its support once) and branch
interleaved on one target directory:

| one-line edit in | master | branch |
|---|---|---|
| `lines` (`transcript.rs`) | 20.5, 21.2 s | 18.9, 19.2 s |
| `secretary` (`review.rs`) | 17.7, 17.4 s | 15.5, 16.2 s |
| `routes.rs` | 13.0, 13.4 s | 12.5, 13.8 s |

An edit at or below the secretary is about 1.5 s faster: the secretary's test binary lost 18
files to crates that check beside it. What a session that works in one crate waits for,
`cargo check -p <crate> --all-targets` after one line there:

| one-line edit in | master | branch |
|---|---|---|
| core / `secretary` (`review.rs`) | 5.2, 5.4 s | 4.4, 4.4 s |
| `api` (`routes.rs`) | 4.7, 4.5 s | 4.8, 5.0 s |
| `io` (`objects.rs`) | 4.1, 4.4 s | 4.4, 4.3 s |

`api` gained `sync`, its unit tests and two laws; `io` is unchanged within the spread.

The browser's module, `prodrome_bg.wasm` on x86_64-linux by `nix build`, grew by 5,256 bytes
after `wasm-opt` (4,865,823 to 4,871,079): its code by 5,247 bytes and two functions, its data
by 7. Its x86_64 line moves from 4,870,000 to 4,880,000. The pages' module shrank by 9 bytes
(3,602,848 to 3,602,839). Both modules still compile the secretary crate, the mail crates and
the sources, since `wasm` and `pages` depend on `api`, whose server records and secretary routes
name them; LTO keeps none of it it does not call. Cutting them off takes `api`'s server half
(`server`, `secretary/routes`) out, which is row 32, and the stores' table (`stores`, `asks`,
which `wasm` names) below the secretary, into a crate above `topics` and `digest`.

The crates after PR 31, from `scripts/context.sh` at this branch's head, in thousands of tokens.
"own" is the crate's own files in its DEVELOP, "deps" its direct dependencies but the Prodrome.

| # | Crate (`app/<name>`) | own | use | deps | DEVELOP |
|---|---|---|---|---|---|
| 1 | `base` | 58.2 | 24.6 | 0 | 226 |
| 2 | `record` | 31.9 | 11.1 | 1 | 224 |
| 3 | `release` | 13.4 | 6.7 | 1 | 192 |
| 4 | `store` | 47.0 | 23.9 | 1 | 239 |
| 5 | `signatures` | 49.0 | 23.6 | 3 | 276 |
| 6 | `todo` | 74.7 | 24.9 | 4 | 326 |
| 7 | `held` | 68.4 | 24.1 | 2 | 285 |
| 8 | `memory` | 38.7 | 18.3 | 3 | 279 |
| 9 | `mail` | 46.3 | 24.6 | 3 | 286 |
| 10 | `health` | 17.2 | 8.5 | 3 | 258 |
| 11 | `triage` | 41.2 | 22.8 | 4 | 306 |
| 12 | `services` | 31.4 | 14.1 | 3 | 256 |
| 13 | `google` | 33.3 | 14.6 | 5 | 313 |
| 14 | `connect` | 57.6 | 24.1 | 7 | 347 |
| 15 | `inbox` | 106.1 | **33.5** | 13 | **540** |
| 16 | `conversations` | 63.8 | **26.7** | 7 | 389 |
| 17 | `lines` | 124.3 | **39.2** | 12 | **565** |
| 18 | `topics` | 63.2 | 20.5 | 14 | **558** |
| 19 | `digest` | 73.9 | **28.4** | 11 | **523** |
| 20 | `cards` | 49.5 | 13.3 | 15 | **550** |
| 21 | `secretary` | 178.8 | **35.4** | 19 | **767** |
| 22 | `api` | 107.9 | **25.6** | 21 | **744** |

**Row 32 (the server's half of `api`), one pull request.** The box's half of `api` is the crate
`suzatary-server`, and each module keeps its path under the crate's name:
`suzatary_server::{gate, portal, routes, secretary::routes, server}`. The CLI's `serve` is still
the CLI's; what blocks it is below.
- **The seam is who names what.** The browser's modules name `wire`, `query` and two items of
  `routes` (`fold`, `MAX_POSTED`); the box names everything. So:

  | module | named by | went to |
  |---|---|---|
  | `wire`, `wire/shape` | `wasm`, `pages`, the CLI, the generator | stays |
  | `query` | `wasm`, the CLI | stays |
  | `routes` | `wasm` (`fold`, `MAX_POSTED`), the CLI, `api`'s tests | stays, cut at the item: `briefings`, `briefing` (the secretary's `Briefings` and briefs) and `calendar` (Google) are `suzatary_server::routes` |
  | `sync` | `routes` | stays |
  | `clienterror` | the CLI's `serve`, the generator (`Sent`) | stays whole: the seam between `Sent` and `Report` would widen `Kind::name`, and it names only `base` and `held` |
  | `server`, `secretary/routes` | the CLI, `web_vectors` | server |
  | `gate` | the CLI (`serve`, `call`, `doctor`) | server |
  | `portal` | the CLI (`instance`, `serve`) | server, with `From<&Portal> for PortalPayload` |
- **Three ties went first:** `Erasing`, a body the generator declares, is `wire`'s; `routes::author`
  takes the chain, clock and renderer it read from the `Server` record; and `PortalPayload`'s
  `From` moved beside `Portal`. The generated `wire.ts`, `payload.ts` and `pages.ts` are master's,
  byte for byte. `MAX_BRIEFINGS` went with its one reader.
- **`api` no longer depends on** `digest`, `memory`, `health`, `services` or `google` (the last
  four stay dev-dependencies of its laws). It names the secretary crate only through `stores` and
  `asks`, so both browser modules still compile the secretary, the mail crates and the sources:
  `pages` through `api`, `wasm` through `api` and directly. The stores' table as a crate of its
  own, below the secretary, is the cut that removes them.
- **The CLI's `serve` is not a crate yet.** A trial move into the crate leaves 42 paths into the
  CLI unresolved, and the CLI's other verbs name `serve` back:
  - `serve` names the instance (`Instance`, `Reserve`, `File`, `InstanceEdits`, `Mounts`,
    `Plugins`, `Project`), `wiring` (`Ground`, `Opened`, `Stores`), `stamp` (`build`, `release`,
    `Release`) and nine verbs' helpers: `secretary` (`Acting`, `ActingStores`, `Postman`,
    `system_prompt`, `BRIEF`), `mail` (`tokens`, `syncer`, `notmuch`, `google_grants`), `plugin`
    (`sync`, `effects`, `components`), `google::sync_calendars`, `notify::push`,
    `coding::spawn_polling`, `doctor::upkeep_line`, `fixture::Scripted` and `commands/mod.rs`.
  - `wiring`, `instance`, `secretary`, `coding`, `mail`, `plugin`, `verify` and `ran` name
    `serve::interpreters` (the ports interpreted from the instance) and `serve::mail_cache`;
    `secretary` and `coding` name `Workers`, `Brain` and `sign_ins`; `ask`, `tap` and `call` name
    `route`, `table` and `asked`; `doctor` names `door`.
  - What removes it: `interpreters` and `mail_cache` are the CLI's wiring, not the server's, and
    move beside `wiring.rs`; the run (`run`, `serving`, `listen`, `performers`, the `real_*`
    builders and each `of(&Instance)` constructor) stays the CLI's verb and hands the crate a
    built `State`; `State` holds the values it reads instead of `Mounts`, `Plugins`, `Postman`
    and `Stamp`; `Checks` takes the plugin and calendar syncs as values; `system_prompt` and
    `BRIEF` move down to the secretary crate. Then `serve` names nothing of the CLI, and its two
    `gate` modules (the door's rules and the axum wrapper) become one.
- **Tests:** `web_vectors.rs` names the secretary's routes, so it is the server's `tests/all.rs`.
  Its vectors stay at `app/api/tests/web`: they are what the tab and the box share, and `pages`
  reads them through its dependency on `api`. Moved beside their writer, they needed a
  `crate_reads` entry for `pages`, which the gate's unit fixture does not know, so every unit
  ran. The CLI's tests that name `serve` stay with it. `#[test]` functions: 2,236 on master and here; `proptest!`
  blocks 164 on both.
- **Nothing widens.** Every item the crate's dependents name was `pub` in `api`.
- **USE:** `api` 19.0k, back within the line, so its `[ratchet.use]` row is gone; `server` 6.5k.
  The reading set is 1,205 tokens under master's: the new manifest and root and the longer paths
  cost 1,096, and an import-once pass over the production code of the files the branch touches
  paid it back.

Measured with `cargo check` on four cores, one line edited, the second run of each, master and
branch on one target directory (the secretary's routes were `api`'s on master):

| one-line edit in | master | branch |
|---|---|---|
| `--workspace --all-targets`, `lines` (`transcript.rs`) | 16.6 s | 15.9 s |
| `--workspace --all-targets`, `routes.rs` | 10.5 s | 10.0 s |
| `--workspace --all-targets`, the secretary's routes | 10.5 s | 8.7 s |
| `--workspace --all-targets`, the conversation worker | 8.1 s | 8.4 s |
| `-p suzatary-api`, `routes.rs` | 4.2 s | 2.7 s |
| `-p` api / `suzatary-server`, the secretary's routes | 3.9 s | 0.8 s |
| `-p suzatary`, the conversation worker | 8.7 s | 8.1 s |
| `-p suzatary`, a verb (`todo.rs`) | 8.5 s | 8.0 s |

A cold `cargo build -p suzatary-wasm --target wasm32-unknown-unknown` took 79.5 s on master and
77.8 s here, 141 units on both: the browser's developer still compiles every model crate.

The browser's module, `prodrome_bg.wasm` on x86_64-linux by `nix build`, grew by 397 bytes after
`wasm-opt` (4,877,806 to 4,878,203), and the pages' module shrank by 1,757 (3,602,881 to
3,601,124). No line of `web/wasm-budget` moves.

The crates after PR 32, from `scripts/context.sh` at this branch's head, in thousands of tokens:

| # | Crate (`app/<name>`) | own | use | deps | DEVELOP |
|---|---|---|---|---|---|
| 1 | `base` | 58.2 | 24.6 | 0 | 226 |
| 2 | `record` | 31.9 | 11.1 | 1 | 224 |
| 3 | `release` | 13.4 | 6.7 | 1 | 192 |
| 4 | `store` | 47.0 | 23.9 | 1 | 239 |
| 5 | `signatures` | 49.0 | 23.6 | 3 | 276 |
| 6 | `todo` | 74.7 | 24.9 | 4 | 326 |
| 7 | `held` | 68.4 | 24.1 | 2 | 285 |
| 8 | `memory` | 38.7 | 18.3 | 3 | 279 |
| 9 | `mail` | 46.3 | 24.6 | 3 | 286 |
| 10 | `health` | 17.2 | 8.5 | 3 | 258 |
| 11 | `triage` | 41.2 | 22.8 | 4 | 306 |
| 12 | `services` | 31.4 | 14.1 | 3 | 256 |
| 13 | `google` | 33.3 | 14.6 | 5 | 313 |
| 14 | `connect` | 57.6 | 24.1 | 7 | 347 |
| 15 | `inbox` | 106.1 | **33.5** | 13 | **540** |
| 16 | `conversations` | 65.5 | **27.3** | 7 | 391 |
| 17 | `lines` | 134.7 | **40.9** | 12 | **576** |
| 18 | `topics` | 63.2 | 20.5 | 14 | **560** |
| 19 | `digest` | 74.0 | **28.4** | 11 | **525** |
| 20 | `cards` | 49.5 | 13.3 | 15 | **552** |
| 21 | `secretary` | 180.2 | **35.7** | 19 | **771** |
| 22 | `api` | 66.3 | 19.0 | 16 | **621** |
| 23 | `server` | 41.8 | 6.5 | 20 | **665** |

**After row 32: the stores' table, one pull request.** `stores` and `asks` are the crate
`suzatary-stores`, between `digest` and `cards`, and each keeps its path under the crate's name:
`suzatary_stores::{stores, asks}`.
- **Where the table sits is decided by what it names.** The rows name `TodoEvent` and `GENESIS`
  (`record`), `Hold`, `Admission` and `Shared` (`store`), `DecisionEvent` (`todo`), `MemoryEvent`
  (`memory`), `InboxEvent` (`inbox`), `LineEvent` and `LinesRule` (`lines`), `MembershipEvent`
  and `AliasEvent` (`topics`), `DigestEvent` (`digest`) and the Prodrome's `Declared`.
  `Database` adds `ChainName` (`base`), `Device` (`signatures`), `AttachmentId` and `Upload`
  (`held`), the memory's texts and keys, `PluginName` (`health`), `TriageState` and `LocalState`
  (`triage`), and the inbox's and the lines' `findings`. `asks` names `RefusalEvent` (`store`) and
  `Kind`. The highest of these is `digest`, so the table sits directly above it, and nothing in it
  names `cards`. The macro's impls are of the crate's own traits (`Store`, `Written`) and, through
  `word_literal!`, for its own `Kind`, so the orphan rule allows each of them in this crate, aliases
  of the Prodrome's generics (`Shared<MembershipEvent>`, `Declared`) among them.
- **The secretary's shipped code names nothing of the table.** `Secretary` and `Agent` are built
  over `store`'s `dyn Prodrome<E>`, `Doc` and `Lists`, never over `Store` or `Kind`. Only test
  code reads it: `secretary::live_shape` (under `nothing`) and the unit tests of `mail_health`,
  `review` and the `remembered` law. So the secretary depends on the crate only under `nothing`,
  as an optional dependency, and `live_shape` stays at its path; nothing of the table stays above.
- **A trial move first.** It compiled at the first try except for two ties no guard sees. The test
  scene signs with the cast's keys (`connect`) and parses an instant (`chrono`), which the new
  crate takes as optional dependencies under `nothing`. And `api`'s unit tests call
  `cards::settings::overview::calendar`, a `nothing` item that only the secretary's `nothing` had
  enabled for them, so `api` takes `cards` with `nothing` as a dev-dependency.
- **Tests.** Of the secretary's 40 test files, the 26 that name the table and the crates below it,
  and nothing of the secretary, are the new crate's `tests/all.rs`, with `digest.` and
  `inbox.proptest-regressions` and the support the table's library includes (`in_memory`,
  `conversations`, `scene`). The six that also name `cards` (`failing`, `fixes`, `lapsed`,
  `noticed`, `settings_overview`, `undo`) test the cards' modules; `cards` and the table do not
  depend on each other, so they are the cards crate's first `tests/all.rs`, over a dev-dependency
  on the table. The eight that name the secretary's items stay (`calls`, `context_cost`, `filed`,
  `held_values`, `model_port`, `surface`, `triage_cards`, `triage_memory`), with
  `tags.proptest-regressions`, which no test file reads. `crate_reads` names the new crate for
  `calendar_reading.rs`'s read of `app/plugin-ics/ics.schema`. `#[test]` functions: 2,239 on
  master and here; `proptest!` blocks 164 on both.
- **The browser no longer compiles the secretary crate.** `cargo tree -e normal` for both
  `suzatary-wasm` and `suzatary-pages` is `api base cards connect conversations digest google
  health held inbox lines mail memory record release services signatures store stores todo
  topics triage`. What is left, and what pulls it in:
  - `digest`, `health`, `triage`: the table's rows and `Database` (`DigestEvent`, `PluginName`,
    `TriageState`), and `lines` and `inbox` name `health` and `triage` too. A `Database` of
    `dyn` stores, split from the rows, would cut `health` and `triage` from the table, not from
    the module.
  - `cards`: `api`'s settings route and its wire (`settings::overview`, `OverviewLine`); the
    overview's reading in a crate below `cards`, or in `api`, would cut it.
  - `mail`, `services`, `google`, `connect`: `inbox` (the acts name accounts, drive paths,
    calendar events and connections), `conversations`, `topics` and `api`'s wire. Cutting them
    takes the acts' payloads out of `inbox` into crates of their own.
  - The wire carries these of its own accord, and they stay: `MailMessage` and `MailRow`
    (`mail`), `WatchesPayload` and `Gone` (`triage`), and `OverviewLine` (`cards`).
- **Every list follows:** the workspace, `flake.nix`'s `library`, `LIBRARY_CRATES`, `live_reads`,
  `crate_reads`, `context.toml` (a unit, and `depends` for the secretary, `api`, `server`, `io`,
  the CLI and `wasm`), the two scripts and two documents that name the table's file or path.
  `app-build.sh` and the gate's `fixture_tests` still enable `suzatary-secretary/nothing`, which
  now carries `suzatary-stores/nothing`, the feature that owns the stores held in memory.
- **Nothing widens.** Every item a dependent names was `pub` in the secretary.
- **USE:** secretary 33.4k (35.7k on master), its `[ratchet.use]` row lowered to 33,374; the
  table 2.4k. The reading set is 383 tokens under master's (2.1.179): the new manifest and root
  cost about 830 and the cards' test manifest about 140, and an import-once pass over the
  production code of the files the branch touches paid that back.

Measured with `cargo check` on four cores, one line edited, the second run of each, master and
branch interleaved on one target directory:

| one-line edit in | master | branch |
|---|---|---|
| `--workspace --all-targets`, `lines` (`transcript.rs`) | 17.7, 16.2 s | 16.8, 15.4 s |
| `--workspace --all-targets`, the secretary (`review.rs`) | 13.2, 13.3 s | 11.5, 12.1 s |
| `--workspace --all-targets`, the table (`stores.rs`) | 14.7, 13.3 s | 13.1, 12.3 s |
| `-p suzatary-secretary --all-targets`, `review.rs` | 3.3, 3.6 s | 3.0, 3.0 s |
| `-p` secretary / `suzatary-stores --all-targets`, `stores.rs` | 3.3, 3.7 s | 1.4, 1.9 s |

A cold `cargo build -p suzatary-wasm --target wasm32-unknown-unknown` took 76.1 s on master and
72.1 s here, 141 units on both: the secretary's unit is gone and the table's has come, and no
other crate leaves, since the rest is named by `inbox`, `lines`, `topics` and `api`'s wire.

The browser's module, `prodrome_bg.wasm` on x86_64-linux by `nix build`, grew by 466 bytes after
`wasm-opt` (4,839,778 to 4,840,244, at 2.1.179), and the pages' module shrank
by 1,664 (3,601,163 to 3,599,499). No line of `web/wasm-budget` moves.

The crates after this cut, from `scripts/context.sh` at this branch's head, in thousands of
tokens:

| # | Crate (`app/<name>`) | own | use | deps | DEVELOP |
|---|---|---|---|---|---|
| 1 | `base` | 58.2 | 24.6 | 0 | 242 |
| 2 | `record` | 32.0 | 11.2 | 1 | 240 |
| 3 | `release` | 13.4 | 6.7 | 1 | 208 |
| 4 | `store` | 47.0 | 23.9 | 1 | 255 |
| 5 | `signatures` | 49.0 | 23.6 | 3 | 292 |
| 6 | `todo` | 74.7 | 24.9 | 4 | 342 |
| 7 | `held` | 70.0 | 24.3 | 2 | 302 |
| 8 | `memory` | 38.7 | 18.3 | 3 | 295 |
| 9 | `mail` | 46.3 | 24.6 | 3 | 302 |
| 10 | `health` | 17.2 | 8.5 | 3 | 274 |
| 11 | `triage` | 41.2 | 22.8 | 4 | 322 |
| 12 | `services` | 31.4 | 14.1 | 3 | 272 |
| 13 | `google` | 33.3 | 14.6 | 5 | 329 |
| 14 | `connect` | 57.6 | 24.1 | 7 | 363 |
| 15 | `inbox` | 106.1 | **33.5** | 13 | **556** |
| 16 | `conversations` | 65.5 | **27.3** | 7 | 407 |
| 17 | `lines` | 134.7 | **40.9** | 12 | **592** |
| 18 | `topics` | 63.2 | 20.5 | 14 | **576** |
| 19 | `digest` | 74.0 | **28.4** | 11 | **542** |
| 20 | `stores` | 6.0 | 2.4 | 15 | **546** |
| 21 | `cards` | 49.5 | 13.3 | 15 | **569** |
| 22 | `secretary` | 175.0 | **33.4** | 20 | **784** |
| 23 | `api` | 66.3 | 19.0 | 16 | **604** |
| 24 | `server` | 41.8 | 6.5 | 21 | **681** |

Stage 4 is the remedies of the over-budget section, ordered by
what that measurement shows. Stage 5 moves the stable crates to their own repositories, pinned
by rev like the Prodrome (→ `prodromedependency`), which takes them out of every train's
compile.

## What else moves

- **Tests**: `core/tests/all.rs` (one binary, `autotests = false`) splits into one `tests/all.rs`
  per crate, and each test module goes to the crate it exercises: `inbox*`, `card_*` and
  `act_literal` to inbox; `memory*` to memory; `held*` to held; `held_lines` and `transcript` to
  lines; `topics` to topics; `digest` to digest; `triage_*` to cards; `surface`,
  `mcp-surface.json`, `secretary-surface.json`, `model_port` and `calls*` to secretary;
  `snapshot`, `object_replica` and `frontiers` to store; `pieces`, `laws`, `tree`, `release` and
  `decisions_schema` to todo; `drive_port`, `puller_port` and `transport_port` follow their ports;
  `web_vectors` goes to api. `tests/support` and `proptest-regressions/` go with them. The vectors stay FROZEN (→
  `oneevaluator`).
- **Features**: `nothing` moves to every crate with inert effects (`base`, `store`,
  `signatures`, `conversations`, `lines`, `secretary`). `fixture` goes to `lines` and
  `secretary`. `wasm` goes to `inbox` (`inbox_json`). `app-build.sh` and `merge-gate.sh` (lines
  373 and 323) name the new features and crates.
- **The wasm build**: `app/wasm` depends on store, signatures, todo, lines, topics,
  conversations, inbox (feature `wasm`), base and api (`routes`, `query`). It no longer compiles
  the secretary's turn, the mail crates or the sources. `pages` depends on the crates whose wire
  it shows.
- **The wire generator**: `wire::tests::generates` moves to `api`. `scripts/wire.sh` (line 22)
  and `flake.nix`'s `.#suzatary-web-ts` (line 655) run `-p suzatary-api`. In `context.toml`, the
  `wire/generated/wire` and `payload` witnesses name `api`.
- **Nix (crane)**: the source filters at `flake.nix` 204, 325–329, 456–459, 547 and 594 list
  `app/<crate>` in place of `app/core`. `suzatary-core-deps` and `checks.suzatary-core` (lines
  342 and 1040) become one dependency build and one check over the model crates.
  `cargoArtifacts` stays keyed on `Cargo.lock`. The examples follow their crates: `vector_json`
  goes to todo (`flake.nix` 792, `web-test.sh` 31) and `junk_seed` to triage (843).
- **The release plan**: `app/core/releases.toml` moves to `app/todo/releases.toml`. `train.sh`
  reads it at lines 285, 325, 1031, 1060 and 1116.
- **The guards**: `guards/context.rs` pins the new units; `lists.rs` follows `Kind` to `api`;
  `changelog.rs` follows `release` to todo; `merge_gate.rs` and `publication.rs` follow the crate
  paths. `merge-gate.sh`'s `crate_reads`/`live_reads` (lines 323 and 508) and the kept passes (→
  `gatekeptpasses`) follow the read paths.
- **`context.toml`**: one `[[unit]]` per crate, with `depends` equal to its `Cargo.toml`, and the
  `ratchet.use` row for `core` deleted. `app/core/README.md`'s citations (34k of plans) are cut
  to each new crate's README.

## Generic pieces that belong in Prodrome

After C1, `store` and `replica` are generic over a schema. Their only other Suzatary names are
`ChainName`, `Refusal` and `wire::SinceRead`. Once those become the store's own types, `store`
joins prodrome-core's `store` module and `replica` becomes a new Prodrome module, so neither
needs a repository of its own. Prodrome's USE (109k, over its own budget) is the largest fixed cost
of every crate here; splitting it is that repository's plan.
