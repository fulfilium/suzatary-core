#!/usr/bin/env bash
# The public tree of a commit, and the leak guard over it. Never touches a git remote or a
# repository's visibility.
#
#   scripts/public-tree.sh <out> [<rev>]          materialise the public tree of <rev> (HEAD)
#   scripts/public-tree.sh --store [<rev>]        the same, added to the Nix store as a deploy fetches it; prints its path
#   scripts/public-tree.sh --check <dir>          the leak guard, every rule
#   scripts/public-tree.sh --check --full ...     the same, reading every file again (or SUZATARY_GUARD_FULL)
#   scripts/public-tree.sh --check --strict <dir> the same, failing where a rule would skip
#   scripts/public-tree.sh --check --lists [--admitted <manifest> | --text] <dir>   secrets, admitted files and lists; `--text` admits nothing
#   scripts/public-tree.sh --check --terms <dir>  the data repository's lists alone
#
# The tree is <rev>'s `git archive` (never an uncommitted edit) minus `export-ignore` paths, with
# scripts/public-changelog.md as its changelog, dated from the real changelog or `unreleased`,
# never the clock. Nothing else is removed: whatever else would leak is the guard's to find.
#
# Invariant: every byte of <dir> is either plain text read by both gitleaks and `suzatary leaks`,
# or in a file scripts/public-admitted.sha256 lists by path and SHA-256, admitted at a terminal.
#
# It fails on: a symbolic link; an email outside the reserved example domains (a URL's userinfo
# is not one); a `.ts.net` name with a non-placeholder tailnet; a home path other than `ada` or
# `root`; a path to a private-history file; this deployment's box, tailnet or Unix user; a term
# of `<data>/local/{personal,context}-terms.txt`; a tailnet address outside 100.64.0.0/24 or a
# phone number outside 555; outside `--terms`, a gitleaks secret not fingerprinted in
# `.gitleaksignore`, or an opaque file (`suzatary_core::leak::Opaque`) or one with an ignored
# finding that the manifest does not admit with these bytes; with every rule, a front page's
# relative link to nothing.
# A path with a control character is refused (2) before anything reads it. Terms match as whole
# words (`[a-z0-9_]`) of NFKD-folded lowercase text with whitespace runs as one space.
# <data> is the instance file's directory; without one the rules that need it skip, or fail
# under `--strict`. A hit prints only `file:line` (`:0` for a path, link or unadmitted file),
# never what it guards.
#
# Exit, as grep's: 0 clean, 1 a hit, 2 could not run whole. Only 1 says the tree leaks; a caller
# that acts on a leak must never read a 2 as one.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FRONT_PAGES=(README.md CONTRIBUTING.md SECURITY.md docs/README.md)

usage() {
  echo "usage: public-tree.sh <out> [<rev>] | public-tree.sh --store [<rev>] | public-tree.sh --check [--full] [--strict | --lists [--admitted <manifest> | --text] | --terms] <dir>" >&2
  exit 2
}

# NUL-separated, relative to the tree.
files_of() {
  if [ "$(git -C "$1" rev-parse --show-toplevel 2>/dev/null)" = "$(cd "$1" && pwd -P)" ]; then
    git -C "$1" ls-files -z
  else
    (cd "$1" && find . -type f -print0 | sed -z 's#^\./##')
  fi
}

# app/Cargo.toml for a commit older than app/release.toml.
fresh_changelog() {
  local rev="$1" version date
  version="$( { git -C "$REPO" show "$rev:app/release.toml" 2>/dev/null \
    || git -C "$REPO" show "$rev:app/Cargo.toml"; } | sed -n 's/^version = "\(.*\)"$/\1/p' | head -1)"
  date="$(git -C "$REPO" show "$rev:CHANGELOG.md" | sed -n "s/^## \[$version\] - \(.*\)$/\1/p" | head -1)"
  [ -n "$date" ] || date=unreleased
  git -C "$REPO" show "$rev:scripts/public-changelog.md" | sed "s/@VERSION@/$version/; s/@DATE@/$date/"
}

