# suzatary-tooling

The merge train's typed program, the binary `train`. `scripts/train.sh` builds it from its own
checkout before it does anything else, and calls it for each verb that has moved out of shell.
Today that is the verdict table under `.git/train/gates`.

## Use

- `train table read <commit> [--half tree|live] [--full]` prints one word: `ok`, `failed`,
  `unknown` or `finding`. With `--half`, the word is that half's `Reading` of the commit's tree.
  Without it, the word is the train's verdict, the half that weighs more in the `LandOrder`
  (ok < unknown < failed < finding). With `--full`, the rows of full gates alone count.
- `train table write <half> <commit> <status> [--full]` appends the row of a half's run that the
  gate ended with `<status>`, and exits 0 for green, 1 for red, and 2 with no row for 75 ("did
  not start").
- The tool exits 64 on a usage error and 3 when the table or git cannot be read. A row that is
  not `<ok|failed> <commit> <instant>` is an error that names its file and line.
- The `seams` feature reads the guards' `SUZATARY_TRAIN_GATE` and keys a stub gate's rows by its
  bytes. The train's own build has no `seams`.

## Depends on

No member of the workspace (law 9), and `thiserror`. It runs `git` as a subprocess.

## Develop

`cargo test -p suzatary-tooling` runs its laws (`tests/laws.rs`). The guards in
`app/cli/tests/guards/train.rs` run `scripts/train.sh`, and so this tool, against stubs.
