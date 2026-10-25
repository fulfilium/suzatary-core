# Suzatary in Rust: the architecture

Decided 2026-09-07 with the owner ("build the whole thing"; "let's not care about
backwards compatibility"). Every stage is built from this: the pieces, which
way the arrows point, which rules are types, which claims are laws, in what
order the Python left. `CLAUDE.md` wins where they disagree; the Prodrome's
`SPEC.md` (<https://github.com/fulfilium/prodrome/blob/main/SPEC.md>) is the
database's contract and is not restated here.

Check each stage against three readers: a Rust maintainer (crate graph, I/O
discipline, typed errors, no `unsafe`), Edward Kmett (structure as laws,
illegal states unrepresentable, decoration separate from evaluation), the
Pijul maintainers (order is causal, facts content-addressed, replicas exchange
facts, conflicts are values).

## 1. What is what

**The Prodrome is the temporal FPL database and nothing else**: literals,
objects and the DAG, the trust rule, folds and registers, FPL and its
breakpoints; no briefing, mailbox, phone, gate or page. Since 2026-09-09 it is
<https://github.com/fulfilium/prodrome> (was `prodrome/` here; public, MIT OR
Apache-2.0, own flake and CI), taken BY REVISION (§8.8): an upstream push
cannot change this build without a commit here saying so.

**Suzatary is an application built on the Prodrome**: routes and wire, the
tailnet gate, push, briefings, mail, the typst decoration, the agent's tools,
the CLI. Since 2026-09-08, three Rust crates and one binary; no Python in the
tree. Nothing flows from Suzatary into the Prodrome — no type, concept or
feature flag.

## 2. The crates

Arrows point down only; a crate depends on those below it and nothing above.

```
suzatary        one binary: serve | mcp | sync | web | todo … (axum, tokio live HERE and nowhere else)
suzatary-plugin the plugin host: wasmtime components holding only their world's imports (`suzatary plugin sync`)
suzatary-io     the interpreters: tailscaled whois, FCM, ntfy, notmuch + maildir, typst, IMAP/SMTP/OAuth transport
suzatary-core   the application's MEANING, pure: wire types, view decoration, link graph, notification decision,
                markup escape, mail models + triage state, the typst block, the sync algebra
prodrome-core   the database (unchanged in role; gains `view`, §4)
```

`app/core`, `app/io`, `app/cli`, `app/bench`, `app/wasm`, `app/plugin`,
`app/plugin-fence` and `app/plugin-ics` are the ONE Cargo workspace, at `app/`.
`app/plugin` is the plugin host (`docs/PLUGINS-PLAN-2026-10-02.md`), a crate of
its own because wasmtime is the largest tree in the workspace and only plugins
need it; `cli` depends on it for `suzatary plugin sync` (stage 2,
2026-10-03). `app/wit/plugin.wit` is the plugin world's contract
(`suzatary:plugin@0.4.0`), `app/plugin-fence` is a hostile test plugin the
flake builds into the components `app/plugin/tests/fence.rs` attacks the host
with, and `app/plugin-ics` is the app's first reference plugin, the calendar,
built by `.#suzatary-plugins` for wasm32-wasip2 into `ics.wasm`, held to the
same fences.
`prodrome-core` is a GIT DEPENDENCY pinned by `rev` in `app/Cargo.toml`'s
`[workspace.dependencies]`, vendored by crane from the lock (§8.8);
`flake.nix` pins the SAME sha as `inputs.prodrome`, and the tab's module is
built here from that crate (`app/wasm`, `packages.suzatary-wasm`, §11.1), so the
binary's evaluator and the tab's are one build. Upstream holds `core`, `wasm`, `SPEC.md`, the seeded conformance
vectors; here stay `app/conformance/literals.json` and `app/cli/tests/live.rs`,
the pins on THIS box's chain (the fold pins went to the data repository,
2026-09-26). (`py`, the PyO3 binding, left in §8.6.)

ONE MODULE PER VERB FAMILY (`docs/AUDIT-2026-09-09.md` finding 6):
`app/cli/src/commands/<verb>.rs` owns the verb's clap types AND its function —
`serve` (its own directory), `mcp`, `todo`, `todos`, `notify`, `site` (`web`), `mail` (ten corpus and transport verbs),
`replica`, `vault`, `up`, `verify`, `weather`, `doctor`, `fixture`. `main.rs`
is the argument tree, the instance file's location and a `match`; `lib.rs`
re-exports `commands::serve` so tests can drive that router with fakes.
Nothing is organised by the stage it arrived in.

Rules for every crate:

- `#![forbid(unsafe_code)]`. Clippy at `-D warnings` over `--all-targets`.
- Typed errors per crate (`thiserror`), `Result` on every fallible path, no
  panics on input. Libraries never use `anyhow`; the binary may, at its edge.
- `suzatary-core`: no I/O, no async. `suzatary-io`: I/O, no policy. The
  binary has the runtime. Meaning that needs an effect takes it as a
  capability (§3) and stays in `core`.
- rustls, never OpenSSL. Few dependencies, each named with its reason in the
  crate's `lib.rs` header.
- Every wire type is declared ONCE, in `suzatary-core`, with `serde`; the
  browser's TypeScript is GENERATED (`ts-rs`) at build time, and so is each
  answer's decoder, folded from that text (`wire::shape`) into `decoder.ts`'s
  combinators and annotated with its type, so tsc refuses one that misread it.
  What a page sends has a type and no decoder; a field an answer may leave out
  has none either (a missing field is a break). A hand-written type or decoder
  in `web/src` for a server shape is a second declaration, and drifts.

## 3. Rules as types: capabilities

Every effect is a trait in `suzatary-core`; pure code is generic over the
bundle it is handed:

```rust
pub trait Whois      { fn login_of(&self, peer: IpAddr) -> Result<Option<Login>, WhoisError>; }
pub trait Notify     { fn push(&self, message: &Push) -> Result<Delivered, NotifyError>; }
pub trait Mailbox    { fn search(&self, q: &Query, …) -> Result<Vec<MailRow>, MailError>; … }
pub trait Corpus     { fn queue(&self, limit: Option<usize>) -> Result<Value, CorpusError>; … }
pub trait Repository { fn diff_summary(&self) -> Result<String, RepositoryError>; }
pub trait Manuals    { fn document(&self, which: Manual) -> Option<String>; … }
pub trait Render     { fn html(&self, markup: &Escaped) -> Result<Html, RenderError>; }
pub trait Clock      { fn now(&self) -> Datetime; }

pub trait ServerCapabilities:
    Chain + Whois + Notify + Mailbox + Render + Clock + Briefings + Devices {}
pub trait AgentCapabilities:
    Chain + Corpus + Mailbox + Repository + Manuals + Render + Clock {}
```

ONE TRAIT PER EFFECT, one file each in `app/core/src/capability/`; a BUNDLE
is the only place the list is said. `Corpus`, `Repository` and `Manuals` were
one `Triage` until the 2026-09-09 audit: a bundle is a list of authorities,
and a member naming five says less than nothing.

`suzatary-io` implements them against the world; tests implement them with
fakes; the binary chooses. A RULE becomes a TYPE:

- **The triage agent cannot send mail**: its bundle has no sending trait; the
  remaining test checks only the bundle's shape.
- **Reading mail never yields a send token**: `ImapToken` and `SmtpToken` are
  distinct types; the refresh minting one cannot return the other.
- **Confirmed and provisional content are a sum**, `Content::Confirmed(_) |
  Content::Provisional(_)`, not a record with a bool. "Rendered means
  confirmed" is `Render::html` taking `Escaped`, which only the confirmed arm
  produces.
- **Loopback never writes** (SPEC §5, webapp Gate): an actor is a `Login` from
  `Whois`; nothing constructs one from an address.
