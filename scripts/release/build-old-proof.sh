#!/bin/bash
# Build actual current-main binaries and files placed by its source installer.
# No branch switch or worktree modifications. Run after build-linux.sh made
# sunna-fix-builder. OUTPUT is the same work directory used by prove-update.py.
set -euo pipefail
cd "$(dirname "$0")/../.."
out=$(realpath -m "${1:?proof directory}"); mkdir -p "$out"
work=$(mktemp -d /tmp/sunna-fix-old-proof.XXXXXX)
name="sunna-fix-old-proof-$$"
cleanup() { docker rm -f "$name" >/dev/null 2>&1 || true; rm -rf "$work"; }
trap cleanup EXIT
git rev-parse "${SUNNA_OLD_REF:-main}" > "$out/old-revision.txt"
git archive "${SUNNA_OLD_REF:-main}" | tar -xf - -C "$work"
mkdir -p "$work/crates/client/examples"
cp tools/login-test/old-viewer.rs "$work/crates/client/examples/update-proof.rs"
ln -s /target "$work/target"
docker run --rm --name "$name" -v "$work:/src" -v "$out:/proof" \
  -v "$PWD/scripts/release/legacy-inner.sh:/legacy-proof.sh:ro" \
  -v "$HOME/.cargo:/root/.cargo" -v "$HOME/.rustup:/root/.rustup:ro" \
  -v sunna-fix-old-target:/target -e CARGO_TARGET_DIR=/target \
  -e PATH=/root/.cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin \
  sunna-fix-builder bash /legacy-proof.sh
