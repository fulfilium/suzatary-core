# suzatary-tooling

The merge train's typed program, the binary `train`. `scripts/train.sh` builds it from its own
checkout before every verb, and calls it for each verb that has moved out of shell: the verdict
table under `.git/train/gates`, publication to the public repository (the remote `core`), the gate
runs, and the lock.

## Use

- `train table read <commit> [--half tree|live] [--full]` prints one word: `ok`, `failed`,
  `unknown` or `finding`. With `--half`, the word is that half's `Reading` of the commit's tree.
  Without it, the word is the train's verdict, the half that weighs more in the `LandOrder`
  (ok < unknown < failed < finding). With `--full`, the rows of full gates alone count.
- `train table land <commit>` and `train table set-aside <half> <task> <words>` (at a terminal):
  `reading::Judged`.
- `train publish [--skip <sha>]... [--admit <path>]... [<rev>]`: core's master gains the public
  tree of each landed commit after the newest it publishes, through `<rev>` (origin's master),
  oldest first, one row each (`Source:`, `Skipped:`, `Admitted:` trailers). `--admit` is taken
  only when stdin is a terminal. Exits 0 when published or already published, 1 when refused or
  stopped by a leak or a held path.
- `train publishable <commit>` and `train push-gate <commit> <branch> <subject> <trailer>`: the
  one check before a gate branch, and the push after it; the pushed commit on stdout. Both exit 0
  publishable, 1 a leak, 3 the check could not run, 4 held (a new admission no row holds).
- `train published <rev>` prints the row of core's master whose `Source:` is `<rev>`, or nothing.
- `train gate [--full] [--remote github]`, `train bisect [--remote github]`, `train nightly` and
  `train check <pr>` run the merge gate's halves (`gating::Plan`, one for each) and report, as
  `scripts/train.sh` did: the rows go to the table, a red half stops the other (not on a nightly),
  and HUP, INT or TERM stop every half before anything waits, write no row, close the gate branch
  and exit 128 plus the signal. `nightly` waits for a running train's lock; `check` takes none and
  writes no row. Each gate half runs as a process group whose id the train prints. `gate` and
  `nightly` run no pre-check. A live `bisect` names the stores its probes run at.
- `train locked <verb> -- <command>...` runs `scripts/train.sh <verb>` under the lock, and passes
  a signal on to it; another train's lock refuses with exit 1, naming its holder.
- Every verb exits 64 on a usage error and 3 when a step it needs could not run. A line of the
  table that does not parse (`row::Line`) is an error that names its file and line.
- The `seams` feature reads the guards' `SUZATARY_TRAIN_GATE`, `SUZATARY_TRAIN_PUBLIC`,
  `SUZATARY_TRAIN_BIN`, `SUZATARY_TRAIN_GH`, `SUZATARY_TRAIN_POLL` and
  `SUZATARY_TRAIN_CACHE_TIMEOUT`. The train's own build has no `seams`: its leak guard's matcher is the
  deployed app (`Matcher::deployed`), and its driver the checkout's `scripts/public-tree.sh`.

## Depends on

No member of the workspace (law 9); `thiserror`, `rustix` (a process group signalled and reaped)
and `signal-hook` (signals as messages). It runs `git`, `gh`, `scripts/public-tree.sh`,
`scripts/publication-run.sh`, `scripts/app-bin.sh --deployed`, the deployed `suzatary leaks`, the
merge gate and `scripts/train.sh sync` and `nightly-report` as subprocesses.

## How a gate run works

`gating::step` is pure: a `Plan` (for each half a list of pieces, each perhaps waiting for the
other half's piece) is run by messages (a child's group gone, a poll's answer, a timer, a signal)
into effects. `supervisor::Supervisor` is the one runner: every child in a process group of its
own, the train its subreaper, and every event on one channel. `gate_run.rs` does the effects.

## Develop

`cargo test -p suzatary-tooling` runs its laws (`tests/laws.rs`, `tests/publication.rs`,
`tests/gating.rs`) and the `compile_fail` doc tests of `admitted.rs`. The guards in `app/cli/tests/guards/train.rs`
run `scripts/train.sh`, and so this tool, against stubs.
