# suzatary-io

The interpreters: each capability trait of the model crates, run against the world (IMAP and
SMTP, notmuch, Drive, GitHub, the weather and push). It makes no decisions and writes no log: a
refusal comes back as a typed value. Below it are `suzatary-os` (files), `suzatary-io-access`
(processes, HTTP and secrets), `suzatary-io-local` (the box's own stores) and
`suzatary-io-chains` (the chains on disk); beside it are `suzatary-io-typeset` (Typst on the box)
and `suzatary-io-model` (the `claude` CLI).

## Use

- `smtp` is the only path that sends mail, and only the binary's `send` and the tap reach it.

## Depends on

`suzatary-os`, `suzatary-io-access`, `suzatary-io-local`, `suzatary-io-chains`, the model crates
below `suzatary-api` and the Prodrome.

## Develop

`cargo test -p suzatary-io`. A test that runs a fake program uses `suzatary-testkit`; a fixture
that talks to notmuch sets its own `NOTMUCH_CONFIG` under a scratch directory.
