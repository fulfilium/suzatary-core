# suzatary-io

The interpreters: each capability trait of the model crates, run against the world (files and
processes through `suzatary-os`, IMAP and SMTP, HTTP, the `claude` CLI, notmuch). It makes no decisions and writes no
log: a refusal comes back as a typed value.

## Use

- `objects::Objects` is a chain's object store on disk; `reading` reads a chain under a policy.
- `model::ClaudeCli` is the model port, fenced by a settings file (`model::Fence`).
- `smtp` is the only path that sends mail, and only the binary's `send` and the tap reach it.

## Depends on

`suzatary-os`, the model crates below `suzatary-api`, the Prodrome and `prodrome-typst-wasm` (Typst compiled in the closed world).

## Develop

`cargo test -p suzatary-io`. A test that runs a fake program uses `suzatary-testkit`; a fixture
that talks to notmuch sets its own `NOTMUCH_CONFIG` under a scratch directory.
