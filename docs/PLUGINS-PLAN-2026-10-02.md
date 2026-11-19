# Plugins: the outside world behind WASI interfaces

The owner, 2026-10-02: "things like calendars and providers for this sort of
thing should be given some wasi interface where the suzatary coder can build
plugins for it that satisfy the interface, should be the same for data
providers and email stuff, basically it can do custom shit that's plug and
play".

A design; stages 1 to 5 are built (2026-10-02 to 2026-10-05, below), and
stage 5's second half, the routine taught to build a plugin, waits. It replaces `USERREPO-PLAN-2026-10-01.md` §3's
"a plugin is compiled in, so it runs with the app's authority": a plugin is a
WebAssembly COMPONENT loaded at run time, holding exactly the authority its
imports give it. Its neighbours: the Prodrome's
`docs/design-register-types.md` §6.0 (views are data, and the open question of
agent-authored state), §6.2 (caches), and the decision `memoisedfold`.

## 1. A plugin is a bundle of three data

| Part | What it is | Who checks it |
|---|---|---|
| a component | a `.wasm` targeting one of the app's WIT worlds (§2) | the host, at load: the world and its version match, or it is refused |
| a schema | the events and registers of the plugin's own store, declared as data (§4) | the admission laws (§5) |
| views | Typst modules over that store's reading (§6.0's pages as data) | the tripwire and the closed intent vocabulary, as every view |

A manifest names the three, the world it targets and the authorities it asks
for. Plug and play: a bundle dropped into the user repository's `plugins/`
and named in `instance.toml` is loaded at the next start, with no rebuild of
the app. Where a bundle comes from (the app's own reference plugins, the
Suzatary coder, the owner) is left open on purpose, as for views.

## 2. The worlds: the record of authorities, as WIT

CLAUDE.md's rule, "a record is a list of authorities, one trait per effect,
passed as a value", is exactly the component model's: a component can call
only what it IMPORTS, and the host links each import per plugin from the
manifest. So each world is a capability record, and a missing import is a
missing power that no code inside the plugin can reach around.

- **`source`** exports `sync(cursor) -> (items, cursor)`: read the outside
  world from where it last stopped. It imports `fetch` (below), a clock and
  a logger. Calendars, data providers, a mail account's read side.
- **`actor`** exports `decide(acts) -> list<proposal>`: handed the events
  of its store (its own, or the one its manifest grants it), it returns PROPOSALS as values, each an act of
  the Inbox's closed vocabulary, a reason and what it rests on. It imports the clock and the log and
  nothing that reaches outward, so it never performs: what it proposes
  waits for the owner's tap, through the existing performers (CLAUDE.md,
  "The secretary proposes and the owner taps"), and what a tap performs is
  the stored act, never a body passed in. Accepting an invite.
- More worlds as needs appear (a `transform` that only maps items, with no
  imports at all), each one a record with no field it does not need.

