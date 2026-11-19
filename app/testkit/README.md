# suzatary-testkit

Programs that a test writes and then runs, without `ETXTBSY`: the test's own process never runs
a file it opened for writing. And the scratch directories a test works in.

## Use

- `fake_program` links a program's name to the committed wrapper `fake-program`, which reads the
  program's text from a data file.
- `write_program` and `copy_program` make a regular file at the path, written by a child process.
- `scratch(name)` is a `TempDir`, removed when it drops; `this_test()` is the running test's
  directory, removed when its thread ends. `SUZATARY_KEEP_SCRATCH=1` keeps both.

## Depends on

`tempfile`, and nothing of this repository.
