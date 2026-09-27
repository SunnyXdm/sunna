#!/usr/bin/env bash
# Build Sunna.app and install it in /Applications, so Sunna opens from
# Launchpad, Spotlight or the Dock like any other app.
#
#   scripts/install-mac-app.sh             # build, install, open
#   scripts/install-mac-app.sh --dmg       # also make dist/Sunna.dmg to share
#   scripts/install-mac-app.sh --no-open   # don't open it afterwards
#
# Run it again to update. Needs the Xcode command line tools
# (xcode-select --install) and Rust (https://rustup.rs).
#
# The app is signed ad hoc (no Apple developer account), which is all a
# Mac needs for an app built on it. A copy downloaded from elsewhere (the
# DMG) needs right-click → Open the first time. macOS may ask again for
# Accessibility (to send ⌘Tab and friends to the remote) after an update.
set -euo pipefail
cd "$(dirname "$0")/.."

[ "$(uname)" = Darwin ] || { echo "This builds the macOS app; run it on a Mac." >&2; exit 1; }
command -v cargo >/dev/null || { echo "Rust is missing: install it from https://rustup.rs" >&2; exit 1; }
xcrun --find codesign >/dev/null 2>&1 || { echo "Xcode command line tools are missing: run xcode-select --install" >&2; exit 1; }

DMG=0
OPEN=1
for arg in "$@"; do
  case "$arg" in
    --dmg) DMG=1 ;;
    --no-open) OPEN=0 ;;
    *) sed -n '2,17p' "$0"; exit 2 ;;
  esac
done

VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
BUILD="$(git rev-list --count HEAD 2>/dev/null || echo 1)"
COMMIT="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"

echo "Building Sunna $VERSION (build $BUILD, $COMMIT)…"
# custom-protocol: the production build (embedded UI, icon from the bundle).
SUNNA_GIT_HASH="$COMMIT" cargo build --release -p sunna --features custom-protocol

APP=dist/Sunna.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/sunna "$APP/Contents/MacOS/Sunna"
cp apps/sunna/icons/icon.icns "$APP/Contents/Resources/Sunna.icns"
printf 'APPL????' >"$APP/Contents/PkgInfo"
cat >"$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Sunna</string>
  <key>CFBundleDisplayName</key><string>Sunna</string>
  <key>CFBundleIdentifier</key><string>dev.sunna.app</string>
  <key>CFBundleExecutable</key><string>Sunna</string>
  <key>CFBundleIconFile</key><string>Sunna</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleVersion</key><string>$BUILD</string>
  <key>LSMinimumSystemVersion</key><string>13.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.utilities</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSHumanReadableCopyright</key><string>Sunna $VERSION ($COMMIT)</string>
</dict>
</plist>
PLIST
plutil -lint "$APP/Contents/Info.plist" >/dev/null
codesign --force --deep --sign - "$APP"

DEST=/Applications
[ -w "$DEST" ] || { DEST="$HOME/Applications"; mkdir -p "$DEST"; }
if pgrep -x Sunna >/dev/null; then
  echo "Quitting the running Sunna…"
  osascript -e 'quit app "Sunna"' >/dev/null 2>&1 || true
  sleep 1
fi
rm -rf "$DEST/Sunna.app"
cp -R "$APP" "$DEST/"
# So Finder, Spotlight and the Dock pick up the new icon and version.
touch "$DEST/Sunna.app"
LSREGISTER=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister
[ -x "$LSREGISTER" ] && "$LSREGISTER" -f "$DEST/Sunna.app" >/dev/null 2>&1 || true
echo "Installed $DEST/Sunna.app"

if [ "$DMG" = 1 ]; then
  STAGE="$(mktemp -d)"
  cp -R "$APP" "$STAGE/"
  ln -s /Applications "$STAGE/Applications"
  rm -f dist/Sunna.dmg
  hdiutil create -volname Sunna -srcfolder "$STAGE" -ov -format UDZO dist/Sunna.dmg >/dev/null
  rm -rf "$STAGE"
  echo "Made dist/Sunna.dmg"
fi

[ "$OPEN" = 1 ] && open "$DEST/Sunna.app"
exit 0
