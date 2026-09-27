#!/usr/bin/env bash
# Build Sunna.app and install it in /Applications, so Sunna opens from
# Launchpad, Spotlight or the Dock like any other app.
#
#   scripts/install-mac-app.sh             # build, install, open
#   scripts/install-mac-app.sh --dmg       # also make dist/Sunna.dmg to share
#   scripts/install-mac-app.sh --no-open   # don't open it afterwards
#   scripts/install-mac-app.sh --adhoc     # sign ad hoc (see below)
#
# Run it again to update. Needs the Xcode command line tools
# (xcode-select --install) and Rust (https://rustup.rs).
#
# The app is signed with a certificate this Mac makes for itself the first
# time ("Sunna Local Code Signing", in your login keychain; macOS asks for
# your password once, to trust it for code signing). macOS remembers
# permissions like Accessibility (to send ⌘Tab and friends to the other
# computer) by that signature, so they survive updates. With --adhoc there's
# no certificate, and macOS forgets them at every update. No Apple developer
# account is involved. It's built for this Mac's chip. A copy downloaded
# from elsewhere (the DMG) is blocked the first time: open it, then allow it
# in System Settings → Privacy & Security → Open Anyway (on macOS 14 and
# earlier, right-click it and choose Open).
set -euo pipefail
cd "$(dirname "$0")/.."

[ "$(uname)" = Darwin ] || { echo "This builds the macOS app; run it on a Mac." >&2; exit 1; }
command -v cargo >/dev/null || { echo "Rust is missing: install it from https://rustup.rs" >&2; exit 1; }
xcrun --find codesign >/dev/null 2>&1 || { echo "Xcode command line tools are missing: run xcode-select --install" >&2; exit 1; }

DMG=0
OPEN=1
ADHOC=0
for arg in "$@"; do
  case "$arg" in
    --dmg) DMG=1 ;;
    --no-open) OPEN=0 ;;
    --adhoc) ADHOC=1 ;;
    *) awk 'NR > 1 && /^#/ { sub(/^# ?/, ""); print; next } NR > 1 { exit }' "$0"; exit 2 ;;
  esac
done

VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
BUILD="$(git rev-list --count HEAD 2>/dev/null || echo 1)"
COMMIT="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"

echo "Building Sunna $VERSION (build $BUILD, $COMMIT)…"
# custom-protocol: the production build (embedded UI, icon from the bundle).
SUNNA_GIT_HASH="$COMMIT" cargo build --release -p sunna --features custom-protocol

LSREGISTER=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister
# Earlier versions of this script built the app inside the project, where
# Launchpad and Spotlight found it as a second Sunna.
if [ -d dist/Sunna.app ]; then
  [ -x "$LSREGISTER" ] && "$LSREGISTER" -u dist/Sunna.app >/dev/null 2>&1 || true
  rm -rf dist/Sunna.app
  echo "Removed the old build copy in dist/ (it showed up as a second Sunna)."
fi

# Put the bundle together outside the project, for the same reason.
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
APP="$WORK/Sunna.app"
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

if [ "$ADHOC" = 1 ]; then
  echo "Signing ad hoc: macOS will forget Sunna's permissions at every update."
  codesign --force --sign - "$APP"
else
  if ! has_identity; then
    echo "Setting up Sunna's signing certificate on this Mac (once), so macOS keeps"
    echo "Sunna's permissions across updates…"
    has_certificate || make_certificate || echo "Couldn't make the certificate."
    has_certificate && { trust_certificate || echo "The certificate wasn't trusted."; }
  fi
  if ! has_identity; then
    echo "Sunna's signing certificate isn't set up, so nothing was installed. Run this" >&2
    echo "again to retry (macOS asks for your password once), or add --adhoc to install" >&2
    echo "without it (macOS then asks for Accessibility again after every update)." >&2
    exit 1
  fi
  echo "Signing with \"$IDENTITY\" (if macOS asks whether codesign may use its key, choose Always Allow)…"
  if ! SIGN_ERROR="$(codesign --force --timestamp=none --sign "$IDENTITY" "$APP" 2>&1)"; then
    echo "$SIGN_ERROR" >&2
    echo "Signing failed, so nothing was installed. If macOS asked whether codesign may" >&2
    echo "use the key, run this again and choose Always Allow." >&2
    exit 1
  fi
fi
codesign --verify --strict "$APP"
# What macOS ties Sunna's permissions to.
requirement() { codesign -d -r- "$1" 2>&1 | sed -n 's/^designated => //p'; }
REQUIREMENT="$(requirement "$APP")"
echo "Signed: $REQUIREMENT"

DEST=/Applications
[ -w "$DEST" ] || { DEST="$HOME/Applications"; mkdir -p "$DEST"; }
# Quit Sunna (the app and any session it has open) and wait until it's gone.
if pgrep -x Sunna >/dev/null; then
  echo "Quitting the running Sunna…"
  osascript -e 'quit app "Sunna"' >/dev/null 2>&1 || true
  for _ in $(seq 50); do pgrep -x Sunna >/dev/null || break; sleep 0.1; done
  if pgrep -x Sunna >/dev/null; then
    pkill -x Sunna || true
    for _ in $(seq 30); do pgrep -x Sunna >/dev/null || break; sleep 0.1; done
  fi
  if pgrep -x Sunna >/dev/null; then
    echo "Sunna is still running: quit it (and any session), then run this again." >&2
    exit 1
  fi
fi

# macOS keeps Sunna's Accessibility permission for one signature. When that
# changes (from the old ad hoc builds, or after a missing installed copy),
# the old entry stays in the list, shown as on, but no longer applies: clear
# it so Sunna can ask afresh.
OLD_REQUIREMENT=""
[ -d "$DEST/Sunna.app" ] && OLD_REQUIREMENT="$(requirement "$DEST/Sunna.app")"
if [ "$OLD_REQUIREMENT" != "$REQUIREMENT" ]; then
  if tccutil reset Accessibility dev.sunna.app; then
    [ -n "$OLD_REQUIREMENT" ] && echo "Sunna's signature changed, so its old Accessibility entry was cleared."
    echo "To send ⌘Tab and friends to the other computer, quit System Settings if it's open, then"
    if [ "$ADHOC" = 1 ]; then
      echo "in Sunna: Settings → System shortcuts → Allow… (with --adhoc, after every update)."
    else
      echo "in Sunna: Settings → System shortcuts → Allow… (once: it stays allowed through updates)."
    fi
  else
    echo "If Sunna says shortcuts aren't allowed while System Settings shows it on: select Sunna"
    echo "in Privacy & Security → Accessibility, remove it with −, then allow it again from Sunna."
  fi
fi

rm -rf "$DEST/Sunna.app"
ditto "$APP" "$DEST/Sunna.app"
# So Finder, Spotlight and the Dock pick up the new icon and version.
touch "$DEST/Sunna.app"
[ -x "$LSREGISTER" ] && "$LSREGISTER" -f "$DEST/Sunna.app" >/dev/null 2>&1 || true
echo "Installed $DEST/Sunna.app"

if [ "$DMG" = 1 ]; then
  STAGE="$WORK/dmg"
  mkdir -p "$STAGE" dist
  ditto "$APP" "$STAGE/Sunna.app"
  ln -s /Applications "$STAGE/Applications"
  rm -f dist/Sunna.dmg
  hdiutil create -volname Sunna -srcfolder "$STAGE" -ov -format UDZO dist/Sunna.dmg >/dev/null
  echo "Made dist/Sunna.dmg"
fi

[ "$OPEN" = 1 ] && open "$DEST/Sunna.app"
exit 0