- **A plugin is one function and its grant** (2026-10-24). A plugin is a WebAssembly
  component of `app/wit/plugin.wit`'s `source` world: one export, `sync(cursor,
  credentials)`, which returns items and the next cursor, and one import of ours,
  `fetch`. It is built for wasm32-wasip2, so it also imports WASI's clocks, random
  numbers and standard streams; the host links those with nothing in them (an empty
  environment and standard input, a bounded standard error, the box's own clocks) and
  links no WASI filesystem or sockets, so
  `suzatary_plugin::Host::load` refuses a component importing either
  (`LoadError::NotASource`) before it runs. The effects behind the imports are a
  record, `suzatary_plugin::Effects` (transport and secret store), lent per call.
  `fetch` crosses `suzatary_core::plugin::Target::admit` against the `Manifest`,
  which parses only to declared origins and to credential NAMES in the plugin's own
  `PLUGIN_<NAME>_` namespace; a credential's value is a URL the host resolves, holds
  only in `Outgoing` and requests only at an allowed origin over https or loopback,
  and an answer that echoes it, or its path or query, is withheld; a call makes at
  most `MAX_FETCHES`. Each call is a fresh `Store` under a `Budget` (an epoch
  deadline, a `ResourceLimiter` memory cap), stopped with `RunError::Deadline` or
  `RunError::Memory`. The host links its own WASI monotonic clock
  (`suzatary_plugin`'s `clock.rs`), whose every wait ends at the call's deadline:
  a wait blocks in the host, where the epoch never fires. The host keeps its own record of each
  fetch (`Run::fetches`), so what reads "when did this link last sync" rests on the
  host's observation.
- **What a plugin learns enters through its schema's door**. A plugin's schema is a
  DECLARATION shipped beside its component (`[[plugins]] schema`, the calendar's
  `app/plugin-ics/ics.schema`), admitted by the Prodrome's
  `declared::Declaration::admit` when the host loads it (`LoadError::Schema` names the
  law; no Rust schema mirrors it). An item is an EVENT of that schema in the objects'
  literal grammar; `suzatary_core::plugin::admit` parses it there, prints it back the
  same (a second spelling would be a second object) and holds it to the plugin's own
  actor. `store::learn` then writes only what the store does not already read
  (`store::Knowledge`), so a sync is monotone and idempotent and its timer needs no
  lock. Each plugin's store is CACHE tier (`[cache]`, `cache/` under the data root):
  `cache/plugins/<name>/store`, a `Genesis`-rooted prodrome at the admitted
  declaration with its text in `schema` beside it (another schema begins it afresh),
  box-local, never pushed, never staged by `safe-push.sh` (a guard test), rebuilt by
  syncing again. The calendar is a READER of every such store whose schema has its
  registers (`calendar::Occurrence::reads`/`of`: an occurrence keyed by calendar,
  original start and `UID`, five discrete registers, a calendar only in its writer's
  namespace). Readers reach it as `capability::Calendar` in the secretary's record,
  read in the box's clock by `suzatary_io::calendar::Clocked`; its titles are other
  people's words, so the `calendar` verb is Tainted and `context` carries only times.
  Meetings are never todos: nothing writes from it to a chain.
- **The actor world is an interface only**. `app/wit/actor.wit` documents a plugin that
  reads a store and proposes acts for the owner to tap; no plugin targets it, and the
  host has no machinery for it until one does. Proposals a removed actor filed stay in
  the Inbox's history, named `Proposer::Plugin`.

Tagless final, or traits. Preferred over an effect enum with an interpreter
loop: a route needing whois, a fold and a reply should read as three lines,
not a state machine.

## 4. What the Prodrome gains: `view`

`prodrome_core::view::entries(chain, t, untrusted) -> Vec<Entry>`: one todo as
the folds see it at `t`. The COMPOSITION of §6's folds — confirmed registers
(state, content, conflicts), loose registers (the claim an untrusted actor's
event would make), `flatten` for the function, `fulfillment` for the value.
In the Prodrome because every word is its own and the server and the
browser's offline fold must be the SAME reading; exposed by `suzatary-wasm`.
Law (§6): `entries` equals that composition, on random DAGs. An `Entry`
carries the NAMES of its candidate content objects, not the records; the
Prodrome does not know what a body is for. A register with two candidates is
a conflict: it prices as its most urgent world (`Least`) and the page draws
every candidate.

## 5. The phone is a replica

The phone holds objects (IndexedDB, the core in Wasm) and the HEADS they end
at. An act is the Prodrome's `Change` (2026-10-02), written by the module's
`Todos.append` into the replica the tab holds: its deps are its todo's heads in
the tab's own fold, never a head of the whole replica, so a phone with several
heads acts as one with one. Sync exchanges content-addressed objects by name,
then `adopt` and, where heads diverged, nothing more: the heads are derived, and a replica holding
several is one replica (2026-10-03; until then the box wove an eventless
`Woven` over every head after a write, a bridge for the agent's legacy batches
and the readers of one head, all gone). Union of sets is idempotent,
commutative, associative: two tabs writing apart converge by adoption in
either order (`web/test/append.test.ts`, `sync.rs`'s proptest). Every writer
writes `Change`s, the box since change identity (2026-09-30), the phone since
2026-10-02 and the agent since 2026-10-03; legacy envelopes are read forever
and written by nothing:

- nonces, base tips, `/api/act/batch` and the outbox replay were DELETED, not
  ported. Idempotence is the hash.
- divergence is more than one head, and a register written on two of them is
  a conflict the UI shows (`Entry.conflicts`); a later write settles it.
- time stays data (SPEC §1): the phone's `at` is its claim, never clamped on
  adopt. A wrong clock yields a future-dated object the UI can say and
  `verify` reports; rewriting it is what the Prodrome refuses to do.

Two routes: `GET /api/objects?since=<frontier>` (names and literals outside
what the frontier names) and `POST /api/objects` (the server rehashes,
verifies parents and a change's genesis, refuses another actor's event, and
takes the rest in through the Prodrome's `store::accept`, which verifies each
on receipt). A FRONTIER IS A TOKEN THE BOX HANDED OUT (2026-10-25,
`suzatary_core::frontier`; a replica of 400 heads named them in the URL, 26
kB, and was refused 431): every answer to a pull or a post ends at the heads
of what its asker then holds, which the box records under their digest (the
sha256 of the sorted names), keeping each store's 512 latest, box-local in
the state directory, and answers as `frontier {digest, count}` beside
`since: Known|Unknown`. The asker names it back as `since`, 64 characters
whatever it holds, so no request grows with a replica and no server limit is
raised. A post carries `since` in its body, and its receipt's frontier is
the closure of that, of what it adopted and of what the box already held of
it. A token the box does not hold (let go, or from another box), or one
naming a head its store no longer holds (a later epoch holding less, after a
rollback or a migration), is answered as to a replica holding nothing,
`since: Unknown`, which is complete because a pull from nothing is; the tab's
`Frontier` carries an era that answer moves on, read against the frontier
the question was asked from (each piece of a pull is kept standing where its
question stood), so a token from before it never wins a join again and the
fall back happens once. The module is set reconciliation over a Prodrome
store and names nothing of this deployment's (its records are its own
`frontier::Records`), so it can move to `prodrome_core::store` as is. A token only ever names what its asker holds, never more: a
pull cut short records the heads of what the piece leaves held. `GET /api/<chain>/tip`, the receipt and the `/events` frame answer
`heads`, every head sorted, never one picked. The gate decides WHO may post; the trust policy what their events
BIND. Built after the server (§8), so sync exists once, in Rust.

## 6. Laws at the application layer

Properties (`proptest`), beside the Prodrome's (SPEC §9). A stage is not done
while its law is not a test.

1. **view** = the composition of the folds it names (§4).
2. **sync is set union**: adopting the same objects in any order, any number
   of times, yields the same store; the joined store folds as the union of the
   two logs folds (the Prodrome's convergence law, through the wire).
3. **the notification decision is a fold**: `decide(objects since tip, now) ->
   Option<Push>` is deterministic and total; the watcher only interprets it.
4. **the markup escape is injective and its image holds no unescaped
   special**: injective because it has a total inverse (`decode ∘ encode =
   id`); in the image every special byte is immediately preceded by an
   odd-length backslash run — equivalently, every maximal backslash run is
   even except one ending at another special, which is odd. The image is FULL
   of special bytes (an image with no `[` would have deleted the character);
   none is STRUCTURE, so DATA (a subject, a quoted line, a composed sentence)
   is never read as markup. A property over arbitrary strings, not examples.
   A writer's MARKUP is a different door (2026-10-02, `MarkupSource::checked`):
   admitted when it compiles in the closed world, the Prodrome's
   `Project::restricted()`, whose library has no `html` and no `plugin`, so
   what any writer, the triage agent included, can produce is typst's own
   elements and nothing else; the tripwire still refuses a script href, and a
   loop is killed by the box's child timeout (`suzatary typeset`) or the
   tab's worker timeout.
5. **every row renders verified**: every row the record can hold renders as a
   block typst reads back as the block built (`every_row_renders_verified`, in
   `suzatary_core::decoration`'s tests), and every record on the live chains
   does (`every_live_record_renders_verified`, `app/cli/tests/live_blocks.rs`).
6. **the wire is the type**: serialising a generated TypeScript value and
   deserialising it in Rust is the identity, checked in the node suite against
   the generated declarations.

Laws 7–10 were the batch's (§10.6 until 2026-10-24: accepting folds as the
union, rejecting changes nothing, an ancestor-open pick is refused, accepting
twice is once); they went with the batches, since a triage write is now a card
the tap performs as any changeset.

## 7. The block

A record is shown as a typst block, `#kind(fields)[body]`, by
`suzatary_core::decoration::block::raise_block` over a `Shown` row (the record
and the fold's answers: the price, the outcome, the author). The price is the
number the core computed; typst only draws it.

THERE IS ONE TYPST GRAMMAR IN THE TREE (2026-09-09): `decoration::read`,
typst's own parser (`typst-syntax`, pinned to the binary's version; a hand
parser is a second reading of another's language), and `decoration::value`,
the one renderer. `render_verified` is the agent's containment check over
them: build the call, render it, parse it back, refuse unless it is the call
that was built. Its law is a proptest over generated rows: every row the
record can hold renders verified, and no comment run is dropped.

Until 2026-10-09 this section was the document: a Cofree-style tree of day
files over the blocks (`Doc<A> = (A, Layout<Doc<A>>)`), printed into a scratch
tree beside the typst templates and compiled into PDFs, with the round trip
`lower ∘ parse ∘ print ∘ decorate = lower ∘ decorate` as its law. The owner
no longer wanted the PDFs, so the tree, its reader `lower_block`, the
templates, `suzatary pdf`, its smoke and the web app's Doc page went; the
chain is the record, and nothing reads a block back into one.

## 8. Stages, each deleting its Python

No stage adds behaviour. Each reproduced the Python it replaced against the
tests pinning it (browser suite, conformance vectors, `tests/`), then DELETED
it in the same series. Two applications that both work is the failure mode.

0. **This document and the skeleton**: `app/` workspace, capability traits,
   wire types with `ts-rs` generation in `scripts/build-web.sh`, CI running
   `cargo test`/clippy for `app/`.
1. **`view`** in `prodrome-core` and `prodrome-wasm`; the browser's offline
   fold uses it.
2. ✅ **The server** (2026-09-07). Every route, the gate, devices and FCM, the
   watcher, briefings, mail routes over notmuch, markup rendering, TLS, static
   files, deep links; `webapp.py`, `push.py`, `notify.py`, `devices.py`,
   `nonces.py`, `tailnet.py`, `markup.py` deleted. **Evidence: a
   differential** — both servers on the live 565-object chain, byte-identical
   at a fixed instant across nineteen routes and refusals (the 929 KB chain
   payload among them); hence `refusal::shown` reproduces CPython's `repr`, so
   quoting noise could not hide a real difference.
3. ✅ **Replica sync** (§5) replaces acts (2026-09-07). One algebra,
   `suzatary_core::sync::receive`, decides a post with no I/O: rehash by
   REPRINT (a name is what the bytes are, never what a client says), refuse a
   parent nobody holds, refuse an event naming an actor other than the
   poster's, adopt parents-first, name the merge when two heads remain. §6.2
   is two proptests over generated DAGs. **The merge has no actor**: a
   `Woven` with no event asserts no fact (SPEC §3); `mk_woven` sorts parents,
   so two replicas joining the same heads make the same bytes. The phone
   seals locally (`prodrome-wasm` gains `lifecycle`, `seal`, `merge_object`)
   and folds what it holds, so nothing is optimistic. Deleted: `intents.rs`,
   the nonce store, `POST /api/act/{complete,cancel,reopen,confirm,add,batch}`,
   `web/src/{outbox,queue,optimistic}.ts`, the `since` list.
   **A replica cannot AUTHOR**: escaping (§6.4), pricing, deriving an id from
   every id the chain has mentioned, and `authoring::confirm_todo`'s two
   refusals are `suzatary-core`'s, and a TypeScript copy is a second reading.
   So `POST /api/author` builds the event and APPENDS NOTHING; the replica
   seals the result. `add`/`confirm` need the box; `complete`, `cancel`,
   `reopen` need nothing. **The Prodrome's one new primitive**:
   `EventStore::adopt_objects`, adoption over a name → print map (a wire
   replica is bytes); `adopt` is it with a directory as source (SPEC §3).
4. ✅ **The agent's tools** (2026-09-07). `suzatary mcp` on `rmcp`: the same
   twenty-two verbs, arguments and answers; authoring and `render_verified` in
   `suzatary-core`; CLI `todo add|update|show|list` and the three lifecycle
   records beside `confirm`. Deleted: `mcp_server.py`, `authoring.py`,
   `bin/suzatary-mcp`, the Python `todo` subcommands and their tests.
   **The rules became types**: `AgentCapabilities` has `Chain` and nothing
   that sends, so a send tool would not compile; `record`'s actor is
   `RecordedBy`, a one-value enum, so a forged trusted origin does not exist;
   `VERBS` is closed and its serialised `tools/list` snapshotted in
   `app/core/tests/mcp-surface.json`. **`render_verified` does not go through
   `Render`**: `Render::html` takes `Escaped`, which only `Content::Confirmed`
   yields, and agent writes are provisional. It measures the result rather
   than trusting a blocklist.
5. ✅ **The decoration** (§7) and `pdf` (2026-09-07). `suzatary_core::decoration`
   reads; `suzatary_io::build` stages and compiles; `suzatary_core::site` is
   the attestable-history site (its `Html` newtype went 2026-09-30: each page
   is Typst, `site::page::Page`, compiled by `render::Typst::document`); verbs
   `pdf`, `smoke`, `web`, `todos open`. Deleted: `document.py`,
   `typst_ast.py`, `build.py`, `web.py`, `view.py`, `todos.py`.
   `/doc/*.pdf|svg` stages through the function `suzatary pdf` calls, so a
   fetched PDF and the briefing's are one build. **Evidence**: the round trip
   on the live chain (248 rows in, 248 out) and generated rows; the staged
   tree byte-identical to the Python's on all 86 day files but for the clock.
   (The document, `pdf`, `smoke` and `/doc/*` were deleted on 2026-10-09; §7.)
6. ✅ **Mail, and the LAST PYTHON** (2026-09-08). OAuth, IMAP, SMTP, the
   corpus, `state.json`, the syncer, every remaining verb — then `suzatary/`,
   `tests/`, `bin/`, `prodrome/py/`, `ty`, `ruff`, the interpreter.
   **The account MODEL is `suzatary-core`'s; the ROSTER is the deployment's.**
   Account kinds and the notmuch query scoping one are the PROGRAM's; the owner's
   particular mailboxes are this box's. `mail::account` holds no addresses;
   `[[mail.accounts]]` in `instance.toml` carries `sync = "gmail" | "ms-imap"
   | "proton"` with that strategy's fields; `Roster::new` is the smart
   constructor holding the invariants.
   **THE TWO TOKENS ARE TWO TYPES AND TWO GRANTS.** `mail::oauth` (device-code
   flow and refresh as a state machine over values) is generic over a SEALED
   `Protocol` with two implementors: `refresh_request::<Imap>` asks for
   `IMAP.AccessAsUser.All`, `read_refresh::<Imap>` answers an `ImapToken`,
   nothing converts one token to the other. Types are not enough (Microsoft
   issues an SMTP token from a refresh token whose consent covered both), so
   the grants live in SEPARATE SLOTS chosen by `P::SLOT`; the cutover
   authorised each protocol separately. `Consent` is zero-sized; its
   constructor `given_at_a_terminal` is called only by `auth` after an
   `is_terminal` check (`given_by_tap` came with §10.3 "Acts"), so the syncer
   cannot start a flow and an expired grant is a REFUSAL naming the verb.
   **The MSAL cache is not read** (§9): an undocumented serialisation of a
   library no longer linked. The owner re-authorises once.
   **The sync is a state machine over an abstract session**
   (`mail::imap::pull`), tested against a fake server and maildir, making the
   UIDVALIDITY rule, content-digest naming and read-flag pushback checkable.
   `sync` and `backfill` are one function at two settings.
   ⚠️ **ONE DELIBERATE DIVERGENCE, AND IT IS A FIX.** `ms_imap_sync.py`
   fetched by SEQUENCE NUMBER and pushed `\Seen` with `UID STORE`; the spaces
   coincide only in a never-expunged mailbox. This is UID-based throughout.
   **The verbs answer the same bytes**: `mail::verbs` field order is a
   derived `Serialize`'s (declaration order, as `dataclasses.asdict`);
   `timestamp` reproduces `str(datetime)` (a SPACE, not `T`; the header's own
   offset), because the MCP surface is a versioned contract.
   `app/core/tests/mcp-surface.json` passed unchanged.
   **`state.json` is byte-identical**: key order, two-space indent, sorted
   hashes and senders, empty fields omitted, trailing newline —
   `TriageState::print` reproduces `save_state`; the live file (2,192
   entries, 598,582 bytes) round-trips identically.
   **Sending is the CLI's alone**, over `suzatary_io::smtp` — Microsoft
   XOAUTH2, Gmail's API on lieer's grant, Proton Bridge STARTTLS on 1025 with
   the PINNED self-signed certificate. Bcc unsupported by design.
   **Found by the live-chain suite**: `todo_add` overwrote `build_todo`'s
   midnight `created` with the clock, so agent todos did not round-trip
   (§6.5). Fixed in `agent`, stated in `Shown::as_written` (a block carries a
   displayed DATE), idempotent in `decoration::block::date_value`.
   `Operator` is native. Left in the tree: `app/`, `web/`, `android/`,
   `scripts/`, `nix/`, the typst templates, `events/`, `state.json`,
   `instance.toml`, `mbsyncrc`. The conformance vectors are FROZEN wherever
   they live: evidence of what the reference said, no generator left.
7. **After the soak ends (2026-10-02)**: device keys signing objects as a
   sidecar (`events/sigs/<hash>`), so an actor is a key rather than a network
   position. Not a stored kind; not before 1.0.
8. ✅ **The split** (2026-09-09). `prodrome/` is
   <https://github.com/fulfilium/prodrome>: a README whose quick start is a
   doctest, its own flake (`packages.prodrome-wasm`,
   `checks.prodrome-core`/`prodrome-clippy`) and CI, which run on the commit
   before this repository pins it. `app/Cargo.toml` takes it by `rev`;
   `flake.nix` pins the same sha; crane vendors from the lock, so the Nix
   build reaches no network (a `cargoLock.outputHashes` entry until
   2026-09-13; crane's `cargoArtifacts`, keyed on `app/Cargo.lock`, is shared
   by the release, the fixture, both checks and both generators).
   **The tests decided the split**: four read this box's `events/` (the
   literal round trip, `read_dag`/`read_chain` agreement, the fold pin,
   §6.7's rows); a public crate reading a private chain builds for nobody, so
   they are `app/cli/tests/live.rs` over `app/conformance/`, `test = false`
   and named by the merge gate (like `tests/guards.rs`), since the build
   sandbox has neither store nor vectors. **Lost, stated**: the forty seeded,
   forked view DAGs (§6.7 with concurrent writes and refused claims) shared a
   file with the live block and were scrubbed, so nothing pins a CONFLICTED
   row on either host. Upstream wants `conformance/view.json` and
   `core/tests/view.rs`. **Moved, 2026-09-26** (datasplit §9.2): the fold pin
   and §6.7's rows (`live.json`, `view-live.json`) are a print of the owner's
   history, so they left the code repository for the data one
   (`<data>/conformance/`); their four Rust tests and one node test stay
   here and read them there, through the instance and `$SUZATARY_LIVE_VIEW`.
   The round trip and the order stay as they were.

## 10. After the port: what is decided and not yet built

Decided with the owner on 2026-09-09, in this order.

**10.1 ✅ The generic payload (Prodrome 0.2) — done 2026-09-09.** A todo's
body, detail, source, subtodos, notes and the two markup newtypes are the
application's decoration. `prodrome-core` is generic over a host payload
(`payload::Payload`: the record constructor and printable fields; vocabulary
= core kinds + host's; the parser stays a whitelist). Suzatary's is
`app/core/src/record.rs` — `Record` with `MarkupSource`, `StringSource`,
`NoteSite`, `Source`, `Note`, `SubTodo` and the `mk_*` constructors — and the
crate is taken `default-features = false`, so the upstream REFERENCE payload
is not linked. Bytes unchanged: `app/cli/tests/live.rs` pins that every object
on this chain parses under `Record`, prints back byte for byte and hashes to
its name, folds matching `app/conformance/`; no soak restart. The parameter
runs through events, folds and view directly (no API compatibility owed);
`crate::record` names the applied types (`TodoEvent`, `Envelope`, `Authored`,
`Node`, `EventStore`) once, so an IMPORT LINE shows the payload. The wasm's
`content`/`records` JSON is the payload's fields as §2 literals;
`web/src/prodrome/core.ts` narrows the two instants and says why it has no `spec`.

**10.2 ✅ A store per concern — done 2026-09-09.** Two chains: the owner's life
(`events/`) and this project's work (`events-suzatary/`: roadmap, audit,
system issues the triage agent finds). Separate lists in the app; the briefing
reads only the first; the agent's verbs take the store they write to (system →
project, the owner's life → his). The plan lives in that store, priced with the
same specs.

*The trust policy belongs to a CHAIN, not to a deployment.* `Deployment`
keeps the BOX's facts — who may open the app, who writes as whom, categories,
the push click-through URL; `untrusted` and `roster` are
`chains::ChainPolicy`'s. `Chains` is a head and a tail, not a `Vec` ("always a
first chain" is a shape); `ChainName` is a slug newtype, so `..` is refused
where the file is read. A writer's actor a chain's roster lacks stops the
server, naming the chain.

*`instance.toml`'s `[[chains]]` is an ARRAY whose ORDER is meaning.* The first
is what bare `/api/…` paths, the briefing, the document and a verb with no
`--chain` mean. The old `[chain]` table is not parsed (§9).

*The routes take the chain as a PATH SEGMENT, not a query parameter.*
`/api/<chain>/{chain,fold,todo/<id>,tree,tip,me,objects,author,events}`: a
chain is a resource in one tree, and `?chain=` would read as a
filter. Bare paths serve the first chain, so the phone, briefing links and
scenes are untouched. Not chain-scoped: briefings, mail, the document, device
registration.

*`me` is per chain.* The login→actor mapping is the deployment's; whether the
chain's roster knows the actor is the chain's (unknown: `actor: null`, no act
controls). It carries the chain NAMES the switcher is built from.

*The watcher warms every chain and pages for the first only*: a push says
what the agent did to the owner's life, not the assistant's own work.

**10.3 The secretary.** A conversation in the app with a fully trusted agent:
a bundle wider than the triage agent's (trusted appends, may change the
repository), irreversible acts such as sending behind a tap, a model that
need not be Claude, a conversation store. It carries the chat's judgement —
reading mail and chain, proposing drafts — to the phone; the terminal chat
stays for engineering. After 10.2.

*Why it exists.* The owner, 2026-09-09: "it was nice when you were suggesting
things to draft and such to make progress". That judgement lived at a
terminal. The secretary is NOT a wider triage agent: A HUMAN IS PRESENT, so
its output is read before it is acted on, which makes trust affordable here
and unaffordable for the unattended pass.

**The five things it is, each of them a mechanism.**

*(1) THE MODEL IS A CAPABILITY.* `capability::Model`: a `Conversation`
(system prompt + `Turn`s) and the closed verb list in, a `Reply` out — text
and the TOOL CALLS IT WANTS, as VALUES; nothing behind the trait performs a
tool. `secretary::converse` dispatches each call through `secretary::call` (a
`match` from name to pure handler, like `agent::call`), appends answers as
`Turn::Tool`, and asks again, for a bounded number of rounds. An invented verb
gets a refusal naming the real ones; the AUTHORITY is all on this side. The
envelope PARSE IS PURE in the core (`Reply::parse`), shared with the scripted
fake the tests drive. First interpreter: `io::model::ClaudeCli`, the `claude`
CLI with `--output-format json` and the setup token in pass (as
`scripts/hourly-triage.sh`). **It is given NO TOOLS**: no MCP config,
`--strict-mcp-config`, a settings hook denying everything. No Messages API
interpreter: no key in `secretspec.toml` (2026-09-09), and an untestable
credential is not a code path.

*(2) THE SECRETARY IS A THIRD BUNDLE.* `SecretaryCapabilities` adds `Model`,
`Conversations` and `Briefings`, and its chain writes are the trusted actor
`assistant`, so its todos are CONFIRMED — right because the conversation IS
the review. Nothing SENDS: no `Notify`, `Whois` or `Devices`, and no sending
trait exists. It gained the right to be BELIEVED, not to act outward.
`SecretaryOnly` is the pin (like `AgentOnly`): a fake implementing exactly the
bundle, which stops compiling when a trait joins.

*(3) SENDING IS A PROPOSAL AND A TAP.* `propose_email` STORES the whole
message (account, recipients, cc, subject, body) in the conversation, shown
as a card with every field; only `POST
/api/secretary/conversations/<id>/proposals/<n>/tap` performs it. The tap
carries an INDEX, never a body: confirmed text = stored text = sent text.
`secretary::Tap` has one constructor, `Tap::of` (a `signatures::Trusted`, the
door's proof that a device key the history trusts for the actor a named
tailnet peer maps to signed the request, and the CSRF triple); the function
reaching `suzatary_io::smtp` takes one. No verb reaches
the transport (no trait) and no route does (no `Tap`). Loopback is refused by
the same `actor_of` as everywhere, keeping the triage agent away.

*(4) A CONVERSATION IS A PRODROME OF ITS OWN, NEVER TURNS ON THE CHAIN*
(2026-10-01, reversing 2026-09-08's "a conversation store, not the
Prodrome"; `docs/CONVERSATIONPRODROME-PLAN-2026-09-30.md`). A turn has no
fulfillment or lifecycle, so on the TODO chain it would be §10.1's generic
payload reversed; that still holds. What changed is that the Prodrome is
generic over a schema, and a conversation is one: `secretary::line_event`
(4a, below) is its event vocabulary, one constructor per kind of line (`Opened`, `Titled`,
`Said`, `Proposed`, `Reviewed`, `ReviewFailed`, `Stopped`, `Rewound`,
`MemorySeen`, `Session`, `Kept`, `Scoped`, `Unheard`, `Coding`), keyed by
`ConversationId`, each carrying
the entry `Scrubbed` (identity numbers, a link's credential parameters, any
JWT) and `held`, how many lines its writer held, so the same words said twice
are two objects. ONE STORE, the conversation the entity key
(`secretary-lines/`, beside `[secretary] conversations`), not a
genesis per conversation: the key already keeps conversations apart in the
fold, and the Prodrome's append reads the whole directory either way. Every
line writes the one register, the transcript (`transcript_registers`), and
rests on its frontier: a grow-only set under inclusion, union its join, so two
heads are no conflict and the next line rests on both. The readings are
`secretary::transcript`'s: the lines in causal order (ties by the instant
stamped, then the name), `Stored` (what the page draws), `owed` (equal to
`queue::delta` for one writer) and `standing`, law-tested, with two replicas
that wrote apart reading the same once each holds the other's objects. It is
PUSHED with the other stores (`scripts/safe-push.sh`), so a line is an object
only after the door. The JSON-lines files before it were carried over once,
on 2026-10-01, and the verb that did it is deleted; the data repository's
earlier commit holds them with the code that read them. The memory is a
prodrome too (2026-10-02), but its words are not objects: an immutable store
cannot forget, so the history and the text are two stores (below, MEMORY).

*(4a) THE CONVERSATIONS LOCK THEIR WORDS* (words stage 2,
`docs/WORDS-PLAN-2026-10-09.md`). The store is `secretary-lines/`, at the
schema `secretary::line_event`: a line's SHAPE in the clear (who, when, which
verbs, which references, which files, `held`, `carried`) and its words in
slots (`line_words`), each through the redaction door once, when it is
written (`line_event::seal`), and sealed with ChaCha20-Poly1305 under a fresh
key, its associated data the line's print without its words, the slot's
number and the redaction rules the words passed (`redact::Rules`), so a sealed
slot moved anywhere else does not open and cannot claim other rules. Opening a
slot under the current rules parses it; a slot under any other rules is
scrubbed as it opens, once per slot. Every change to the rules is followed by
`suzatary secretary migrate-scrub` (`secretary::rescrub`), which appends to
the same prodrome a `Resealed(of, from)` line for each slot whose words open
only under older rules, its one slot those words scrubbed under the current
rules and sealed under a new key. A reader takes a slot from its own seal
while that key opens it and from the newest reseal once it does not
(`transcript::Through`); a reseal that opens with a key held here supersedes
`from`, so the one sweep destroys
that key as it destroys an erased one (`transcript::dead`), and a tab forgets
it the same way. Nothing is deleted and the genesis does not move. A second
run appends nothing, and a run stopped anywhere leaves every line reading the
same words. The one residue: the old ciphertext stays in the data
repository's history, unreadable once its keys are gone, except by a key kept
in an earlier vault snapshot. A model turn's prose is one slot and each call's arguments
another, so a forget can take one call. `turn_shape::{split, join}` are a
lawful pair; a slot whose key is gone joins as `Turn::Erased`, its shape
whole, saying why (`words::Gone`). The keys are `secretary-lines-keys/`, one
file per key named by its SHA-256, box-local, in the daily vault, never
pushed; `transcript::push` is the one door that writes a line, keeping its
keys first. ERASING appends `Erased { keys, erasure }` and runs the sweep,
which destroys every key an erasure of the same conversation names, after
forgetting the answer blocks rendered from it (a block that may still be on
disk keeps its key for the next sweep) and the words the box keeps opened in
memory (`secretary::opened`, a cache every reader goes through, keyed by the
key's name and the digest of the slot's binding and sealed bytes, one per
keys directory in a process, holding at most 32 MiB of words, oldest out
first: below that bound, after a digest read, it is the whole store's
plaintext in the server's memory): monotone over the lines, so it takes no
hold. The owner erases a turn or a conversation behind the `Tap`
(`POST …/<id>/erase`); a forget erases by causation what its remembering
provably copied, the `remember` call's arguments and its answer
(`secretary::caused`), and the box offers the opening turn and the round's
prose on an `Erase` card, never erasing them unasked. The digest keeps an
erased line in its order and renames its leaf, so the nodes above it rename
(and, where a renamed node moves a chunk's cut, at most the chunk after it
at each level) and their summaries go in the one sweep; a carried line is the leaf
of the line it carries. The pull (`/api/stores/lines/objects`) carries the
keys of the lines it sends, never an erased one's, to any viewer login
the gate admits, the same set that read the conversations before: a revoked
device's signed read is answered 410 and its tab deletes what it holds, but
it can still pull keys with unsigned reads while its login can view, since
reads stay open to any device on the owner's login (the plan's §5.6 defers
keys bound to a device). What is pushed in the clear is the
SHAPE: who, when, the verbs called, the references (`mail:`, `thread:`,
attachment hashes), file sizes, the padded length of each slot, and a coding
session's branch slug (`claude/<task-slug>`); an erased line's shape stays
readable, and an `Erased` line says which keys went, and when. What still
holds the words: a push notification already sent, `briefings/` (until
stage 6), the server's in-process memo of rendered pages, Anthropic's
own session transcripts, and a conversation's title, which the title job
wrote from its first answer and a forget does not offer to erase. The
plain store the lines were carried from on 2026-10-11 is gone (words stage 4):
its schema, the verb that carried it and its law are deleted, and the data
repository's earlier commits hold it beside the code that read it. A carried
line keeps the name of the object it carried (`Carried::From`), which the
digest reads as its leaf; nothing writes one any more. A TAB holds the lines in
the core's worker (`web/src/prodrome/linesync.ts`), which restores, pulls and
lands them and answers every reading of them; on the device they are a store
of their own, one row per skeleton node and per key beside a head row
(`linerows.ts`, database version 7), so a landing writes what arrived. The
row is its replica and the keys the pulls gave (`web/src/prodrome/locked.ts`),
joined as a pair of unions; a key the objects name as erased opens nothing
and is owed, so whichever tab reads it, from a pull or from the row another
tab left, forgets the blocks its words rendered and then drops it, by the
box's own reading of which slots are dead (`transcript::{dead, fed}`, the
module's `Conversations.erased`), a forget that fails keeping the key until
one succeeds. Its plaintext row of the old store is deleted once
the lines' row holds objects, its rendered blocks cleared first.

