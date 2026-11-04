# Suzatary 2.0: a public framework for private auto-secretaries

Written 2026-09-27 for discussion, from the owner's direction that day:
- typed interfaces that an agent satisfies to build a custom version for each
  person;
- a clear separation between what is user-defined and what is generally
  useful;
- contributions flowing back through PRs;
- a public roadmap kept as a prodrome, with private issues in private
  prodromes;
- the FPL/Typst view of a prodrome made a standard piece rather than part of
  Suzatary.

It ties together SMALLBOX, MULTIPRODROME, CODINGSESSION and the roles. It is
a map, not a series: §9 orders the existing plans as steps towards it, §10
notes related work, and §11 lists what is open.

**This document changes as the abstractions get better** (the owner,
2026-09-28: "we get to change the vision as we get better at thinking about
the abstractions"). Revised 2026-09-28: §1b is new, and §5, §6, §7, §9 and
§11 follow from it.

## 1. The picture

```
┌──────────────────────────── public ────────────────────────────┐
│ Prodrome        the database: chain, FPL, laws, wasm            │
│                 + prodrome-typst (the standard view) + `site`   │
│ Suzatary core   ports + conformance suites, default adapters,   │
│                 the roles, the web app, the NixOS module         │
└─────────────────────────────────────────────────────────────────┘
┌──────────────── private, one per person ────────────────────────┐
│ Instance repo   flake: config, chosen + custom adapters,         │
│                 the developer's CLAUDE.md, private prodromes     │
│ Data repo       chains (life, inbox, project), state, briefings  │
└─────────────────────────────────────────────────────────────────┘
┌──────────────── one small machine ──────────────────────────────┐
│ Box             mail daemons, the search index, secrets, serve  │
│                 reached over a tailnet                          │
└─────────────────────────────────────────────────────────────────┘
      code changes: cloud sessions (routines, or any ACP agent)
```

- **Prodrome** knows nothing of secretaries.
- **Suzatary core** knows nothing of any one person.
- **An instance** knows everything about one person, and nothing it holds
  flows upward except through a reviewed PR (§6).

## 1b. The abstractions underneath (2026-09-28)

Four ideas that make the rest simpler:

- **A prodrome is two layers.** With every price empty, it is close to git's
  object database: immutable objects named by their hash, a DAG of parents,
  union as merge, and tips derived from the objects (git without refs). It
  differs from git in three ways: it stores events rather than snapshots,
  time is data (a fold answers "what was believed at `t`"), and a policy
  decides who binds and who only claims. FPL is the layer on top that makes it
  a prioritisation database. The crates split only if the code shows a
  boundary worth one, and everything stays in one repository.
- **"No price" is a term, `Absent`** (on the Prodrome's main, not yet released). It has no value
  at any instant and counts for nothing when combined. A note, a reference or
  a decision nobody has priced is an ordinary object, with no special cases.
- **A set of prodromes is a prodrome.** A prodrome is named by its genesis. A
  union of several is still a store, one tree per genesis, so "mounting"
  another prodrome only names a tree. The law that keeps it clean is
  DISJOINTNESS: no parent link crosses geneses, and prodromes relate only
  through a qualified `Ref(genesis, id)`. Reading the union is then reading
  each on its own, so each syncs, is trusted and is priced independently. An
  item's identity becomes (genesis, id).
- **An effect is idempotent by content.** A proposal (an email, a changeset,
  an act) is a content-addressed object in the inbox prodrome, with its own
  price, usually `Ref` to the todo it serves. Submitting it is
  `submit(id) -> response`: the effect runs only when the fold holds no
  completion for it, and the completion carries the response. Submitting the
  same object again returns that response. "Nothing was sent" records
  nothing, so trying again is submitting again. No "in progress" state is
  ever visible; a lock under the interpreter handles concurrent submits.
  (PROPOSALS-PLAN-2026-09-28, being built.)

And one for documents:
- **Contracts are files, decisions are items.** Whatever the code must obey
  (SPEC, WEB-SPEC, CLAUDE.md) changes in the same PR as the code
  and stays a file. Whatever has a lifecycle (designs, plans, proposals,
  decisions: proposed, accepted, built, superseded) becomes an item in the
  project prodrome, with a Typst body, `Absent` by default, and a
  `supersedes` link. The viewer reads them with their full history. Not yet
  built; the first candidates are this document's §9 steps.

## 2. Ports: the interfaces an agent must satisfy

A **port** is a Rust trait in `suzatary-core` with typed inputs and outputs,
errors as values, and a CONFORMANCE SUITE. The suite is a set of tests any
implementation must pass against a fake world, plus an optional live smoke
test that only reads.

The code is already built this way: its capability traits (`Clock`, `Whois`,
the mail capabilities, `Drive`, `Model`, `Notify`) are ports that lack only
their suites and a published contract.

| Port | Default adapters | Notes |
|---|---|---|
| `MailSource` | Gmail API, IMAP+OAuth (Microsoft), Proton Bridge | incremental sync, IDLE or push |
| `MailSender` | SMTP submission, Gmail API | irreversible: always behind a tap |
| `SearchIndex` | notmuch | the source of truth for read state and tags |
| `Drive` | Proton Drive CLI, rclone (~70 backends) | list, read, upload; delete, move and share only behind a tap |
| `Calendar` | Google, Graph | new |
| `Notifier` | ntfy, FCM | |
| `ModelBackend` | Claude subscription, Anthropic API, OpenAI-compatible | per-job orders (MODELCHOICE) |
| `SecretStore` | secretspec | values never leave the box |
| `CodeRunner` | Claude routines, any ACP agent (§4) | the developer role's engine |

**Rules for every port:**
- Reads are free. Writes are classified as REVERSIBLE (upload, tag) or
  IRREVERSIBLE (send, delete, share). An irreversible write is always a tapped
  proposal.
- The classification lives in the port's types, so an adapter cannot
  reclassify it.

**Adapters are WebAssembly components from the start** (the owner,
2026-09-27: "why wait for adapters for later?"). The premise is agents
writing adapters for people, which is exactly when a sandbox matters. The
technology is ready: WASI 0.2 is stable, WASI 0.3 adds async, and Wasmtime
is a Rust library the host embeds directly.
- **The contract.** Each port is a WIT interface, generated from, and checked
  against, the Rust types.
- **The manifest.** An adapter ships one declaring the hosts it may reach, the
  secret names it needs, and each write operation's class (reversible or
  irreversible).
- **The grant.** The host gives it exactly that when it is instantiated:
  outbound HTTP to those hosts through `wasi:http`, sockets through
  `wasi:sockets` where a protocol needs them (IMAP, SMTP), and secrets handed
  in per call, never readable by another adapter.
- **One mechanism.** Default adapters are components too, so there is no
  second, trusted path.
- **Local daemons.** notmuch and Proton Bridge are served by the host as
  granted local capabilities, e.g. "this adapter may reach Bridge on
  localhost".
- **Any language.** Adapters can be written in Rust, Go, JS or Python, and
  the conformance suite runs against the component, whatever it is written
  in.

## 3. Connecting components is a conversation, not a settings page

"Connect my Dropbox":
1. The secretary recognises a `Drive` need.
2. **If a default adapter exists**, it proposes a config changeset to the
   instance. **If none exists**, it proposes a DEVELOPER session (§4) to
   write one in the instance repo, which must pass the port's conformance
   suite.
3. Everything that needs the person arrives as a card: a sign-in, an OAuth
   consent, an API key (the secret-request card that exists today), a
   device-code login.
4. The person approves, and `deploy.sh` makes it live: a commit to their
   user repository that moves the app's pin.

**Settings becomes derived.** It lists the components the instance declares.
Each shows its port, its adapter, whether its secrets resolve, its last
success, and whether it passed conformance. Each is rendered from the port's
own description, not from hand-written pages.

**"The secretary can do custom things for people" means exactly this.** It
never gains powers on the fly. Custom needs become configuration or reviewed
code in the person's instance.

## 4. The roles

| Role | Runs | Touches | Its work returns as |
|---|---|---|---|
| **Triager** | box, on new mail | `MailSource`, `SearchIndex` (read), its proposal store | a batch to accept |
| **Secretary** | box, in the app | the chains, reads of every port, writes only through proposals | tapped proposals |
| **Developer** | cloud | the instance repo (and, via §6, PRs to core); no mail, no secrets | a branch and a report, promoted after gates |
| **Operator** | the box's main session | promote, migrations, health | commits and notes |

**The developer speaks ACP.** Claude routines are the default `CodeRunner`,
but the role is an Agent Client Protocol client, so a person can bring any
ACP agent. The app shows a session the way an editor would: its plan, its
diffs, its permission requests.

**Cloudflare's agent platform (Flue, Durable Objects) is a possible HOST for
the secretary** for people with no box at all. The mail-bound parts still
need a machine, so it is an adapter target, not the default.

## 5. Public and private prodromes

- **Private.** Each instance keeps its own prodromes in its data repo: life,
  inbox (decisions priced by what they serve, MULTIPRODROME §5b) and project
  (the instance's own issues and roadmap). Private stays private: the
  personal-terms guard (DATASPLIT §9.1) generalises to every upstream path.
- **Public.** The core repository carries ONE public prodrome, `roadmap/` (the
  Prodrome repository already keeps one this way). It holds the framework's
  roadmap, priced with FPL like any todo, so ordering is by fulfillment, not
  by label.
- **Issues stay on GitHub.** That is where contributors are, and asking a
  stranger to learn a database to report a bug is a barrier. A GitHub Action
  mirrors each issue into the public roadmap:
  - a new issue becomes a todo (kind `issue`, linking back);
  - close and reopen become the lifecycle events;
  - a maintainer's `/price` comment becomes its spec;
  - Claude's suggested price is stored as a CLAIM by `pricing-bot`, shown
    beside the item and binding nothing until a maintainer replies
    `/price accept`.

  The roadmap is then the priced view of the issues, and GitHub stays the
  door. **Built in the Prodrome repository** (2026-09-28): `prodrome-github`,
  its own crate apart from the core, with `mirror.yml` and `price.yml`, and
  the roadmap on GitHub Pages. The core repository reuses it once public.
- **Private to public in one step.** When an instance's project prodrome
  holds an issue that is really about the core, the operator (or the
  secretary, as a proposal) files the GitHub issue. From there the mirror
  carries it.

## 6. Contributing back

- **Every change to the core is a PR.** That includes the owner's own changes,
  once the core is public: the owner's instance is just the first instance.
  Since 2026-09-27 this holds already: a cloud session opens one PR,
  described in two or three plain sentences, and the main session reviews it,
  sends fixes back or makes them, runs the gates and merges it.
- **The developer role upstreams.** A change made in an instance that touches
  only generic code becomes an "upstream PR" proposal. On a tap, the developer
  opens it from a fork, after the personal-terms guard and the core's gates
  pass. Nothing leaves an instance without the tap, and nothing personal
  leaves at all.
- **Adapters are contributions too.** A new default adapter is accepted
  upstream when it passes its port's conformance suite in the core's CI.
- **Licence:** MIT OR Apache-2.0 (already). The core becomes public from a
  fresh history, after DATASPLIT §9.5.

## 7. The Prodrome as a standard, not a Suzatary feature

What Suzatary draws today should be anyone's for any prodrome:
- **`prodrome-typst`,** a Typst Universe package. It has the FPL figures, the
  explain tree, and a todo list and roadmap layout, all taking a prodrome's
  exported fold as data. The figures (settled 2026-09-28) use ONE continuous
  colour, Typst's OKLCH gradient from red to green sampled at the value, never
  three discrete state colours: a small pie on a row, and for thirty days one
  thick line coloured by its own value inside a 0–100% frame.
- **`prodrome site`** (CLI). It renders a store to a static page plus PDF:
  the roadmap ordered by fulfillment, each item's curve and its history.
  Any repository with a prodrome gets a roadmap page on GitHub Pages from one
  Action.
- **A small web view library** for pages that fold live over the existing
  wasm core: the pie, the line, the list. The Suzatary app uses it too, so
  there is one implementation.

The public roadmap's "nice visualisation" is this, pointed at the core
repository's `roadmap/`. **Built** (2026-09-28): the Prodrome's viewer on
GitHub Pages folds in the tab and typesets with Typst compiled to wasm, with
a Typst editor demo. Typst, the viewer and the GitHub mirror are optional
extras; `core/` and `cli/` know none of them.

## 8. Hosting and the network

- **The machine.** A small NixOS machine: the person's own or a small VPS. The
  instance flake builds the whole system, and builds come from a binary cache
  (SMALLBOX rule 1), so a new machine is a switch, not a compile.
- **Reaching it: one person, one connection.** The owner's idea, stated
  plainly: the app is reachable by exactly the people the instance names, and
  by no one else. There are two free-tier ways, each paid for (or not) by the
  person:
  - *Default: a tailnet.* The box joins it, eventually as its own node
    through `tsnet`/`libtailscale` with no system daemon, and the tailnet
    identity is the login (as today). Phones need the Tailscale app.
  - *For people who will not install Tailscale: Cloudflare Tunnel plus
    Cloudflare Access.* The box dials out, so no port is open. Access admits
    only the named identities (an email one-time code, Google, or a passkey)
    at Cloudflare's edge, and the app verifies Access's signed JWT as the
    login. The free tier covers 50 users.
- **Nobody pays us.** There is no hosted service: each person brings their
  own model subscription, machine or VPS, and free tiers. This matches the
  Prodrome's non-goals ("multi-tenancy; a hosted service").
- **Onboarding.**
  1. `suzatary init` runs WITH the person's Claude Code. It creates the
     instance repo, the data repo, the developer routine (through the
     routines API, as on 2026-09-27) and the tailnet node.
  2. The secretary's first conversation connects the components (§3).
  3. The only manual step left is the routine's fire token, which the web UI
     alone mints.

## 8b. The app is local-first

The owner, 2026-09-27: "we can do some local storage to make things really
cool". The phone is already a replica: chain objects in IndexedDB, folded
offline by the wasm core, acts sealed locally. Taking that seriously:

- **Content-addressed caches persist.** Rendered Typst HTML, answer blocks
  and objects are named by their hash, so a stored copy is never stale. They
  move from per-tab memory into IndexedDB, and open instantly and offline.
- **Offline mail.** Recent and relevant threads, and their attachments, are
  cached from the box's notmuch search. Read and unread and tag changes queue
  locally and sync back as notmuch tags, the replica pattern again. notmuch
  stays the one truth.
- **Typst compiles in the browser, by default** (the owner, 2026-09-27:
  "if we had typst run client side as wasm is that not better?"). Everything
  in the replica renders offline, nothing goes stale, and a page that prints
  now-values just recompiles.
  - The compiler is the SAME Typst crate the box runs (0.15.1 today), built to
    wasm by this repository, not a third-party bundle at another version.
    typst.ts's web compiler is about 27 MB unpacked, as a scale.
  - It is downloaded once per build and kept by the service worker, together
    with TTF/OTF fonts, because Typst reads no woff2.
  - Pages compile lazily, as they scroll into view.
  - The door moves with it: the tripwire (`render::suspicious`) and the rule
    that model-written Typst may not use the `html` module or `eval` run
    inside the same wasm module, before anything reaches the page.
  - The box keeps rendering as the fallback for a first visit and for weak
    devices.
- **A real Typst editor comes with it** (the owner: "we can even offer typst
  autocomplete and syntax highlighting right?"). The same wasm module carries
  `typst-syntax` and `typst-ide`, the crates Tinymist is built on. They give
  exact highlighting coloured by the design tokens, completion, hovers and
  signature help for Typst and the theme's helpers (`#panel`, `#note`,
  `#sep`, …), a live preview on every keystroke, and errors underlined as the
  person types.
  - Our vocabulary joins the completions: `todo:` suggests real todos from
    the replica, and categories and people follow.
  - A body that does not compile is flagged before it is sealed. That is a
    candidate law: every stored body compiles.
  - It appears first in the add sheet and the edit form (body and detail),
    which already say they take "typst markup source".
- **Cache budgets, each shown and adjustable in Settings.** Eviction is least
  recently used, EXCEPT that what serves a low-fulfillment todo is kept
  longest: the price decides what is worth the space.

  | Part | Default |
  |---|---|
  | chain replicas | complete (all of today's data is 28 MB); shallow replicas later (below) |
  | conversations | complete, text only |
  | compiled-output memo | a few MB, keyed by source hash |
  | Typst compiler and fonts | one per build, kept |
  | mail bodies | about 30 days, plus anything a todo links |
  | attachments | opt-in, e.g. 200 MB |
  | local search index | derived from what is cached, never synced |

- **Shallow replicas, when a prodrome grows large.** Like a shallow git clone,
  the phone holds a verified checkpoint of the fold at some tip, plus the
  objects after it, and fetches older history on demand. This is a Prodrome
  feature, with a law: folding from the checkpoint plus the tail equals the
  full fold.
- **Local search.** A small wasm full-text index over cached mail, todos and
  conversations gives instant results offline.
- **An app shell keyed to the build hash.** A service worker serves the shell
  with no network. Each build has one identity, and the app knows when a new
  one exists and offers to reload. That ends the stale-bundle confusion
  (2026-09-27: a phone showed every todo unpriced while the desktop was
  right).
- **Persistent storage.** Call `navigator.storage.persist()`, so the replica
  is not evicted under pressure.
- **Encrypted at rest.** A non-extractable WebCrypto key per device encrypts
  the local store. It is the same device key the signatures stage needs, so
  one mechanism serves both.
- **With GitHub as the hub** (MULTIPRODROME), a verified phone replica can
  sync with the hub directly while the box is down.

## 9. How today's plans become steps

Where each stands on 2026-09-28. The Prodrome half is ahead; the half that
lets a second person run Suzatary (steps 3, 5, 6 and 8) has not started, and
is where attention goes next.

1. **SMALLBOX.** Builds and code leave the box. Code already has: every
   change comes from a cloud session as a PR. Builds and gates have not: the
   box still runs 35-minute gates. CI and a binary cache come when CI minutes
   return on 1 Oct, so every PR is checked off the box.
2. **Roles.** The developer is live (routines and cloud sessions, landing by
   PR). Still to do: name the four roles in CLAUDE.md and ARCHITECTURE; ACP
   later.
3. **Tests a stranger can trust.** One law, one test, instead of the same
   rule checked in many places; fixtures built by running the real code,
   never written by hand; gates fast enough to run on every PR. A
   precondition for contributions, learned the hard way on 2026-09-27.
4. **The first port with a suite: `Drive`,** with the component host.
   Proton Drive and rclone are its first two adapters, as components, and
   upload is part of the port. This proves the port and suite pattern
   on something small.
5. **Many prodromes** (§1b). Done: derived tips (0.9.0) and `Absent`. Being
   built: the inbox prodrome, with proposals as objects. Next: identity by
   genesis, disjointness and the qualified `Ref`, which also price proposals
   by the todo they serve. Then decisions as items.
6. **The instance split.** Move the owner-specific code (adapters wired for
   these accounts, the persona) out of the core into an instance flake that
   takes the core as an input.
7. **The public core.** A fresh history after DATASPLIT §9.5, the `roadmap/`
   prodrome, the issue mirror (built, in the Prodrome), and upstream PRs.
8. **`prodrome-typst` and the viewer** in the Prodrome repository: built, with
   the marks being redrawn. `prodrome site` becomes the viewer's static
   export.
9. **Onboarding:** `suzatary init` with the person's Claude Code.

## 10. Related work (a quick search, 2026-09-27)

- **OpenClaw** (MIT): the closest in spirit. A self-hosted personal assistant
  with persistent memory and a skills system, connecting to messaging apps
  and triaging email, for the cost of a VPS plus model usage. QwenPaw and Jan
  Agent are similar.
- **Home Assistant:** the architectural precedent.
  - Community integrations behind common interfaces.
  - Config flows that walk a person through sign-ins.
  - Local-first, with contributions flowing upstream.
  - A remote-access service (Nabu Casa).
- **Not seen combined elsewhere:**
  - a typed, append-only database of commitments priced by fulfillment (the
    Prodrome), rather than chat memory;
  - sandboxed, conformance-tested adapters written by the person's own agent;
  - a propose-then-tap rule for irreversible acts, in the port types;
  - per-person instances that upstream their generic improvements.

## 11. Open, for discussion

1. **Adapters are components from the start** (decided 2026-09-27). Still
   open: how much of today's native mail code becomes components in the first
   series, and how much stays as host capabilities?
2. **The instance split:** does the owner's instance live in the data repo,
   or in its own repo beside it?
3. ~~Issue pricing~~ — settled 2026-09-28: a maintainer's `/price` comment,
   with Claude's suggestion stored as a claim until `/price accept`.
4. **ACP:** implement the client in the web app, or on the box and relay it?
5. **Tailscale:** embed `tsnet` in `suzatary serve`, or keep the system
   daemon?
6. **Naming:** "Suzatary" for the framework, and something else for an
   instance?
7. **Where the effect lives.** Is "a request object, and one binding
   completion carrying its response" a Prodrome operation that every host
   gets, or a Suzatary rule written over plain objects? It starts in
   Suzatary (PROPOSALS-PLAN) and moves down if a second host needs it.
8. **Decisions as items:** which record kind, and does merging a PR complete
   its item automatically, through the GitHub mirror?
