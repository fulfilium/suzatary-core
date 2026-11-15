# The project's tasks are a public roadmap in a repository of their own — 2026-11-15

The request: the project's own task list becomes a public Prodrome store, so that anybody
who has the code can read what the project is working on, and so that a coding session can
read a task's history and propose a change to it.

The owner decided where the store lives, the same day: "We can make a separate git repo for
it and have it be a submodule I guess or the reverse that repo's prodrome can have the app as
a submodule (this sounds more optimal to me but you can figure it out). Of course those are
public tasks, while tasks that are under specifically [one deployment] go there, this takes us
closer towards the thing other people can reuse."

The owner answered the note's questions later that day (§11). The frame of those answers:
"at this point we're the only users of the suzatary-core … When it's public facing it's as if
you're creating a public issue". Every point below is decided.

This note agrees with `docs/SHAREDPRODROME-PLAN-2026-10-25.md` (cited below as SHARED) and
`docs/MULTIPRODROME-PLAN-2026-09-27.md` (MULTI), except where §10 says otherwise. It uses
SHARED's terms: a prodrome is a store, its members, its home and its workflow, and the
project's prodrome is no special case in the core. Line numbers are at `origin/master` d0a490e
(release 2.1.135). The note quotes no measurement of a private store beyond the owner's
counts: 1,333 objects, 43 open todos, and 63 objects that name a private term.

## 0. What "the project's task list" means

- **The project's task list is the open list of one public store**: ⟦project tasks⟧ =
  `open(fold(P))`. It is not a union with any private chain.
- **A deployment's upkeep is another denotation**: ⟦upkeep D⟧ = `open(fold(U_D))`, the work of
  running one box. A task about one deployment (its failures, its data, its owner) lives
  there, and the public list says nothing about it (§11, "Later").
- **A release is a todo of P** (§5), so anybody who can read P can compute the version.

Today both meanings share the deployment's second chain (`CLAUDE.md`, "THERE ARE TWO CHAINS";
the `twochains` decision), and the version is read from it (`app/core/releases.toml:9`). So
only a machine that holds the private data can say what the project is doing.

## 1. Where the store lives

### 1.1 A public roadmap repository per project, which pins its code

Each project has its own roadmap: one store in its own public repository. "Roadmap" is the
word the Prodrome already uses for its own store. The pattern for any project is
`<project>-roadmap`, and for this one the repository is `fulfilium/suzatary-core-roadmap`
(decided by the owner, 2026-11-15). Its tree:

```
objects/<sha256>.py      the store (SPEC §3)
.gitattributes           objects/** -text -diff, as memory/.gitattributes
.gitignore               /.heads (heads are derived; memory/.gitignore)
charter.toml             public = true, its one field (§3.1)
releases.toml            the release plan, moved from app/core/releases.toml (§5)
flake.nix, flake.lock    inputs.suzatary = the application, by revision (§1.2)
.github/workflows/ci.yml builds the pinned application; runs verify and todos open
README.md                a link to the viewer around an SVG of the open list (§8.1)
roadmap.svg, roadmap-dark.svg  that SVG, light and dark, drawn at each push
.github/workflows/pages.yml  the static viewer on GitHub Pages (§8.1)
```

It is not one store for the whole public family, and not a store inside a code repository.

| | A store beside `memory/` in the code repository | The code holds the roadmap as a submodule | **The roadmap pins the code (decided)** |
|---|---|---|---|
| Whose history carries the churn of task edits | the code's, through the train; every new object is a path admitted at publish | the code's: every edit moves the pointer | the roadmap's alone |
| Can another project reuse the code without these tasks? | no | no | yes: the code names no roadmap |
| A cycle? | no | yes | no: roadmap → code → Prodrome, as user repository → code (USERREPO §1) |
| One commit as a checkpoint of tasks and their reader | yes | with a second commit | yes (USERREPO §2) |

A flake input beats a submodule for the same reasons USERREPO §7 gives: the user repository
already pins the application that way, with no detached head, forgotten update or recursive
clone.

### 1.2 The pin

- **The train's land bumps it**, in one commit that also holds the `Completed` objects of the
  release's `Completes:` todos (§5). That commit says: "R shipped as v, completing these".
- **A stale pin means only that an older reader was used.** Shipped kinds are frozen and
  evolution adds kinds, and the roadmap's CI builds its pinned revision.
- **A failed push of that commit is repaired by the next train command**, as `record_origin`
  repairs a failed record today (`scripts/train.sh:170-190`).
