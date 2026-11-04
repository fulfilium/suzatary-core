# suzatary-wasm

The browser's module: the Prodrome's exports, instantiated at this application's schemas, with
the evaluator. The tab and the box therefore give the same answers for the same objects.

## Use

- One class per schema (for example `new Todos(objects)`) reads the objects once; each method
  answers one question about them, and `append` writes the Prodrome's own `Change` into them.
- A refusal is a JavaScript exception, never a panic. There is no clock: every instant is an
  argument.
- `web/src/prodrome/` loads it lazily; `scripts/web-inputs.sh` builds it.

## Depends on

`suzatary-core` (with `wasm`), `prodrome_core` and `prodrome-wasm-exports`.

## Develop

Build with `nix build .#suzatary-wasm`; `scripts/web-test.sh` runs `web/test/wasm.test.ts`
against the built module.
