# The documentation

Read in this order: the constitution says what must stay true, the
architecture says how the pieces fit, the specs say what each piece promises,
and the plans are how it got here.

## Start here

- [`../README.md`](../README.md) — what Suzatary is, how it works, the quick
  start.
- [`../CLAUDE.md`](../CLAUDE.md) — **the constitution.** Every invariant and
  the mechanism that holds it: the code's rules, the FPL semantics, the
  limits, the safety model (*Irreversibles* and *Containment*), operations.
  Where any other document disagrees, this one wins.
- [`ARCHITECTURE.md`](ARCHITECTURE.md) — the crates and which way the arrows
  point, rules as types, the laws at the application layer, the replicas, and
  §10's design decisions as built.
- [`guide/guide.typ`](guide/guide.typ) — running your own, from nothing:
  the template, `suzatary init`, the tailnet, the model, mail, deploying.
  `nix build .#docs` renders it to HTML and PDF.
- [`APP.md`](APP.md) — what the app is for: each destination graded by need,
  the flows, what should and must not exist.
- [`VISION-2.0.md`](VISION-2.0.md) — where it is going: a public framework,
  private instances.

## Contracts, specs and laws

- **The Prodrome's [`SPEC.md`](https://github.com/fulfilium/prodrome/blob/main/SPEC.md)**
  — the database's contract: objects, the DAG, the folds, FPL, the laws.
- **The Prodrome's [`docs/design-register-types.md`](https://github.com/fulfilium/prodrome/blob/main/docs/design-register-types.md)**
  — the shape everything here follows: the history is the free
  join-semilattice, a type supplies an order, the application is a reading,
  replicas sync by union, caches are memoised pure functions.
- [`WEB-SPEC.md`](WEB-SPEC.md) — the web app's own contract: its types, the
  doors untyped data enters by, its laws, each naming the test that pins it.
- [`../AGENTS.md`](../AGENTS.md) — instructions for a coding agent: the
  commands, the style rules for the Rust and the TypeScript, and the rules
  for written English.
- [`TS-STYLE.md`](TS-STYLE.md) — how the TypeScript is written.
- [`SIMPLIFY-PLAN-2026-09-28.md`](SIMPLIFY-PLAN-2026-09-28.md) — the long
  form of the Rust style: the types tell the story.
- [`../OPERATOR.md`](../OPERATOR.md) and [`../SECRETARY.md`](../SECRETARY.md)
  — the manuals the triage agent and the secretary work from, pricing table
  included.
- [`OPERATOR.md`](OPERATOR.md) — the operator's notes: the settings of
  suzatary-core that let a train's tree half run on GitHub's runners, and
  where a deploy's builds come from.
- [`CODINGSESSION-ROUTINE.md`](CODINGSESSION-ROUTINE.md) — the fixed brief of
  a cloud coding session the secretary can propose.
- [`BENCH.md`](BENCH.md) — the deployed server's speed, measured.

## Audits

Reads of the whole tree at a moment, and what they found.

- [`AUDIT-2026-09-09.md`](AUDIT-2026-09-09.md) — the structure, after the
  port to Rust.
- [`AUDIT-WEB-2026-09-10.md`](AUDIT-WEB-2026-09-10.md) — `web/`, after a
  design pass.
- [`AUDIT-UX-2026-09-12.md`](AUDIT-UX-2026-09-12.md) — the app, read as a
  person using it.

## The design plans, dated

Each plan was written before its work and is kept as written, so it records
why a thing is the way it is. They are dated, and they are addressed to the
project's owner; where a plan and the constitution disagree, the plan is the
older word. Newest first.

