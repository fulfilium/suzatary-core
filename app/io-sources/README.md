# suzatary-io-sources

The interpreters of Drive, the GitHub mirrors, the web, the weather and push (FCM and ntfy), and
the briefing files. It makes no decisions and writes no log: a refusal comes back as a typed
value. Below it are `suzatary-os` (files), `suzatary-io-access` (processes, HTTP and secrets),
`suzatary-io-local` (the box's own stores) and `suzatary-io-mail` (the mail transports).

## Depends on

`suzatary-os`, `suzatary-io-access`, `suzatary-io-local`, `suzatary-io-mail`, the model crates
below `suzatary-api`.

## Develop

`cargo test -p suzatary-io-sources`. A test that runs a fake program uses `suzatary-testkit`.
