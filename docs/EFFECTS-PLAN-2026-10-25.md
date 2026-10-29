# Effects: a pure store, its inputs and its outputs (2026-10-25)

The owner, 2026-10-24, after the plugin platform became one function and its
grant (PR #243): does mail fit the same shape, and could "all of them" go into
notmuch? Then: "If it simplifies code and reuses structure I'm all for it,
especially if it makes things more general and reusable." And on 2026-10-25,
widening the frame: "some things are inputs and some things are outputs, they
kinda like form the effects that our agent can perform over the pure
datastructure we have, notice the high level bird's eye view thinking".

A design; nothing is built. It began as the sources plan
(`docs/SOURCES-PLAN-2026-10-24.md`, renamed to this file), which is now its
input half. It recommends ONE shape:

- **§0, the bird's-eye view.** The box is a pure store and its readings, an
  agent that reads them and proposes, and the effects at its edge: inputs
  through one `Source` port and outputs through one `Sink` port.
- **§1 to §5, inputs.** Every source is a value of one port: pull from a
  cursor, yield items or a failure of a class. The runner keeps the cursor,
  the last run and the health. Each item a non-mail source brings is written
  as an RFC 5322 message under `<mail root>/sources/<source>/`, which notmuch
  indexes beside the mail. The plugin contract stays as PR #243 leaves it:
  one `sync` function and its grant.
- **§9, outputs.** Every proposal the owner taps is a write to a store or an
  output to the world, and that split becomes a type. Every output goes
  through one `Sink` port and one performer, at most once by its object's
  name, and is confirmed by the input that brings its effect back.
- **§10** holds the reviews of the whole design.

Measured on master at `317c09b`. PR #243 (open, head `1fda5cd`) rewrites the
plugin files. Where this plan builds on it, it names its head. Line counts are
`scripts/size.sh`'s rule: each Rust file up to its first `#[cfg(test)]`,
blank and comment lines not counted.

## 0. The bird's-eye view

Four things exist, and nothing else:

| Part | What it is | Its law |
|---|---|---|
| **the store** | prodromes (sets of content-named objects) and the box's other stores, each one row of one list, and the READINGS, which are folds over them | a reading is a function of the set of objects it reads |
| **the agent** | the secretary, the triage agent and the box's rules: they read readings and PROPOSE | it holds no `Tap`, so it performs no output |
| **inputs** | sources, one port: each pull adds objects or messages to the store | monotone: what is learned is added, so a pull needs no lock |
| **outputs** | sinks, one port: each performs one tapped proposal on the world | non-monotone, so it is performed once, under the inbox's lock, after a tap |

Every component of today's code sits in exactly one part:

| Part | Component | Evidence |
|---|---|---|
| store | every store, declared once as a row (chains, inbox, lines, memory, digests, decisions, topics, plugin caches, the box-local tier) | `app/core/src/store/kind.rs:119-148` |
| store | the todo readings: one `view` per instant, the chains read as one set | `app/core/src/believed.rs:25`, `app/core/src/chain_set.rs:299` |
| store | the inbox's reading, the calendar's, the Settings overview, the release | `app/core/src/secretary/inbox_reading.rs`, `app/core/src/calendar/reading.rs:15`, `app/core/src/settings/overview.rs:478`, `suzatary_core::release` |
| store | the mail index: the maildir and notmuch, a cache-tier store read through `Mailbox` | `app/io/src/mailbox.rs:1-5` |
| store | the instance file, a store written by compare-and-swap under one `flock` | `app/core/src/settings/configure.rs:368` (`Edits::swap`) |
| agent | the secretary's verbs and the triage agent's verbs | `app/core/src/secretary/verbs/`, `app/core/src/agent/` |
| agent | the box's rules that file cards with no model: secrets, connections, failures, the mail-health todo | `app/core/src/secretary/noticed.rs:284`, `fixes.rs:61`, `connections.rs:43`, `failing.rs:270,317`, `mail_health.rs:75` |
| agent | the open-list sweep and the review | `app/core/src/secretary/sweep/candidates.rs:125` |
| agent | the plugin actor world: it returns proposals and never performs (#243 keeps its WIT, unhosted) | `app/wit/plugin.wit` on master, `app/wit/actor.wit` on #243 |
| inputs | the mail pullers, the syncer and IMAP IDLE | `app/io/src/pullers.rs`, `app/io/src/sync.rs:79-147`, `app/core/src/mail/idle.rs` |
| inputs | the older-mail search | `app/core/src/mail/older.rs` |
| inputs | the plugin host's `sync`, and the Google calendar | `app/plugin/`, `app/cli/src/commands/google.rs:12` |
| inputs | the Drive's listings and probe, and the owner's uploads | `app/core/src/drive/port.rs:17`, `drive/probe.rs:16`, `app/io/src/upload_index.rs` |
| inputs | the owner's devices, whose objects the door admits | `app/cli/src/commands/serve/table.rs:26,76` |
| inputs | the coding poll, which reads `claude/` branches | `app/cli/src/commands/coding.rs:150`, `app/core/src/coding.rs:444` |
| outputs | the tap's one performer, under the inbox's hold | `app/core/src/secretary/routes.rs:1049-1130`, `app/core/src/secretary/tapped.rs:1-6`, `inbox::submit` `app/core/src/secretary/inbox.rs:823` |
| outputs | sending mail: `Post` and the one transport file | `app/core/src/secretary/routes.rs:34-38`, `app/cli/src/commands/secretary.rs:42`, `app/cli/src/commands/mail.rs:428` |
| outputs | the act performer: coding sessions, patches, calendar events, tapped reads, sign-ins | `app/cli/src/commands/secretary.rs:384-520` |
| outputs | the Google calendar insert, the vault's keep, push notifications | `app/io/src/google.rs:178`, `app/core/src/vault.rs:44`, `app/core/src/capability/notify.rs:206` |

The operator's shell (`scripts/`, `nix/`) builds and deploys the app and is
not part of its run time. The models the agent runs on belong to the agent:
a call to a model is how the agent computes, and their health register
(`app/io/src/model_health.rs`) stays its own.

Two components look like they cross the line, and do not:

- The secretary writes a changeset to a chain at once, as `assistant`
  (CLAUDE.md, "The agent's writes do not reach a chain; they are cards").
  That is a write to the store that the store's own trust policy admits,
  not an output.
- The agent asks for inputs: `sync_mail`, `search_older`, an allowed web or
  Drive read. A pull only adds to the store, so asking for one performs
  nothing on the world (PLUGINS-PLAN §3).

## 1. The duplication today

Five sources pull: the mail accounts, the ICS calendar plugin, a Google
account's calendars, the Drive (it is probed, not pulled), and the uploads
(the owner pushes them, and the upload index reads them). Each source is
built separately in five ways.

### 1.1 Health: one semilattice, four places to keep it

The value is already shared. `suzatary_core::mail::health` is a semilattice
keyed by any string (`app/core/src/mail/health.rs:1-10`, `Healths` at `:188`,
`Healths::one` at `:194`). What is not shared is where it is kept and how it
is read:

| Source | Where its health is kept | Evidence | Lines |
|---|---|---|---|
| mail accounts | `mail_health::Health`, a Doc beside the heartbeat | `app/io/src/mail_health.rs:1-22`, written by `app/io/src/mail_sync.rs:32-45` | 20 |
| Drive | `drive_health::Health`, a second Doc of the same type | `app/io/src/drive_health.rs:1-22`, probed by `app/core/src/drive/probe.rs:16-25` | 18 + 39 |
| a plugin's links | the `links` field of each plugin's `ledger.json` | `app/core/src/plugin/ledger.rs:22-23`; on #243 `:22-23` of the same file | part of 80 |
| Google calendars | **nowhere**: a failure is one journal line | `app/cli/src/commands/google.rs:46-55` | 0 |

The server then holds two Docs of one type side by side (`answered` and
`probes`, `app/cli/src/commands/serve/mod.rs:206-212`, wired at `:1225`), and
reads the plugin links by opening every ledger
(`app/cli/src/commands/serve/mod.rs:868-890`, 19 lines). Model health
(`app/io/src/model_health.rs`) and the data repository's pushes
(`overview.rs:259`) use the same type. They are not sources, and they stay.

### 1.2 Cards: one rule, a lookup per source

`secretary::failing` already generalises the card to every connection
(`app/core/src/secretary/failing.rs:1-16`). But its input names each source
separately (`Observed.mail` and `Observed.links`, `failing.rs:112-114`), and
so does every reader of it:

- `failing()` matches on the connection's table to find its health: a
  `Link` searches the plugin map by name, an `Account` reads the mail map,
  and `Folder` (the Drive) reads nothing (`failing.rs:270-292`, 42 lines). So
  a failing Drive files no card, and a failing Google calendar cannot, since
  it has no health.
- The removal offers are two rules: a mail account failing for
  `ABANDONED` days (`failing.rs:371-404`) and a calendar quiet for `QUIET`
  days (`quiet_calendar`, `failing.rs:424-470`, 46 lines), each with its own
  table match and its own health lookup.
- `noticed::Seen` carries the same two fields again (`app/core/src/secretary/noticed.rs:255-262`).
- A mail account has a second, mail-only rule: failing for a day proposes a
  todo on the project chain (`app/core/src/secretary/mail_health.rs:1-8`,
  `propose_persistent` at `:75`, 114 lines, called from
  `app/cli/src/commands/secretary.rs:1757`). The fix card from `failing` is
  filed for the same failure.

### 1.3 Ledgers and cursors: three runners

| Source | Runner | Cursor | Last run | Evidence |
|---|---|---|---|---|
| mail | `Syncer::sync`, one puller per account, one `notmuch new` | each puller's own state files | `last-sync.json` | `app/io/src/sync.rs:79-147` (57 lines), `app/io/src/pullers.rs` (147) |
| plugins | `suzatary plugin sync` | `cursor` beside the store | `ledger.json`'s `last` | master `app/cli/src/commands/plugin.rs:235,376,812`; #243 `app/cli/src/commands/plugin/mod.rs:162-240,348` |
| Google calendars | rides the plugin timer | none: reads the whole window each time | none | `app/cli/src/commands/plugin.rs:110-126`, `app/cli/src/commands/google.rs:12-59` (53) |

The mail port is the only one with conformance laws (`Puller`,
`app/core/src/mail/puller.rs:10-16`; its laws `Lands`, `Once` and `Fails`,
`app/core/src/conformance/puller.rs:11-20`). The plugin has laws of its own,
and the Google calendar has none.

### 1.4 Settings rows: one function per source

`settings::overview` draws each source by its own function, though each one
draws the same thing: a label, a detail, the connection's standing joined
with its health, and its cards.

| Function | Evidence | Lines |
|---|---|---|
| `mail()` | `app/core/src/settings/overview.rs:580-629` | 53 |
| `plugins()`, `last_run()` | `overview.rs:635-724` | 81 |
| `drive()` | `overview.rs:726-766` | 38 |
| `Store`, `Seen`, `Recorded` (a plugin's cache tier as Settings reads it) | `overview.rs:153-203` | 33 |
| `Observed`'s per-source fields `mail`, `plugins`, `drive` | `overview.rs:250-257` | in the above |

### 1.5 Readers: one per source, and two search backends

| Reader | Evidence | Lines |
|---|---|---|
| mail: notmuch through `Mailbox` | `app/io/src/mailbox.rs` | 747 (stays) |
| the calendar: a prodrome fold over every plugin store with the calendar's registers | `app/io/src/calendar.rs:11-45`, `app/core/src/calendar/reading.rs` | 34 + 132 |
| plugin stores themselves | `app/io/src/plugin_store.rs`; the store row `app/core/src/store/kind.rs:143`; `Database.plugins` `app/core/src/store/database.rs:55,109` | 62 (#243) + rows |
| learning into them | `store::learnt`, `store::knowledge` (#243; `plugin::` on master) | 22 + 53 |
| Google calendars into the same store | `app/io/src/google.rs:396` `learn` | 29 |
| the upload index | `app/io/src/upload_index.rs` (130), `capability::UploadIndex` `app/core/src/capability/upload_index.rs:11-29` (22), `secretary::file_search` (121), `secretary::file_query` (145), `secretary::upload_standing` (47) | 465 |
| the `search` verb's file half, which merges two backends | `app/core/src/secretary/verbs/search.rs:13-40,69-95` | 55 |

### 1.6 Total

About **1,500 production lines** are spent building the same five things once
per source. That is health storage and lookups (≈ 150), the second mail
health rule (114 + its call), runners and ledgers (≈ 280), Settings rows
(205), card lookups and removal rules (≈ 125), and per-source readers and
stores (≈ 830, of which the upload index is 465). With the outputs
of §9, the stages in §6 remove about 1,735 lines and add about 965 (a few of
each are the migration verbs that one stage adds and the next deletes), net
≈ −770. Each stage removes more than it adds, except the deferred Drive
stage.

## 2. One source interface

### 2.1 The port

```rust
/// A source: something outside the box that the box reads from where it last stopped.
pub trait Source {
    /// Pull from `cursor` (none on the first pull, or after the source's messages were dropped).
    fn pull(&self, cursor: Option<&[u8]>, into: &mut dyn Deliver) -> Result<Pulled, Unpulled>;
}

/// What one pull came to: the cursor the next pull starts from, and its line for the journal.
pub struct Pulled { pub cursor: Vec<u8>, pub said: String }

/// Where a pull's items go: the one writer of `sources/` (§3).
pub trait Deliver {
    fn item(&mut self, item: Item) -> Result<(), Undelivered>;
    fn gone(&mut self, key: &ItemKey) -> Result<(), Undelivered>;
}
```

`Unpulled` is today's (`app/core/src/mail/puller.rs:18-26`): a `Failure`
class, parsed where the failure is observed, and the words the report shows.
`Puller` becomes `Source`. The mail pullers ignore `into`, since lieer,
mbsync and the IMAP pull land RFC 5322 files in the account's maildir
themselves (`pullers.rs:145-158`). They return an empty cursor, because
their state is their own files. The laws in `conformance::puller` (`Lands`,
`Once`, `Fails`) become the laws of every source: what a pull lands is
what the outside holds, once; a second pull with nothing new changes
nothing; an unreachable outside fails in its class and never succeeds having
landed nothing.

A source names itself by a `SourceKey`, a closed sum with one print and one
parse:

```rust
pub enum SourceKey {
    Mail(EmailAddress),                    // mail:ada@example.test
    Link { plugin: PluginName, credential: CredentialName }, // link:ics/PLUGIN_ICS_WORK
    GoogleCalendar(EmailAddress),          // google-calendar:ada@example.test
    Uploads,                               // uploads
    Drive,                                 // drive
}
```

`connect::Connection` maps to its `SourceKey`s by one function. That
function replaces each table match in §1.2.

### 2.2 What the runner keeps, once, for every source

- **One health register**: one `Doc<(), Healths>` keyed by the `SourceKey`
  print. It replaces the three in §1.1, and the Google calendar gains one.
  A plugin's per-link results are keyed `link:<plugin>/<credential>`, so the
  link lookup in `failing.rs:181-188` becomes one `health(&key)`.
- **One ledger per source**: the last run (`LastRun`, `Came` from the plugin
  ledger, `ledger.rs:26-50` on #243) and the cursor, box-local under
  `cache/sources/<key>/`. The mail's `last-sync.json` and the plugin's
  `ledger.json` and `cursor` become this one shape.
- **One runner**: `suzatary sources sync` runs each source that is due, under
  the one hold the mail sync takes today (`app/io/src/sync.rs:83-89`), then
  `notmuch new` once, then the inbox tagging (`sync.rs:98-110`). Each source
  kind has an interval: mail every 5 minutes, as now; a plugin, the Google
  calendar and the Drive every 15. The IDLE watcher wakes this runner as it
  wakes the mail timer now. One runner is also a notmuch requirement (§3.7):
  there must be one writer.

### 2.3 Two kinds of implementation

| Kind | Who | Authority | Items |
|---|---|---|---|
| native, trusted | the mail pullers, the Google calendar, the uploads, the Drive | sockets, processes, the box's grants | the source builds each item from what it read |
| sandboxed | the plugin host, one `Source` value per `[[plugins]]` | `fetch`, GET only, to the origins the manifest allows; the credential named, never held (#243's `app/wit/plugin.wit:11-34`) | `sync(cursor, credentials) -> (items, cursor)` (#243's `plugin.wit:36-52`), each item admitted at the declared schema by `plugin::admit` |

The plugin's adapter is the only code that knows a plugin exists. It calls
the host, admits each item, delivers it, and turns each fetch's `Answer` into
a health result keyed by its link (`Answer::outcome`, `ledger.rs:61-80` on
#243). The WIT does not change. A plugin cannot say an item is gone: it
reports the item again with a status of its schema, as the ICS plugin
reports a cancelled occurrence today. Only a native source calls `gone`.

## 3. Everything as a message

### 3.1 The shape

Every item of a declared schema is written as one message:

```
Message-ID: <{first 32 hex of sha256(source key, item key)}@{kind}.sources.invalid>
Date: {the item's own instant: an event's start, an upload's arrival, a file's modification}
From: "{the source as the owner calls it}" <{kind}@sources.invalid>
Subject: {the item's title}
X-Suzatary-Source: {the SourceKey's print}
X-Suzatary-Item: {the item key}
MIME-Version: 1.0
Content-Type: multipart/mixed

  text/plain; charset=utf-8   each field of the item as "name: value" lines, and a file's text
  application/x-suzatary-item the item's literal, exactly as it was admitted
```

- `.invalid` is reserved (RFC 2606), so no real message can share a
  Message-ID with an item.
- The text part is printed from the declared schema's fields by ONE function
  for every schema. No per-source renderer is written. The uploads' and the
  Google calendar's items are events of declared schemas too: the Google
  calendar already writes the calendar's schema (`google/calendar.rs:15-19`).
- The literal part is the record. A reader that needs structure (the
  calendar's week) parses it with the schema's own parser, which is the one
  reading that exists today. notmuch does not index `application/*` parts,
  so the literal adds no search terms.
- No `text/calendar` part is generated, although invites carry one. It would
  be a second spelling of the occurrence, and printing iCalendar is code
  that nothing here reads back. notmuch would also index its raw lines
  (`DTSTART:20261024T…`) as terms. The text part already says the title,
  the time and the place.
- The file is `sources/<kind>/<key slug>/cur/<item hash>.<content hash>:2,S`.
  The `S` flag means the message is read, so `tag:unread` never matches it.
- One maildir folder per source: `sources/<kind>/<key slug>/`. Removing a
  source deletes its folder, as removing an account deletes its maildir
  (`app/cli/src/commands/serve/mail_cache.rs:1-25`). `Roster::new` refuses
  an account whose `subdir` equals or contains `sources/`, or lies inside it
  (`app/core/src/mail/account.rs:286` is where it refuses two accounts
  claiming one folder). §3.6 says why.

### 3.2 Updates and deletions

notmuch notices a file by its NAME, when it scans a directory whose mtime
moved. It does not reread a file whose name is unchanged. So the content
hash is in the name, and a changed item is a new file:

1. write the new file into `tmp/`, then rename it into `cur/`;
2. unlink every other file in that folder whose name begins with the same
   item hash;
3. `notmuch new` (once, at the end of the run) adds the new file and drops
   the old one. The Message-ID stays the same, so the message keeps its
   identity, its tags and every handle that names it.

If the run dies between 1 and 2, two files share one Message-ID. notmuch
treats them as one message with two files, and the next pull's step 2
removes the stale one. An unchanged item has the same content hash, so it
writes nothing. That holds even for the Google calendar, which reads its
whole window each time (`google/calendar.rs:33-37`). A `gone` unlinks the
item's files. A cancelled occurrence is not gone: it is a new version whose
status is cancelled, so the week can say so, as it does now.

### 3.3 Threading

notmuch threads by `References` and `In-Reply-To` only. Items carry neither,
so each item is a thread of its own. An invite email and its calendar
occurrence stay two threads. Linking them (a `References:` naming the
invite's Message-ID) is possible later, and nothing here needs it.

### 3.4 What the mail-only paths must skip

Every path that means mail says so in a type: `Query::mail(q)` is
`(q) and not path:sources/**`, its one constructor. Tags are not used to
keep items out, because tags are not the truth (CLAUDE.md, Evidence) and the
box's `new.tags` lives in the data repository's notmuch config.

`new.tags` may tag every new file, items included, for example with
`inbox`. That is harmless only because every inbox reader goes through
`Query::mail`. So stage 5's laws include one: an item tagged `inbox` never
reaches the inbox page, the triage queue, a watch or the sweep.

| Path | Today | After |
|---|---|---|
| the triage queue | `tag:inbox and date:14d.. and path:<account>/**` (`app/io/src/mailbox.rs:359`) | unchanged: already scoped to an account |
| the inbox page | `tag:inbox`, unscoped with no account (`mailbox.rs:724-727`) | `Query::mail`: if the box's `new.tags` includes `inbox`, items would show here |
| mail watches | `from:`/`subject:` with the owner's own accounts left out (`app/core/src/mail/watch.rs:319-334`), rung at `:455` | `Query::mail`: a meeting titled like the awaited subject must not ring a watch |
| the open-list sweep | `thread:`, `from:` and referent terms, unscoped (`app/core/src/secretary/sweep/candidates.rs:125-142`) | `Query::mail`: a calendar entry "BA123" is not evidence that a flight was taken |
| sent-mail checks | `folder:Sent` (`app/core/src/secretary/verbs/sync_mail.rs:31`) | unchanged: no source folder is named `Sent` |
| unread | `tag:unread` | unchanged: items land with the `S` flag |
| older mail | lands read in `<subdir>/older-inbox`, outside the `+inbox` tagging (`app/io/src/older.rs:6-14`) | unchanged: under an account's path |
| dedup | `state.json` keys the queue alone | unchanged |
| junk sweep, account removal | per account | unchanged |
| `search` (the secretary's and the phone's) | unscoped | **everything**: this is the point |

### 3.5 Held values

A `Handle` names a Message-ID and a value's place in that message
(`held_value::Place`, the id at most `held_value::MAX_MESSAGE` = 998 bytes). An
item's Message-ID is under 70 bytes. So the masker that every model
read goes through, the reveal and the send fill work on items unchanged.
Today a number in an item has no handle of its own, since no message names
it, so a model reads it through `held_value::withheld`, which can mask it and
cannot reveal it. As a message, it can be revealed and sent on a tap
like a number in mail.

Since held values stage 3 a handle names a holder, a message or a file
(`held_value::Holder`), and a file is named by its source (`held_value::HeldFile`:
a message's attachment, a Drive path or an upload). An
upload's text is masked with handles that name the upload
(`held:file:<FromUpload>.<n>`), and the upload itself is a held value of kind
`file` (`held:file:<FromUpload>.file`). The reveal and the send read an upload
by its id from the upload store, never from the maildir. So stage 5 needs no second reveal path for an
upload, with one condition: the mail reader masks a message's text as that
message's and withholds a placeholder of any other holder. An upload's item
must therefore be masked as its upload (`Holder::File`), or its placeholders
read as withheld. Stage 5 adds that, and a test that an upload item's
placeholder still reveals.

### 3.6 Uploads, Drive, privacy and size

- **The upload index is deleted.** It is a cache: "deleting it loses
  nothing" (`app/io/src/upload_index.rs:1-3`). The uploads source pulls the
  set of uploads that a live turn names, which is the set `refresh(named)`
  reads today (`capability/upload_index.rs:11-21`). It delivers each one's
  words as the index holds them: a file on the box as its text layer, a
  private one as its masked note, and a file whose text names an identity
  number as `FileRead::of` withholds it. An upload no live turn names any
  more is `gone`. `upload_read` keeps opening the bytes by id from the
  upload store, so the bytes never enter the maildir.
- **Drive** (stage 7, deferred): the allowed folders' files as items, the
  text layer indexed and capped, and the bytes left in Drive, opened by
  `drive_read` as now. The private folder is never listed
  (`app/core/src/drive/fenced.rs:26-30`), so it is never indexed either.
  Its probe (`drive/probe.rs`) becomes this source's pull.
- **Privacy.** `sources/` is under the mail root: box-local, never pushed,
  never in the vault, as mail is (CLAUDE.md, Operations). That is the
  calendar's tier today (cache) and the upload index's. Taint does not
  change: `search` already answers with attacker-controlled mail, so it is
  already Tainted, and `context` (Clean) reads no notmuch.
- **No synced root holds `sources/`.** A puller that mirrors local folders
  up to its server would upload every item. Two examples are mbsync with
  `Create Far` or `Create Both` over a broad `Patterns`, and a lieer
  repository whose root is above `sources/`. The law: the source root and
  every account's root are disjoint. `Roster::new` refuses an account
  `subdir` that equals or contains `sources/` or lies inside it (§3.1), and
  stage 5 adds a guard test that checks the source root against every
  account's root.
- **Erasure.** Today a forgotten conversation's upload entry is a JSON file
  that is deleted. In notmuch, unlinking a file and running `notmuch new`
  removes the document, but Xapian can keep the old terms in free blocks
  until the database is compacted. So the sweep that erases a conversation's
  words runs `notmuch compact` after removing an upload's message, at most
  once a day (§4.2 says how its cost is measured first).
- **Size.** The text part is capped at `MAX_BODY_CHARS` (200 000,
  `app/core/src/mail.rs:127`), and the cut is declared in the text, as mail
  bodies are. The number of items is bounded where each source reads: the
  Google calendar reads at most 20 calendars × 250 events
  (`google/calendar.rs:24-28`), and the ICS expansion is bounded (#243,
  commit `59d20c4`). That is at most a few thousand items a year against the
  54,861 messages notmuch holds today (CLAUDE.md).

### 3.7 Rebuilding

The maildir is the cache tier for every source, and the plugin stores go.
Deleting `sources/<kind>/<key>/` and that source's cursor makes the next pull
deliver everything again, as deleting a plugin store does today. notmuch's
database is rebuilt from the files, as it is for mail.

## 4. The reviews this was held to

### 4.1 Rich Hickey: one idea, or two complected?

"A source yields messages" puts two ideas into one sentence. The plan keeps
them apart:

- **Running is not data.** `Source::pull` returns items or a failure class
  and nothing else. The cursor, the last run, the health, the card and the
  Settings line are the runner's, computed from what pulls returned. No
  source writes its own health, so no source can report itself well.
- **Identity is not state.** The Message-ID is the item's identity, and each
  file is one value of it. A changed event is a new value under the same
  identity, and the old file is let go. Nothing is edited in place.
- **The record is not the index.** The message carries the literal (the
  record) and a text projection of it (for the index). The projection is
  one function of the declared schema, so the two cannot drift. Readers that
  need structure parse the literal, never the text.

What it does complect: one index now holds several kinds, so a mail-only
path must say "mail". That is why `Query::mail` is a constructor and not a
path filter each caller remembers. The `Date` header is also one chosen
instant (an event's start) for an item that may have several. It is a sort
key only, and the item's own instants stay in the literal.

### 4.2 notmuch's author: where will notmuch surprise us?

- **Files, not contents.** `notmuch new` sees renames, additions and
  removals by scanning directories whose mtime moved, and never rereads a
  file under the same name. Hence the content hash in every file name
  (§3.2). Writing a new version in place would leave the old version
  indexed.
- **One Message-ID, several files.** notmuch indexes the terms of every
  file of a message, and `show` shows one of them. A run cut short is
  therefore searchable under both versions until the next pull removes the
  stale file. That is acceptable, and it is why step 2 runs on every pull.
- **One writer.** Xapian takes a write lock. Two `notmuch new` at once (the
  5-minute mail timer and the 15-minute plugin timer today) would make one
  fail. One runner under one hold removes the race (§2.2).
- **Future dates.** A meeting next week has a `Date` in the future. Newest
  first, it sorts above all mail. `date:` ranges read naturally
  (`date:today..7d` is the coming week). The inbox and the queue are
  scoped to mail, so they never see it.
- **No Message-ID would be worse.** Without one, notmuch makes one from a
  hash of the file, so every version of an item would be a new message.
  The writer always sets one.
- **Volume is not the problem.** A few thousand items against ≈ 55,000
  messages is under 10%. The costs that matter are the per-run directory
  scan (one folder per source) and churn. Writing only on a content change
  keeps the churn at what actually changed.
- **Deleted is not erased.** Removed documents can survive in Xapian's free
  blocks until `notmuch compact`. §3.6 runs it after an erasure. The stage
  that adds it measures its time on a copy of the live database's size
  before the daily sweep calls it.
- **Text parts are indexed and others are not.** `text/*` is indexed (HTML
  with its tags stripped), and `application/*` is not, which is the split
  §3.1 relies on.

## 5. The alternative: one generic index beside notmuch

A second index (SQLite FTS5 or tantivy) for everything that is not mail,
with notmuch left as it is.

| | Everything in notmuch | A second index |
|---|---|---|
| code removed | the upload index (465), the plugin stores and their learning (≈ 170), the calendar's store reader, the `search` verb's merge of two backends (55) | the upload index's storage only. The merge stays and grows: two ranked lists, two query languages |
| code added | the writer (≈ 150) and `Query::mail` (≈ 25) | an index crate, its schema, its updates and deletions, its query parser, its rebuild (≈ 400 or more) |
| search quality | one query language, one ranking; `from:`, `date:`, `subject:`, phrases, prefixes and `thread:` all work on items | fields can be typed (an event's start as a real range), but every search is two searches merged by hand |
| held values | unchanged: a handle names a Message-ID or a file's source (held values stage 3) | a second reveal path for a second kind of handle |
| failure modes | one lock, one rebuild, one place to be wrong; mail paths must exclude `sources/` | two indexes that can disagree; two rebuilds; a crate's on-disk format to migrate |
| simpler | the phone's search shows everything with no new route; one masker; one reader | mail-only paths need no care |
| harder | structured queries over items (a week) parse literals rather than query fields | everything that reads both |

**Recommendation: everything in notmuch.** It removes more, adds less, and
keeps one reading. The only thing the second index does better is typed
queries over items, and the one reader that needs them (the calendar's week)
reads a few hundred literals out of one folder.

## 6. Stages

Each stage is one PR, gated in full, and leaves the app working. Input and
output stages alternate wherever an output stage needs an input stage's
piece. Line counts are estimates from §1's and §9.1's measurements. Each PR
measures its own with `scripts/size.sh`. Stages 1 to 4 change no stored item
and can land in any order after #243. Stages 5 and 6 need stage 2's runner.

### Stage 1 (both): one health register

- **General piece:** `Endpoint`, a sum of `In(SourceKey)` (§2.1) and
  `Out(SinkKey)` (§9.3) with one print, and one `Doc<(), Healths>` keyed by
  it, `cache/endpoints/health.json`. The register holds both sides of one
  connection apart: a mail account's reading and its sending are two keys,
  as their grants are two slots.
- **Removes:** `app/io/src/mail_health.rs` (20), `app/io/src/drive_health.rs`
  (18), the ledger's `links` and the server's `links()` that opens every
  ledger (`serve/mod.rs:868-890`, 19), and the `probes` Doc beside
  `answered` (`serve/mod.rs:210-212,1225`). `failing::Observed.{mail, links}`,
  `noticed::Seen.{mail, links}` and `overview::Observed.{mail, drive}` become
  one field each, and the four table-match lookups become `health(&key)`.
  The Google calendar's sync records health, so it can now fail on Settings
  and file its card.
- **Migration:** `suzatary endpoints migrate-health` joins the three old Docs
  into the new one, renaming each key to its `Endpoint`. The join is a
  semilattice, so running it twice is running it once. A failure's episode
  (its `since`) survives the move, so no card is filed twice. Stage 2
  deletes the verb and the old files' readers.
- **Lines:** about −130, +85 (35 of them the verb). **Net ≈ −45.**
- **The phone:** no change, except that a Google calendar that fails says so
  on its Settings line and gets its card.

Stage 1 BUILT (2026-10-27): `suzatary_core::source_key::SourceKey` (`mail:`,
`plugin:`, `link:`, `google-calendar:`, `drive`) keys one
`crate::health::Healths<SourceKey>` in `source-health.json` beside the
heartbeat (`suzatary_io::source_health`), not under `cache/`, because a
failure's `since` cannot be rebuilt by syncing again. It is a
`suzatary_io::docs::Locked` document, the read-change-write under a lock
that the pairing file uses too, so writers that join at once lose nothing.
Every reader reads it through the secretary's record (`health::Register`,
read-only). The Drive's probe rides the source timer (`suzatary plugin
sync`) beside the plugins and the Google calendars, so the server writes
nothing to it. `Endpoint` waits for stage 3, which adds the first `SinkKey`.
`connect::Connection::sources` maps a connection to its keys; a plugin's own
lines read `Manifest::ran` and `Manifest::link`, the keys its runs are
recorded under. A plugin's run is a health result under `plugin:<name>`, so
the plugin ledger went whole, and stage 2's ledger holds only the cursor.
`suzatary sources migrate-health` carries the three old registers, folding a
mail key written before addresses were folded, and `verify` refuses while
the old ones hold a failure the new one lacks. `plugin migrate`, which ran
in the 2.1.21 window, is deleted. Measured with `scripts/size.sh`: −31
production lines before the merge of 2.1.28.

### Stage 2 (inputs): one source port and one runner

- **General piece:** `Source`, `Pulled`, `Deliver` (with no writer yet:
  stage 5 adds it); `conformance::puller` becomes `conformance::source`,
  run against every implementation; one ledger per source; `suzatary sources
  sync`, one timer in `nix/services.nix` in place of the mail and plugin
  timers.
- **Removes:** `commands/google.rs`'s `sync_calendars` (53) as a separate
  runner, which becomes a `Source`; the plugin runner's loop and ledger
  plumbing (≈ 110 of #243's `plugin/mod.rs`, 281), which becomes the plugin
  adapter; `Syncer::sync`'s loop (57), which moves into the runner; the
  stage-1 migration verb (35).
- **Migration:** the plugin `cursor` moves to `cache/sources/<key>/`, read
  once by `suzatary sources migrate-ledgers` and deleted in stage 4 (stage 1
  removed the ledger). Mail's per-puller state files do not move.
- **Lines:** about −295, +225. **Net ≈ −70.**
- **The phone:** no change. A calendar refreshes on the runner's cadence.

### Stage 3 (outputs): the split as a type, one sink port and one performer

- **General piece:** `Proposal = Write | Output` (§9.2); one `Sink` port and
  a table of native sinks (§9.3); one performer, `inbox::submit` calling
  the store's writer for a `Write` and the sink for an `Output`.
  `Unperformed` is the one failure (`NotSent` or `MayHaveGone`, each with
  its `Failure` class), and every sink's outcome joins the stage-1 register
  under `Out(SinkKey)`.
- **The stored literals do not move.** Each act prints with the
  constructor it prints with today, so no inbox object changes and no
  migration runs. `app/core/tests/act_literal.rs` already reads every arm
  back from its print, and is the pin.
- **Removes:** `Acting::begin`'s chain of `if let` per act, each one
  repeating the `NotYet`, `Real` and `Recording` arms
  (`app/cli/src/commands/secretary.rs:384-520`, ≈ 80 of 128): a build
  without a sink has no table entry, and the fixture's recording is a table
  of its own. The output arms of `routes::performed`
  (`app/core/src/secretary/routes.rs:1049-1130`, ≈ 40 of 84) become one
  `Prepared::Output` arm.
- **Lines:** about −120, +85. **Net ≈ −35.**
- **The phone:** no change. Every card reads and taps as it does now.

### Stage 4 (both): one Settings line, one card rule, one failing todo

- **General piece:** `overview::sources()` becomes `overview::endpoints()`,
  one line per connection: the label, the kind's detail (one `match` on the
  kind: the mail credential's sentence from `overview.rs:591-603`, "Through
  the <name> plugin.", the Drive's folders), and the connection's standing
  joined with the health of each of its endpoints. One removal rule, "a
  source failing for `ABANDONED` days, or a calendar source holding nothing
  within `QUIET` days".
- **Generalises** `secretary::mail_health` (114) into the failing-endpoint
  todo of §8.1 and §9.6: one rule over `Endpoint`, proposed through the
  same path and from the same call (`app/cli/src/commands/secretary.rs:1757`).
  It files onto the prodrome the instance names for its upkeep, one key in
  the instance file, and uses that prodrome's own todo shape. It names no
  project chain and no todo kind (§8.1).
- **Removes:** `mail()`, `plugins()`, `last_run()`, `drive()`, `Store`,
  `Seen`, `Recorded` (205); in `failing.rs`, the per-source part of
  `unused` and `quiet_calendar` (≈ 80); the stage-2 migration verb (≈ 40).
- **Lines:** about −325, +160 (135 for the line and the removal rule, ≈ 10
  for the general todo, and ≈ 15 for a sink's price and its owner-only card).
  **Net ≈ −165.**
- **The phone:** the same lines in the same groups. A failing Drive line
  shows its failure and its terminal step, as a failing account does. A
  calendar, upload or sending endpoint failing for a day files a todo card
  for the upkeep prodrome, as a mail account does today for the project
  chain.

### Stage 5 (inputs): the writer, `Query::mail`, and uploads as messages

- **General piece:** the one `Deliver` over `sources/` (§3.1, §3.2), the text
  projection of a declared schema's fields, `Query::mail` and its four
  callers (§3.4), and the uploads source with a declared schema of its own.
- **Laws it adds:** an item tagged `inbox` never reaches the inbox page,
  the triage queue, a watch or the sweep (§3.4). A guard test holds the
  source root disjoint from every account's root (§3.6). An upload item's
  placeholder still reveals (§3.5).
- **Removes:** the upload index whole: `app/io/src/upload_index.rs` (130),
  `capability::UploadIndex` (22), `secretary::file_search` (121),
  `secretary::file_query` (145), `secretary::upload_standing` (47), and the
  `search` verb's file half and its merge (55). `upload_read` still refuses
  a held or erased upload, and reads that fact from the upload store, which
  owns it.
- **Migration:** the index is a cache, so there is no data to carry. `suzatary
  sources migrate` deletes the index's directory once, and stage 6 deletes
  that verb with its other work.
- **Lines:** about −520, +235. **Net ≈ −285.**
- **The phone:** the Mail page's search also finds the files the owner sent
  in a chat, each row named "Upload" and opening as a message whose text is
  the file's words, inline among the mail (§8.3). The Inbox does not change.
  The Android APK is a portal over the same pages and sees the same. A web
  change draws a row's source where its account is drawn now.

### Stage 6 (both): calendars as messages, and the loop closed

- **General piece:** the plugin adapter and the Google calendar deliver
  through stage 5's writer. The calendar's week reads `path:sources/link/**`
  and `path:sources/google-calendar/**` and parses each literal with the
  calendar schema's parser; `calendar::Reading` keeps its days and its zone.
  Once mail and calendars are both messages, every confirmation is a
  notmuch query, so this stage adds the inbox's `Confirmed` response and the
  one confirmer that runs after each pull (§9.5).
- **Removes:** `app/io/src/plugin_store.rs` (62), `store::learnt` and
  `store::knowledge` (75), the `Plugin` store row and `Database.plugins`
  (`store/kind.rs:143`, `store/database.rs:55,109`), `io::calendar::Clocked`
  over stores (34), the prodrome fold in `calendar/reading.rs` (≈ 70 of 132),
  and `io::google::learn` (29).
- **Migration:** plugin stores are cache tier, "rebuilt by syncing again"
  (CLAUDE.md). `suzatary sources migrate` deletes `cache/plugins/*/store`
  and each cursor once, the next run delivers every occurrence again, and
  the stage after this deletes the verb. Old stores are not read.
  `Confirmed` is a new inbox constructor, an addition the schema's laws
  cover (`app/core/tests/inbox_schema.rs`); no stored object changes.
- **Lines:** about −345, +175 (85 for the calendar, ≈ 90 for the confirmer
  and the phase). **Net ≈ −170.**
- **The phone:** the calendar page is unchanged. Search finds meetings, and a
  number in a meeting's title or place can be revealed on a tap like one in
  mail. A sent message whose copy reaches Sent, or a calendar event that
  comes back from the calendar, reads "Confirmed" on its card, and a send
  that may have gone stops asking the owner to check Sent once its copy is
  found.

### Stage 7 (deferred): Drive as a source

Deferred by the owner (§8.2). Nothing in stages 1 to 6 depends on it.

- **General piece:** none new; a Drive `Source` delivering the allowed
  folders' files.
- **Removes:** `drive/probe.rs` (39), which becomes its pull.
- **Adds:** listing and text extraction into items (≈ 120). **Net ≈ +80.**
  It is the one stage that adds code, because it adds a capability: Drive
  files become searchable. Without it, the Drive keeps its probe, and its
  health is still stage 1's register.
- **The phone:** search finds Drive files by their words; opening one is the
  `drive_read` tap it is today.

**In all, stages 1 to 6 are about −1,735 and +965, net ≈ −770.**

## 7. Questions for the owner

1. **The mail-only "failing for a day" todo.** A failing account files a fix
   card (`failing.rs`) and, after a day, a todo on the project chain
   (`mail_health.rs`). Recommend deleting the todo and keeping the card,
   which is the same rule for every source. Yes?
2. **Drive in search (stage 6 when asked, now stage 7).** It is the one stage that adds code. Do you
   want Drive files found by their words, or is reading them on a tap
   enough?
3. **Items in the phone's Mail search.** Recommend showing them inline,
   labelled by source. The alternative is a filter that is off by default.

## 8. The owner's answers (2026-10-25)

### 8.1 The failing-source todo: kept, and made general

The owner, against §7's recommendation: "the todo should be on the project,
but then for those you're proposing adding a todo that the agent can fix,
the agent can pick up the most important tasks first".

So the "failing for a day" todo becomes the rule for every source, not only
for mail, and for every sink (§9.6). Stage 4 generalises
`secretary::mail_health` and does not delete it.

- **One rule over `SourceKey`.** A source whose health reads as failing for
  longer than `health::persistent()` (a day, `app/core/src/mail/health.rs:27`)
  proposes one todo. It goes through the path that
  `mail_health::propose_persistent` takes today
  (`app/core/src/secretary/mail_health.rs:68-140`), so the owner's tap adds
  it.
- **Onto the upkeep prodrome, in its own shape.** The owner wants the
  project's prodrome to be an ordinary prodrome shared with him, with no
  project special case in core; that design is
  `docs/SHAREDPRODROME-PLAN-2026-10-25.md`, still to come. So the rule names
  neither the project chain nor a todo kind. Today it does both: it finds the
  chain by `deployment.project()` (`app/cli/src/commands/secretary.rs:1759`)
  and writes the kind `suzateryTodo` (`mail_health.rs:54`). The general rule
  files onto the prodrome the instance names for its upkeep, one key in the
  instance file, and builds the todo from that prodrome's own todo shape. A
  deployment whose upkeep key names no prodrome files no todo, and the cards
  still stand.
- **Once per failure episode.** The todo's id is a function of the source
  and the instant its failure began (its `since`), as `todo_of` is today
  (`mail_health.rs:26-34`). Asking again asks for the same todo, and a
  failure after a success is a new one. The id's prefix names the kind
  (`mailsync`, `linksync`, `calendarsync`, `uploadsync`). For a mail account
  the digest is over the lowercased address, exactly as today, so an
  episode open at the deploy is not proposed twice.
- **Priced the way todos are priced.** Today the todo is a flat 45%
  (`mail_health.rs:21`). The general todo is a `Price::Deadline`
  (`app/core/src/todo/price.rs:19-31`, `Deadline` at
  `app/core/src/todo/deadline.rs:16-26`). It decays from the kind's start
  percentage to its end percentage by `since` plus the kind's window, so how
  long a source has failed is read from the term at each instant and never
  rewritten. On the scale's anchors (CLAUDE.md, "The scale has anchors"):

  | Kind | Start | End | By | Why that date |
  |---|---|---|---|---|
  | mail | 45 | 35 | `since` + 7 days | the routine pull reads 7 days (`SYNC_WINDOW_DAYS`, `app/core/src/mail/imap.rs:145`), so older mail then needs a backfill |
  | a calendar link, a Google calendar | 65 | 55 | `since` + 7 days | the calendar reads a week back (`google/calendar.rs:33-37`) |
  | uploads | 65 | 55 | `since` + 7 days | the owner's own files go unsearchable |

  The class adjusts nothing in the number. It decides who can act, below.
- **The card is only for what the owner alone can do.** `failing.rs`
  already files a card only for a remedy a tap gives: a secret or a sign-in
  (`app/core/src/secretary/failing.rs:157-158,340`). When such a card
  exists, the todo names it (`proposal:<conversation>/<n>`, the one text
  form, `app/core/src/reference.rs:188`) in `waiting_on`. "waitingOn only
  when nothing we can do" (CLAUDE.md) is then true: the agent skips it and
  the owner's card is the one ask. When no card exists (an outage, an
  unclassified failure, a `Retry`), the todo is the agent's to fix, and
  `waiting_on` is empty. So the two never ask the same thing twice.

**How an agent picks the todo up.** Nothing polls the upkeep prodrome for
work. The coding poll reads `claude/` branches for a session's report
(`app/core/src/coding.rs:24,444`), not todos. On this deployment the upkeep
prodrome is the project chain, and a todo on it is read as work in three
places:

- the main session reads the queue with `todos --chain suzatary open`,
  ascending, most urgent first (`ROADMAP.md:9-13`;
  `app/cli/src/commands/todos.rs:54-58`);
- the secretary proposes a coding session only "for work they have asked
  for or that a todo on the project chain already names"
  (`SECRETARY.md:586-587`), and the owner's tap starts it;
- the pull request that fixes it names the todo on a `Completes:` line, and
  the train marks it done when it lands (`AGENTS.md:348-356`).

A source that succeeds again does not close the todo by itself, as today.
The fix's pull request or the owner closes it.

### 8.2 Drive in search: not now

Stage 7 stays in the plan as deferred, and nothing in stages 1 to 6 depends
on it. The Drive keeps its probe, whose health is stage 1's register.

### 8.3 Items in the phone's Mail search: inline

Items are shown inline among the mail, each row labelled by its source
(stage 5's web change: the row draws its source where it draws an account
now).

## 9. Outputs

### 9.1 What the box can be asked to do, measured

A tapped proposal is one of three sums today:

- `Proposal` (`app/core/src/secretary/store/proposal.rs:13-39`): `Email`,
  `Forward`, `Changes`, `Act`, `Remember`, `Decide`;
- `changes::Act` (`app/core/src/secretary/changes.rs:28-86`): `Add`,
  `Update`, `Complete`, `Cancel`, `Reopen`, `Tend`, `Reprice`;
- `acts::Act` (`app/core/src/secretary/acts.rs:22-173`, 19 arms): `Auth`,
  `AuthSend`, `Secret`, `Web`, `Patch`, `DriveRead`, `CodingSession`,
  `Device`, `ClaudeSignIn`, `SignInCode`, `UnlockStore`, `Connect`,
  `Disconnect`, `Grant`, `Configure`, `GoogleClient`, `GoogleSignIn`,
  `CalendarEvent`, `Erase`.

That is 30 things, not counting the two sums that only hold others. The
vault's "Keep it private" (`app/core/src/vault.rs:44`) is a tap outside them.
Sorted by what each one changes:

| Kind | Proposals | Where its effect lands |
|---|---|---|
| **write to a store** (17) | `Add`, `Update`, `Complete`, `Cancel`, `Reopen`, `Tend`, `Reprice` | a todo chain (`changes::apply`, `changes.rs:561`) |
| | `Remember`, `Decide`, `Erase` | the memory, the decisions, the conversations' keys |
| | `Device` | the signatures' history (`KeyAdded`) |
| | `Connect`, `Disconnect`, `Grant`, `Configure` | the instance file, by compare-and-swap under its `flock` (`settings/configure.rs:368`) |
| | `Secret`, `GoogleClient` | the secret store (the value travels in the tap's body alone) |
| **output to the world** (13) | `Email`, `Forward` | a mail server |
| | `Patch`, `CodingSession` | a repository, the cloud routine |
| | `CalendarEvent` | a Google calendar |
| | `Web`, `DriveRead` | a web server, the Drive: a read whose request itself leaves the box, so it waits for a tap |
| | `Auth`, `AuthSend`, `GoogleSignIn`, `ClaudeSignIn`, `SignInCode`, `UnlockStore` | an identity provider or the secret store's agent: a consent or an exchange whose result the box then keeps |

**Are the instance changes writes to a store? Yes.** The instance file is
read whole, compared with what the card read and written whole, under one
lock (`Edits::swap`). That is a register with compare-and-swap, which is a
store. `Secret` and `GoogleClient` write the secret store. So there are only
two kinds: a write to a store, which the store's own writer performs, and an
output to the world, which a sink performs.

The performers today:

- one `match` over 18 prepared arms (`routes::performed`,
  `app/core/src/secretary/routes.rs:1049-1130`, 84 lines);
- the act performer `Acting::begin`, a chain of `if let` over each outward
  act, each one repeating three arms: `NotYet`, `Real` and `Recording`
  (`app/cli/src/commands/secretary.rs:384-520`, 128 lines);
- `Post` for mail (`routes.rs:34-38`), reaching the one transport file
  (`app/cli/src/commands/mail.rs:428`);
- three ways to say "it may have gone": `Done::MayHaveGone`
  (`app/core/src/secretary/responded.rs:64`), `Inserted::Unknown`
  (`app/io/src/google.rs:53-61`), and the coding session's own states;
- no health for any output. A failed send's reason is a string
  (`Tried::NotSent`, `responded.rs:14`), so no class names its fix.

### 9.2 The split as a type

```rust
pub enum Proposal { Write(Write), Output(Output) }

pub enum Write {
    Chain(Changeset),            // Add .. Reprice
    Remember { .. }, Decide(Draft), Erase { .. },
    Device(Enrolment),
    Instance(InstanceEdit),      // Connect, Disconnect, Grant, Configure
    Secret(SecretWrite),         // Secret, GoogleClient
}

pub enum Output {
    Mail(Mail),                  // Email, Forward
    Patch { .. }, CodingSession { .. },
    CalendarEvent { .. },
    Read(Read),                  // Web, DriveRead
    SignIn(SignIn),              // Auth, AuthSend, GoogleSignIn, ClaudeSignIn, SignInCode, UnlockStore
}
```

Each arm prints with the constructor it prints with today, so the inbox's
stored objects do not move and no migration runs.
`app/core/tests/act_literal.rs` already reads every arm of every act back
from its print, and stays the pin. A `Write` is performed by its store's
writer under that store's hold, as `changes::apply` is now. An `Output` is
performed by its sink.

### 9.3 One sink port

```rust
/// A sink: something outside the box that a tapped output changes.
pub trait Sink {
    fn perform(&self, output: &Output, consent: Consent) -> Result<Receipt, Unperformed>;
}

/// What a sink said it did, and the input that will show it.
pub struct Receipt { pub confirms: Confirms, pub said: String }

/// Why it did not, by what its transport knew.
pub enum Unperformed {
    NotSent { class: Failure, said: String },
    MayHaveGone { class: Failure, said: String },
}
```

- `Consent` is today's, minted only by `Consent::given_by_tap` from a `Tap`
  (`app/core/src/secretary/tap.rs:27,37`), so the agent, which holds no
  `Tap`, cannot perform an output: the same proof as today, moved to one
  port.
- **Native sinks**, trusted: the mail transport, the Google calendar insert,
  the publisher of a patch, the coding routine, the web fetcher, the Drive
  (read and keep), and the sign-in flows. Each is one table entry. A build
  without one has no entry, so "not performed by this build" is said once.
  Today it is repeated per act.
- **The sandboxed kind is not the actor world.** An actor returns proposals
  and never performs (#243 keeps that world unhosted), so it is part of the
  agent, not a sink. Calling it a sink would let a plugin perform. A
  sandboxed sink would be a world of its own: one `perform(output) ->
  receipt` export, importing a fetch that may write only to its manifest's
  origins, called only by the performer after a tap. Nothing needs one now,
  and #243's fetch is GET-only, so no plugin can change the world today.
- **What is shared with the sources, and nothing more** (§10.2): the
  `Endpoint` key (a connection may have an input side, an output side or
  both: a mail account reads and sends, a Google account reads and writes
  its calendar, the Drive reads and keeps), the one health register, the
  failing rule (§9.6) and the Settings line. **What is not shared:** the
  cursor, the runner and `Deliver` belong to inputs; consent, at-most-once
  and confirmation belong to outputs. An output has no ledger of its own,
  because the inbox's state register already records each try and its
  settlement per proposal. A sink's health is the join of those outcomes,
  so nothing is kept twice.

### 9.4 Once, by the object's name

The output is already a content-addressed object: the inbox's `Asked`,
whose proposal is named by its content (`ProposalId`,
`app/core/src/secretary/inbox.rs:268-286`). `inbox::submit`
(`inbox.rs:823`) asks the inbox's one non-monotone question under
`submit.lock` ("does it still stand, so I may perform?"). It performs only
while nothing settled the proposal, and otherwise returns the recorded
response. A try that sent nothing records `NotSent`, so a resubmit runs again
(`docs/ARCHITECTURE.md`, "inbox::submit(id) is the one way a tap performs
anything"). `app/core/tests/inbox.rs` is the law: any interleaving of
submits, resubmits, dismissals and acknowledgements performs each effect at
most once. The sink port changes none of this. It is the one call the
performer makes inside that rule.

### 9.5 Closing the loop: an output is confirmed by an input

Today the owner closes the loop by hand. A send that may have gone is
marked "check Sent" until the owner taps acknowledge, which appends
`Checked` (`docs/ARCHITECTURE.md`, "A send that may have gone stays in the
Inbox"; `app/cli/src/commands/serve/table.rs:101`). A sink's "sent" is the
sink's claim, not an observation.

Generalised: every `Receipt` names the input that will show the effect.

| Output | `Confirms` | How it arrives |
|---|---|---|
| `Email`, `Forward` | the message with the Message-ID the box minted (`app/cli/src/commands/mail.rs:491`) in that account's Sent | the mail source's next pull; `id:<mid>` in notmuch |
| `CalendarEvent` | the event, by the id Google answered, in that account's calendar | the Google calendar source; after stage 6, an item message |
| `CodingSession` | its report on a `claude/` branch naming its URL | the coding poll, which already works this way (`app/core/src/coding.rs:444`, `app/cli/src/commands/coding.rs:150`) |
| `Patch` | its branch on the repository's mirror | a listing of that mirror's branches, which the coding poll already reads for `claude/` |
| `Auth`, `AuthSend`, `GoogleSignIn`, `ClaudeSignIn`, `SignInCode` | the grant or token in its store, and the next success of the endpoint it serves | stage 1's health register |
| `Web`, `DriveRead`, `UnlockStore` | its own answer | the answer itself, recorded as today |

The inbox gains one response, `Confirmed { proposal, by }`, whose `by` is a
reference in its one text form (`mail:<hash>`, `proposal:…`). It sits above
`Sent`, `MayHaveGone` and `Checked`, since all three say "it went" and
`Confirmed` is the observation that it did. It is incomparable with
`Dismissed`. So a send that the owner dismissed and that turns up in Sent is
a conflict, shown and not tiebroken. The confirmer is a reading: from the
inbox and the mail index, the set of `Confirmed` responses owed. It runs
after each pull and only appends, so it takes no lock. A may-have-gone send
whose copy arrives stops asking the owner. One whose copy never arrives
still asks, and `Checked` stays for it.

### 9.6 Failing outputs

A sink's `Unperformed` carries a `Failure` class, parsed where the failure
is observed, as a source's `Unpulled` does (`app/core/src/mail/puller.rs:18-26`).
Its outcome joins the stage-1 register under `Out(SinkKey)`. The rule is
§8.1's, unchanged: an endpoint failing for a day proposes one todo on the
upkeep prodrome, once per episode, priced by a deadline. A card is filed
only for what the owner alone can do: a lapsed send consent is the
`AuthSend` card, which the todo names in `waiting_on`. A sending endpoint is
priced as its account's mail is (45% to 35% over 7 days): mail that cannot
go out is a real consequence within weeks.

## 10. The whole design, reviewed

### 10.1 Conal Elliott and Phil Wadler: the denotation

- **The store** denotes a set of objects `S`, joined by union. A reading at
  instant `t` is a function of `S` restricted to `at ≤ t`. Every fold
  already is one (CLAUDE.md, "A register holds its frontier").
- **An input** denotes an increase of `S`: `S ↦ S ∪ ΔS`. A pull is
  monotone, so two pulls in either order, or twice, give the same `S`.
- **An output** denotes a request whose meaning is an observation, not a
  sink's reply: `confirms(o) : S → Bool`, monotone (once confirmed, it stays
  confirmed). That is why §9.5 matters to the denotation and is not a
  convenience. Without it, `Sent` means "the sink said so".
- **The agent** is close to a function from readings to proposals, but not
  exactly. It is `Reading → (Proposals, Pulls)`. Its proposals are objects
  it adds to the inbox, and its pulls ask sources for more of `S`. Both only
  increase `S`, so the agent is itself an input to the store. The one arrow
  from `S` out to the world is the performer, and it needs a `Tap`, itself
  an object the owner's device adds. So "a pure store plus inputs and
  outputs" is the right denotation, once the agent's own writes are counted
  as inputs.
- **The types say it.** `Proposal = Write | Output` is data describing
  effects, interpreted by one shell, in the manner of an algebra of effects
  with its handler. The agent cannot perform because it cannot build a
  `Consent`. That is a fact about which values its code can construct
  (`Tap::of` is the only constructor, behind the gate), as "no send tool" is
  today.
- **Where it is weaker.** The secretary's direct chain write is a `Write`
  whose consent is the store's trust policy, itself part of `S`. That is
  still a function of `S`, and it is not an output. Time enters only through
  `at`, which is data (CLAUDE.md, "Order is causal, time is data").

### 10.2 Rich Hickey: do inputs and outputs share only what they share?

They share the endpoint, its health, its failing rule and its Settings line,
because those are about the connection, not the direction. They do not share
the operation. A source is monotone and needs no coordination. A sink is
non-monotone and needs consent, a lock and at-most-once (PLUGINS-PLAN §3
draws the same line with CALM). So there are two ports, `Source` and `Sink`,
and no `Effect` port with both `pull` and `perform`: that would complect a
read that may run on any timer with a write that may run once after a tap.
There is also no output ledger beside the inbox's state register: the
register is the ledger, and a second one would be a copy to keep
consistent.
