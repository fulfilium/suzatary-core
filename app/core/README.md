# suzatary-core

The application's meaning, as pure Rust: no I/O, no async and no clock of its own. Every other
crate builds on it, and the browser runs it through `app/wasm`.

## Use

- Each effect is a trait beside its domain (`Render`, the chain, the model); the base ones
  (`Clock`, `Entropy`) are `suzatary_base::capability`'s. A caller passes a record of them,
  such as `agent::Agent`, which has no field that sends mail.
- `record` holds the todo record, its `mk_*` constructors and the two source languages,
  `MarkupSource` and `StringSource`.
- `chain_set` reads every chain once; its entries are priced by the Prodrome's
  `view::entries`, the one reading the server, the CLI and the browser share.
- `store` holds the generic store shapes (`Prodrome<E>`, `Log<E>`, `Doc<K, V>`); a new store is
  one row of `stores!` in `store/kind.rs`.
- `secretary` holds the secretary's proposals, cards, phases and conversations.
- `wire` declares each cut's payloads; `suzatary_api::wire` holds the rest and the generator.
- Errors are typed with `thiserror`; a fallible constructor is the only way to make a checked
  value (`SessionId::new`, `MarkupSource::checked`).

## Depends on

`suzatary-base`, and the Prodrome (`prodrome_core`), the database; `prodrome-wasm-exports` only under the `wasm`
feature.

## Develop

`cargo test -p suzatary-core`. The laws are in `tests/` (proptest), and the vectors the browser
checks are written to `tests/web/`.
