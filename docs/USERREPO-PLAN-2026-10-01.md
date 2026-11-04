# The user repository: a private host that pins the public app

The owner, 2026-10-01: "We never need to do backwards compatibility btw, as we
can always move everything and delete the old version of stuff, what's
important is that we can always pull out an earlier checkpoint of the whole
project", and the repository that holds a user's data should pin the
application, "not the other way around due to one being private and the other
should be public one day, it can also host the specific section of the code
that acts like plugins and such".

A design, not built. It extends `DATASPLIT-PLAN-2026-09-14.md` (the data tree
split out of this repository) and replaces the live clone that an
earlier plan, since deleted, described.

## 1. The graph

Every arrow is a LOCKED pin (a flake input: a revision and its content hash):

```
prodrome (public)  ←  suzatary, the app (public one day)  ←  the user repository (private)
nixos config (private): enables the app's module, names the user checkout's path; pins nothing above
```

- **Private pins public, never the reverse.** The app never names a user, a
  path or a store. That is what lets it be public, and what lets a second
  person host their own: their user repository pins the same app.
- **The user repository is ONE repository** for everything Suzatary-specific
  about its user: the stores, `instance.toml`, briefings and notes, the live
  pins (`conformance/`), the personal-terms list, and the plugins. They must
  move together with the app pin, so splitting them would break the
  checkpoint.
- **The OS config stays its own repository**, for three reasons. Cadence: the
  user repository gets an automated commit every five minutes, while the OS
  changes by hand behind a sudo rebuild. Authority: the agent's push writes
  the user repository, and the file that grants sudo, tailnet and services
  must be out of its reach. Portability: a second host need not run NixOS.
  It does not pin the user repository either, or every automated commit
  would leave it stale.

## 2. The checkpoint

The invariant this design protects, in place of backward compatibility:

> One commit of the user repository names the whole state: the stores, and
> the exact app (and through it the exact Prodrome) that wrote them.

- **Restore** is a checkout of that commit and a build from its lock.
- **A migration is one commit**: bump the app's pin, run the app's migration
  verb, delete the old store or format, commit. The commit before it holds
  the old data with the code that reads it, so no reader of an old form is
  ever kept. `secretary-inbox/`, kept beside `secretary-inbox-proposals/`
  "until the owner says", can go under this rule.
- **What a commit does not hold**: mail (a rebuildable cache) and secrets (in
  secretspec, captured by the weekly encrypted vault). A full checkpoint is a
  commit plus the vault snapshot of its week.

## 3. The user repository as a flake and a host

The user repository becomes a flake: `inputs.suzatary` is the app, and its
outputs are what this user runs.

- **Stage one**: it re-exports the app's packages (the binary, the web, the
  module), so nothing changes but where the pin lives.
- **Later, the host crate**: the app becomes a library plus a default binary
  (`suzatary::run(Host)`), and the user repository's own small crate builds
  this user's binary and browser module from it. That is the same shape as
  the wasm work: a host instantiates the app's generic pieces with its own.
- **Plugins** are what a host adds, and since 2026-10-02 they are
  WebAssembly components loaded at run time, not code compiled in:
  `PLUGINS-PLAN-2026-10-02.md` (a component targeting a WIT world, its own
  schema and its Typst views; its authority is exactly its imports, never the
  app's). The user repository's `plugins/` holds this user's. What stays
  compiled into the host crate is only what is not a plugin: the secretary's
  and the agent's manuals, and this deployment's configuration.

## 4. Deploy is a commit

Today `scripts/promote.sh` fast-forwards a separate live clone, and the units
run that clone's scripts. Under this design:

- **The units run the app from the user repository's lock**: the app's
  source is a Nix store path, so `hourly-triage.sh` and its siblings run from
  `/nix/store/<app>/scripts`, never from a git checkout. The live clone goes.
- **A deploy is a commit** to the user repository that moves the app's pin
  (`nix flake lock --update-input suzatary` to a revision on the app's
  `master`), so its history records which code ran when, which nothing does
  today. Rollback is a commit back to an earlier lock.
- **The roots refresh watches the user repository's `flake.lock`** instead of
  the live clone's `HEAD`, and the deploy keeps `promote.sh`'s hard-won wait:
  only a roots run that STARTED after the move counts, and only its success
  restarts the web.
- **The dev loop is unchanged**: code is edited in an app checkout, and its
  builds link into `roots-dev`, which the units never read.

## 5. What moves where

| From | To |
|---|---|
| the live clone (`~/.local/share/suzatary/live`), `services.suzatary.repo` | the app's store path, from the user repository's lock |
| `scripts/promote.sh` moving a branch | a deploy commit moving a lock, run by a script the app ships |
| the nixos config's `services.suzatary.dataDir` and the briefing units' paths | one option naming the user checkout; everything else derived from it |
| deployment knowledge in the app (anything naming this box) | the user repository, found by the existing personal-terms guard and a new one for paths |

## 6. Stages

Each is one pull request, gated in full, and stage 2 needs the owner's yes
for its NixOS rebuild.

1. **The user repository is a flake that pins the app** and re-exports its
   packages. No behaviour change; checkpoints start.
2. ✅ **Built 2026-10-04, app side** (the cut-over on the box is in its pull
   request's report). **The units run the app from that lock**;
   the roots refresh watches it; a deploy is a lock commit (a script the app
   ships, keeping `promote.sh`'s wait); the live clone and `promote.sh` are
   deleted. As built: `services.suzatary.userRepo` is the one option; the
   roots service is the lock's one reader (`nix/app-input.nix`) and links
   `$SUZATARY_ROOTS/app` only after the binary and the bundle build, so a
   failed build moves no unit; every other unit roots that link for itself
   and runs its script from it; `scripts/deploy.sh <rev>` pins the input
   with `nix flake lock --override-input`, commits "Deploy <rev>", waits, and
   pushes last. The code root is read-only and git-less, so the memory
   snapshot, the smoke build and the agent's diff moved to the state, the
   cache and the data tree.
3. **The app is a library and a default binary**; the user repository's host
   crate builds this user's binary and browser module.
4. **Plugins**: `PLUGINS-PLAN-2026-10-02.md`'s stage 5, components loaded
   from this repository's `plugins/`; deployment-specific code still in the
   app moves out, found by a guard that finds code naming this deployment.
5. **A second host**: a flake template a new user starts from, and the
   owner's decision on publishing the app (a fresh clone, as
   `DATASPLIT-PLAN-2026-09-14.md` §9 says, since this repository's history
   holds the data).

## 7. Non-goals

- A git submodule. A flake input gives the same checkpoint with the tools
  already in use, and none of a submodule's traps (a detached head, a
  forgotten update, a recursive clone). The one thing a submodule adds, the
  app's source inside the data tree, is not needed: the app is edited in its
  own checkout.
- Compatibility of any kind with an old store or format. The checkpoint is
  the compatibility.
