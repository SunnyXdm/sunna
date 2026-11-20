#!/usr/bin/env bash
# Build in old glibc. Build output and snapshot belong only to this invocation.
# Usage: build-linux.sh VERSION OUTPUT [TEST_PUBLIC_KEY]
set -euo pipefail
cd "$(dirname "$0")/../.."
version=${1:?version}; out=$(realpath -m "${2:?output}"); testpub=${3:-}
work=$(mktemp -d /tmp/sunna-fix-build.XXXXXX)
name="sunna-fix-build-$$"
cleanup() { docker rm -f "$name" >/dev/null 2>&1 || true; rm -rf "$work"; }
trap cleanup EXIT
mkdir -p "$out" "$work/src"
tar --exclude=.git --exclude=target --exclude=dist --exclude=.gradle --exclude=build --exclude=node_modules -cf - . | tar -xf - -C "$work/src"
if [ -n "$testpub" ]; then
  python3 scripts/release/version.py "$version" --root "$work/src"
fi
python3 scripts/release/version.py "$version" --root "$work/src" --check
docker build -t sunna-fix-builder -f scripts/release/Dockerfile.linux scripts/release
features=custom-protocol
if [ -n "$testpub" ]; then cp "$testpub" "$work/test.pub"; features="$features,sunna-update/test-key"; else cp crates/update/release.pub "$work/test.pub"; fi
flags=(--rm)
if [ "${SUNNA_RELEASE_DETACH:-0}" = 1 ]; then
  flags=(-d)
  trap - EXIT
  echo "Build container: $name; staging directory: $work (remove both after it exits)."
fi
docker run "${flags[@]}" --name "$name" \
  -v "$work/src:/src" -v "$out:/out" -v "$work/test.pub:/test.pub:ro" \
  -v "$HOME/.cargo:/root/.cargo" -v "$HOME/.rustup:/root/.rustup:ro" \
  -v sunna-fix-target:/target -e CARGO_TARGET_DIR=/target -e SUNNA_TEST_PUBLIC_KEY=/test.pub \
  -e FEATURES="$features" -e VERSION="$version" -e PATH=/root/.cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin \
  sunna-fix-builder bash /src/scripts/release/linux-inner.sh
