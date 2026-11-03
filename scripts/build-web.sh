#!/usr/bin/env bash
# Build the web app: one esbuild bundle, no npm, no lockfile.
#
# The toolchain is the flake's (`nix develop .#triage`): esbuild, tsc and
# node. Nothing here reaches the network, and web/dist/ is git-ignored — the
# repository releases the program, and the bundle is its output, exactly like
# the document (CLAUDE.md, Operations).
#
#   scripts/build-web.sh          build web/dist/
#   scripts/build-web.sh --dev    …with __DEV__ true (the timing lines in
#                                 web/src/app/perf.ts), for a `suzatary serve`
#                                 over web/dist: the page loads its files by
#                                 build (`/b/<build>/`), so no static server
#                                 serves the app
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

# The static checks, the modules and the wire's TypeScript and decoders, BEFORE tsc, which checks
# every decoder against its type: the inputs scripts/web-test.sh reads too, prepared once.
. "$root/scripts/web-inputs.sh"

echo "==> tsc --noEmit -p web"
tsc --noEmit -p web

# `__DEV__` is a LITERAL by the time anything runs, so a production bundle does
# not carry the measuring strings at all rather than carrying a flag it checks.
dev=false
if [[ "${1:-}" == "--dev" ]]; then dev=true; fi

echo "==> esbuild web/src/main.ts -> web/dist/app.js and its chunks (__DEV__=$dev)"
rm -rf web/dist
mkdir -p web/dist
meta="$(mktemp -d)"
trap 'rm -rf "$meta"' EXIT
# `--external:./prodrome.js` keeps the Wasm glue OUT of the bundle on purpose.
# It is a build output (`$SUZATARY_PRODROME_WASM`, copied beside app.js below),
# so it does not exist when tsc and esbuild run; and it must stay a separate
# module so `web/src/prodrome/core.ts` can `import()` it LAZILY — the .wasm is a few
# hundred kilobytes and most visits never open a page that needs the core.
#
# SPLIT BY PAGE AND MINIFIED. Each page `web/src/pages/table.ts` imports is a
# chunk of its own, and code two of them share is a chunk too, each named by
# its content and written beside app.js, so every relative URL the app
# resolves against its own (`./prodrome.js`, `./typst/kit.typ`) is one
# directory for all of them, and each is served under its build's name as
# app.js is (`serve::versioned`).
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

# What the metafile names, as JSON arrays for the two bundles below: every
# chunk of this build, which the worker keeps with its shell, and the chunks
# app.js imports as it loads, which the loader asks for beside it.
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

# THE LOADER (web/src/boot.ts), the one script index.html names: it picks a
# build and imports that build's app.js under its name (`b/<build>/`), asking
# for the build's stylesheets and the chunks app.js imports beside it; the app
# draws the body and links its own stylesheets, and every file it asks for
# after, resolved against its own URL, is that build's.
esbuild web/src/boot.ts \
  --bundle \
  --minify \
  --format=esm \
  --target=es2022 \
  --define:__PRELOAD__="$preload" \
  --outfile=web/dist/boot.js

# THE EDITOR'S TYPST WORKER (web/src/typst/worker.ts), its own module bundle
# beside app.js: the page terminates it when a compile outlives its time, so
# a text that loops is refused and never hangs the tab. It loads the same
# typst.js and typst_bg.wasm the Typst views do.
esbuild web/src/typst/worker.ts \
  --bundle \
  --minify \
  --format=esm \
  --target=es2022 \
  --external:./typst.js \
  --outfile=web/dist/typstwork.js

# The core's worker: the Prodrome module in a thread of its own, answering the pages' questions about the chains.
esbuild web/src/prodrome/worker.ts \
  --bundle \
  --minify \
  --format=esm \
  --target=es2022 \
  --external:./prodrome.js \
  --outfile=web/dist/prodromework.js

# THE APP SHELL'S WORKER (web/src/sw.ts), its own bundle beside app.js so its
# scope is the app's. An IIFE, because a worker registered without
# `type: "module"` runs a classic script. It keeps each build's files under a
# cache named by that build, its chunks included, answers `b/current` as it
# last heard it and asks the box again behind the answer, and never touches
# the API.
esbuild web/src/sw.ts \
  --bundle \
  --minify \
  --format=iife \
  --target=es2022 \
  --define:__CHUNKS__="$chunks" \
  --outfile=web/dist/sw.js

