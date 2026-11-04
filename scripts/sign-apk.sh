#!/usr/bin/env bash
# Sign an unsigned Suzatary APK, outside the nix store: the store is world-readable and copied
# to substituters, so a key in a derivation is a key published. The keystore lives in
# secretspec (box) or a GitHub Actions secret (CI), never in git.
#
# Never rotate the key casually: Android refuses an update signed with a different key, and
# uninstalling loses the app's localStorage. Losing the keystore costs a reinstall on every
# device. Back it up with the vault.
#
# Usage:  sign-apk.sh <unsigned.apk> <signed.apk>
# Needs:  apksigner on PATH (`nix run .#apk-sign -- …` supplies it), and
#         SUZATARY_KEYSTORE + SUZATARY_KEYSTORE_PASSWORD in the environment.
#
# ONE-TIME KEY CREATION, in a plain terminal, never in an agent session:
#
#   keytool -genkeypair -v \
#     -keystore ~/suzatary-release.p12 -storetype PKCS12 \
#     -alias suzatary -keyalg RSA -keysize 4096 -validity 10000 \
#     -dname "CN=Suzatary, O=bmabsout, C=PT"
#   secretspec set ANDROID_KEYSTORE_PASSWORD      # the password keytool asked for
#   base64 -w0 ~/suzatary-release.p12 | secretspec set ANDROID_KEYSTORE_B64
#
# RSA (PKCS#1 v1.5) signatures are deterministic, so re-signing an unchanged APK yields an
# identical file; ECDSA would differ every run.
set -euo pipefail

IN="${1:?usage: sign-apk.sh <unsigned.apk> <signed.apk>}"
OUT="${2:?usage: sign-apk.sh <unsigned.apk> <signed.apk>}"

: "${SUZATARY_KEYSTORE:?sign-apk: SUZATARY_KEYSTORE is unset (path to the PKCS12 keystore)}"
: "${SUZATARY_KEYSTORE_PASSWORD:?sign-apk: SUZATARY_KEYSTORE_PASSWORD is unset}"

if [ ! -r "$SUZATARY_KEYSTORE" ]; then
  echo "sign-apk: cannot read keystore at $SUZATARY_KEYSTORE" >&2
  exit 1
fi

# Passwords come from the environment, never argv, which every process on the box can read.
# apksigner picks schemes from --min-sdk-version: at 26, v2 + v3 and no v1 (asking for v1
# explicitly does not produce one).
apksigner sign \
  --ks "$SUZATARY_KEYSTORE" \
  --ks-pass env:SUZATARY_KEYSTORE_PASSWORD \
  --key-pass env:SUZATARY_KEYSTORE_PASSWORD \
  --min-sdk-version 26 \
  --out "$OUT" \
  "$IN"

# apksigner can write a file a phone refuses; the printed certificate digests show whether this
# is the same key as the installed release.
apksigner verify --min-sdk-version 26 --print-certs --verbose "$OUT"
