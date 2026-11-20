#!/bin/sh
# Generate once, without emitting or copying private material.
set -eu
umask 077
key=${SUNNA_RELEASE_KEY:-$HOME/.config/sunna/release.key}
pub=${1:-crates/update/release.pub}
mkdir -p "$(dirname "$key")" "$(dirname "$pub")"
if [ ! -f "$key" ]; then
  openssl genpkey -algorithm ED25519 -out "$key"
fi
chmod 600 "$key"
# DER SubjectPublicKeyInfo ends with the 32-byte raw Ed25519 public key.
openssl pkey -in "$key" -pubout -outform DER | tail -c 32 > "$pub"
chmod 644 "$pub"
echo "Release key ready; public key: $pub"
