#!/usr/bin/env bash
# The web app's unit tests: esbuild the TypeScript, run it with node's test runner.
#
# web/test/wasm.test.ts loads the same .wasm the browser fetches (via `--target nodejs` glue
# from the same derivation) and drives it against the conformance vectors and the live chain:
# hashes and knot instants exactly, fulfillments to 1e-9. `cargo test` cannot cover the glue,
# wasm-opt's rewrite or core.ts's decoders. `tsc` runs in scripts/build-web.sh.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

. "$root/scripts/web-inputs.sh"

# Fails if web/static/tokens.css or web/src/view/tokens.ts has drifted from tokens.typ.
node "$root/scripts/gen-tokens.mjs" --check

out="$(mktemp -d)"
trap 'rm -rf "$out"' EXIT

node "$root/scripts/typst-packages.mjs" > "$out/packages.json"
export SUZATARY_TYPST_PACKAGES="$out/packages.json"

# Without `.#suzatary-vector-json`'s output, run the same two `cargo run`s that derivation runs.
if [[ -z "${SUZATARY_PRODROME_CONFORMANCE:-}" ]]; then
  echo "==> generating the conformance vectors (cargo run --example vector_json; no SUZATARY_PRODROME_CONFORMANCE supplied)"
  prodrome="$(cargo metadata --manifest-path app/Cargo.toml --format-version 1 | node -e '
    const path = require("path")
    const core = JSON.parse(require("fs").readFileSync(0, "utf8")).packages.find(p => p.name === "prodrome-core")
    console.log(path.dirname(path.dirname(core.manifest_path)))')"
  vectors() { cargo run -q --manifest-path app/Cargo.toml -p suzatary-core --example vector_json -- "$@"; }
  mkdir "$out/vectors"
  vectors fpl "$prodrome/conformance/fpl.py" "$prodrome/wasm/conformance/term-json.json" > "$out/vectors/fpl.json"
  vectors series "$prodrome/conformance/series.py" > "$out/vectors/series.json"
  export SUZATARY_PRODROME_CONFORMANCE="$out/vectors"
fi

# A clone of the code alone has no instance, and wasm.test.ts skips its live-chain tests.
instance="${SUZATARY_INSTANCE:-$root/instance.toml}"
if [[ -f "$instance" ]]; then
  data="$(dirname "$(realpath "$instance")")"
  export SUZATARY_LIVE_EVENTS="$data/events"
fi

# `--external:./prodrome.js`: the Wasm glue is a build output core.ts imports lazily, never
# called under node, so marking it external lets a test import `core.ts` at all.
entries=(web/test/*.test.ts)
[[ -z "${SUZATARY_GATE_UNITS:-}" ]] || read -ra entries <<<"$SUZATARY_GATE_UNITS"
esbuild "${entries[@]}" \
  --bundle \
  --platform=node \
  --format=esm \
  --target=es2022 \
  --define:__DEV__=false \
  --external:./prodrome.js \
  --external:./typst.js \
  --out-extension:.js=.mjs \
  --outdir="$out"

# Files named explicitly: `node --test <dir>` loads the directory itself as a module (node 24).
# Several zones, so a test or module that reads the device's zone fails here.
for zone in UTC America/New_York Europe/Berlin Pacific/Kiritimati; do
  echo "==> the web unit suite with TZ=$zone"
  TZ="$zone" node --test "$out"/*.test.mjs
done
