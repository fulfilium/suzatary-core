#!/usr/bin/env bash
# Leaves in app/target the dependencies' units alone, for this action (cargo-target) to keep. Cargo
# rebuilds a workspace member whose files a checkout stamped newer, whatever the target holds, and
# identifies a dependency by what it was built from, never by a file's time; so the members' units
# are bytes no later tree reads, removed by the names cargo gives their files. A name this misses
# is bytes kept, never a wrong answer.
#
#   target-deps.sh      from the repository's root, with cargo and jq on PATH
set -euo pipefail

names="$(cargo metadata --manifest-path app/Cargo.toml --no-deps --offline --format-version 1 |
  jq -r '.packages[] | .name, (.targets[].name | ., gsub("-"; "_"))' | sort -u)"
[[ -n "$names" ]] || { echo "target-deps.sh: cargo metadata named no member" >&2; exit 1; }
for dir in app/target/*/; do
  for name in $names; do
    rm -rf "$dir".fingerprint/"$name"-* "$dir"build/"$name"-* \
      "$dir"deps/"$name"-* "$dir"deps/lib"$name"-* "$dir$name" "$dir$name".d
  done
  rm -rf "$dir"incremental "$dir"examples
done
rm -rf app/target/doc
du -sh app/target
