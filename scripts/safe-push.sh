#!/usr/bin/env bash
# The data push: the only path by which what the box wrote reaches the data repository. It
# checks the data alone (the allowlist, the append-only walk, `suzatary verify`) and builds
# nothing: the code it runs already passed the merge gate. Then each public store's clone
# (`suzatary instance public`) goes through scripts/roadmap-push.sh, behind door 2; a clone that
# push refuses stays unpushed, and the data push has gone either way.
#
# Usage: safe-push.sh (--dry | <commit-message>)
set -euo pipefail

SCRIPTS="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="${SUZATARY_REPO:-$(cd "$SCRIPTS/.." && pwd)}"
cd "$REPO"

# --dry runs every check and says what it would commit; it commits, moves and pushes nothing.
DRY=0
if [ "${1:-}" = --dry ]; then
  DRY=1
  MSG=""
  # No transport: a remote is neither read nor written, whatever runs.
  export GIT_ALLOW_PROTOCOL=none
elif [ "${1:0:2}" = -- ]; then
  echo "safe-push: unknown flag ${1}; usage: safe-push.sh (--dry | <commit-message>)" >&2
  exit 64
else
  MSG="${1:?usage: safe-push.sh (--dry | <commit-message>)}"
fi

# The verifier is the deployed binary. Only --dry may name another in SUZATARY_BIN, so a push's
# verifier is never a variable away.
if [ -n "${SUZATARY_BIN:-}" ] && [ "$DRY" -eq 0 ]; then
  echo "safe-push: SUZATARY_BIN names the verifying program for --dry only; refusing" >&2
  exit 64
fi
app() {
  if [ -n "${SUZATARY_BIN:-}" ]; then "$SUZATARY_BIN" "$@"; else "$REPO/scripts/app-bin.sh" "$@"; fi
}

# The instance file's directory is the data tree. Required: a push that guessed its data tree
# would verify one tree and commit another.
INSTANCE="${SUZATARY_INSTANCE:-$REPO/instance.toml}"
if [ ! -f "$INSTANCE" ]; then
  echo "safe-push: no instance file at $INSTANCE" >&2
  echo "  Set SUZATARY_INSTANCE=/path/to/instance.toml (the data tree's), or run" >&2
  echo "  from a checkout that holds one. Refusing rather than pushing blind." >&2
  exit 64
fi
export SUZATARY_INSTANCE="$INSTANCE"
# realpath: the code checkout's `instance.toml` is a link to the data tree's.
DATA="$(dirname "$(realpath "$INSTANCE")")"
# Human-edited data files, staged only when the data is a tree of its own.
DATA_EXTRA=''
DATA_EXTRA_PATTERN='^$'
cd "$DATA"

# Narrow on purpose: the callers are agent-driven and the agent reads attacker-controlled mail.
# If this path committed code, a rewritten scripts/hourly-triage.sh would be pushed and run as
# the trusted wrapper next time. Code reaches master only through the merge gate and the train.
#
# Objects are named by their sha256; tips are derived, so no run writes `HEAD` or `refs/`.
# Never staged (the pattern refuses them) because a pushed object cannot be erased: the words
# stores (`secretary-memory-texts/`, `secretary-digest-texts/`), the locked words' keys
# (`secretary-lines-keys/`, so destroying a key erases words a pushed object holds as
# ciphertext), and the topics. `requests/` is provenance only, never a body or an answer.
#
# The three lists are the stores' rows (`suzatary_stores::stores::Kind`), spelled out so a quiet
# pass runs no program but git; app/cli/tests/guards/lists.rs pins each.
ALLOWED_PATTERN='^(events/(objects/[0-9a-f]{64}\.py|\.lock|\.gitattributes|\.gitignore)|events-suzatary/(objects/[0-9a-f]{64}\.py|\.lock|\.gitattributes|\.gitignore)|inbox/(objects/[0-9a-f]{64}\.py|\.gitattributes|\.gitignore)|secretary-lines/(objects/[0-9a-f]{64}\.py|\.gitattributes|\.gitignore)|memory/(objects/[0-9a-f]{64}\.py|\.gitattributes|\.gitignore)|digests/(objects/[0-9a-f]{64}\.py|\.gitattributes|\.gitignore)|decisions/(objects/[0-9a-f]{64}\.py|\.gitattributes|\.gitignore)|state\.json|requests/([0-9a-f]{64}\.json|\.gitignore))$'
ADD_PATHS='events/ events-suzatary/ inbox/ secretary-lines/ memory/ digests/ decisions/ state.json requests/'
WALKED='events/objects/ events-suzatary/objects/ inbox/objects/ secretary-lines/objects/ memory/objects/ digests/objects/ decisions/objects/ requests/'
# Never while the data tree is this checkout, where those files are a human's.
if [ "$DATA" != "$REPO" ]; then
  DATA_EXTRA='instance.toml mbsyncrc notes/ briefings/'
  DATA_EXTRA_PATTERN='^(instance\.toml|mbsyncrc|notes/.*|briefings/.*)$'
