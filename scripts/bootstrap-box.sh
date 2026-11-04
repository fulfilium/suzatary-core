#!/usr/bin/env bash
# Bootstrap a box's successor from the Proton Drive vault. ONE key, ONE login.
#
# Every oddity below guards a failure a recovery drill found:
#   - download + decrypt happen BEFORE the store swap (the Drive session lives
#     in the bootstrap pass store — swapping first strands the download);
#   - newest-snapshot selection sorts on digits only (legacy un-timestamped
#     names beat timestamped ones in plain sort: '-' < '.');
#   - chown after extract (tar-as-root preserves the archive's original uid,
#     and gpg-agent refuses private keys owned by someone else);
#   - `gpgconf --kill all` after the swap (keyboxd keeps serving the OLD
#     public-key DB across a homedir swap; killing just gpg-agent is not enough);
#   - clone clears inherited credential helpers and uses GIT_ASKPASS (the old
#     image shipped a gh helper holding a dead login that outranked everything).
#
# This script is also uploaded unencrypted beside the vault on Drive (vault-backup.sh), since
# cloning the repo needs the PAT the vault holds. No secrets in here.
#
# Prerequisites: proton-drive CLI installed and `auth login` done. On a fresh box `suzatary
# auth` first creates a disposable pass/gpg store marked ~/.password-store/.suzatary-bootstrap,
# which this script may replace. The vault restore key is the one key.
#
# Usage: bootstrap-box.sh /path/to/vault-restore-key.asc <box> [<owner>/<repo>...]
#
# <box> is the fetcher whose `<box>-vault` to restore; each <owner>/<repo> is cloned into
# ~/Documents with the restored PAT.
#
# Afterwards: Bridge may need no re-login; seed mail via `suzatary sync`; `proton-drive auth
# login` again if wanted (the session died with the bootstrap store); `suzatary up` refuses
# unless this box is named in the data repository's FETCHER (single-fetcher).
set -euo pipefail

KEY="${1:?usage: bootstrap-box.sh /path/to/vault-restore-key.asc <box> [<owner>/<repo>...]}"
BOX="${2:?usage: bootstrap-box.sh /path/to/vault-restore-key.asc <box> [<owner>/<repo>...]}"
shift 2
REPOS=("$@")
DRIVE="${PROTON_DRIVE_BIN:-$HOME/bin/proton-drive}"
REMOTE_DIR="/my-files/Backups/$BOX-vault"

command -v gpg >/dev/null || { echo "need gpg on PATH"; exit 1; }
command -v git >/dev/null || { echo "need git on PATH"; exit 1; }
"$DRIVE" filesystem list / >/dev/null || { echo "proton-drive not authed — run: $DRIVE auth login"; exit 1; }

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

# Phase 1 needs the current (possibly bootstrap) store.
NEWEST=$("$DRIVE" filesystem list "$REMOTE_DIR" \
  | grep -o "$BOX-vault-[0-9-]*\\.tar\\.gz\\.gpg" \
  | while read -r f; do printf '%s %s\n' "$(echo "$f" | tr -dc 0-9)" "$f"; done \
  | sort -n | tail -1 | cut -d' ' -f2)
[ -n "$NEWEST" ] || { echo "no snapshot found in $REMOTE_DIR"; exit 1; }
echo "bootstrap: restoring from $NEWEST"
"$DRIVE" filesystem download "$REMOTE_DIR/$NEWEST" "$WORK/"

gpg --import "$KEY"
gpg --batch -d "$WORK/$NEWEST" > "$WORK/state.tar.gz"
tar tzf "$WORK/state.tar.gz" >/dev/null   # decryption + integrity proven before any swap

# The marker is read once: the first pass moves the store that holds it.
bootstrap=0
[ ! -f "$HOME/.password-store/.suzatary-bootstrap" ] || bootstrap=1
for d in .password-store .gnupg; do
  if [ -e "$HOME/$d" ]; then
    if [ "$bootstrap" = 1 ]; then
      mv "$HOME/$d" "$HOME/$d.bootstrap"
    elif [ -n "$(ls -A "$HOME/$d" 2>/dev/null | grep -v openpgp-revocs.d)" ]; then
      echo "bootstrap: $HOME/$d exists and is not a marked bootstrap store — refusing to overwrite."
      echo "  (fresh boxes get a disposable store via 'suzatary auth'; merging into a lived-in HOME is human work)"
      exit 1
    fi
  fi
done

tar xzf "$WORK/state.tar.gz" -C "$HOME"
chown -R "$(id -u):$(id -g)" "$HOME/.gnupg" "$HOME/.password-store"
chmod 700 "$HOME/.gnupg" "$HOME/.password-store"
gpgconf --kill all 2>/dev/null || true    # keyboxd MUST restart onto the restored DB

mkdir -p "$HOME/.config/secretspec"
[ -f "$HOME/.config/secretspec/config.toml" ] || cat > "$HOME/.config/secretspec/config.toml" <<'EOF'
[defaults]
provider = "pass"
profile = "default"
EOF

ASKPASS="$WORK/askpass"
cat > "$ASKPASS" <<'EOF'
#!/bin/sh
case "$1" in
  *sername*) echo "$BOOTSTRAP_OWNER" ;;
  *) pass show "secretspec/suzatary/default/$BOOTSTRAP_TOKEN" | head -1 ;;
esac
EOF
chmod +x "$ASKPASS"
mkdir -p "$HOME/Documents"
# Each token reaches only its own repositories, so a clone tries one, then the other.
for repo in "${REPOS[@]}"; do
  name="${repo#*/}"
  for token in SUZATARY_DATA_PUSH_TOKEN SUZATARY_REPOS_TOKEN; do
    [ ! -d "$HOME/Documents/$name" ] || break
    BOOTSTRAP_OWNER="${repo%%/*}" BOOTSTRAP_TOKEN="$token" GIT_ASKPASS="$ASKPASS" GIT_TERMINAL_PROMPT=0 \
      git -c credential.helper= clone "https://github.com/$repo.git" "$HOME/Documents/$name" || true
  done
  [ -d "$HOME/Documents/$name" ] || { echo "bootstrap: neither token clones $repo"; exit 1; }
  if [ -x "$HOME/Documents/$name/scripts/git-credential-secretspec.sh" ]; then
    git -C "$HOME/Documents/$name" config credential.helper \
      "$HOME/Documents/$name/scripts/git-credential-secretspec.sh"
  fi
done

echo
echo "bootstrap: done. Restored: pass store, GPG keys, OAuth caches, lieer creds, Bridge state, assistant memory; cloned: ${REPOS[*]:-nothing}."
echo "bootstrap: 'suzatary up' will refuse unless this box is named in the data repository's FETCHER — that refusal is the single-fetcher invariant working."
