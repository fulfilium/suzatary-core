#!/usr/bin/env bash
# Makes the Codespace `scripts/train.sh gate --remote <name>` runs the gate's tree half in, and
# prints its name:
#   machine       standardLinux32gb (4 cores, 16 GB, 32 GB of disk)
#   idle timeout  10 minutes, the backstop for a train that could not stop it
#   retention     30 days stopped, so its warm target outlives a quiet fortnight
# Run it at a terminal: the machine is billed while it runs. It makes one only in a repository
# the logged-in account owns (never an organisation's), and only where no gate Codespace exists.
#
#   scripts/gate-codespace.sh [<repo>]   (default: this checkout's origin)
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."
name=suzatary-gate
repo="${1:-$(gh repo view --json nameWithOwner -q .nameWithOwner)}"
login="$(gh api user -q .login)"
owner="$(gh repo view "$repo" --json owner -q .owner.login)"
if [[ "$owner" != "$login" ]]; then
  echo "gate-codespace.sh: $repo belongs to $owner, not to $login, the account logged in: a gate's Codespace is made only in the account's own repository" >&2
  exit 1
fi
existing="$(gh codespace list -R "$repo" --json displayName,name -q ".[] | select(.displayName == \"$name\") | .name")"
if [[ -n "$existing" ]]; then
  echo "gate-codespace.sh: $repo has a gate Codespace already: $existing" >&2
  echo "  use it (scripts/train.sh gate --remote $existing), or delete it first (gh codespace delete -c $existing)" >&2
  exit 1
fi
gh codespace create --repo "$repo" --branch master \
  --devcontainer-path .devcontainer/devcontainer.json \
  --machine standardLinux32gb --idle-timeout 10m --retention-period 720h \
  --display-name "$name"
