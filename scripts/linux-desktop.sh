#!/usr/bin/env bash
# Start a headless Linux desktop (Xvfb + XFCE) for sunnad to host, if it
# isn't running already. For machines without a monitor, like the dev VM.
#
#   scripts/linux-desktop.sh [display] [WIDTHxHEIGHT]
#
# Defaults: display :40, 1710x1112 (a MacBook Air's "looks like" size, so
# the desktop fills the viewer's screen at native UI size).
set -euo pipefail
DISPLAY_NUM="${1:-${SUNNA_DISPLAY:-:40}}"
SIZE="${2:-${SUNNA_DESKTOP_SIZE:-1710x1112}}"
LOG_DIR="${XDG_RUNTIME_DIR:-/tmp}/sunna-desktop"
mkdir -p "$LOG_DIR"

if DISPLAY="$DISPLAY_NUM" xdpyinfo >/dev/null 2>&1; then
  echo "Desktop $DISPLAY_NUM already running ($(DISPLAY="$DISPLAY_NUM" xdpyinfo | awk '/dimensions/{print $2}'))."
  exit 0
fi
for tool in Xvfb startxfce4 dbus-launch; do
  command -v "$tool" >/dev/null || { echo "missing $tool (sudo apt install xvfb xfce4 dbus-x11)" >&2; exit 1; }
done

echo "Starting desktop $DISPLAY_NUM at $SIZE (logs in $LOG_DIR)."
nohup Xvfb "$DISPLAY_NUM" -screen 0 "${SIZE}x24" -nolisten tcp >"$LOG_DIR/xvfb.log" 2>&1 &
for _ in $(seq 50); do
  DISPLAY="$DISPLAY_NUM" xdpyinfo >/dev/null 2>&1 && break
  sleep 0.1
done
DISPLAY="$DISPLAY_NUM" nohup dbus-launch --exit-with-session startxfce4 >"$LOG_DIR/xfce.log" 2>&1 &
echo "Desktop $DISPLAY_NUM is up. Stop it with: pkill -f 'Xvfb $DISPLAY_NUM'"
