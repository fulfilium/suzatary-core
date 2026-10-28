#!/usr/bin/env bash
# THE STATIC CHECKS — the ONE spelling of each, called by both
# scripts/build-web.sh (before tsc and the bundle) and
# scripts/web-test.sh (before the unit tests), so the innerHTML door and the
# cast ceiling cannot drift into two definitions that quietly disagree.
#
# Neither check touches web/src/wire/generated/wire.ts or suzatary-wasm —
# they are pure `grep` over web/src, web/test and web/e2e — so this script
# needs nothing from the flake beyond the coreutils/grep the triage shell
# already carries, and costs a few seconds rather than a build.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

# ESCAPING IS A TYPE, NOT A HABIT. In a browser the type is the DOM node:
# web/src/view/dom.ts sets text through textContent and attributes through
# setAttribute, and nothing may parse a string into markup. Every string this
# app displays came out of attacker-controlled mail, so this is a gate rather
# than a convention — a mechanism, not discipline.
#
# ⚠️ `--binary-files=text`, NEVER `-I`. `-I` calls a file holding a NUL byte
# binary and skips it WITHOUT SAYING SO, so a source file spelling a sentinel
# as a literal NUL would go unread, and an `innerHTML =` added to it would
# build green.
#
# EIGHT FORMS, BECAUSE THE RULE IS ABOUT ALL OF THEM. CLAUDE.md says "never
# markup parsed from a string, with ONE counted exception"; a gate that names
# four of the ways a string becomes markup enforces those four. So the list is
# every form the platform offers, `DOMParser`,
# `Range.createContextualFragment`, `setHTMLUnsafe`/`parseHTMLUnsafe` and an
# iframe's `srcdoc` included.
#
# ONE EXCEPTION, AND IT IS COUNTED. Todo bodies are typst source,
# `suzatary_io::render` renders them and web/src/rich.ts is the single place
# that parses that HTML into nodes. So the rule became "exactly one door"
# rather than "none": the forms above are banned outright everywhere, an
# innerHTML ASSIGNMENT is banned everywhere but rich.ts, and rich.ts must hold
# exactly one — a door that disappeared, or a second one, fails the build.
# Prose may say "innerHTML"; only the assignment is counted, and it is counted
# by OCCURRENCE rather than by line, because two doors written on one line are
# two doors.
# AND A GATE THAT CAN SKIP A FILE MUST SAY SO. A skip is SILENT: grep exits 0
# over a directory it has not finished reading, and no number anywhere
# disagrees with any other. So the gate counts, TWICE, because one count is
# not enough.
#
# ⚠️ THE OBVIOUS CENSUS DOES NOT CATCH THE BUG IT WAS WRITTEN FOR. "The files
# grep scanned must equal the files `find` reports" sounds like the whole
# answer, and under GNU grep 3.12 (this shell's, measured) `grep -c` prints a
# line for EVERY file it opened — including one `-I` refused to search, which
# it reports as `file:0`. So that comparison would count a NUL file as
# scanned while never reading it. It is kept because it catches
# a real and different class — a file grep cannot open, an `--include` or
# `--exclude` that drifts from `find`'s idea of the tree — but it is the
# SECOND check here, not the first.
#
# The first catches the skipped file: `-I` and
# `--binary-files=text` differ on exactly one thing, a file holding a byte grep
# calls binary, so the two `-l` listings agreeing IS the claim "every source
# file in this tree is plain text". An empty file is absent from both, so it
# fails neither.
plain=$(grep -RIl --include='*.ts' -e '' web/src web/test web/e2e | wc -l)
text=$(grep -Rl --binary-files=text --include='*.ts' -e '' web/src web/test web/e2e | wc -l)
if [[ "$plain" != "$text" ]]; then
  echo "web-static.sh: $((text - plain)) .ts file(s) hold a byte grep reads as binary" >&2
  echo "  Name them with: grep -RIL --include='*.ts' -e '' web/src web/test web/e2e" >&2
  echo "  Spell the byte as an escape (\\u0000). A file grep will not read as text is a" >&2
  echo "  file that drops out of every tool that greps this tree, silently." >&2
  exit 1
fi

