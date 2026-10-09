#!/usr/bin/env bash
# Install the Sunna host on this Linux computer from this checkout, for your
# user (no root needed), then set it up to start by itself.
#
#   scripts/install-host-linux.sh              # build, install, set up
#   scripts/install-host-linux.sh --virtual    # options go to `sunna-host setup`
#
# Installs sunnad and sunna-host into ~/.local/bin. Run it again to update.
# Needs Rust (https://rustup.rs), a C/C++ compiler and, on x86-64, nasm.
set -euo pipefail
cd "$(dirname "$0")/.."

[ "$(uname)" = Linux ] || { echo "This installs the Linux host; on a Mac see README.md." >&2; exit 1; }
. scripts/linux-build-env.sh
check_build_tools

echo "Building the Sunna host…"
SUNNA_GIT_HASH="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)" cargo build --release -p sunnad

BIN="$HOME/.local/bin"
DATA="${XDG_DATA_HOME:-$HOME/.local/share}/sunna"
mkdir -p "$BIN" "$DATA"
# Copy then rename: the running host keeps its old binary until it restarts.
place() {
  cp "$1" "$2.new"
  chmod 755 "$2.new"
  mv -f "$2.new" "$2"
}
place target/release/sunnad "$BIN/sunnad"
place scripts/sunna-host "$BIN/sunna-host"
place scripts/linux-desktop.sh "$DATA/linux-desktop.sh"
APPLICATIONS="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
mkdir -p "$APPLICATIONS"
# Desktop Entry Exec uses double quotes, with shell metacharacters escaped.
DESKTOP_EXEC="$(printf '%s' "$BIN/sunnad" | sed 's/[\\`$"]/\\&/g; s/%/%%/g')"
cat >"$APPLICATIONS/dev.sunna.Host.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Sunna
NoDisplay=true
Exec="$DESKTOP_EXEC"
EOF
echo "Installed sunnad and sunna-host in $BIN."
case ":$PATH:" in
  *":$BIN:"*) ;;
  *) echo "Note: $BIN isn't on your PATH; add it (e.g. in ~/.profile) to type sunna-host anywhere." ;;
esac

"$BIN/sunna-host" setup "$@"
