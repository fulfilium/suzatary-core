# The project's prodromes are prodromes shared with the owner — 2026-10-25

The owner, 2026-10-25:
- "I want the project's prodrome to just act like a prodrome that's shared
  with me. So it stays general, I should have edit access to change the
  priority and launch an agent to build it, but also it's okay if the
  workflow is different and linear by default."
- "In theory this should simplify the code."
- "It might be worth having memory that has to do with implementation
  similar to optmem so we can also share that with those agents as well."

Earlier the same day, about failing sources (`docs/SOURCES-PLAN-2026-10-24.md`
§8.1, PR #247): a persistent failure files a todo "that the agent can fix, the
agent can pick up the most important tasks first".

His answers to this plan's questions are in §9. The plan below already
follows them.

**The test this plan is held to: it removes more code than it adds.**
- §1 measures every line that exists only because the project is special.
- §2 shows that being "shared" needs nothing new beyond what a `[[chains]]`
  entry already holds, except a home.
- §3–§6 give each change with its line count.
- §7 gives the stages, with what each deletes and adds.
- Where a part of the request adds code with nothing to delete in return,
  the stage says so and why.

Line numbers are at `origin/master` 007868d (release 2.1.17). Counts use
`scripts/size.sh`'s rule:
- for Rust, `app/*/src` up to each file's first `#[cfg(test)]`;
- for TypeScript, `web/src` outside `generated/`;
- blank and comment lines are not counted.

Shell and Markdown are listed separately, because the rule does not count
them. Counts of lines still to be written are estimates and are marked ≈.

## 1. What exists only because the project is special

| # | Where | What | Lines |
|---|---|---|---|
| P1 | `app/core/src/coding.rs:241-249` | `handed_todo`: the `merge<hex>` id "Hand to the main session" writes | 6 |
| P2 | `app/core/src/secretary/routes.rs:82-83`, `:117-127`, `:158`, `:242-265` | the `handover` chain on a drawn card, and `handed()`, which looks for that todo | 36 |
| P3 | `app/core/src/secretary/routes.rs:2128-2238` | `hand`: writes the hand-over todo onto `[coding] chain` | 96 |
| P4 | `app/core/src/secretary/readings.rs:1148`, `app/core/src/wire.rs:2452-2453` | the wire's `handed` field | 2 |
| P5 | `app/cli/src/commands/serve/proposals.rs:307-316`, `serve/table.rs:105-109` | the `…/proposals/<n>/hand` route | 13 |
| P6 | `web/src/inbox/coding.ts:73-76,94,101`, `web/src/wire/api.ts:258-260`, `web/src/prodrome/asks.ts:17,99` | the "Hand to the main session" button and its call | 10 |
| P7 | `app/core/src/deployment.rs:104-109` | `Deployment::project()`: "second in the instance file's order of meaning" | 4 |
| P8 | `app/core/src/agent/reads.rs:120-123` | the agent told `suzateryTodo` is "this system's own upkeep" | 4 |
| P9 | `web/src/app/chain.ts:10-15`, `web/src/shell/route.ts:5,82-83,147-148`, `web/src/shell/nav.ts:33,61,79-80`, `web/src/shell/keys.ts:43`, `web/src/shell/layout.ts:25`, `web/src/pages/todos.ts:414` | `PROJECT_CHAIN = "suzatary"`, its label, and a `#/project` route beside the chain picker | 15 |
| | | **Total** | **186** (161 Rust, 25 TypeScript) |

These places name the project but need no deletion:
- **`KINDS`** (`app/core/src/decoration/mod.rs:25`). Taking `"suzateryTodo"`
  out of it is an edit of one line (§4). `CATEGORY_KINDS` and its two
  functions (`:30-46`), and their copy in `web/src/todo/grouping.ts:127`,
  serve the `life` kind `ekaTodo`, not the project.
- **The failing-mail todo's shape** (`app/core/src/secretary/mail_health.rs:54`).
  `"suzateryTodo"` becomes `"todo"`, also an edit. `propose_persistent`
  already takes its chain by name (`:76-88`); only its caller (P7) is special.
- **Onboarding** (`app/cli/src/commands/init/onboarding.rs:184-187`).
  `CHAINS` is the template a new deployment starts from. It names what it
  creates, and nothing reads the name back.
- **Doctor** (`app/cli/src/commands/doctor.rs:89`). That line is clap's
  command name. `doctor` only lists chains (`:132-155`).
- **The release reading.** `chain = "suzatary"` (`app/core/releases.toml:9`)
  is already configuration:
  - `Plan` carries the chain as data
    (`app/core/src/release/plan.rs:10-16,147-150`);
  - `release` opens the chain of that name
    (`app/cli/src/commands/release.rs:137`);
  - `Reading`'s `refs` filters by the name it is handed
    (`app/core/src/release/reading.rs:256-265`).

  Moving the releases to a new prodrome (§6) is a one-line edit of that
  file.
- **Shell and manuals.**
  - `scripts/safe-push.sh:131-133` lists `events-suzatary/` as it lists every
    store.
  - `scripts/hourly-triage.sh:227` tells the agent `chain="suzatary"`.
  - `OPERATOR.md:110-132`, `SECRETARY.md:586-587` and `ROADMAP.md:9-13` name
    the project chain.

  These change in words, with the stage that moves what they describe.

## 2. What "shared" needs: nothing new but a home

**Members already exist.** A `[[chains]]` entry carries `actors` (the
roster), `untrusted` and `owner` (`app/cli/src/instance/file.rs:128-144`).
`ChainDir::of` refuses an owner who is off the roster or untrusted
(`app/cli/src/instance/chain_dir.rs:43-50`). That IS the member list. A
personal chain is the case of one human member, and so is the project's.

**The owner's rights already exist.**
- He is the project chain's `owner` (`app/cli/src/instance.rs:808-811`;
  `suzatary init` writes both chains that way, `onboarding.rs:184-187`).
