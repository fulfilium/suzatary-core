# The typed tooling plan (2026-11-19)

The plan of the roadmap task `typedtooling`: `scripts/train.sh` (the merge train) and then
`scripts/merge-gate.sh` (the gate) stop being shell. The design note is the description of pull
request #559; this file keeps only what guides the work still open. The task completes with T4.

## 1. The decision

Port the train to Rust as a small workspace member, `app/tooling` (`suzatary-tooling`, binary
`train`), in four pull requests, each deleting the shell it replaces. The member depends on no
other member and builds with `-p` into its own target directory under `.git`, so a gate lane never
builds the whole binary for it. Its pure core is a state machine, and one supervisor loop over one
channel runs its effects. Port the gate's orchestration into the same crate afterwards, in three
pull requests, only once the train's port has met the bar in section 7. Do not use Python
(`nopython`). Do not use Nix for orchestration: it cannot express a push, a wait or a signal.
`git` and `gh` stay subprocesses. The test seams (`SUZATARY_TRAIN_*`) are the cargo feature
`seams`, so the binary the train runs reads none of them (`seamfeature`). The stored formats under
`.git/train/gates` and `.git/gate` do not change, so no cutover forces a re-gate.

## 2. The train's states and events

A train's state is never stored. It is read from the refs and the table. `T` is the `train` ref
and `t` its tree.

| state (read from) | `build <pr>…` | `gate [--full] [--remote github]` | `bisect` | `land` | `publish [<rev>]` | `record [<rev>]` |
|---|---|---|---|---|---|---|
| **None**: no `train` ref | → **Built**, or refused when no pull request merges | refused: "no train" | refused | refused | publishes `origin/master`'s missing rows | records `origin/master`'s newest release |
| **Built**: `T` = `origin/master` + merges + release | rebuilt from `origin/master` | runs both halves of `t` → **Gated** | refused: "gate first" | refused: "gate first" | as None | as None |
| **Gated(r_tree, r_live)**, each a `Reading` of `t` | rebuilt | adds rows | **Red**: probes prefixes, reads the table first; **Green**: refused | both **Green** → push → **Landed**; **Unjudged**, **Red** or **Finding** → refused, each with its own words | as None | as None |
| **Stale**: `origin/master` moved under `T` | — | gates `t` anyway | — | rebuilds on the new master, then refuses: "gate again" | — | — |
| **Landed**: `origin/master` = `T`, tag on origin | — | — | — | the next land's `sync` repairs the rest | → **Published** | — |
| **Published**: core holds `Source: T` | — | — | — | — | no-op | → **Recorded** |
| **Recorded**: the roadmap holds `Shipped: T` | — | — | — | — | — | no-op |

HUP, TERM and INT stop every half the train started before anything waits on one; KILL runs
nothing, and the lock is free when the last process that holds it ends. Beside the machine:
`nightly` gates `origin/master` full and files one upkeep todo per red tree; `check <pr>` gates a
pull request's head on the runners and records no verdict; `sync` records `origin/master`'s newest
release on the roadmap, the repair of a land cut short. A land's progress is a reading of three
remotes (`landreadsdone`), never a file.

## 3. The types

```rust
pub enum Half { Tree, Live }
pub enum Outcome { Green, Red }                    // the one fact a row stores
pub enum Ended { Judged(Outcome), DidNotStart }    // T3 adds RunLost, Stopped, LeakGuardRefused
pub enum Reading { Unjudged, Agreed(Outcome), Finding }   // a diamond: the join of the rows
pub enum LandOrder { Green, Unjudged, Red, Finding }       // the chain a land reads
pub struct Table { .. }                            // .git/train/gates/<gate>/<half>[-full]/<tree>

pub struct Built { base: Commit, merges: Vec<Merge>, release: Option<Release>, head: Commit }
pub struct Landable { built: Built, tree: TreeId } // made only by Table::landable; land takes it
pub enum LeakCheck { Clean(PublicTree), Hits(Vec<Hit>), CouldNotRun(CheckError) }
pub struct Matcher(PathBuf);                       // only Matcher::deployed(&Roots)
pub struct Admitted(RepoPath);                     // only Admitted::at_terminal(path, &Terminal)

pub enum Msg { Ended(Half, ExitStatus), Stop(Signal), Polled(RunState), Published(CacheWord) }
pub enum Effect { Start(Half, Steps), SignalGroup(Half, Signal), Cancel(RunId), DeleteBranch(Branch), Write(Row) }
pub fn step(state: Gating, msg: Msg) -> (Gating, Vec<Effect>);   // pure; one runner owns every child
```

The runner puts each half in its own process group (`Command::process_group(0)`), signals a group
with `rustix::process::kill_process_group`, and receives every reap and signal on one `mpsc`
channel. The leak guard's matcher stays the deployed binary.

## 4. The laws, as tests

1. **The reading.** `reading(rows ++ rows') = reading(rows) ⊔ reading(rows')` in the diamond; no
   rows read Unjudged; Finding absorbs; it gives what the shell's `verdict` gave.
   `law_1_a_reading_is_the_join_of_its_rows`, and the land's order:
   `a_trains_verdict_is_the_greatest_half_in_the_land_order` (both in T1).
2. **The stop.** For every sequence of `Msg` with a `Stop` in it, the effects after the `Stop`
   are a `SignalGroup` for every started half before any wait, one `Cancel` per run and one
   `DeleteBranch`, no `Write`, and the worktree's removal after every half's `Ended`. Proptest over
   generated interleavings, with no sleep and no retry (`racetest`). The 12 timing guards are its
   witnesses on real processes. (T3)
