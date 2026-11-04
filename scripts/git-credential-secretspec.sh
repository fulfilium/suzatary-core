#!/usr/bin/env bash
# Git credential helper for the data repository's push token, resolved from `pass` on demand.
#
# Never put the token in a remote URL: .git/config is inside the triage agent's Read scope.
#
# Wire it in the data repository alone, through the units' link to the deployed app: never a dev
# checkout (unreviewed code in the push path) and never a store path (the next collection
# deletes it). The empty entry clears global helpers, gh's among them:
#   git -C <data repository> config credential.https://github.com.helper ''
#   git -C <data repository> config --add credential.https://github.com.helper \
#     "$SUZATARY_ROOTS/app/scripts/git-credential-secretspec.sh"
set -euo pipefail

# Only `get`: pass owns the value. Exit 0 on store/erase; nonzero makes git treat the helper as
# broken.
[ "${1:-}" = "get" ] || exit 0

# `$0` is absolute: the helper is wired by absolute path.
REPO="${SUZATARY_REPO:-$(cd "$(dirname "$0")/.." && pwd)}"

# --reason satisfies secretspec.toml's require_reason="agents" and lands in the audit log.
PAT=$(secretspec get SUZATARY_DATA_PUSH_TOKEN \
  --file "$REPO/secretspec.toml" \
  --reason "git push/fetch of the data repository against github.com")

# GitHub ignores the username of token auth, but git needs one: the URL's, else the generic one.
user="x-access-token"
while IFS='=' read -r key value; do
  [ "$key" = "username" ] && [ -n "$value" ] && user="$value"
done
echo "username=$user"
echo "password=$PAT"
