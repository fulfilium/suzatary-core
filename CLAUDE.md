# CLAUDE.md — Suzatary

The owner's secretary system: FPL-driven typst todos + autonomous 7-account mail
triage on this box (the fetcher). Three Claude roles: the **main assistant
session** (full tools, talks to the owner), the **5-minute triage agent**
(MCP-restricted; manual `OPERATOR.md`), and the **secretary** (a conversation
in the app; manual `SECRETARY.md`, `docs/ARCHITECTURE.md` §10.3), trusted to
author as `assistant` and still unable to send. This file is the constitution:
the rules a session obeys on every turn. [`AGENTS.md`](AGENTS.md) is the style
guide for the Rust, the TypeScript, the comments and the written English. The
private history, verbatim, is the CLAUDE history file in `docs/` (private
repository only): grep it before assuming something was never tried.

**The project's memory**: each rule's history is a decision in `memory/`, named
by `(→ id)`; AGENTS.md's "The project's memory" says how to read and propose one.

## The core principle

**Nothing load-bearing is left to memory or discipline.** Every invariant lives
in a mechanism — a type, a test, a gate, a lock — and what cannot be
mechanized (irreversible or outward-facing acts) gets the owner's explicit
confirm. Everything below derives from this.

## Code (the principle applied to programs)

- The code follows [`AGENTS.md`](AGENTS.md): sums over records with a flag,
  smart constructors, total functions, exhaustive matches, a pure core with
  effects at the edge, typed errors, no backward compatibility (its §10;
  → `nobackcompat`). Tools are pinned in the flake (`.#triage` is the operator
  env).
- **The language is Rust**, rustls and never OpenSSL, and ONE of it (→
  `rustlanguage`). **A data push verifies the data, never the code**:
  `safe-push.sh` runs no cargo and builds nothing (→ `datapushverifiesdata`).
  Per clone: `git config blame.ignoreRevsFile .git-blame-ignore-revs` (→
  `blameignorerevs`).
- **The web app is held to the same standard**: a page is a closed
  `PageState` with a pure `step` and an idempotent `draw` over one `Source`,
  every law of `docs/WEB-SPEC.md` names its node test, and the cast ceiling in
  `scripts/web-static.sh` only moves down (→ `webalgebra`). The TypeScript
  follows `docs/TS-STYLE.md` (→ `tsstyle`).
- **The look is the owner's design system**, `@local/typst-design`;
  `tokens.typ` names each colour role and `scripts/gen-tokens.mjs` prints the
  rest. Keep no second definition of a colour, mark or template; HTML pages
  carry classes, not colours (→ `designsystem`).
- **The record is the app's**: `app/core/src/record.rs` holds `Record`, its
  vocabulary, the `mk_*` constructors and the two source languages
  (`MarkupSource`, `StringSource`) (→ `recordisapp`). A `MarkupSource` has
  three doors: `checked` (a writer's markup, compiled in the CLOSED WORLD and
  passed by the tripwire), `escaped` (DATA, chosen by the field) and
  `from_chain` (stored bytes) (→ `markupdoors`). On a SHIPPED kind `""` means
  absent, never `None`. Every fold is a projection of the core's registers,
  read once per instant by `believed::Believed`; nothing reads the
  linearisation as an order between a todo's registers.
- **No JSON for todos, anywhere in the repo**: the chain is the database; the
  JSON that exists is not todos (`state.json`, mail dedup). Every other store
  is a prodrome of its own schema, and none writes to the TODO chain: the
  secretary's memory with erasable words (→ `secretarymemory`), its digest (→
  `digests`, `compactasking`), the conversations (→ `conversationsstore`),
  their words sealed per slot (→ `lockedwords`; after a change to the
  redaction rules, run `suzatary secretary migrate-scrub`, → `migratescrub`),
  and the recorded decisions (→ `recordeddecisions`).
- **A change lands as a series, not a monolith**: one logical change per
  commit — the primitive, the operation, the route, the wire, the page, the
  docs — each buildable and tested on its own, because a commit of thousands
  of lines cannot be bisected or explained by `git blame`. A single-commit
  branch is fast-forwarded, not merged with `--no-ff`. An agent brief says
  this in as many words.

