# suzatary (the binary)

The one binary, `suzatary`, organised by verb: `src/commands/<verb>.rs`. `src/verb.rs` is the
argument tree and the one `match` that runs a verb. It connects the core's decisions to the
interpreters of `suzatary-io` and serves the web app (`suzatary serve`).

## Use

- Run `suzatary --help` for the verbs. Scripts run the deployed program through
  `scripts/app-bin.sh <verb>`; `scripts/app-build.sh` builds a dev binary.
- The library (`suzatary_cli`) exists so that integration tests can drive the verbs and the
  router: `commands::serve` is the HTTP surface, `instance::Instance` the parsed
  `instance.toml`.
- `suzatary context` prints the context budget of each unit (`context.toml`).

## Depends on

`suzatary-core`, `suzatary-io`, `suzatary-plugin` and the Prodrome.

## Develop

`cargo test -p suzatary --test <name>`; the guards are `--test guards`. The merge gate is
`scripts/merge-gate.sh`.
