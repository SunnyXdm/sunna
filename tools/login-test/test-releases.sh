#!/usr/bin/env bash
# Host-only release pair for an isolated VM. Never uses the real signing key.
set -euo pipefail
cd "$(dirname "$0")/../.."
out=$(realpath "${1:?empty output directory for test releases}")
[[ -z $(ls -A "$out") ]]
work=$(mktemp -d /tmp/sunna-login-build.XXXXXXXX)
trap 'rm -rf "$work"' EXIT
export SUNNA_RELEASE_KEY="$work/test.key" SUNNA_TEST_PUBLIC_KEY="$work/test.pub"
scripts/release/keygen.sh "$SUNNA_TEST_PUBLIC_KEY"
mkdir -p "$work/src/apps" "$work/src/tools/login-test"
cp -r crates "$work/src/"
cp -r apps/sunnad "$work/src/apps/"
cp tools/login-test/client.rs "$work/src/tools/login-test/"
cp Cargo.toml Cargo.lock "$work/src/"
sed -i '/^    "\(apps\/sunna-cli\|apps\/sunna\|crates\/android\|crates\/viewer\)",$/d' "$work/src/Cargo.toml"
export PATH="$HOME/.cargo/bin:$PATH" CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$work/target}"
for version in 0.1.0 0.2.0; do
    sed -i '/^\[workspace.package\]$/{n;s/^version = .*/version = "'"$version"'"/;}' "$work/src/Cargo.toml"
    cargo build --manifest-path "$work/src/Cargo.toml" --features sunna-update/test-key -j2 -p sunnad -p sunna-update
    dest=$out/$version
    stage=$work/stage
    mkdir -p "$stage"/{bin,lib,share/sunna} "$dest"
    cp "$CARGO_TARGET_DIR/debug/"{sunnad,sunna-update} scripts/sunna-host "$stage/bin/"
    strip -s "$stage/bin/sunnad" "$stage/bin/sunna-update"
    cp scripts/{linux-desktop,wayland-desktop,system-host}.sh "$stage/share/sunna/"
    cp "$stage/bin/sunna-update" "$dest/sunna-update-linux-$(uname -m)"
    tar -C "$stage" -czf "$dest/sunna-host-linux-$(uname -m).tar.gz" .
    python3 scripts/release/manifest.py "$version" "$dest" "http://127.0.0.1:48991/$version"
done
dest=$out/broken
mkdir "$dest"
cp "$out/0.2.0"/sunna-update-* "$dest/"
# Same failed-start fixture as scripts/release/prove-update.py.
cat >"$stage/bin/sunnad" <<'BAD'
#!/bin/sh
if [ "${1:-}" = --version ]; then echo 'sunnad 0.3.0'; exit; fi
exit 1
BAD
mv "$stage/bin/sunna-update" "$stage/bin/updater"
cat >"$stage/bin/sunna-update" <<'BAD'
#!/bin/sh
if [ "${1:-}" = --version ]; then echo 'sunna-update 0.3.0'; exit; fi
exec "$(dirname "$0")/updater" "$@"
BAD
chmod 755 "$stage/bin/sunna-update"
tar -C "$stage" -czf "$dest/sunna-host-linux-$(uname -m).tar.gz" .
python3 scripts/release/manifest.py 0.3.0 "$dest" http://127.0.0.1:48991/broken