## Semantics (the FPL layer)

- **A todo's history is one fulfillment function**: every transition is a
  `Piece` of one `Piecewise` derived by `flatten`, never authored. Pricing a
  todo by its bare spec is a bug. Append any later event and every earlier
  moment reads the same (→ `onefulfillment`).
- **ORDER IS CAUSAL, TIME IS DATA**: ancestry alone decides what a write
  supersedes; `at` is data. No clock decides who wins; concurrent writes to
  one register are a conflict to show and settle (→ `ordercausal`). A
  `Change` rests on its todo's HEADS (→ `changeheads`).
- **The inbox is a schema of its own, a machine that only climbs**: one
  inflationary state register, a `Phase` ordered by "further along"; a
  proposal in conflict performs nothing, and `submit.lock` guards the one
  non-monotone question (→ `inboxschema`). A proposal is priced by the todo
  it serves, through `chain_set::ChainSet` (→ `proposalpriced`). A remember
  card names its words and never holds them (→ `remembercard`).
- **A register holds its frontier** (`prodrome_core::fold`): the writes to one
  (todo, kind) that no later write to it descends from. One candidate is a
  value; more is a CONFLICT the app shows WHOLE and the next write settles.
  Nothing picks a winner: a conflict prices as its most urgent world
  (`Least`), and a reading that can show only one record takes the candidate
  of the least object name and says so. The fold is a monoid action
  (`extend`, law-tested); no read path keeps state.
- **One todo as the folds see it is one query**, `prodrome_core::view`, the
  same reading on the server and in the browser (→ `onequery`).
- **The % IS a fulfillment: LOW = urgent**, sort ascending. Conjunction = power
  mean, p = −4 — read p_mean as a graded ∀—∃ quantifier, not a
  connective (⊤ is no unit: a Flat(1.0) member dilutes, test-pinned). Parent
  value = offset δ∈[−1,1], corrected form x·(1−|δ|)+max(0,δ). ⚠️
  Never re-flip the renderer; if data looks inverted, ask the owner.
- Deadlines bound WHEN, not stakes: missing-it-hurts → low %; merely moot
  (forms, small recoverable costs) → floor ~70% even on the last day.
- **THE SCALE HAS ANCHORS**: 10: life-threatening, this hour. 20: an
  irreversible legal or financial blow tomorrow. 35–45: a real consequence
  within weeks (a government filing). 50–65: work that matters this month.
  70–85: errands, forms, habits. 90+: parked. Every writer prices against the
  table in OPERATOR.md and SECRETARY.md, never by feel (→ `scaleanchors`).
- `waitingOn` only when nothing we can do — one unblocking email from us
  means `none`. Meetings are never todos (test: anything owed AFTER it ends?);
  recurring-but-never-done items (a standing feedback request) aren't either.
- **One evaluator prices**: the Prodrome's Rust core. Its conformance vectors
  are FROZEN: a disagreement is a finding, never a file to refresh. Here,
  `app/cli/tests/live.rs` pins this box's chain; the fold pins live in
  `<data>/conformance/` (→ `oneevaluator`).
- Typst only DRAWS a fulfillment the core wrote in: the number on a page, in a
  block and in a briefing is one `prodrome_core::view::entries` call, and
  `suzatary todos open` prices with the same function. There is no PDF (→
  `nopdf`).
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

