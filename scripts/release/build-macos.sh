#!/bin/bash
# Produces unsigned downloads, signed on install. Built on macOS 26 (arm64).
# SUNNA_MAC_ARCH=x86_64 builds for the other chip (needs that Rust target).
# SUNNA_DMG_OUT=DIR also writes Sunna-macos-ARCH.dmg there: the same app, ad hoc
# signed, for people who would rather drag it to Applications.
set -euo pipefail
export SUNNA_PUBLIC_RELEASE=1
cd "$(dirname "$0")/../.."
out=${1:?output directory}; mkdir -p "$out"; out=$(cd "$out" && pwd)
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
host=$(uname -m); [ "$host" != arm64 ] || host=aarch64
arch=${SUNNA_MAC_ARCH:-$host}
case "$arch" in aarch64|x86_64) ;; *) echo "SUNNA_MAC_ARCH must be aarch64 or x86_64" >&2; exit 2;; esac
if [ "$arch" = "$host" ]; then
  built=target/release
  cargo build --release -p sunna -p sunna-update --features custom-protocol
else
  built=target/$arch-apple-darwin/release
  cargo build --release --target "$arch-apple-darwin" -p sunna -p sunna-update --features custom-protocol
fi
work=$(mktemp -d); trap 'rm -rf "$work"' EXIT
mkdir -p "$work/app/bin" "$work/app/Sunna.app/Contents/MacOS" "$work/app/Sunna.app/Contents/Resources"
cp "$built/sunna" "$work/app/Sunna.app/Contents/MacOS/Sunna-real"
cp "$built/sunna-update" "$work/app/Sunna.app/Contents/MacOS/Sunna"
cp apps/sunna/icons/icon.icns "$work/app/Sunna.app/Contents/Resources/Sunna.icns"
cp "$built/sunna-update" scripts/release/sign-mac.sh "$work/app/bin/"
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
cp "$built/sunna-update" "$out/sunna-update-macos-$arch"
# The check signs a fixture with Ed25519, which macOS's own openssl (LibreSSL) lacks.
if ! openssl genpkey -algorithm ED25519 -out /dev/null 2>/dev/null; then
  for dir in /opt/homebrew/opt/openssl@3/bin /usr/local/opt/openssl@3/bin; do
    [ ! -x "$dir/openssl" ] || { PATH="$dir:$PATH"; break; }
  done
fi
python3 scripts/release/check-updaters.py "$out" "macos-$arch"
if [ -n "${SUNNA_DMG_OUT:-}" ]; then
  mkdir -p "$SUNNA_DMG_OUT" "$work/dmg"
  cp -R "$work/app/Sunna.app" "$work/dmg/Sunna.app"
  ln -s /Applications "$work/dmg/Applications"
  # Ad hoc: there is no Apple developer identity. macOS asks for Open Anyway once.
  codesign --force --timestamp=none --identifier dev.sunna.app --sign - "$work/dmg/Sunna.app/Contents/MacOS/Sunna-real"
  codesign --force --timestamp=none --sign - "$work/dmg/Sunna.app"
  codesign --verify --strict "$work/dmg/Sunna.app"
  hdiutil create -quiet -ov -volname Sunna -fs HFS+ -format UDZO -srcfolder "$work/dmg" "$SUNNA_DMG_OUT/Sunna-macos-$arch.dmg"
fi
