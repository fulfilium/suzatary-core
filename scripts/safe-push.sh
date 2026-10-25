#!/usr/bin/env bash
# The data push: the ONLY path by which what the box wrote (the agent's
# replicas, the server's stores, the dedup state) reaches the data repository.
# A push is a union of objects into that repository, so everything it checks
# is a function of the data: the allowlist, the append-only walk, and
# `suzatary verify` of every store. It builds nothing and runs no suite: the
# code it runs is a commit the merge gate already passed (scripts/train.sh),
# and a verdict about the code is not a verdict about the data. Callers:
# scripts/hourly-triage.sh, the voice server.
#
# Usage: safe-push.sh (--dry | <commit-message>)
set -euo pipefail

# SUZATARY_REPO is set by the systemd unit (nix/nixos-module.nix, the ONE
# place this box says where it is); the fallback resolves this script's own
# checkout, so it is runnable by hand from any clone rather than assuming
# this one's (ROADMAP.md, "a second host").
REPO="${SUZATARY_REPO:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
cd "$REPO"

# --dry is this path up to its commit, over the tree as it is: it stages into
# the judged copy of the index, runs every check on the data, says what it
# would commit, and commits, moves and pushes nothing. A push is the same
# path with the commit and the push after it. The merge gate runs it against
# the instance's data repository, so a path it cannot stage, a file its
# allowlist refuses or a store that does not verify is red there first.
DRY=0
if [ "${1:-}" = --dry ]; then
  DRY=1
  MSG=""
  # No transport at all: a remote is neither read nor written, whatever runs.
  export GIT_ALLOW_PROTOCOL=none
elif [ "${1:0:2}" = -- ]; then
  # A flag this script does not know is never a commit message.
  echo "safe-push: unknown flag ${1}; usage: safe-push.sh (--dry | <commit-message>)" >&2
  exit 64
else
  MSG="${1:?usage: safe-push.sh (--dry | <commit-message>)}"
fi

# THE PROGRAM THAT VERIFIES is the deployed one, scripts/app-bin.sh's. A dry
# run may name another in SUZATARY_BIN (the gate's build of the tree it
# gates); a push refuses one, so what it trusts is never a variable away.
if [ -n "${SUZATARY_BIN:-}" ] && [ "$DRY" -eq 0 ]; then
  echo "safe-push: SUZATARY_BIN names the verifying program for --dry only; refusing" >&2
  exit 64
fi
app() {
  if [ -n "${SUZATARY_BIN:-}" ]; then "$SUZATARY_BIN" "$@"; else "$REPO/scripts/app-bin.sh" "$@"; fi
}

# THE DATA IS ITS OWN TREE. The instance file names where the chains,
# state.json and the briefings live; `SUZATARY_INSTANCE` names the instance
# file, and its directory is the data tree, so the agent's commit lands in the
# data repository and the code repository never sees a chain object. It is
# REQUIRED, loudly: `suzatary verify` reads the stores through it, and a push
# that guessed its data tree would verify one tree and commit another.
INSTANCE="${SUZATARY_INSTANCE:-$REPO/instance.toml}"
if [ ! -f "$INSTANCE" ]; then
  echo "safe-push: no instance file at $INSTANCE" >&2
  echo "  Set SUZATARY_INSTANCE=/path/to/instance.toml (the data tree's), or run" >&2
  echo "  from a checkout that holds one. Refusing rather than pushing blind." >&2
  exit 64
fi
export SUZATARY_INSTANCE="$INSTANCE"
# realpath, not dirname: the code checkout's `instance.toml` is a link to the
# data tree's, and the data tree is where the file really lives.
DATA="$(dirname "$(realpath "$INSTANCE")")"
# What the agent may stage BEYOND its chain set, and only once the data is a
# tree of its own: the files a human edits there. In the code checkout they
# stay a human's to commit with git.
DATA_EXTRA=''
DATA_EXTRA_PATTERN='^$'
# The tree this run commits from is the data tree; `$REPO` stays the checkout
# the tools run from.
cd "$DATA"