- **One writer model, over a DAG**: every writer (the box, the phone, the
  agent's run) appends a `Change(genesis, deps, event)` over its todo's HEADS
  in the replica it holds. Heads are SETS everywhere and nothing joins them.
  Legacy envelopes are read forever and written by nothing; an empty store's
  first write is a `Genesis`. An append rereads the whole store (about 250 ms)
  (→ `onewritermodel`).
- **Wall clocks.** `at` is the writer's clock, and nothing clamps what a phone
  claims. An instant is a UTC point in time; a DAY is the owner's, in
  `[persona] zone`. A wrong clock produces a future-dated object, which
  `verify` reports and nothing rewrites; a fold reads events with `at <= t`.
  Equal-instant contradictions resolve deterministically but arbitrarily, by
  canonical print. `gate::CLIENT_MIN` refuses an older bundle with 426 (→
  `instantsutc`).
- **Sampling.** `Within` is evaluated at 65 evenly spaced points; a composite
  term's curve (Conj, Offset, Gate, Shift, Within, Importance, After) is drawn
  from 96 samples plus its transitions, and its knots say `exact: false`.
  Flat, Decay, Curve and Piecewise of those are exact.
- **Signatures are the deployment's, from `[signatures] root`**: every writer
  signs, every write route asks `Trust::admits`, the one door, and the
  readings still fold under `Untrusted`. A key is revoked at the terminal alone
  (`suzatary device revoke`); the wipe (`410`) clears a device's copy, and
  removing the device from the tailnet is the real cut-off (→ `signatures`).
- **Ancestry is a bitset per object** (`prodrome_core::registers`): per
  (genesis, todo) for changes, over every object for legacy ones — O(n²) bits
  for a legacy store, fine at thousands; the core states the same limit.
- **Trust is a set of names.** A chain's untrusted actors are the whole
  policy: no per-event trust, no capability, no revocation. One rule, the
  core's `binds`, applies it, through the host's `Policy<E>` over a store's
  schema; `chains::ChainPolicy.untrusted` is this deployment's, one per chain.

## Evidence (the principle applied to claims)

- **"Found nothing" requires "here's what I searched."** The routine sync is
  INBOX + 7 days; anything older or elsewhere needs `backfill` or the
  secretary's `search_older` (→ `syncanddeepsearch`). The queue is HASH-gated,
  not read-flag-gated (→ `hashgatedqueue`). Sent = continuous everywhere.
  **Junk unsynced by choice** (monthly `junksweep`). A folder is not an
  account. `suzatary mail idle` only triggers the 5-minute timer early (→
  `mailidle`).
- Read full bodies (`suzatary show` strips HTML); never truncate. Never read
  suppressed stderr as a negative result.
- **The open list is swept, by proposal**, once a day after `review`: the
  model's only write is a `complete`/`cancel` proposal with a `why` and the
  `mail:` that proves it. Absence of evidence is not evidence (→ `opensweep`).
- `state.json` (root) is the sole dedup authority; notmuch tags aren't truth.
  It holds only an HMAC per message; the rest is box-local `local/mail.json`
  (→ `dedupkey`).
- **Probes never demand logins**, and that is a TYPE: `Consent` has two
  constructors, `given_at_a_terminal` and `given_by_tap`. A backfill
  `EXAMINE`s, so it cannot mutate the server (→ `consenttype`).
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
- **The secretary proposes and the owner taps**: only `POST
  …/proposals/<n>/tap` turns a proposal into mail, through a `Tap` minted by
  `Tap::of` from the door's proof and the CSRF triple; loopback mints none, and
  the tap carries an index, never a body. Exactly ONE file names the
  transport, `commands::mail::deliver` (→ `secretaryproposes`).
- **A proposal carries its reasons and can be answered no**: `why` and
  `sources` are required, and dismissing sits behind the same `Tap`. There is
  NO CLI twin of the tap (→ `proposalreasons`).
- **A proposal can be the box's own act** (`Auth`, `Secret`), performed by the
  same `Tap` through `Perform`, held in no record (→ `boxacts`).

## Containment (the agent reads attacker-controlled mail)

Threat model: an injected email can at worst mis-triage or write bad todo text
— never shell/network/credentials/outbound.

- **The fence is `scripts/tool-guard.sh`**: a PreToolUse hook, matcher `.*`,
  that permits `mcp__suzatary__*` plus `ToolSearch` and exits 2 on everything
  else, INCLUDING tools that do not exist yet. ⚠️ `permissions.allow` is NOT
  an exclusive allowlist. ⚠️ Never deny `ToolSearch`: the verbs are DEFERRED
  tools loaded through it (→ `toolguard`).
- Containment is aimed at the AGENT: it lives in
  `.claude/triage-settings.json`, filled and probed by `scripts/triage-fence.sh`
  and passed with `--strict-mcp-config`, never in `.claude/settings.json`,
  which every interactive session loads (→ `fenceattheagent`).
- Removal is checkable; a blocklist is not. The write verbs round-trip what
  they render and refuse a difference, and the agent's markup compiles in the
  closed world, where `html` and `plugin` do not exist. Its text stays
  provisional: `(unconfirmed)` and served as SOURCE until `suzatary todo
  confirm` (→ `closedworldmarkup`).
- **The fence has a type behind it**: `suzatary mcp`'s verbs are pure handlers
  over an `Agent` record with nothing outward; the verb list is closed and
  snapshotted; every write's actor is the constant `agent::WRITES_AS`.
  `tool-guard.sh` stays as the belt (→ `agentrecord`).
- **Agent content is MARKED, not hidden**: every reader tags a triage-authored
  todo `(unconfirmed)` where it stands, never moved or sunk. Confirmation is
  `suzatary todo confirm <id>`, human-only, with no MCP twin (→
  `markednothidden`).
- ⚠️ **The agent's writes do not reach a chain; they are cards**: the agent
  holds the secretary's verb types, its `Holds` files a `Proposal::Changes`
  card, one per todo, and only the owner's tap performs it. Its chain is a
  READ replica (→ `triagecards`).
- **The secretary reviews the triage cards**: it withdraws duplicates and noise
  with a REQUIRED `why`, leaves the rest for the owner, and never performs one
  (SECRETARY.md; → `secretaryreviews`).
- ⚠️ **A request from this box is local wherever it dials**: the gate answers
  this box's own node as `Local`, which reads and never writes; a failed lookup
  refuses (→ `localnode`).
- **Replica writes widen none of it.** `POST /api/objects` needs a named
  tailnet peer in `web.act_as` and the CSRF triple; loopback may not write. The
  client gains `at` and the ORDER, both always data (SPEC §1). It does not
  gain the ACTOR (the box refuses any object whose event names an actor other
  than the poster's mapped one, so a page cannot write as `triage`) nor a name
  (the box rehashes every literal by REPRINTING it).
- `Read(<glob>)` scoping is enforced; `Bash()` and `Edit()` argument scoping is
  NOT. Remaining layers, each live-fire tested: the edit-guard hook, the
  alarm-only nix-store trust gate, safe-push's allowlist (→ `fencelayers`).
- Agent runs are STATELESS by design (→ `statelessagent`). The agent reads the
  secretary's memory in the owner's words alone and never writes it (→
  `agentreadsmemory`).
- **Proton Drive is the secretary's by permission**: a closed enum of two reads
  and two writes, `[secretary] drive_allow`, and every SSN/ITIN/TIN/EIN
  redacted before a model sees it; the triage agent gets no Drive (→
  `protondrive`). A PDF or an image reaches the model as itself unless its text
  names such a number (→ `filesasthemselves`). A file the owner sends that
  names one is kept private in the vault folder, on his say (→ `vault`).
- Secrets never in git, URLs or transcripts: static → secretspec/pass
  (audited), rotating caches → owner-managed, vaulted. SSN/TIN never in this
  repo; resume_typst/thesis are PUBLIC.
- ⚠️ **A plugin is one function and its grant**: a WebAssembly component of
  `app/wit/plugin.wit` whose one import of ours is `fetch`, with no WASI
  filesystem or sockets, credentials resolved by the host, origins allowed by
  its manifest, and every call bounded (→ `plugins`). The calendar is the
  first; its titles are tainted, its store is cache tier, and nothing writes
  from it to a chain (→ `calendarplugin`).
- ⚠️ **A coding session is the owner's tap, and the routine's brief is the
  fence**: the secretary only proposes a `Task`; the box never merges, builds
  or promotes a cloud branch, and `scripts/deploy.sh` stays the only deploy
  (→ `codingsession`).
- ⚠️ **The app talks to Claude alone, through the `claude` CLI**; the model is
  a PORT in the core, and changing a model is an edit of the instance file (→
  `claudealone`).
- ⚠️ **The app may set a secret, and may never read one back**: a tapped
  Secret card with a `Settable` name, the value on stdin and `Secret`-wrapped,
  the writing capability in no record (→ `settings`). The box files its own
  setup cards: a declared secret that does not resolve (→ `noticedsecret`), a
  failing connection by its class (→ `failingconnection`), a model when none
  answers (→ `modelbootstrap`); a connection is a template read back out of the
  instance (→ `connections`), and the settings page is read-only (→
  `settingspage`).
- ⚠️ **A secret is asked for in the Inbox, never in a chat**: the tap is the
  write, and the value has no field in any type. Nothing in a session ever
  needs a secret's VALUE, only its name — if a step seems to need one pasted
  here, the step is wrong (→ `secretask`).
- **The code repository holds nothing personal, mechanically**: the guards
  check every tracked file against box-local term lists (→ `nothingpersonal`),
  the leak guard checks the public tree (→ `publictree`), and every published
  byte is plain text both checks read or is admitted by digest at a terminal
  (→ `publishedplaintext`). `memory/` is published, so the same guards read it.
  `train.sh publish --admit` runs only at a terminal (`-t 0`), so cron and
  `land` cannot admit a file, though a shell can (SECURITY.md).

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
- **Layout.** THERE IS NO PYTHON and THERE IS ONE WORKSPACE. `app/` is the
  application — `core` (meaning, pure), `io` (the interpreters), `cli` (the one
  binary, ORGANISED BY VERB: `cli/src/commands/<verb>.rs`). `web/` is the
  frontend, `android/` the portal, `scripts/` the operator's shell, `nix/` the
  deployment, `memory/` the project's memory. New application behaviour goes
  into `app/` (→ `nopython`).
- **THERE ARE TWO CHAINS**: `life` (the owner's todos) is FIRST, and `suzatary`
  is the system's own work. **The routing rule**: anything about the SYSTEM (a
  failing gate, a flaky test, a broken sync) goes to `suzatary`; anything about
  THE OWNER'S LIFE goes to `life`. Routes take the chain as a path segment (→
  `twochains`).
- **The Prodrome is a dependency, not a directory**, taken BY REVISION, the same
  sha in `app/Cargo.toml` and `flake.nix`. Its `SPEC.md` is the database's
  contract and its `conformance/` is FROZEN. The tab's wasm module is this
  repository's own, `app/wasm` (→ `prodromedependency`).
- The shell carries NO INTERPRETER: decisions over JSON are verbs of the binary
  (→ `nointerpreter`). **`suzatary` is NOT on PATH in `.#triage`**: scripts and
  prompts run `nix develop .#triage -c scripts/app-bin.sh <verb>`, always the
  deployed program, and `scripts/app-build.sh` builds a dev loop (→
  `notonpath`).
- **A worktree is not a sandbox**: a fixture that talks to notmuch uses ABSOLUTE
  paths under the agent's scratch dir and its own `NOTMUCH_CONFIG`; a brief
  that can touch mail, secrets, units or roots forbids the live ones by name;
  the reviewer reads any `notmuch-config` an agent wrote before merging (→
  `worktreenotsandbox`).
- **The merge gate is `scripts/merge-gate.sh`**, and a branch reaches master
  only through a green run of it (→ `mergegate`). `--no-live` is the tree half,
  `--only-live` the live half that reads the owner's data; `--list` names each
  step's half and reads (→ `gatehalves`). A tree step's pass is kept by what it
  reads; `memory/` and `docs/` are among the paths the cargo and web steps never
  read (→ `gatekeptpasses`, `gatebuildonce`, `codespacewarm`).
- **The merge queue is `scripts/train.sh`**: reviewed PRs ride a TRAIN (`build`,
  `gate`, `land`); a STACKED PR lands with its base or after it. Nobody merges a
  train by hand (→ `train`). `land` publishes the landed commit's public tree to
  suzatary-core, which every deployment runs (→ `publication`).
- **Deploy is a commit to the user repository, by `scripts/deploy.sh`, and
  nothing else.** A rollback is a deploy of an earlier rev; committing or
  pushing in a dev checkout deploys nothing. Never hand-roll the deploy's wait
  for a `suzatary-roots` run's `Result=success` (→
  `deploy`).
