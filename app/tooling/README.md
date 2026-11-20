# suzatary-tooling

The merge train's typed program, the binary `train`. `scripts/train.sh` builds it from its own
checkout before every verb, and calls it for each verb that has moved out of shell: the verdict
table under `.git/train/gates`, and publication to the public repository (the remote `core`).

## Use

- `train table read <commit> [--half tree|live] [--full]` prints one word: `ok`, `failed`,
  `unknown` or `finding`. With `--half`, the word is that half's `Reading` of the commit's tree.
  Without it, the word is the train's verdict, the half that weighs more in the `LandOrder`
  (ok < unknown < failed < finding). With `--full`, the rows of full gates alone count.
- `train table write <half> <commit> <status> [--full]` appends the row of a half's run that the
  gate ended with `<status>`, and exits 0 for green, 1 for red, and 2 with no row for 75 ("did
  not start").
- `train publish [--skip <sha>]... [--admit <path>]... [<rev>]`: core's master gains the public
  tree of each landed commit after the newest it publishes, through `<rev>` (origin's master),
  oldest first, one row each (`Source:`, `Skipped:`, `Admitted:` trailers). `--admit` is taken
  only when stdin is a terminal. Exits 0 when published or already published, 1 when refused or
  stopped by a leak or a held path.
- `train publishable <commit>` and `train push-gate <commit> <branch> <subject> <trailer>`: the
  one check before a gate branch, and the push after it; the pushed commit on stdout. Both exit 0
  publishable, 1 a leak, 3 the check could not run, 4 held (a new admission no row holds).
- `train published <rev>` prints the row of core's master whose `Source:` is `<rev>`, or nothing.
- Every verb exits 64 on a usage error and 3 when a step it needs could not run. A row that is
  not `<ok|failed> <commit> <instant>` is an error that names its file and line.
- The `seams` feature reads the guards' `SUZATARY_TRAIN_GATE`, `SUZATARY_TRAIN_PUBLIC` and
  `SUZATARY_TRAIN_BIN`. The train's own build has no `seams`: its leak guard's matcher is the
  deployed app (`Matcher::deployed`), and its driver the checkout's `scripts/public-tree.sh`.

## Depends on

No member of the workspace (law 9), and `thiserror`. It runs `git`, `scripts/public-tree.sh`,
`scripts/app-bin.sh --deployed` and the deployed `suzatary leaks` as subprocesses.

## Develop

`cargo test -p suzatary-tooling` runs its laws (`tests/laws.rs`, `tests/publication.rs`) and
the `compile_fail` doc tests of `admitted.rs`. The guards in `app/cli/tests/guards/train.rs`
run `scripts/train.sh`, and so this tool, against stubs.