- **A store migration is one roadmap commit**: bump the pin, migrate, commit (USERREPO §2,
  AGENTS.md §10).

### 1.3 Schema, and several stores side by side

- **The schema** is the deployments' own `TodoEvent<Record>` (`app/record/src/record.rs`), so
  the same folds, views and prices read it. The releases are todos of the same store, because
  a release composes its items as `Ref`s within one store
  (`app/core/src/release/reading.rs:160,244`).
- **Several projects are combined in the view, not in a store.** A deployment mounts each
  project's roadmap as one more chain and shows one priority order across them.
  - Today the app already reads every chain as one `ChainSet` and prices them on one scale
    (`app/signatures/src/chain_set.rs:1-15`). But it shows one chain at a time, through the
    chain picker (`web/src/shell/nav.ts:49,152`).
  - The combined list needs one page of the open rows of the chosen chains, sorted ascending,
    each row tagged with its store (MULTI §4, "One list across mounts"). That is stage 7.
- **A dependency across stores is a sentence for now.** `RefIn(<chain>, <todo>)` exists, but
  it names a deployment's LOCAL chain name, which means something different on another
  deployment. So a public store never stores one. A task that waits on another project's task
  says so in its text, until a reference named by genesis exists (MULTI §4).
- **Different Prodrome pins.** Each roadmap repository pins its own reader, and a deployment
  reads every mounted store with its own pin. Formats are add-only, so the only effect of a
  difference is that a kind newer than the reader's pin shows as unknown until the pin moves.
- **A store in another schema** needs that schema in the app. The Prodrome's own roadmap uses
  the reference todo schema, so mounting it is a later step, not part of these stages.

## 2. What becomes of the private chain

**A clean public genesis carries today's open project tasks, restated. The old chain stays
private as the deployment's upkeep chain and the archive** (SHARED §6, option (b)).
- **The carry is prepared by the main session.** Its dry run prints, for each of the 43 open
  todos, the restated text, door 1's verdict, and a recommendation: carry it, or leave it as
  upkeep. The owner approves the list.
- **The release todos are carried too**: the completed ones as completed, and the open one
  with its items. Otherwise MAJOR.MINOR would read `base.0` on the new store (SHARED §6).
- **The link runs one way, private to public.** Each carried todo gets a private `Cancelled`
  whose rationale names its successor; a cancelled upstream is surfaced, never hidden. A
  public todo names nothing private. A carried todo keeps its id when the id passes the guard,
  so `Completes:` lines keep their meaning.

The alternatives it beat:
- **A rewritten history**: replaying all 1,333 objects with the 63 edited. Every hash changes,
  so every reference breaks, and each edited object claims words its actor never wrote.
- **A redaction door per object.** The 63 sit in the middle of the DAG, and a published object
  whose dependency is missing does not `verify`. Stubs change names, which is the rewrite
  again. And the door judges once, then publishes forever.

## 3. What may be in a public task

### 3.1 One guard at every door out

The guard is today's matcher, `suzatary_core::leak` (`suzatary leaks`), driven by
`scripts/public-tree.sh --check` and `scripts/public-scan.sh`. It refuses:
- private terms, the box's names, tailnet names and addresses, home paths, emails outside the
  reserved domains, telephone numbers and secrets;
- an opaque file.

It prints only `file:line` (`scripts/public-tree.sh:1-35`). It runs at three doors:

1. **At the write, before the object is stored**, for a chain whose `charter.toml` says
   `public = true`. This covers the CLI, `POST /api/objects`, `POST /api/author` and a tapped
   card. A refused object from the phone stays local and named (`phonereplica`). The check
   must come before the store because a `Change` rests on its todo's heads (`changeheads`), so
   a stored object is an ancestor of every later write to its todo.
2. **At the push**: `public-scan.sh` over every commit the push adds, with their messages and
   identities. It refuses to run without the lists (`scripts/public-scan.sh:34-38`), so only
   the box can push.
3. **In the carry**, over every restated object.

`public = true` is the charter's one field. Members stay in the deployment's `[[chains]]`
entry (SHARED §2), and no signature object is published: the push right alone backs a public
actor (§11, "Later").

### 3.2 Measurements

The matcher pins host names, paths and the box's names. **Nothing mechanical pins a
measurement's numbers**: a number cannot be recognised as one by its spelling. The rule is
§3.4's: a public task cites measurements of generated stores only, and a deployment's figures
stay on its private chain. A public task is written deliberately, like a public issue (§4),
and that deliberate write is where the rule is kept.

