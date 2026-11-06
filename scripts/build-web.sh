#!/usr/bin/env bash
# Build the web app into web/dist/ (git-ignored) with the flake's esbuild, tsc and node; no npm,
# no network.
#
#   scripts/build-web.sh          build web/dist/
#   scripts/build-web.sh --dev    …with __DEV__ true (web/src/app/perf.ts), for `suzatary serve`
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

# Before tsc, which checks the generated decoders against their types.
. "$root/scripts/web-inputs.sh"

echo "==> tsc --noEmit -p web"
tsc --noEmit -p web

# `__DEV__` is a literal, so a production bundle drops the measuring code entirely.
dev=false
if [[ "${1:-}" == "--dev" ]]; then dev=true; fi

echo "==> esbuild web/src/main.ts -> web/dist/app.js and its chunks (__DEV__=$dev)"
rm -rf web/dist
mkdir -p web/dist
meta="$(mktemp -d)"
trap 'rm -rf "$meta"' EXIT
# The wasm glue stays external: it is copied in below, after esbuild runs, and is imported lazily.
# Chunks are written beside app.js, so every relative URL the app resolves (`./prodrome.js`,
# `./typst/kit.typ`) is one directory for all of them.
esbuild app=web/src/main.ts \
  --bundle \
  --splitting \
  --minify \
  --format=esm \
  --target=es2022 \
  --sourcemap \
  --define:__DEV__="$dev" \
  --external:./prodrome.js \
  --external:./typst.js \
  --external:./views.js \
  --chunk-names='[name]-[hash]' \
  --metafile="$meta/app.json" \
  --outdir=web/dist

