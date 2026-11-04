#!/usr/bin/env bash
# `suzatary auth`: interactive credential setup for a box. Run by the owner at a terminal, never
# by the agent; secrets it touches go to the pass store or nowhere.
set -euo pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"

echo "suzatary auth — what should this box become?"
echo "  [1] fetcher   restore everything from the Proton Drive vault (needs your restore key)"
echo "  [2] verify    report what this box already has"
read -rp "> " ROLE

case "$ROLE" in
  1)
    "$REPO/scripts/install-proton-drive.sh"
    # Drive sessions live in the pass store; a fresh box gets a disposable one, which the vault's
    # store replaces.
    BOOTSTRAPPED=""
    if [ ! -d "$HOME/.password-store" ]; then
      echo "auth: creating a disposable bootstrap keyring for the Drive session"
      gpg --batch --generate-key <<'EOF'
%no-protection
Key-Type: eddsa
Key-Curve: ed25519
Key-Usage: cert,sign
Subkey-Type: ecdh
Subkey-Curve: cv25519
Subkey-Usage: encrypt
Name-Real: Suzatary Bootstrap
Name-Email: bootstrap@localhost
Expire-Date: 0
%commit
EOF
      pass init "Suzatary Bootstrap" >/dev/null
      # bootstrap-box.sh may replace only a store carrying this marker, and only after the
      # download and decrypt, since the Drive session lives in it.
      touch "$HOME/.password-store/.suzatary-bootstrap"
      BOOTSTRAPPED=1
    fi
    "$HOME/bin/proton-drive" auth login
    read -rp "path to vault-restore-key.asc (from your password manager): " KEY
    [ -f "$KEY" ] || { echo "auth: no such file: $KEY"; exit 1; }
    read -rp "the fetcher whose vault to restore (its FETCHER name): " BOX
    [ -n "$BOX" ] || { echo "auth: name the fetcher"; exit 1; }
    read -rp "repositories to clone (owner/name, space-separated): " -a CLONE
    "$REPO/scripts/bootstrap-box.sh" "$KEY" "$BOX" "${CLONE[@]}"
    cd "$REPO" && secretspec check || true
    if [ -n "$BOOTSTRAPPED" ]; then
      echo "auth: run '$HOME/bin/proton-drive auth login' once more — the restored"
      echo "      pass store replaced the bootstrap one that held the session."
      rm -rf "$HOME/.password-store.bootstrap" "$HOME/.gnupg.bootstrap"
    fi
    echo "auth: fetcher restored. Next: suzatary up"
    ;;

  2)
    echo "== secretspec:"; (cd "$REPO" && secretspec check) || true
    echo "== proton-drive session:"
    if [ -x "$HOME/bin/proton-drive" ] && "$HOME/bin/proton-drive" filesystem list / >/dev/null 2>&1; then
      echo "authed"
    else
      echo "absent or not authed"
    fi
    echo "== mail store:"; [ -d "$HOME/Mail/.notmuch" ] && echo "present" || echo "absent"
    ;;

  *) echo "auth: unknown role"; exit 1 ;;
esac