scanned=$(grep -Rc --binary-files=text --include='*.ts' '' web/src web/test web/e2e | wc -l)
present=$(find web/src web/test web/e2e -name '*.ts' | wc -l)
if [[ "$scanned" != "$present" ]]; then
  echo "web-static.sh: the escaping gate opened $scanned of $present .ts files — it must open every one" >&2
  echo "  a file grep declines to read is a file outside the one-door rule, and it will not announce itself." >&2
  exit 1
fi
echo "==> the one door: $present .ts files, all of them plain text, all of them scanned"

if grep -Rn --binary-files=text --include='*.ts' \
    -e '\.outerHTML' -e 'insertAdjacentHTML' -e 'document\.write' \
    -e 'DOMParser' -e 'createContextualFragment' \
    -e 'setHTMLUnsafe' -e 'parseHTMLUnsafe' -e 'srcdoc' \
    web/src web/test web/e2e; then
  echo "web-static.sh: markup is parsed from a string above — build DOM nodes instead (web/src/view/dom.ts)" >&2
  exit 1
fi

if grep -Rn --binary-files=text --include='*.ts' --exclude='rich.ts' \
    -e '\.innerHTML[[:space:]]*=' web/src web/test web/e2e; then
  echo "web-static.sh: only web/src/rich.ts may parse markup — build DOM nodes instead (web/src/view/dom.ts)" >&2
  exit 1
fi

doors=$(grep -o --binary-files=text '\.innerHTML[[:space:]]*=' web/src/rich.ts | wc -l)
if [[ "$doors" != "1" ]]; then
  echo "web-static.sh: web/src/rich.ts is THE door and must hold exactly one innerHTML assignment (found $doors)" >&2
  exit 1
fi

# THE DEVICE'S RENDERED BLOCKS HAVE ONE WRITER: the worker's serve constructs the device's store, and no page module
# does, so no page can write back a block an erasure removed (web/test/onewriter.test.ts is its law).
makers=$(grep -Rl --binary-files=text --include='*.ts' -e 'rendered\.device(' web/src | sort | tr '\n' ' ')
if [[ "$makers" != "web/src/prodrome/questions.ts " ]]; then
  echo "web-static.sh: only web/src/prodrome/questions.ts may construct rendered.device (found: ${makers:-none})" >&2
  exit 1
fi

# A LITERAL IS HELD UNHASHED ONLY THROUGH ITS ADMISSION: `Lines.hold_admitted` takes JSON, so its one caller is
# core.ts's `holdAdmitted`, which takes only `content.Admitted` rows, and no other file may call it.
holders=$(grep -Rl --binary-files=text --include='*.ts' -e '\.hold_admitted(' web/src web/test web/e2e | sort | tr '\n' ' ')
if [[ "$holders" != "web/src/prodrome/core.ts " ]]; then
  echo "web-static.sh: only web/src/prodrome/core.ts may call Lines.hold_admitted (found: ${holders:-none})" >&2
  exit 1
fi

# A CEILING ON `as`, THAT ONLY MOVES DOWN. `el`/`svg` are typed by tag, so a
# cast narrowing what one
# of them returns is a cast with no job left; this count is the blunt
# instrument that keeps that true — it is a literal-substring count of ' as '
# on lines that are not whole-line comments (it still counts `import x as y`
# and a trailing comment's word), so the ceiling and the number a reader can
# reproduce are the same grep. Prose stays out because a comment's "as" twice
# failed a gate in one day (2026-10-03) with no cast added. Only
# `web/src/wire/generated/` is excluded, as output nobody hand-edits; the decoders
# narrow `unknown` through the one cast in `decoder.ts`.
# A future commit may lower it and must never raise it.
as_ceiling=32
as_count=$(grep -rn --binary-files=text --include='*.ts' -e ' as ' web/src | grep -v '/generated/' | grep -cvE '^[^:]+:[0-9]+:[[:space:]]*(//|\*|/\*)')
if [[ "$as_count" -gt "$as_ceiling" ]]; then
  echo "web-static.sh: web/src holds $as_count ' as ' occurrences outside generated/, over the ceiling of $as_ceiling" >&2
  echo "  a new cast needs a reason the doors in docs/WEB-SPEC.md §2 don't already give — lower the ceiling when you remove one" >&2
  exit 1
fi

echo "==> cast ceiling: $as_count <= $as_ceiling"

