# The typed tooling plan (2026-11-19)

The plan of the roadmap task `typedtooling`: `scripts/train.sh` (the merge train) and then
`scripts/merge-gate.sh` (the gate) stop being shell. The design note is the description of pull
request #559. The port ended at its third stage (section 7): `build`, `land`, the records and
`sync` stay in `train.sh`, which calls the tool for the table, the publication and the gate.

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
nothing. Only the train's process holds the lock, so it is free the moment the train ends; a KILLed
train's halves run on as orphans, the next gate removes their worktree, and the next remote gate
cancels and deletes the gate branch they left. Beside the machine:
`nightly` gates `origin/master` full and files one upkeep todo per red tree; `check <pr>` gates a
pull request's head on the runners and records no verdict; `sync` records `origin/master`'s newest
release on the roadmap, the repair of a land cut short. A land's progress is a reading of three
remotes (`landreadsdone`), never a file.

## 3. The types

```rust
pub enum Half { Tree, Live }
pub enum Outcome { Green, Red }                    // the one fact a row stores
pub enum Ended { Judged(Outcome), Unjudged, Refused(Check) }   // T3: a lost run is Unjudged; Stopped is a state
pub enum Reading { Unjudged, Agreed(Outcome), Finding }   // a diamond: the join of the rows
pub enum LandOrder { Green, Unjudged, Red, Finding }       // the chain a land reads
pub struct Table { .. }                            // .git/train/gates/<gate>/<half>[-full]/<tree>
pub enum Line { Row(Row), SetAside(SetAside) }     // gateverdicts: a set-aside is a row of its own
pub enum Beside { Nothing, Data(Data) }            // a key: the tree, and a live row's stores' digest

pub struct Built { base: Commit, merges: Vec<Merge>, release: Option<Release>, head: Commit }
pub struct Landable { built: Built, tree: TreeId } // made only by Table::landable; land takes it
pub enum LeakCheck<T> { Clean(T), Hits, CouldNotRun(CheckError) }   // T2; T is PublicTree
pub enum Publishable<T> { Publishable(T), Leak, CouldNotRun(CheckError), Held(Vec<RepoPath>) }
pub enum Push { Row, GateBranch }                  // a gate branch holds new admissions alone
pub struct Matcher(PathBuf);                       // only Matcher::deployed()
pub struct Admitted(RepoPath);                     // only Admitted::at_terminal(path, &Terminal)

pub struct Plan { tree: Vec<Step>, live: Vec<Step>, on_red: OnRed, .. }   // a Step: a Piece, and
                                                   // the other half's piece it waits for
pub enum Msg { Ended(Half, Exit), Polled(Answer), Woke(Alarm), Stop(Signal) }
pub enum Effect { Start(Half, Job), SignalGroup(Half, Signal), Wake(Alarm), Write(Half, Outcome),
                  AppendLog(String), RemoveWorktree, Cancel(Runs), DeleteBranch(GateBranch), Unlock, .. }
pub enum Runs { OfBranch(GateBranch), OfCommit(Oid, GateBranch) }   // a close, from the push's start
pub enum Status { Read(u8), Unread }               // a leader's status; unread is never green
pub fn step(state: Gating, msg: Msg) -> (Gating, Vec<Effect>);   // pure; one runner owns every child
```

The runner (`Supervisor`, T3) puts each child in its own process group
(`Command::process_group(0)`), signals a group with `rustix::process::kill_process_group`, is its
children's subreaper so a group is gone only when its orphans are, and receives every reap, timer
and signal (signal-hook) on one `mpsc` channel. The lock is a `File` no child inherits. The leak
guard's matcher stays the deployed binary.

## 4. The laws, as tests

1. **The reading.** `reading(rows ++ rows') = reading(rows) ⊔ reading(rows')` in the diamond; no
   rows read Unjudged; Finding absorbs; it gives what the shell's `verdict` gave. Since
   `gateverdicts`, a key's reading is the join of the rows that stand at it: judged at its data,
   and named by no set-aside (`the_land_order_with_set_asides`,
   `a_land_reads_the_live_half_at_its_newest_row`); the live half's key is its newest row's data.
   `law_1_a_reading_is_the_join_of_its_rows`, and the land's order:
   `a_trains_verdict_is_the_greatest_half_in_the_land_order` (both in T1).
2. **The stop.** For every sequence of `Msg` with a `Stop` in it, the effects after the `Stop`
   are a `SignalGroup` for every started half before any wait, one `Cancel` per run and one
   `DeleteBranch`, no `Write`, and the worktree's removal after every half's `Ended`. Proptest over
   generated interleavings, with no sleep and no retry (`racetest`):
   `law_2_a_stop_signals_every_group_first_and_writes_nothing`. Its witnesses on real processes
   are `train_stopped_by_term_stops_the_half_it_runs` and
   `train_gate_github_killed_deletes_its_branch`. (T3, `tests/gating.rs`)
3. **A red half stops the other.** For any interleaving, an `Ended(h, Red)` before the other half
   ends yields `SignalGroup(other)` and no green row for it; beside it, no two gates run at once
   (one worktree). `law_3_a_red_half_stops_the_other`. (T3)
4. **Bisect.** If red is monotone over prefixes, the answer is the least red prefix in at most
   ⌈log₂ n⌉ probes, and a prefix already in the table is never probed.
   `law_4_bisect_finds_the_least_red_prefix`. (T3)
