#!/usr/bin/env bash
# Build the Sunna host as a Debian/Ubuntu package: dist/sunna-host_<version>_<arch>.deb
#
#   scripts/package-linux-deb.sh
#
# Install it with `sudo apt install ./dist/sunna-host_*.deb`, then run
# `sunna-host setup`. Build on the oldest release you want to support
# (glibc is forward-compatible, not backward): Ubuntu 22.04 covers 22.04+
# and Debian 12+.
set -euo pipefail
cd "$(dirname "$0")/.."

command -v dpkg-deb >/dev/null || { echo "dpkg-deb is missing (build this on Debian or Ubuntu)" >&2; exit 1; }
command -v objdump >/dev/null || { echo "objdump is missing: sudo apt install binutils" >&2; exit 1; }
. scripts/linux-build-env.sh
check_build_tools
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
BUILD="$(git rev-list --count HEAD 2>/dev/null || echo 1)"
COMMIT="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
ARCH="$(dpkg --print-architecture)"

echo "Building the Sunna host $VERSION (build $BUILD, $COMMIT)…"
SUNNA_GIT_HASH="$COMMIT" cargo build --release -p sunnad

NAME="sunna-host_${VERSION}-${BUILD}_${ARCH}"
ROOT="dist/deb/$NAME"
rm -rf "$ROOT"
mkdir -p "$ROOT/DEBIAN" "$ROOT/usr/bin" "$ROOT/usr/share/sunna" "$ROOT/usr/share/doc/sunna-host"
install -m 755 target/release/sunnad "$ROOT/usr/bin/sunnad"
install -m 755 scripts/sunna-host "$ROOT/usr/bin/sunna-host"
install -m 755 scripts/linux-desktop.sh "$ROOT/usr/share/sunna/linux-desktop.sh"
mkdir -p "$ROOT/usr/share/applications"
cat >"$ROOT/usr/share/applications/dev.sunna.Host.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Sunna
NoDisplay=true
Exec=/usr/bin/sunnad
EOF
install -m 644 README.md "$ROOT/usr/share/doc/sunna-host/README.md"
SIZE_KB="$(du -sk "$ROOT/usr" | cut -f1)"
# The newest glibc sunnad was linked against: the package asks for at least
# that, so it only installs where it can run.
GLIBC="$(objdump -T target/release/sunnad | grep -o 'GLIBC_[0-9.]*' | sed 's/^GLIBC_//' | sort -uV | tail -1)"
[ -n "$GLIBC" ] || { echo "couldn't read which glibc sunnad needs" >&2; exit 1; }

cat >"$ROOT/DEBIAN/control" <<EOF
Package: sunna-host
Version: ${VERSION}-${BUILD}
Architecture: $ARCH
Maintainer: Sunna <sunna@localhost>
Installed-Size: $SIZE_KB
Depends: libc6 (>= $GLIBC), libgcc-s1, libstdc++6
Recommends: xvfb, x11-utils, dbus-x11, xfce4, pulseaudio-utils
Suggests: tailscale
Section: net
Priority: optional
Description: Sunna host: share this computer with the Sunna app
 Streams this computer's desktop to the Sunna app on a Mac, with the
 keyboard, mouse and clipboard sent back. Run "sunna-host setup" after
 installing: it shares your X11 or GNOME/KDE Wayland desktop, or a separate
 virtual desktop on machines without a screen, and starts by itself from
 then on. Build $COMMIT.
EOF

cat >"$ROOT/DEBIAN/postinst" <<'EOF'
#!/bin/sh
set -e
if [ "$1" = configure ]; then
  echo "Sunna host installed. As the user whose desktop to share, run: sunna-host setup"
fi
EOF
chmod 755 "$ROOT/DEBIAN/postinst"

mkdir -p dist
dpkg-deb --build --root-owner-group "$ROOT" "dist/$NAME.deb" >/dev/null
rm -rf dist/deb
echo "Made dist/$NAME.deb"
