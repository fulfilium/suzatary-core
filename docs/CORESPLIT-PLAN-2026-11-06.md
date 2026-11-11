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
| C2 | `wire.rs` holds every route's types, and low modules use them (`FailedChain`, `SinceRead`, `DevicePayload`, `ErrorPayload`, `MailWatch`, `Card*`) | 50 | Split by concept: each wire type moves to the lowest crate that names it (table below). The ts-rs generator test moves to `api`. |
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
| base | capability (mod, clock, entropy, manuals, typeset, whois), cast, chains, content_id, escape, finite, model_name, narrowing, nonempty, persona, reference, refusal, rule_version, seat, table, term_json, timings, tree, tripwire, url, words, zone; the C8 ids | `Refusal`, `Reference`, `ChainName`, `Zone`, `Secret`, `Datetime`, `Clock`, `Entropy`, `Typeset` |
| record | decoration, record, todo | `TodoEvent`, `Record`, `MarkupSource` |
| store | replica/\*, snapshot, store/\* (with `Hold`) | `Prodrome`, `Reads`, `Log`, `Doc`, `StoreError`, `Objects`, `Frontier` |
| signatures | believed, chain_set, devices, epoch, signatures/\*, sync; `Tap` | `Trust`, `Enrolment`, `ChainSet`, `Tap` |
| todo | authoring, render, render_cache, decisions/\*, priced, reading, release, rendering, template, todos, verb, view | `Authored`, `Fulfillment`, `Decisions`, `Released` |
| held | attachment, pdf_text, upload_index, enclosed, mail_error, excerpt, file_read, for_model, held_file, held_value, leak, redact, s/{attached, attachment_source, upload_standing}, upload, vault | `HeldFile`, `AttachmentId`, `Redacted`, `Enclosure` |
| memory | s/{carried, erasure, fact_life, key_ring, line_words, locked_text, memory, memory_event, opened, provenance, salted_text, scrubbed_under, slots, texts, two_phase, words} | `Words`, `Fact`, `KeyName`, `Erasure` |
| mail | mail, mail/{mail_sync, mailbox, account, account_email, address, cache, declared, dedup_key, folders, grants, hash_prefix, idle, imap, mailroom, oauth, outbox, puller, removal, send, sync_now, transport, verbs} | `MailMessage`, `MailAccount`, `EmailAddress`, `Consent`, `mail::Fault` |
| health | capability/limited, connect/{failure, label, lacking, refused}, health, plugin/{credential, name, origin, refusal, said}, source_key | `Failure`, `Remedy`, `Healths`, `SourceKey`, `PluginName` |
| triage | mail/{corpus, junk_folder, older_mail, heartbeat, gone, junk, known, local_state, older, state, triage, watch}, rule | `Corpus`, `TriageState`, `Rule` |
| services | repositories, repository, drive (with `effect`), plugin, plugin/{bundle_path, fetch, item, manifest, run, url}, repos, usage, weather (with `effect`), web (with `effect`) | `DrivePath`, `RepoName`, `Manifest`, `Allowance` |
| google | calendar/\* (with `effect`), google/\* | `Occurrence`, `Scope`, `Held` |
| connect | secrets, conformance (but not the model's laws), connect and its other submodules, deployment, settings (but not overview) | `Connection`, `Standing`, `Secrets`, `Settable` |
| inbox | coding, s/{act_vocabulary, acts, card, diff, proposal, changes, decide, inbox, inbox_event, inbox_json, inbox_reading, inbox_registers, own_round, phase, proposer, responded, sending, sign_in, stamp, tap, tapped}; `Proposal`, `Reasons`, `Style` | `Act`, `Fold`, `ProposalId`, `InboxEvent`, `Proposal` |
| conversations | s/{about, answer, found, model (with `model/laws`), model_sessions, outcome, queue, search, store, taint, tool_name, turn_shape, verb_name} | `Entry`, `Stored`, `Kind`, `ConversationState` |
| lines | s/{act_lines, conversation_json, conversations, held_lines, held_replica, holder, line_event, line_form, line_shape, object_stores, outline, outlines, rescrub, step, targets, transcript} | `Conversations`, `LineEvent`, `HeldReplica`, `LineReplica` |
| topics | s/{answers, readings, rules, stream, title, topics} | `Topic`, `Target`, `Dress`, `Window` |
| digest | s/{caused, compact, compact_\*, compaction, digest\*, indexed_tree, memories, recollection, shown_record, told} | `Digest`, `DigestEvent`, `Memories` |
| cards | notify (with `effect`), s/{afresh, bootstrap, build, coding, connections, done_elsewhere, failing, filed, fixes, google_sign_in, mail_health, merged_patches, noticed, request, undo}, settings/overview | `Filing`, `Noticed`, `Notify` |
| secretary | agent (with `record`), conversation, s/{answering, briefings, record, briefs, call, daily, file_query, file_search, gate, live_shape, token_expiry, opening, read_holders, review, surface, sweep, verb_answer, verbs}, secretary (mod) | `Agent`, `Secretary`, `Model` (with their `compile_fail` tests) |
| api | server, clienterror, gate, portal, query, routes, s/routes, stores (`Kind`, `Store`, `Database`), wire/\* | the route table, 75 wire types |

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
| 9 | C2: the wire split by the table above; generator unchanged | ~93 types out of `wire.rs` (2,795 lines) |
| 10 | Pin the layers until the crates exist: a guard reads the layer table from `context.toml` and refuses any `crate::` path that points up | one guard |

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

After PR 31, rerun the table. Stage 4 is the remedies of the over-budget section, ordered by
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
