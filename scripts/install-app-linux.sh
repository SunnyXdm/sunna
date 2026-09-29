#!/usr/bin/env bash
# Install the Sunna app on this Linux computer from this checkout, for your
# user (no root needed): the Computers window and the viewer each session
# opens in, plus an entry in your desktop's app menu.
#
#   scripts/install-app-linux.sh
#
# Installs sunna into ~/.local/bin. Run it again to update. Needs Rust 1.95
# or later (https://rustup.rs), a C/C++ compiler, WebKitGTK and, for
# hardware and HEVC decoding, FFmpeg's development files.
set -euo pipefail
cd "$(dirname "$0")/.."

[ "$(uname)" = Linux ] || { echo "This installs the Linux app; on a Mac run scripts/install-mac-app.sh." >&2; exit 1; }
. scripts/linux-build-env.sh
check_build_tools

version=$(rustc --version | awk '{print $2}')
if [ "$(printf '%s\n1.95.0\n' "$version" | sort -V | head -1)" != 1.95.0 ]; then
  echo "Rust $version is too old for the viewer (it needs 1.95 or later). Update it with:" >&2
  echo "  rustup update stable" >&2
  exit 1
fi
command -v pkg-config >/dev/null || need_package "pkg-config" pkg-config pkgconf pkgconf-pkg-config
pkg-config --exists webkit2gtk-4.1 || need_package "WebKitGTK (for the app's window)" libwebkit2gtk-4.1-dev webkit2gtk-4.1 webkit2gtk4.1-devel
if ! pkg-config --exists libavcodec libavutil; then
  echo "Note: FFmpeg's development files are missing, so the viewer will decode H.264 only, in software." >&2
  if command -v apt-get >/dev/null; then echo "  For GPU and HEVC decoding: sudo apt install libavcodec-dev libavutil-dev" >&2
  elif command -v pacman >/dev/null; then echo "  For GPU and HEVC decoding: sudo pacman -S ffmpeg" >&2
  elif command -v dnf >/dev/null; then echo "  For GPU and HEVC decoding: sudo dnf install ffmpeg-free-devel" >&2
  fi
fi

echo "Building Sunna…"
SUNNA_GIT_HASH="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)" cargo build --release -p sunna --features custom-protocol

BIN="$HOME/.local/bin"
SHARE="${XDG_DATA_HOME:-$HOME/.local/share}"
mkdir -p "$BIN" "$SHARE/applications" "$SHARE/icons/hicolor/scalable/apps" "$SHARE/icons/hicolor/128x128/apps" "$SHARE/icons/hicolor/256x256/apps"
# Copy then rename: a running Sunna keeps its old binary until it restarts.
cp target/release/sunna "$BIN/sunna.new"
chmod 755 "$BIN/sunna.new"
mv -f "$BIN/sunna.new" "$BIN/sunna"
cp apps/sunna/icons/icon.svg "$SHARE/icons/hicolor/scalable/apps/dev.sunna.app.svg"
cp apps/sunna/icons/128x128.png "$SHARE/icons/hicolor/128x128/apps/dev.sunna.app.png"
cp apps/sunna/icons/128x128@2x.png "$SHARE/icons/hicolor/256x256/apps/dev.sunna.app.png"
cat > "$SHARE/applications/dev.sunna.app.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Sunna
GenericName=Remote Desktop
Comment=Use your other computers from this one
Exec=$BIN/sunna
Icon=dev.sunna.app
Terminal=false
Categories=Network;RemoteAccess;
Keywords=remote;desktop;vnc;screen;
DESKTOP
command -v update-desktop-database >/dev/null && update-desktop-database -q "$SHARE/applications" || true
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q -t "$SHARE/icons/hicolor" 2>/dev/null || true

echo "Installed Sunna in $BIN, with an entry in your app menu."
case ":$PATH:" in
  *":$BIN:"*) ;;
  *) echo "Note: $BIN isn't on your PATH; add it (e.g. in ~/.profile) to start sunna from a terminal." ;;
esac
echo "Open it from your app menu, or run: sunna"
