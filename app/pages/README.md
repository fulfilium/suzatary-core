# suzatary-pages

The app's pages in the view-tree shape: each one is a `viewtree::Program` over the runtime's
sums (`runtime`), compiled to wasm for the tab, where `web/src/painter` paints its patches.

## Use

- `lib.rs` exports each page with `viewtree::export!`.
- `runtime` holds what a host hands a page (`Context`, `Reading`, `Answer`) and what a page asks
  of it (`Effect`); `scripts/wire.sh` writes their TypeScript to `web/src/wire/generated/pages.ts`.
- `places` names the places a page links to.

## Depends on

`suzatary-core` and `viewtree`.

## Develop

`cargo test -p suzatary-pages`; each page's laws are in `tests/laws/<page>.rs`.
