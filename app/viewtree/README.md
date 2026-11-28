# viewtree

A page's view as data, the keyed diff that paints it, and the patches a painter applies. It
knows nothing of Suzatary.

## Use

- Implement `program::Program`: a model, one pure `update` that answers a new model and the
  effects to perform, and a `view` that is a function of the model alone.
- `subscriptions` says which sources the program listens to at a model, as `subs::Subs` of slot to
  source, data with no closure. The runtime keeps what it last sent and each frame carries the
  `subpatch::SubPatch`es from it to the next; a source that is equal is never started again. A
  program that listens to nothing leaves it as it is.
- A view is a tree of `node::Node`s whose handlers are messages, never closures.
- `paint::Painter` keeps the last tree and answers the smallest list of `patch::Patch`es;
  `patch::encode` prints them as the bytes `web/src/painter` applies.
- `viewtree::export!` makes a page class for the browser (the `host` feature).

## Depends on

Nothing of this repository; `serde` and `ts-rs` only with the `host` feature.

## Develop

`cargo test -p viewtree`. The patch format is `src/patch.rs`, and the painter in
`web/src/painter/painter.ts` reads it.
