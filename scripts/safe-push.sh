#!/usr/bin/env bash
# The data push: the only path by which what the box wrote reaches the data repository. It
# checks the data alone (the allowlist, the append-only walk, `suzatary verify`) and builds
# nothing: the code it runs already passed the merge gate.
#
# Usage: safe-push.sh (--dry | <commit-message>)
set -euo pipefail

REPO="${SUZATARY_REPO:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
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
# The three lists are the stores' rows (`suzatary_core::store::Kind`), spelled out so a quiet
# pass runs no program but git; app/cli/tests/guards/lists.rs pins each.
ALLOWED_PATTERN='^(events/(objects/[0-9a-f]{64}\.py|\.lock|\.gitattributes)|events-suzatary/(objects/[0-9a-f]{64}\.py|\.lock|\.gitattributes)|inbox/(objects/[0-9a-f]{64}\.py|\.gitattributes)|secretary-lines/(objects/[0-9a-f]{64}\.py|\.gitattributes)|memory/(objects/[0-9a-f]{64}\.py|\.gitattributes)|digests/(objects/[0-9a-f]{64}\.py|\.gitattributes)|decisions/(objects/[0-9a-f]{64}\.py|\.gitattributes)|state\.json|requests/([0-9a-f]{64}\.json|\.gitignore))$'
ADD_PATHS='events/ events-suzatary/ inbox/ secretary-lines/ memory/ digests/ decisions/ state.json requests/'
WALKED='events/objects/ events-suzatary/objects/ inbox/objects/ secretary-lines/objects/ memory/objects/ digests/objects/ decisions/objects/ requests/'
# Never while the data tree is this checkout, where those files are a human's.
if [ "$DATA" != "$REPO" ]; then
  DATA_EXTRA='instance.toml mbsyncrc notes/ briefings/'
  DATA_EXTRA_PATTERN='^(instance\.toml|mbsyncrc|notes/.*|briefings/.*)$'
fi

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
# is a history rewrite and must not be pushed. Every store in WALKED, not just events/.
# shellcheck disable=SC2086
REWRITES=$(git status --porcelain -- $WALKED | grep -vE '^(\?\?|A )' || true)
if [ -n "$REWRITES" ]; then
  echo "safe-push: APPEND-ONLY VIOLATION under an events objects/ — a modification or" >&2
  echo "  deletion of a chain object is a history rewrite and will not be pushed:" >&2
  echo "$REWRITES" >&2
  exit 1
fi

# The stores' fsck; any problem aborts.
app verify

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
