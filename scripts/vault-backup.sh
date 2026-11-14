#!/usr/bin/env bash
# Encrypted snapshot of the box's irreplaceable state -> Proton Drive.
#
# Holds what is neither in git nor rebuildable: the pass store, the GPG homedir, the OAuth
# caches, the lieer credentials, the assistant's memory, and the box-local stores (VAULTED).
#
# Excluded: the mail corpus (rebuildable), Bridge's vault.enc (per-box), anything in git, and
# Claude Code's session transcripts (they contain pasted credentials). A forgotten fact's words
# leave the backups within VAULT_KEEP runs: older snapshots are deleted, trash included.
#
# Encrypted to the vault key VAULT_RECIPIENT names; its private half lives off-box with the
# owner, so this timer carries no secret. Restore: gpg --import vault-restore-key.asc, then
# gpg -d < tarball. Rotation (which shreds every earlier snapshot) is scripts/vault-rotation.md.
set -euo pipefail

# Second resolution: the Drive CLI skips a same-named remote file and still exits 0.
STAMP=$(date -u +%Y%m%d-%H%M%S)
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
# Named for `<data>/FETCHER`, so bootstrap-box.sh on a successor finds it.
DATA=$(dirname "$(realpath "$SUZATARY_INSTANCE")")
BOX=$(tr -d '[:space:]' < "$DATA/FETCHER")
RECIPIENT=$(tr -d '[:space:]' < "$DATA/VAULT_RECIPIENT")
VAULT="$BOX-vault"
DRIVE="${PROTON_DRIVE_BIN:-$HOME/bin/proton-drive}"
OUT="$WORK/$VAULT-$STAMP.tar.gz.gpg"
REMOTE_DIR="/my-files/Backups/$VAULT"
DRIVE="${PROTON_DRIVE_BIN:-$HOME/bin/proton-drive}"
VAULT_KEEP=7

# Oldest first, by digits alone, as bootstrap-box.sh picks the newest.
snapshots() {
  grep -o "$VAULT-[0-9-]*\\.tar\\.gz\\.gpg" \
    | while read -r f; do printf '%s %s\n' "$(echo "$f" | tr -dc 0-9)" "$f"; done \
    | sort -n | cut -d' ' -f2
}

# bridge-v3 + cert ride along where a Bridge runs: its vault key also lives in the pass store, so
# restoring both may skip the re-login.
#
# The IMAP sync state is not here (rebuildable). The epochs must never be: a restored box starts
# its epochs above every one it answered (`suzatary_core::epoch`), and a week-old count would let
# tabs' replicas keep later words.
#
# The box-local stores (`suzatary_core::store::Kind`); app/cli/tests/guards/lists.rs pins this list.
VAULTED='secretary-lines-keys secretary-memory-texts secretary-digest-texts secretary-topics secretary-topic-aliases secretary-asks local secretary/attachments secretary/files'
# Claude Code names a project's directory by its path with every `/` a `-`.
tar czf "$WORK/state.tar.gz" --exclude="*.lock" -C "$HOME" \
  .password-store \
  .gnupg \
  ".claude/projects/${HOME//\//-}/memory" \
  $(cd "$HOME" && ls -d .config/protonmail/bridge-v3 .config/protonmail/bridge-cert.pem 2>/dev/null) \
  $(cd "$HOME" && ls -d .local/state/suzatary/oauth 2>/dev/null) \
  $(cd "$HOME" && ls .ms-oauth-*.json 2>/dev/null) \
  $(cd "$HOME" && ls Mail/*/.credentials.gmailieer.json 2>/dev/null) \
  -C "$DATA" $(cd "$DATA" && ls -d $VAULTED 2>/dev/null)

gpg --batch --yes --trust-model always -r "$RECIPIENT" \
  -o "$OUT" --encrypt "$WORK/state.tar.gz"

# create-folder fails if the folder exists.
"$DRIVE" filesystem create-folder /my-files Backups 2>/dev/null || true
"$DRIVE" filesystem create-folder /my-files/Backups "$VAULT" 2>/dev/null || true
"$DRIVE" filesystem upload "$OUT" "$REMOTE_DIR"

# The recovery recipe rides alongside, unencrypted (no secrets): cloning the repo needs the PAT,
# which is in the vault, so the recipe must be reachable before the repo is.
SELF_DIR=$(cd "$(dirname "$0")" && pwd)
"$DRIVE" filesystem upload -f replace "$SELF_DIR/bootstrap-box.sh" "$REMOTE_DIR"

# The upload can skip a same-named file and exit 0: trust only the remote listing.
"$DRIVE" filesystem list "$REMOTE_DIR" | grep -qF "$(basename "$OUT")" \
  || { echo "vault-backup: $(basename "$OUT") NOT found in remote listing after upload" >&2; exit 1; }

# Prune only after the read-back. `trash` then `delete` from /trash, never `empty-trash`: the
# trash holds the owner's whole Drive.
STALE=$("$DRIVE" filesystem list "$REMOTE_DIR" | snapshots | head -n "-$VAULT_KEEP")
for old in $STALE; do
  "$DRIVE" filesystem trash "$REMOTE_DIR/$old"
  "$DRIVE" filesystem delete "/trash/$old"
done
KEPT=$("$DRIVE" filesystem list "$REMOTE_DIR" | snapshots | wc -l)
[ "$KEPT" -le "$VAULT_KEEP" ] \
  || { echo "vault-backup: $KEPT snapshots in $REMOTE_DIR after pruning to $VAULT_KEEP" >&2; exit 1; }

echo "vault-backup: uploaded $(basename "$OUT") ($(stat -c%s "$OUT") bytes) + bootstrap-box.sh; kept $KEPT, deleted $(wc -w <<<"$STALE")"
