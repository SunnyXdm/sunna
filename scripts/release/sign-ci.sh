#!/bin/bash
set -euo pipefail
umask 077
key=$(mktemp); trap 'rm -f "$key"' EXIT
printf '%s\n' "$RELEASE_KEY" > "$key"
unset RELEASE_KEY SUNNA_TEST_PUBLIC_KEY
SUNNA_RELEASE_KEY="$key" python3 scripts/release/manifest.py "$@"
