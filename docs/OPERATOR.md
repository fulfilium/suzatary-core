# The operator's notes: core's settings for the gate, and where a deploy's builds come from

The root [`OPERATOR.md`](../OPERATOR.md) is the triage agent's manual. This
file is for the person who runs `scripts/train.sh`: what the public repository
`suzatary-core` (this checkout's remote `core`) must allow before
`scripts/train.sh gate --remote github` can run a train's tree half on
GitHub's runners there.

## What runs where

`gate --remote github` splits the merge gate's tree half
(`scripts/merge-gate.sh --no-live`) between the box and core:

| Step | Where | Why |
|---|---|---|
| the leak guard (`public-tree.sh --check --strict`) | the box, first | it reads the data repository's lists, and a tree that fails it sends nothing |
| `fmt` | the box, before the push | its workspace names test targets the public tree leaves out |
| `changelog` | the box, before the push | it compares with this repository's `origin/master` |
| the deploy's builds (`.#suzatary`, `.#suzatary-web`, `.#suzatary-plugins`) | core, master's `publication.yml`, dispatched with the gate commit | pushed to the `bmabsout` Cachix for the box |
| `public-tree` | the box, after `publication.yml` | it builds the public tree from this private repository (`public-tree.sh --store HEAD`), substituting from the Cachix |
| `deployed-shape` | the box, after `publication.yml` | it runs the binary `public-tree` built |
| `clippy`, `clippy-fixture`, `doc` | core, job `checks` | |
| `build`, `fence-plugin`, `test`, `test-fixture`, `plugin-fence`, `plugin-sync` | core, job `tests` | |
| `guards` | core, job `guards` | |
| `web-inputs`, `web-test`, `build-web` | core, job `web` | |
| `e2e` (with `web-inputs` and `build-web` again) | core, jobs `e2e-1` to `e2e-4` | each runs a quarter of the scenes (`scripts/e2e-part.sh`) |
| `nix-web`, `nix-module`, `release-free` | core, job `nix` | |

The box's lists are `box_before` and `box_after` in `scripts/train.sh`, the jobs' are the matrix
of `.github/workflows/gate.yml`, and `app/cli/tests/guards/gate_yml.rs` fails
when the two do not hold every step of the tree half between them. Every job
runs on `ubuntu-24.04-arm`: the box is aarch64, and no step needs x86_64.

The train pushes one commit, the public tree of the train commit on core's
`master`, to the branch `gate/<train sha>`, never to `master`, waits for the
run, and deletes the branch. The push starts `gate.yml`, the lanes. Then the
train dispatches master's `publication.yml` with the gate commit as its
input (`gh workflow run publication.yml --ref master -f commit=<sha>`). That
run builds what a deploy builds from the gate commit and pushes it to the
Cachix. The box waits for that run before it runs `box_after`, so
`public-tree` downloads the builds instead of compiling them. It waits at
most 600 seconds for the run to appear and 12000 seconds
(`SUZATARY_TRAIN_CACHE_TIMEOUT`) for it to end, past the run's own
180-minute limit. Each question to GitHub has 60 seconds, and ten failed
questions in a row end the wait. When the run failed, never appeared, ran
past the wait, or could not be started or asked about, the train says which
and the box builds the public tree itself, as before. Then the live half
runs on the box, and its `deploy-build` finds the same builds in the store.
The `gate.yml` run is waited on beside it. When the half ends, and when a
train is killed, the train cancels the gate commit's runs that have not
ended, then deletes the branch.

The train's `gh` must be allowed to dispatch a workflow on core (a
fine-grained token's Actions permission, read and write).

## The settings, once

Run `scripts/core-settings.sh` at a terminal, with `gh` logged in as an
administrator of core and `CACHIX_AUTH_TOKEN` in secretspec (`secretspec set
CACHIX_AUTH_TOKEN`, the push token of the `bmabsout` cache, scoped to that
cache). It sets:

- **Actions on**, allowing GitHub's own actions,
  `cachix/install-nix-action` and `nix-community/cache-nix-action`'s
  `restore` and `save` alone; every workflow pins each by its commit.
- **`gate.yml`, `gate-store.yml` and `publication.yml` on, every other
  workflow off.** The public tree carries the private repository's
  workflows too (`quick.yml` would run on every publication,
  `update-flake-lock.yml` on its schedule).
- **The runs' token reads only** (`default_workflow_permissions=read`) and
  cannot approve a pull request. `gate.yml` asks for `contents: read` itself,
  and its checkout keeps no credential.
- **An outside contributor's run waits for an approval**
  (`all_external_contributors`). `gate.yml` runs on a push to `gate/**` alone,
  so no pull request runs it; this covers a workflow added later.
- **The ruleset `master-updates-admin-only`**: only a repository
  administrator may move `master` (an `update` rule whose one bypass is the
  administrator role). A run's token is never an administrator, even when its
  workflow asks for `contents: write`. The train's publish pushes as the
  owner's login, which is one.
- **The environment `cachix`**, which only a run on `master` may enter, and
  in it **the secret `CACHIX_AUTH_TOKEN`**, which `publication.yml` alone
  names, read from secretspec and given to `gh secret set` on its stdin, so
  it is never in an argument or printed. A repository-wide copy of the
  secret, which the script set before, is deleted.

Nothing else in the gate sets any of these. Core's other ruleset, which keeps
`master` fast-forward only and undeletable, stays as it is.

The update rule matters because `default_workflow_permissions=read` is only a
default. A workflow file in a gate commit runs on the push to its branch, and
it may ask for `contents: write`. Without the rule, it could fast-forward
`master` with a commit that changes only a script or `flake.lock`, and the
next publication run would run that code. With the rule, only an
administrator moves `master`.

The credential the train pushes to core with must be allowed to push a
change under `.github/workflows/` (a fine-grained token's Workflows
permission, read and write), since every publication and every gate commit
carries `gate.yml`.

## Who can reach the Cachix token

`publication.yml` has two jobs, so the code that reads a tree and the token
are never in one job:

- **`build`** enters no environment and holds no secret. Its token reads
  this public repository and nothing else. It checks out master at `.` and
  the commit to build apart, at `./tree`. It runs master's tripwire over the
  tree, evaluates and builds the tree, and measures what a push would add.
  Then it exports the unserved paths (`nix-store --export`) and the list of
  served references as an artifact.
- **`push`** enters the environment `cachix`, so GitHub hands it the token
  when the job starts. It runs on a fresh runner with master's checkout
  alone. It builds master's `cachix` before it reads the artifact. It
  fetches the served references from the two caches, with their signatures
  checked, and then imports the export. It measures the paths again (each
  in the store, none served, at most 1 GB together) and pushes that list.

So a build that the tree compromises can choose what one push contains, at
most 1 GB in the paths its list names. It never holds the token.

Only master's copy of the workflow runs these jobs. The environment admits
`master` alone, and both jobs refuse any other ref and any run that the owner
(`github.triggering_actor`, `bmabsout`) did not start. The update rule above
keeps any workflow from moving `master`. A run starts only from a push to
`master` (a publication) or from a dispatch on `master` (a train's gate
commit), and the train does both with the owner's credential.

A gate branch carries the train's own workflow files, and GitHub runs them
on a push to that branch. The environment's policy checks only the branch,
so no workflow on a gate branch may enter it. That is why the train
dispatches master's workflow and does not let the gate branch run one. No
job of `gate.yml` holds a secret. Its token reads the repository and nothing
else, and it reads the `bmabsout` cache as a plain substituter.

The only code from the tree that runs is in the `build` job:

- Nix's pure evaluation of its flake, with `accept-flake-config = false` and
  `--no-update-lock-file`, so it locks no input from the runner's disk;
- its builds, which run in Nix's sandbox as build users that the daemon
  ends after each build.

## What reaches the cache

The privacy boundary is the box's leak guard: the train and a publication
push to core only a public tree whose leak guard passed whole
(`--check --strict`, with the data repository's lists). Beyond that:

- No script or Nix file on the box pushes to the Cachix or names its token;
  `scripts/core-settings.sh` hands the token to GitHub and pushes nothing.
  `publication.yml` is the one writer, and it runs only in core
  (`if: !github.event.repository.private`).
- Master's `scripts/public-source.sh` runs over `./tree` before the builds.
  It is a tripwire, not a boundary: it refuses a tree holding a file
  `.gitattributes` marks `export-ignore`, as this private repository's own
  checkout does. A public tree holds no `.gitattributes`, so it passes every
  public tree, whatever else that tree holds.
- Every build is pure (no `--impure`, no environment read), so no build
  reads the deployment.
- A run on `master` reads only the Actions caches saved on `master`. So a
  lane of `gate.yml`, which runs a train's code outside Nix's sandbox, cannot
  plant a store entry that the `build` job restores. `gate-store.yml` writes
  those caches, and so could any job of a run on `master`. With the update
  rule and the two jobs, planting an entry there takes an escape from Nix in
  the `build` job, which holds no token. That risk is accepted.

A gate tree that turns red in a `gate.yml` lane may already be in the cache,
since the publication run ends before the lanes usually do. That tree passed
its leak guard, and the cache evicts it in time.

`app/cli/tests/guards/publication.rs` holds each of these.

## The two caches and their budgets

- **The `bmabsout` Cachix** holds 5 GB on Cachix's free plan and evicts the
  least recently used past it; the Prodrome's CI pushes to it too.
  `publication.yml` is its one writer here: for each publication and each
  gate commit it pushes the closures of the app, the web bundle and the
  plugins, less what cache.nixos.org or the cache already serve. A landed
  train's publication holds the tree its gate commit held, so its run finds
  the builds served already and adds almost nothing; a train that never
  lands leaves its builds for the cache to evict. Its step "Measure what the push adds" prints that
  size, and fails, pushing nothing, when one publication would add more
  than 1 GB; the deploy of that commit then builds on the box.
- **The Actions cache of core** holds 10 GB and evicts past it. The gate's
  jobs keep a warm Nix store there: `gate-store.yml`, on each push to
  `master`, builds the dependency builds no binary cache serves (the three
  crane dependency builds and the Prodrome's typst module), collects the
  store down to them and saves it under one key of `flake.nix`, `flake.lock`
  and the Cargo manifests and lock, then purges every older entry of it.
  It is saved on `master` because a run on a branch reads only the caches of
  that branch and of `master`, and each train's gate branch is new. Each
  gate job restores it, or the newest before it when the key moved, and
  saves nothing; a miss or a failed restore makes the job slower, never red.
  `publication.yml` restores it too, so a run on a `Cargo.lock` that did not
  move builds only the app, and a run on one that moved starts from the
  newest store before it.
  The job's last step fails when the entry is over 5 GB, so two of it, the
  most there is while a new one is saved, stay inside the 10 GB. The purge
  needs `actions: write`, which `gate-store.yml` alone holds.

## What the verdict means

A run's verdict is the tree half's, recorded under the train commit's tree as
any gate's: success is green; red only where a job's `red` step failed, which
it does only when its `scripts/merge-gate.sh` exited 1; anything else (a
cancel, a runner lost, a timeout, a job that never reached its gate, a gate
that exited 75 because it did not start) is no verdict, so the train gates
again, never lands, and never calls the tree red.

## A deploy downloads its builds

Every commit of suzatary-core's `master` is a publication
(`scripts/train.sh publish`), and each one runs
`.github/workflows/publication.yml` on GitHub's runners: it builds, for the
box's system (aarch64-linux), the app, the web bundle and the plugins, and
pushes them to the `bmabsout` Cachix, which the box substitutes from (its
NixOS configuration lists it).

`scripts/deploy.sh <rev>` waits for that run on the commit it pins before it
builds, and says which way it went:

- the store holds the builds already (the merge gate's `deploy-build` made
  them): no wait;
- the run succeeded: the build downloads every output;
- the run failed, was still going after `SUZATARY_DEPLOY_CACHE_TIMEOUT`
  seconds (1200, about what a build here takes), never appeared within `SUZATARY_DEPLOY_CACHE_APPEAR`
  seconds (120), or GitHub could not be asked: the box builds them itself,
  as it always did.

A commit deployed with `--ref <branch>` is not on `master`, so no run builds
it and the box does.

## A deploy restarts every unit that runs the app

The units that run the app for longer than one job are the ones part of
`suzatary-app.target`, declared by `servingUnit` in `nix/nixos-module.nix`,
and once the roots run links the new app a deploy try-restarts each of
them that is running and checks that it runs the new app (today the web and
the IMAP watcher).