- Editing a todo and changing its priority go through the one
  `authoring::update_todo` on every chain (CLAUDE.md, "The server is
  `app/`").
- He can launch an agent today: the coding session is his tap firing the one
  routine (`app/cli/src/commands/coding.rs:36-80`; CLAUDE.md, "A coding
  session is the owner's tap").
- What is missing is launching one FROM A TODO (§3).

**Readers who are not the owner** (the coding agents, §5) read a prodrome by
cloning its home. The door's `viewers` and `act_as` are not involved. Those
two are deployment-wide (`app/core/src/deployment.rs:20-25`), and a login
there reads the owner's mail. So no member ever comes in through them, and
no roles table is added.

**A home is the store's path.** `events` resolves under the data root, or
stands as given when absolute (`app/cli/src/instance/roots.rs:6-12`). A store
in another repository's checkout therefore needs no Rust, only that the box
commits and pushes that checkout (§6).

| | Code repository (`master`) | An orphan data branch of a repository | A repository of its own | The data repository |
|---|---|---|---|---|
| Writes | through the train only (CLAUDE.md, "The merge queue") | commits the box pushes to that branch alone; never `master`, never the train | commits the box pushes | as today |
| Readable by a cloud session | yes | yes when it is a branch of `bmabsout/Suzatary`, the routine's repository; a branch of the public mirror is readable once that mirror is public | once granted to the routine and the environment, read-only (§6) | no |
| Published | with the code, through `public-tree.sh` | with the repository it lives on | on its own decision | never |
| Precedent | — | the Prodrome's `roadmap-data` (below) | — | every store today |
| Fits | rarely written knowledge that changes with the code: the memory (§5) | rejected for the todos (§6): the owner chose a repository of its own | frequently written work: the project's todos (§6), the owner's choice | private work: upkeep (§4) |

**The precedent: the Prodrome keeps its own roadmap as a store on an orphan
branch.** (At the pinned revision `de71ccc`, `README.md:30-56`,
`docs/github-agent.md`; there is no `docs/roadmap.md` at that revision.)
- **The store.** The roadmap is a Prodrome store on `roadmap-data`, apart from
  `main`.
- **Issues are items.** `mirror.yml` turns an issue's events into `gh-<n>`
  items, through the deterministic, offline `prodrome-github` binary.
- **A pricing bot proposes.** `price.yml` has Claude suggest a price. The
  suggestion is stored as `pricing-bot`'s claim, which binds nothing until a
  maintainer replies `/price accept`.
- **CI reads it.** CI checks out the branch and runs `verify` and `list` over
  it on every push to `main` (`.github/workflows/ci.yml:92-108`).
- **The push is a union.** `.github/scripts/roadmap-push.sh` pushes only new
  object files. A race is a merge of disjoint files followed by a retry.

The owner's words (2026-10-25): "The public prodrome would basically be like
the prodrome's prodrome." §6 takes its workflows, its CI check and its push.
It does not take its home: the owner chose a repository of its own over an
orphan branch (§9).

## 3. Launch an agent from a todo; the hand-over goes; at most two at once

**Why the hand-over can go (P1–P6, 163 lines).** "Hand to the main session"
writes a `merge<hex>` todo on `[coding] chain` so that the main session merges
the branch (`secretary/routes.rs:2128-2238`). But the routine already opens
ONE pull request, and the main session reviews and merges it
(`docs/CODINGSESSION-ROUTINE.md`, rule 3). The pull request IS the hand-over,
and the train marks the todo done from its `Completes:` line
(`AGENTS.md:342-359`).

**Building from a todo reuses the coding session as it is.** "Build it" on a
todo of `[coding] chain` asks the box to file the existing
`Act::CodingSession { task }` card.
- The card's `serves` is the todo. Every proposal already has the field
  (`app/core/src/secretary/verbs/filing.rs:30-35`), and the inbox prices a
  card by the todo it serves (CLAUDE.md, "A PROPOSAL IS PRICED BY THE TODO IT
  SERVES").
- The `Task` (`app/core/src/coding.rs:56-95`: plain text, at most 4 KB) is a
  fixed template around the todo's stored text:

  ```
  Build todo:<chain>/<id>. Its text follows, as the owner's prodrome holds it.
  When the pull request finishes it, put `Completes: <id>` on a line of its description.

  <body, as typst source>

  <detail, as typst source>
  ```

- The tap, the fire, the poll and the report are today's
  (`app/core/src/coding.rs:462-480`). The train records the todo done at land.
- **At most two in progress** (the owner, §9). A todo counts as in progress
  while a card serving it is waiting, or its session is `Fired` or
  `Reported`. The box refuses a third card and names the two. This is a
  reading of the inbox, so nothing new is stored. It also refuses a second
  card for a todo already in progress, a todo in a register conflict, and a
  task over 4 KB.
- It adds:
  - one function in `secretary` (the template, the refusals and the count);
  - one route, `POST /api/<chain>/todo/<id>/build`, behind `Trust::admits`
    (`app/cli/tests/writes.rs` pins the route list);
  - the button.

  About +63 lines (≈).
- It leaves alone `propose_coding_session` (for sessions the owner asks for
  in chat), the routine's brief and the session's stored shape.

**`[coding] chain` keeps its key and changes its meaning.** It stops being
"where the hand-over writes" (`coding.rs:238-239`). It becomes "the prodrome
whose todos a session builds". Startup already checks that the chain is served
(`app/cli/src/instance.rs:757-766`).

**"Linear by default" is this act plus a reading. It is not a state
machine.**
- **The queue** is that prodrome's list, ascending in fulfillment, as `todos
  open` prints it (`app/cli/src/commands/todos.rs:54-67`). A priority change
  moves a todo in it.
- **Building and in review** are the session's own `Fired` and `Reported`
  (`coding.rs:320-357`). The session is stepped only forward (`:412-441`),
  and its card now names the todo.
- **Done** is `Completed`, written by the train
  (`app/cli/src/commands/release.rs:95-108`).
- Every state is read from a store that only climbs, so states move one way
  with no status register to keep in step.
- The other chains offer no "Build it" and keep today's free-form
  fulfilment.

**The top of the queue as a card** (the owner, §9: both paths, the tapped
todo first). Whenever the prodrome has fewer than two todos in progress and
no build card waits, the box offers its first ready todo, meaning open with
nothing in `waiting_on`. The offer is a Noticed card
(`app/core/src/secretary/noticed.rs:36-46`), filed with no model and withdrawn
when nothing owes it (`:284`, `:360`). When the top changes, the waiting card
is withdrawn and the new top is offered. It is the same act, and the owner's
tap still starts it. About +30 lines (≈). There is nothing for it to delete,
so §7 lists it as a stage that adds.

## 4. Upkeep is a private prodrome, and nothing in core names the project

The owner (§9): the project's todos are published, and upkeep moves to a
private prodrome where triage files. The private prodrome is the chain that
exists: `suzatary`, in the data repository, which keeps its whole history
private.
- **One key names it:** `[upkeep] chain = "suzatary"`. It is read where
  `[coding]` is read (`app/cli/src/instance.rs:571-573`), and the instance
  refuses a chain it does not serve. About +6 lines (≈).
- **`Deployment::project()` goes** (P7, −4). Its one caller
  (`app/cli/src/commands/secretary.rs:1757-1763`) passes `[upkeep]`'s chain,
  and files nothing when there is none. The fix cards still stand
  (SOURCES-PLAN §8.1).
- **`mail_health.rs:54` writes `kind: "todo"`.** `"suzateryTodo"` stops being
  a writable kind (`decoration/mod.rs:25`), and the agent's special line
  about it goes (P8, −4). Stored records of that kind still read and draw:
  `Block::of` takes any name (`app/core/src/wire.rs:3005-3020`), and an
  unknown kind files under "other" (`decoration/mod.rs:40-45`).
  OPERATOR.md:129-132 uses the kind on `life` today, so the manual's line
  changes with it.
- **Upkeep todos are not built by a cloud session.** Their text names the
  owner's accounts (`mail_health.rs:48-50`), and a task must carry no
  personal details (SECRETARY.md:589). The main session reads that queue as
  today (`todos --chain suzatary open`, ROADMAP.md:9-13).
- **The web's project special case goes** (P9, −15, about +3 for the label).
  The chain picker, which already lists every served chain
  (`web/src/shell/nav.ts`, `Pick`), reaches each list. "Project" leaves the
  menu.

Net: −23 + 9 = **−14** (≈).

There is no migration. This deployment's second chain is `suzatary`, which is
what `project()` returns today and what `[upkeep]` will name.

## 5. The project's memory: a second store of the decisions schema, in the code repository

The owner asked for an implementation memory, "similar to optmem", that the
coding agents can share.

**What it holds.** Implementation knowledge only:
- the owner's design decisions, with his words and the date;
- review lessons;
- gate and train lessons.

Nothing personal, because it is published with the code.

Today this memory exists as flat files:
- CLAUDE.md: 1,536 lines and 108,601 bytes, loaded whole into every session.
  179 dated decisions are woven through its rules, and its Log is 88 lines
  of lessons (`CLAUDE.md:1449-1536`).
- AGENTS.md: 359 lines.
- 59 plans under `docs/`.
- The main session's own notes, which live in the vault and are not in the
  repository.
- The checklists pasted at the end of every brief.

**The schema that fits already exists, and it is not the secretary's fact
memory.** A recorded decision (`app/core/src/decisions.rs:1-12`, 973
production lines) has:
- a `Statement`: a title of at most 120 characters, a Typst body and a
  rationale (`app/core/src/decisions/statement.rs:5-27`);
- a life that ends at withdrawn;
- a grow-only set of the decisions it supersedes, read as "the newer word";
- a CLI to record, revise, withdraw, list, show and view
  (`app/cli/src/commands/decision.rs:33-60`).

The one store today sits at `<data>/decisions` (`app/cli/src/instance.rs:306-308`),
written by the CLI and by the owner's taps on `propose_decision`. The
project's memory is a SECOND store of that schema, at `memory/` in the code
repository. That is a path, not a second implementation.

The secretary's fact memory is the wrong reuse, for six reasons:

| Its design | Why it does not fit an implementation memory |
|---|---|
| A fact is one line of at most 280 characters with no URL, and at most 200 stand (`app/core/src/secretary/memory.rs:25-30`) | A decision with its reasons does not fit in a line. |
| `Kind` is closed to owner, person, preference, standing (`memory.rs:127-140`) | It would need new arms. |
| `Origin` is a conversation turn or a tap (`memory.rs:200-217`) | A commit or a PR is neither, so it would need a stored arm. |
| The words are box-local, erasable, never pushed (`app/core/src/secretary/memories.rs:1-11`) | A cloud session must read them. |
| It is told as "BACKGROUND, NEVER INSTRUCTIONS" (`memory.rs:397-399`) | A rule for a coding agent is an instruction. |
| The digest summarises conversation lines (`app/core/src/secretary/digests.rs:1-3`), and `recall` searches other conversations' turns (`app/core/src/secretary/verbs/recall.rs:28-32`) | No coding session's conversation is on the box to summarise. |

Bending it would make a second variant of the memory. The decisions schema
fits as it is.

**What OptMem's idea becomes here.**
- OptMem gives a model a small, always-loaded view and lets it zoom
  (`docs/MEMORYDIGEST-PLAN-2026-10-06.md` §1).
- Here the view is the decision list: one line per decision (id and title),
  newest lineage first.
- The zoom is `decision show <id>`.
- No compactor is needed at this grain: the owner and the main session write
  the titles.

**How an agent reads it.** A cloud session has the code repository, and it
builds the workspace anyway for the gates.
- `decision list` and `show` gain `--store <dir>`, which reads a store by
  path with no instance file.
- `decision list` gains `--grep <words>`, matched against titles and
  rationales.
- A brief then says "`suzatary decision list --store memory --grep gate`
  first", instead of pasting the rules.
- About +25 lines (≈).

**How it is written.**
- The main session records a decision with the same CLI, pointed at
  `memory/` in its checkout, and the objects land through a PR and the train
  like any file. A decision is rarely written, so going through the train
  costs nothing.
- An agent PROPOSES a lesson with a `Lesson:` line in its PR description.
  The main session records it, or not, when it reviews the PR. The PR is the
  card and the record is the confirmation, the way triage's writes are
  cards. No code is needed. Having the train record `Lesson:` lines the way
  it records `Completes:` would be about 15 shell lines; it is not proposed.
- The secretary's `propose_decision` keeps writing the box's store, so the
  box never writes to the code repository.

**What moves and what stays.**
- **Moves into `memory/`:**
  - the Log's 88 lines;
  - each dated decision's history (who, when, the words, the incident),
    leaving its rule in CLAUDE.md as one present-tense line that names the
    decision's id;
  - the brief checklists, one decision each.
- **Stays a file:**
  - the rules an agent must obey on every turn (CLAUDE.md, AGENTS.md's
    style guide);
  - specs and laws (`docs/`), as `decisions.rs:2-3` already says.
- **CLAUDE.md shrinks** by at least the Log's 88 lines. The rest is
  measured in that stage's PR rather than promised here.
- **Duplicates go.** Rules stated in both CLAUDE.md and AGENTS.md (for
  example `Completes:`, in CLAUDE.md's "Versioning" and AGENTS.md:342-359)
  keep one copy.
- `scripts/merge-gate.sh` lists `memory/` among the paths no step reads, so
  recording a decision re-runs no tree step (one shell line).

**What this costs in code.** About +25 lines and one shell line. It deletes
no code. It removes at least 88 lines of always-loaded instructions and every
pasted checklist. §7 lists it as a stage that adds code.

## 6. The project's todos move: a fresh genesis in a repository of its own

The owner (§9) chose (b): a FRESH genesis that carries the open todos is
published, and the old history stays private as the upkeep prodrome (§4). He
chose its home twice:
- first, before the orphan branch was on the table;
- again, on 2026-10-25, after seeing it.

The home is A REPOSITORY OF ITS OWN, called `<project repository>` below; its
name is his (§10).

**The rejected alternative: an orphan data branch.** This would follow the
Prodrome's `roadmap-data`. The branch would sit either on the public mirror
(`suzatary-core`) or on `bmabsout/Suzatary`. Its advantages:
- writes never touch `master` or the train;
- it is published with the repository it lives on;
- the box already holds a credential for the mirror.

The owner preferred a repository of its own. It keeps the project's work
apart from the code's history and its access, and it can be granted, watched
and published on its own. The cost below is the same either way, except for
the one new credential this home needs.

It is created PUBLIC in the owner's new organization, `fulfilium`, beside
the rest of the public family (§6.2), as `fulfilium/<project repository>`.

**What is granted, and to whom.** Each grant is listed, so nothing is left to
the brief alone.
- **The box: one fine-grained token, `SUZATARY_PROJECT_PUSH_TOKEN`.**
  - Its resource owner is `fulfilium` (a token belongs to one resource
    owner), and it covers `<project repository>` alone with Contents read
    and write.
  - It is declared in `secretspec.toml` beside `SUZATARY_DATA_PUSH_TOKEN`
    (`secretspec.toml:43`).
  - It is handed to git by the credential helper that already serves the
    data repository (`scripts/git-credential-secretspec.sh`). The helper
    then chooses its token by the repository git asks about
    (`credential.useHttpPath`), about +6 shell lines, rather than a second
    helper.
  - It reaches no other repository.
- **The coding agents: read access, by mechanism.**
  - The repository is public, so reading needs no credential.
  - The routine's sources gain `fulfilium/<project repository>` beside
    `bmabsout/Suzatary`, so a session clones it at start.
  - The cloud environment's GitHub access gains it: the owner installs the
    Claude GitHub App on that repository of `fulfilium`.
  - A repository ruleset on every branch (`~ALL`) refuses creation, update
    and deletion to every actor but the repository-admin role, which is its
    only bypass. The box's token acts as the owner, an admin. The App can
    clone and fetch but never push, whatever a session is told.
  - The routine's brief must change. Its rule 2 says "Never touch any GitHub
    organisation. Work in `bmabsout/Suzatary` only."
    (`docs/CODINGSESSION-ROUTINE.md:34`), which forbids even reading a
    repository of `fulfilium`. It becomes: "Work in `bmabsout/Suzatary`. You
    may read `fulfilium/<project repository>` and the memory; never push to,
    open anything in, or change any organisation's repository." The text is
    changed by a commit and an update through the routines API, as that
    file says.
- **A session never writes a todo there.** Its PR on `bmabsout/Suzatary`
  names the todo with `Completes: <id>`, as now. The train's land records the
  done on the box, which pushes it to `<project repository>` like any other
  write.
- **That repository's CI needs no secret.** It builds `fulfilium/suzatary-core`,
  which is public, at the revision the user repository's flake pins.

- **The carry is one migration verb**, `suzatary prodrome carry --from
  suzatary --to <dir> <todo>...`.
  - It begins a new store (`Genesis`, as `suzatary init` does).
  - It authors each named open todo there with its current text and spec,
    as the owner.
  - It also carries the release todos the plan declares, the completed ones
    as completed. Otherwise `Reading::of` would read `base.0` on the new
    store, because MAJOR.MINOR is the last COMPLETED release's
    (CLAUDE.md, "Versioning").
  - Dry run first, then `--apply` with the owner's yes.
  - Ids stay the same, so `Completes:` lines keep their meaning once
    `releases.toml`'s `chain` names the new prodrome.
  - About +60 lines (≈). The verb is deleted in the next stage.
- **What is carried passes the leak guard first.** `suzatary leaks` runs over
  every new object before the repository is pushed. Upkeep todos are not
  carried.
- **The instance gains one `[[chains]]` entry for it.**
  - Its name is the owner's to choose (§10); this plan calls it `project`.
  - It sits at an absolute path in the box's clone of `<project repository>`,
    with the owner as its owner.
  - `triage` is not on its roster.
  - `[coding] chain` and `releases.toml` name it.
- **Triage cannot file there.** The agent's write verbs refuse a chain whose
  roster does not name the agent (`agent::WRITES_AS`). About +4 lines (≈),
  if the check does not exist already; stage 6 verifies.
- **The box pushes it as it pushes the data repository.** `safe-push.sh`
  commits and pushes the box's clone of `<project repository>` with that
  token, under the same rules it applies to the data repository:
  - an allowlist of the store alone (`objects/<64 hex>.py`, `.lock`,
    `.gitattributes`);
  - the append-only walk;
  - no code path.

  Before the push, it runs `suzatary leaks` over what it adds, because the
  repository will be public. On a lost race it merges and retries, as
  `roadmap-push.sh` does. About 20 shell lines. The vault carries the clone.
- **The code and its data test each other, in both directions**, as the
  Prodrome's CI reads its roadmap (`ci.yml:92-108`).
  - **The data's CI:** on every push to `<project repository>`, its CI
    builds `fulfilium/suzatary-core` at the pinned revision, then runs
    `suzatary verify --store .` and `suzatary todos open --store .`.
  - **The code's CI:** on every push to `fulfilium/suzatary-core`'s `master`
    (every publication), a job beside `publication.yml`'s checks out
    `fulfilium/<project repository>` at its head and runs the same two
    commands. So no publication ships a binary that cannot read the
    project's own data.
  - That job is NOT a step of the merge gate. A tree step's pass is a
    function of the tree alone, kept by its inputs (CLAUDE.md, "The merge
    gate"), and another repository's head is not one of them. The merge
    gate's live half keeps reading the store on the box, as it reads every
    chain the instance names.
  - Both runs need `verify` and `todos open` to read a store by path with no
    instance file: the same `--store` reading §5 gives `decision`. About
    +8 lines (≈).
  - The YAML is about 30 lines in the data repository's workflow and about
    15 in the code's.
- **Old references keep reading.** `todo:suzatary/<id>` still names the
  private chain, which is unchanged. A `RefIn` in a `life` spec or an inbox
  price names the chain by name, so nothing stored is renamed.

This is the one part of the request that cannot be net negative. Its
permanent cost is about +4 Rust lines and about 20 shell lines, plus a
migration verb that is added and then deleted. Publishing a fresh genesis
needs the box to push somewhere new and that place's CI to read it, and
nothing existing can be deleted in return.

Measured for this home, stage 6 adds:

| Piece | Lines (≈) |
|---|---|
| the carry verb (deleted in stage 7) | +60 Rust |
| triage refused on a chain whose roster lacks it | +4 Rust |
| `--store` for `verify` and `todos open` | +8 Rust |
| `safe-push.sh` for the second repository | +20 shell |
| the credential helper choosing by repository | +6 shell |
| the two CI checks | +45 YAML |

The permanent Rust cost is about +12, plus the shell and the YAML.

§6.2 is stage 5, the move into `fulfilium`, and it must land first: this
repository is created there, and the code's CI reads it from there.

### 6.1 In 2.3: the public prodrome, modelled on the Prodrome's roadmap

Once `<project repository>` is public, the project's prodrome takes the rest
of `roadmap-data`'s shape. These are later stages, listed here so that nothing
is re-designed:
- **Issues become items, reusing `prodrome-github`.** That crate's pure parts
  are taken as a library at the revision `app/Cargo.toml:64` already pins:
  `delivery_of`, the `/price` grammar `parse_price`, `actor_of_login` and the
  `author_association` standing.
  - Only the function that builds an item's record is per-schema. The
    crate's builds `TodoEvent<Todo>`, the reference payload (`github/src/lib.rs:386`).
  - Suzatary's builds `TodoEvent<Record>` through the existing
    `authoring::build_todo`.
  - Better still, `prodrome-github` becomes generic over the payload
    upstream, so Suzatary writes no mapping at all.
  - The workflows are `mirror.yml` and `price.yml`, taken as they are, on
    `<project repository>`. Its issues are the project's issues, so each
    workflow writes only its own repository, and the run's own
    `GITHUB_TOKEN` is enough. The push
    is that repository's own; `roadmap-push.sh`'s union and retry are
    already in stage 6's push.
- **The pricing bot is a member whose writes are claims.** It sits on the
  chain's roster and in its `untrusted` (§2: members already exist). Its
  price is a claim beside the confirmed one until the owner accepts it.
  That is what `untrusted` means today; no new mechanism is needed.
- **CI** already runs `verify` and `todos open` there from stage 6.

Cost, all ≈:
- the record mapping, about 40 lines, or none if the crate is generalised
  upstream;
- the workflows, copied as YAML.

None of this is needed for the owner's request, which ends at stage 7.

### 6.2 The public family moves into `fulfilium` (stage 5)

The owner, 2026-10-25: a GitHub organization of his own, `fulfilium`, groups
the PUBLIC family. The private repositories stay under his account.

| Repository | Where | How |
|---|---|---|
| `suzatary-core` (the publication) | `fulfilium/suzatary-core` | transferred |
| `prodrome` | `fulfilium/prodrome` | transferred |
| the project prodrome | `fulfilium/<project repository>` | created, public (§6) |
| the Prodrome's prodrome | `fulfilium/<prodrome's repository>` | created, public (below) |
| the web framework | `fulfilium/<its repository>` | created, public. It is the owner's choice; nothing in this repository names it yet |
| `Suzatary` (this development repository) | `bmabsout/Suzatary` | stays |
| `suzatary-data` | `bmabsout/suzatary-data` | stays |
| `suzatary-core-archive` | `bmabsout/suzatary-core-archive` | stays |
| `typst-design` | `bmabsout/typst-design` | stays; the owner did not name it (`flake.lock:146-159`) |

**The Prodrome's prodrome is the same shape** (the owner: "Prodrome's
prodrome should be similar").
- Its roadmap leaves the orphan `roadmap-data` branch for a repository of its
  own in `fulfilium`.
- The owner likes that the store "acts as a good test case", so the
  Prodrome's CI keeps reading it on every push. At the pinned revision
  `de71ccc`, `.github/workflows/ci.yml:97-108` checks out
  `ref: roadmap-data`; that becomes
  `repository: fulfilium/<prodrome's repository>`. `pages.yml:20,59-64` reads
  the same branch and changes the same way.
- Suzatary does the same in both directions (§6): its publication's CI reads
  `<project repository>`, and that repository's CI reads the code.
- **One Prodrome decision.** `mirror.yml` and `price.yml` push items to the
  roadmap store (`mirror.yml:22,53,100`, `price.yml:132,148,165`,
  `.github/scripts/roadmap-push.sh:15`).
  - If the issues stay on `fulfilium/prodrome`, those workflows write ANOTHER
    repository. A run's `GITHUB_TOKEN` cannot do that, so each needs a
    fine-grained token: resource owner `fulfilium`, the roadmap repository
    alone, Contents write, held in an environment only those two jobs enter.
  - If the issues move to the roadmap repository, its own `GITHUB_TOKEN` is
    enough, as §6.1 has for Suzatary.
  - That choice belongs to the Prodrome's own plan. These lines are there,
    not in this repository.

**GitHub keeps redirects after a transfer, but every reference is pinned and
updated anyway.** A redirect is not a contract: it ends the day the old name
is reused. GitHub Pages sites are NOT redirected at all.

| What | Where (file:line) | New value | Who |
|---|---|---|---|
| Create the organization; allow fine-grained tokens in it; allow the actions core runs (`cachix/install-nix-action`, `nix-community/cache-nix-action`, as `scripts/core-settings.sh:9-11` sets per repository, since an org policy can override a repository's) | the web UI | `fulfilium` | owner |
| Transfer `suzatary-core` and `prodrome` | the web UI | `fulfilium/…` | owner |
| The Prodrome flake input | `flake.nix:22` (and its comment `:10`) | `github:fulfilium/prodrome/de71cccc7565c770ce282eba07b1718d612b87bd` | main session |
| Its lock | `flake.lock:102,108` (`"owner"`), relocked by `nix flake lock` with the same `rev` and `narHash` | `"owner": "fulfilium"` | main session |
| Cargo's git dependencies | `app/Cargo.toml:64,68,248` (and the comment `:6`) | `git = "https://github.com/fulfilium/prodrome"`, same `rev` | main session |
| Their lock | `app/Cargo.lock:3064,3077,3089` (`source = "git+…"`). Cargo treats the URL as part of the source, so `cargo update -p prodrome-core -p prodrome-wasm-exports -p prodrome-typst-wasm` rewrites the three lines with the same commit | `git+https://github.com/fulfilium/prodrome?rev=de71ccc…` | main session |
| The guard that pins the Prodrome | `app/cli/tests/guards/merge_gate.rs:1484`. `prodrome_pins` filters lines naming `bmabsout/prodrome`, and `merge_gate_reads_one_prodrome_through_cargo_and_the_flake` (`:1497-1514`) fails loudly if it finds none, so it changes in the same commit | `"fulfilium/prodrome"` | main session |
| The remote `core` of the dev checkout, which `train.sh` (`:885`, `:1362-1370`, publish) and `core-settings.sh` (`:37`) read; no file names it | `.git/config` | `git remote set-url core https://github.com/fulfilium/suzatary-core` | main session |
| The user repository's `suzatary` input, which `deploy.sh` and `nix/app-input.nix:1-3` read | `suzatary-data`'s `flake.nix` and `flake.lock` | `github:fulfilium/suzatary-core`, same commit, deployed with `scripts/deploy.sh` | main session |
| Core's settings and its Cachix push secret (`CACHIX_AUTH_TOKEN` in the environment `cachix`, `.github/workflows/publication.yml:21,38`). Checked after the transfer, then set again: the script is idempotent and refuses without a terminal (`scripts/core-settings.sh:30`) | `scripts/core-settings.sh fulfilium/suzatary-core` | the same values | owner |
| The Cachix cache itself | `publication.yml:51-52,99`, `gate.yml:81-82`, `gate-store.yml:58-59`, guard `app/cli/tests/guards/publication.rs:90` | unchanged: `bmabsout` is a Cachix account, not a GitHub owner | nobody |
| The train's push token for core (a fine-grained token, `docs/OPERATOR.md:66-69`). A token has one resource owner, so the old one cannot reach `fulfilium` | secretspec | re-minted with resource owner `fulfilium`, `suzatary-core` alone, Contents and Workflows read and write | owner |
| `SUZATARY_PROJECT_PUSH_TOKEN` (§6) | `secretspec.toml`, beside `:43` | resource owner `fulfilium`, `<project repository>` alone | owner mints, main session declares |
| `SUZATARY_REPOS_TOKEN` (`secretspec.toml:45`) covers `[[secretary.repos]]` and `[coding] repo`. If the instance's `[[secretary.repos]]` names a `fulfilium` repository, it needs a second token, since one token cannot span two owners | the data repository's `instance.toml` | `bmabsout/Suzatary` stays on the old token | owner checks; main session edits |
| The coding routine's repositories | the routine (routines API) and `docs/CODINGSESSION-ROUTINE.md:4,29,34` | sources `bmabsout/Suzatary` and `fulfilium/<project repository>`; rule 2 as §6 words it | main session |
| The cloud environment's GitHub access | the Claude GitHub App's installation | installed on `fulfilium/<project repository>` | owner |
| The APK's update source for Obtainium. `release-apk.yml:14` says it watches "this repository's releases", and `scripts/core-settings.sh:12-14` turns every other workflow off in core, so the APK is released from `bmabsout/Suzatary`, which stays | the phone's Obtainium entry | unchanged, unless the phone watches `suzatary-core` (`docs/APKPROFILE-PLAN-2026-10-23.md:168` offers that), in which case `https://github.com/fulfilium/suzatary-core` | owner, on the phone |
| Pages | this repository names none. The Prodrome's: `mirror.yml:120`, `.github/pricing-prompt.md:27`, its README | `https://fulfilium.github.io/prodrome/…`. Pages are not redirected, so every item link written into an issue comment before the move breaks | main session, in the Prodrome |
| Links in prose | `README.md:17,167`; `CLAUDE.md:257,898`; `docs/ARCHITECTURE.md:7,21,422`; `docs/README.md:28,30`; `.github/workflows/tests.yml:91` | `github.com/fulfilium/prodrome` | main session |
| Names that stay | `app/cli/src/instance.rs:1358`, `app/cli/tests/instance.rs:101,138,551,556`, `app/core/src/settings.rs:787` (`bmabsout/Suzatary`); `app/core/src/web.rs:661-697` (an allow rule for `github.com/bmabsout`, a test of the rule, not a pin) | unchanged | nobody |

The crate is called `suzatary-core` and so is the repository. Every other
`suzatary-core` in the tree (`app/*/Cargo.toml`, `flake.nix:317-330`,
`app/Cargo.lock:3977-4109`) names the crate and does not move.

**Order.**
1. Create the organization and its settings.
2. Transfer the two repositories.
3. Mint the tokens.
4. Land one PR with every file row above. The guard and both locks move
   together, so the gate stays green.
5. Set the remote `core`.
6. Deploy the user repository's input.
7. Run `core-settings.sh`.

Stage 6 (§6) then creates `<project repository>` in the organization.

**Cost.** No production lines. File edits, two lock regenerations and one
guard string; the rest is settings. This stage neither adds nor deletes code.

## 7. Stages

Each stage is one PR, with its migration, if any, inside it.

| Stage | What | Deletes | Adds (≈) | Net (≈) | The phone after it |
|---|---|---|---|---|---|
| 1 | §3: "Build it" from a todo, at most two in progress; the hand-over goes | P1–P6: 163 | 63 | **−100** | A todo on the project's list has "Build it". Its card shows the whole task and is priced by the todo. A third is refused, naming the two. The session card has no "Hand to the main session". |
| 2 | §4: `[upkeep]`; `project()`, the special kind and the web's project case go | P7–P9: 23 | 9 | **−14** | "Project" leaves the menu; the chain picker reaches each list. The failing-mail todo lands on the same chain as a plain todo. |
| 3 | §3: the top of the queue as a card | 0 | 30 | **+30** | One "Build next: <todo>" card in the Inbox while fewer than two are in progress. |
| 4 | §5: the project's memory, `memory/` in the code repository; `decision --store`, `--grep` | 0 code; CLAUDE.md ≥ 88, the brief checklists | 25, 1 shell | **+25** | Nothing visible. Briefs shrink, and a session reads `decision list --store memory`. |
| 5 | §6.2: the public family moves into `fulfilium`; every reference pinned | 0 | 0 (two locks, one guard string, settings) | **0** | Nothing visible: the same app, built from the same commits under new names. |
| 6 | §6: the fresh genesis, carried, in `fulfilium/<project repository>`; its grants, push and both CI checks | 0 | 72, 26 shell, 45 YAML | **+72** | The project's list is the new prodrome. Its done releases read as before, and the old todos are on the upkeep list. |
| 7 | the carry verb goes, once it has run | 60 | 0 | **−60** | Nothing visible. |
| **Plan** | | **246** | **199** | **≈ −47** | |

Stages 3, 4 and 6 add code, and none of the three has anything to delete in
return:
- **Stage 3, the queue card** (+30): the owner chose "both" (§9). The tapped
  path alone costs nothing beyond stage 1.
- **Stage 4, the memory** (+25 code): it trades code for instructions. It
  removes ≥ 88 always-loaded lines from CLAUDE.md and the checklists pasted
  into every brief.
- **Stage 6, the move** (+12 permanent Rust, 26 shell, 45 YAML): §6 says why.

Without stage 3 the plan nets ≈ −77. Stages 1, 2 and 7 each delete more than
they add, stage 5 moves no code, and the plan as a whole is negative.

Tests move with their code:
- the hand-over's tests (`app/cli/tests/coding.rs:336` and the route's) go
  in stage 1;
- stages 1 and 5 add tests where AGENTS.md §8 asks for them: a security
  boundary (only the tap fires; triage refused on a chain whose roster lacks
  it), the in-progress count, and the carry migration.

The manuals change in words in the stage whose mechanism they describe:
- OPERATOR.md, SECRETARY.md and `scripts/hourly-triage.sh:227` move to
  "upkeep" in stage 2;
- CLAUDE.md's coding-session paragraph loses the hand-over in stage 1.

## 8. Reviews

**Rich Hickey: one idea or a complection?** One idea: a project prodrome is
a prodrome.
- **Who may act** stays where it was: a chain's roster, `untrusted` and
  `owner`, and the tap.
- **How work flows** is a reading of facts that already exist: the price,
  the session, the lifecycle.
- **Where it lives** is a path.
- **The memory** is a second store of an existing schema.

The one new link is a build card's `serves`, a field that already exists.
`[coding] chain` names where a builder reads; `[upkeep] chain` names where
the box files its own repairs. Neither grants anything.

**A security reviewer: what can a member or an agent reach that they cannot
today?**
- **The triage agent's fence is unchanged:**
  - the same verbs (`app/core/tests/mcp-surface.json`), `tool-guard.sh` and
    `WRITES_AS`;
  - its writes are still cards;
  - building is not in its surface.
  - It loses reach: it cannot file onto the published prodrome (§6).
- **The one widening is the build card.** A todo's stored text becomes a
  task. Today `propose_coding_session` refuses a round that read a todo
  (`app/core/src/secretary/verbs/propose_coding_session.rs:13-18`). Text the
  owner tapped onto the chain could have started as triage's, from mail, and
  the owner accepted building it unedited (§9). The bounds:
  - the card shows the whole task, and the tap carries an index, never a
    body;
  - the routine holds no secret and no connector, and pushes only a
    `claude/` branch and a PR (`docs/CODINGSESSION-ROUTINE.md`);
  - the main session reviews the PR, and the train gates it.

  The worst case is a wasted session and a bad PR.
- **The build route** files a card and fires nothing. Only the existing tap
  fires, behind `Tap::of` (`app/core/src/secretary/tap.rs:32-37`).
- **Coding agents as readers** reach two things:
  - the code repository, which they hold today; the memory adds no secret
    and no route to the box;
  - `fulfilium/<project repository>`, which is public; their access is
    read-only by a ruleset whose only bypass is the repository-admin role
    (§6), not by the brief alone.
- **The box's new credential** is a fine-grained token whose resource owner
  is `fulfilium`, for `<project repository>` alone. Each push carries only
  new object files from the allowlist, after the leak guard. A leaked token
  could append to that repository and nothing else, and `verify` in both CI
  checks refuses an object that does not rehash.
- **The transfers (§6.2)** move no credential's reach. Every token is
  re-minted per resource owner, so no one token spans the owner's private
  repositories and the organization's public ones.
- **The CI checks** hold no secret: both repositories they read are public.
- **The published prodrome** holds only carried todos that passed the leak
  guard, and later the owner's and the main session's writes. Upkeep, which
  names accounts, stays in the private chain. The memory is published with
  the code, so a decision is held to the same guard as CLAUDE.md is today.

**The maker of a simple issue tracker: is "linear by default" the smallest
workflow that lets an agent pick the most important work first?** Yes:
- one list ordered by priority;
- a limit of two in progress;
- one button, and a card for the top;
- done set by the merge.

A tracker adds states because its cards do not know what is happening to
them. Here the session reports its own state, so "in progress" and "in
review" come free.

**Branch or repository of its own.** This revision first recommended an
orphan branch of the public mirror. Its case: one repository fewer, the
credential already on the box, publication together with the mirror, and the
Prodrome's precedent. The owner chose a repository of its own after seeing
that case (§9). The difference in code is nil, and the difference in reach
is one token for one repository. What the owner gets for it is the
project's work kept apart from the code's history and access, granted,
watched and published on its own terms.

## 9. The owner's answers (2026-10-25)

He answered the questions of this plan's first version (5d4d34b), numbered
as they were there.

1. **Publication in 2.3:** (b). A fresh genesis that carries the open todos
   is published, and the old history stays private (§6).
2. **May triage file onto the published prodrome?** No. Upkeep moves to a
   private prodrome, and triage files there (§4, §6).
3. **Tapped todo or top of the queue?** Both, through one act: the tapped
   todo first (stage 1), the queue card after (stage 3).
4. **Build a todo whose text came from a triage card, unedited?** Yes,
   because the card shows the whole task (§8).
5. **The in-progress limit:** 2 (§3).
6. **The home:** a repository of its own, moved in its own stage (stage 6).
   Asked again on 2026-10-25, after seeing the orphan-branch option (§2, §6),
   he confirmed a repository of its own. The branch is kept in §6 as the
   rejected alternative.

## 10. What is still open

1. **The repository's and the prodrome's names.** The repository is
   `<project repository>` here, and the prodrome is `project`.
   `suzatary` stays the private upkeep chain, so that stored `RefIn`s and
   `todo:` references keep their meaning.
2. **Which of the old chain's open todos are carried.** Stage 6's dry run
   lists every open todo with the leak guard's verdict, and the owner picks.
