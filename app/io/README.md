# suzatary-io

The interpreters: each capability trait of the model crates, run against the world (chains on
disk, IMAP and SMTP, the `claude` CLI, notmuch). It makes no decisions and writes no log: a
refusal comes back as a typed value. Below it are `suzatary-os` (files), `suzatary-io-access`
(processes, HTTP and secrets) and `suzatary-io-local` (the box's own stores); Typst on the box
is `suzatary-io-typeset`, beside it.

## Use

- `objects::Objects` is a chain's object store on disk; `reading` reads a chain under a policy.
- `model::ClaudeCli` is the model port, fenced by a settings file (`model::Fence`).
- `smtp` is the only path that sends mail, and only the binary's `send` and the tap reach it.

## Depends on

`suzatary-os`, `suzatary-io-access`, `suzatary-io-local`, the model crates below `suzatary-api`
and the Prodrome.

## Develop

`cargo test -p suzatary-io`. A test that runs a fake program uses `suzatary-testkit`; a fixture
that talks to notmuch sets its own `NOTMUCH_CONFIG` under a scratch directory.
