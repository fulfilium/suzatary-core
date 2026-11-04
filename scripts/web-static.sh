#!/usr/bin/env bash
# The web's static checks, the one spelling of each, called by build-web.sh and web-test.sh.
# Pure grep over web/src, web/test and web/e2e.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

# Escaping: every displayed string came from attacker-controlled mail, so nothing may parse a
# string into markup except web/src/rich.ts, which holds exactly one innerHTML assignment
# (counted by occurrence, not line). Every form the platform offers is banned.
#
# `--binary-files=text`, never `-I`: `-I` silently skips a file holding a NUL byte, so an
# `innerHTML =` added to it would build green.
#
# A skipped file is silent, so the gate counts twice. First: `-I` and `--binary-files=text`
# listings agree only if every source file is plain text. Second: files grep opened equal
# files `find` sees (GNU grep 3.12's `-c` lists a file `-I` refused as `file:0`, so this alone
# would miss a NUL file; it catches an unopenable file or a drifting `--include`).
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

# The device's rendered blocks have one writer, so no page can write back a block an erasure
# removed (web/test/onewriter.test.ts).
makers=$(grep -Rl --binary-files=text --include='*.ts' -e 'rendered\.device(' web/src | sort | tr '\n' ' ')
if [[ "$makers" != "web/src/prodrome/questions.ts " ]]; then
  echo "web-static.sh: only web/src/prodrome/questions.ts may construct rendered.device (found: ${makers:-none})" >&2
  exit 1
fi

# A literal is held unhashed only through its admission: `Lines.hold_admitted` takes JSON, so its
# one caller is core.ts's `holdAdmitted`, which takes only `content.Admitted` rows.
holders=$(grep -Rl --binary-files=text --include='*.ts' -e '\.hold_admitted(' web/src web/test web/e2e | sort | tr '\n' ' ')
if [[ "$holders" != "web/src/prodrome/core.ts " ]]; then
  echo "web-static.sh: only web/src/prodrome/core.ts may call Lines.hold_admitted (found: ${holders:-none})" >&2
  exit 1
fi

# A ceiling on ` as `, that only moves down: never raise it. Counts the substring on lines that
# are not whole-line comments (`import x as y` counts), outside web/src/wire/generated/.
as_ceiling=23
as_count=$(grep -rn --binary-files=text --include='*.ts' -e ' as ' web/src | grep -v '/generated/' | grep -cvE '^[^:]+:[0-9]+:[[:space:]]*(//|\*|/\*)')
if [[ "$as_count" -gt "$as_ceiling" ]]; then
  echo "web-static.sh: web/src holds $as_count ' as ' occurrences outside generated/, over the ceiling of $as_ceiling" >&2
  echo "  a new cast needs a reason the doors in docs/WEB-SPEC.md §2 don't already give — lower the ceiling when you remove one" >&2
  exit 1
fi

echo "==> cast ceiling: $as_count <= $as_ceiling"

# An instant is UTC, so cutting its first ten characters names UTC's day, not the owner's;
# `dayOf` reads it in the owner's zone. `day` and `writtenDay` are for a written day.
utc_days=$(grep -rnE --binary-files=text --include='*.ts' '\b(day|writtenDay)\([^()]*([.]at|At)\)|([.]at|At|At\([^()]*\))[.]slice\(0, 10\)' web/src | grep -v '/generated/' || true)
if [[ -n "$utc_days" ]]; then
  echo "web-static.sh: a day cut from an instant, which is UTC's day and not the owner's — use dayOf:" >&2
  echo "$utc_days" >&2
  exit 1
fi

echo "==> no instant is cut to UTC's day"

# Only web/src/shell/address.ts writes `location` or `history`: a page that set the hash itself
# would write a place the log never held, and back would skip it.
writers=$(grep -rnE --binary-files=text --include='*.ts' 'location\.(hash|href)[[:space:]]*=[^=]|location\.(replace|assign)\(|history\.(pushState|replaceState|back|forward|go)\(' web/src | grep -v '^web/src/shell/address.ts:' || true)
if [[ -n "$writers" ]]; then
  echo "web-static.sh: only web/src/shell/address.ts writes the address — call address.go, address.replace or address.back:" >&2
  echo "$writers" >&2
  exit 1
fi

echo "==> the address has one writer"

# A spec that spells the write headers goes stale when `gate::CLIENT_MIN` rises (426), so every
# spec uses the harness's `WRITE_HEADERS`.
spelled=$(grep -rniE --binary-files=text --include='*.ts' 'x-suzatary-(act|client)' web/e2e | grep -v '^web/e2e/harness.ts:' || true)
if [[ -n "$spelled" ]]; then
  echo "web-static.sh: a spec spells a write header; use WRITE_HEADERS from web/e2e/harness.ts:" >&2
  echo "$spelled" >&2
  exit 1
fi

# Every writing `method:` has `WRITE_HEADERS`, or a `headers` bound to it, within three lines.
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
# A `*_POST` helper's definition names WRITE_HEADERS, http.actRequest or the header.
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
