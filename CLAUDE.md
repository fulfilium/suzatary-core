# CLAUDE.md — Suzatary

The owner's secretary system: FPL-driven typst todos + autonomous 7-account mail
triage on this box (the fetcher). Three Claude roles: the **main assistant
session** (full tools, talks to the owner), the **5-minute triage agent**
(MCP-restricted; manual `OPERATOR.md`), and the **secretary** (since
2026-09-09; a conversation in the app; manual `SECRETARY.md`,
`docs/ARCHITECTURE.md` §10.3), trusted to author as `assistant` and still
unable to send. This file is the constitution. Full history, verbatim: the
CLAUDE history file in `docs/`, kept in the private repository only — grep it
before assuming something was never tried.

## The core principle

**Nothing load-bearing is left to memory or discipline.** Every invariant lives
in a mechanism — a type, a test, a gate, a lock — and what cannot be
mechanized (irreversible or outward-facing acts) gets the owner's explicit
confirm. Everything below derives from this.

## Code (the principle applied to programs)

- Typed functional transforms: signatures first; illegal states unrepresentable
  (sums over records-with-a-bool, exhaustive `match`); parse-don't-validate at
  boundaries; **smart constructors over post-hoc validation** — records are
  dumb data, the constructor is the only parameters→trusted-value path
  (`mk_*` in the Prodrome, `Roster::new`, `Query::new`, `Login::from_whois`);
  errors as values, never panics on input; pure core / effectful shell; plain
  over clever. Tools pinned in the flake (`.#triage` is the operator env).
- **The language is Rust** (2026-09-08, ARCHITECTURE §8.6; the Python is
  gone). Every crate is `#![forbid(unsafe_code)]`; clippy `pedantic` at `-D
  warnings` over `--all-targets`; `missing_docs` warns (a public item with no
  doc comment is a rule not written down); typed errors per crate
  (`thiserror`), `anyhow` only at the binary's edge, never in a library;
  rustls, never OpenSSL, and ONE of it. `cargo fmt --check` and clippy gate
  the merge gate and CI. **A data push
  verifies the data, never the code** (the owner, 2026-10-09): `safe-push.sh`
  runs no cargo and builds nothing, since the code it runs passed the merge
  gate before it landed. The 0.10.0 reformat commit is in
  `.git-blame-ignore-revs` (`git config blame.ignoreRevsFile
  .git-blame-ignore-revs` per clone).
- **The web app is held to the same standard** (the owner, 2026-09-16; contract
  `docs/WEB-SPEC.md`, series `docs/WEBALGEBRA-PLAN-2026-09-16.md`): a page is a
  closed `PageState` with a pure `step(state, event)` and an idempotent `draw`
  over one `Source` (`web/src/algebra/source.ts`), never a closure of flags; every law
  in the spec names the node test that pins it, over a seeded generator
  (`web/test/gen.ts`); the Rust writes vectors under `app/core/tests/web/` that
  the pure modules must agree with; the `innerHTML` door and a cast ceiling in
  `scripts/web-static.sh`, the ceiling only moving down. **The TypeScript is written in
  the owner's style** (2026-09-25, `docs/TS-STYLE.md`): namespace modules, tagged
  sums with their algebra, views as functions of values with effects passed
  in, types first, terse, almost no comments. Every brief touching `web/`
  names that file.
- **The look is the owner's design system** (2026-10-04): the public Typst
  package typst-design (`@local/typst-design`, a flake input on every shell's
  `TYPST_PACKAGE_PATH`, bundled for the tab as `typst/packages.json` by
  `scripts/typst-packages.mjs`). The site and the guide open with its
  `working-document`, the kit draws its `pie` and `fulfillment-color`, and
  `tokens.typ` names each app colour role as a sample of its ramps, from which
  `scripts/gen-tokens.mjs` prints `tokens.css` and `tokens.ts`. No second
  definition of a colour, mark or template is kept here; HTML pages carry
  classes, not colours, and the answer preamble is the package's names drawn
  with typst's own elements, since HTML export drops its shapes.
