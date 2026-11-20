#!/usr/bin/env bash
set -euo pipefail
export GNUPGHOME=${SUNNA_REPO_GNUPGHOME:-$HOME/.config/sunna/repo-gnupg}
install -d -m 700 "$GNUPGHOME"
identity='Sunna packages <sunna@sunnydx.dev>'
if ! gpg --batch --list-secret-keys "$identity" >/dev/null 2>&1; then
  gpg --batch --pinentry-mode loopback --passphrase '' --quick-generate-key "$identity" ed25519 sign 0
fi
out=${1:-$(dirname "$0")/repo-key.asc}
gpg --batch --armor --export "$identity" > "$out"
test -s "$out"
chmod 644 "$out"
gpg --fingerprint "$identity"