5. **Publication.** The rows are the first-parent commits after the newest `Source:`, oldest
   first; a skip is accepted only for a failing commit that is not the last;
   `publish(publish(s)) = publish(s)`. `law_5_the_rows_are_the_first_parent_commits_after_the_newest_source`,
   `law_5_publishing_twice_is_publishing_once` and `a_skip_is_never_taken_on_a_check_that_did_not_run`
   (T2, `tests/publication.rs`).
6. **Record.** Running it twice is running it once, and each write is skipped exactly when its
   repository holds the result (`landreadsdone`). (T4)
7. **The stored formats.** The table's and the pass store's lines parse and print to the same
   bytes, and a table today's script wrote reads the same in the port.
   `law_7_the_stored_rows_read_and_print_as_train_sh_wrote_them` (T1; the pass store's in G2).
8. **Admission.** No `Admitted` without a `Terminal`, as a `compile_fail` doc test (T2,
   `admitted.rs`). Beside it, the one check before every push to core, as one table of its
   outcomes: `the_check_before_a_push_is_a_table_of_its_outcomes` (`tests/publication.rs`).
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
  `published`, `skipped_by`, `newest`, `on_chain` and `gate_push` became `train publish`,
  `publishable`, `push-gate` and `published`, with `LeakCheck`, `Matcher::deployed` and
  `Admitted::at_terminal`. Every push to core, a gate branch's too, passes one check
  (`publishable`). Laws 5 and 8. Next in the same vein: `scripts/deploy.sh` still reads core's
  `Source:` trailers itself, and moves to `train published` when it is ported.
- **T3, the gate run and the stop path.** `gate`, `beside`, `finish`, `run_half`, `run_here`, the
  `cloud_*` functions, `gate_run`, `bisect`, `nightly_gate`, `nightly_remote`, `check`, `report`,
  `failing_lines`, the lock and `scripts/groups.sh`'s use by the train (618 lines) became
  `gating::step`, `Supervisor`, `Lock` and the verbs `train gate`, `bisect`, `nightly`, `check`
  and `locked`; every other verb of `train.sh` runs as a child of `train locked`. Laws 2, 3 and 4.
  Left for T4: `report_nightly` (`train.sh nightly-report`) and `sync`, which call the app through
  `resolve_app`, ported with `build` and `land`.
  Also left for the last stage, from T3's third review: the law of a green end folds
  `Status::reaped` and does not bind the reaping loop itself; the KILL after the grace is held by a
  law for a stop's alarm, not for the rest of a group after its leader exits (no test leaves a
  process that ignores TERM); and the calls to the network that no signal interrupts (the sweep,
  the delete, `check`'s reads and its post), which the shell had too.
- **T4, build, land, record and the entry (not done: the port ended at T3, section 7).** `build`, `cut_release`, `land`, `record_release`,
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
| T2, publication | +15,540 | +25,483 |
| T3, the gate run and the stop path (projected +30,400 at T1 and T2's rate) | +28,826 | +54,309 |

T2 removed 269 lines of `train.sh` (about 5,800 tokens) and five stub guards whose properties
became laws or types, and added about 1,580 lines of Rust (about 21,300 tokens, with its README): a
typed refusal for each of the shell's `die` lines, the four outcomes of the one check as a sum, the
gate branch as a type, and the rows and admissions read once as values. At this rate the third
clause of the bar fails after T4 unless T3 and T4 delete more than they add. So T3's brief states
its projected delta before any code, and each later stage deletes, in the same pull request, the
stub guards its laws replace.

T3 removed 618 lines of `train.sh` (about 12,100 tokens) and five stub guards whose properties
became laws, and added about 2,550 lines of Rust (about 37,700 tokens, 3.1 for each token of
shell, after its two reviews): the machine and its plans, the watch of a run on the runners, the runner, the lock, the
interpreter of the effects and the verbs. The growth was accepted at the review of #571, and the
ratchet raised by the merged tree's figure.

**The port ended at its third stage** (decided at the review of #585). The third clause cannot
hold: three stages measured about 3.1 tokens of Rust for each token of shell they replaced, the
four together could not leave the reading set smaller, and T4 projected +26,500 more for the last
658 lines. So T4 and the gate's port (G1 to G3) do not happen. `build`, `cut_release`, `land`,
`record_release`, `record_done`, `sync` and `nightly-report` stay in `train.sh`, a sequence of
tool calls whose decisions (the table, the publication, the gate runs, the lock) are the tool's
verbs, as section 6 says a script may stay. What T4 was to carry that stands alone landed in the
shell instead: the build sets `context.toml`'s `reading` from the merged tree, and a pull request
declares its growth on a `Reading-growth:` line (#585).

Left from the third stage's reviews, worth doing in the tool by itself one day, each small:

- the calls to the network that no signal interrupts: the sweep and the delete of gate branches,
  and `check`'s reads and its comment, bounded in time and run under the supervisor;
- the law of a green end bound to the reaping loop itself (`reap_group` as a fold from its start),
  not only to the fold of `Status::reaped`;
- the kill after the grace held by a law for a leader's exit as for a stop, with a witness that
  leaves a process ignoring TERM in a gate's group.

Revisit Python only if `nopython` is revised. Move the gate's Rust steps to Nix only when a
derivation per workspace member keeps a CLI-only change near today's time on the runners; the
typed gate's step runner keeps a `Runner::Derivation(attr)` arm for that.