- **The record is the app's** (2026-09-09, Prodrome 0.2, ARCHITECTURE §10.1):
  a todo's content is a DECORATION of a fulfillment curve and the database is
  generic over it. `app/core/src/record.rs` holds `Record` — the twelve
  stored fields, the vocabulary they nest, the `mk_*` constructors, and the two
  source languages as newtypes (`MarkupSource` for `[..]`, `StringSource` for
  `".."`). A `MarkupSource` has three doors and no other constructor
  (2026-10-02, pinned in `app/cli/tests/writes.rs`): `checked`, a writer's
  markup as written, admitted when typst's parser finds no error, it stands
  inside `[..]` as it stands alone, it compiles in the CLOSED WORLD and its
  render passes the tripwire, whoever the writer (the owner, the secretary and
  the triage agent alike; `authoring` is its one caller); `escaped`, for text
  that is DATA and never markup (a subject, a quoted line, a sentence the box
  composes, a decision's detail), chosen by the FIELD, never by guessing; and
  `from_chain`, for stored bytes, minted only where they are read (a record's
  literal, a decision's, the dev fixture). A blocker is `escape_string`'s. The
  closed world is the Prodrome's `Project::restricted()` (`suzatary_io::
  typeset`): typst's library with `html` and `plugin` REMOVED from its scope
  and from `std` (so no spelling, field access or `eval` reaches either), no
  file but the text, no package, font or clock; the box runs it in a child
  (`suzatary typeset`) that caps its own CPU time (two seconds, so a busy box
  never refuses a correct text; a minute of wall clock is only a backstop)
  and address space, and the tab runs the same code as WebAssembly in a
  worker it terminates after two. The Prodrome crate is taken `default-features = false` (its
  reference payload is not linked); the stored bytes did not move
  (`app/cli/tests/live.rs` pins it). On a SHIPPED kind `""` means absent, never
  `None` (239 objects already print it; absence with two spellings is worse
  than a sentinel with one meaning). Every fold is a projection of the core's
  registers, read once per instant by `believed::Believed`; nothing reads the
  linearisation as an order between a todo's registers.
- **No JSON for todos, anywhere in the repo**: the chain is the database,
  generated blocks carry their own lifecycle and author, and the category
  vocabulary is `instance.toml`'s `[categories]`. The JSON that exists is not todos: `state.json` (mail dedup). The
  secretary's memory is a prodrome of its own with erasable text (2026-10-02,
  ARCHITECTURE §10.3; `secretary::memories`): its HISTORY,
  `memory/`, says which facts were remembered and forgotten, by id,
  a two-phase set whose register only climbs, and names each fact's words by
  a salted hash; the WORDS are `secretary-memory-texts/`, one file per fact,
  box-local, in the vault, never pushed, and deleted by forgetting, so the
  history stays verifiable while the words are gone; the digest's summaries
  (`digests/`, `secretary-digest-texts/`, 2026-10-06) are kept the same way
  and swept by the same sweep, and written by one rule in the box's engine
  (`secretary::compaction`) through a `Compact` that answers words only,
  within a budget folded from the digest's own objects, every call counted
  before it is made (off unless `[secretary] compact = true`); every session
  opening is told the facts and that view in the owner's words alone, the
  rest by kind (`secretary::told`), and `zoom` opens a part as tainted as
  what it holds. The conversations are a
  prodrome of their own (2026-10-01, ARCHITECTURE §10.3 decision 4), and
  since words stage 2 their WORDS ARE LOCKED (`secretary::line_event`): one
  store, `secretary-lines/`, keyed by conversation, a line's shape in the
  clear and each slot of its words (a turn, a model's prose, each call's
  arguments) through the redaction door and sealed under a key of its own in
  `secretary-lines-keys/`, box-local, vaulted, never pushed. Erasing is an
  `Erased` line naming keys and the sweep destroying them (monotone, no
  hold); a forget erases the copies its remembering provably made
  (`secretary::caused`) and offers the rest on a card. The plain store it
  was carried from is deleted with its reader (words stage 4). Never turns
  on the TODO chain: a chat turn has no
  fulfillment or lifecycle, so that would be §10.1's generic-payload mistake
  pointed the other way; its own schema is not. So are the RECORDED DECISIONS
  (2026-10-03, ARCHITECTURE §10.8; `suzatary_core::decisions`): designs and
  plans as items of `decisions/`, keyed by decision, a discrete statement,
  an inflationary life and a grow-only supersedes set, ordered by
  supersession alone; specs and laws stay files. Not a `decision` todo,
  which is a choice awaiting the owner's tap.
- **No backward compatibility, not even for data** (the owner, 2026-09-25: "no
  one outside of our usecase depends on our code"; 2026-10-01: "what's
  important is that we can always pull out an earlier checkpoint of the whole
  project"). Rename and delete freely and update every caller in the same
  series; no re-exports kept for importers, no deprecated aliases, no dual
  APIs, in Suzatary or the Prodrome. Stored data (chain objects,
  conversations, `state.json`, `instance.toml`) keeps reading until its
  format changes; a format change is an explicit migration verb, and once it
  has run on the box its verb, its reader of the old form and the old store
  are deleted, since the data repository's earlier commit holds the old data
  beside the code that read it (`docs/USERREPO-PLAN-2026-10-01.md` §2).
- **A change lands as a series, not a monolith** (single commits of thousands
  of lines cannot be bisected or explained by `git blame`): one logical change
  per commit — the primitive, the operation, the route, the wire, the page,
  the docs — each buildable and tested on its own. A single-commit branch is
  fast-forwarded, not merged with `--no-ff` (a merge node with no conflict and
  no series carries no information). An agent brief says this in as many words.

**Style.** Follow [`AGENTS.md`](AGENTS.md), the style guide for the Rust, the TypeScript and the comments.

## Semantics (the FPL layer)

- **A todo's history is one fulfillment function** (2026-09-03): every
  transition — a spec revision, a completion (flat 1.0 from that instant), a
  reopening — is a `Piece` of one `fpl.Piecewise`, derived by prodrome
  `flatten`, never authored. Nothing rewrites the curve before its instant;
  today's number and a todo's thirty days are that term at an instant and
  across a window. Pricing a todo by its bare spec is a bug. The environment is
  time-indexed the same way (`prodrome.History`, read per knot; `fpl.bound`
  gates a snapshot's binding on its instant). The law: append any later event
  and every earlier moment reads the same (random logs, in the Prodrome's
  `core/tests`, beside the constructor laws `mk_piecewise`: unit, join, no
  repeats).
- **ORDER IS CAUSAL, TIME IS DATA** (2026-09-04). What a write supersedes is
  ancestry alone; `at` is stamped by the writer on receipt and used only as
  data (deadlines, completions, the time machine's window). No clock decides
  who wins. Events on different todos commute (law); concurrent writes to one
  register are a conflict to show and settle, never a tiebreak. Since
  2026-10-01 (Prodrome #29) a `Change` rests on its todo's HEADS, what its
  writer saw, so one writer's content, spec and state writes to a todo are
  ordered as written; a change written from 2026-09-30 until then rests only
  on the frontiers of the registers it writes, and a reader that needs an order
  among those, or among two writers' concurrent writes (undo's stream), breaks
  the tie by the stamped instant and says so. The inbox needs no order at all
  (2026-10-01): a proposal's responses write one inflationary register, below.
- **The inbox is a schema of its own, a machine that only climbs**
  (2026-10-01, the Prodrome's `docs/design-register-types.md` stage 4;
  `secretary::inbox_event`). `Asked` writes a proposal's content register and
  each response (`NotSent`, `Started`, `Sent`, `Applied`, `Acted`,
  `MayHaveGone`, `Checked`, `Dismissed`, and the box's or the secretary's
  withdrawal, `Superseded` or `NoLongerNeeded`) its state register, a `Phase`
  ordered by "further along": every try numbered and below every settled
  phase, may-have-gone below checked, a withdrawal below the owner's no (two
  closings settle as his), the settled phases otherwise incomparable, so sent on one device and dismissed on another is a CONFLICT,
  and a proposal in conflict performs nothing. The state is INFLATIONARY: the
  store refuses a response below or beside its reading. Every reading is
  monotone and takes no lock except `submit`'s one question, "does it still
  stand, so I may perform?", asked under `submit.lock` against that reading.
  A PROPOSAL IS PRICED BY THE TODO IT SERVES (2026-10-04, the Prodrome's
  SPEC law 41): the inbox's `Price` is `RefIn(<chain>, <todo>)` for what
  `serves` names, its own spec only where it serves none, linked through the
  todo chains read as ONE set of prodromes (`chain_set::ChainSet`, each chain
  under its own policy, named by its chain), and every row is priced through
  the same set (`ChainSet::entries`), so a proposal reads its todo's row at
  every instant, whatever that todo's term refers to; a chain the set does
  not hold reads `∅`, and a member that does not read is held as that
  failure, moving nothing that does not refer into it
  (`app/core/tests/inbox_prices.rs`).
  A REMEMBER CARD NAMES ITS WORDS, NEVER HOLDS THEM (2026-10-03): its `Asked`
  carries the memory's salted `TextName`, the words live in the memory's text
  store, and the one sweep (`secretary::texts::dead`, over every history's
  naming, under the memory's hold) deletes them once nothing live names them, so dismissing the
  card, or forgetting the fact it became, erases them while the history stays.
- **A register holds its frontier** (`prodrome_core::fold`): the writes to one
  (todo, kind) that no later write to it descends from. One candidate is a
  value; more is a CONFLICT the app shows WHOLE (every candidate on the page,
  `Entry.states`/`candidates` on the wire) and the next write settles.
  Nothing picks a winner: a conflict prices as its most urgent world
  (`Least`), and a reading that can show only one record (a row, a patch's
  base) takes the candidate of the least object name and says
  so. The fold is a monoid action (`extend`, law-tested); no read path keeps
  state (0.13: one `view` call rebuilds in about a millisecond).
- **One todo as the folds see it is one query** (SPEC §6.7,
  `prodrome_core::view`, 2026-09-07): the confirmed registers for lifecycle,
  content and conflicts; the registers under the empty policy for what an
  untrusted actor has CLAIMED and not bound; `flatten` for the function and
  `fulfillment` for its value. A composition of §6 and §7, not a fifth fold, in the core so
  the server and the browser's offline fold are the SAME reading (ARCHITECTURE
  §4). An entry carries the NAMES of its candidate content objects and its
  function as a PRINT, which the caller holding the objects looks up. SPEC
  §9.9 is the law.
- **The % IS a fulfillment: LOW = urgent**, sort ascending. Conjunction = power
  mean, p = −4 — read p_mean as a graded ∀—∃ quantifier, not a
  connective (⊤ is no unit: a Flat(1.0) member dilutes, test-pinned). Parent
  value = offset δ∈[−1,1], corrected form x·(1−|δ|)+max(0,δ). ⚠️
  Never re-flip the renderer; if data looks inverted, ask the owner.
- Deadlines bound WHEN, not stakes: missing-it-hurts → low %; merely moot
  (forms, small recoverable costs) → floor ~70% even on the last day.
- **THE SCALE HAS ANCHORS** (the owner, 2026-09-09: 10% is an emergency
  where life is at stake this hour; 20% is a catastrophe that lands
  tomorrow). 10: life-threatening, this hour. 20: an irreversible legal or
  financial blow tomorrow. 35–45: a real consequence within weeks (a
  government filing). 50–65:
  work that matters this month. 70–85: errands, forms, habits. 90+: parked.
  Every writer — the triage agent, the secretary, this session — prices
  against the table in OPERATOR.md and SECRETARY.md, never by feel.
- `waitingOn` only when nothing we can do — one unblocking email from us
  means `none`. Meetings are never todos (test: anything owed AFTER it ends?);
  recurring-but-never-done items (a standing feedback request) aren't either.
- **One evaluator prices.** Priorities are DATA on the chain (`Authored.spec`,
  `SpecRevised`), priced only by the Prodrome's Rust core (since 2026-09-06;
  the Python host and its two-implementation tests went 2026-09-08 — they
  tested a boundary that no longer exists). The evidence is SPEC §9's laws in
  the crate's own `core/tests/` and its conformance vectors, which are FROZEN
  (no generator left: a disagreement is a finding, never a file to refresh) and
  live upstream since 2026-09-09. Here: `app/cli/tests/live.rs` over
  `app/conformance/literals.json` (object names and order only), the pins on
  THIS box's chain; the fold pins (`live.json`, `view-live.json`) print the
  owner's history, so they went to the data repository (2026-09-26), at
  `<data>/conformance/`, and `live.rs` and `wasm.test.ts` read them there.
- A block's fulfillment is written in by the core and typst only DRAWS it (no
  `evalSpec`/`priorityFromTime` in typst since 2026-09-02). It is the entry's
  `value`, so the number on a page, in a block and in a briefing is one
  `prodrome_core::view::entries` call. `suzatary todos open` prices with the
  same function. There is no PDF document since 2026-10-09 (the owner: "We
  don't need the pdfs anymore").
- Term algebra: atoms `Flat|Decay|Curve`; ops
  `Conj|Offset|Gate|Importance|OffsetBy`; temporal `Shift|Within|After`;
  `Piecewise` for a todo's whole history; event Env `Completed|Cancelled`.
  Semantics: the Prodrome's SPEC.md §7 (github.com/fulfilium/prodrome),
  and a private research repository. Terms never hold callables. A Cancelled upstream
  prices as moot (1.0) and MUST be surfaced by reporting, never silently. Never
  trust compile-time numbers: reads come from the chain (`fold::content`,
  `specs`, `env`; absence in the last IS openness) and typst is render-only.
- Semantics, not just evaluation, are pinned: the Prodrome's
  `core/tests/all/calibration.rs` (monotone / spanning / smooth / toward-the-forbidden-failure). Known shape:
  strict p over a long `Within` window is a cliff — strictness goes inside
  horizons, and the meta-conjunction across them takes p = 0.

## Limits (stated, not hidden)

- **One writer model, over a DAG** (2026-10-03). Every writer appends the
  Prodrome's `Change(genesis, deps, event)` over its todo's HEADS in the
  replica it holds, and nothing when that replica already holds the event:
  the box (since change identity, prodrome `b9dc00c`, 2026-09-30), the phone
  (its module's `Todos.append`, 2026-10-02, posted as objects and taken in
  through `sync::receive`'s rules and the Prodrome's verify-on-receive) and
  the agent's run (into its replica, §10.6). Heads are derived and read as
  SETS everywhere (`tip`, `since=`, every receipt and `/events`); no writer
  needs one, so nothing joins them: the eventless `Woven` the box wrote after
  every write, `merge`, the legacy root and the one-tip fields went together.
  Concurrent writes to one register are a CONFLICT the app shows until a
  later write descends from both. Legacy envelopes are history, read
  forever and written by nothing; the live chains stay legacy-rooted, and an
  empty store's first write is a `Genesis` (`suzatary init` begins every
  chain, and the agent's replica writes only into a prodrome its chain
  began). A binary from before change identity refuses a store holding a
  `Change` (`unknown constructor "Change"` on every read), so a rollback past
  it is a rollback of the objects too. The inbox (`[secretary] inbox`,
  `-cards` on its name) and the conversations (`[secretary]
  conversations`) are `Genesis`-rooted prodromes at schemas of their own,
  written in `Change`s only. The forms each replaced, and the verbs that
  carried them over, were deleted once those had run on the box. An append
  rereads the whole store (about 250 ms at the live size, measured), a cost
  the Prodrome's own index would remove.
- **Wall clocks.** `at` is the writer's clock, and a writer is a PHONE: nothing
  clamps what it claims (SPEC §1). An instant MEANS A POINT IN TIME: naive and
  UTC by definition, stamped so by every writer (the box's one `Clock`, the
  tab's `time.moment`) and read so by every reader (the tab's one `time.ms`;
  a guard refuses `chrono::Local`). A DAY is the owner's, in `[persona]
  zone`, a NAMED zone (`zone::Zone`, the deployment's, handed to the tab by
  `/api/me`), never the box's or a device's: today, `created`, a deferral (from
  its day's start) and a deadline (reached at its day's END) all read it, and
  he changes it when he moves. A page writes by the box's contract or not at
  all: `gate::CLIENT_MIN` refuses an older bundle with 426. The page then
  says "Suzatary <version> is ready" with Reload, and the refused act was not
  sent (2026-10-06, the `instant-timezone-semantics` report). A wrong clock produces a future-dated
  object, which `verify` reports and the UI can say; rewriting it is the one
  thing the Prodrome refuses to do. Ordering is causal either way, but a fold
  reads events with `at <= t`, so a future-dated act is stored and not yet
  believed. Equal-instant contradictions resolve deterministically but
  arbitrarily, by canonical print.
- **Sampling.** `Within` is evaluated at 65 evenly spaced points; a composite
  term's curve (Conj, Offset, Gate, Shift, Within, Importance, After) is drawn
  from 96 samples plus its transitions, and its knots say `exact: false`.
  Flat, Decay, Curve and Piecewise of those are exact.
- **Signatures are the deployment's, from `[signatures] root`** (2026-10-06,
  SPEC law 42; `suzatary_core::signatures`), which every instance declares
  (`suzatary init` writes it; there is no unsigned path). Every writer signs: the tab with
  its device key, the box with the root (`SUZATARY_BOX_KEY`), the agent's
  replica with its own, and each actor is registered with the key its
  writes are signed by and no other (`signatures::Custody`: the agent's for
  `triage` whatever `untrusted` names; `device init` repairs a box where it
  was not); the trusted keys are `KeyAdded`/`KeyRevoked` objects the root
  signed (by a tapped device card, by `suzatary device trust`, or by a
  one-time code `suzatary device pair` prints, which trusts the key that signs
  the request carrying it: ten minutes, one use, five tries, kept only as a
  keyed hash and never logged; `signatures::Pairing`), and `POST /api/objects`
  refuses a write no trusted key of
  its actor proves, beside the `act_as` gate; every other write route asks
  the same of the request (`Trust::admits`, the one door), performs it once,
  pushes its provenance in `requests/` (who asked, what it wrote; never a
  body or an answer, which stays on the box for a replay), and the server
  and `device init` adopt each chain (resolving the keys only reads). The readings still fold under
  `Untrusted`, not `Proven`: history beneath each chain's epoch is unsigned
  and stays valid, so the proof is enforced at the door and shown, never a
  second policy. A request a revoked key signs, a read included (the tab
  signs its reads), is answered with the wipe (`410`), and the tab deletes
  what it holds; a key is revoked at the terminal alone (`suzatary device
  revoke`, 2026-10-09). A wipe clears the device's copy and stops nothing
  else: reads stay open to any device on the owner's Tailscale login, so
  removing the device from the tailnet is the real cut-off.
- **Ancestry is a bitset per object** (`prodrome_core::registers`): per
  (genesis, todo) for changes, over every object for legacy ones — O(n²) bits
  for a legacy store, fine at thousands; the core states the same limit.
- **Trust is a set of names.** A chain's untrusted actors are the whole
  policy: no per-event trust, no capability, no revocation. One rule, the
  core's `binds`, applies it. Since prodrome 0.3 the core asks the host's `Policy<E>`
  (`prodrome_core::policy`), over a store's schema `E`, the event type
  (`47d075e`: the chains are the todo schema, `TodoEvent<P>`; the inbox and the
  conversations are their own, `InboxEvent` and `LineEvent`); `chains::ChainPolicy.untrusted`, a
  `policy::Untrusted`, is this deployment's implementation, one per chain.

## Evidence (the principle applied to claims)

- **"Found nothing" requires "here's what I searched."** Corpus limits: the
  routine sync (every 5 min since 2026-09-01; a stale unit in ~/.config
  shadowed it before) = INBOX + 7 days; anything older or elsewhere needs
  `backfill`, or the secretary's `search_older` (2026-10-17,
  `suzatary_core::mail::older`): a typed query asked of each account's server
  for dates before the routine window, at most 25 pulled per account into a
  leaf of its own, its answer naming what was and was not searched. The queue is HASH-gated, not read-flag-gated, so mail the owner
  reads on his phone is still triaged exactly once (the unread gate once hid a
  booking for a day). Sent = continuous everywhere. **Junk unsynced by choice**
  (monthly `junksweep`). A folder is not an account. `suzatary mail idle`
  (2026-09-10) watches the Microsoft accounts over IMAP IDLE and triggers the
  same 5-minute timer early on a debounced wake; it lowers latency and never
  replaces the timer, which stays the fallback of record for those accounts and
  the only path for Gmail and Proton.
- Read full bodies (`suzatary show` strips HTML); never truncate. Never read
  suppressed stderr as a negative result.
- **The open list is swept, by proposal** (2026-09-21,
  `docs/OPENSWEEP-PLAN-2026-09-21.md`; `secretary::sweep` and its
  `candidates`). Evidence that closes a todo rarely names its id (an
  airline's receipt never says `todo:life/flightcheckin`). So once a day after
  `review` the secretary walks the open list: for each todo, mail AFTER its
  `created` (or its latest trusted reopen, which stands until newer mail
  arrives) in the same thread, from the same sender, or naming its referents
  (a code, a flight, a person) — never its id, never its age — is laid
  before the model, whose ONLY write is `propose_changes` of
  `complete`/`cancel` with a `why` and the `mail:` that proves it; any other
  act, or a todo not shown that round, is refused before storage. The owner taps
  it in the Inbox. A todo with no later mail produces nothing: absence of
  evidence is not evidence.
- `state.json` (root) is the sole dedup authority; notmuch tags aren't truth.
  It is pushed, so since 2026-10-09 it names each message by an HMAC of its
  canonical hash under `SUZATARY_DEDUP_KEY` and holds nothing else of it; the
  hash, the locator, the note and the noise senders are the box-local
  `local/mail.json` (`suzatary_core::mail::known`).
- **Probes never demand logins**, and that is a TYPE (2026-09-08): the
  device-code flow takes a `Consent` with exactly two constructors —
  `given_at_a_terminal`, claimed by `suzatary auth` alone after an
  `is_terminal` check, and `given_by_tap` (2026-09-14), claimed by the
  secretary's tap route alone (Irreversibles). An unattended path gets a
  refusal naming the verb to run. A backfill `EXAMINE`s rather than `SELECT`s
  (`Pull::backfill`), so it cannot mutate the server. An account is
  `MailAccount { email, subdir, sync }` in `instance.toml`, not a positional
  tuple.
- Facts that look wrong but aren't: `mbsyncrc`'s `Patterns` ↔
  `inbox_folders` must match (test-pinned, `app/cli/tests`); briefing cron
  hour is UTC on purpose (10:03 UTC is the owner's early morning).
- ⚠️ Every nix devShell exports `SOURCE_DATE_EPOCH=315532800`, and typst
  honours it in `datetime.today()`: inside `.#triage` today is 1980-01-01 to
  typst, so every date it compares with today is wrong. Every typst the box
  runs unsets it; clear it before believing a `typst query` result.

## Irreversibles (the principle where mechanisms end)

- **Every outbound email: show the owner final recipients/subject/body, get
  explicit confirm of THAT text.** "Send it" never carries to a new draft. Same
  for rebuilds/sudo/service changes — capability (passwordless sudo) is not
  permission. All 7 accounts send (MS XOAUTH2, Proton Bridge, Gmail via lieer's
  grant); Bcc unsupported by design.
- **Sending is structurally human-only, as a TYPE**: the agent's
  verbs take an `Agent` record, whose fields hold no `Notify`, no `Whois` and
  nothing that sends, so a send verb would not compile
  (`app/core/src/capability/`, one file per effect and per record; its
  `compile_fail` doc tests and the field-by-field destructuring are the pin;
  the verb list's snapshot is `app/core/tests/mcp-surface.json`).
  `suzatary_io::corpus::Operator` holds no send path either. `suzatary send`
  over `suzatary_io::smtp` is the one command that reaches outward, and the
  SMTP grant is a separate consent from the IMAP one, in a separate slot, so a
  leaked read token cannot send. Do not "helpfully" add a send MCP tool.
- **The secretary proposes and the owner taps** (2026-09-09, ARCHITECTURE §10.3).
  The `Secretary` record carries no sending effect; `propose_email` STORES a
  whole message (account, recipients, subject, body) in the conversation, shown
  in the app with every field. Only `POST …/proposals/<n>/tap` turns one into
  mail, and its handler needs a `suzatary_core::secretary::tap::Tap`, whose one
  constructor `Tap::of` takes a `signatures::Trusted`, the door's proof that a
  device key the history trusts for the actor a named tailnet peer maps to
  signed the request, plus the CSRF triple. That covers what the
  record cannot: a ROUTE holds more than its record and could otherwise reach
  the transport crate. Loopback mints no `Tap`, so the box's shell and the
  triage agent are refused. The TAP CARRIES AN INDEX, NEVER A BODY: what is
  sent is what was stored and shown. Exactly ONE file names the transport
  (`app/cli/tests/writes.rs` pins it): `commands::mail::deliver`, which the
  human's `send` calls and the tap reaches through its postman.
- **A proposal carries its reasons and can be answered no** (2026-09-10).
  `propose_email` requires `why` and `sources`; `secretary::store::Reasons`
  refuses either empty, deserialisation included, so a card always shows what
  the message is for and what it rests on. `POST …/proposals/<n>/dismiss`
  sits behind the SAME `Tap::of` (a `Tap` is a witness that the owner
  asked, not a permission to send, so answering is ONE gate; loopback mints
  neither). It records a `dismissed {reason, at}` outcome and a `Turn::Act` the
  secretary reads next round — an act, not a person's turn, because
  `thinking` is derived from the last turn and a person's would leave a page
  polling a model nobody started. `suzatary secretary dismiss` is the
  operator's twin; there is NO CLI twin of the tap.
- **A proposal can be the box's own act** (2026-09-14, chain `secretaryacts`,
  ARCHITECTURE §10.3 "Acts"): `propose_act` stores a closed `Act` — `Auth {
  account }` (the device-code sign-in `suzatary auth` runs) and `Secret`
  (Containment) — and the SAME `Tap` performs it, through a `Perform` trait
  held in NO record, handing the flow `Consent::given_by_tap`. A tree-walking
  test pins both `Consent` constructors' caller lists. The chat sees the act's
  turns — the URL and the user code, never the device code — and a restart
  closes a waiting act rather than resuming it.

## Containment (the agent reads attacker-controlled mail)

Threat model: an injected email can at worst mis-triage or write bad todo text
— never shell/network/credentials/outbound.

- **The fence is `scripts/tool-guard.sh`**: a PreToolUse hook, matcher `.*`,
  that permits `mcp__suzatary__*` plus `ToolSearch` and exits 2 on everything
  else, INCLUDING tools that do not exist yet. ⚠️ `permissions.allow` is
  NOT an exclusive allowlist (measured 2026-09-01: under `dontAsk` with `allow:
  ["mcp__suzatary__*"]` an agent called `Monitor`, shell not named `Bash`, and
  got uid 1000). A bare `deny` does remove a tool, but a blocklist of names
  loses to the next release. ⚠️ Never deny `ToolSearch`: the verbs are
  DEFERRED tools loaded through it; denying it leaves the agent silently
  toolless, not safe.
- Containment is aimed at the AGENT, not this directory: it lives in
  `.claude/triage-settings.json`, a template `scripts/triage-fence.sh` fills
  with each guard's absolute path, failing closed (`'<guard>' || exit 2`),
  and probes before its spawner passes it (`hourly-triage.sh` with
  `--settings "$FENCE" --tools ToolSearch --strict-mcp-config`; a pass on
  new mail whose stream names no `mcp__suzatary__` call fails), never in
  `.claude/settings.json`, which every interactive session loads (while it held
  the deny list a plain session here could not write a file — measured,
  test-pinned). `--strict-mcp-config` keeps account-level MCP servers (an
  interactive session holds `mcp__claude_ai_Gmail__send_message`) off the
  agent.
- Removal is checkable; a blocklist is not. So the write verbs don't blocklist
  bad typst: they build an AST, render it, re-parse it, and refuse to write if
  the result differs (`suzatary_core::decoration::render_verified`, rendering
  with `decoration::value` and reading back with `decoration::read` — one
  typst value language, read by typst's own parser; deliberately NOT through
  `Render`, because `Render::html` takes an `Escaped` and only confirmed
  content yields one). The same principle keeps markup safe (2026-10-02):
  the agent's `body`, `detail` and `detail_append` are TYPST MARKUP, and what
  it can produce is a fact about the closed world's library, not a scan of its
  text. It CAN write typst's own elements (emphasis, lists, headings, math,
  raw, `#link` to any scheme, a `#let` of its own). It CANNOT name `html` or
  `plugin` (removed, not refused), so it writes no element or attribute
  typst's own markup would not (no `data-intent`, no `<button>`, no `style`);
  it cannot read a file, fetch a package or the clock; a `javascript:`/`data:`
  href or a script compiles and is refused by the tripwire at the door; a loop
  is killed and refused. Its text is still provisional: marked `(unconfirmed)`
  where it stands and served as SOURCE until `suzatary todo confirm`.
- **The fence has a type behind it**. The verbs are `suzatary mcp`
  (`app/cli/src/commands/mcp.rs`): pure handlers in `suzatary_core::agent`
  over an `Agent` record — chain, mail corpus, mailbox, the repository's
  gates, manuals, renderer, clock, and the secretary's memory read-only (eight
  fields, the last a `secretary::recollection::Recollection`, whose stores are
  private and whose every method reads; 2026-10-24; a record is a
  list of authorities, one trait per effect, passed as a value) — and nothing
  outward. "No send tool" and "no confirm twin" are facts about what
  typechecks; the verb list is closed and snapshotted; every write's actor is
  the constant `agent::WRITES_AS` (`Writer::Triage`), which no argument
  reaches, so a forged trusted origin cannot be expressed. `tool-guard.sh` stays as the belt. Reads come back as
  curated context (`context`, `todo_show`, `list_open_todos`, and the
  memory's `recall` and `zoom`), writes
  as verbs (`todo_add`, `todo_update`, `record`).
- **Agent content is MARKED, not hidden** (2026-09-02). The content register
  folds `Authored` from every actor (a fold that dropped them would be an outage
  reporting success), so the trust asymmetry is carried to the reader: every
  generated block carries `authoredBy:` (and `state:`/`resolvedAt:` from the
  trusted fold), typst reads nothing on the side (no projection file), and
  every reader (the web app, `suzatary todos open`) tags a triage-authored todo
  `(unconfirmed)` where it stands —
  never moved or sunk, because a mis-priced provisional todo hidden at the
  bottom is the failure the marker prevents. Confirmation is a trusted
  re-append of the same content: `suzatary todo confirm <id>`, human-only like
  sending, no MCP twin (test-pinned; an agent that can confirm itself has
  confirmed nothing). The provisional record stays on the chain.
- ⚠️ **The agent's writes do not reach a chain; they are cards** (2026-10-24,
  ARCHITECTURE §10.6; the owner: one path for every change that waits for his
  tap). The triage agent's `todo_add`, `todo_update` and `record` are the SAME
  verb types the secretary holds (`agent::verbs::{TodoAdd, TodoUpdate,
  Record}`, generic over `agent::Holds`): each builds one `changes::Act` and
  hands it to its surface's `Holds::write`. The secretary's performs it at
  once as `assistant`; the agent's files it through
  `secretary::filed::TriageCards`, the one door the agent's call holds (its
  stores are private and its one method files as `Proposer::Triage`, stored
  `Triaged`), as a `Proposal::Changes` card in the Inbox, joining the open
  triage card about the same todo so the Inbox holds one card per todo, after
  building it against the chain as the tap will. Only the owner's tap
  performs a card (the one performer, `changes::apply`, as `assistant`); its
  `why` and `sources` are the agent's, required, and the card says who asked.
  The agent's `chain` is still a REPLICA of its target (`events-triage/`),
  synced before a run and READ; nothing appends to it. The fence is
  unchanged (same `tool-guard.sh`, `agent::VERBS`, `WRITES_AS`, snapshot).
  The open batches written before this are carried into cards by `suzatary
  proposals carry` (dry run first, `--apply` after the deploy with the owner's
  yes); that verb, `secretary::batch_cards` and the replica's files go in its
  follow-up.