fi

# The data repository's commit and push; its own `exit`s end only it.
data_push() {
  # Serialize pushes (the voice server and the timer can both land here); one lock per repository,
  # in its git directory.
  exec 9>"$(git rev-parse --git-dir)/suzatary-push.lock"
  flock 9

  # Each listed path that names a file git could stage, as a glob pathspec. A path naming none
  # (a store before its first object) is left out: `git add` refuses it as fatal.
  STAGE=()
  set -f
  # shellcheck disable=SC2086
  for path in $ADD_PATHS $DATA_EXTRA; do
    if [ -n "$(git ls-files --cached --others --exclude-standard -- ":(glob)$path")" ]; then
      STAGE+=(":(glob)$path")
    else
      echo "safe-push: $path names nothing git could stage; skipped"
    fi
  done
  set +f

  # Asked before verifying, so a quiet pass every five minutes verifies nothing.
  if [ "${#STAGE[@]}" -eq 0 ] || [ -z "$(git status --porcelain -- "${STAGE[@]}")" ]; then
    echo "safe-push: nothing to commit"
    exit 0
  fi

  # Append-only: objects are only ever added, so any status but '??' or 'A ' under a walked store
  # is a history rewrite and must not be pushed. Every store in WALKED, not just events/. The one
  # change admitted besides is a line the lines' store keeps as its husk, which `verify --husked`
  # checks against the whole the last commit held.
  HUSK='^( M|M |MM) secretary-lines/objects/[0-9a-f]{64}\.py$'
  # shellcheck disable=SC2086
  CHANGES=$(git status --porcelain -- $WALKED | grep -vE '^(\?\?|A )' || true)
  REWRITES=$(printf '%s\n' "$CHANGES" | grep -vE "^\$|$HUSK" || true)
  HUSKED=$(printf '%s\n' "$CHANGES" | grep -E "$HUSK" | cut -c4- || true)
  if [ -n "$REWRITES" ]; then
    echo "safe-push: APPEND-ONLY VIOLATION under an events objects/ — a modification or" >&2
    echo "  deletion of a chain object is a history rewrite and will not be pushed:" >&2
    echo "$REWRITES" >&2
    exit 1
  fi

  # The stores' fsck; any problem aborts. A store opened as its program's alone (the lines') whose
  # record a door left stale, one the box's watch did not see, is recorded and verified again, so
  # such a door stops no push; the record writes only the store's own ignored files.
  if ! app verify; then
    # A dry run writes nothing, a record included: it says what a push would do and stops.
    if [ "$DRY" -eq 1 ]; then
      echo "safe-push: dry run; a push would record the stores opened as their program's alone and verify again; nothing is recorded" >&2
      exit 1
    fi
    echo "safe-push: recording the stores opened as their program's alone, then verifying again" >&2
    # The second verify judges: a store whose record failed (a plugin's, never pushed) is named
    # here and stops the push only if it fails that verify too.
    app prodrome record --opened alone || true
    app verify || {
      echo "safe-push: verify found the above, so nothing is committed; a .gitignore that does not name .writes is brought by: scripts/app-bin.sh --deployed migrate-ignores; a record of heads older than its objects (a pull, a copy or a restore) by: scripts/app-bin.sh --deployed prodrome record" >&2
      exit 1
    }
  fi
  # A new line is a whole: a husk enters history only in place of the whole a commit held.
  for path in $(git status --porcelain --untracked-files=all -- secretary-lines/objects \
    | grep -E '^(\?\?|A ) ' | cut -c4-); do
    name="${path##*/}"
    if [ "$(sha256sum < "$path" | cut -d' ' -f1)" != "${name%.py}" ]; then
      echo "safe-push: $path does not hash to its name, and no commit held its whole" >&2
      exit 1
    fi
  done
  if [ -n "$HUSKED" ]; then
    WAS="$(mktemp -d)"
    for path in $HUSKED; do git cat-file blob "HEAD:$path" > "$WAS/${path##*/}"; done
    app verify --husked "$WAS"
    rm -rf "$WAS"
  fi

  CHANGED=$(git status --porcelain | awk '{print $2}')
  UNEXPECTED=""
  for f in $CHANGED; do
    echo "$f" | grep -qE "$ALLOWED_PATTERN|$DATA_EXTRA_PATTERN" || UNEXPECTED="$UNEXPECTED $f"
  done
  if [ -n "$UNEXPECTED" ]; then
    # Reported, never an abort: only STAGE is staged and the staged set is judged below, so an
    # unexpected file cannot ride along either way.
    echo "safe-push: files outside the agent's set are modified (NOT committed):$UNEXPECTED"
    echo "safe-push: committing only $ADD_PATHS — expected while a human is working here."
  fi

  # STAGE goes into a copy of the index; every path it differs from HEAD by must match the
  # allowlist, or nothing is committed. The judged copy becomes the index, so what is committed is
  # exactly what was judged (the server writes objects meanwhile).
  INDEX="$(git rev-parse --path-format=absolute --git-path index)"
  JUDGED="$INDEX.safe-push"
  trap 'rm -f "$JUDGED"' EXIT
  if [ -f "$INDEX" ]; then cp "$INDEX" "$JUDGED"; else rm -f "$JUDGED"; fi
  GIT_INDEX_FILE="$JUDGED" git add -A -- "${STAGE[@]}"
  REFUSED=$(GIT_INDEX_FILE="$JUDGED" git diff --cached --no-renames --name-only \
    | grep -vE "$ALLOWED_PATTERN|$DATA_EXTRA_PATTERN" || true)
  if [ -n "$REFUSED" ]; then
    echo "safe-push: the staged set holds files the allowlist refuses, so nothing is committed:" >&2
    echo "$REFUSED" >&2
    exit 1
  fi
  # A store writes its own `.gitignore` (the Prodrome's SPEC §3), `/.heads` and `/.writes`, so its
  # record of heads and the mark of its last hold stay on the box. One that ignored more could keep
  # the store's objects out of every push, so a staged one holding anything else commits nothing.
  for ignore in $(GIT_INDEX_FILE="$JUDGED" git diff --cached --no-renames --name-only \
    | grep -E '^[a-z-]+/\.gitignore$' | grep -v '^requests/' || true); do
    case "$(GIT_INDEX_FILE="$JUDGED" git show ":$ignore" 2>/dev/null)" in
      $'/.heads\n/.writes') ;;
      *)
        echo "safe-push: $ignore is not the store's own \`/.heads\` and \`/.writes\`, so nothing is committed; scripts/app-bin.sh --deployed migrate-ignores --check says what each store lacks" >&2
        exit 1
        ;;
    esac
  done
  # Else an empty commit would fail a clean run.
  if GIT_INDEX_FILE="$JUDGED" git diff --cached --quiet; then
    echo "safe-push: nothing in $ADD_PATHS changed — nothing to commit"
    exit 0
  fi
  if [ "$DRY" -eq 1 ]; then
    echo "safe-push: dry run; it would commit:"
    GIT_INDEX_FILE="$JUDGED" git diff --cached --no-renames --name-status
    exit 0
  fi
  mv "$JUDGED" "$INDEX"
  git commit -m "$MSG"
  git push
  echo "safe-push: committed and pushed"
}

# Each public store's clone, pushed behind door 2 under roadmap-push.sh's own subject: the data
# push's message is private, and none of it reaches a public commit.
public_pushes() {
  local clone clones code=0 status
  clones="$(app instance public)" || { echo "safe-push: the public stores could not be listed" >&2; return 2; }
  while IFS= read -r clone; do
    [ -n "$clone" ] || continue
    if [ "$DRY" -eq 1 ]; then
      echo "safe-push: dry run; would push $clone through roadmap-push.sh"
      continue
    fi
    status=0
    "$SCRIPTS/roadmap-push.sh" "$clone" || status=$?
    # 1: door 2 refused, and roadmap-push.sh filed it on the upkeep chain, once.
    [ "$status" -eq 1 ] || [ "$status" -eq 0 ] || code=$status
  done <<<"$clones"
  return "$code"
}

# Outside any `||`, so `set -e` stops the data push at its first failure, as it always has.
set +e
(
  set -e
  data_push
)
data=$?
public_pushes
public=$?
exit $((data ? data : public))
