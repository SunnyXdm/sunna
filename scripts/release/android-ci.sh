#!/bin/bash
set -euo pipefail
export SUNNA_PUBLIC_RELEASE=1
umask 077
work=$(mktemp -d); trap 'rm -rf "$work"' EXIT
printf '%s' "$ANDROID_KEYSTORE" | base64 -d > "$work/release.jks"
printf 'storeFile=%s\nstorePassword=%s\nkeyAlias=%s\nkeyPassword=%s\n' "$work/release.jks" "$ANDROID_STORE_PASSWORD" "$ANDROID_KEY_ALIAS" "$ANDROID_KEY_PASSWORD" > "$work/signing.properties"
unset ANDROID_KEYSTORE ANDROID_STORE_PASSWORD ANDROID_KEY_ALIAS ANDROID_KEY_PASSWORD
SUNNA_ANDROID_SIGNING="$work/signing.properties" scripts/release/build-android.sh "$@"
