#!/bin/bash
# Uses the same local identity as source installs; no ad-hoc fallback.
set -euo pipefail
APP=${1:?bundle path}
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
  has_certificate || make_certificate
  trust_certificate
fi
has_identity || { echo "Set up Sunna Local Code Signing in the login keychain before updating." >&2; exit 1; }
codesign --force --timestamp=none --identifier dev.sunna.app --sign "$IDENTITY" "$APP/Contents/MacOS/Sunna-real"
codesign --force --timestamp=none --sign "$IDENTITY" "$APP"
codesign --verify --strict "$APP"

# Both modes must be recognized as the same signed code across releases.
requirement() { codesign -d -r- "$1" 2>&1 | sed -n 's/^designated => //p'; }
new=$(requirement "$APP")
[ -n "$new" ] && [ "$new" = "$(requirement "$APP/Contents/MacOS/Sunna-real")" ] || {
  echo "The host and the app have different signing requirements." >&2; exit 1;
}
if [ -d "${2:-}" ] && [ "$new" != "$(requirement "$2")" ]; then
  echo "The update would change Sunna’s signing identity. The installed app was kept." >&2
  exit 1
fi