### 3.3 The routing rule, as what a writer is refused

| Writer | On the public store | On the upkeep chain |
|---|---|---|
| the triage agent | refused: not on the roster (SHARED §6) | cards, as today |
| the in-app secretary | refused: a public store's roster does not name it; its writes go to the private chain | as today |
| the box's automated filers (mail health, a red nightly gate, failing sources) | refused: they take their chain from `[upkeep] chain` alone (SHARED §4) | — |
| any writer | a word, host or path the guard refuses (§3.1) | — |
| a pull request's `Completes:` | — | refused: it resolves only against the plan's store (§5). The main session closes a private task by hand |
| "Build it" | — | refused: `Deployment::builder` accepts only the plan's store |

A term the lists learn after publication: the nightly `public-tree.sh --check --lists` over
the box's clone reports it, and what to do is the owner's call each time.

### 3.4 A worked example: a task about a deployment's measurements

The main session measures a cold start on the box at `<n>` objects and `<t>` ms.
- **Today** it files a todo with those numbers on the second chain, and pastes them into a
  brief by hand.
- **Under this plan** it writes two todos:
  - **public**, id `coldstart`: body "A cold start reads every object again."; detail "Keep
    the reading across restarts. Measure on a generated store of a few thousand objects,
    before and after."
  - **private**, on the upkeep chain, id `coldstartbox`: the box's `<n>`, `<t>`, the date and
    the command, with the rationale "the facts behind `todo:<roadmap>/coldstart`".
- **The owner taps "Build it" on `coldstart`.** The brief carries only the id, and the session
  reads the task from its clone (§4.3). Its pull request says:

  ```
  Completes: coldstart
  Task-note: coldstart a generated store of 5,000 objects started in 900 ms before and 40 ms after
  ```

- **At review, the main session**:
  - records the `Task-note:` on the public task, because the session's store was generated;
  - measures the box again and writes the new `<t>` on the private twin, then completes it by
    hand. If the box is still slow, the twin stays open, and a new public task says why in
    words.

## 4. Who writes, and how

- **A public task is written deliberately, by the owner or by the main session**, after door 1:
  as a public issue is. The roster is the deployment's `[[chains]]` entry (SHARED §2): `owner`
  and `assistant`, both trusted. They are the only ones who change a price.
- **A coding session writes no object.** It proposes in its pull request's description:
  - `Completes: <id>` (AGENTS.md:408-430);
  - `Task: <one line>` for a new task;
  - `Task-note: <id> <one line>` for a note on an existing one.

  The main session records each, or not, on the box, as it records `Lesson:` lines
  (AGENTS.md, "The project's memory", item 4).
  - It beat a session writing objects on its branch, whose merge would be the act. That branch
    is on another repository, the lists are absent there, and a review would read printed
    objects instead of sentences.
