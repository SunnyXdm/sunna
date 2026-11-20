#!/usr/bin/env bash
# Supply an empty private proof directory outside the checkout and outside /tmp.
# Builds only test releases. All artifact servers listen on loopback.
set -euo pipefail
cd "$(dirname "$0")/../.."
export PATH="$HOME/.cargo/bin:$PATH"
work=$(realpath "${1:?empty private proof directory}")
[[ -d "$work" && ! -L "$work" && -z "$(ls -A "$work")" ]]
export SUNNA_RELEASE_KEY="$work/test.key" SUNNA_TEST_PUBLIC_KEY="$work/test.pub"
scripts/release/keygen.sh "$SUNNA_TEST_PUBLIC_KEY"
cleanup() {
  if [[ -n ${feed_pid:-} ]]; then kill "$feed_pid" 2>/dev/null || true; fi
  rm -f "$SUNNA_RELEASE_KEY"
}
trap cleanup EXIT
for label in A B; do
  version=0.1.0
  [[ "$label" == A ]] || version=0.2.0
  scripts/release/build-linux.sh "$version" "$work/releases/$label" "$SUNNA_TEST_PUBLIC_KEY"
  mkdir -p "$work/runtime/$label"
  cp "$work/releases/$label"/sunna-* "$work/runtime/$label/"
  python3 scripts/release/manifest.py "$version" "$work/runtime/$label" "http://127.0.0.1:48991/$label"
done
SUNNA_OLD_REF=HEAD scripts/release/build-old-proof.sh "$work"
python3 scripts/release/prove-update.py "$work" "$work/releases/A/.proof-client"
scripts/release/prove-systemd.sh "$work"
python3 -m http.server 48991 --directory "$work/runtime" --bind 127.0.0.1 >"$work/runtime-server.log" 2>&1 &
feed_pid=$!
for label in A B; do scripts/release/prove-linux.sh "$work/runtime/$label"; done