echo "==> static files"
cp web/static/index.html web/static/tokens.css web/static/style.css web/static/manifest.webmanifest web/static/icon.svg web/static/icon-192.png web/static/icon-512.png web/dist/

# THE SITE VOUCHES FOR THE APP. .well-known/assetlinks.json is how Android
# decides that com.bmabsout.suzatary may open this host's links itself instead
# of handing them to a browser: the OS fetches it over https from the host in
# the manifest's intent filter and checks that the installed APK's signing
# certificate is one it names. It is a DIRECTORY copy rather than another
# filename on the line above because the path is fixed by the platform — the
# file must be served at exactly /.well-known/assetlinks.json — and anything
# else Android or a certificate authority ever asks for at that path belongs
# beside it, copied by the same line.
cp -r web/static/.well-known web/dist/

# The two faces, vendored (SIL OFL, web/static/fonts/OFL.txt): nothing is
# fetched from a font service at runtime.
cp -r web/static/fonts web/dist/

# THE CORE, FOR THE BROWSER. `suzatary-wasm` is `prodrome-core` compiled to
# WebAssembly at this application's schemas (`app/wasm`), so the chain page's
# verification, the thirty-day marks and the offline fold all run the SAME
# evaluator the box does instead of a second reading of the same spec. Copied,
# never compiled here: this script is a bundler and Nix is the build
# (`nix build .#suzatary-wasm`), from the revision `app/Cargo.toml` links the
# crate at, so the tab and the binary are one core.
echo "==> suzatary-wasm from $SUZATARY_PRODROME_WASM"
cp "$SUZATARY_PRODROME_WASM/web/prodrome.js" "$SUZATARY_PRODROME_WASM/web/prodrome_bg.wasm" web/dist/
chmod u+w web/dist/prodrome.js web/dist/prodrome_bg.wasm
echo "    prodrome_bg.wasm: $(du -h web/dist/prodrome_bg.wasm | cut -f1)"

# THE PAGES' MODULE (`app/pages`), copied the same way and loaded by the page drawn from it.
echo "==> suzatary-views-wasm from $SUZATARY_VIEWS_WASM"
cp "$SUZATARY_VIEWS_WASM/web/views.js" "$SUZATARY_VIEWS_WASM/web/views_bg.wasm" web/dist/
chmod u+w web/dist/views.js web/dist/views_bg.wasm
sed 's/^/    /' "$SUZATARY_VIEWS_WASM/SIZES"

# TYPST FOR THE TAB, copied the same way and loaded the same way: lazily, by
# `web/src/typst/compiler.ts`, the first time a Typst view is drawn.
echo "==> typst-wasm from $SUZATARY_TYPST_WASM"
cp "$SUZATARY_TYPST_WASM/web/typst.js" "$SUZATARY_TYPST_WASM/web/typst_bg.wasm" web/dist/
chmod u+w web/dist/typst.js web/dist/typst_bg.wasm
sed 's/^/    /' "$SUZATARY_TYPST_WASM/SIZES"

# THE KIT AND ITS VIEWS ARE DATA, fetched beside the module: a view is a file the
# tab compiles, so a new page is a new file. Beside them, every package on
# TYPST_PACKAGE_PATH (the owner's design system), the files the box's typst
# reads, so the tab and the box compile against one package.
mkdir -p web/dist/typst
cp web/typst/*.typ web/dist/typst/
node scripts/typst-packages.mjs > web/dist/typst/packages.json

# COMPRESSED WHEN BUILT, NEVER PER REQUEST. `suzatary serve` hands a `.br`
# to a request that accepts brotli and compresses no frontend file on the fly
# (`commands/serve`), so every file worth compressing gets its `.br` here, at
# quality 11, which no per-request compressor can afford: typst-wasm's is the
# Prodrome's own, and the core, the bundle and the rest are brotlied below.
# Fonts and images are compressed formats already; a file under 1 KiB, or one
# brotli does not shrink, keeps no `.br`.
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

echo "built: $(du -sh web/dist | cut -f1) in web/dist/"
ls -1 web/dist
