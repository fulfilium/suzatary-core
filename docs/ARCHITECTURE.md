# The architecture

This document is the map. It names each unit of the repository, shows how data flows between
them, and lists the invariants that a change can break. `CLAUDE.md` holds the rules and wins
where the two disagree. Each unit's `README.md` says how to use it, and `context.toml` lists the
units and their dependencies. The database's contract is the Prodrome's `SPEC.md`
(<https://github.com/fulfilium/prodrome>).

## 1. The data flow

```
 mail servers ──IMAP/lieer/Bridge──▶ maildir + notmuch ◀── suzatary mail … (cli, io)
                                          │
                           every 5 min: scripts/hourly-triage.sh
                                          │
                       claude -p, fenced to the `suzatary mcp` verbs
                                          ▼
 ┌──────────────── the data root (the user repository) ─────────────────┐
 │ events/, events-suzatary/  the todo chains (Prodrome DAGs)          │
 │ inbox/                      the proposals: triage cards, emails …   │
 │ secretary-lines/ …          conversations, memory, digests, topics  │
 │ state.json, local/, cache/  dedup, box-local tier, plugin stores    │
 └───────────┬──────────────────────────────▲─────────────────────────────┘
             │ fold (prodrome-core, core)   │ append (a tap, a trusted write)
             ▼                              │
     suzatary serve (cli) ── GET reads, POST /api/objects, taps ──┐
             │  ▲                                                  │
             │  └─ secretary: model port (claude CLI) ─ verbs ─ proposals
             ▼                                                  │
   tab: web/ + app/wasm + app/pages ◀── IndexedDB replica ──────┘
             ▲
   android/ portal (a WebView, push)

 scripts/safe-push.sh ─▶ the data repository's remote      vault ─▶ Proton Drive
 suzatary send / a tap ─▶ io::smtp (the one path outward for mail)
 plugins timer ─▶ app/plugin host ─▶ plugin-ics ─▶ cache/plugins/<name>/store
```

- The mail is a cache. The routine sync reads INBOX and the last seven days; the timer runs
  `scripts/hourly-triage.sh`, which syncs, runs the triage agent, and hands the data to
  `scripts/safe-push.sh`.
- The triage agent reads attacker-controlled mail. It holds only the `suzatary mcp` verbs, and
  each of its writes becomes a card in the inbox. Only the owner's tap performs a card.
- The secretary is a conversation in the app. The box asks the model through the model port,
  runs the verbs the model asks for, and stores each proposal in the inbox. The tap route
  performs it.
- The tab holds a replica of each store and folds it with the same core as the box (through
  `app/wasm`). It exchanges objects with the box by name through `/api/objects` and
  `/api/stores/<store>/objects`.
- A deploy is a commit to the user repository by `scripts/deploy.sh`. The units run the app
  that the user repository's lock names.

## 2. The units

Arrows point down: a unit depends only on the units below it in its list of dependencies.

**The Prodrome** (external, `prodrome-core`, `prodrome-wasm-exports`, `prodrome-typst-wasm`).
The temporal FPL database: literals, objects and the DAG, the trust rule, folds and registers,
the evaluator, the closed-world Typst compiler. It knows nothing of Suzatary. It is pinned by
revision, the same sha in `app/Cargo.toml` and `flake.nix`. Read its `README.md` and `SPEC.md`.

**core** (`app/core`, `suzatary-core`). The application's meaning, pure: no I/O, no async, no
clock. It owns the record (`record.rs`), the capability traits and the records of them that
each role holds (`capability/`), every store's row (`store/kind.rs`, the `stores!` table), the
chain set that prices every row (`chain_set`), the agent's and the secretary's verbs (`agent/`,
`secretary/`, `verb/`), the wire types (`wire.rs`), signatures, the gate's decisions and the
replicas the tab holds (`replica/`). It depends on the Prodrome. Start at its `README.md`, then
`lib.rs`, `record.rs` and `capability/`.

**io** (`app/io`, `suzatary-io`). The interpreters: each capability trait run against the
world (files, subprocesses, IMAP and SMTP, HTTP, the `claude` CLI, notmuch, tailscaled). It
makes no decisions. It depends on core and the Prodrome. Start at `run.rs` (every subprocess),
`objects.rs` (a store on disk) and `model.rs` (the model port).

**plugin** (`app/plugin`). The plugin host: wasmtime components of `app/wit/plugin.wit`,
each call in a fresh store under a time and memory budget, each `fetch` admitted against the
plugin's manifest. It depends on core and io. Start at its `README.md` and `Host::load`.

**plugin-ics** (`app/plugin-ics`). The calendar plugin, built to `ics.wasm`, with its schema
`ics.schema`. **plugin-fence** (`app/plugin-fence`) is a hostile plugin that
`app/plugin/tests/fence.rs` runs against the host. **testkit** (`app/testkit`) writes and runs
fake programs for tests.

**cli** (`app/cli`, the binary `suzatary`; `suzatary-dev` adds the fixtures). One module per
verb, `src/commands/<verb>.rs`; `src/verb.rs` is the argument tree and the one `match`. It
owns the runtime (tokio, axum), the instance file (`instance/`), the HTTP server
(`commands/serve/`, its routes in `serve/table.rs`), the tap that performs a proposal
(`commands/tap.rs`) and the one file that names the mail transport
(`commands::mail::deliver`). It depends on core, io and plugin. Start at `verb.rs`, then the
verb you change.

**wasm** (`app/wasm`). The tab's module: the Prodrome's exports at this application's schemas,
with the evaluator, so the tab and the box give the same answers. It depends on core. Start at
its `README.md`.

**viewtree** (`app/viewtree`). A page's view as data, the keyed diff and the patches a painter
applies. It knows nothing of Suzatary. Start at `program.rs` and `patch.rs`.

**pages** (`app/pages`). The app's pages as `viewtree` programs, compiled to wasm for the tab.
It depends on core and viewtree. Start at `lib.rs` and `runtime.rs`.

**web** (`web/`). The frontend: TypeScript, one esbuild bundle, no framework, no npm. It owns
the shell (`src/app/`, `src/shell/`), the replicas' sync (`src/prodrome/`), the painter of the
pages' patches (`src/painter/`), the DOM door (`src/view/dom.ts`) and the one door for Typst's
HTML (`src/rich.ts`). Its wire types and decoders are generated from `app/core/src/wire.rs`
(`src/wire/generated/`). It depends on core, wasm, pages, viewtree, `prodrome-typst-wasm` and
the design system `typst-design`. Start at its `README.md`, `src/main.ts` and `src/app/app.ts`;
`docs/WEB-SPEC.md` holds its doors and the laws no test checks.

**android** (`android/`). A portal: a WebView onto the box, https only, no credential and no
fold. Native code exists only for OS integration: deep links, push (`Push.java`,
`PushReceiver`), the share target and the widget. `nix/apk.nix` builds the APK without
Gradle; `scripts/sign-apk.sh` signs it. Every update uses the same signing key.

**scripts** (`scripts/`). The operator's shell, with no interpreter: decisions over data are
verbs of the binary. The merge gate (`merge-gate.sh`), the merge queue (`train.sh`), the
deploy (`deploy.sh`), the data push (`safe-push.sh`), the triage pass (`hourly-triage.sh`),
the agent's fence (`tool-guard.sh`, `triage-fence.sh`), the web build and tests
(`build-web.sh`, `web-test.sh`, `web-e2e.sh`, `web-static.sh`) and the generators
(`wire.sh`, `gen-tokens.mjs`).

**nix** (`flake.nix`, `nix/`). The build and the deployment. The flake builds every package
(`.#suzatary`, `.#suzatary-wasm`, `.#suzatary-plugins`, the APK) and the dev shells (`.#triage`
is the operator's). `nix/nixos-module.nix` declares the user units: `suzatary-web`,
`suzatary-triage` (its timer), `suzatary-plugins`, `suzatary-mail-idle`, `suzatary-roots`,
`suzatary-vault` and `proton-bridge`.

**The manuals.** `OPERATOR.md` is the triage agent's prompt, `SECRETARY.md` the secretary's,
both read at run time through `capability::Manuals`. `AGENT.md` is the guide for setting up a
deployment; a guard holds its commands to the binary. `memory/` holds the project's recorded
decisions.

## 3. The stores

Every store is one row of `stores!` in `app/core/src/store/kind.rs`, with its tier: PUSHED
(committed to the data repository), LOCAL (on the box and in the vault, never pushed) or CACHE
(on the box, rebuilt by syncing). A prodrome store has its own schema and its own hold.

| Store | What it holds | Tier |
|---|---|---|
| chains (`events/`, `events-suzatary/`) | the owner's todos (`life`, first) and the system's (`suzatary`) | pushed |
| inbox | each proposal and its phase, a register that only climbs | pushed |
| lines | the conversations; each line's words sealed per slot | pushed |
| line keys | the keys of the lines' words | local |
| memory, memory texts | which facts stand; each fact's words | pushed, local |
| digests, digest texts | the summary tree of the conversations; its words | pushed, local |
| decisions | the recorded decisions | pushed |
| topics, topic-aliases | the topics on rounds and notes | local |
| plugin | what a plugin learned, at its declared schema | cache |
| triage (`state.json`) | the dedup authority, an HMAC per message | pushed |
| requests | each signed request the server performed | pushed |

## 4. The invariants a change can break

Each invariant names the mechanism that holds it. When the mechanism refuses your change, the
change is wrong, not the mechanism.

**Meaning**

- A todo's history is one fulfillment function, a `Piecewise` derived by `flatten`. A later
  event never changes how an earlier moment reads. Held by the Prodrome's laws and
  `app/cli/tests/live.rs`.
- Order is causal and time is data: ancestry alone decides what a write supersedes. Two
  concurrent writes to one register are a conflict, shown whole, never a winner. Held by
  `prodrome_core::fold` and its laws.
- One evaluator prices: the Prodrome's core, on the box, in the CLI and in the tab
  (`app/wasm`). The browser computes no fulfillment of its own. The conformance vectors are
  frozen. Held by `web/test/wasm.test.ts` and the Prodrome's conformance tests.
- Low fulfillment is urgent. Never flip the renderer.
- The inbox's phase only climbs, and `inbox::submit` under `submit.lock` is the only door that
  performs a proposal. Held by `app/core/tests/inbox.rs` and `inbox_schema.rs`.
- Every stored format is frozen once shipped; evolution adds kinds. Held by the stored
  vocabulary snapshots under `app/core/tests/` and `live.rs`.

**Authority**

- The triage agent cannot send and cannot write a chain. Its verbs take an `Agent` record
  with no field that sends; its writes are cards. Held by the `compile_fail` doc tests in
  `app/core/src/agent/record.rs`, the snapshot `app/core/tests/mcp-surface.json` and the fence
  `scripts/tool-guard.sh`.
- Only the owner's tap sends mail or performs a proposal: a `Tap` minted by `Tap::of` from the
  door's proof. Exactly one file names the transport. Held by `app/cli/tests/writes.rs`.
- The secretary's verb list is closed. Held by `app/core/tests/secretary-surface.json`.
- An address is not an identity: every request asks tailscaled whose peer it is; loopback
  never writes. The one chain write is `POST /api/objects`. Held by `app/cli/tests/writes.rs`
  and `gate.rs`.
- Every writer signs, and every write route asks `Trust::admits`. Held by
  `app/cli/tests/signatures.rs`.
- A secret is set by a tap and never read back. A probe never demands a login (`Consent` has
  two constructors).

**Content and escaping**

- A writer's markup enters through `MarkupSource::checked` and compiles in the closed world
  (no `html`, no `plugin`); data enters through `escaped`. Held by the escape's laws and the
  decoration's `every_row_renders_verified`.
- The browser has three doors: the DOM (`web/src/view/dom.ts`), Typst's HTML through
  `web/src/rich.ts`, and the generated decoders. Held by `scripts/web-static.sh`.
- Every wire type is declared once, in `app/core/src/wire.rs`; the TypeScript is generated.
  `scripts/wire.sh` writes it into `web/src/wire/generated/`, which git ignores, and the type
  check reads the page against it.
- Every identity number is redacted before a model sees it. Held by the redaction laws in
  core.

**The repository**

- Nothing personal is in a tracked file. Held by the leak guards
  (`app/cli/tests/guards/leak.rs`, `scripts/public-tree.sh`).
- A unit's USE stays within `budget.use`, and the reading set does not grow past its ratchet.
  Held by `app/cli/tests/guards/context.rs`.
- A branch reaches master only through a green `scripts/merge-gate.sh` run and the train.
- An `app/` or `web/` change moves an entry of `CHANGELOG.md`'s `## [Unreleased]`. Held by the
  gate's `changelog` step.
