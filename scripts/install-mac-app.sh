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
# The app is signed with a certificate this Mac makes for itself the first
# time ("Sunna Local Code Signing", in your login keychain; macOS asks for
# your password once, to trust it for code signing). macOS remembers
# permissions like Accessibility (to send ⌘Tab and friends to the other
# computer) by that signature, so they survive updates; an ad hoc signature
# changes with every build and lost them. No Apple developer account is
# involved. It's built for this Mac's chip. A copy downloaded from
# elsewhere (the DMG) is blocked the first time: open it, then allow it in
# System Settings → Privacy & Security → Open Anyway (on macOS 14 and
# earlier, right-click it and choose Open).
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

IDENTITY="Sunna Local Code Signing"
KEYCHAIN="$HOME/Library/Keychains/login.keychain-db"
# A usable identity is a certificate with its key, trusted for code signing.
has_identity() { security find-identity -v -p codesigning "$KEYCHAIN" 2>/dev/null | grep -qF "\"$IDENTITY\""; }
has_certificate() { security find-certificate -c "$IDENTITY" "$KEYCHAIN" >/dev/null 2>&1; }

# A self-signed code-signing certificate and its key, made once.
make_certificate() {
  local dir status
  dir="$(mktemp -d)"
  cat >"$dir/cert.conf" <<CONF
[req]
distinguished_name = dn
x509_extensions = ext
prompt = no
[dn]
CN = $IDENTITY
[ext]
basicConstraints = critical, CA:false
keyUsage = critical, digitalSignature
extendedKeyUsage = critical, codeSigning
CONF
  # macOS's own openssl (LibreSSL): its .p12 is one `security` can import.
  /usr/bin/openssl req -x509 -newkey rsa:2048 -nodes -days 3650 -config "$dir/cert.conf" \
    -keyout "$dir/key.pem" -out "$dir/cert.pem" >/dev/null 2>&1 &&
    /usr/bin/openssl pkcs12 -export -inkey "$dir/key.pem" -in "$dir/cert.pem" -name "$IDENTITY" \
      -passout pass:sunna -out "$dir/identity.p12" >/dev/null 2>&1 &&
    security import "$dir/identity.p12" -k "$KEYCHAIN" -P sunna -T /usr/bin/codesign >/dev/null
  status=$?
  rm -rf "$dir"
  return $status
}

# Trust it for code signing (macOS asks for your password, once).
trust_certificate() {
  local pem status
  pem="$(mktemp)"
  echo "macOS will ask for your password to trust Sunna's certificate for code signing (once)."
  security find-certificate -c "$IDENTITY" -p "$KEYCHAIN" >"$pem" &&
    security add-trusted-cert -r trustRoot -p codeSign -k "$KEYCHAIN" "$pem"
  status=$?
  rm -f "$pem"
  return $status
}

if ! has_identity; then
  echo "Setting up Sunna's signing certificate on this Mac (once), so macOS keeps"
  echo "Sunna's permissions across updates…"
  has_certificate || make_certificate || echo "Couldn't make the certificate."
  has_certificate && { trust_certificate || echo "The certificate wasn't trusted."; }
fi
SIGNED=adhoc
if has_identity; then
  echo "Signing with \"$IDENTITY\" (if macOS asks whether codesign may use its key, choose Always Allow)…"
  if SIGN_ERROR="$(codesign --force --deep --timestamp=none --sign "$IDENTITY" "$APP" 2>&1)"; then
    SIGNED=identity
  else
    echo "$SIGN_ERROR" >&2
  fi
fi
if [ "$SIGNED" = adhoc ]; then
  echo "Note: signing ad hoc, so macOS will ask for Accessibility again after each"
  echo "update. Run this script again to retry the certificate."
  codesign --force --deep --sign - "$APP"
fi

DEST=/Applications
[ -w "$DEST" ] || { DEST="$HOME/Applications"; mkdir -p "$DEST"; }
if pgrep -x Sunna >/dev/null; then
  echo "Quitting the running Sunna…"
  osascript -e 'quit app "Sunna"' >/dev/null 2>&1 || true
  sleep 1
fi

# Moving from an ad hoc build to the certificate: Sunna's Accessibility
# entry belongs to the old signature (it shows as on, but no longer
# applies). Clear it so macOS asks afresh, once.
if [ "$SIGNED" = identity ] && [ -d "$DEST/Sunna.app" ] &&
  codesign -dv "$DEST/Sunna.app" 2>&1 | grep -q "Signature=adhoc"; then
  tccutil reset Accessibility dev.sunna.app >/dev/null 2>&1 &&
    echo "Cleared Sunna's old Accessibility entry: allow it once more (Sunna asks), and it stays allowed from now on." ||
    echo "In System Settings → Privacy & Security → Accessibility, remove Sunna (−) once; Sunna then asks afresh and it stays allowed."
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
