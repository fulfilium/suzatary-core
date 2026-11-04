#!/usr/bin/env bash
# Encrypted snapshot of the box's irreplaceable state -> Proton Drive.
#
# WHAT: the small set of things that are neither in git nor rebuildable from a
# provider: the pass store, the GPG homedir (both box keys), the OAuth caches
# (restoring these skips ~7 re-auth dances on a new box), the lieer
# credentials, and the assistant's memory directory.
#
# DELIBERATELY EXCLUDED: the mail corpus (rebuildable),
# Bridge's vault.enc (per-box; re-login is the documented recovery), anything
# already in a git repo, and Claude Code's own session transcripts (they contain
# pasted credentials and do not belong in any backup). The words of the
# secretary's memory (`secretary-memory-texts/`), the digest's summaries
# (`secretary-digest-texts/`), the keys of the conversations' locked words
# (`secretary-lines-keys/`, 32 bytes each), the topics and their aliases
# (`secretary-topics/`, `secretary-topic-aliases/`, whose names are words) and the
# files its conversations hold ARE here: nothing else holds them. The conversations and the memory's and the digest's
# histories are stores the data tree pushes, so they are not. The
# box-local tier (`local/`: the person's prose, the leak guard's term lists,
# the mail's private ledger) is here for the same reason: no push carries it. A
# forgotten fact's words are deleted from the texts, so the next snapshot does
# not hold them; a snapshot taken before still does, until it is deleted:
# the run keeps the newest VAULT_KEEP snapshots and deletes every older one
# from the Drive, its trash included, so a forgotten fact's words leave the
# backups within VAULT_KEEP runs.
#
# CRYPTO: asymmetric, to the dedicated vault key, which the data repository's
# VAULT_RECIPIENT names (the code names no deployment's key). The
# PRIVATE half lives OFF-BOX with the owner (~/vault-restore-key.asc); the box
# needs only the public half to encrypt, so this timer
# carries no secret. Restore: gpg --import vault-restore-key.asc, then
# gpg -d < tarball. A vault encrypted to a key stored inside the vault would
# be circular — hence the separate keypair.
# Rotating it, which shreds every earlier snapshot at once, is
# scripts/vault-rotation.md.
set -euo pipefail

# Second-resolution stamp: same-day reruns must not collide — the CLI skips
# same-named remote files, so a rerun would silently upload nothing while the
# script reported success.
STAMP=$(date -u +%Y%m%d-%H%M%S)
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
# The vault is named for the fetcher it backs up, as `<data>/FETCHER` names it,
# so bootstrap-box.sh on a successor finds it by that name.
DATA=$(dirname "$(realpath "$SUZATARY_INSTANCE")")
BOX=$(tr -d '[:space:]' < "$DATA/FETCHER")
RECIPIENT=$(tr -d '[:space:]' < "$DATA/VAULT_RECIPIENT")
VAULT="$BOX-vault"
DRIVE="${PROTON_DRIVE_BIN:-$HOME/bin/proton-drive}"
OUT="$WORK/$VAULT-$STAMP.tar.gz.gpg"
REMOTE_DIR="/my-files/Backups/$VAULT"
DRIVE="${PROTON_DRIVE_BIN:-$HOME/bin/proton-drive}"
VAULT_KEEP=7

# The snapshots a listing names, oldest first: by their digits alone, as
# bootstrap-box.sh picks the newest (an un-timestamped legacy name sorts
# below a timestamped one in plain sort).
snapshots() {
  grep -o "$VAULT-[0-9-]*\\.tar\\.gz\\.gpg" \
    | while read -r f; do printf '%s %s\n' "$(echo "$f" | tr -dc 0-9)" "$f"; done \
    | sort -n | cut -d' ' -f2
}

# bridge-v3 + cert ride along where the box runs a Bridge (listed, so a box
# reading no Proton mail snapshots without them), because Bridge's vault key
# ALSO lives in the pass store (its keyring backend) — restoring both may
# bring Bridge up on a new box with no re-login at all. If Proton invalidates the session
# server-side, the documented fallback (interactive re-login) still applies.
#
# THE MICROSOFT GRANTS are this program's own typed store at
# $XDG_STATE_HOME/suzatary/oauth/<slug>.json, one file per account, 0600 in a
# 0700 directory. msal's older caches at ~/.ms-oauth-<slug>.json are listed
# too, so a box restored from an older vault still finds whatever it had; they
# are simply absent on a box that has cut over.
#
# The IMAP SYNC STATE under .local/state/suzatary/imap/ is deliberately NOT
# here. It is UID bookkeeping over a maildir that is itself a rebuildable cache
# — losing it costs one re-fetch of a seven-day window, not a credential.
#
# The EPOCHS under .local/state/suzatary/epochs/ are deliberately NOT here
# either, and must never be. A box restored from a vault must start its epochs
# as a fresh box does, above every one it answered (`suzatary_core::epoch`): a
# week-old count brought back would answer tabs with epochs below the ones they
# hold, and their replicas would keep the later words.
#
# The box-local stores, as their rows (`suzatary_core::store::Kind`) place them
# for this deployment; the guard `app/cli/tests/guards/lists.rs` holds the list
# equal to that derivation.
VAULTED='secretary-lines-keys secretary-memory-texts secretary-digest-texts secretary-topics secretary-topic-aliases local secretary/attachments secretary/files'
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

# create-folder is idempotent-ish: it fails if the folder exists, which is fine.
"$DRIVE" filesystem create-folder /my-files Backups 2>/dev/null || true
"$DRIVE" filesystem create-folder /my-files/Backups "$VAULT" 2>/dev/null || true
"$DRIVE" filesystem upload "$OUT" "$REMOTE_DIR"

# The recovery recipe rides alongside, unencrypted (it holds no secrets):
# cloning the repo needs the PAT, the PAT is in the vault, and the vault
# recipe is this script — so the recipe must be reachable before the repo is.
# `-f replace` so an updated recipe overwrites the stale one; identical
# content is skipped automatically.
SELF_DIR=$(cd "$(dirname "$0")" && pwd)
"$DRIVE" filesystem upload -f replace "$SELF_DIR/bootstrap-box.sh" "$REMOTE_DIR"

# Trust nothing that wasn't read back: the upload above can skip a
# same-named file and exit 0. The snapshot is only "uploaded" if a remote
# listing shows it.
"$DRIVE" filesystem list "$REMOTE_DIR" | grep -qF "$(basename "$OUT")" \
  || { echo "vault-backup: $(basename "$OUT") NOT found in remote listing after upload" >&2; exit 1; }

# Prune only once the new snapshot is read back, so a failed upload never
# costs an old one. `trash` then `delete` from /trash, never `empty-trash`:
# the trash is the owner's whole Drive's, and only the vault's are ours.
STALE=$("$DRIVE" filesystem list "$REMOTE_DIR" | snapshots | head -n "-$VAULT_KEEP")
for old in $STALE; do
  "$DRIVE" filesystem trash "$REMOTE_DIR/$old"
  "$DRIVE" filesystem delete "/trash/$old"
done
KEPT=$("$DRIVE" filesystem list "$REMOTE_DIR" | snapshots | wc -l)
[ "$KEPT" -le "$VAULT_KEEP" ] \
  || { echo "vault-backup: $KEPT snapshots in $REMOTE_DIR after pruning to $VAULT_KEEP" >&2; exit 1; }

echo "vault-backup: uploaded $(basename "$OUT") ($(stat -c%s "$OUT") bytes) + bootstrap-box.sh; kept $KEPT, deleted $(wc -w <<<"$STALE")"