**Fetch, never sockets, never secrets.** No world imports `wasi:sockets` or
`wasi:filesystem`. `fetch(request, credential-name?)` is the host's: it
allows only the hosts the manifest declares, injects the named credential
(from secretspec, by NAME; a plugin's manifest declares the names it needs,
the Settings page sets them like any other) into the request, and returns
the response. A plugin never holds a token, so it cannot leak one. A consent
flow (OAuth's device code) stays the host's, behind `Consent::given_by_tap`;
the manifest declares the provider's endpoints and scopes as data.

**Bounded.** Each call runs under wasmtime with a fuel or epoch budget and a
memory cap, so a plugin that loops or grows is stopped and the run recorded,
never the host taken with it.

## 3. CALM decides which world an effect belongs to

A `source` is MONOTONE: what it learns is appended as objects of its schema,
and an object is idempotent by its content name, so syncing twice, or from
two hosts, is syncing once (§1 of the Prodrome's design). It needs no
coordination and can run on any timer. An `actor`'s ACT is NON-MONOTONE
("may I send this?"), so it is performed once, under the inbox's lock,
after a tap; its asking is not, since a proposal is filed by its act and
asking again files nothing, so the actor runs on the same timer. The
split is the one the box already has between triage and the tap, now stated
as two worlds.

## 4. A plugin's state is a store of its own schema

A source's items land in a store at the plugin's schema (events, registers,
orders, inflationary or not), so a plugin's data is one more field of
`store::Database` and is read, synced, verified and viewed like the inbox
or the conversations. The manifest says its TIER:

- **history**: objects pushed with the user repository, part of every
  checkpoint (a ledger a provider will not keep for us);
- **cache**: a replica of an outside truth, rebuildable by syncing again,
  never pushed and never in a checkpoint (§6.2's rule; mail in notmuch is
  this tier today, and so is a calendar).

Declaring a schema as data, rather than as Rust behind `schema!`, is the
step the Prodrome's §6.0 records as open ("an agent adding STATE of its
own"). Plugins are its first user, so the declarative schema is built for
them (§7, stage 3) and reused by anything else that needs it.

## 5. Admission is a law check

A bundle is loaded only if its laws hold, checked mechanically at admission
and recorded beside it:

- the component targets a world this app has, at its version;
- `sync` is idempotent: run twice from one cursor over a recorded response,
  it yields the same objects;
- every item it emits parses into its schema and prints back the same;
- the schema's order is a partial order and an inflationary register only
  climbs (the Prodrome's laws 2 and 4, run over the plugin's own generated
  values);
- its views compile against its schema's reading and pass the tripwire.

A bundle that fails is refused with the law that failed, never loaded with a
warning. Who wrote it does not change the check.

## 6. What existing paths become

Nothing is ported for its own sake. The first plugin is new work, and an
existing path moves only when touching it anyway pays for the move.

- **Calendar**: the first plugin, a `source` reading an iCal (ICS) feed.
  Google, Microsoft and Proton all publish a secret iCal link per calendar,
  so it needs no new OAuth consent and is read-only by construction; the
  link is a credential, set on the Settings page by name. Tier: cache. Its
  view feeds the briefing and the secretary's context; meetings never
  become todos (CLAUDE.md), anything owed after one may.
- **Mail**: today `mbsync`, `lieer` and Proton Bridge feed notmuch, and
  `commands::mail::deliver` sends. A mail source and a mail actor are worlds
  they could become, one account at a time; the send side keeps every rule
  it has (the tap, the stored message, the separate send consent).
- **Data providers** (a bank export, a health record, a shipment tracker):
  sources, each with its own schema and views.

## 7. Stages

Each is one pull request, gated in full.

1. **The host and the `source` world**: wasmtime with the component model in
   `suzatary-io`, the WIT for `source` and the host's `fetch` (allowlist,
   credential by name, budget and memory cap), a manifest type parsed once,
   and a test plugin built in the flake that proves each fence (a host not
   allowed, a credential not declared, a loop, a memory bomb).
   **Built 2026-10-02** (the `plugin-host-source-world-xrt7p2` report):
   the WIT is `app/wit/plugin.wit` (`suzatary:plugin@0.1.0`); the host is a
   crate of its own, `app/plugin` (`suzatary-plugin`), not `suzatary-io`, so
   nothing compiles wasmtime before a verb calls it; the manifest and the
   fetch fence are pure, in `suzatary_core::plugin`; credentials are named in
   a `PLUGIN_<NAME>_` namespace and bound to one use and one origin, and an
   answer echoing a value is withheld; the test plugin is `app/plugin-fence`,
   built for wasm32-unknown-unknown and componentised by `wasm-tools`, its
   fences `app/plugin/tests/fence.rs`. Nothing calls the host yet.
2. **The ICS plugin** as the app's first reference plugin, its items
   appended to a cache-tier store at a schema written in Rust for now, its
   view in Typst, the briefing reading it.
   **Built 2026-10-03** (the `ics-plugin-calendar-store-2z7nlc` report):
   `app/plugin-ics`, built by `.#suzatary-plugins` into `ics.wasm`, reads each
   `url` credential its manifest declares (`sync` is now lent the declared
   NAMES, `suzatary:plugin@0.2.0`) and emits one `calendar.occurrence` item
   per occurrence in a window of 7 days back and 60 ahead; RFC 5545's
   recurrence (RRULE, RDATE, EXDATE, RECURRENCE-ID, VTIMEZONE) is written
   there over chrono and chrono-tz, since no maintained RRULE crate fits
   wasm32-unknown-unknown. Its cursor holds what it emitted, so an unchanged
   feed emits nothing and an occurrence gone from its feed is emitted once as
   cancelled. The store is `suzatary_core::calendar` (a `Genesis`-rooted
   prodrome at `CalendarEvent`, cache tier under the data root's `cache/`),
   entered only through `Occurrence::admit` (parse, print back the same, a
   calendar the manifest declares) and `calendar::learn` (only what the store
   does not already read). `suzatary plugin sync` runs it on a 15-minute
   timer and keeps a ledger beside the cursor; the Settings page sets each
   link by name and shows when it last synced; the secretary reads it through
   a Tainted `calendar` verb (its brief's Today list), `context` carries
   today's and tomorrow's times only, and `#/calendar` is a Typst week.
3. **Declarative schemas**: a plugin's schema as data, with the admission
   laws of §5 run over it; the ICS plugin moves to it.
   **Built 2026-10-03** (the `plugin-schema-declarations-3w9gcn` report):
   the manifest's `schema` names a declaration in the Prodrome's schema
   language (its main, `declared::Declaration`, PR #36) beside the component;
   `Host::load` admits it under the Prodrome's eight laws and refuses the
   plugin by the law it breaks. An item is an event of that schema, printed in
   the objects' own grammar and named by the schema's hash, admitted by
   `suzatary_core::plugin::admit` (it parses, prints back the same, and says
   the plugin wrote it). §5's laws beyond the schema's are `plugin::Law`, held
   over a `suzatary_plugin::Trial` (the run, recorded, replayed from the same
   cursor and from its own at one instant): items, deterministic, idempotent;
   a run that breaks one learns nothing. Each plugin's store is
   `cache/plugins/<name>/store` at its declaration, the text beside it.
   `app/plugin-ics/ics.schema` is the calendar's schema (one `Learned` event
   keyed by calendar, original start and `UID`, five discrete registers) and
   the Rust `CalendarEvent` is deleted; `calendar` reads every store whose
   schema has the calendar's registers, and the fixture feeds' readings are
   pinned byte for byte across the move. §5's law on views waits for views.
4. **The `actor` world**, through the tap, first for something small and
   reversible.
   **Built 2026-10-05**, in two steps. The world and its fence (the
   `portsuites-plugins-stages-2o0pin` report): an actor PROPOSES and never
   performs; it exports `decide(acts) -> list<proposal>`, handed every event
   its store holds as a set, imports the clock and the log only, so
   `Host::load_actor` refuses one importing `fetch`, and its manifest may
   name no origin or credential. Then through the tap (`actorinbox`, the
   `actor-inbox-stage-4-2mgfay` report):
   - A proposal is an act of the Inbox's CLOSED act vocabulary, not of the
     plugin's own schema, since only an act the Inbox holds has a performer:
     the WIT's `proposal` is the act's literal, a reason and its sources
     (`suzatary:plugin@0.3.0`). For now a plugin may ask only for a change
     to a todo chain, and an `Offer` holds a `Changeset`, so no other act is
     expressible past the door; a decision, mail, a pull request, a coding
     session and the box's acts wait for a stage that gives each the checks the
     secretary's verbs run. `plugin::propose` admits an `Offer` or refuses
     it naming the plugin: a literal outside the vocabulary (an Inbox
     response, a field claiming an actor), one not printed as itself, an act
     a plugin may not ask for, an offer over 24 KiB whole or resting on more
     than 8 sources or one over 256 bytes, no reason or no ground. The
     sync's journal names who first asked for an act it already held.
   - Bounded: one decision is read to 8 proposals, the rest refused by
     count, and a plugin with 5 proposals waiting files no new act until the
     owner answers one, so an actor minting a new act each tick fills five
     cards and stops. A change is built against its chain (`changes::checked`,
     the check `propose_changes` runs) before it is filed.
   - An actor reads ONE store: its own, or the one plugin's its manifest
     grants it (`reads = "ics"`), where that store is at its schema.
     `suzatary plugin sync` asks each actor after the sources, on the same
     timer.
   - An offer is filed through `secretary::filed`, the one path the
     secretary's proposals take too, under the inbox's hold, as the inbox
     kind `Offered`, whose `plugin` field names who asked (`Asked` is
     unchanged byte for byte), in the box's conversation `plugin-<name>`.
     The inbox's identity is the act, so the same act from the same plugin
     is one proposal, and the next tick files nothing; an act someone else
     already asked for is not drawn again for a plugin.
   - The card says "proposed by the <name> plugin" (`Card.by`, and the
     Inbox row's), and the owner's tap performs it through the existing
     route and performers, the response his.
   - The reference actor is the calendar's, `due.wasm` (`app/plugin-ics`
     with `--features due`): a `Due: <thing>` entry, today or later and
     never cancelled, proposes a todo for the thing due that day.
5. **Plugins from the user repository**: `plugins/` there, loaded by name
   from `instance.toml`, and the Suzatary coder's routine taught to build
   and submit one as a pull request to the user repository.
   **Built 2026-10-05** (same report): a manifest's `from = "user"` reads its
   component and schema under the data repository, each pinned by the
   sha256 the owner approved (`component_sha256`, `schema_sha256`), read
   once, checked and loaded as the same bytes, and admitted by the same
   laws; the admission (when, and both digests) is recorded in the plugin's
   ledger; a path is plain segments, and one a link resolves outside its
   root is refused. Not yet: the routine is not taught to build one.

## 8. Non-goals

- Plugins in the browser. The component model runs there too (transpiled),
  and a view's state may one day come from one, but nothing here needs it.
- A plugin store or registry. A plugin is a file in a private repository.
- Native plugins (`dlopen`): they would run with the app's authority, which
  is what this design exists to avoid.
