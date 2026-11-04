# suzatary-plugin

The plugin host. It runs a WebAssembly component of `app/wit/plugin.wit`'s `source` world with
its grant.

## Use

- `Host::new` holds one wasmtime engine and linker; `Host::load` loads a component and refuses
  one that imports a WASI filesystem or sockets.
- Each call runs in a fresh store under a `Budget` of time and memory. `fetch` crosses
  `suzatary_core::plugin::Target::admit`, and a credential is resolved by name inside the host.
- `Run` and `Synced` describe a call's result; `RunError` says why a call stopped.

## Depends on

`suzatary-core`, `suzatary-io` and the Prodrome.

## Develop

`cargo test -p suzatary-plugin`; `tests/fence.rs` runs `app/plugin-fence` against every fence.
