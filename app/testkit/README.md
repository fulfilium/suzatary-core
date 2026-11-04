# suzatary-testkit

Programs that a test writes and then runs, without `ETXTBSY`: the test's own process never runs
a file it opened for writing. It is a dev-dependency of `suzatary-io` and `suzatary` alone.

## Use

- `fake_program` links a program's name to the committed wrapper `fake-program`, which reads the
  program's text from a data file.
- `write_program` and `copy_program` make a regular file at the path, written by a child process.

## Depends on

Nothing of this repository.