- **Two roots dirs**: the units link into `~/.local/state/suzatary/roots`
  (`SUZATARY_ROOTS`); `app-bin.sh`, `web-dist.sh` and `root-serving.sh` without
  it link into `…/roots-dev`, so a dev build cannot repoint what the units run.
  The roots refresh when the lock moves, and `roots/app` moves last (→
  `rootsrefresh`).
- **A record is shown as a block** (`suzatary_core::decoration`): `raise_block`
  prints it as `#kind(fields)[body]`, and typst is READ with typst's own parser
  only to check that a write renders as what was built.
- **`suzatary web <outdir>`** writes a static site of the attestable history,
  every page Typst behind the tripwire (→ `staticsite`). `suzatary todos open`
  is the open list the briefing reads.
- **The server is `app/`, in Rust** (`suzatary serve`, axum, default
  127.0.0.1:8787; the unit binds the tailnet via `scripts/serve-web.sh`).
  - Reads are GETs. The ONE chain write is `POST /api/objects`, a replica
    handing over its objects (ARCHITECTURE §5). `POST /api/author` builds an
    event and appends nothing; `POST /api/act/device` writes a push token;
    `app/cli/tests/writes.rs` pins that nothing else answers a POST.
  - A tab is a trusted actor exactly when the local Tailscale daemon names its
    peer as a login in `web.act_as` (the actor is that mapping's value, never a
    request field) AND the request carries the CSRF half whois cannot cover:
    `X-Suzatary-Act: 1` (it forces a CORS preflight; the server sends no CORS
    header), `Content-Type: application/json`, and not labelled cross-site by
    the browser.
  - `authoring::confirm_todo`, `build_todo` and `update_todo` are one function
    each, reached by the CLI, the routes and the agent's verbs: callers differ
    only in the ACTOR. A `parent` makes the new todo a subtodo in ONE act.
  - It does everything semantic and authors NO HTML (plain text, JSON and
    Typst); every HTML string it sends is typst's, unaltered.
  - **The deployment is `instance.toml`**, read only by the binary and checked
    at startup; `suzatary doctor` prints what it read (→ `instancefile`).
  - **An address is not an identity**: every request asks tailscaled whose peer
    it is, loopback included; a proxy's header or a foreign `Host` is refused;
    every answer carries `serve::POLICY` (→ `addressnotidentity`).
  - **Mail** is read-only: notmuch as a subprocess, bodies as capped text, the
    agent's disposition a BADGE, never a filter (→ `mailreads`).