- **The secretary reviews the triage cards** (§10.6). Two verbs,
  `triage_cards` and `triage_withdraw`; the rule is `SECRETARY.md`'s
  "Reviewing the agent's cards": withdraw duplicates and noise with a REQUIRED
  `why`, leave the rest for the owner, and never perform one (it holds no tap).
  A hold is DERIVED from what is still open; every withdrawal and hold writes a
  `Reviewed` line. `scripts/hourly-triage.sh` runs `suzatary secretary review`
  at the end of each pass, before `safe-push.sh`; it exits zero when cards are
  left and when the model is unreachable. A pass with no new mail runs no
  model and still calls `safe-push.sh`, so what the server wrote since the last
  pass (taps, proposals, conversations, the app's chain writes) is committed
  within five minutes. `suzatary secretary reviews --since <date>` is the day's
  decisions for the briefing. The old mechanism remains for objects `triage`
  wrote to the chain before batches (two open todos, 2026-09-10); `untrusted =
  []` waits until those are settled by hand.
- ⚠️ **A request from this box is local wherever it dials** (2026-10-05,
  #82): loopback cannot write, and a request from this box to its own tailnet
  address would otherwise resolve to the owner's login, a `web.act_as` key,
  so the gate asks tailscaled whose NODE a peer is and answers this box's own
  node (`Self.ID`, compared with the whois answer's `StableID`, never an
  address) as `Local`: it reads and never writes. A failed lookup refuses.
  `tool-guard.sh` stays the first fence: the agent holds no shell or HTTP
  client at all, so it cannot dial anything.
- **Replica writes widen none of it.** `POST /api/objects` needs a named
  tailnet peer in `web.act_as` and the CSRF triple; loopback may not write. The
  client gains `at` and the ORDER, both always data (SPEC §1). It does not
  gain the ACTOR (the box refuses any object whose event names an actor other
  than the poster's mapped one, so a page cannot write as `triage`) nor a name
  (the box rehashes every literal by REPRINTING it).
- Measured: `Read(<glob>)` scoping IS enforced; `Bash()`/`Edit()` arg scoping
  is NOT. Remaining layers, each live-fire tested: edit-guard hook (backstop,
  not the fence); nix-store trust gate (reports non-allowlisted tree changes
  pre-run; ALARM-ONLY since 09-01 — it used to restore them, and the only
  writer it caught was the main session); safe-push allowlist (the agent's
  stores + `state.json`, nothing rendered; no code path, since code reaches
  master through the train alone).
- Agent runs are STATELESS by design (rejected 2026-08-30: persistent memory
  would make injections durable instead of one-run; continuity lives in
  reviewed data — `state.json`, todo text, future Gate terms).
  No verb reads the main session's memory (the `memory` verb went
  2026-10-09). **The agent reads the secretary's memory and never writes it**
  (the owner, 2026-10-24): `context` carries the newest facts and the record in
  the owner's words alone, in one answer of `CONTEXT_CAP` plus `[agent]
  memory_bytes`, the manual first (`agent::remembered`), and `recall` and
  `zoom` read further over the same clean set: the facts, the owner's own lines
  and the summaries made only of them (`Label::is_owners`); any other part
  shows its kinds alone. No verb writes a fact or a summary: a triage write is
  a card, and its lines in the `triage` conversation carry no words from mail,
  which `app/core/tests/triage_memory.rs` pins over every verb, so mail it
  reads reaches no later session through the memory. **Proton Drive is the secretary's by
  permission** (2026-09-25, `docs/SECRETARYDOCS-PLAN-2026-09-25.md`): the CLI
  is reached only through a closed enum of two reads (list, download) and two
  writes (create-folder, upload) that `ProtonDrive::keep` alone uses, a file
  outside a `[secretary] drive_allow` folder is read only on a tap, and every
  SSN/ITIN/TIN/EIN is `redact`ed (`Redacted` is the only type a read answers
  in) before a transcript or a model sees it; the triage agent gets no Drive.
  Since 2026-09-29 a PDF or an image reaches the model AS ITSELF
  (`suzatary_core::file_read`, the `email-attachments-jakuay` report):
  `FileRead::of` reads a PDF's text layer first and withholds any file whose
  text names one of those numbers, answering with the redacted text instead;
  a scan's pixels cannot be redacted, and are handed over as they are.
  **A file the owner sends that names one is KEPT PRIVATE, on his say**
  (2026-09-29, `suzatary_core::vault`): the upload route holds it back and the
  composer asks; "Keep it private" puts it in his Drive at `vault::FOLDER`
  through a `Vault` held in no record, so the model can never write there,
  and the box keeps only a note (where, and the redacted text the model is
  given). `drive::allowed` never opens that folder, whatever `drive_allow`
  says, so reading it again is a `drive_read` tap, and `propose_email`
  records it without reading it, so the tap that sends it is its only read.
  Secrets never in git, URLs or transcripts: static → secretspec/pass
  (audited), rotating caches → owner-managed, vaulted. SSN/TIN never in this
  repo; resume_typst/thesis are PUBLIC.
- ⚠️ **A plugin's authority is its imports** (2026-10-02,
  `docs/PLUGINS-PLAN-2026-10-02.md` stage 1; `suzatary-plugin`,
  `suzatary_core::plugin`). A plugin is a WebAssembly component targeting a
  world of `app/wit/plugin.wit`, and a world is a record of authorities: the
  `source` world imports the host's `fetch`, a clock and a logger, and no
  WASI, so no filesystem, socket, environment or randomness exists to reach
  for. `Host::load` refuses a component importing anything else before it
  runs. `fetch` is the host's: the `[[plugins]]` manifest parses only to the
  origins it allows and to credential NAMES in `PLUGIN_<NAME>_` (so the app's
  own keys are outside any manifest's reach), each bound to one use and one
  origin that is https or loopback, never one in the clear; the host resolves
  the value through the secret effect and puts it in the request, so a plugin
  never holds one, and an answer that echoes it is withheld
  (`Refusal::Echoed`). Each call is a fresh store under a time
  and memory `Budget`, stopped with a typed `RunError`; whatever the host keeps
  or prints of a plugin's words is `plugin::Said`, at most 512 bytes, cut
  where the answer crosses (`world.rs`). The transport bounds
  every plugin: the deployment passes `Ureq::public_only()` (no redirect, no
  name resolving off the public internet, so never this box or the tailnet).
  The fences are `app/plugin/tests/fence.rs`, run by name against the hostile
  `app/plugin-fence` the flake builds and against every reference plugin.
  **The calendar is the first** (2026-10-03, stage 2; `app/plugin-ics`,
  `suzatary_core::calendar`): `sync` is lent the NAMES of the credentials its
  manifest declares (a calendar's secret iCal link is one of use `url`,
  `PLUGIN_ICS_*`, asked for by the box's own card, never in a chat), and
  what it emits enters only through its schema's door. **A plugin's schema
  is a DECLARATION** (2026-10-03, stage 3): `[[plugins]] schema` names its
  text beside the component (`app/plugin-ics/ics.schema`; no Rust schema
  mirrors it), admitted by the Prodrome's eight laws when the host loads it
  or the plugin is refused by the law it breaks; an item is an event of it
  (`plugin::admit`: parsed, printed back the same, written as the plugin
  itself), and a sync is learned only when its replays keep `plugin::Law`
  (deterministic over the answers it was given, idempotent from its own
  cursor), through `plugin::learn` (only what the store does not already
  read, so its 15-minute timer, `suzatary plugin sync`, takes no lock). Each
  plugin's store is CACHE tier, `cache/plugins/<name>/store` under the data
  root, its schema's text beside it: box-local, never pushed, never staged
  by `safe-push.sh` (a guard test), rebuilt by syncing again. The calendar
  is the READER of every such store whose schema has its registers. Its titles are
  other people's words, as mail is: the secretary's `calendar` verb is
  Tainted and `context`, which is Clean, carries none of it, so an invite's
  title can never reach the memory. Meetings are
  never todos; nothing writes from the calendar to a chain. **An actor only
  proposes** (2026-10-05, stage 4): the `actor` world imports the clock and
  the log and returns its proposals as values, for now only a change to a
  chain, a `Changeset` by type (`plugin::propose`), each bounded, a few per decision and none new
  while five wait, filed as the plugin's (`Offered`) and performed only by
  the owner's tap; it reads its own store or the one its manifest grants
  (`reads`); a bundle
  may live in the user repository (`from = "user"`), pinned by the sha256
  the owner approved, never where a link takes it outside.
- ⚠️ **A coding session is the owner's tap, and the routine's brief is the
  fence** (2026-09-27, chain `codingsession`,
  `docs/CODINGSESSION-PLAN-2026-09-27.md`). The secretary only PROPOSES one
  (`propose_coding_session`, a `Task`: plain text, ≤ 4 KB, parsed once);
  `Act::CodingSession` is performed by the tap alone, through the act
  performer (`commands::coding::Coder`, in no record), which fires the ONE
  routine `[coding]` names with the task and nothing else. The session runs
  in the cloud with no route to the tailnet; what it may do is the routine's
  saved prompt (`docs/CODINGSESSION-ROUTINE.md`, versioned here, pasted
  once), and the payload arrives as untrusted data. `SUZATARY_ROUTINE_TOKEN`
  is read from secretspec per fire and set from Settings, never a chat. A
  session's life is the closed `coding::Session`, stepped only by
  `Session::step`; the poll reads `claude/` branches for a report naming the
  session's URL and never runs their code. The box never merges, builds or
  promotes a cloud branch: "Hand to the main session" writes one todo on the
  `[coding]` chain, and `scripts/deploy.sh` stays the only deploy.
- ⚠️ **A job's model is a data decision, not a configuration detail**
  (2026-09-19, chain `modelrouter`, `docs/MODELROUTER-PLAN-2026-09-17.md`): the
  mail a job reads and the chain it writes reach THAT vendor. A provider is
  declared in `instance.toml` with its `base_url`, the NAME of its secret
  (never a value) and its `attested` claim; a provider that declares
  attestation and answers without a receipt is REFUSED, never quietly used.
  Changing a job's model is the owner's decision, recorded on the project chain.
  The fence does not rest on the model's quality (everything above holds for
  any model), so a weaker model is a cost decision, not a security one — and
  `triagebench` (`docs/TRIAGEBENCH-PLAN-2026-09-19.md`) gates it anyway: no job
  changes model until this box's own year of decisions says it may, scored
  blind with the incumbent anonymous in the pool. A job names an ORDER, not a
  model, and uses the subscription until the allowance the box measures runs
  down, holding back a `reserve` automated jobs never take (the subscription is
  the owner's across every project; a 5-minute timer must never be why his session
  is refused).
- ⚠️ **The app may set a secret, and may never read one back** (2026-09-21,
  chain `settings`, `docs/SETTINGS-PLAN-2026-09-19.md`; a second host has a
  browser and a tailnet identity, no shell). Each rule is a mechanism:
  - A secret is stored only by a tapped Secret card, whose name is a
    `suzatary_core::settings::Settable`, minted against the closed list
    the INSTANCE FILE declares it needs (a provider's `key`, a channel's
    secrets, the two GitHub tokens, one per purpose) — never
    `secretspec.toml`, which also declares the signing password and the box's
    keys, so those are refused by absence, not by a blocklist.
  - The tap needs a `Tap`, pinned to one caller by a tree-walking test; the
    capability that writes is in NO record, so a verb that set a secret would
    not compile.
  - secretspec sets only a name its file declares, and a deployment's code
    root is read-only, so the first secret a verb asks for declares the names
    its instance derives (reading an instance writes nothing) in a file of its
    own under the state directory that `extends` the code root's
    (`secrets::Declarations`, named by its text's digest so no stale
    writer drops a newer name; `State::declare` writes it before the names
    move; `suzatary secretspec` prints it, 2026-10-13).
  - The value goes to `secretspec set` on STDIN, never argv (`/proc` publishes
    argv), and is `Secret`-wrapped from the byte it is parsed, so no log,
    error, refusal or response carries it. The card's outcome names the KEY
    and the Settings row says whether it RESOLVES — the whole read surface;
    `doctor` says the same.
  - **A declared secret that does not resolve files its own card** (2026-10-07,
    `docs/SETUPAGENT-PLAN-2026-10-07.md` stage 1; `secretary::{noticed,
    fixes}`): the box, with no model asked, files a `Noticed` Secret card in
    the `setup` conversation, its identity (name, declaration, episode), and
    withdraws a waiting one nothing owes any more; the page has no raw field.
  - **A failing connection files its fix, by its failure's class**
    (2026-10-13, stage 3; `connect::Failure`, `secretary::failing`): the
    class is parsed where the failure is observed and stored with it in the
    health registers, each class has one remedy, and the card is that
    remedy's step of the connection's own template, once per (connection,
    since); what is unused (a provider no job orders, a calendar quiet for 90
    days, an account failing a week) is offered for removal on a card the
    owner may dismiss.
  - **When no model answers, the box asks for one** (2026-10-07, stage 0;
    `secretary::bootstrap`): Connect runs `claude setup-token` in a terminal the
    server holds (`sign_ins::SignIns`, in no record), and the code card's tap
    sends the token to one test call (`settings::Verify`) and the store, never
    a transcript or an answer; every model key a card stores is tested first.
  - **A connection is a template applied, read back out of the instance**
    (2026-10-09, stage 4; `suzatary_core::connect`, `secretary::connections`):
    a template is data, a connection's tables, steps and check are functions
    of it, and adding, removing or granting one is a tapped card whose edit
    is byte-preserving and refused unless its text reads back as the
    algebra's tables. A link served from an origin its plugin may not reach
    is stored nowhere and asked for as a Grant naming only the origin. Its
    `Group` and `Standing` are the overview's (one of each), a standing the
    join of its steps', and a terminal's step is a `Run` spelled by the one
    `connect::Reach` init spells by.
  - The page (`#/settings`) is what is connected, by group (2026-10-09,
    stage 2; `settings::overview`, `GET /api/overview`): one reading of the
    instance and the stores, a group nothing is connected to only offered
    with Ask, every line in the owner's words (no key name) and read-only,
    because the instance file is the one writer of deployment; a line draws
    the Inbox's own OPEN CARDS, placed by the reading (one per need, on the
    first line that needs it) and named by reference, so the page invents no
    act UI; no reading names a secret (a secret card's act carries its
    label), though the inbox's stored `Act::Secret` still does, so a
    replica's objects carry it until that format changes. The page writes
    nothing: no route changes a setting (stage 5, 2026-10-13, deleted
    `POST /api/settings/{provider,models,reserve}` and the page's editors).
  - Unchanged: `secretspec set` from a terminal is the operator's path; the
    four release and mail-transport secrets are settable only from a
    terminal; `web/e2e/overview.test.ts` proves the value never comes back
    over the wire (asserting on the CDP network log, not only the DOM).
- ⚠️ **A secret is asked for in the Inbox, never in a chat** (2026-09-21,
  chain `secretask`, `docs/SECRETASK-PLAN-2026-09-21.md`; a key pasted into a
  Claude session cannot be scrubbed from the server-side record). The secretary
  proposes `Act::Secret { name: Settable, why }` (a name from the settings
  page's closed list); the card's write-only field is sent in the tap body —
  THE TAP IS THE WRITE — to `settings::Writes`, and the value has no field in
  any type, so it is never in a proposal, transcript, outcome or log. A secret
  tap without a value, or a value on any other act, is refused. Nothing in a
  session ever needs a secret's VALUE, only its name — if a step seems to
  need one pasted here, the step is wrong.
- ⚠️ **A provider is declared by a card, by preset** (2026-09-22, chain
  `providersetup`, `docs/PROVIDERSETUP-PLAN-2026-09-22.md`; a card since
  2026-10-13): `settings::ProviderSetup` over a closed `Preset` and a
  `WorkspaceUrl` limited to `https://` Databricks hosts; base URL, key name
  and `attested` are DERIVED, never typed, and a Configure card's provider
  change stores a `ProviderSetup`, so no card can aim the secretary at an
  arbitrary endpoint. `settings::Edits` (no record) writes one
  `[[providers]]` table atomically, behind the tap.
- ⚠️ **Which model answers each job is asked for by a card** (2026-09-26,
  chain `modelchoice`, `docs/MODELCHOICE-PLAN-2026-09-26.md`). `Preset` is
  the closed list — `ClaudeSubscription`, `AnthropicApi` (both BUILT IN: a
  key, no `[[providers]]` row), `Databricks`, `Phala` (attested, with the
  source commit a receipt must name), `OpenRouter` — and every URL is
  derived. EACH BACKEND IS SPELLED ONCE: `claude-cli/` is the subscription
  only, `anthropic/` the Messages API only; an order with nothing usable
  answers with `Instance::fallback` (`claude-cli/<[secretary] model>`), never
  a guess. `suzatary_core::models` parses a job's order at the route
  (`ModelEntry` = a built-in or DECLARED backend and a `ModelName` of
  `[A-Za-z0-9._:/-]{1,128}`; an undeclared backend is refused by name) and
  `Reserve` in `[0, 1]`. **A change is `Act::Configure`** (2026-10-13,
  `docs/SETUPAGENT-PLAN-2026-10-07.md` stage 5; `settings::configure`): one
  act over a closed sum of what can be configured (a job's order, the
  reserve, a provider declared or removed), each a `Move` from the value the
  card read to the one it writes, both shown on the card. One rule performs
  every arm: the tap writes `to` only while the file still reads `from`
  (already `to` is done, anything else writes nothing and says what it reads
  now), parses `to`'s entries again against the providers declared then,
  and refuses to remove a provider a job orders; a card whose From no longer
  holds is withdrawn by the box. The read, the compare and the write are ONE
  step under one `flock` on `instance.toml.lock` beside the file
  (`Edits::swap`; every instance-file edit and `suzatary instance set` hold
  it), so two writers at once never lose a change, and `swap` is called by
  the performer alone (`app/cli/tests/writes.rs`). The secretary files a card
  through `propose_act` `configure`, reading From through the read-only
  `Configuration` its record holds, and only in a round that read nothing
  but the owner's words, as a coding session; the arms are private, so only
  `Configure::{order, reserve, declare, remove}` build one. After a tap the
  server reads the file again, swapping `Models` and the providers behind
  the locks the brains read per turn, so a change takes effect with no
  restart and logs one line (job, from, to, who); the instance file is
  tracked, so the change is also a diff. `CLAUDE_CODE_OAUTH_TOKEN` is asked
  for by its Secret card like every other preset's key.
- `suzatary secretary review` (2026-09-26) builds its brain through
  `Job::Review`, not `Job::Secretary`, so the Review order a card sets
  takes effect; an empty Review order still answers with
  `Instance::fallback`, the same model an empty Secretary order would (both
  read the one job-independent fallback). Triage runs `claude` directly
  under `tool-guard.sh` with the model the wrapper pins, never through
  `choose_brain`, so `[models]` has no triage order (deleted 2026-10-09:
  a key that read as a choice and chose nothing).
- **The code repository holds nothing personal, mechanically** (2026-09-27,
  `docs/DATASPLIT-PLAN-2026-09-14.md` §9.1): `app/cli/tests/guards.rs`'s
  `personal_terms_no_tracked_file_names_a_person` walks `git ls-files` of
  this repository and fails, naming FILE and LINE only, on any hit against
  `personal-terms.txt` in the box-local tier (`<data>/local`, which
  ignores itself; 2026-10-09: never pushed, carried by the vault) and is never copied here, so a failing assertion cannot leak
  what it is guarding. Case-insensitive WHOLE WORDS, whitespace normalised
  across a line break, blank/`#` lines ignored.
  Skips with a line where there is no instance (CI, a fresh worktree);
  the merge gate's live half runs it by name. Since
  2026-10-04 the one matcher is `scripts/public-tree.sh --check`:
  `public-tree.sh <out>` writes the tree a public repository would start
  from (every committed file but the private history `.gitattributes`
  declares `export-ignore`, the reports, the CLAUDE history and the changelog,
  which starts afresh from `scripts/public-changelog.md`), and
  `public_tree_holds_nothing_of_this_deployment` fails on any line of it
  holding an address outside the reserved domains, a `.ts.net` name, a home
  path, this deployment's box, tailnet or user (read from `<data>/FETCHER`,
  the instance's `app_url` and its roots), or a term of the two private
  lists (`context-terms.txt` beside the first), or a relative link of a
  front page (`README.md`, `CONTRIBUTING.md`, `SECURITY.md`,
  `docs/README.md`) naming no file of the tree. Box facts live in the data
  repository: `<data>/FETCHER`, `<data>/VAULT_RECIPIENT`, and the Android
  portal's box and sender, which the release workflow writes into the APK's
  profile from the repository's `SUZATARY_PORTAL_*`. The matcher
  is pinned separately, against a throwaway tree and deployment. Since
  2026-10-09 it is the binary's (`suzatary_core::leak`, `suzatary leaks`),
  which also refuses a tailnet node's address and a telephone number.
  **Every published byte is plain text both checks read, or admitted**
  (2026-10-10, the owner: hand-rolled decoders are an arms race a filter
  loses): secrets are gitleaks's (`scripts/secret-rules.toml`, a fixture key by
  fingerprint in `.gitleaksignore`); the matcher reads words and decodes
  nothing; a file it cannot read (a binary, a base64 or hex run, a
  quoted-printable body) or a fingerprinted one is a hit until
  `scripts/public-admitted.sha256` lists its path and digest, a line
  `train.sh publish --admit` takes at a terminal, as it takes a new
  fingerprint or a change to the rules. `public-scan.sh` runs the same guard
  over every commit of a Prodrome range. `-t 0` stops cron and land, not a
  shell (SECURITY.md).

## Operations

- **One fetcher** (named in `<data>/FETCHER`; `suzatary up` refuses elsewhere) —
  activation, not restoration, is what splits brains. The lock is
  hostname-based: accident prevention, not adversary-proof (a clone named
  exactly as the fetcher would pass). Replicas: `suzatary replica init|sync <host>`,
  zero credentials.
- Recovery = restore key (the owner's LastPass) + one Proton login:
  `bootstrap-box.sh` beside the daily encrypted vault on Drive, seven kept (pass
  store, GPG, OAuth caches, Bridge state, assistant memory; never transcripts
  or mail). Docs live at Drive `/shared-with-me/the owner Documents`.
- Triage auth: ~1-year `claude setup-token` in pass (re-mint ~Aug 2027).
- **Layout.** THERE IS NO PYTHON (2026-09-08, ARCHITECTURE §8.6) and THERE IS
  ONE WORKSPACE (2026-09-09, §8.8). `app/` is the application — `core`
  (meaning, pure), `io` (the interpreters), `cli` (the one binary, ORGANISED BY
  VERB: `cli/src/commands/<verb>.rs` owns that verb's clap types and work,
  `verb.rs` is the argument tree and a `match`, nothing is named for the stage
  it arrived in; `suzatary-dev`, built only under the `fixture` feature, is
  that tree plus `bench` and the e2e fixture's verbs). `web/` is the TypeScript frontend, `android/` the portal,
  `scripts/` the operator's shell, `nix/` the deployment; `events/`,
  `events-suzatary/` and `state.json` the stores; `instance.toml` and
  `mbsyncrc` the configuration; `tokens.typ` at root (the colour roles). Gone
  with the Python: `suzatary/`, `tests/`, `bin/`,
  `prodrome/py/`, `ty.toml`, `ruff.toml`, `pyproject.toml`. New application
  behaviour goes into `app/`; two applications that both work is the failure
  mode.
- **THERE ARE TWO CHAINS** (2026-09-09, §10.2). `instance.toml`'s `[[chains]]`
  is an array whose ORDER is meaning: `life` (`events/`, the owner's own todos) is
  FIRST — what the bare `/api/…` paths, the briefing and
  every verb with no `--chain` mean — and `suzatary` (`events-suzatary/`) is
  this project's own work: the roadmap, the audit's findings, issues the agent
  notices about the system. **The routing rule** (stated the same in
  `OPERATOR.md` and the triage prompt): anything about the SYSTEM (a failing
  gate, a flaky test, a broken sync, a verb to fix) goes to `suzatary`;
  anything about THE OWNER'S LIFE goes to `life`. The trust policy and actor
  roster belong to a CHAIN (`suzatary_core::chains::ChainPolicy`; one store's
  untrusted agent is not the other's); the tailnet door, the categories and the
  app URL are `Deployment`'s (one answer however many chains). Routes take the
  chain as a PATH SEGMENT
  (`/api/<chain>/{chain,fold,todo/<id>,tree,tip,me,objects,author,events}`);
  mail, briefings and device registration are not scoped. Both
  stores are on the agent's `safe-push.sh` allowlist and walked by its
  append-only check.
- **The Prodrome is a dependency, not a directory**:
  <https://github.com/fulfilium/prodrome>, public under MIT OR Apache-2.0, taken
  BY REVISION in `app/Cargo.toml` (`prodrome-core`,
  `prodrome-wasm-exports` and `prodrome-typst-wasm`, the closed world) and by
  the same sha as `flake.nix`'s
  `inputs.prodrome`, so the crate the binary links and the wasm a tab fetches
  are one build. The tab's module is this repository's own (`app/wasm`, `nix
  build .#suzatary-wasm`, 2026-10-01): the Prodrome's exports generic over a
  schema, instantiated as `Todos` (`TodoEvent<Record>`, priced) and
  `Proposals` (`InboxEvent`), never the Prodrome's reference module, which
  reads a payload this application does not store. Its `SPEC.md` is the
  database's contract and its `conformance/` is FROZEN evidence with no
  generator. Not public: `app/conformance/` + `app/cli/tests/live.rs` (SPEC §9
  against THIS box's `events/`, run by name by the merge gate's live half
  because the build sandbox has no chain).
- **There is no document build** (2026-10-09): the PDF, `suzatary pdf`, its
  smoke and the typst templates are deleted; the repository releases the
  program. `.#triage` carries typst and the fonts for the pages, the site and
  the guide. The shell carries
  NO INTERPRETER (2026-09-08): the wrapper's decisions over JSON are verbs of
  the binary (`gate`, `secretary review --page-after`, and `instance get|set`
  for a script reading `instance.toml`), `node` is there for the
  web tests, and the e2e fixture's chain is a verb behind the `fixture`
  feature.
- **`suzatary` is NOT on PATH in `.#triage`** (2026-09-08; it made every `app/`
  edit force a ~30-minute release rebuild, plus `.#suzatary-fixture` and
  `.#suzatary-wire-ts`, before any test ran). The shell carries `.#rust`'s
  toolchain (`flake.nix`'s shared `rustToolchain`): `scripts/app-build.sh`
  builds incrementally for a dev loop (never deployed); `scripts/app-bin.sh`
  resolves and runs the pure `.#suzatary` derivation on demand, cached by tree
  — always the deployed program, in scripts (`serve-web.sh`,
  `.claude/mcp-suzatary.json`) and prompts: `nix develop .#triage -c
  scripts/app-bin.sh <verb>` replaces `nix develop .#triage -c suzatary <verb>`
  everywhere.
- **A worktree is not a sandbox** (2026-09-21,
  `todo:suzatary/worktreeisnotasandbox`; a fixture with a RELATIVE
  `database.path` ran `notmuch new` over the live 54,861-message store).
  Worktrees share the box's notmuch database, secretspec store, user units, nix
  roots and network; the fence is the brief. A fixture that talks to notmuch
  uses ABSOLUTE paths under the agent's scratch dir and its own
  `NOTMUCH_CONFIG` (`fixture.rs` sets it and never inherits the box's); a brief
  that can touch mail, secrets, units or roots forbids the live ones by name;
  the reviewer reads any `notmuch-config` an agent wrote before merging.
- **The merge gate is `scripts/merge-gate.sh`** (2026-09-28): `nix develop
  .#triage -c scripts/merge-gate.sh` runs fmt, clippy both ways, the
  workspace tests, `live` and `guards`, web-test, build-web (tsc), the whole
  e2e suite and `.#suzatary-web`, every step even after a failure, and prints
  each step's result and time. A branch reaches master only through a green
  run of it. It has TWO HALVES (2026-10-03): `--no-live` is the tree half, a
  function of the tree alone, run with the data tree hidden
  (`SUZATARY_INSTANCE` names nothing), so it runs in a Codespace of this
  repository (`.devcontainer/`, Nix and the public Cachix, nothing personal);
  `--only-live` is the live half, the tests that read the owner's data tree
  (`--test live`, `--test guards`, the `all` modules in its `live_modules`,
  `web-test.sh`'s live chain), on the box only. `--list` names every step's
  half, and `app/cli/tests/guards/merge_gate.rs` fails when a test that
  reads the data tree is outside the live half. A tree step's PASS IS KEPT
  (2026-10-05) in `.git/gate/`, keyed by HEAD's entries outside the paths
  it declares it never reads (`--list` prints each) and the environment the
  steps read, so a re-gate runs only the steps whose inputs moved; only a
  clean checkout reads or keeps a pass, and a step that reads what another
  writes outside the tree is kept or run `together` with it. A step may be
  split into UNITS, each kept by what it reads (2026-10-12): `test` by
  workspace member from `cargo metadata`, `web-test` and `e2e` by test file
  from esbuild's import graph, the live pins by `live_reads`; the leak guard
  keeps each clean file by its hash and its rules' hash; `--full` reads no
  pass, and the train runs it on the first train gated a day after a fully
  passed tree landed; a half's lanes
  run side by side (the Rust's checks and its tests apart, so the tests' runs
  leave cargo's lock to the checks), then e2e alone, in four shards (20 of 20 green on 2026-10-17, 253 to
  271 s against about 600 s in one). `deployed-shape`,
  never kept, renders the site and runs the dry data push from HEAD's public
  tree as a deploy runs it (a read-only store path, no `.git`, the units'
  PATH, as nobody under root), with the release binary public-tree built.
  EACH THING IS BUILT ONCE (2026-10-11, the `gate-build-once` report): one
  debug build serves the tests and the named suites, and e2e's fixture shares
  its dependencies (the fixture turns on the core's `fixture`, so the
  workspace's crates compile a second time); the shells link with mold and the dev profile optimises the
  dependencies; and the live half builds HEAD's public tree with the deploy's
  own list on the box (`deploy-build`), so the deploy of a landed commit
  only links. No build is incremental (2026-10-10). A Codespace's target
  STAYS WARM: it is cleared only when no unit in it is read again (a new
  compiler, dev or test profile, or Rust flags); room is made from the
  store's unrooted paths, then by age alone, the units no gate read for a
  week, three days, one (`cargo sweep --time`; never `--maxsize`, whose
  first fit took the newest set's small units), a cold gate's room (15 GB)
  when the locks moved since the last gate, and units no gate read for
  two days go after each gate; the shell the gate runs in is rooted before
  anything collects; the three crane dependency builds (`suzatary-deps`,
  `-wasm-deps`, `-core-deps`) are rooted. The wire's TypeScript and the
  vectors are the flake's core-only builds (the `gate-warm-fast` report).
- **The merge queue is `scripts/train.sh`** (2026-10-03; GitHub's is for
  organisations, and the gate is the box's): reviewed PRs ride a TRAIN.
  `build <pr>...` merges them onto `train` from `origin/master` in an order
  that is a function of their heads (2026-10-19): a PR whose head holds more
  of the others' merges first, so a stacked PR's merge carries its base
  (`in`, its `Completes:` on that merge once, named in `Carries:`), and the
  head's name orders the rest; "Merge #N: <title>" with the PR's URL. A
  `refs/pull/<n>/head` behind the head GitHub names refuses the build; a
  changelog conflict of additions is joined, a line a PR wrote under
  Unreleased that its merge put under a release is moved back, and
  `app/Cargo.lock` keeps the train's side and cargo adds the PR's; any other
  conflict is skipped for its author. `gate` runs only inside `.#triage`,
  refusing a variable its steps read that the caller exported and the shell
  does not set (`SUZATARY_SHELL_SETS`), runs each half here in a worktree of
  its own at the train's commit (`.git/train/worktree`, cargo's target moved
  in and back out), so the checkout can change under it, and keeps each
  verdict in `.git/train/`, keyed by half and tree (a tree both green and red
  is a flaky gate, and lands nothing); `gate --remote <codespace>` runs the tree half in that
  Codespace over `gh codespace ssh` (each commit sent as a git bundle down
  the pipe, so the Codespace holds no credential; the gate run detached
  there and its log read once a minute, so a dropped connection costs a
  poll; the Codespace stopped as soon as its half is done, its keep-alive
  ending with the train; `scripts/gate-codespace.sh` makes it, idle timeout
  10 minutes) while
  the live half runs here; `gate --remote github` runs it in core instead
  (2026-10-20, docs/OPERATOR.md): the steps that read this repository on
  the box, then the train commit's public tree, once its leak guard passes,
  pushed to core's `gate/<train sha>` alone, where `.github/workflows/gate.yml`
  runs the other lanes in parallel on free arm runners; a job's gate exiting
  1 is red, success green, anything the runners did no verdict; `bisect` finds the merge that turned a red half
  red; `land` pushes master only from a HEAD both halves passed, then
  publishes it, then `scripts/deploy.sh <rev>` is the operator's. A STACKED
  PR branches from its base PR's branch and lands with it or after it, in
  the same train or a later one. Nobody merges a train by hand.
- **How code flows** (2026-10-07): develop HERE, in this private repository
  (it holds the reports and the history); the merge gate judges a tree, its
  `public-tree` step building HEAD's public tree on its own as the roots run
  would; `train.sh land` moves master, then PUBLISHES: the public tree of
  the landed commit (`scripts/public-tree.sh <out> <rev>`, a function of the
  commit's tree, its leak guard passed whole, `--check --strict`) committed
  to suzatary-core's `master` (this checkout's remote `core`) as one commit
  whose `Source:` trailer names the landed commit, pushed with the train's
  own credential. That branch is a table of one function along master's
  first-parent chain: a source is published once (`train.sh publish
  [<rev>]` again is a no-op), a commit is made of its source and parent
  alone, and the table only climbs, never rewritten (SECURITY.md states
  the one exception, a leak, the owner's call). A path no row has held is
  held until `train.sh publish --admit <path>`, at a terminal, names it in
  the row's `Admitted:` trailer. Every deployment, this
  box included, runs suzatary-core unchanged, pinned by its user
  repository's flake (private while the repository is): `deploy.sh <rev>`
  pins a commit of the declared branch's first-parent chain as it is, or
  the publication whose trailer names the SOURCE commit land printed, and
  refuses any other rev, naming both lookups (`--ref <branch>` for a fix
  not yet landed). A publish walks every landed commit after the newest
  published, so one a land could not publish is published by the next; a
  landed commit whose tree can never pass (a term added to the lists since)
  is passed over only by `publish --skip <sha>`, named by the next row's
  `Skipped:` trailer, and deploy refuses it by name. The live half's
  `public-check` runs the same `--check --strict` on HEAD, so a leaking
  commit never lands.
- **Deploy is a commit to the user repository, by `scripts/deploy.sh`, and
  nothing else** (2026-10-04, `docs/USERREPO-PLAN-2026-10-01.md` stage 2;
  the live clone and `promote.sh` are gone). The units run the app the user
  repository's `flake.lock` names (`services.suzatary.userRepo`, the one
  option): a store path, never a git checkout, so nothing writes under the
  code root, and the binary takes its
  code root from the unit's `SUZATARY_REPO` (the instance's `repo` names
  only a command line's checkout). `deploy.sh <rev>` resolves the rev on
  the branch the user flake DECLARES (asked of a mirror of it, never of
  Nix's cache: a commit of its first-parent chain, or the one publishing
  that source), refusing any other or one without `nix/app-input.nix`,
  computes the pinned lock in a scratch copy
  of the flake, BUILDS FIRST (`scripts/app-builds.sh`, the roots run's own
  list, from the tree that lock names, rooted under `deploy-`, with no
  timeout: a failed build moves nothing), runs the NEW binary's `verify`
  from its tree against the live instance (plugin manifests' worlds
  included, no component loaded) and its chains' and secretary's stores (a
  refusal moves nothing: the instance changes in the same deploy, or a store
  is migrated first), roots the previous app's builds until it succeeds,
  writes the lock into the user
  repository only with its "Deploy <rev>" commit under safe-push's push
  lock, waits (briefly: it only links) for a `suzatary-roots` run whose
  `ExecMainStartTimestamp` is at or after the commit, restarts
  `suzatary-web` only on that run's `Result=success` with `roots/app` the
  lock's tree and its builds the same set at the same paths the deploy
  built, waits for `/api/about` to answer 200 from that app's builds,
  and pushes last. Until the web restarts, any failure stops a run still
  building, appends `Revert "Deploy <rev>"` and runs the roots again to
  relink the previous app, so the log never names a rev that did not run
  and only grows; the revert is pushed whatever its relink's wait does.
  TERM, HUP and INT are failures like any other; after the web restarts a
  failure keeps and pushes the deploy and says to re-run it.
  A rollback is a deploy of an earlier rev. Committing or
  pushing in a dev checkout deploys nothing. Never hand-roll that wait (a
  stale `Result=success` took the app down 12 minutes on 2026-09-26).
- **Two roots dirs**: the units link into `~/.local/state/suzatary/roots`
  (`SUZATARY_ROOTS`, set by the module in every unit): `app` (the tree the
  lock names, once built), the roots run's own `lock-src`, `lock-bin`,
  `lock-dist`, `lock-plugins` and `lock-shell`, the callers' `app-bin` and
  `web-dist`, and each unit's own root of what it runs (`<unit>-app`,
  `*-serving`). `app-bin.sh`, `web-dist.sh`
  and `root-serving.sh` without it — a dev shell, a CLI call — link into
  `…/roots-dev`, so a dev build cannot repoint what the units run. The
  rooted dev profile `roots/triage-profile` and the `deps-<system>` roots
  stay in `roots/`.
- **The roots refresh when the lock moves** (`suzatary-roots.path`,
  2026-09-21, `docs/LAZYBUILDSTALL-PLAN-2026-09-21.md`; the lock since
  2026-10-04): a user path unit on the user repository's `flake.lock` runs
  the one reader of the lock (`nix/services.nix` `roots`, over
  `nix/app-input.nix`) at `Nice=10`. It reads only a COMMITTED lock, builds
  only a revision a deploy pinned on the branch the box follows or the pin
  before it, a revert's way back (`scripts/followed.sh admit`), roots
  the tree before building it, refuses a tree without `nix/app-input.nix`,
  builds the binary, the bundle, the plugins and the `.#triage` shell (a
  profile) under names only it moves, and moves `roots/app` last, so a
  failed run leaves every unit on the app it ran and a unit's start builds
  nothing. `nix/module-check.nix` evaluates the module and pins these
  facts, the run's order included.
- **A record is shown as a block** (`suzatary_core::decoration`): `raise_block`
  prints it as `#kind(fields)[body]`, and typst is READ with typst's own parser
  (`typst-syntax`; a hand parser is a second reading) only to check that a
  write renders as what was built. The document tree over the blocks went
  with the PDF (2026-10-09); nothing reads a block back into a record.
- **`suzatary web <outdir>`** (`suzatary_core::site`) is the chain's second
  consumer: a static site of the attestable history — every todo's lifecycle
  with each event's object hash and the objects it rests on, plus
  `objects/<hash>.py` a stranger can rehash. It reads only the objects and
  `view::entries` (so it is evidence about the stored format); nothing reads it
  back. Each page is Typst (`site::page::Page`, 2026-09-30), every string a
  literal through `escape_string` because every one came out of
  attacker-controlled mail, compiled whole by `render::Typst::document` behind
  the same `suspicious` tripwire; a refused page is replaced by one saying so.
  No stylesheet, no script. `suzatary todos open` is the open list the briefing
  reads, ascending in fulfillment, `(unconfirmed)` where the row stands.
- **The server is `app/`, in Rust** (`suzatary serve`, axum, default
  127.0.0.1:8787; the unit binds the tailnet via `scripts/serve-web.sh`), the
  chain's third consumer.
  - Reads are GETs. The ONE chain write is `POST /api/objects`, a replica
    handing over its objects (ARCHITECTURE §5). `POST /api/author` builds an
    event and appends nothing; `POST /api/act/device` writes a push token;
    `app/cli/tests/writes.rs` pins that nothing else answers a POST.
  - A tab is a trusted actor exactly when the local Tailscale daemon names its
    peer as a login in `web.act_as` (resolved once and stashed; the actor is
    that mapping's value, never a request field; loopback resolves to no peer
    and may not write — the box's shell has the CLI, and the triage agent
    runs here) AND the request carries the CSRF half whois cannot cover (any
    page on the owner's phone can POST here as him): `X-Suzatary-Act: 1` (forces a
    CORS preflight; the server sends no CORS header), `Content-Type:
    application/json`, and not labelled cross-site by the browser.
  - `authoring::confirm_todo`, `build_todo` and `update_todo` are one function
    each, reached by the CLI (`suzatary todo add|update|confirm`), the routes
    (`/api/act/add`) and the agent's verbs: callers differ only in the ACTOR.
    A `parent` makes the new todo a subtodo in ONE act: the child, plus the
    parent's spec revised to `Conj[own, Ref(child), …]`
    (`docs/TODOTREE-PLAN-2026-09-25.md`). `suzatary todo add --parent`, the
    verbs' `parent`, and the add sheet's "Add a subtodo" all reach it.
  - It does everything semantic; the browser never computes a fulfillment of
    its own. The client gets a todo's reading part ("Why this number"
    included) as typst's HTML, `untrusted` (this deployment's §5 policy) and
    each object's canonical literal to rehash, and since 0.12.x can run the
    same evaluator (`suzatary-wasm`) to verify the chain, sample a curve or
    fold offline — a second machine, not a second reading. It authors NO HTML
    (plain text, JSON and Typst; `web/dist/` is the one thing served as
    files); every HTML string it sends is typst's, unaltered.
  - **The deployment is `instance.toml`**, named in one place (2026-09-08):
    repo root, `events/` path, actor roster, trust policy, categories, who may
    read and write the web app, the maildir, the dedup authority, `mbsyncrc`,
    the last-sync file, and every mail account with its sync strategy. Only the
    binary reads it: `suzatary-core` and `suzatary-io` never see a path, so a
    library reaching for a deployment would not compile (`mail::account` holds
    the account model with no address). Committed, nothing secret (the roster
    is in every object's `actor`). CHECKED AT STARTUP: a writer the gate would
    refuse, or one the roster does not know, stops the server. `suzatary
    doctor` prints what it read; nothing reaches for a singleton.
  - The tailnet is SHARED, so the bind address is half the access control: the
    gate asks tailscaled's local API (a unix socket, not the CLI) who owns each
    peer and admits `web.viewers` only (Tailscale LOGINS, not actors);
    unresolved 403, STATIC FILES gated too. AN ADDRESS IS NOT AN IDENTITY
    (2026-10-09): nothing is exempt, so loopback asks whois like any peer and a
    `tailscale serve` or proxy on the box reads nothing; a request carrying a
    proxy's header (`gate::PROXY_HEADERS`) or a `Host` other than `app_url`'s
    (`gate::Door`, against DNS rebinding) is refused before the lookup. Plain
    http listens on loopback alone (`serve::listening::Listening`), `serve`
    refuses an `app_url` whose port it does not bind (`gate::door_at`), and
    `serve-web.sh` refuses to start without the tailnet address and the
    certificate, or beside a `tailscale serve` of its port. Every answer
    carries `serve::POLICY` (the CSP: own-origin scripts, workers and wasm, no
    inline script, inline style attributes alone because typst lays out with
    them, no base, form or frame) and every API answer `no-store`, a JSON read
    with a weak `ETag` of its bytes (`serve::validated`; the page keeps its
    last answers and sends the tag, and a 304 is that answer again), but an
    object's literal asked for by its name (`…/content?hashes=`, 2026-10-21,
    `docs/LAZYSYNC-PLAN-2026-10-21.md`), which never changes and is sent
    `private, immutable` (`reply::Immutable`); a refusal of one is `no-store`.
  - **Mail** (2026-09-04), read-only: notmuch as a SUBPROCESS, maildir through
    `mail-parser`, the agent's own queries, at
    `/api/mail/{inbox,search,message/<id>,thread/<id>}` behind the same gate.
    Bodies are TEXT (HTML converted, capped at 200 KB, the cut declared); links
    filtered to http/https/mailto; the agent's disposition is a BADGE, never a
    filter (an inbox is by DATE; `do_new`'s watermark is the queue's question);
    reads ask the notmuch INDEX for the newest ids first (loading a message
    hashes every file it matches). The canonical hash is reproduced byte for
    byte, CPython's two-digit-year cutoff included (triage state and every
    `source.hash` key on it); `source.hash` lookups are a prefix test with a
    six-character floor.
- **`web/` is the app's frontend**, the chain's fourth consumer: TypeScript,
  one esbuild bundle, no framework, no npm (`scripts/build-web.sh`; tsc is the
  type gate; `web/dist/` git-ignored); a todo app on a phone (2026-09-03).
  Routes, modules, keyboard and tests: ARCHITECTURE §11.1. The rules:
  - **The browser never computes a fulfillment of its own and never has a
    second evaluator** (0.12.x): it runs the same core as `suzatary-wasm`, the
    Prodrome's generic exports at this application's own schemas (one class
    per schema, `new Todos(objects)` read once per change of a chain in the
    core's one Web Worker and freed when it moves; `questions.ts`),
    verifies the chain with it, samples a todo's thirty days with it and folds
    offline with it; `/api/chain` carries `untrusted` so that fold applies this
    deployment's §5 policy. A module answer that moved is a finding: the
    switch from the reference module was gated on every answer the web takes
    agreeing byte for byte (the `suzatary-wasm-module-b0u2c3` report).
  - **Escaping is the DOM** (`web/src/view/dom.ts`: `createElement`/`textContent`;
    the build refuses markup parsed from a string) with ONE exception:
    **typst's HTML is the one string the browser parses, through one door
    (`web/src/rich.ts`).** WHAT IS READ IS TYPST, WHAT REACTS IS TYPESCRIPT
    (`docs/TYPST-FIRST-PLAN-2026-09-25.md`): bodies, a secretary's answers,
    the todo reading page with "Why this number" (`suzatary_core::reading`),
    the Inbox's decision prose and a day's briefing are all typst's HTML,
    compiled by `suzatary_io::render`. Bodies and answer blocks are cached
    content-addressed under `$XDG_CACHE_HOME`; a reading page prints the
    clock, so it lives in a bounded process memo and never on disk. `rich`
    travels beside the source, never instead, and a page never fetched reads
    as its plain text. Safe because every authored text compiles in the
    closed world, whose library has no `html` (no writer, the agent included,
    can write an element or attribute typst's own markup would not), and data
    is escaped by `escape_markup`; typst HTML-escapes text; **rendered means
    confirmed** (an untrusted record is
    served as source until `suzatary todo confirm`); and the tripwire
    (`suzatary_core::tripwire::suspicious`, the box's renderer's and, through
    the core's wasm, the door's for a page the tab compiles) REFUSES (never
    edits) a script element, an inline handler, a `javascript:`, `vbscript:`
    or `data:` URL (read as a browser reads it: references decoded, controls
    and spaces out, since typst writes a tab raw in an attribute), a base, an
    http-equiv, a form or a frame, on cache reads as well as compiles; and
    `rich.ts` keeps an href only of a scheme that navigates (`view/href.ts`).
    `scripts/web-static.sh` (run by `build-web.sh` and `web-test.sh`) fails
    if any other file assigns `innerHTML` or `rich.ts` holds other than
    exactly one. Mail bodies are `textContent`; a
    person's turn and a verb's answer go through `web/src/chat/refs.ts`; a model's
    answer goes through `rich.ts` block by block (typst's HTML;
    compiled in the same closed world, its preamble drawn with typst's own
    elements, the tripwire holds); a day's
    briefing (`GET /api/briefings/<date>`: that day's brief, block by block,
    or a saved file under `briefings/`) goes through the door the same way.
  - **The phone is a replica** (0.13.x, ARCHITECTURE §5): an act is sealed in
    the tab and folded from what the browser holds — nothing optimistic,
    nothing rolled back, no queue, nonce or base tip. A REFUSED object stays
    local and NAMED, never retried; the one escape (reset the replica from the
    box) says first what it loses. The phone cannot author (the markup door,
    the FPL term, the derived id and `confirm_todo`'s refusals are
    `suzatary-core`'s), so `add`/`confirm` ask `POST /api/author`. `GET
    /api/me` → `actor: null` means no act controls.
  - **A reference has ONE text form** that prints and parses (`todo:life/x`,
    `mail:<hash>`, `thread:<id>`, `briefing:<day>`,
    `proposal:<conversation>/<n>`; `suzatary_core::reference::Reference`,
    ARCHITECTURE §10.3), because a model writes text. A chip's reference is
    loaded into the round through the verbs the model could call itself — no
    second reader. A link between todos is derived from content
    (`view.references`), never a stored kind.
  - **Local-first, and staleness is DATED** (`cache.ts`, `stale.ts`). A service
    worker keeps the SHELL and nothing else (`sw.ts`, 2026-09-27): every build
    file lives under `/b/<build>/`, cached under that build's name (learned at
    `b/current`), the API never. A start runs the build last heard, answered
    from the worker's cache and asked again behind it, so it waits on the
    network for nothing; the four heavy pages are chunks of its build; a
    running page is OFFERED a newer one (`newbuild.ts`) and never mixed with
    it. Data
    stays the replica's and the banner's: a chain served silently from a cache
    shows a number with no provenance. Typst's HTML is kept on the device by
    the content it renders (`rendered.ts`, 20 MB, least recently used first),
    and a tab's uncaught failure is one `warn` line on the box (`POST
    /api/client-errors`: a write's gate, a per-login budget, nothing stored).
  - Wire types AND their decoders are generated from `app/core/src/wire.rs`
    (`scripts/wire.sh`; types are erased at run time, so each answer is decoded
    by `decoder.ts`'s `d`, folded from its type), and `api.ts` names each route
    once over `http.ts`.
  - **A todo's body and detail are edited as Typst** (2026-10-02, the owner:
    "I know how to write typst and I do expect syntax highlighting and live
    recompile while editing"; `web/src/typst/editor.ts`, WEB-SPEC §3.14): a
    native textarea over its shaded twin (typst-wasm's `highlight`), typst-ide's
    completion and hover, the diagnostics under the field with their spans
    marked, and a preview compiled once per frame in a worker running the same
    `Project.restricted()` the box admits a text by, checked by the box's
    tripwire before it is parsed, and terminated when a compile outlives two
    seconds. The box is still the one that refuses; the editor shows the same
    words sooner.
  - **Browser tests are the UI's evidence; screenshots are for the eye**
    (`scripts/web-e2e.sh`, headless chromium over CDP, against real `suzatary
    serve` fixtures). The fixture's whois lives behind a cargo feature
    `.#suzatary` does not enable (`.#suzatary-fixture` is `suzatary-dev`, the
    one binary built with it): a feature the deployed build does not compile
    is stronger than a flag. A page nothing renders in
    CI rests on somebody having looked at it.
- **THE INSTANCE FILE HAS TWO ROOTS, NOT ONE** (2026-09-14, chain `datasplit`,
  `docs/DATASPLIT-PLAN-2026-09-14.md`, ARCHITECTURE §10.7): `repo` (the code
  checkout) and `data` (the instance file's own directory by default) are two
  types, `CodeRoot`/`DataRoot`, so a new path in `instance.toml` declares its
  side rather than inheriting the checkout. `SUZATARY_INSTANCE` is the one env
  var every job — the CLI, the units, `hourly-triage.sh`, `safe-push.sh` —
  carries to find both roots, and `safe-push.sh` REFUSES to run without it
  rather than silently skip the live tests it gates on. **MIGRATED 2026-09-26**:
  the chains, `state.json`, `briefings/`, `notes/`, `mbsyncrc` and this box's
  `instance.toml` live in the PRIVATE data repository, checked out beside
  this one; its instance names both roots absolutely, and
  `instance.toml` in this checkout is an IGNORED LINK to it, so a command run
  here reads the same deployment the units do. `safe-push.sh` resolves the
  data tree with `realpath` through that link and commits the agent's writes
  there; nothing data-shaped is committed to the code repository again (the
  `.gitignore` names every data path). Its history still holds the data, so a
  public code repository is a fresh clone (plan §9). The data tree's literal
  path lives ONLY in the nixos config repository (`services.suzatary.userRepo`
  and the briefing and main-session units); the public-tree guard forbids
  a home path in any tracked file — reach for `$SUZATARY_REPO`/`$SUZATARY_INSTANCE` or the module's options,
  never a hand-typed path.
- **The Android app is a PORTAL** (`android/`, 2026-09-04): a WebView onto
  `https://<your-box>.<tailnet>.ts.net/`, the box its APK PROFILE names
  (`nix/apk-profile.nix`, 2026-10-23, `docs/APKPROFILE-PLAN-2026-10-23.md`)
  or, for the generic APK, the one typed at first launch and kept once that
  box answers `GET /api/portal` with its Firebase sender (443 since 2026-09-05), https ONLY: cleartext is refused for every host, an
  SSL or load error is the offline screen and never a downgrade (2026-10-09),
  and data extraction rules keep everything out of backup and device
  transfer; screenshots stay allowed (the owner sends them). No fold,
  fulfillment, verifier or HTML in Java: `MainActivity.java` keeps this origin
  in the WebView, sends other links to the system browser, never proceeds past
  an SSL error, and draws its failure screen with Android widgets. NO
  credential: `tailscale whois` sees the phone's login, so removing the device
  from the tailnet removes the app. Native is earned only by OS integration
  (deep links, push, share target, widget), each a shell over the same API;
  details in ARCHITECTURE §11.2. The rules: links are ordinary https URLs the
  site vouches for (`web/static/.well-known/assetlinks.json`); push is a DATA
  message the app composes, never an FCM "notification", with no Firebase SDK;
  the widget prints the number as the box printed it; the APK is an unsigned
  build signed outside (`scripts/sign-apk.sh`, `$SUZATARY_KEYSTORE`), and every
  update MUST use the same signing key (losing the keystore costs a reinstall
  everywhere).
- **Prodrome** (`events/`): the todo system's own VCS — a content-addressed
  Merkle chain of lifecycle events, one canonical constructor-literal per
  object (PARSED by a whitelist walker, never imported/eval'd), byte-hash
  verification (shipped kinds' fields are frozen; evolution adds kinds), fold =
  function of the event SET (merge-convergent). Triage-actor events are
  provisional by construction; git is transport + second witness, not the
  truth. Genesis backfilled 2026-08-31 from 229 commits (165 events since Oct
  2025). Since the inversion (0.10, 2026-09-01) the chain holds todo CONTENT
  (`Authored`). **A log that is a DAG**
  (2026-09-04; "order is causal, time is data"): `Woven(parents, event)`, with
  `event=None` for a plain merge (a merge is structure, not a fact about a
  todo), and since 2026-09-30 `Change(genesis, deps, event)`, whose deps are
  its todo's heads, what its writer saw (SPEC §3, since 2026-10-01), the only
  object written since 2026-10-03 besides an empty store's `Genesis` (above,
  Limits); the legacy `Sealed` and `Woven` are read, never written. Still NO tree
  object (REJECTED: a derived index kept consistent by hand is the dual-write
  class the inversion killed). `verify`
  reports an untrusted event dated behind ANY ancestor (the agent's `at` is the
  server clock, so that is a forged object or a clock gone backwards).
- Traps: never patchelf the Drive CLI (Bun payload); nixos consumes this repo
  as a LOCKED path input — `nix flake lock --update-input suzatary` before
  rebuild; mail is a rebuildable cache, never committed.
- **A remote builder, and two rooted dependency builds** (2026-09-13).
  The box's daemon ships x86_64-linux derivations to a Codespace
  (`nix-builder`, made from an empty PERSONAL repository — never an org's —
  reached through gh's ssh transport; the nixos config repository says
  how). This box is aarch64, so the builder takes the APK and the checks (`nix
  build .#checks.x86_64-linux.suzatary-core`; warm clippy 37 s there against
  981 s for a local `flake check`), never the ARM binary. Crane's dependency
  build is a build-time input nothing roots, and the daemon collects below 3 GB
  free, so it is rooted per architecture: `nix build -o
  ~/.local/state/suzatary/roots/deps-<system> .#suzatary-deps` (and
  `.#packages.x86_64-linux.suzatary-deps`). Only a lock change rebuilds them;
  re-run those two after one. Never `--no-link` a build whose artifacts should
  survive the next collection.
- Briefings live in the assistant chat, never systemd/email.
- **A commit message is a claim about a diff**: `scripts/hooks/commit-msg`
  requires a tracked file the message names to be in the staged diff or
  declared in a `Mentions:` trailer. Per clone: `git config core.hooksPath
  scripts/hooks` (relative; test-pinned on the deployed checkout).

## Versioning, changelog, roadmap

**THE VERSION IS A READING OF THE PROJECT CHAIN** (the owner, 2026-10-12;
plain feature releases since 2026-10-23; `suzatary_core::release`). A release
is a todo on the `suzatary` chain whose spec composes its items (`Ref`s),
declared in order in `app/core/releases.toml` with its own `major` and
`minor`: `suzatarytwo` is 2.0, `suzatarytwoone` 2.1, `suzatarytwotwo` 2.2,
`suzatarytwothree` 2.3. A plan whose versions do not strictly increase is
refused (`Plan::new`). MAJOR.MINOR is the last completed release's (`base`.0
before any); PATCH counts the trains since that MAJOR.MINOR was first tagged,
from origin/master's newest `Release:` trailer. Items show a release's
progress and move no version; `release read` lists them with what became of
each. `Reading::of` is the one rule, a join over the SET of objects: completed
once is completed, so a reopened release stays shipped
(`app/core/tests/release.rs` pins it the last completed release's, monotone
and a function of the set). The tags up to 2.1.2 were read while items
counted toward MINOR. A PR names what it completes on
`Completes:` lines of its description (AGENTS.md); `scripts/train.sh` checks
each id at `build` and cuts the release there (so the gate judges it), and at
`land` asks `suzatary release check` whether the chain with those todos done
reads at least that version, pushes master and `v<version>` in one atomic
push, and only then records each done as `assistant` naming the PR. Every
train command first fetches origin's master and tags and records origin's
newest release (`suzatary release record`, idempotent; `train.sh record` by
hand), so a failed record is repaired by the next command; the last
release is origin/master's newest `Release:` trailer, its one reader; the
binary that writes the chain is built from a named commit (origin/master,
or the train at land; `SUZATARY_APP_REV` to `scripts/app-bin.sh`), never the
checkout as it is; and `train.sh record <rev>` refuses a rev off
origin/master's first-parent chain. `suzatary release
items <release> --add|--remove` rescopes a release in one revision; an item
moved out stays listed, as dropped. So the tag, the changelog,
`app/release.toml` (the version and the IDS of what it completed, never a
title, since the file is public; `/api/about` and Settings read titles off
the chain) and the chain say one thing, and a refused push writes nothing. THE VERSION IS NO COMPILED
INPUT (2026-10-13, with the selective gate): `app/Cargo.toml`'s version is
fixed at 0.0.0, no crate's source reads `app/release.toml` (guards'
`a_release_cut_changes_no_file_a_crate_compiles`), and the binary reads it at
RUN TIME (2026-10-14): `.#suzatary-bin` is built without it, and `.#suzatary`
copies that binary beside `share/suzatary/release.toml`, which
`suzatary_cli::stamp::release` reads from beside the executable (for
`/api/about`, `--version`, `doctor`; the package runs `--version` before it
ships), so a train moves no gate key and compiles no binary
(`scripts/release-free.sh`, a gate step, evaluates both derivations with
another release); a cargo build says `dev`. A version the tree declares that no tag names (the hand-set 1.10.0)
folds into the next release. Never set the version or close `## [Unreleased]` by hand. The
flake reads the release file for the APK's name. The contracts keep their own
snapshots (`app/core/tests/mcp-surface.json`, the stored vocabulary under
`app/core/tests/`), so widening one fails a test instead of drifting.

- `CHANGELOG.md` — what changed, per release. The merge gate's `changelog`
  step refuses an `app/` or `web/` change that moves no `## [Unreleased]`
  entry since `origin/master`.
- `ROADMAP.md` — recorded direction, NOT built, plus a Rejected section with
  reasons (a deleted idea gets re-proposed; a rejected one does not).
- **A release is a `v<version>` tag**, made by `train.sh land` alone, and CI
  has two workflows sized for two events (after September's included Actions
  minutes ran out on the 12th; one run had been eighty minutes of six
  sandboxes each compiling the workspace from zero). `quick.yml` runs on every
  push to master: the commit check and `check` (fmt, clippy, docs) from
  `cargo` against a restored dependency build with one set of flags
  (`.github/cargo-ci.toml`), not run again on git tree entries that already
  passed, so a docs change costs the commit check alone; the suite and the
  browser are the merge gate's. `tests.yml` runs on a `v*.*.0` tag (a train
  that moved MINOR or MAJOR; every train tags, and a patch is not worth an
  hour of runner) and a manual dispatch: the Nix derivations — release,
  fixture, both checks, both generators — from crane's one shared dependency
  build, which no release moves. `release-apk.yml` builds the APK only when the Android shell
  (`android/`, `nix/apk.nix`, the icon) changed since the previous tag, or
  the tag starts a new MAJOR (the owner, 2026-10-13): the web app updates
  itself behind the portal. 1.10.0 (2026-10-12) was cut by hand, the
  last one, to close the 0.17.0 drift; `v1.0.0` names a release commit that
  never reached master.
- The Log below — operating lessons an agent needs in context: *why things
  are the way they are*, not *what shipped*.

## Log

The lesson of each incident; the full entries, verbatim, are at the end of
the CLAUDE history file in `docs/` ("Moved from CLAUDE.md's Log on 2026-09-25").

- **2026-10-01** — A test that passes because a read is fast asserts a race:
  when the conversations became a store, a scripted sign-in granted before
  the tap's own answer was read. Make the order the test needs happen (a
  pending poll first), never a sleep or a retry.
- **2026-09-15** — Store every line, show rounds: drawing the record is not
  showing the conversation. Every act a person takes must wake what should hear
  it (phone-sealed Done/Cancel/Reopen are heard at ADOPTION). A clean rebase
  can still not compile: `cargo check` a merge of parallel work before pushing
  it.
- **2026-09-14** — The box names a session before the process exists
  (`--session-id`); the queue is a derivation (`secretary::queue::delta`), so
  nothing is delivered twice. Every session opens through the one fenced path
  (`--system-prompt-snapshot` is on by default). Subagents get no background
  notifications (foreground gates); a worktree agent is cut from the shell's
  current directory; `-q` hides a refused merge message, so check `git log -1`
  before deleting a branch.
- **2026-09-10 (evening)** — A text form a model must produce has to print
  and parse identically (round-trip proptest; the five spellings asserted in
  the Rust, `web/test/refs.test.ts` and `SECRETARY.md`). A derived flag
  constrains what you may append (hence `Turn::Act`). `Scripted` refuses an
  uncovered cue by name.
- **2026-09-09 (evening)** — The secretary's widening is the right to be
  believed, affordable only because a human is present. When a rule moves to a
  layer the old mechanism cannot see, the mechanism moves with it (`Tap`). A
  conversation is not an event on the todo chain (since 2026-10-01 it is a
  schema of its own).
- **2026-09-08** — Put a rule in types AND storage (separate token slots:
  Microsoft mints a send token from a refresh token whose consent covered
  both). Porting a defect is not fidelity (sequence numbers vs `UID STORE`).
  Gate a port by reprinting the live file (`state.json`, byte-identical). Check
  `df` before believing a build failure; a full disk makes the triage timer's 1
  GB guard look like a quiet system.
- **2026-09-07 (evening)** — A hand parser for somebody else's language is a
  second reading of it. A clock-dependent document cannot be diffed against
  itself at two instants. Grep for importers before believing a module has one
  consumer; a tool nothing runs is a tool nothing checks.
- **2026-09-07 (later)** — Sync converges at a fixpoint, not on the first
  round trip. A refusal has no undo: name it, never drop it quietly.
- **2026-09-07** — A port's gate is a differential (two servers on the live
  chain, routes compared byte for byte). A test-only seam is a cargo FEATURE.
  Ask what a test is about before deleting it with its scaffolding.
- **2026-09-06** — Running both sides of a delegated function is running one
  side twice. A boundary costs conversion, not computation. An import cycle
  says a layer is misplaced. Pin semantic changes as tests.
- **2026-09-05** — A stale `tailscale serve` rule holds a port across
  restarts (tailscaled's own state): `tailscale serve status` before blaming
  the app; `sudo tailscale serve reset`. The app binds v4 only; a v6-first
  client falls back.
- **2026-09-04** — The worktree harness absolutises the SHARED
  `core.hooksPath`; after every agent lands, `git config core.hooksPath
  scripts/hooks` (the wiring test checks it).
- **2026-09-02 (night / later)** — Committed output that changes with the
  clock is a daily commit about nothing. Retire a second evaluator only when
  the whole population agrees.
- **2026-09-02** — `set -o pipefail` does not cross `bash -c`: set it inside.
  `nixos-rebuild switch` re-activates `timers.target` (unpauses timers). Shared
  git config is shared.
- **2026-09-01 (night)** — A guard refusing its author is the guard working:
  teach it, never bypass it. Restart a guard only in the same `&&` chain as
  what it guards. A pipeline's status is its LAST command's. Measure, then
  believe.
- **2026-09-01 (latest)** — A commit message is a claim about a diff: `git
  show --stat` first. A test of the instance you had in mind tests your memory.
  A guard that acts SILENTLY is worse than none.
- **2026-09-01 (late)** — A capability is not a name: deny by default.
  Over-restriction fails QUIETLY. Ask the filesystem, not the agent:
  self-report is not measurement.
- **2026-09-01** — The repo is not the box (`~/.config/systemd/user/`
  outranks nix's units). "It is configured" needs "here is what is running":
  `systemctl --user show -p FragmentPath <unit>` before any claim about
  cadence.
- **2026-08-31 (late)** — Coupling you can't SEE is coupling, even when the
  measurement says zero. `env_at`'s `untrusted` is REQUIRED, never defaulted.
  `do_archive()` is NOT a dry run.
- **2026-08-31** — Removing a field can silently kill a consumer;
  `Completed.at` beats git blame as evidence.
- **2026-08-29** — A recovery drill on foreign hardware exists to harvest
  bugs (it found five). This file was once reduced 1079→~120 lines.
- **Earlier** (in history): 08-25 sent-sync everywhere · 08-19 coverage audit
  · 08-14 waiting-on section · 08-10 Proton folder lesson · 08-08
  units+secrets declared · 07-29 send path · 07-27 disk-full+auth hardening
  · 07-04 archiving, voice (sunset 08-18) · 07-03 hourly agent · 06-29
  identity model + CLI · 06-28 first build.
