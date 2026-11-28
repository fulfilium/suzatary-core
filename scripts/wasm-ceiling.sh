#!/usr/bin/env bash
# The shipped wasm modules, as the box serves them (the `.br` beside each, else the module itself),
# against one ceiling that stands for a phone's first load: 180,000 bytes a second (Lighthouse's
# Slow 4G, 1.6 Mbit/s, times the 0.9 the cold-load bench applies) for 120 seconds. The modules are
# cached by the service worker after that one load, and the typst module is fetched only when a
# page compiles. Exit 1 naming the total when it is over.
#
#   scripts/wasm-ceiling.sh <dist>
set -euo pipefail

(($# == 1)) || { echo "usage: scripts/wasm-ceiling.sh <dist>" >&2; exit 64; }
bytes_per_second=180000
seconds=120
ceiling=$((bytes_per_second * seconds))
total=0
for wasm in "$1"/*.wasm; do
  shipped="$wasm"
  [[ ! -e "$wasm.br" ]] || shipped="$wasm.br"
  size=$(stat -c %s "$shipped")
  echo "    $(basename "$wasm"): $size bytes shipped"
  total=$((total + size))
done
if ((total > ceiling)); then
  echo "wasm-ceiling.sh: the modules ship $total bytes, over the $ceiling a phone loads in $seconds s at $bytes_per_second bytes a second: make the modules smaller" >&2
  exit 1
fi
echo "    total $total bytes, ceiling $ceiling"