# `page:line` for each relative link of a front page that names no file of <dir>.
dangling_links() {
  local dir="$1" page line target path
  for page in "${FRONT_PAGES[@]}"; do
    [ -f "$dir/$page" ] || continue
    while IFS=$'\t' read -r line target; do
      case "$target" in '' | '#'* | *://* | mailto:*) continue ;; esac
      path="$(realpath -m "$dir/$(dirname "$page")/${target%%#*}")"
      case "$path" in "$(realpath "$dir")"/*) [ -e "$path" ] && continue ;; esac
      echo "$page:$line"
      echo "public-tree: $page:$line links to nothing the tree holds" >&2
    done < <(awk '{
      s = $0
      while (match(s, /\]\([^) ]+\)|(src|href)="[^"]+"/)) {
        t = substr(s, RSTART, RLENGTH); s = substr(s, RSTART + RLENGTH)
        if (t ~ /^\]/) t = substr(t, 3, length(t) - 3); else { sub(/^[a-z]+="/, "", t); sub(/"$/, "", t) }
        print FNR "\t" t
      }
    }' "$dir/$page")
  done
}

materialise() {
  local out="$1" rev
  rev="$(git -C "$REPO" rev-parse --verify --quiet "${2:-HEAD}^{commit}")" || {
    echo "public-tree: ${2:-HEAD} names no commit" >&2
    exit 1
  }
  [ ! -e "$out" ] || [ -z "$(ls -A "$out")" ] || {
    echo "public-tree: $out exists and is not empty" >&2
    exit 1
  }
  mkdir -p "$out"
  git -C "$REPO" archive --format=tar "$rev" | tar -x -C "$out"
  # git holds no empty directory.
  find "$out" -mindepth 1 -type d -empty -delete
  fresh_changelog "$rev" >"$out/CHANGELOG.md"
  echo "public-tree: wrote $(find "$out" -type f | wc -l) files to $out" >&2
}

# Recorded in <git-common-dir>/public-trees under the hash of <rev>'s tree and this script.
store_path() {
  local rev tree key record path dir code=0
  rev="$(git -C "$REPO" rev-parse --verify --quiet "${1:-HEAD}^{commit}")" || {
    echo "public-tree: ${1:-HEAD} names no commit" >&2
    exit 1
  }
  tree="$(git -C "$REPO" rev-parse "$rev^{tree}")"
  key="$({ echo "$tree"; sha256sum <"$REPO/scripts/public-tree.sh"; } | sha256sum | cut -c1-40)"
  record="$(git -C "$REPO" rev-parse --path-format=absolute --git-common-dir)/public-trees/$key"
  path="$(cat "$record" 2>/dev/null || true)"
  if [ -n "$path" ] && nix-store --check-validity "$path" 2>/dev/null; then
    echo "public-tree: the public tree of ${rev:0:12} is in the store already" >&2
    echo "$path"
    return
  fi
  dir="$(mktemp -d)"
  { materialise "$dir/tree" "$rev" && path="$(nix store add-path --name source "$dir/tree")"; } || code=$?
  rm -rf "$dir"
  [ "$code" -eq 0 ] || exit "$code"
  mkdir -p "$(dirname "$record")" && echo "$path" >"$record"
  echo "$path"
}

# gitleaks, offline, with scripts/secret-rules.toml, three decodings deep. It reads a copy with
# every file named by a number, so gitleaks's default path allowlist (`*.svg`, lock files,
# `node_modules/`) passes over nothing; sealed slots are placeholders (`suzatary leaks
# scan-copy`). `.gitleaksignore` fingerprints admit; `gitleaks:allow` admits nothing. Prints
# `file:line`, never the secret. gitleaks exits 0 or 3 (found) when it ran.
secrets() {
  local code=0
  mkdir "$found.tree"
  tr '\n' '\0' <"$found.todo" | (cd "$1" && "$bin" leaks scan-copy "$found.tree") >"$found.index" || return 2
  # A fingerprint of a path the tree holds, by its number; any other line admits nothing.
  : >"$found.ignore"
  if [ -f "$1/.gitleaksignore" ]; then
    awk -F'\t' 'NR == FNR { n[$2] = $1; next }
      { r = $0; if (match(r, /:[^:]*:[0-9]+$/) && (substr(r, 1, RSTART - 1) in n))
          print n[substr(r, 1, RSTART - 1)] substr(r, RSTART) }' \
      "$found.index" "$1/.gitleaksignore" >"$found.ignore" || return 2
  fi
  printf '{{ range . }}{{ .File }}\t{{ .StartLine }}\t{{ .RuleID }}\n{{ end }}' >"$found.template"
  (cd "$found.tree" && gitleaks dir . --config "$REPO/scripts/secret-rules.toml" --no-banner --no-color \
    --log-level error --redact --max-decode-depth 3 --ignore-gitleaks-allow --exit-code 3 \
    --gitleaks-ignore-path "$found.ignore" --report-format template \
    --report-template "$found.template" --report-path "$found.secrets") || code=$?
  [ "$code" -eq 0 ] || [ "$code" -eq 3 ] || return 2
  awk -F'\t' 'NR == FNR { p[$1] = $2; next } {
      f = $1; sub(/^\.\//, "", f)
      print p[f] ":" $2; printf "public-tree: %s:%s holds a secret (gitleaks rule %s)\n", p[f], $2, $3 > "/dev/stderr" }' \
    "$found.index" "$found.secrets"
}

terms_of() {
  grep -vE '^[[:space:]]*(#|$)' "$1" | sed 's/^[[:space:]]*//; s/[[:space:]]*$//' | tr '[:upper:]' '[:lower:]'
}

skipped() {
  if [ -n "$strict" ]; then
    echo "public-tree: $1, so the check is not whole (--strict)" >&2
    exit 2
  fi
  echo "public-tree: skipped $1" >&2
}

# check <whole|lists|terms> <dir> <manifest or empty for dir's own>
check() {
  local rules="$1" dir="$2" manifest="$3" instance data hits repo
  instance="${SUZATARY_INSTANCE-$REPO/instance.toml}"
  terms="" words="" links="" found="" leaked="" failed=""
  # Any failure but a hit is a check that did not run: 2, never a hit's 1.
  trap 'status=$?; rm -rf "$terms" "$words" "$links" "$found" "$found".*; [ "$status" -eq 0 ] || [ -n "$leaked" ] || exit 2' EXIT
  terms="$(mktemp)"
  words="$(mktemp)"
  links="$(mktemp)"
  found="$(mktemp)"
  local bin="${SUZATARY_BIN:-}"
  [ -n "$bin" ] || bin="$("$REPO/scripts/app-bin.sh")/bin/suzatary"
  [ -x "$bin" ] || { echo "public-tree: no matcher at $bin" >&2; exit 2; }
  if [ -n "$instance" ] && [ -f "$instance" ]; then
    data="$(dirname "$(realpath "$instance")")"
    # `suzatary_cli::instance::Local::DIR`.
    local_dir="$data/local"
    [ -f "$local_dir/personal-terms.txt" ] || {
      echo "public-tree: $local_dir/personal-terms.txt is missing" >&2
      exit 2
    }
    terms_of "$local_dir/personal-terms.txt" >>"$terms"
    if [ -f "$local_dir/context-terms.txt" ]; then
      terms_of "$local_dir/context-terms.txt" >>"$terms"
    else
      skipped "context-terms.txt: the box-local tier has none yet"
    fi
    # Every instance names `repo`, so a file that does not read stops the check.
    repo="$("$bin" --instance "$instance" instance get repo)" || {
      echo "public-tree: the binary ($bin) read no repo from $instance, so the check did not run whole" >&2
      exit 2
    }
    {
      [ -f "$data/FETCHER" ] && tr -d '[:space:]' <"$data/FETCHER" && echo
      { "$bin" --instance "$instance" instance get web.app_url 2>/dev/null || true; } |
        sed -n 's#^[a-z]*://\([^/:]*\).*#\1#p' | tr '.' '\n' | grep -vxE 'ts|net|com|org|www|localhost|[0-9]+' || true
      { realpath "$instance"; echo "$repo"; "$bin" --instance "$instance" instance get data 2>/dev/null || true; } |
        sed -n 's#^/\(home\|Users\)/\([^/]*\).*#\2#p'
    } | tr '[:upper:]' '[:lower:]' | grep -E '^[a-z0-9_-]+$' | sort -u >"$words" || true
  else
    skipped "the data repository's lists and this deployment's names: no instance file"
  fi

  if [ "$rules" = whole ]; then
    # Private-history files below the root (a root one is never linked).
    git -C "$REPO" ls-files | git -C "$REPO" check-attr --stdin export-ignore |
      sed -n 's/: export-ignore: set$//p' | grep / >"$links" || true
  fi

  # Every reader after this one reads lines.
  local odd
  odd="$(files_of "$dir" | while IFS= read -r -d '' path; do
    if [[ "$path" =~ [[:cntrl:]] ]]; then printf '%q\n' "$path"; fi
  done)" || failed=yes
  if [ -n "$odd" ]; then
    printf 'public-tree: a path holds a control character, refused:\n%s\n' "$odd" >&2
    exit 2
  fi
  local match=("$bin" leaks --terms "$terms" --words "$words" --links "$links") scope="$rules"
  local ignore admitted=()
  if [ "$rules" != terms ]; then
    [ -n "$manifest" ] || manifest="$(cd "$dir" && pwd -P)/scripts/public-admitted.sha256"
    [ -f "$manifest" ] || manifest=/dev/null
    ignore="$(cd "$dir" && pwd -P)/.gitleaksignore"
    [ -f "$ignore" ] || ignore=/dev/null
    admitted=(--admitted "$manifest" --findings "$ignore" --admit-by "$admit_by")
  fi
  # A file that passed under the same rules is not read again. The key hashes everything that
  # judges a file. The table holds `<sha256>  <path>` lines of clean files only. Paths, links and
  # symbolic links are checked whole every time.
  local passes key table read_from
  passes="${SUZATARY_GUARD_PASSES:-${XDG_CACHE_HOME:-$HOME/.cache}/suzatary/public-guard}"
  key="$({
    printf '%s\n' "$rules" "$strict" "${admit_by:-}"
    sha256sum "$terms" "$words" "$links" "$bin" "$(command -v gitleaks || echo /dev/null)" \
      "$REPO/scripts/secret-rules.toml" "$REPO/scripts/public-tree.sh" "${manifest:-/dev/null}" \
      "${ignore:-/dev/null}" | cut -c1-64
  } | sha256sum | cut -c1-64)" || exit 2
  table="$passes/$key"
  files_of "$dir" | (cd "$dir" && xargs -0 -r sha256sum -z -- 2>/dev/null || true) | tr '\0' '\n' |
    LC_ALL=C sort -u >"$found.sums"
  read_from="$table"
  [ -z "$full" ] && [ -f "$table" ] || read_from=/dev/null
  LC_ALL=C comm -12 "$found.sums" "$read_from" | cut -c67- >"$found.kept"
  { files_of "$dir" | tr '\0' '\n' | grep -vxFf "$found.kept" || true; } >"$found.todo"
  echo "public-tree: reading $(wc -l <"$found.todo") files; $(wc -l <"$found.kept") passed these rules before" >&2

  # Hits go to a file, never through a pipe's last command, so a failing matcher is a check that
  # did not run, never a clean tree.
  {
    if [ "$rules" = terms ]; then
      :
    elif [ "$(git -C "$dir" rev-parse --show-toplevel 2>/dev/null)" = "$(cd "$dir" && pwd -P)" ]; then
      git -C "$dir" ls-files -s | awk '$1 == "120000" { sub(/^[^\t]*\t/, ""); print $0 ":0" }' || failed=yes
    else
      (cd "$dir" && find . -type l | sed 's#^\./##; s#$#:0#') || failed=yes
    fi
    files_of "$dir" | tr '\0' '\n' | "${match[@]}" --scope "$scope" --paths || failed=yes
    tr '\n' '\0' <"$found.todo" | (cd "$dir" && xargs -0 -r "${match[@]}" --scope "$scope" "${admitted[@]}" --) ||
      failed=yes
    [ "$rules" = terms ] || [ ! -s "$found.todo" ] || secrets "$dir" || failed=yes
    [ "$rules" != whole ] || dangling_links "$dir" || failed=yes
  } >"$found" || failed=yes
  if [ -n "$failed" ]; then
    echo "public-tree: the matcher ($bin) or the secret scanner failed, so the check did not run whole" >&2
    exit 2
  fi
  hits="$(awk '!seen[$0]++' "$found")"
  # Past 100000 lines the table restarts; a lost pass costs a read, never a verdict.
  local fresh=""
  mkdir -p "$passes" && fresh="$(mktemp "$passes/.fresh.XXXXXX")" &&
    {
      { [ ! -f "$table" ] || [ "$(wc -l <"$table")" -gt 100000 ] || cat "$table"
        sed 's/:[0-9]*$//' "$found" | LC_ALL=C sort -u >"$found.hit"
        awk 'FILENAME == ARGV[1] { hit[$0]; next } !(substr($0, 67) in hit)' "$found.hit" "$found.sums"
      } | LC_ALL=C sort -u >"$fresh" && mv "$fresh" "$table"
    } || { [ -z "$fresh" ] || rm -f "$fresh"; echo "public-tree: could not keep the passes in $passes" >&2; }
  if [ -n "$hits" ]; then
    printf '%s\n' "$hits"
    echo "public-tree: $(printf '%s\n' "$hits" | wc -l) line(s) above would leak or link to nothing" >&2
    leaked=yes
    return 1
  fi
  echo "public-tree: $dir is clean" >&2
}

case "${1:-}" in
  --check)
    shift
    rules=whole strict="" admitted="" admit_by=train full="${SUZATARY_GUARD_FULL:+1}"
    if [ "${1:-}" = --full ]; then full=1 && shift; fi
    case "${1:-}" in
      --lists) rules=lists && shift ;;
      --terms) rules=terms && shift ;;
      --strict) strict=1 && shift ;;
    esac
    if [ "$rules" = lists ] && [ "${1:-}" = --admitted ]; then
      [ $# -ge 2 ] && [ -f "$2" ] || usage
      admitted="$(realpath "$2")" admit_by=scan && shift 2
    elif [ "$rules" = lists ] && [ "${1:-}" = --text ]; then
      admitted=/dev/null admit_by=never && shift
    fi
    [ $# -eq 1 ] && [ -d "$1" ] || usage
    check "$rules" "$1" "$admitted"
    ;;
  --store)
    shift
    [ $# -le 1 ] || usage
    store_path "$@"
    ;;
  "" | -*) usage ;;
  *)
    [ $# -le 2 ] || usage
    materialise "$@"
    ;;
esac