3. **A red half stops the other.** For any interleaving, an `Ended(h, Red)` before the other half
   ends yields `SignalGroup(other)` and no green row for it. (T3)
4. **Bisect.** If red is monotone over prefixes, the answer is the least red prefix in at most
   ⌈log₂ n⌉ probes, and a prefix already in the table is never probed. (T3)
5. **Publication.** The rows are the first-parent commits after the newest `Source:`, oldest
   first; a skip is accepted only for a failing commit that is not the last;
   `publish(publish(s)) = publish(s)`. (T2)
6. **Record.** Running it twice is running it once, and each write is skipped exactly when its
   repository holds the result (`landreadsdone`). (T4)
7. **The stored formats.** The table's and the pass store's lines parse and print to the same
   bytes, and a table today's script wrote reads the same in the port.
   `law_7_the_stored_rows_read_and_print_as_train_sh_wrote_them` (T1; the pass store's in G2).
8. **Admission.** No `Admitted` without a `Terminal`, as a `compile_fail` doc test. (T2)
9. **The matcher.** `suzatary-tooling` depends on no workspace member.
   `law_9_the_tool_depends_on_no_workspace_member` (T1).
10. **The gate's key.** Changing a tree entry outside a step's reads keeps its key, and changing
    one inside moves it, over generated trees. (G2)
11. **Merge order.** The order is a function of the set of heads and respects ancestry. (T4)

## 5. The order of work

Each pull request rides a train, and deletes the shell it replaces. `train.sh` stays the one entry
until T4 and calls the tool for each ported verb. At every commit, one program reads each stored
file. Each port pull request keeps every train guard's assertions, except where a property became a
type, and lists each such exception (`portdifferential`).

- **T1, the crate and the verdict table**: the pull request that added this plan.
- **T2, publication.** `publish`, `checked`, `fresh_rows`, `row`, `tree_of`, `public_commit`,
  `published`, `skipped_by`, `newest` and `on_chain` (about 230 lines) become `train publish`, with
  `LeakCheck`, `Matcher::deployed` and `Admitted::at_terminal`. Laws 5 and 8. #412 lands or
  closes first. Safe while trains run: `publish` is idempotent and only climbs.
- **T3, the gate run and the stop path.** `gate`, `beside`, `finish`, `run_half`, `run_here`, the
  `cloud_*` functions, `gate_run`, `bisect`, `nightly_gate`, `nightly_remote`, `check`, the lock
  and `scripts/groups.sh`'s use by the train (about 500 lines) become the `step`/`run` pair. Laws 2,
  3 and 4, the stop law first. Safe: a stopped gate writes no row, so the worst case is a re-gate.
- **T4, build, land, record and the entry.** `build`, `cut_release`, `land`, `record_release`,
  `record_done`, `sync` and the dispatch go; `train.sh` becomes a launcher of about ten lines;
  `.git/train/skipped` goes. Laws 6 and 11. T4's own land is run by T4's land, as today's script
  lands every change to `land`; the guards over the stubbed origin, core and roadmap run first.
- **G0, in shell, any time.** A `place` column on the gate's step table, which the train reads
  from `--list`. The train's four step lists go, and so does `gate_yml.rs`'s check that they
  partition the steps.
- **G1, the step table in Rust.** `STEPS`, `--list` and `--steps` parsing; `merge-gate.sh` reads
  the table from `gate list`. An unknown step name is a parse error (64).
- **G2, the planner.** Keys, passes, units and `gate-units.mjs`, with law 10.
- **G3, the runner.** Lanes, the heavy turn, the quiet lane and the summary as a value.
  `merge-gate.sh` becomes the launcher, and `gate.yml` runs `cargo build -p suzatary-tooling` in
  the shell step it already has.

## 6. What stays shell

A script stays shell while it is a sequence of tool calls whose decisions are verbs of the binary.
It moves when it supervises children that must be stopped, or keeps state between runs that a type
could hold.

| script | stays or goes | why |
|---|---|---|
| `deploy.sh` | goes, after T4 | it reverts a deploy on TERM, HUP and INT, waits on a unit's run and acts outward: the same `step`/`run` pair fits it |
| `web-e2e.sh` | stays for now | it runs fixture servers and node, and kills them by group; its history is scene work |
| `hourly-triage.sh` | stays | a sequence of tool calls under a fence; its decisions are verbs of the binary |
| `public-tree.sh` | stays | the leak guard's driver; the person who admits a file must be able to read it |
| `safe-push.sh` | stays | it builds nothing (`datapushverifiesdata`) |
| `public-scan.sh` | stays | door 2's driver; its decisions are verbs of the binary |
| `build-web.sh`, `web-static.sh`, `deployed-shape.sh` | stay | a build recipe, grep rules, a scenario of tool calls |

## 7. The bar before the gate

Stop before the gate if, after T4, any of these holds. T4's pull request measures each figure.

- The train's guards have not shrunk by at least the stubs-and-run tests that became types or
  laws.
- Their run time has not fallen.
- The reading-set delta of T1 to T4 together is positive.

Each stage restates the running total of its reading-set delta against `origin/master`, from
`scripts/context.sh`, here:

| stage | delta | running total |
|---|---:|---:|
| T1, the crate and the verdict table | +9,943 | +9,943 |

Revisit Python only if `nopython` is revised. Move the gate's Rust steps to Nix only when a
derivation per workspace member keeps a CLI-only change near today's time on the runners; the
typed gate's step runner keeps a `Runner::Derivation(attr)` arm for that.