- **A concurrent write is a register conflict, never a git conflict.** Writers only add files,
  so two clones merge with no git conflict, and `safe-push.sh` merges and retries on a lost
  race (SHARED §6, after the Prodrome's `roadmap-push.sh`). Two writes to one register are a
  conflict shown whole, and the next write settles it (CLAUDE.md, "A register holds its
  frontier"). A review reads the registers the new objects move, not the object files (§9).
- **Instants are as written, and the push is the usual one.**

### 4.3 What a session reads

A session works in the code repository, and attaching a second repository stops the owner
with a prompt. The roadmap is public, so a session reads it with an anonymous `git clone`, and
runs `suzatary todos open --store <clone>` and `suzatary todo show --store <clone> <id>`
(stage 1 adds `--store` to those verbs, as `decision --store` has,
`app/cli/src/commands/decision.rs:1-28`).
- Stage 1 checks that the environment's git proxy serves that clone. If it does not, SHARED
  §6's grant stands: the routine's sources include the repository, and the owner installs the
  GitHub App on it.
- The routine's rule 2, "Never touch any GitHub organisation. Work in `bmabsout/Suzatary`
  only." (`docs/CODINGSESSION-ROUTINE.md:34`), forbids even that read. Stage 6 rewords it as
  SHARED §6 does: read the roadmap and the memory; never push to, open anything in, or change
  any organisation's repository.

## 5. Releases move with the store

Every reader of the project chain today:

| Where | What |
|---|---|
| `app/core/releases.toml:9` | `chain = "suzatary"`, a deployment's local name |
| `app/core/src/release/plan.rs:74,138-144` | `Plan::declared()`, compiled in by `include_str!` |
| `app/cli/src/commands/release.rs:134-135` | `release read\|items\|known\|cut\|check\|record` open that chain |
| `app/core/src/deployment.rs:111-130` | `builder`: only todos of `plan.chain()` are built |
| `app/cli/src/commands/serve/the_box.rs:110-114` | `GET /api/releases` |
| `scripts/train.sh:170-190` | `sync` and `record_origin` |
| `scripts/train.sh:191-212` | `cut_release` (`release cut`) |
| `scripts/train.sh:228-282` | the `Completes:` check (`release known`) |
| `scripts/train.sh:1003-1026` | `report_nightly`: `todo add --chain suzatary --kind suzateryTodo` |
| `scripts/train.sh:1028-1081` | `checked_release`, `record_release`, `record` |
| `app/cli/src/commands/secretary.rs:2032-2037` | mail health on `deployment.project()` (`deployment.rs:132-137`) |
| `scripts/hourly-triage.sh:144` | `chain="suzatary"` |
| `scripts/safe-push.sh:65-67` | `events-suzatary/` on the allowlist |
| `app/core/src/agent/reads.rs:159` | "this system's own upkeep" |
| `web/src/app/chain.ts:11-15`, `web/src/shell/nav.ts:33,59,79`, `web/src/shell/layout.ts:25` | `PROJECT_CHAIN` and `#/project` |

`app/cli/tests/release.rs` and `app/cli/tests/guards/train.rs` use fixtures, and
`app/cli/tests/live.rs:47-50` reads the first chain, not the project chain.

What changes:
- **`releases.toml` moves beside the store**, which it names by being there. `Plan::declared()`
  goes, and `release` reads the plan at run time (`versionnotcompiled`).
- **A deployment finds the store as the mount with that store's genesis** (MULTI §3).
  `builder` and `GET /api/releases` ask the same question.
- **The cut is a function of the code tree and a roadmap commit.** The release commit names
  that commit in a `Tasks: <sha>` trailer. Both inputs are public, so a runner can cut and
  check a version.
- **`app/release.toml` stays**, as the code's copy of that reading, written by the cut.
- **At land**, `release check` reads the roadmap's head with the train's `Completes:` todos
  done. Then the land commits the `Completed` objects and the pin bump (§1.2).
  - `release record` and `sync`'s chain read go.
  - `record_origin` becomes the repair of that commit.
- **`report_nightly` files on `[upkeep]`'s chain**, because its body names a path on the box.

What stays on the box at a land, and why:
- the leak guard and the roadmap's push scan: only the box holds the lists;
- admitting a new path at a terminal: the owner's word;
- the pushes, with credentials only the box holds;
- the deploy.

## 6. How a deployment shows it

- **The mount.** The roadmap is a `[[chains]]` entry whose `events` is the box's clone, at an
  absolute path (`app/cli/src/instance/roots.rs:5-12`), with its roster and token as SHARED §6
  gives them. Door 1 runs because the clone's charter says `public = true`. Writes are today's
  writes, through the one door (`Trust::admits`), and the owner's priority edit in the app is
  public at the next push.
- **`safe-push.sh` pushes the clone** after `public-scan.sh` (§3.1). Its allowlist takes the
  store alone, as SHARED §6 has it.
- **"Build it"**: `Deployment::builder` accepts a todo whose chain has the plan store's
  genesis. SHARED §3 is otherwise unchanged.
- **The combined list** (§1.3) shows this roadmap beside `life` and any other mounted roadmap,
  in one order.

## 7. Reuse: the same shape for another project

1. Create a public repository.
2. Run `suzatary prodrome init --store . --public` (stage 1): the `Genesis`, `charter.toml`,
   `.gitattributes`, `.gitignore`, and the flake template pinning the application.
3. Optionally add a `releases.toml`. Nothing assumes one.
4. In the deployment's instance file, add a `[[chains]]` entry at the clone, set `[coding]
   chain` to it, and name its push token in `secretspec.toml`.
5. Copy the roadmap CI from the template. It needs no secret.

| Special to this project today | Becomes | Stage |
|---|---|---|
| `app/core/releases.toml`; `Plan::declared()` | `releases.toml` beside the store, read at run time | 4 |
| `chain = "suzatary"`; `builder` by name | the store found by genesis | 4 |
| `Deployment::project()`; `report_nightly`'s and `hourly-triage.sh`'s `suzatary` | `[upkeep] chain` | 0 |
| `PROJECT_CHAIN`, `#/project` | the chain picker, then the combined list | 0, 7 |
| `safe-push.sh:65-67`, a fixed list of stores | each store's home and push from the instance (SHARED §6) | 5 |

## 8. The stages

**Where SHARED stands at d0a490e:**
- **Landed:**
  - stage 1, "Build it": `app/cli/src/commands/serve/table.rs:20` and
    `app/core/src/secretary/build.rs:93,145`; no hand-over route remains;
  - stage 4, the memory: `memory/` and `decision --store|--grep`;
  - stage 5, the move into the organisation: `flake.nix:14` and `app/Cargo.toml:33,35`.
- **Not landed:**
  - stage 2: `project()` is at `deployment.rs:132-137`, and `PROJECT_CHAIN` at
    `web/src/app/chain.ts:11`;
  - stage 3, the top-of-queue card, which is independent of this plan;
  - stages 6 and 7, which the stages below replace.

Each stage is one pull request that deletes what it replaces (AGENTS.md §10). ⚑ marks what
the owner approves.

| Stage | What | Deleted | The test that pins it (AGENTS.md §8) | ⚑ |
|---|---|---|---|---|
| 0 | SHARED stage 2: `[upkeep]`; the automated filers read it | `project()`, `PROJECT_CHAIN`, `--chain suzatary` in the train | the automated filers write only to `[upkeep]` | — |
| 1 | `--store` for `todo show`, `todos open`, `verify`; `prodrome init --store --public` | — | a store read by path reads as the same store mounted (`onequery`) | — |
| 2 | door 1 on a `public = true` charter; the secretary and signature objects refused there | — | each kind of hit is refused before the store holds it; the secretary's write is refused; no matched text is printed | — |
| 3 | create the roadmap repository, its ruleset and token; `prodrome carry`, a dry run with the restated text and a recommendation for each of the 43; then `--apply` and the first push through door 2; the generated `README.md` and the Pages viewer, and the code README's line showing the picture (§8.1) | the carried todos leave the upkeep list (a private `Cancelled` each) | the carry's migration test; `public-scan.sh` over the first range | ⚑ creating the repository, ⚑ the carried list, ⚑ the first push |
| 4 | the plan moves; read at run time; found by genesis; the `Tasks:` trailer; the land's commit and pin bump | `app/core/releases.toml`, `Plan::declared`, `release record`, the carry verb | `guards/train.rs` over a fixture roadmap: cut and check are functions of (tree, roadmap commit); a land completes exactly the `Completes:` todos | — |
| 5 | the deployment mounts it; `safe-push.sh` pushes it | `events-suzatary/` stays: it is upkeep | the push refuses a range `public-scan.sh` refuses | — |
| 6 | the routine reads the roadmap; `Task:` and `Task-note:` in AGENTS.md; the routine's rule 2 | the hand-pasted history in briefs | — | the routine's text |
| 7 | the combined list across mounted chains (§1.3) | the one-chain-at-a-time list as the only view | a law: the combined order is the per-chain orders merged by price | — |

### 8.1 What a visitor sees on GitHub (stage 3)

- **`README.md` is a link and a picture.** Typst draws the open list as SVG, with the
  renderer and look the application already has. There is no PDF (`nopdf`), and GitHub's
  Markdown inlines no PDF anyway. The README shows that image wrapped in one link to the page
  that hosts the prodrome, the Pages viewer below. There is a light and a dark variant, chosen
  by the reader's theme through `<picture>`, since a second draw of the same page costs little.
  - **Who draws it, and when**: the box, at its push, in the same commit that adds the
    objects, with the Typst the application already runs. So it cannot drift from the store.
  - **It is derived and never read back.** No reading takes anything from the SVG.
  - **What an image costs**: its text is not selectable or searchable, and only the one link
    around it works. The page is the view, and the README is its preview.
  - **The guard**: Typst draws glyphs as outlines, so the matcher cannot read the SVG's words,
    and `public-scan.sh` treats a file it cannot read as a hit until it is admitted
    (`publishedplaintext`). Door 2 admits the SVG only when drawing the store at that commit
    with the pinned renderer gives the same bytes. Its words are then exactly the words of
    objects door 1 has read. Without that check, every push would need an admission at a
    terminal.
- **A Pages viewer**, as the Prodrome's own repository has for its roadmap (its `viewer/` and
  `.github/workflows/pages.yml`, SHARED §6.2). The roadmap's `pages.yml` builds the application
  at the pinned revision (and through it the Prodrome at the revision the application pins).
  It writes static pages: the open list and each todo's history, priced by the core and drawn
  by Typst, as every page of the app is. It publishes them with the run's own `GITHUB_TOKEN`
  and Pages permission, and it holds no secret. The Prodrome's viewer reads the reference todo
  payload, not `Record`, so the roadmap uses the application's own pages and not that viewer.
- **The code repository's front page shows the same picture.** Its `README.md` shows the
  roadmap repository's raw `roadmap.svg` and `roadmap-dark.svg` by their URLs, in the same
  `<picture>`, wrapped in the same link to the Pages viewer. It is a link in a text file, not
  an input: the code builds the same without the roadmap repository, and the pin's direction
  stays roadmap → code. The leak guard skips a URL in a front page's link check
  (`scripts/public-tree.sh`, `dangling_links`), so the line needs nothing from it. GitHub
  caches a README's remote images, so the picture there can lag a push by a while. Stage 3,
  which creates the repository, adds the line through the train once the first push has put
  the SVG there.
- **These two are the whole of "inline".** GitHub runs no script in a README, and it does not
  read object files as a list. Typst stays the one markup and the one renderer: no Markdown
  printer is added, here or in the Prodrome.

## 9. What belongs in the Prodrome

Two pieces are generic over a schema. They should go to `fulfilium/prodrome`, with no
Suzatary type in them:
- **the reading of a range**: the registers a set of new objects moves, before and after. It
  would join `prodrome_core::fold`;
- **the union push**: add only new files, merge, retry. It would be a library form of the
  Prodrome's `roadmap-push.sh`.

Door 1, the lists and the routing are Suzatary's: they read a deployment.

## 10. Where this note departs from SHARED and MULTI

| Plan | It said | This note | Why |
|---|---|---|---|
| MULTI §6.4 | `.prodrome/` in the code repository, later | a repository of its own per project | the owner's choice (SHARED §9.6, and today); reuse |
| SHARED §6 | the roadmap's CI builds "the revision the user repository's flake pins" | the roadmap pins the code, bumped at land | a roadmap commit is a checkpoint with its reader, on public inputs only |
| SHARED §6 | a CI job in the code's publication reads the roadmap's head | not built | the train's `release check` reads it before land |
| SHARED §1, §6 | `releases.toml` stays in the code | it moves beside the store, found by genesis | reuse: the code knows no project's releases |
| SHARED §6 | the leak guard before the push | also at the write | a stored object is an ancestor of later writes |
| SHARED §6 | (silent) | no signature object published | a deployment's key registry is a fact about that deployment |
| SHARED §6 | the routine's sources and the App grant | an anonymous clone first | a grant stops the owner with a prompt |

## 11. Decided by the owner (2026-11-15), and what comes later

**Decided:**
1. A public roadmap repository per project, pinning its code; no submodule.
2. A public task is written deliberately by the owner or the main session, after door 1; the
   secretary is refused there; there is no publish hold.
3. Instants as written; the usual push.
4. Several roadmaps are combined in the view; a dependency across stores is a sentence until
   a reference named by genesis exists.
5. The name: `fulfilium/suzatary-core-roadmap`, and `<project>-roadmap` for any project.
6. A term learned after publication is the owner's call each time.

**Later**, because nothing needs them while there is one deployment:
- **Placeholders for private tasks.** A task about a deployment stays on its private chain,
  and the public list says nothing. The main session closes it by hand.
- **A project signing key.** The push right alone backs a public actor until another person
  writes (MULTI §5, stage B).
- **A second deployment's read-only mount**: a mount that refuses every write, for a deployment
  without the push right.
- **A store in another schema** (§1.3).

## 12. What this costs

- **The reading set**: the note adds nothing (no unit cites it), and stage 4 removes
  `app/core/releases.toml` from core.
- **The public code tree** loses `app/core/releases.toml`. The roadmap starts at about the size
  of `memory/` today (178 objects, 173,020 bytes).
- **The gate**: no new step. The train reads a public repository at `build` and `land`
  instead of the private chain. The roadmap's CI is about 30 lines of YAML, with no secret.
- **Code**, all ≈:
  - stage 1: +40;
  - stage 2: +30;
  - stage 3: +60, deleted in stage 4;
  - stage 4: −60 and +25;
  - stage 5: +10;
  - stage 0: SHARED's −14;
  - stage 3 also adds the static pages, the README's SVG and door 2's redraw check (about
    +40), and `pages.yml` (about 30 lines of YAML);
  - stage 7: about +60 lines of TypeScript.

  That is about +71 Rust lines, plus the TypeScript and SHARED §6's shell and YAML.