- **`web/` is the app's frontend**: TypeScript, one esbuild bundle, no
  framework, no npm (→ `webphone`; ARCHITECTURE §11.1). The rules:
  - The browser never computes a fulfillment of its own: it runs the same
    core as `suzatary-wasm` (→ `onebrowserevaluator`).
  - **Escaping is the DOM** (`web/src/view/dom.ts`) with ONE exception:
    **typst's HTML is the one string the browser parses, through
    `web/src/rich.ts`**, refused by the tripwire if it carries a script, a
    handler or a dangerous URL; `scripts/web-static.sh` fails on any other
    `innerHTML`. Mail bodies are `textContent` (→ `typstfirst`).
  - **The phone is a replica**: an act is sealed in the tab; a REFUSED object
    stays local and NAMED, never retried (→ `phonereplica`).
  - **A reference has ONE text form** that prints and parses (`todo:life/x`,
    `mail:<hash>`, `thread:<id>`, `briefing:<day>`,
    `proposal:<conversation>/<n>`; `suzatary_core::reference::Reference`),
    because a model writes text. A link between todos is derived from content,
    never a stored kind.
  - **Local-first, and staleness is DATED**: the service worker keeps the SHELL
    and nothing else (→ `shellworker`).
  - Wire types AND their decoders are generated from `app/core/src/wire.rs`
    (`scripts/wire.sh`), and `api.ts` names each route once over `http.ts`.
  - **A todo's body and detail are edited as Typst**; the box is still the one
    that refuses (→ `typsteditor`).
  - **Browser tests are the UI's evidence; screenshots are for the eye**
    (`scripts/web-e2e.sh`, headless chromium over CDP, against real `suzatary
    serve` fixtures). The fixture's whois lives behind a cargo feature
    `.#suzatary` does not enable: a feature the deployed build does not compile
    is stronger than a flag.
