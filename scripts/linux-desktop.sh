#!/usr/bin/env bash
# Start a headless Linux desktop (Xvfb + KDE Plasma or XFCE) for sunnad to
# host, if it isn't running already. For machines without a monitor, like
# the dev VM.
#
#   scripts/linux-desktop.sh [display] [WIDTHxHEIGHT]
#   scripts/linux-desktop.sh stop [display]    # the desktop and its apps
#
# Defaults: display :40, 1710x1112 (a MacBook Air's "looks like" size, so
# the desktop fills the viewer's screen at native UI size).
#
# SUNNA_DESKTOP=plasma|xfce picks the desktop; default: Plasma's X11
# session if installed, else XFCE. (GNOME is Wayland-only now, which
# sunnad can't capture yet.)
#
# Sound: if a sound server is running (PulseAudio, or PipeWire's pulse
# server), the desktop gets its own silent output, sunna_desktop_<display>,
# and its apps play into it, so the host shares exactly this desktop's sound
# (its monitor) and nothing else on the machine.
#
# Everything is started detached (setsid) with its own D-Bus session, so
# the desktop outlives the terminal or SSH connection that started it, and
# the host: apps opened in it keep running (and using CPU) until it's
# stopped.
set -euo pipefail
if [ "${1:-}" = stop ]; then
  STOP=1
  shift
fi
DISPLAY_NUM="${1:-${SUNNA_DISPLAY:-:40}}"
SIZE="${2:-${SUNNA_DESKTOP_SIZE:-1710x1112}}"
LOG_DIR="${XDG_RUNTIME_DIR:-/tmp}/sunna-desktop"
SESSION_PID="$LOG_DIR/session${DISPLAY_NUM#:}.pid"
SINK="sunna_desktop_${DISPLAY_NUM#:}"
SINK_MODULE="$LOG_DIR/sink${DISPLAY_NUM#:}.module"
mkdir -p "$LOG_DIR"

sound_server() { command -v pactl >/dev/null && pactl info >/dev/null 2>&1; }

if [ -n "${STOP:-}" ]; then
  # Everything the desktop session started shares its session id.
  if [ -f "$SESSION_PID" ] && kill -0 "$(cat "$SESSION_PID")" 2>/dev/null; then
    pkill -TERM -s "$(cat "$SESSION_PID")" || true
  fi
  rm -f "$SESSION_PID"
  pkill -TERM -f "^Xvfb $DISPLAY_NUM " || true
  if [ -f "$SINK_MODULE" ] && sound_server; then
    pactl unload-module "$(cat "$SINK_MODULE")" 2>/dev/null || true
  fi
  rm -f "$SINK_MODULE"
  echo "Stopped the desktop on $DISPLAY_NUM."
  exit 0
fi

DESKTOP="${SUNNA_DESKTOP:-}"
if [ -z "$DESKTOP" ]; then
  if command -v startplasma-x11 >/dev/null; then DESKTOP=plasma; else DESKTOP=xfce; fi
fi
case "$DESKTOP" in
  # No compositing: nobody sees this screen directly, and on Xvfb KWin's
  # compositor renders in software (measured ~65% of a core while idle).
  plasma) SESSION_CMD=startplasma-x11; export KWIN_COMPOSE=N ;;
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

# The desktop's own sound output (see above).
SOUND_ENV=()
if sound_server; then
  if ! pactl list short sinks | awk '{print $2}' | grep -qx "$SINK"; then
    pactl load-module module-null-sink sink_name="$SINK" \
      sink_properties=device.description="Sunna-desktop-${DISPLAY_NUM#:}" >"$SINK_MODULE"
  fi
  SOUND_ENV=(PULSE_SINK="$SINK")
fi

# The display can outlive its desktop session (an earlier version tied the
# session's D-Bus to the SSH connection); start the session if it's gone.
if [ -f "$SESSION_PID" ] && kill -0 "$(cat "$SESSION_PID")" 2>/dev/null; then
  echo "Desktop session on $DISPLAY_NUM is running."
else
  env DISPLAY="$DISPLAY_NUM" "${SOUND_ENV[@]}" setsid nohup dbus-run-session -- "$SESSION_CMD" \
    >"$LOG_DIR/$DESKTOP.log" 2>&1 </dev/null &
  echo $! >"$SESSION_PID"
  echo "Started $DESKTOP on $DISPLAY_NUM."
fi
echo "Stop it (and the apps in it) with: $0 stop $DISPLAY_NUM"