*(4b) TOPICS* (`docs/TOPICS-PLAN-2026-10-21.md`, `secretary::topics`). Two
stores of their own, named by `[secretary] topics`: `secretary-topics/`, one
register per (target, topic), `Off < On`, read as an observed-remove set, and
`secretary-topic-aliases/`, one register per topic name. A topic is one to
three words and at most 24 characters (`Topic::new`). A topic's name is words,
so both stores are box-local, in the daily vault and never pushed; `suzatary
init` and the server begin them. A tab pulls both as it pulls the lines, and
`POST /api/stores/{topics,topic-aliases}/objects` takes a replica's changes
under the act guard, each a `Change` whose event is the poster's actor
(`sync::receive_changes`). The reading is `topics::Reading::of`, a function of
the two object sets, in the box and in the browser module alike. The secretary
puts topics on the round it answers with `add_topics` (the `Secretary` record
holds the stores; the target is the box's, never an argument), at most three
on a target, and a round whose model holds words that are not the owner's may
only use a topic in use, so mail never names a topic; a near twin of a topic in
use (`Topic::twins`) is refused. Every session opening is told the topics in
use with their round counts (`told::context_of`). `round:<conversation>/<turn>`
and `note:<conversation>/<turn>` are `Reference`s. `suzatary secretary topics
backfill --dry-run --plan <file>` proposes topics for the rounds no topic is on,
from a model shown the owner's words alone (`topics::backfill`), and `--apply
<file>` writes that plan as written (`topics::plan::Plan`).
`#/secretary` is every conversation's rounds as one stream under a topic bar
(`secretary::stream`, `Reading::bar`, read from the tab's replicas), filtered
by the topics the address names (`?topics=a,b`); the tab writes a topic by tap
into its own replica and posts it (`POST /api/stores/<store>/objects`).

*(5) THE APP IS THE DOOR.* `#/secretary`, in the More sheet by `nav.ts`'s
slice rule. A person's turn and a verb's answer are text through `refs.ts`
(model prose read attacker mail); a model's answer is typst's HTML block by
block through `rich.ts`, with `html` and `eval` refused (below, "Answers in
blocks"). A card shows recipients,
subject and body verbatim; Send asks once more, naming what will go; a login
mapped to no actor gets no button. `say` answers 202 and the client polls:
`/events` reports a chain tip, and a conversation does not sync as a replica
yet (the plan's stage 3).

**The verbs.** The triage agent's reads (`context`, `todo_show`,
`list_open_todos`, `search`, `show`), plus `todo_add`, `todo_update`,
`record` as `assistant`, `briefing_delta` (authored, resolved, repriced,
newly mail-sourced since a day), `propose_email`, `draft_note`, `sync_mail`
(the timer's sync started early, below). CLOSED, snapshotted at
`app/core/tests/secretary-surface.json`, with `send` and `confirm` pinned
absent.

*A surface is a closed sum* (2026-10-03, `suzatary_core::verb`). `verbs!`
declares each surface once as `SecretaryVerb`/`TriageVerb`, one empty
variant per verb with its name, description, class and schema derived from
its `Args`, and `SecretaryCall`/`TriageCall`, one variant per verb holding
its parsed arguments; every function over a surface is an exhaustive
`match`. A call is read whole where it arrives into a `verb::Request` (the
chain it names and its call), and each verb declares its `Answer` type: only
`attachments` and `drive_read` answer a `VerbAnswer`, which can hand the
model files. A transcript's tool names are read once too, as a
`secretary::tool_name::ToolName` (a verb, the memory's heading, a loaded
reference, or an unlisted name), stored as the bare string.

*Syncing now* (2026-10-03, `secretarysyncnow`). `capability::MailSync` is the
timer's own sync as an effect, interpreted by `suzatary_io::mail_sync::Timer`
over `suzatary-triage.service` and the sync's report, the state directory's
`last-sync.json` (`Paths::last_sync`); `mail idle` starts the unit through
the same door. `sync_mail` answers a sync that
finished within a minute without starting another (a sync is a join, so
asking twice is asking once), else starts the unit and answers its report or
`pending`. Its effect is `Effect::Syncs`: the sweep cannot call it and a
rewind never hides it.

**The briefing.** `suzatary secretary brief` opens a conversation seeded with
the day's delta for the secretary to answer with proposals (§10.4). See "The
brief" and "There is one conversation" below.

**The prompt is a file.** `SECRETARY.md`, through `Manuals` like
`OPERATOR.md`: low fulfillment is urgent, deadlines bound when not stakes,
`waitingOn`, the chain routing rule, the email voice, never send and always
propose. A prompt in a binary cannot be read in a diff.

**✅ Built 2026-09-09. What the build settled, and what it left open.**

- *A HANDLER is concrete*, so the bundle cannot hold the send rule alone —
  hence `Tap`. `app/cli/tests/writes.rs` pins ONE file naming the transport:
  `commands::mail::deliver`, which account sends how, used by `send` and the
  secretary's postman alike.
- *`ServerCapabilities` is fourteen traits, a strict superset of
  `SecretaryCapabilities`* (the server runs the secretary); each route still
  asks for its narrowest bound.
- *"Still answering" is DERIVED*: the last turn is a model turn with no calls,
  the one shape the loop returns on, so every exit (round bound, failure)
  writes one. A flag would be a second thing to keep in step.
- *The chain is resolved PER CALL*: the dispatcher takes every chain's bundle;
  one bound to a chain would answer `chain="suzatary"` about `life`.
- NOT BUILT: the Messages API interpreter. (`Proposal` began as `Email`
  alone; `Changes` and `Act` came below.)

**✅ 2026-09-10: A TURN CARRIES WHAT IT POINTS AT, AND A PROPOSAL CARRIES ITS
REASONS.** The owner: "being able to respond or reference different parts";
"maybe with the email there should be some sort of explanation and sources
around it".

*(1) A REFERENCE IS ONE VALUE, AND ITS TEXT FORM IS THE PROTOCOL.*
`suzatary_core::reference::Reference` = `Todo{chain,id} | Mail{hash} |
Thread{id} | Briefing{date} | Proposal{conversation,index}`, one arm per app
DESTINATION. One text form (`todo:life/flightcheckin`, `mail:<hash>`,
`thread:<id>`, `briefing:2026-09-10`, `proposal:2026-09-09-1/1`), because A
MODEL WRITES TEXT: print and parse are one grammar, round-trip proptested,
shown in `SECRETARY.md`, tokenised by the app. Every part is PARSED
(`ChainName`, `TodoId`, `ConversationId`, two new newtypes), so a reference
cannot name a path. The CHAIN is in a todo's name (ids are per store, §10.2).
A mail hash may be a PREFIX with `view.sourced_from`'s six-character floor.
The store writes the text form (read in a pager); the wire a TAGGED union
(`wire::Reference`), so TypeScript never re-parses the grammar; `TryFrom`
goes through the SAME `FromStr`.

*(2) THE BOX LOADS WHAT A TURN POINTS AT, THROUGH THE VERBS.* `Turn::Person`
carries `references`; `secretary::load_references` resolves each before the
model is asked, as ordinary verb turns via `todo_show`, `show`, `search` (a
thread), `briefing` — NO SECOND READER; an unresolvable reference gets the
model's own refusal. Loading spends no rounds (`MAX_ROUNDS` bounds a model
asking). This made `Briefings` actually join `SecretaryCapabilities`, and the
`briefing` verb makes that authority visible to the snapshot.

*(3) A PROPOSAL SAYS WHY, AND WHAT IT RESTS ON.* `propose_email` requires
`why` and `sources`; `store::Reasons` (private fields) refuses either empty,
deserialisation included, so a hand-edited transcript cannot fold into a
groundless proposal. The card shows the why ABOVE the body and sources as
LINKS.

*(4) A PROPOSAL CAN BE DISMISSED, AND THE DISMISSAL IS A TURN.* Else it stands
forever and the secretary, reading the TRANSCRIPT, never learns the answer.
`POST …/proposals/<n>/dismiss` takes the SAME `Tap`: a `Tap` WITNESSES that
the owner asked, it does not permit sending, so ANSWERING A PROPOSAL IS ONE GATE.
It writes `Entry::Dismissed` (folded as `Outcome::Dismissed { reason, at }`)
and a `Turn::Act` — not a person's turn, since `thinking` derives from the
last turn and a person's would poll a model nobody started. `suzatary
secretary dismiss` is the operator's twin (actor off the roster); there is NO
CLI twin of the TAP: sending is a press on a device the box can name.

*(5) THE APP LINKS WHAT THE MODEL NAMES.* `web/src/chat/refs.ts` tokenises a turn
for the five spellings, building text nodes and anchors through `dom.ts`;
`rich.ts` STAYS THE ONE DOOR, uninvolved. The tokeniser is the SECOND reader
of the grammar and says so; the Rust is the authority. It does not match a
bare `todo:<id>` (typst's spelling, rewritten by `rich.ts`, naming a todo on
whichever chain is open — the ambiguity `Reference::Todo` carries a chain to
avoid). "Ask the secretary" (todo view, message, thread, briefing) and Reply
(a proposal card) attach the reference as a removable chip carried in the HASH
(`#/secretary?ref=…`), a sendable URL. The list page continues the NEWEST
conversation. Reply on a TURN quotes: a turn is not a destination, and a
reference the reader cannot follow leads nowhere.

**✅ 2026-09-11: CHANGESETS, AND THE MORNING BRIEF.** The owner: "the secretary
should make a nice changeset for me and put it in the review section for me
to confirm or deny, or respond to"; and "there's no need for the main session
to send a briefing at all."

**Changesets.** A THIRD write path, beside writing as `assistant` and
proposing an email. `propose_changes({chain, acts, why, sources})` stores a
`Proposal::Changes(Changeset)`: `acts` an ARRAY of `{act, todo, …}` (`add`,
`update`, `complete`, `cancel`, `reprice`), `why`/`sources` under
`store::Reasons`; performed by the same `…/tap` with the SAME `Tap`. The acts
append to this deployment's chain as `assistant` through the SAME
`authoring::build_todo`/`update_todo` and `mk_*` a direct write uses
(`suzatary_core::todos::render_current` verifying, as `agent::writes`): no
second authoring path.

⚠️ THE ACTS ARE STORED, THE EVENTS ARE NOT. An `Authored` is a SNAPSHOT, so an
event built at proposal time would undo whatever moved since. The DIFF and
the TAP both build against the chain AS IT STANDS via `changes::events_of`.

⚠️ AND A REFUSAL APPENDS NONE. The whole changeset is built BEFORE the first
append, under the write lock; a taken id or unknown todo refuses naming the
act's position, and nothing lands. Not a transaction (the Prodrome wants
none): a disk failure between appends leaves the earlier ones, named in the
refusal. Success is `Outcome::Applied { objects, chain }` (a sibling of
`Sent`, naming OBJECTS, undoable one at a time); failure `Outcome::Failed`.

The card is `secretary::diff::shown`, the one renderer of a changeset, whoever
asked for it (a triage card included, §10.6): five row kinds — new todo,
completion, cancellation, repricing, update — in the same before/after
columns; a second renderer could start eliding a field. A row is an act's
POSITION and the event it would be; a changeset is confirmed WHOLE, and acts
that no longer build show a `problem` line, not a dead Confirm.

`SECRETARY.md`'s "Writing directly, or proposing a changeset" is the rule, a
judgement: write directly for routine, evidenced, undo-able work; propose for
a real commitment of his or anything unsure — a new todo at or under 45%, any
change to a todo HE wrote, money/legal/medical, a cancellation, a
repricing across rows. "When in doubt, propose." Distinct from §10.6's
"Reviewing the agent's cards" (someone else's writing), though both land in
the same review section.

**Acts (2026-09-14; project chain `secretaryacts`, plan
`docs/SECRETARYACTS-PLAN-2026-09-14.md`).** A FOURTH kind, the first the BOX
performs: `Proposal::Act(Act)`, `secretary::acts::Act` a CLOSED sum with one
arm, `Auth { account: EmailAddress }` — `suzatary auth`'s device-code sign-in,
asked for from the chat (re-consenting a Microsoft account needed a shell
the owner's phone lacks). `propose_act({act, account, why, sources})` uses
`Reasons`; the account is checked twice (address shape, then THIS roster);
the vocabulary is a `match` with no free-form arm, so no shell command can be
spelled into it. The card shows `Act::sentence`, carried ON THE WIRE, with
Run and the usual two-press confirm.

`routes::tap` hands `Consent::given_by_tap(&Tap)` to a `Perform`, a trait IN
NO BUNDLE held beside `SecretaryCapabilities` like the postman. The consent
has exactly two constructors, `given_at_a_terminal` (claimed by `suzatary
auth` alone) and `given_by_tap` (the tap route alone); `app/cli/tests/writes.rs`
walks the tree to pin both lists. `Acting` is the binary's performer: `Real`
splits a synchronous REQUEST (so the tap answers `Outcome::Started { at,
expires_at }`) from a POLL on a bare `std::thread` behind a deadline
(`expires_in + 2 × interval`, ceiling twenty minutes, a channel with
`recv_timeout`, not a sleep race); `NotYet` is a `Failed` saying no performer
is built. The conversation gets `Turn::Act` lines written as `assistant`:
Microsoft's `message` (URL and user code, never the `device_code`) with the
expiry, then "imap grant stored for <account>" or "the act did not finish:
<reason>", recorded on the proposal's object in the inbox (below). A second
tap on a started act runs nothing. `refs.ts` links an `https://` token ONLY in an act
turn or a line of the box's. The line relaying Microsoft's message points at its card
(`proposal:<conversation>/<n>`), so while the act runs the card carries it
(`ProposalRow.announced`) and a tap in Settings reads the link and the code there
(2026-10-17, audit S22). The box's lines are drawn as the box's (`TurnWho::Box`), never
as the owner's act, and wake no model (`queue::wakes`, audit S3).

⚠️ A RESTART FORGETS THE CODE, AND SAYS SO. Only `message`,
`verification_uri`, `user_code`, `expires_in` may be held (plan §5), so a
`Started` act cannot resume: `acts::reconcile`, once at `serve` startup before
the watcher, closes each with `Failed` and an act turn saying to ask afresh
(idempotent). The browser suite uses a RECORDING performer behind the
`fixture` feature, so no scene reaches Microsoft.

**The inbox (2026-09-28, plan `docs/PROPOSALS-PLAN-2026-09-28.md`; its own
schema since 2026-10-01).** A proposal is a Prodrome object in its OWN store,
at its OWN schema (`secretary::inbox_event::InboxEvent`; `[secretary] inbox`,
default `inbox/`, a
`Genesis` labelled `inbox` and then `Change`s; never `life` or `suzatary`).
`Asked` carries the act, `why`, `sources`, the conversation turn that asked
it, the todo it `serves` and its spec, kept as data.

A REMEMBER CARD NAMES ITS WORDS, NEVER HOLDS THEM (since 2026-10-03, the
owner's option 3 of the `secretary-memory-erasable-wtxzet` report).
`Proposal::Remember { text, kind }` carries the memory's own `TextName`; the
words are a salted text in the memory's text store (below), kept from the
moment the card is asked (`memories::Held::keep_card`, which names the text a
card already keeps word for word, so the same fact asked again is the same
card). A tap remembers the card's own text (`Held::remember_kept`), so the
card and the fact it became share one file. The card reads its words through
the `Dress`'s memory (`secretary::readings::words`): `GET
/api/secretary/memory` carries every card's words its text store keeps, by
name (`MemoryPayload.cards`), so the box and the tab's replica draw a card
alike (`wire::RememberWords`: `kept { fact }`, `gone {}` with no Remember to
offer, or `unread {}` where the reading was given no memory). They are deleted by the memory's ONE sweep, which reads
the inbox as one more naming (`secretary::texts::dead`): a text goes when
something names it and nothing live does, live being a standing fact or a card still
open. Every dismissal runs it (the route's, the CLI's, a withdrawal, a
supersession), and so does every forget. A remember card is asked under the
memory's hold and then the inbox's, and a remember tap takes them in the same
order, so the sweep, asked under the memory's hold, never decides about a
text a card is about to name. The store before this one,
`secretary-inbox-proposals/`, whose cards held their words in their act, was
carried over once on the box (2026-10-03; each card kept its id, which is the
hash of its act as first asked, words included), and its verb and reader were
then deleted.

Its name, a `ProposalId`, is its CONTENT. `Proposal::recurs` is the one rule.
An effect whose repetition does harm (an email, a changeset, a fact, a push, a
coding task) is its act alone, so the same email proposed in a later round is
the same object. An act meant to recur (a sign-in, a secret, a web or document
read) also includes the instant it was asked.

The conversation holds only `Entry::Proposed { object, revises }`, and the
card reads the object's registers. `Asked` writes the CONTENT register
(discrete). Each response is its own constructor with its own fields and
writes the STATE register (`secretary::phase::Phase`), ordered by "further
along" and declared INFLATIONARY, so the store refuses a response below or
beside the reading it supersedes:

- `NotSent { attempt, reason }` and `Started { attempt, began, expires_at }`
  are TRIES, numbered: the code goes from a running flow back to "not sent"
  and from "not sent" to a running flow again, and the number is what makes
  that a climb. Every try is below every settled phase.
- `Sent`, `Applied`, `Acted`, `MayHaveGone` and `Dismissed` settle it;
  `Checked` (the owner checked a send that may have gone) is above
  `MayHaveGone`. The settled phases are otherwise incomparable: sent on one
  device and dismissed on another, or sent and may-have-gone, is a CONFLICT,
  shown and never tiebroken.

The reading is the state register's maximal write (with the send a check
checked, found by ancestry); the responses in causal order are only what a
card counts as tries. Nothing is ordered by the instant a response carries.

`inbox::submit(id)` is the one way a tap performs anything. "Is it still
standing, so I may perform?" is the inbox's ONE non-monotone question, and
it is asked only there, under the `Prodrome::locked` hold (an `flock` on
`submit.lock`), against the reading: perform while nothing settled it (or its
running flow went stale), else return the recorded response. Nothing sent
records a try, so a resubmit runs again. A proposal in conflict is performed,
dismissed and checked by nobody until a response descends from both. Every
other reading is monotone in the objects and takes no lock.

A send that may have gone stays in the Inbox, marked to check Sent, until the
owner acknowledges it: `POST …/proposals/<n>/acknowledge`, behind the tap's
gate, appends `Checked`. It is never sent again, before or after.

`app/core/tests/inbox.rs` is the law: any interleaving of submits, resubmits,
dismissals and acknowledgements, concurrent ones included, performs each effect
at most once; `app/core/tests/inbox_schema.rs` holds the order's laws (a
partial order, every transition the code makes climbs, the real append refuses
one that does not, a union reads the join). A proposal is priced by the todo it
serves (2026-10-04, the Prodrome's design §6.3.1 and SPEC law 41): the inbox's
schema has a valuation, a `Price` alone (no `History` or `Bind`, which carry a
todo's history and environment), each candidate of the content register
pricing as `RefIn(<chain>, <todo>)` for what `serves` names and as its own spec
only where it serves none (every writer files it unpriced). `inbox::value`
links that term through the todo chains read as ONE set of prodromes
(`suzatary_core::chain_set`: every prodrome of each chain by its genesis, a
legacy chain's by its least root as `nest::Set` keys it, each read by
`fold::member` under its own chain's policy, each chain's name declared for
its writers' prodrome, by the same `chain_set::home` the writer appends
into). A member that does not read, a store error, two geneses or a term
that does not link, is held as that failure: a term referring into it reads
the failure or `∅`, and nothing else moves (the law test adds such a chain
beside every case). `chain_set::Read` reads each store once per request and
holds each member's nodes and policy, so the set is a function of the
objects alone. Every row is priced through the same
set (`ChainSet::entries`, the Prodrome's `view::entries` re-linked across
it: the box's fold, todo, tree and mail routes, the document, `todos open`
and the tab's list), so a proposal's value is its todo's row's at every
instant, whatever the todo's term refers to, and moves exactly when that
row does (`app/core/tests/inbox_prices.rs`, over terms with `Ref`, `RefIn`
and `After` and two chains under different policies); a chain or a todo the
set does not hold reads `∅`. The agent's and the secretary's text verbs
still price a todo within its own chain: the agent's record holds one
chain. The Inbox lists them ascending in that value, and the briefing's
`proposals_waiting` reads the same list (`readings::waiting`).

The store before this one faked a proposal as a todo on one sealed chain. It
was carried over once, on 2026-10-01; its reader and the verb that moved it
are deleted, as is the earlier verb that filed conversations' proposals into
it, and the data repository's earlier commit holds it with the code that
read it.

**The brief.** `secretary brief` OPENS WITH WEATHER: a `weather` verb, one
line with the umbrella verdict, over `suzatary_io::weather` and
`suzatary_core::weather` (`suzatary weather`'s machinery, via the trait); city
and timezone offset are `[persona]`'s (ROADMAP.md "2.0: a second host").
`Weather` is on `SecretaryCapabilities` and `ServerCapabilities`, NOT
`AgentCapabilities`: it reaches the network (a public feed, no credential,
sending only a committed city name), and the unattended pass still reaches
nothing. What returns is DATA, never instructions.

`SECRETARY.md`'s "The morning brief": weather; what changed since the last
brief (new, completed, repriced, newly urgent, agent-authored named as such —
from `briefing_delta`, not the priced list); good news from the mail; the
secretary's OWN review summary (`Reviewed` lines, §10.6); waiting changesets
and email proposals; two or three moves OFFERED, never done, as proposals
where concrete. NO NEWS: no web tool here, so the brief says so;
`{{persona.news_filter}}` names what a news-reading interpreter would surface
(ROADMAP.md; unbuilt).
It prints the conversation id on its own line; `--json` answers
`{"conversation": "<id>", "url": "<deep link>"}` for `suzatary notify
brief <id>`, whose deep link is built like `notify briefing <date>`'s, in
the ONE place (`capability::Push::printed`) deciding what a push says; a
brief that failed is `notify no-brief`.

**Retired the same day: the assistant's chat briefing.** Per
`briefings/README.md`, nothing new is written under `briefings/`;
`/api/briefings*` keeps serving the old files. The secretary's brief is the
briefing.

**There is one conversation, and it remembers (2026-09-14; project chain
`onechat`, plan in `docs/ONECHAT-PLAN-2026-09-13.md`).** The owner: "I want to
have 1 default chat where the agent knows everything." *(1) A session beside
the stateless turn.* `capability::SessionModel: Model` — `open` with a session
id THE BOX MINTS before the process starts, `resume` with only the turns since
the last answer — via `ClaudeCli` over Claude Code's session
(`--session-id`/`--resume`, stream-json parsed per line). The Messages-API
interpreter has none, so the bundle answers `Sessions::sessions() ->
Option<&dyn SessionModel>` and the worker matches (a `bool` was refused). The
session is a line in the file (`Entry::Session`); a lost one re-opens over the
whole transcript, once. *(2) The transcript is the queue.*
`secretary::queue::delta` derives the undelivered turns from the last answer's
position: exactly-once by construction. One worker per conversation, nudged
with no payload, runs until nothing is pending. Send is never disabled;
`MAX_PENDING` refuses in a sentence. *(3) The partial is never a record.* The
answer streams over the conversation's SSE endpoint as `state`, `delta`,
`turn` frames; the partial is one `textContent` node, and `turn` refetches the
folded payload, so a proposal is drawn only from what was stored. *(4) A fixed
default.* `store::DEFAULT`, opened lazily by the read gate; `#/secretary`
forwards into it; `secretary brief` writes the morning into it (`--new` keeps
a dated one). The fence rides every launch; an argv test pins that `open` and
`resume` carry it and nothing widening it (Claude Code snapshots the system
prompt per session).

**The chat, tightened, and the secretary as a layer (2026-09-15; project
chain `chatshape`; plan `docs/CHATSHAPE-PLAN-2026-09-15.md`).** The owner: "make
everything tighter", "have the chat available everywhere with context relevant
to where I click from". THE RULE: the transcript is the record; the page is
the conversation. Every line stays stored; the page draws ROUNDS — opening
turn, one folded trace of status lines and verb answers (refusals marked),
the answer, cards where proposed (`Standing` carries `after_turn`, turns their
`at`). The model is told the same (SECRETARY.md): a turn with calls is STATUS,
one line; without calls it is the ANSWER; an empty `say` with no calls is
legal. A tap, dismissal or performed act nudges the worker like `say`; a
`working` frame names the verb about to run; an answer while nobody holds the
stream pushes once, with its first prose line. Windowed by a `before` cursor
(`first`/`total`), DOM keyed by absolute turn index so answers append (day
separators, scroll and send keys: the plan).
THE LAYER: context is where you are, never typed. `?about=<reference>`
restricts the conversation to rounds carrying or naming a thing — filtered,
then windowed; the round boundary is defined once (`secretary::about`) and
pinned against the page's grouping over one shared fixture; rows carry their
absolute `index`. On a phone an Ask button opens a SHEET with the same page
and the thing in view as a chip; at 62rem+ a PANEL beside the content, open by
default, remembered per viewer. One conversation, one implementation, three
frames. What is in view is a closed sum over the `Screen`
(`web/src/chat/inview.ts`); naming nothing gets the whole conversation. An act may
carry ONE LINE reaching both records: the event's `note` and an act turn that
wakes the model. Done, Cancel, Reopen are sealed by the phone, so their line
is heard when the box ADOPTS the object (§5).

**Many chats, one memory, a chat as good as the best (2026-09-25; project
chain items `multichat` and `chatux`; plan `docs/CHAT-PLAN-2026-09-25.md`).**
MEMORY (a prodrome with erasable text since 2026-10-02,
`secretary::memories::Memories`): two stores read as one. The HISTORY,
`[secretary] memory` (default `memory/`), is a `Genesis`-rooted prodrome at the `secretary::memory_event::MemoryEvent`
schema, keyed by `FactId`: `Remembered` names a fact's words by the SHA-256 of
a salted text (`secretary::salted_text::Text<Fact>`) with its kind and who said it,
and `Forgotten` names the id and the version it made. Each writes the fact's
one register, its `Life` (`secretary::fact_life`), remembered below forgotten
and declared INFLATIONARY, so the store's append refuses a remembering of a
forgotten fact; the history's reading, `secretary::two_phase::TwoPhase`, is a
two-phase set (remembered minus forgotten) ordered by inclusion, a function of
the object set (laws in `core/tests/memory_schema.rs`). It is pushed with the
other stores. The WORDS, `secretary-memory-texts/`, are one file per standing
fact or open remember card named by that hash (`suzatary_io::texts`),
box-local, backed up by the vault and never pushed. Forgetting writes the `Forgotten` object and then
deletes the words (and any a failed deletion left), so the history still says
that fact N existed and was forgotten on a date, by someone, while its words
are gone; the salt goes with them, so the name confirms no guess. A fact whose
file is missing, forgotten or lost, reads as absent, never as an error.
THE DIGEST (since 2026-10-06, MEMORYDIGEST stages 3 and 3b): every
conversation's lines, as one set, are the leaves of ONE summary tree, the
Prodrome's `digest` (a balanced tree in the crate's causal order, so its shape
is a function of the set), read by `secretary::digest`. What was said of a
node is the history `[secretary] digests` (default `digests/`, pushed), a
prodrome at `secretary::digest_event::DigestEvent` keyed by the node's content
name, one discrete register per node: `Summarized` names a summary's words by
a salted `TextName` in `secretary-digest-texts/` (box-local, never pushed, a
`Text<Line>` store of its own), `Retracted` says one is wrong, and two
summaries of one node are a conflict shown whole (a single-record reading
takes the least object name). A node's `Label` (taint and kinds) is the join
of its lines' (`taint::line`), never a model's word. A node is named by its
LIVE lines, so a late line, or one the owner excluded from the digest
(`Erased`, dropped from the causal order of every line, so no other line of
any conversation moves),
renames the spine above it, and the summaries of renamed nodes are past in
the digest's `Naming<Line>`: the one sweep, under the memory's hold, deletes
their words with no case of its own. EXCLUDING IS NOT ERASING: the line's
words stay in its conversation object, which is pushed, in the data
repository's history and in every tab's replica, and no page or verb offers
the act; erasing a line's words needs them box-local and salted like a
fact's, a stage of its own. A `Retracted` or an `Erased` binds only from the
deployment's writers (`digest_policy::Owners`, an allowlist): only a `Tap`
constructs either, and the digest store's door refuses one written as anyone
else, on append and on receipt, so a forged one is never stored or pushed. A line's taint is its round's so far
(`taint::rounds`): an answer after a mail read is tainted. The sweep,
`Held::summarize` (the one writer of a summary) and `Held::exclude` run under
the memory's hold and refuse while any store they read left an object out
(`Prodrome::excluded`). A sweep that follows a write (`forget`, `dismiss`,
withdrawing or revising a proposal, `exclude`) never fails the write: the
write and its own lines stand, a refused sweep is logged, and the next sweep
deletes what it did not. THE COMPACTOR (stage 4) is a rule in the box's
engine (`secretary::compaction`): pending non-empty means compact the most
pending node, asking a `capability::Compact` (words only, no verbs) with the
clean view as context and the node's children as id-free lines, five calls at
most, each call first recorded as an `Asked` (no register) and refused past a
cap, and record a `Summarized` (with the node's taint) or an `Unfit` (model
and limit); its budget is a fold over the `Asked` objects, and it runs only
when `[secretary] compact = true`. THE RECORD (stage 5): a model with no
session, and a session opening, is told `secretary::told::context_of`, a
function of the objects, the budget and the asking conversation's window: the
facts whole, standing instructions first, then the view of every conversation
under 128 KB in the owner's own words alone (`Label::is_owners`), a part a line
led by its handle, a summary of his words fenced `derived:`, everything else by
kind, runs merged (`digest_view`, the one rendering the compactor's log shares
under `Label::is_clean`), and then the conversation's last 60 live turns, which
the record leaves out. `zoom(handle)` (`told::zoom`, over `Digest::cover`)
opens any span, and its answer carries its nodes' taint, which `taint::turn`
reads as the answer's class. A model's answer joins its round and what its model
still holds (`queue::held` over the `Scope` the asks recorded on `Scoped` lines:
what `queue::for_model` sends and, for a kept session, every live turn from the
first its opening sent, recorded then on its `Kept` line, never derived again;
every stored turn for a transcript), and remember's gate asks the same. A turn
said past a kept session (a model holding no session sent the whole window, or
the box ending the round) writes `Unheard` for that session, which ends it, so
the next held round opens one with the whole window rather than resume one
that never heard it. An act's
sentence never names what it acted on; that travels in its references, and a line
pointing at a todo nobody trusted confirmed is stamped `Stamp::Unconfirmed` where the
box writes it (`stamp::pointing`), so it reads tainted. A said
line's `by` is a typed `stamp::Stamp`, an actor's or the box's (`Entry::noted`),
read clean; words relayed from elsewhere stay unstamped.
`Fact::new` is the one door, for a text read back as for one written; the
secretary writes through `remember`/`forget`, and a `remember` after anything
tainted in its round is a `Proposal::Remember` card instead
(`secretary::taint`, total, deny by default). The writes' non-monotone
questions (room under the cap, a duplicate, does the id stand, the next
number) are asked under the history's `memory.lock`; taint is asked of the
round's turns the call already rests on, which only gain taint. Each
conversation is told what moved since its `MemorySeen`, once, by derivation.
The JSON-lines `memory.jsonl` it replaced was carried over once on the box
(2026-10-02), and its verb and reader were then deleted. RECEIPTS (since 2026-10-02): `remember` answers the fact's id and kind,
never its words, and a `remember` turn's receipt (`wire::Receipt`, a sum)
takes only the id from the answer and the words from the memory: `standing`
with them, `forgotten` with none, `unread` when the memory will not read or
holds no words for a fact that is not forgotten. So a forgotten fact's
receipt says it was remembered and forgotten and never what it said, an
answer stored before included. A conversation that quoted a fact still keeps
its words: the owner's turn, the model's call arguments, an answer that
quoted it and a stored answer from before. Forgetting deletes the memory's
copy and the card's, not those (the `secretary-memory-erasable-wtxzet` report
names every one, and the options for erasing them; an inbox card's words are
erasable since 2026-10-03, above). TOPICS: `Opened.kind`, `Titled` (last
wins, so rename is an append), a list and a search over person turns and
answers. STOP: `POST …/stop` pulls the running round's `Halt` and answers once
the round has ended; the round, not the route, writes `Entry::Stopped` (an
act, and the end of its session, known by its stored line), which closes the
question by itself (`Stored::closes`), so in one write it is never asked
again. Stored conversations hold an empty model turn after it, from before
2026-10-02; it reads as part of its stop. ASKING AGAIN: `Entry::Rewound { to }` sets the last round aside —
kept, indexed, never delivered; `Stored::live` is the path everything that
delivers or decides reads — and `Stored::rewind` refuses a round that wrote
anything, because a rewind never hides a write. ANSWERS IN BLOCKS (the owner:
Typst for most of this): `secretary::answer` cuts an answer with typst's own
parser at top-level blank lines into blocks whose sources concatenate to it;
each is rendered on read as typst's HTML (MathML for equations) through
`render::Typst` — the same compile, caches and tripwire as a todo's body,
rooted at an empty directory — and parsed by `rich.ts` alone. The model
writes this Typst and may not reach raw elements: a block naming the `html`
module or `eval` has no `Renderable` and is text, as is one that does not
compile. STREAMED BY NAME: the worker closes blocks as the reply streams and
sends `block { n, sha, source }`; the page asks for a rendering by sha, never
by text, so it cannot make the box render what the model did not write. THE
PAGE: the answer's place (`answer.ts`, a sum stepped by frames) is the last
row — Queued, Thinking, what is being checked, the answer landing, Stop
beside it, Regenerate and Edit once the box offers them; `anchor.ts` holds the
box (a send puts the question at the top; nothing that grows moves it);
copy per answer and block; a draft per conversation. Nothing is drawn as an
image any more: the whole-turn SVG route (`…/turns/<n>/render?theme=`,
chain item `typstanswers`) went with typst-first step 6, and a day's briefing
is `GET /api/briefings/<date>` — that day's brief in the same HTML blocks, or
a saved file and its rendering.

**The web app's contract (2026-09-16; project chain `webalgebra`; plan
`docs/WEBALGEBRA-PLAN-2026-09-16.md`).** The owner: "I want the quality of the
webapp to be the same" as the Prodrome's: a contract with laws, sums with
constructors as the only door, folds as monoid actions, a pure core under an
effectful shell, properties and frozen vectors. `docs/WEB-SPEC.md` is that
contract (the types, the three doors `dom.ts`/`rich.ts`/the decoders, laws each
naming its test, vectors). A page is a closed `PageState`, a pure `step(state,
event)` and an idempotent `draw(state, root)` over one `Source`
(`web/src/algebra/source.ts`), never closure flags; properties run over
`web/test/gen.ts`; Rust vectors under `app/core/tests/web/` must agree with
the pure modules (a disagreement is a finding, never a refresh); the `as`-cast
ceiling only moves down. Unchanged: the doors, `textContent` for the
untrusted, the pinned generated wire, one evaluator in wasm, no framework, no
npm.

**10.4 The briefing is the delta.** It reports what changed since the last
one, what became urgent, good news from the mail, and two or three proposed
moves; the priced list is the app's job.

**10.5 ✅ Undo is an append — done 2026-09-10.** The chain never rewrites, so
undo appends the INVERSE of the todo's last event; the folds read the event
set (§6.1), so the register folds are unchanged.
`Completed`/`Cancelled` ↔ `Reopened` (which one it cleared is read off the
stream); `SpecRevised` → the spec in force one event earlier; the stream is
causal, and among a todo's writes to different registers (which a `Change`
leaves unordered) ordered by the instant stamped; `Authored`
→ a re-append of the PREVIOUS content record via `authoring::revise` (so
escaping and the round trip hold); `Created` cannot be undone (the id would
dangle) and the refusal names a cancel.

UNDO IS AN INVOLUTION over a todo's stream, so `redo` is `undo` again after
one check: the last event is a redo target exactly when its freshly computed
inverse is the SAME ACT as the event before it, ignoring `at`, `actor`,
`note`. No stored marker.

TWO ROUTES, ONE FUNCTION: `suzatary todo undo|redo <id>` (human-only, like
`confirm` and `send`) and `POST /api/author` with `{"act": "undo"|"redo",
"todo": "<id>"}`, appending nothing — the phone cannot invert a `SpecRevised`
or `Authored` (§5's reasoning), so one path through the box beats a local
lifecycle-only shortcut.

NEITHER AGENT SURFACE GAINS A VERB: a triage claim's inverse settles nothing;
the TRUSTED secretary must not, because the last event may be THE OWNER'S OWN.
Both manuals say so; the verb-list snapshots fail if `undo`/`redo` appear.

**10.6 An agent's writes are cards in the Inbox, performed only by the owner's tap.**

*Since 2026-10-24 (the owner: one path for every change that waits for his tap).* A chain change
that waits for the owner has one shape, a `Proposal::Changes` card, and one performer, the tap
(`changes::apply`, as `assistant`). The triage agent's `todo_add`, `todo_update` and `record` are
the secretary's verb types (`agent::verbs`, generic over `agent::Holds`): each builds one
`changes::Act`, and the surface's `Holds::write` decides what happens to it. The secretary
performs it at once. The triage agent files it through `secretary::filed::TriageCards`, the one
door its call holds, as a card asked by `Proposer::Triage` (stored `Triaged`) in the `triage`
conversation, after building it against the chain as the tap would. A later write about a todo
that an open triage card names joins that card and replaces it, so the Inbox holds one card per
todo. The agent's record holds its chain as a `store::Reads`, the read half of `Prodrome`, so no
append or accept is reachable from it (`capability::Agent`'s `compile_fail` tests); the box
gives it `suzatary_io::reading::Reading`, and its replica (`events-triage/`) went with the
batches. The one performer, `changes::apply`, takes the secretary's record, never the agent's. The secretary's review lists the open triage cards (`triage_cards`) and
may withdraw one (`triage_withdraw`); it cannot perform one. No batch was standing when this
was deployed (the carry's dry run on the box found none), so the migration that would have
carried them into cards was deleted unrun.

What follows is the design this replaced, kept as the record of why: the replica, the batch, and
accepting as the restricted join.

**10.6 (until 2026-10-24) An agent's run writes to a REPLICA, and accepting is the restricted
JOIN.**

The owner, 2026-09-10: "an agent's changes are actually their own mini prodrome
and we can accept the changes by merging them in or not, rather than have
every todo be something we confirm or deny."

*What it replaces is a CEREMONY, not a rule.* Triage writes were CLAIMS on the
chain (unbound under §5's untrusted policy, tagged `(unconfirmed)`, each
confirmed by `suzatary todo confirm <id>`): the right question, the wrong
UNIT. A run is one decision over one inbox, confirmed as a dozen; and a
declined claim is not refused, it stands forever.

*What made a better unit available is §5.* Once the phone is a replica,
another writer's work held apart until taken in is an existing shape
(`store::accept`, `sync::receive`'s laws). The triage agent is the
second replica.

**The five things it is, each of them a mechanism.**

*(1) THE AGENT WRITES TO ITS OWN STORE, A REPLICA OF THE CHAIN IT PROPOSES TO.*
`[[chains]]` gains `proposals`, one directory per chain — `events-triage/` for
`life`, `events-triage-suzatary/` for `suzatary` — since a proposal targets
ONE chain. Before a run the replica is SYNCED from the target (the Prodrome's
`store::sync`, the join); `record`, `todo_add`, `todo_update` append THERE, each
the Prodrome's `Change` over its todo's heads in the replica (2026-10-03,
before which a run sealed legacy objects one on the last, one head beyond the
target). Runs stay STATELESS and nothing about a run is kept: no head, no ref,
no receipt. The agent READS the replica (target + what stands) on purpose:
`list_open_todos` must stop re-proposals and `todo_add`'s duplicate-id refusal
covers pending ids; and what it read is what it rests on, so a `record` on a
still-proposed todo rests on that proposal, and the Inbox says so on the row.

*(2) A BATCH IS WHAT STANDS ABOUT ONE TODO.* What the replica holds and the
target does not is STANDING; a batch is the standing writes to one todo with
whatever standing objects they rest on. DERIVED, NEVER STORED:
`proposals::batches(target, replica)` is a function of the two object sets.
A change's deps are its own todo's writes, so a batch is closed over the
target by itself and two todos' batches commute (take one, leave the other);
an object a legacy run sealed on another todo's is in each batch that rests
on it. Accepted objects fall out of the difference, so a half-accepted batch
needs no bookkeeping. (A stored batch is the hand-kept index §9 rejects.) A
batch's NAME is the least of its heads, the object a `batch:` reference
spells. `GET /api/proposals` and
`/api/<chain>/proposals` list them, ordered by the head's instant,
each object RENDERED AS A DIFF: new todo (body, detail, category, price,
rationale); completion, cancellation, reopening (naming and quoting the
todo); repricing old → new with rationale; wording before/after — IN PROSE via
`todos::markup_text` and `view::string_text`, not typst source.

*(3) ACCEPTING IS ADOPTION.* `POST /api/proposals/accept` and
`/api/<chain>/proposals/accept` take `{objects: [hashes]}`; the box takes in
exactly those, the Prodrome's `store::accept` (the RESTRICTED JOIN, §6.3 of its
design: sync, an overlay's flush and an adoption are the same function with a
different choice). Nothing is woven: the chain's heads are derived, and a
batch beside the owner's writes is one more head. The chain is the PATH's
(§10.2). The set must be ANCESTOR-CLOSED over the chain, else REFUSED BY NAME,
in reading terms: picking a completion means taking the todo it completes.

*(4) REJECTING IS DELETION FROM THE REPLICA*, THE ONE PLACE AN OBJECT IS
REMOVED — allowed because those objects never entered a chain and nothing
anyone holds rests on them; no-rewrite is a promise about a HISTORY SOMEBODY
HAS. `…/proposals/reject` is DESCENDANT-CLOSED within the batch, refused by
name (deleting a named parent breaks `verify`). `suzatary verify` fsck's every
replica; `safe-push.sh`'s append-only walk deliberately skips them (a
rejection is a deletion on purpose, not a history rewrite). REJECTING IS NOT
ACCEPTING: nothing else is recorded. The `REJECTED` receipt beside each
replica went with the legacy writer (2026-10-03): its guard refused the same
bytes again, and a run's every write carries the clock's instant, so only a
replay of the very write could produce them; its record is the data
repository's history and the review's `Reviewed` line.

BOTH ROUTES ARE WRITES: the gate's actor and the CSRF triple (`X-Suzatary-Act:
1`, `application/json`, not cross-site). LOOPBACK MAY NOT, and THE AGENT HAS NO
VERB FOR EITHER: `AgentCapabilities` carries no `Proposals`, and
`agent::VERBS`' snapshot pins both names absent. An agent that accepted its
own batch would have confirmed nothing.

*(5) TRUST BECOMES MEMBERSHIP.* `untrusted = []` on both chains: BEING ON THE
CHAIN IS BINDING, since only the owner's act or the assistant's (trusted because
a human reads its conversation, §10.3) reaches one. The core still asks the
HOST (`prodrome_core::policy`, 0.3); this deployment answers by the STORE an
object is in rather than its `actor` field — stronger, since a store is a
place a write had to reach.

⚠️ THE OLD MECHANISM STAYS IN THE CODE; the cutover is two steps.
`(unconfirmed)`, `Entry.claimed`, `Confidence` and `suzatary todo confirm`
remain for objects `triage` already wrote. Measured on `events/` 2026-09-10:
seventy-six carry `actor='triage'`; TWO open todos still claim —
a filing todo (68%, an `Authored`, two `Completed`) and
`protonmbsyncrcmissing` (90%, an `Authored`, SEVEN weekly `Completed`);
`triageverbsstale` was since closed by a trusted actor; `events-suzatary/`'s
two triage objects are a resolved todo's content. Emptying `untrusted` BINDS
those completions, so it waits until the owner has looked; until then
`instance.toml` names `triage` untrusted. The verb goes, later, once no
claiming object remains.

**The Inbox.** `#/inbox` is IN THE BAR — the first override of `nav.ts`'s
slice rule: an unseen proposal is what the app most needs a thumb to reach.
History (looked at when investigating) can afford two taps and moves to the
sheet.
It draws pending batches per chain as diffs (Accept all, a pick, Reject) and
the secretary's standing email proposals via `/api/secretary/…` (the tap
stays in §10.3(3); the Inbox links the conversation). Badge: batches plus
standing proposals. The list draws no `(unconfirmed)` row for anything
pending (not on the chain); the todo VIEW gains a line linking the Inbox when
a proposal stands against it.

**The verbs.** `suzatary todos proposals [--chain <name>]` reads batches;
`suzatary proposals accept|reject --chain <name> <hash>…` performs one — the
routes' own `proposals::batches`, `proposals::accept`, `proposals::reject`,
because two ancestor-closure implementations is one wrong later. The briefing
and `todos open` read the chain and are untouched.

**✅ 2026-09-11: THE SECRETARY REVIEWS, AND THE OWNER GETS THE RESIDUE.** After
six batches in one night (three re-proposing one todo), the owner: "the point of
the big opus agent is to have all the context in mind and to have the ability
to approve this sort of thing … there's too many things in the inbox for me
to go over." The answer is a REVIEWER: the secretary's trusted append is
exactly adoption. `SecretaryCapabilities` gains `Proposals` and
`proposals_list`, `proposals_accept`, `proposals_reject`;
`AgentCapabilities` does NOT — (4)'s asymmetry is between a batch's WRITER and
its reviewer. Two bundles let one gain a member the other may not.

*THE RULE IS IN `SECRETARY.md`, AND IT IS A JUDGEMENT.* "Reviewing the agent's
batches": ACCEPT ordinary work carrying evidence (a new todo with source and
rationale, a completion the mail supports, a repricing within its row);
REJECT with a reason a duplicate (keep the NEWEST), noise, a restatement, a
repricing with no new fact; HOLD anything at or under 45, any completion or
cancellation of a todo HE wrote, money/legal/medical, anything the
sources cannot decide. **When in doubt, hold: a held batch costs one tap, a
wrong accept is a chain that does not rewrite.** A rejection costs nothing;
the agent re-proposes if it still matters.

*A HOLD IS DERIVED, NOT CLAIMED.* No `proposals_hold`: held is what still
stands when the round ends, per `proposals_list`. A verb could be forgotten,
leaving a batch silently unheld.

*THE REVIEW IS RECORDED, AND THAT IS WHAT MAKES IT DELEGATION RATHER THAN
LOSS.* Every settle writes `Entry::Reviewed` (chain, batch head, decision,
objects, REQUIRED `why`), printed as `accepted 3 objects of batch
a1b2c3d4e5f6… on life: <why>` — the only trace a settled batch existed.
`suzatary secretary reviews --since <date>` is the day's decisions for the
briefing; the Inbox shows a held batch's reason ("Held
for you: …") and a collapsed "Settled today".

*THE TIMER RUNS IT.* `scripts/hourly-triage.sh` runs `suzatary secretary
review` after each pass, before `safe-push.sh`, so adoptions and deletions
ride in the batch's commit. It exits zero when batches are held or
the model is unreachable: a failed review leaves every batch standing.

**And the duplicate is closed at the source too.** `todo_add`'s id check reads
the replica, so a pending id was refused, but the message invited a new id. It
now NAMES THE BATCH ("todo `<id>` is already STANDING in batch `<head>` …
todo_update it"), via `Prodrome::settled` (this store's objects on the chain it
writes for: all, for a chain; the TARGET's, for a replica) and
`proposals::batch_of` (an unsettled object's head). A READ, not a way to
settle: no `Proposals` on `AgentCapabilities`.

**The laws** (`proptest`, over generated batches, beside §6's):

7. **accept is union**: after accepting a batch, the target folds as the target
   ∪ that batch would, at every instant — §6.2's equality through another
   door.
8. **reject is nothing**: rejecting leaves the target's fold identical, and
   leaves the replica a store `verify` still passes.
9. **an ancestor-open pick is refused**, naming every missing ancestor.
10. **accepting is idempotent**: the hash is the name, so accepting a batch
    twice is the store accepting it once.

**10.7 The instance file resolves under TWO roots, not one**
(`docs/DATASPLIT-PLAN-2026-09-14.md` §1; the owner, 2026-09-13: "maybe we should
make the data separated from the repo … without doing that for the moment").
`Instance.repo: PathBuf` became `Instance.roots: Roots { code: CodeRoot, data:
DataRoot }`, newtypes a relative path resolves under via one shared `under`.
A NEW path must pick a side: no conversion exists, so `data.under` where
`code.under` was meant fails to typecheck rather than resolving a data path
under the checkout. `data` defaults to `"."` (the instance file's directory),
so today both roots coincide. `CodeRoot`: templates, `scripts/`,
`secretspec.toml`, `web/dist`, `build/` — what a second deployment shares by
cloning. `DataRoot`: every chain's `events`/`proposals`, `state.json`,
briefings, the secretary's conversations, `mbsyncrc` — what stays private if
the code went public. Both are resolved once in `app/cli/src/instance.rs`;
`suzatary-core` and `suzatary-io` see neither.

**10.8 ✅ Designs and plans are recorded decisions — built 2026-10-03**
(project chain item `decisionitems`; the `decisions-schema-seeding-iwj51a` report).
A decision is a store of its own (`suzatary_core::decisions`), as the inbox
and the conversations are: `Decided`, `Revised` and `Withdrawn`, keyed by
`DecisionId`, at `decisions/` under the data root, a `Genesis` labelled
`decisions` then one `Change` per event. Its registers say what it MEANS: the
statement (title, Typst body through the one markup door, rationale) is
discrete, so two concurrent revisions are a conflict shown whole; the life is
stated below withdrawn and inflationary, so the store refuses a restatement
of a withdrawn decision; the links a decision supersedes are a grow-only set
read by union. The reading, `Ledger`, orders decisions by supersession alone
(`a ≤ b` when `b` or what it supersedes names `a`), never by a clock or a
name; "in force" is neither withdrawn nor superseded, both monotone, so going
out of force needs no coordination. Concurrent links can close a cycle no
single writer can (the writer refuses one under the store's hold); the
reading names it. A supersedes link is WHOLE: a later plan that replaces part
of an earlier one is recorded as a revision or a note, not a link. Specs and
laws stay files; a decision names the file it summarises. Writers:
`suzatary decision record|revise|withdraw` (the main session), and the owner's
tap on the secretary's `propose_decision` card; the secretary reads with
`decisions`. `suzatary decision view` is the Typst document, a section per
lineage.

**10.9 ✅ Deploy is a commit to the user repository — built 2026-10-04**
(`docs/USERREPO-PLAN-2026-10-01.md` stage 2). What the box runs is a function
of one file, the user repository's `flake.lock`: its `suzatary` input names a
commit of this repository, and `nix/app-input.nix` reads the locked tree from
that file alone. The roots service is its one reader, and `$SUZATARY_ROOTS`
is the memo of that function (`app`, then `app-bin` and `web-dist`, its two
builds), moved only when every build succeeds and only for a committed lock,
so the units see either the old app or the new one, never a half-built one. Every unit runs from that
link, rooted for itself, as a read-only store path, so the code root holds no
state: the agent's diff lives in the
state directory, the cache and the data tree. A deploy is the one write of the
lock (`scripts/deploy.sh`), a commit, so the user repository's history is the
deploy log (a deploy that never ran is followed by its revert, so the log
only grows and never lies) and a rollback is a deploy of an earlier rev; the
wait after it is
`promote.sh`'s, unchanged, for the 2026-09-26 outage it answers.

## 9. Dropped

- **A git repository as the event store's immutability.** Content addressing
  is the immutability; replica sync is the distribution. A git mirror of
  `events/` may remain a backup habit and is not architecture.
- **Backwards compatibility** with the Python at any seam: token caches,
  state files, the intents API. Data the Prodrome holds is untouched — the
  stored format is under the 1.0 soak and no stage adds a kind.

## 11. The clients, as built

Moved from `CLAUDE.md`'s Operations on 2026-09-25; the constitution keeps the
rules and points here for the modules. The web contract is `docs/WEB-SPEC.md`.

### 11.1 The web app (`web/`)

- Routes: the hash parses into a union (`#/` prefixes every page, so no
  `startsWith` router) resolved against the width into the `Screen` `main.ts`
  matches exhaustively. `#/` is the open list as cards, ascending; bottom tab
  bar under 62rem; installable manifest. `#/todo/<id>` is a pushed screen on a
  phone and a detail pane at 62rem and up, where the whole card is the link and
  `Mounted.absorb` keeps the list's fold, filter and scroll
  (`web/src/shell/layout.ts`, pure; `web/src/todo/todoview.ts`, one view for both).
  `#/mail`: inbox, a notmuch search whose query is in the hash, a message with
  its todos, a thread; bodies are `textContent`, never `rich.ts`. The bar is
  Todos · Inbox · Secretary · Briefing · More (Project, Mail, Calendar,
  Chain, Usage, Settings, About) (`nav.TABS`/`OVERFLOW`, node-tested). A
  slot is what a phone opens the app FOR: Inbox (what waits on him, with a
  count); Project (`#/project` calls `setChain`) and Mail, a corpus, sit
  behind More. `#/calendar[/<day>]` is a week of the calendars from a day
  (today when none), Typst's HTML from `GET /api/calendar/<day>` through the
  one door, every title a string literal since every one is a stranger's.
- A todo's links are what `view.references` derives from content
  (`#link("todo:<id>")` or another todo's id in prose) — never a stored kind —
  drawn as two lists on the todo view; `rich.ts`'s hardening rewrites a
  `todo:` anchor to `#/todo/<id>`.
- **References** (2026-09-10, ARCHITECTURE §10.3):
  `suzatary_core::reference::Reference` (a todo on its chain, a message, a
  thread, a briefing, a proposal) has ONE text form that prints and parses —
  `todo:life/x`, `mail:<hash>`, `thread:<id>`, `briefing:<day>`,
  `proposal:<conversation>/<n>` — because a model writes text.
  `web/src/chat/refs.ts` tokenises a turn into text nodes and anchors via `dom.ts`,
  never `rich.ts` (a turn is the least trusted string served). "Ask the
  secretary" on a todo, message, thread or briefing (Reply on a proposal)
  attaches a removable chip carried in the hash (`#/secretary?ref=…`); the
  box loads what each chip names into that round through the verbs the model
  could call itself — no second reader.
- One store holds the fold (`web/src/app/store.ts`, pure data + subscribers;
  `sync.ts` its only fetch, coalesced): pages render from it, a write replaces
  one entry, a tip move refetches once. Lists are keyed by todo id
  (`reconcile.ts`: removals, then inserts and moves from the end, with
  already-ordered keys found as a longest increasing subsequence and left
  alone), so one moved todo moves one node and scroll survives.
- **The phone is a replica** (0.13.x, ARCHITECTURE §5): its own objects and
  heads in IndexedDB; an act is a `Change` over its todo's heads, spelled by
  `suzatary-wasm`'s `lifecycle` and written by `Todos.append` in the same
  reading that folds it (`sync.appendAct`; `replica.ts` pure: what to
  push, ask for, learn from a receipt, hold back on a refusal; `sync.ts` its
  shell; `pending.ts` its look). The list is a fold of what this browser holds:
  nothing optimistic or rolled back, no queue, nonce or base tip (set union is
  idempotent; the hash is the name). Sync is `POST /api/objects` then `GET
  /api/objects?since=<frontier>` (the token the box's last answer named, §5), on
  every move of the box's heads and reconnect; diverged heads stay a head
  set (the box wove them with a `Woven` until 2026-10-03), and conflicts
  show in `Entry.conflicts`, every
  candidate in `Entry.states`/`candidates`. A refused object stays local and NAMED (which, why in the
  box's words, how many objects rest on it), never retried; the one escape,
  resetting the replica from the box, says first what it loses. The phone
  cannot author (the markup door, the FPL term, the derived id and
  `confirm_todo`'s two refusals are `suzatary-core`'s): `add`/`confirm` ask
  `POST /api/author` and append the answer; `complete`/`cancel`/`reopen` need no
  network. `GET /api/me` → `actor: null` means no act controls.
- **Every store a replica in the tab** (2026-10-03,
  `docs/APP-AS-READING-PLAN-2026-10-02.md` stage 1). The inbox and the
  conversations join the chains: `web/src/prodrome/stores.ts` is a store's
  replica as a semilattice (union of the objects, the heads of the later
  contact), `storesync.ts` its shell: kept in IndexedDB beside the chains
  (`pages/<name>` for the page's stores, each its replica alone since they
  lock no words, moved once from a keyed `store/<name>`; the lines one node
  per row in a store of their own), pulled from the frontier the last answer named (§5), `GET /api/stores/<store>/objects?since=`
  (the conversations first, then the inbox, which the box writes first) in
  pieces of at most a megabyte of literals, each kept before the next is asked
  from the heads it left (2026-10-13: the whole lines store was 34 MB, which a
  phone never pulled inside its wait), only once something reads a store (the
  "Needs you" popup reads the inbox alone, `storesync.proposals`), told
  of a move by one stream for every store, `GET /api/stores/events`, each
  frame naming its store and its heads, so a store the box adds costs a tab
  no connection (2026-10-21: every store's
  objects are also served by hash, `…/content?hashes=`, kept for good, and
  the lines' skeleton at `GET /api/stores/lines/skeleton?since=`;
  `docs/LAZYSYNC-PLAN-2026-10-21.md`), and read by the core in its worker
  (`questions.ts`, 2026-10-24): the page sends the inbox's objects, the
  lines' content and their keys by what changed, and asks for the list, a
  conversation, a search or a purge's keys; the worker keeps one
  `Proposals` per change of the inbox and one `Lines`
  (`secretary::held_lines::HeldLines`), which decodes each line once and
  reads through the Prodrome's memoised fold, one node per conversation and
  one leaf per line, so a new line computes its leaf, its conversation and
  the store, and the page's thread reads nothing that grows with the
  stores. Since stage 2 of that plan
  the tab pulls the lines' skeleton in place of their objects and keeps each
  literal by its name in the `content` store, fetched when a reading needs
  it: a conversation reads its own lines and what they rest on, the list and
  a search every line, and no reading is made until every line it reads is
  held (`lines.need`). The todo chains are not held
  here: the Inbox prices through the replicas the chain pages read
  (`sync.heldChains`, 2026-10-04: the open chain from memory, unsent acts
  included, every other served chain from its row on this device joined with
  what `sync.pullServed` took this session: one `since=` request per chain
  after each exchange and before the Inbox prices, never a stream, held in
  memory so it never races the open chain's row), read into one `Chains`,
  the same set a chain page's rows, its 30-day line and its price basis link
  through (`Chains.linker`: the set read once per reading, a `Linker` the
  page frees when the reading is replaced), so a row and the proposal
  serving it are one number; a proposal or a row on a chain this device
  holds nothing of is NOT YET PRICED, listed last and saying so (WEB-SPEC
  §3.9, §3.10), never `∅`, and a chain held as a failure is one line on the
  page (`failed`, on the fold and the Inbox's payload, the box's and the
  tab's alike). The readings are
  `suzatary_core::secretary::readings`, the functions the box's own answers
  are, so the tab and the box read the same objects the same way
  (`web/test/replicas.test.ts`, byte for byte against
  `app/core/tests/web/replicas.json`, in any arrival order). What a replica
  cannot say is asked by content (`web/src/chat/dressed.ts`): a block's HTML by
  its sha (`GET …/blocks/<sha>`, the device's `rendered.ts` first), the
  memory's words (`GET /api/secretary/memory`), and a proposal drawn against
  its chain as the box holds it now, which the tab's replica may trail (`GET
  …/proposals/<n>/drawn`, the box's whole row). Writes keep their routes; a
  page never draws a write's answer, it pulls and reads again. The box opens
  the default conversation at startup, since no read writes.
- **Intents, one set** (2026-10-03, `docs/APP-AS-READING-PLAN-2026-10-02.md`
  stage 2). Every act a page takes is an `asks.Ask`
  (`web/src/prodrome/asks.ts`, a closed vocabulary), held in
  `intents.ts`'s set in the in-memory tier: a 2P-set keyed by the act's
  canonical print (asked, and retracted once the history holds it, both only
  growing), each asking with its answer. A monotone act, every todo act
  (`Seal`), is sealed into the tab's replica, which holds it at once; the
  others are REQUESTS the box answers under its lock (tap, dismiss, "I checked
  Sent", hand, drop, a person's turn, an edit, Stop, Regenerate, a rename,
  forgetting a fact), carried by their own routes, and held once the
  replicas read back the box's record (a proposal's machine has climbed off
  the signature its card was asked from; the question's line stands at or
  past the turns its asker saw). `intentsync.ts` sends what is newly asked,
  folds in the answer, and settles the set on every move of the replicas. A
  card's state (standing, asking, taken, failed, done) is a function of its
  row and the set, never kept; a lost answer leaves the intent in view, and
  asking again is asking once (the inbox machine performs once, and `say`
  answers a second asking with the first one's line). Why the box still
  authors every secretary write: the conversations are written by
  `secretary::transcript` alone, through the redaction door, under a queue
  bound and the worker's round; the inbox's responses are the box's
  `Decision`. So none is a monotone write a page could seal, and each is a
  request.
- Keyboard (`keys.ts`, pure, negative properties tested first: a focused field
  yields only Escape): `j`/`k`, Enter, `c`, `x`, `/`, `n`, Escape, `?`, `g
  t`/`g i`/`g s`/`g b`/`g p`/`g m`; a ⌘K palette over destinations and todos
  (`match.ts`, subsequence not substring). `focus.ts`: a route change focuses
  the new page's heading; a sheet traps Tab and returns focus to its opener.
- Local-first: the last fold is kept in `localStorage`, slim (`cache.ts`: no
  `detail`, `rich.detail` or `stream`), size-guarded, parsed not trusted, drawn
  before the fetch; when the fetch or stream fails it stays under a banner that
  DATES it (`stale.ts`). A service worker for DATA was REJECTED: a cached
  shell serving an old chain silently shows a number with no provenance. Since
  2026-09-27 a worker keeps the SHELL only (`sw.ts`: each build's files, named
  `b/<build>/…`, under a cache named by that build, never the API), and `newbuild.ts`
  offers the box's newer build to a running page, which never reloads
  without a tap; a start (`boot.ts`) loads the build the worker last heard the
  box serve, asking nothing of the network first, and no page mixes two
  builds. Typst's HTML persists by
  content in IndexedDB (`rendered.ts`, `richkeep.ts`), and uncaught failures
  are told to the box (`beacon.ts`, `POST /api/client-errors`).
- `web/src/todo/todos.ts` (ordering, filtering, preview) is pure. Wire types are
  generated from `app/core/src/wire.rs` into `web/src/wire/generated/`; types are
  erased at run time, so every payload passes the decoder generated beside its
  type (`payload.ts`), through `web/src/wire/http.ts`, the one door
  `web/src/wire/api.ts`'s route table calls.
- **Same core, never a second evaluator** (0.12.x): `suzatary-wasm` (`app/wasm`,
  since 2026-10-01; the Prodrome's reference module before) is `prodrome-core` as
  WebAssembly at this application's schemas, loaded lazily by
  `web/src/prodrome/core.ts`, every answer decoded. A schema is a class built by
  `prodrome_wasm_exports::schema!`: `Todos` (`TodoEvent<Record>`, priced), whose
  `verify`, `fold` and `list` are the todo readings, `Proposals`
  (`InboxEvent`), whose `readings` are each proposal's registers
  (`proposalReadings`; `web/test/proposals.test.ts` over the frozen
  `app/io/tests/fixtures/inbox-proposals`) and whose `inbox` is the Inbox's
  list, and `Conversations` (`LineEvent`), whose
  `list`, `conversation` and `search` take the inbox's class by reference. A
  `Todos` reads the objects once and is freed when its question is answered;
  `Todos.append` writes an act into what was read (the objects cross once per
  act), and the evaluator (`fulfillment`, `link`, `series_knots`,
  `term_json`) and `lifecycle` are free functions. The chain page's verdict is
  `Todos.verify` (rehash and parse every object; `linearise`'s causal order); `verify.ts` is a labelled
  fallback and the page names which ran. A todo's thirty days are the core's
  own knots of its linked term (`thirty.ts`, drawn by `curve.ts`); a row's pie
  is its fold's value, sampling nothing.
  Offline, a browser holding the objects (`objects.ts`, IndexedDB;
  `/api/chain` is megabytes) folds them itself and the banner names the TIP,
  not the clock. `Todos.list` is the box's own `routes::fold` (SPEC §6.7, each
  row composed by `view::entry`), so the tab decodes the `FoldPayload` the box
  sends. `/api/chain` carries
  `untrusted`, so a local fold applies this deployment's §5 policy. `web/test/wasm.test.ts` runs the
  `.wasm` under node against the flake input's seeded vectors (900 fulfillment
  samples, 1429 knots) and `events/`;
  `web/e2e/core.test.ts` checks a real browser reaches it.
- **Escaping is the DOM** (`web/src/view/dom.ts`: `createElement`/`textContent`; the
  build refuses markup parsed from a string) with ONE exception: **typst's HTML
  is the one string the browser parses, through one door (`web/src/rich.ts`).**
  What is read is Typst (`docs/TYPST-FIRST-PLAN-2026-09-25.md`): a body, a
  secretary's answer block, the todo reading page (`suzatary_core::reading`:
  header, body, detail and "Why this number", one page per todo and no theme
  in its key, since the HTML carries classes and the stylesheet colours
  them), the Inbox's decision and held-batch prose, and a day's briefing, all
  compiled by `suzatary_io::render`. Bodies and answer blocks are cached
  content-addressed under `$XDG_CACHE_HOME` (bodies warmed at startup and per
  tip move); a reading page prints the core's values at the request instant,
  so it lives in a bounded process memo (256) and never on disk. `rich`
  travels beside the source, never instead; offline, a page never fetched is
  its plain text (`web/src/todo/reading.ts`). Safe because every authored text
  and answer block compiles in the closed world (no `html`: no writer can make
  an element or attribute typst's own markup would not) and data is escaped;
  typst HTML-escapes text; **rendered means confirmed** (an untrusted record is
  served as source until `suzatary todo confirm`); and the tripwire
  (`suzatary_core::tripwire::suspicious`, run by the box's renderer and, through
  the core's wasm, by the tab's door) REFUSES (never edits) a script element,
  an inline handler, an executable URL (`javascript:`, `vbscript:`, `data:`,
  read as a browser reads it), a base, an http-equiv, a form or a frame, on
  cache reads as well as compiles; the door then keeps an href only of a
  scheme that navigates (`web/src/view/href.ts`), and the page's
  Content-Security-Policy (`serve::POLICY`) runs no inline script.
  `scripts/build-web.sh` fails if any other file assigns `innerHTML` or
  `rich.ts` holds other than exactly one.
- Tests: `scripts/web-test.sh` runs the pure modules under node, in CI; a
  `--dev` build against a real `suzatary serve` replaced
  `scripts/web-fixtures.py`. **Browser tests are the UI's evidence; screenshots
  are for the eye**: `scripts/web-e2e.sh` drives `.#triage`'s headless chromium
  over CDP (`web/e2e/cdp.ts`; node 24's `WebSocket`, no npm) against three real
  `suzatary serve` fixtures on chains `suzatary-dev fixture-chain` builds; two are
  gated to a READ-only login (chains cannot move, refusals are real) and differ
  only in a waiting triage CARD (§10.6) — a suite picks a state by
  deployment, never by file order. They run `.#suzatary-fixture` (`suzatary-dev`), a
  second derivation with a whois fixture behind a cargo feature `.#suzatary`
  does not enable (a local browser arrives from 127.0.0.1); the fixture refuses to run
  with a certificate. A page nothing renders in CI rests on somebody having
  looked.

### 11.2 The Android portal (`android/`)

- **Deep links** (2026-09-05): the briefing's `#/todo/<id>` and
  `#/briefings/<date>` https links open the app when installed, the site
  otherwise. The site vouches via `web/static/.well-known/assetlinks.json`
  (copied by `build-web.sh`, pinned by `app/cli/tests/frontend.rs`), which
  `autoVerify` fetches over 443 at INSTALL (needs `tailscale cert`'s
  certificate; else Settings → Open by default). `suzatary://todo/<id>` is
  the unverifiable fallback, mapped to the same URL in `MainActivity`; the
  WebView gets the URI whole because the route is in the FRAGMENT. A push's
  click URL (`web.app_url`, `suzatary_core::notify`) is such a link.
- **Push** (2026-09-06; "not dependent on background running"): Google's push
  wakes the app; no thread, service or alarm. NO Firebase SDK (thirty AARs a
  Gradle-free build cannot vendor): `Push.java` sends the legacy C2DM
  `com.google.android.c2dm.intent.REGISTER` to `com.google.android.gms` with
  `sender`/`subtype`/`X-subtype`/`scope`/`kid` and an IMMUTABLE PendingIntent,
  and reads `registration_id` from the `REGISTRATION` broadcast.
  Reverse-documented: each extra's comment gives its source and confidence
  (action, `sender`, `app`, reply HIGH; InstanceID extras MEDIUM); a device
  that answers nothing has no push, logged, never thrown. The token (a delivery
  name, not a credential) is kept, and `MainActivity` hands it to the box's
  page with `WebView.postWebMessage` (delivered only to a main frame at the
  box's origin) when a page loads and when a new token is kept. The page
  (`web/src/app/pushtoken.ts`) reports it to `POST /api/act/device` signed by
  its device key (SPEC law 42), when it is new or a week old; native code
  holds no key and posts nothing. The box sends a DATA message, never
  an FCM "notification": `PushReceiver` reads `title`, `body`, `click` as text,
  caps them, checks `click` against this host like `MainActivity.targetOf`, and
  opens MainActivity by CLASS.
- **Share target and widget** (2026-09-10). A share is an
  `ACTION_SEND`/`text/plain` filter on the same activity:
  `EXTRA_TEXT`/`EXTRA_SUBJECT` as text, capped, percent-encoded once into
  `#/add?text=…&title=…` (fragment: no server sees it); `draft.sharedBody`
  alone decides what the two mean; the write is `POST /api/author` plus the
  replica's seal; `onNewIntent` uses the same `targetOf`; a share is CONSUMED
  (the hash returns to the list's). The widget renders `GET /api/fold?limit=5`
  (`query::fold_limit` + `routes::most_owed`: the n lowest open fulfillments, a
  selection, not an evaluator) via `org.json` in a `RemoteViews`; a tap opens
  `#/todo/<id>`. **The number is printed as the box printed it** (a phone-side
  `value * 100` could disagree). Refresh: `updatePeriodMillis` at the 30-minute
  floor plus the push wake; a failed fetch draws the last fold "stale since
  <time>".
- **The APK is a BUILD**: `nix/apk.nix` (aapt2/javac/d8/zipalign, no Gradle, no
  network), x86_64-linux on every host (no aarch64 Linux build-tools; an aarch64 box
  needs `boot.binfmt.emulatedSystems`, or CI). UNSIGNED (a key in the store is
  published); `scripts/sign-apk.sh` signs outside from `$SUZATARY_KEYSTORE`
  (secretspec + two GitHub secrets, never git).
  `.github/workflows/release-apk.yml` publishes to the tag's `v<version>`
  release (the rolling `app` release on manual dispatch), skipping the upload
  when the unsigned hash matches the last published. The phone updates via
  Obtainium on this repo's releases (a rolling tag was invisible to it),
  "fallback to older releases" on. Every update MUST use the same signing key:
  losing the keystore costs a reinstall everywhere.
- **The APK is a function of its PROFILE** (2026-10-23,
  `docs/APKPROFILE-PLAN-2026-10-23.md`): a TOML file `nix/apk-profile.nix`
  reads, `box` and `sender`, both optional, refused at evaluation when
  malformed. `.#apk-unsigned` is the generic APK (`android/profile/generic.toml`,
  empty): `Box.java` finds no box in `Profile.java`, so `MainActivity` asks for
  one at first launch and keeps it only once it answers `GET /api/portal`
  (`suzatary_core::portal::Portal`, from `[notify] fcm_sender`) with how it
  pushes. `.#apk-profiled-unsigned` reads `SUZATARY_APK_PROFILE`; the release
  workflow writes the owner's profile from the repository variables, so the
  owner's APK keeps its package, key and box.
- **The APK is a function of its ROUTE** (2026-10-03, a spike,
  the `tsnetportal-android-spike-x2b41w` report): how the WebView reaches
  the box, one `Route.java` per `android/route/<name>/` with the jars, native
  code, resource overlay and package it needs. `device` (the portal) is the
  phone's Tailscale app and adds nothing; `tsnet` (`.#apk-tsnet-unsigned`,
  `com.bmabsout.suzatary.tsnet`, beside the portal) embeds a tailnet node
  (tsnet via gomobile, arm64) up while the app is in the foreground, and sends
  every WebView request through its loopback proxy (ProxyController), which
  asks a per-process password and tunnels to the box's 443 only.
