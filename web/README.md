# web

The app's frontend: TypeScript, one esbuild bundle, no framework and no npm. The box serves it,
and the Android portal shows it. Its laws are its tests; `docs/WEB-SPEC.md` holds its doors and
the laws no test can check, and `docs/TS-STYLE.md` its layout rules.

## Use

- `scripts/build-web.sh` writes `web/dist/`; `suzatary serve` serves it. `static/index.html`
  loads `src/boot.ts`, which loads `src/main.ts`; `src/sw.ts` is the service worker, which keeps
  the shell and nothing else.
- The page talks to the box only through the routes `src/wire/api.ts` names; the wire types and
  their decoders are generated from `app/api/src/wire.rs` by `scripts/wire.sh`.

## Depends on

- `app/core`: the generated wire types (`src/wire/generated/`).
- `app/wasm`: the evaluator the tab runs (`prodrome_bg.wasm`), the same core as the box.
- `app/pages` and `app/viewtree`: the pages compiled to wasm (`views_bg.wasm`) and the patches
  `src/painter` applies.
- `prodrome-typst-wasm` (`typst_bg.wasm`) and the design system `typst-design`.

## Develop

`scripts/web-test.sh` runs the unit tests (`test/`), `scripts/web-e2e.sh <scene>` the browser
scenes (`e2e/`), and `scripts/web-static.sh` the static checks.
