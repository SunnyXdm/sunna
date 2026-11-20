#!/bin/bash
# Produces unsigned downloads, signed on install. Built on macOS 26 (arm64).
set -euo pipefail
export SUNNA_PUBLIC_RELEASE=1
cd "$(dirname "$0")/../.."
out=${1:?output directory}; mkdir -p "$out"; out=$(cd "$out" && pwd)
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
arch=$(uname -m); [ "$arch" != arm64 ] || arch=aarch64
cargo build --release -p sunna -p sunna-update --features custom-protocol
work=$(mktemp -d); trap 'rm -rf "$work"' EXIT
mkdir -p "$work/app/bin" "$work/app/Sunna.app/Contents/MacOS" "$work/app/Sunna.app/Contents/Resources"
cp target/release/sunna "$work/app/Sunna.app/Contents/MacOS/Sunna-real"
cp target/release/sunna-update "$work/app/Sunna.app/Contents/MacOS/Sunna"
cp apps/sunna/icons/icon.icns "$work/app/Sunna.app/Contents/Resources/Sunna.icns"
cp target/release/sunna-update scripts/release/sign-mac.sh "$work/app/bin/"
cat > "$work/app/Sunna.app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
<key>CFBundleName</key><string>Sunna</string>
<key>CFBundleIdentifier</key><string>dev.sunna.app</string>
<key>CFBundleExecutable</key><string>Sunna</string>
<key>CFBundleIconFile</key><string>Sunna</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>$version</string>
<key>CFBundleVersion</key><string>$version</string>
<key>LSMinimumSystemVersion</key><string>13.0</string>
<key>NSLocalNetworkUsageDescription</key><string>Sunna connects your devices and lets a viewer reach this Mac when sharing is on.</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
(cd "$work/app" && zip -qr "$out/Sunna-macos-$arch.zip" .)
cp target/release/sunna-update "$out/sunna-update-macos-$arch"
# The check signs a fixture with Ed25519, which macOS's own openssl (LibreSSL) lacks.
if ! openssl genpkey -algorithm ED25519 -out /dev/null 2>/dev/null; then
  for dir in /opt/homebrew/opt/openssl@3/bin /usr/local/opt/openssl@3/bin; do
    [ ! -x "$dir/openssl" ] || { PATH="$dir:$PATH"; break; }
  done
fi
python3 scripts/release/check-updaters.py "$out" "macos-$arch"