# A DAY OF AN INSTANT IS THE OWNER'S. An instant is UTC, so cutting its first ten characters
# names UTC's day, which is the next day every evening in New York; `dayOf` reads it in the
# owner's zone. `day` and the core's `writtenDay` are for a written day, so a cut applied
# to an `at` is refused.
utc_days=$(grep -rnE --binary-files=text --include='*.ts' '\b(day|writtenDay)\([^()]*([.]at|At)\)|([.]at|At|At\([^()]*\))[.]slice\(0, 10\)' web/src | grep -v '/generated/' || true)
if [[ -n "$utc_days" ]]; then
  echo "web-static.sh: a day cut from an instant, which is UTC's day and not the owner's — use dayOf:" >&2
  echo "$utc_days" >&2
  exit 1
fi

echo "==> no instant is cut to UTC's day"

# THE ADDRESS HAS ONE WRITER. The route is a register of the ui tier and the
# hash is its print, so only web/src/shell/address.ts writes `location` or
# `history`: a page that set the hash itself would write a place the log never
# held, and back would skip it.
writers=$(grep -rnE --binary-files=text --include='*.ts' 'location\.(hash|href)[[:space:]]*=[^=]|location\.(replace|assign)\(|history\.(pushState|replaceState|back|forward|go)\(' web/src | grep -v '^web/src/shell/address.ts:' || true)
if [[ -n "$writers" ]]; then
  echo "web-static.sh: only web/src/shell/address.ts writes the address — call address.go, address.replace or address.back:" >&2
  echo "$writers" >&2
  exit 1
fi

echo "==> the address has one writer"

# A PAGE-SIDE WRITE IN A SPEC CARRIES THE APP'S OWN HEADERS. A spec that spells
# them goes stale the day the box raises its write contract (`gate::CLIENT_MIN`)
# and is answered 426, so every spec uses the harness's `WRITE_HEADERS`.
spelled=$(grep -rniE --binary-files=text --include='*.ts' 'x-suzatary-(act|client)' web/e2e | grep -v '^web/e2e/harness.ts:' || true)
if [[ -n "$spelled" ]]; then
  echo "web-static.sh: a spec spells a write header; use WRITE_HEADERS from web/e2e/harness.ts:" >&2
  echo "$spelled" >&2
  exit 1
fi

# And a spec's write names them at all: every `method: "POST"` (or another
# writing method) has `WRITE_HEADERS`, or a `headers` bound to it, within three
# lines, so a write that forgets the contract fails here and not as a 426.
bare=$(awk '
  FNR == 1 { delete L; delete at; n = 0 }
  { L[FNR] = $0 }
  /method: *"(POST|PUT|PATCH|DELETE)"/ { at[++n] = FNR }
  ENDFILE {
    for (k = 1; k <= n; k++) {
      ok = 0
      for (i = at[k] - 3; i <= at[k] + 3; i++) if (L[i] ~ /WRITE_HEADERS|headers[,: }]/) ok = 1
      if (!ok) print FILENAME ":" at[k] ": " L[at[k]]
    }
  }' web/e2e/*.test.ts)
if [[ -n "$bare" ]]; then
  echo "web-static.sh: a spec writes without WRITE_HEADERS from web/e2e/harness.ts:" >&2
  echo "$bare" >&2
  exit 1
fi
# A helper that writes for the specs (#94's SIGNED_POST, any `*_POST`) carries
# them too: its definition names WRITE_HEADERS, http.actRequest or the header.
helpers=$(awk '
  /(const|function) +[A-Z_]*POST[A-Z_]* *[=(]/ { name = FILENAME ":" FNR; left = 20; ok = 0 }
  left > 0 { if ($0 ~ /WRITE_HEADERS|actRequest|CLIENT_HEADER/) ok = 1; if (--left == 0 && !ok) print name }
  ENDFILE { if (left > 0 && !ok) print name; left = 0 }' web/e2e/*.ts)
if [[ -n "$helpers" ]]; then
  echo "web-static.sh: a write helper for the specs does not carry WRITE_HEADERS:" >&2
  echo "$helpers" >&2
  exit 1
fi
bound=$(grep -nE --binary-files=text 'const headers *=' web/e2e/*.test.ts | grep -v 'WRITE_HEADERS' || true)
if [[ -n "$bound" ]]; then
  echo "web-static.sh: a spec binds headers to something other than WRITE_HEADERS:" >&2
  echo "$bound" >&2
  exit 1
fi

echo "==> every spec's page-side write carries the app's own headers"