# Only commit files agent sessions are supposed to touch. Anything else in the
# diff means something unexpected happened - abort rather than push it.
#
# NARROW ON PURPOSE: the only callers are agent-driven, and the unattended
# agent reads attacker-controlled mail all day. If this path committed code,
# an agent that rewrote scripts/hourly-triage.sh would have it pushed, and the
# NEXT run would execute that as the trusted wrapper, with the Bash the agent
# itself is denied. Code reaches master through the merge gate and the train
# (scripts/train.sh), never through this path.
#
# The agent's set: dedup state and the chains. Objects are
# named by their own sha256 (hence the [0-9a-f]{64} shape), and the tips are
# DERIVED from the objects, so no run writes `HEAD` or `refs/`. The agent
# stages nothing rendered.
# `events-suzatary/` is the second chain (docs/ARCHITECTURE.md §10.2), and a
# chain the trusted push path would not commit is a chain whose objects live
# only on this box.
# `inbox/objects/` is written by the server (the owner's
# taps, the secretary's proposals, sends included) and nothing else commits
# it; a remember card there names its words, which are in
# `secretary-memory-texts/` like a fact's. `secretary-lines/objects/` is the conversations (the server's
# rounds and `secretary review`'s): each line's shape, and its words locked
# under keys that live in `secretary-lines-keys/`, box-local and in the vault,
# never staged and refused by the pattern, so destroying a key erases words a
# pushed object still holds as ciphertext. `memory/objects/` is the memory's
# HISTORY: which facts were remembered and forgotten, by id, naming each
# fact's words by a salted hash and never holding them. The words are
# `secretary-memory-texts/`, box-local and in the vault, deleted when a fact
# is forgotten; nothing here stages them and the pattern refuses them, since
# a pushed object cannot be forgotten. `digests/objects/` is the digest's
# history: which nodes of the conversations' summary tree were summarised,
# naming each summary's words by a salted hash; the words are
# `secretary-digest-texts/`, box-local like a fact's, and refused here for
# the same reason. `decisions/objects/` is the recorded
# decisions (the CLI's and the owner's taps on the secretary's proposals).
# Content-addressed and append-only like a chain, so the rewrite check below
# walks it too. Each store writes its own `.gitattributes` once, telling git
# never to rewrite an object's bytes, and it rides with the store.
# The topics (`secretary-topics/`, `secretary-topic-aliases/`) are never
# staged and the pattern refuses them: a topic's name is words, and a pushed
# object cannot be erased, so both stores stay on the box and in the vault.
# `requests/` is each request a device signed and the server performed, kept
# once under its name as provenance only (who asked, as what actor, when,
# and the chain objects it wrote; never a body or an answer, which stay on
# the box): written once and walked like an object store. Its own
# `.gitignore` keeps a record's write in progress out.
# The three lists are the stores' rows (`suzatary_core::store::Kind`) for this
# deployment, spelled out so a quiet pass runs no program but git; the guard
# `app/cli/tests/guards/lists.rs` holds each equal to its derivation.
ALLOWED_PATTERN='^(events/(objects/[0-9a-f]{64}\.py|\.lock|\.gitattributes)|events-suzatary/(objects/[0-9a-f]{64}\.py|\.lock|\.gitattributes)|inbox/(objects/[0-9a-f]{64}\.py|\.gitattributes)|secretary-lines/(objects/[0-9a-f]{64}\.py|\.gitattributes)|memory/(objects/[0-9a-f]{64}\.py|\.gitattributes)|digests/(objects/[0-9a-f]{64}\.py|\.gitattributes)|decisions/(objects/[0-9a-f]{64}\.py|\.gitattributes)|state\.json|requests/([0-9a-f]{64}\.json|\.gitignore))$'
ADD_PATHS='events/ events-suzatary/ inbox/ secretary-lines/ memory/ digests/ decisions/ state.json requests/'
WALKED='events/objects/ events-suzatary/objects/ inbox/objects/ secretary-lines/objects/ memory/objects/ digests/objects/ decisions/objects/ requests/'
# ONCE THE DATA IS A TREE OF ITS OWN, the human-edited data files ride with
# the agent's commit too — the data repository is one tree and its history
# is machine-written — but never while the data tree is still this
# checkout, where those same files are code-adjacent and a human's.
if [ "$DATA" != "$REPO" ]; then
  DATA_EXTRA='instance.toml mbsyncrc notes/ briefings/'
  DATA_EXTRA_PATTERN='^(instance\.toml|mbsyncrc|notes/.*|briefings/.*)$'
fi

