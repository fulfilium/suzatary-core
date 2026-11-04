#!/usr/bin/env bash
# The web app's unit tests: esbuild the TypeScript into a temp dir, run it with
# node's own test runner.
#
# Only the PURE modules are tested here — verify.ts (hash a literal, walk the
# chain), series.ts (knots to a drawing, linear and only linear) and the
# payload decoders, which scripts/wire.sh generates first. Those are the
# three places the browser does something that can be wrong rather than
# merely ugly. node 24 has crypto.subtle, so the
# hashing under test is the same code the tab runs.
#
# AND THE CORE ITSELF (web/test/wasm.test.ts). node 24 runs
# WebAssembly with no flag, so the SAME .wasm the browser fetches is loaded
# here — through wasm-bindgen's `--target nodejs` glue from the same
# derivation — and driven against the conformance vectors: the seeded ones from
# the Prodrome ($SUZATARY_PRODROME_CONFORMANCE, below), and events/ for the
# live chain. Hashes
# and knot instants exactly, fulfillments to 1e-9. `cargo test` cannot cover that,
# because everything between the Rust and the tab (bindgen's glue, wasm-opt's
# rewrite, core.ts's shapes and decoders) is outside it.
#
# `tsc` is the type gate and runs in scripts/build-web.sh over src AND test;
# this script is the behaviour gate.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

# The static checks first, fast, then the modules and the decoders under test: the inputs
# scripts/build-web.sh reads too, prepared once (scripts/web-inputs.sh).
. "$root/scripts/web-inputs.sh"

# THE ONE TOKEN SOURCE: fails if web/static/tokens.css or
# web/src/view/tokens.ts has drifted from tokens.typ, the roles as samples of
# the owner's design system.
node "$root/scripts/gen-tokens.mjs" --check

out="$(mktemp -d)"
trap 'rm -rf "$out"' EXIT

# The packages the kit imports, as the tab is handed them (web/test/typstview.test.ts).
node "$root/scripts/typst-packages.mjs" > "$out/packages.json"
export SUZATARY_TYPST_PACKAGES="$out/packages.json"

# THE SEEDED VECTORS AS JSON. Without `nix build .#suzatary-vector-json`'s
# output (CI passes it), this runs the same two `cargo run`s that derivation
# runs, over the Prodrome checkout cargo links — the rev app/Cargo.toml pins,
# which is the flake input's — the way scripts/wire.sh generates wire.ts.
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

# THE LIVE CHAIN, WHEN THIS BOX HAS ONE: the data tree is the
# instance file's own directory, found through the checkout's link, and it
# holds the chain. A clone of the code alone has none, and wasm.test.ts skips
# its live-chain tests.
instance="${SUZATARY_INSTANCE:-$root/instance.toml}"
if [[ -f "$instance" ]]; then
  data="$(dirname "$(realpath "$instance")")"
  export SUZATARY_LIVE_EVENTS="$data/events"
fi

# `--external:./prodrome.js` for the same reason scripts/build-web.sh has it:
# the Wasm glue is a build output beside the bundle, not a source module, and
# `web/src/prodrome/core.ts` imports it lazily at runtime. Nothing under node ever calls
# that loader — wasm.test.ts requires the `--target nodejs` glue directly — so
# marking it external is what lets a test import `core.ts` at all.
# The test files, or those the merge gate names (SUZATARY_GATE_UNITS) when only theirs moved.
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

# The files are named explicitly rather than by handing node the directory:
# `node --test <dir>` loaded the directory itself as a module here (node 24).
# THE SUITE IS A FUNCTION OF THE TREE, NOT OF THIS MACHINE'S ZONE: it runs with
# the device set to UTC and to three zones either side of it (one a day from
# UTC, Kiritimati), so a test or a module that reads the device's zone fails here.
for zone in UTC America/New_York Europe/Berlin Pacific/Kiritimati; do
  echo "==> the web unit suite with TZ=$zone"
  TZ="$zone" node --test "$out"/*.test.mjs
done