# JSON arrays: every chunk (the service worker keeps them), and the chunks app.js imports on load
# (the loader preloads them).
chunks="$(node -e '
  const out = JSON.parse(require("fs").readFileSync(process.argv[1], "utf8")).outputs
  const base = p => p.replace(/^web\/dist\//, "")
  const chunks = Object.keys(out).filter(p => p.endsWith(".js") && base(p) !== "app.js").map(base).sort()
  // The worker knows a chunk of any build by its shape alone (`shell.chunk`), so a name of another shape fails here.
  const odd = chunks.filter(c => !/^[a-z][a-z0-9_]*-[A-Z0-9]{8}\.js$/.test(c))
  if (odd.length > 0) { console.error(`build-web.sh: chunks not of the shape the worker routes: ${odd.join(", ")}`); process.exit(1) }
  console.log(JSON.stringify(chunks))
' "$meta/app.json")"
preload="$(node -e '
  const out = JSON.parse(require("fs").readFileSync(process.argv[1], "utf8")).outputs
  const base = p => p.replace(/^web\/dist\//, "")
  console.log(JSON.stringify(out["web/dist/app.js"].imports.filter(i => i.kind === "import-statement" && !i.external).map(i => base(i.path)).sort()))
' "$meta/app.json")"
echo "    chunks: $(node -e 'console.log(JSON.parse(process.argv[1]).length)' "$chunks"), $(node -e 'console.log(JSON.parse(process.argv[1]).length)' "$preload") imported as app.js loads"

# The loader, the one script index.html names: it picks a build and imports its app.js from
# `b/<build>/`.
esbuild web/src/boot.ts \
  --bundle \
  --minify \
  --format=esm \
  --target=es2022 \
  --define:__PRELOAD__="$preload" \
  --outfile=web/dist/boot.js

# The editor's typst worker: the page terminates it when a compile runs too long, so a looping
# text never hangs the tab.
esbuild web/src/typst/worker.ts \
  --bundle \
  --minify \
  --format=esm \
  --target=es2022 \
  --external:./typst.js \
  --outfile=web/dist/typstwork.js

esbuild web/src/prodrome/worker.ts \
  --bundle \
  --minify \
  --format=esm \
  --target=es2022 \
  --external:./prodrome.js \
  --outfile=web/dist/prodromework.js

# The pages' rehearsal: runs each page's first readings' code once on the page's own compiled
# module, so the browser compiles it off the page's thread.
esbuild web/src/pages/rehearse.ts \
  --bundle \
  --minify \
  --format=esm \
  --target=es2022 \
  --external:./views.js \
  --outfile=web/dist/pagework.js

# The service worker, beside app.js so its scope is the app's. An IIFE: a worker registered
# without `type: "module"` runs a classic script.
esbuild web/src/sw.ts \
  --bundle \
  --minify \
  --format=iife \
  --target=es2022 \
  --define:__CHUNKS__="$chunks" \
  --outfile=web/dist/sw.js

echo "==> static files"
cp web/static/index.html web/static/tokens.css web/static/style.css web/static/manifest.webmanifest web/static/icon.svg web/static/icon-192.png web/static/icon-512.png web/dist/

# Android fetches /.well-known/assetlinks.json over https to let the APK open this host's links;
# the platform fixes that path.
cp -r web/static/.well-known web/dist/

# Vendored fonts (SIL OFL): nothing is fetched from a font service at runtime.
cp -r web/static/fonts web/dist/

# The core for the browser (`app/wasm`), the same evaluator the box runs; built by Nix, only
# copied here.
echo "==> suzatary-wasm from $SUZATARY_PRODROME_WASM"
cp "$SUZATARY_PRODROME_WASM/web/prodrome.js" "$SUZATARY_PRODROME_WASM/web/prodrome_bg.wasm" web/dist/
chmod u+w web/dist/prodrome.js web/dist/prodrome_bg.wasm
echo "    prodrome_bg.wasm: $(du -h web/dist/prodrome_bg.wasm | cut -f1)"

echo "==> suzatary-views-wasm from $SUZATARY_VIEWS_WASM"
cp "$SUZATARY_VIEWS_WASM/web/views.js" "$SUZATARY_VIEWS_WASM/web/views_bg.wasm" web/dist/
chmod u+w web/dist/views.js web/dist/views_bg.wasm
sed 's/^/    /' "$SUZATARY_VIEWS_WASM/SIZES"

echo "==> typst-wasm from $SUZATARY_TYPST_WASM"
cp "$SUZATARY_TYPST_WASM/web/typst.js" "$SUZATARY_TYPST_WASM/web/typst_bg.wasm" web/dist/
chmod u+w web/dist/typst.js web/dist/typst_bg.wasm
sed 's/^/    /' "$SUZATARY_TYPST_WASM/SIZES"

# The typst kit and views are data the tab compiles, with every package on TYPST_PACKAGE_PATH,
# so the tab and the box compile against one package.
mkdir -p web/dist/typst
cp web/typst/*.typ web/dist/typst/
node scripts/typst-packages.mjs > web/dist/typst/packages.json

# `suzatary serve` never compresses on the fly; it serves these `.br` files. typst-wasm brings
# its own. A file under 1 KiB, or one brotli does not shrink, keeps no `.br`.
echo "==> brotli -q 11 beside every compressible file"
cp "$SUZATARY_TYPST_WASM/web/typst_bg.wasm.br" web/dist/
chmod u+w web/dist/typst_bg.wasm.br
find web/dist -type f -size +1k ! -name typst_bg.wasm \
  \( -name '*.js' -o -name '*.map' -o -name '*.wasm' -o -name '*.css' -o -name '*.html' \
  -o -name '*.typ' -o -name '*.json' -o -name '*.svg' -o -name '*.webmanifest' \) -print0 |
  xargs -0 -P "$(nproc)" -n 1 brotli -q 11 -k -f
for br in web/dist/*.br web/dist/*/*.br; do
  [[ -e "$br" ]] || continue
  if [[ $(stat -c %s "$br") -ge $(stat -c %s "${br%.br}") ]]; then rm "$br"; fi
done
for f in typst_bg.wasm prodrome_bg.wasm app.js; do
  echo "    $f: $(stat -c %s "web/dist/$f") bytes raw, $(stat -c %s "web/dist/$f.br") bytes brotli"
done

# What a phone downloads on a deploy's first load: each shipped wasm module's brotli bytes against
# its line of web/wasm-budget for the system that built it (stdenv's `system`, set in a derivation
# and in `nix develop` alike), which only moves down.
echo "==> every wasm module within its brotli budget (web/wasm-budget, ${system:?build-web.sh: no Nix system})"
over=0
for wasm in web/dist/*.wasm; do
  name=$(basename "$wasm")
  budget=$(grep -v '^#' web/wasm-budget | awk -v n="$name" -v s="$system" '$1 == n && $2 == s { print $3 }')
  size=$(stat -c %s "$wasm.br")
  if [[ -z "$budget" ]]; then
    echo "build-web.sh: $name has no line for $system in web/wasm-budget" >&2
    over=1
  elif ((size > budget)); then
    echo "build-web.sh: $name is $size bytes brotli, over its $system budget of $budget (web/wasm-budget)" >&2
    over=1
  else
    echo "    $name: $size of $budget bytes brotli"
  fi
done
((over == 0)) || exit 1

echo "built: $(du -sh web/dist | cut -f1) in web/dist/"
ls -1 web/dist
