# suzatary-io

The interpreters: each capability trait of the model crates, run against the world (Drive,
GitHub, the web, the weather and push). It makes no decisions and writes no log: a refusal comes
back as a typed value. Below it are `suzatary-os` (files), `suzatary-io-access` (processes, HTTP
and secrets), `suzatary-io-local` (the box's own stores), `suzatary-io-chains` (the chains on
disk) and `suzatary-io-mail` (the mail transports); beside it are `suzatary-io-typeset` (Typst on
the box) and `suzatary-io-model` (the `claude` CLI).

## Depends on

`suzatary-os`, `suzatary-io-access`, `suzatary-io-local`, `suzatary-io-mail`, the model crates
below `suzatary-api`.

## Develop

`cargo test -p suzatary-io`. A test that runs a fake program uses `suzatary-testkit`.
