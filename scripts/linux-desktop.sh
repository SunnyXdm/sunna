#!/usr/bin/env bash
# Start a headless Linux desktop (Xvfb + KDE Plasma or XFCE) for sunnad to
# host, if it isn't running already. For machines without a monitor, like
# the dev VM.
#
#   scripts/linux-desktop.sh [display] [WIDTHxHEIGHT]
#
# Defaults: display :40, 1710x1112 (a MacBook Air's "looks like" size, so
# the desktop fills the viewer's screen at native UI size).
#
# SUNNA_DESKTOP=plasma|xfce picks the desktop; default: Plasma's X11
# session if installed, else XFCE. (GNOME is Wayland-only now, which
# sunnad can't capture yet.)
#
# Everything is started detached (setsid) with its own D-Bus session, so
# the desktop outlives the terminal or SSH connection that started it.
set -euo pipefail
DISPLAY_NUM="${1:-${SUNNA_DISPLAY:-:40}}"
SIZE="${2:-${SUNNA_DESKTOP_SIZE:-1710x1112}}"
LOG_DIR="${XDG_RUNTIME_DIR:-/tmp}/sunna-desktop"
SESSION_PID="$LOG_DIR/session${DISPLAY_NUM#:}.pid"
mkdir -p "$LOG_DIR"

DESKTOP="${SUNNA_DESKTOP:-}"
if [ -z "$DESKTOP" ]; then
  if command -v startplasma-x11 >/dev/null; then DESKTOP=plasma; else DESKTOP=xfce; fi
fi
case "$DESKTOP" in
  plasma) SESSION_CMD=startplasma-x11 ;;
  xfce) SESSION_CMD=startxfce4 ;;
  *) echo "SUNNA_DESKTOP must be plasma or xfce" >&2; exit 2 ;;
esac

for tool in Xvfb "$SESSION_CMD" dbus-run-session setsid xdpyinfo; do
  command -v "$tool" >/dev/null || {
    echo "missing $tool (Debian/Ubuntu: xvfb xfce4 dbus x11-utils; Arch: xorg-server-xvfb xfce4 or plasma-x11-session, dbus, xorg-xdpyinfo)" >&2
    exit 1
  }
done

if DISPLAY="$DISPLAY_NUM" xdpyinfo >/dev/null 2>&1; then
  echo "Display $DISPLAY_NUM already running ($(DISPLAY="$DISPLAY_NUM" xdpyinfo | awk '/dimensions/{print $2}'))."
else
  echo "Starting display $DISPLAY_NUM at $SIZE (logs in $LOG_DIR)."
  setsid nohup Xvfb "$DISPLAY_NUM" -screen 0 "${SIZE}x24" -nolisten tcp \
    >"$LOG_DIR/xvfb.log" 2>&1 </dev/null &
  for _ in $(seq 50); do
    DISPLAY="$DISPLAY_NUM" xdpyinfo >/dev/null 2>&1 && break
    sleep 0.1
  done
fi

# The display can outlive its desktop session (an earlier version tied the
# session's D-Bus to the SSH connection); start the session if it's gone.
if [ -f "$SESSION_PID" ] && kill -0 "$(cat "$SESSION_PID")" 2>/dev/null; then
  echo "Desktop session on $DISPLAY_NUM is running."
else
  DISPLAY="$DISPLAY_NUM" setsid nohup dbus-run-session -- "$SESSION_CMD" \
    >"$LOG_DIR/$DESKTOP.log" 2>&1 </dev/null &
  echo $! >"$SESSION_PID"
  echo "Started $DESKTOP on $DISPLAY_NUM."
fi
echo "Stop it with: pkill -f 'Xvfb $DISPLAY_NUM'"