- **THE INSTANCE FILE HAS TWO ROOTS, NOT ONE**, `CodeRoot` and `DataRoot`; the
  data lives in the PRIVATE data repository, and `instance.toml` here is an
  ignored link to it. Never hand-type the data tree's path: reach for
  `$SUZATARY_REPO`/`$SUZATARY_INSTANCE` or the module's options (→ `tworoots`).
- **The Android app is a PORTAL**: a WebView onto the box, https only, no
  credential, no fold or HTML in Java; native only for OS integration. Every
  APK update MUST use the same signing key (→ `androidportal`).
- **Prodrome** (`events/`): the todo system's own VCS, a content-addressed
  Merkle DAG of events, fold = function of the event SET; shipped kinds' fields
  are frozen and evolution adds kinds; NO tree object (→ `prodromechain`).
- Traps: never patchelf the Drive CLI (Bun payload); nixos consumes this repo
  as a LOCKED path input — `nix flake lock --update-input suzatary` before
  rebuild; mail is a rebuildable cache, never committed.
- x86_64 derivations build on a Codespace remote builder, made from an empty
  PERSONAL repository, never an org's; after a lock change
  re-root the dependency builds, and never `--no-link` a build whose artifacts
  should survive the next collection (→ `remotebuilder`).
- Briefings live in the assistant chat, never systemd/email.
- **Operating checks**:
  - `cargo check` a merge of parallel work before pushing it; a clean rebase
    can still not compile (→ `checkmerges`).
  - `git merge -q` hides a refused merge: check `git log -1` before deleting a
    branch (→ `quietmerge`).
  - Check `df` before believing a build failure (→ `checkdisk`).
  - Ask what a test is about before deleting it with its scaffolding (→
    `askwhattest`).
  - Grep for importers before believing a module has one consumer (→
    `grepimporters`).
  - Before blaming the app for a held port, run `tailscale serve status` (→
    `tailscaleserve`).
  - Set `set -o pipefail` inside `bash -c`; it does not cross it (→
    `pipefail`).
  - Restart a guard only in the same `&&` chain as what it guards (→
    `restartguard`).
  - A guard refusing its author is the guard working: teach it, never bypass
    it (→ `guardrefusesauthor`).
  - A pipeline's status is its LAST command's (→ `pipelinestatus`).
  - `nixos-rebuild switch` re-activates `timers.target` and unpauses timers (→
    `timersreactivate`).
  - Before any claim about a unit's cadence, check what runs: `systemctl
    --user show -p FragmentPath <unit>` (→ `repoisnotbox`).
- **A commit message is a claim about a diff**: `scripts/hooks/commit-msg`
  requires a tracked file the message names to be in the staged diff or
  declared in a `Mentions:` trailer. Per clone: `git config core.hooksPath
  scripts/hooks` (→ `commitclaim`, `hookspath`).

## Versioning, changelog, roadmap

- **THE VERSION IS A READING OF THE PROJECT CHAIN**: a release is a todo on the
  `suzatary` chain whose spec composes its items, declared in
  `app/core/releases.toml`; `scripts/train.sh` cuts it and tags it. Never set
  the version or close `## [Unreleased]` by hand; how a PR names what it
  completes is AGENTS.md's "Releases and `Completes:`" (→ `versionreading`).
  The version is no compiled input: the binary reads `app/release.toml` at run
  time (→ `versionnotcompiled`). The contracts keep their own snapshots
  (`app/core/tests/mcp-surface.json`, the stored vocabulary under
  `app/core/tests/`), so widening one fails a test instead of drifting.
- `CHANGELOG.md` — what changed, per release. The merge gate's `changelog`
  step refuses an `app/` or `web/` change that moves no `## [Unreleased]`
  entry since `origin/master`.
- `ROADMAP.md` — recorded direction, NOT built, plus a Rejected section with
  reasons (a deleted idea gets re-proposed; a rejected one does not).
- **A release is a `v<version>` tag**, made by `train.sh land` alone. CI's
  `quick.yml` runs on every push to master, `tests.yml` on a `v*.*.0` tag, and
  `release-apk.yml` only when the Android shell changed or a MAJOR starts (→
  `ciworkflows`).