# Serialize pushes: the voice server and the hourly timer can both land here.
# One lock per repository, inside its git directory: two trees share no index
# or remote, so nothing orders a push to one after a push to the other, and a
# throwaway repository a test pushes from cannot wait on the box's own push.
exec 9>"$(git rev-parse --git-dir)/suzatary-push.lock"
flock 9

# THE SET THIS RUN STAGES: each path of the lists above that names a file git
# could stage, in the index (a store removed whole still stages its deletion)
# or in the work tree and not ignored, as a glob pathspec.
# A path naming none is a pathspec `git add` refuses as fatal (a store before
# its first object: `digests/` and `requests/`, 2026-10-06, every busy pass
# for hours), while what it would stage is nothing, the identity of the union
# being staged; so it is left out, one line each.
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

# WHAT THIS RUN WOULD STAGE, asked before anything is verified. The question
# is its own set: a tree a human keeps dirty, or one holding only files
# outside the set, has nothing for it to commit, and verifying the stores
# over that every five minutes would be ritual (the quiet triage pass calls
# this on every tick). Ignored files are never listed.
if [ "${#STAGE[@]}" -eq 0 ] || [ -z "$(git status --porcelain -- "${STAGE[@]}")" ]; then
  echo "safe-push: nothing to commit"
  exit 0
fi

# APPEND-ONLY ENFORCEMENT (events/objects/). The chain is a Merkle log: objects
# are content-addressed and immutable, only ever ADDED. A MODIFY or DELETE under
# events/objects/ is therefore a history rewrite — a tampered or dropped object,
# exactly what the store's on-read verification is meant to catch — and it must
# NOT ride to the remote through this trusted push. Untracked/added (git status
# codes '??' and 'A ') are the only legitimate object states; anything else
# (' M', 'MM', ' D', 'D ', 'R ', etc.) aborts loudly.
# Every chain (§10.2): the rule is a property of a Merkle log and not of one
# directory's name, so a second chain the check did not walk would be a second
# chain a rewrite could ride out through.
# shellcheck disable=SC2086
REWRITES=$(git status --porcelain -- $WALKED | grep -vE '^(\?\?|A )' || true)
if [ -n "$REWRITES" ]; then
  echo "safe-push: APPEND-ONLY VIOLATION under an events objects/ — a modification or" >&2
  echo "  deletion of a chain object is a history rewrite and will not be pushed:" >&2
  echo "$REWRITES" >&2
  exit 1
fi

# CHAIN INTEGRITY GATE. A broken chain (missing object, bad hash) never gets
# pushed — `verify` is the store's fsck, and a nonempty
# problem list is a hard abort.
app verify

CHANGED=$(git status --porcelain | awk '{print $2}')
UNEXPECTED=""
for f in $CHANGED; do
  echo "$f" | grep -qE "$ALLOWED_PATTERN|$DATA_EXTRA_PATTERN" || UNEXPECTED="$UNEXPECTED $f"
done
if [ -n "$UNEXPECTED" ]; then
  # REPORTED, NEVER AN ABORT. The containment never came from this check:
  # `git add -A -- "${STAGE[@]}"` below stages ONLY the agent's set, and the
  # staged set is judged by the pattern, so an unexpected file cannot ride
  # along whether we abort or not. Aborting would only fail the unit every
  # five minutes for as long as a human had an unrelated file open, and an
  # alarm over somebody else's working copy is how a real alarm gets trained
  # into background noise.
  echo "safe-push: files outside the agent's set are modified (NOT committed):$UNEXPECTED"
  echo "safe-push: committing only $ADD_PATHS — expected while a human is working here."
fi

# THE INDEX IS JUDGED BEFORE IT IS THE INDEX. STAGE is staged into a copy of
# the index beside it, and every path that copy differs from HEAD by must
# match the allowlist, or nothing is committed and the index is as it was: so
# what a commit carries is bounded by ALLOWED_PATTERN itself, never by ADD_PATHS
# staying in step with it, and what is committed is exactly what was judged
# (the server writes objects meanwhile, so staging again would not be).
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
# A run that changed nothing in its own set is a clean run, not a failure.
# The early exit above already asks the agent's set, so this is the belt for
# whatever status lists and staging does not change; without it, a run with
# nothing to say would die here on an empty commit — an alarm about nothing.
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