| Date | Plan | About |
|---|---|---|
| 2026-10-21 | [TOPICS](TOPICS-PLAN-2026-10-21.md) | One stream of rounds, with topics that merge |
| 2026-10-21 | [LAZYSYNC](LAZYSYNC-PLAN-2026-10-21.md) | A store's skeleton eagerly, its content by hash on demand |
| 2026-10-03 | [TWOPOINTOH](TWOPOINTOH-PLAN-2026-10-03.md) | 2.0 in waves: every item judged by the register-types structure |
| 2026-10-02 | [APP-AS-READING](APP-AS-READING-PLAN-2026-10-02.md) | The whole app as one reading of history, intents, ui and time |
| 2026-10-02 | [PLUGINS](PLUGINS-PLAN-2026-10-02.md) | The outside world behind WebAssembly interfaces |
| 2026-10-01 | [USERREPO](USERREPO-PLAN-2026-10-01.md) | A private host repository that pins the public app |
| 2026-09-30 | [CONVERSATIONPRODROME](CONVERSATIONPRODROME-PLAN-2026-09-30.md) | A conversation is a prodrome |
| 2026-09-28 | [PROPOSALS](PROPOSALS-PLAN-2026-09-28.md) | A proposal is an object, so a submit is idempotent |
| 2026-09-28 | [SIMPLIFY](SIMPLIFY-PLAN-2026-09-28.md) | The types tell the story |
| 2026-09-27 | [CODINGSESSION](CODINGSESSION-PLAN-2026-09-27.md) | A coding session started from the phone |
| 2026-09-27 | [MULTIPRODROME](MULTIPRODROME-PLAN-2026-09-27.md) | Many prodromes in many repositories |
| 2026-09-27 | [SMALLBOX](SMALLBOX-PLAN-2026-09-27.md) | The box holds mail, secrets and the app, nothing else |
| 2026-09-26 | [LIVEBRANCH](LIVEBRANCH-PLAN-2026-09-26.md) | The box serves a live checkout |
| 2026-09-26 | [MODELCHOICE](MODELCHOICE-PLAN-2026-09-26.md) | Which model answers each job is chosen in Settings |
| 2026-09-25 | [CHAT](CHAT-PLAN-2026-09-25.md) | Many chats, one memory |
| 2026-09-25 | [DESIGN-DECISIONS](DESIGN-DECISIONS-2026-09-25.md) | The design system's decisions for the app |
| 2026-09-25 | [RECURRENCE](RECURRENCE-PLAN-2026-09-25.md) | Recurrence in the term algebra |
| 2026-09-25 | [REDESIGN](REDESIGN-PLAN-2026-09-25.md) | The web app redrawn in the design system |
| 2026-09-25 | [SECRETARYDOCS](SECRETARYDOCS-PLAN-2026-09-25.md) | The secretary reads documents, by permission |
| 2026-09-25 | [TODOTREE](TODOTREE-PLAN-2026-09-25.md) | Todos, subtodos and groups are one tree of terms |
| 2026-09-25 | [TYPST-FIRST](TYPST-FIRST-PLAN-2026-09-25.md) | What is read is Typst; what reacts is TypeScript |
| 2026-09-24 | [GENERALIZE](GENERALIZE-PLAN-2026-09-24.md) | One mechanism per idea |
| 2026-09-24 | [REPOS](REPOS-PLAN-2026-09-24.md) | The secretary in the owner's repositories, by permission |
| 2026-09-23 | [WEBACCESS](WEBACCESS-PLAN-2026-09-23.md) | The secretary on the web, by permission |
| 2026-09-22 | [PHOTOS](PHOTOS-PLAN-2026-09-22.md) | A photo in the chat |
| 2026-09-22 | [PROVIDERSETUP](PROVIDERSETUP-PLAN-2026-09-22.md) | A model provider declared from the app |
| 2026-09-21 | [LAZYBUILDSTALL](LAZYBUILDSTALL-PLAN-2026-09-21.md) | Build when the tree moves, not when the box is asked |
| 2026-09-21 | [OPENSWEEP](OPENSWEEP-PLAN-2026-09-21.md) | The open list swept for evidence, by proposal |
| 2026-09-21 | [SECRETASK](SECRETASK-PLAN-2026-09-21.md) | A secret is asked for in the Inbox, never in a chat |
| 2026-09-19 | [SETTINGS](SETTINGS-PLAN-2026-09-19.md) | A settings screen, and the first secret the app may write |
| 2026-09-19 | [TRIAGEBENCH](TRIAGEBENCH-PLAN-2026-09-19.md) | A benchmark that gates any change of model |
| 2026-09-17 | [INBOXPRODROME](INBOXPRODROME-PLAN-2026-09-17.md) | A decision is a prodrome |
| 2026-09-17 | [MODELROUTER](MODELROUTER-PLAN-2026-09-17.md) | Each job names its own model |
| 2026-09-16 | [WEBALGEBRA](WEBALGEBRA-PLAN-2026-09-16.md) | The web app to the Prodrome's standard |
| 2026-09-15 | [CHATSHAPE](CHATSHAPE-PLAN-2026-09-15.md) | The chat, tightened |
| 2026-09-14 | [DATASPLIT](DATASPLIT-PLAN-2026-09-14.md) | The data in a repository of its own |
| 2026-09-14 | [SECRETARYACTS](SECRETARYACTS-PLAN-2026-09-14.md) | The box's own acts as proposals |
| 2026-09-13 | [ONECHAT](ONECHAT-PLAN-2026-09-13.md) | One conversation that remembers |
