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
# (its monitor) and nothing else on the machine. If that output would
# become the machine's default (on a machine without a sound card), the rest
# of the machine gets a silent output of its own, sunna_silent, instead.
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
SILENT=sunna_silent
SILENT_MODULE="$LOG_DIR/silent.module"
mkdir -p "$LOG_DIR"

sound_server() { command -v pactl >/dev/null && pactl info >/dev/null 2>&1; }
default_device() { LC_ALL=C pactl info 2>/dev/null | sed -n "s/^Default $1: //p"; }
# The index of a sink or source by name.
device_index() { pactl list short "$1" | awk -v name="$2" '$2 == name { print $1 }'; }

# On a machine without a sound card, a desktop's output can become everyone's
# default: PulseAudio drops its placeholder output as soon as another exists,
# and PipeWire picks the only one there is. Other sessions' sound would then
# play into this desktop's output and reach its viewer, so point the
# defaults somewhere else: another output, or a silent one made for this.
keep_defaults_elsewhere() {
  case "$(default_device Sink)" in sunna_desktop_*) ;; *) return 0 ;; esac
  local other
  other="$(pactl list short sinks | awk '$2 !~ /^sunna_desktop_/ { print $2; exit }')"
  if [ -z "$other" ]; then
    pactl load-module module-null-sink sink_name="$SILENT" \
      sink_properties=device.description=Dummy-Output >"$SILENT_MODULE"
    other="$SILENT"
  fi
  pactl set-default-sink "$other"
  case "$(default_device Source)" in sunna_desktop_*) pactl set-default-source "$other.monitor" ;; esac
  echo "$other"
}

if [ -n "${STOP:-}" ]; then
  # Everything the desktop session started shares its session id.
  if [ -f "$SESSION_PID" ] && kill -0 "$(cat "$SESSION_PID")" 2>/dev/null; then
    pkill -TERM -s "$(cat "$SESSION_PID")" || true
  fi
  rm -f "$SESSION_PID"
  pkill -TERM -f "^Xvfb $DISPLAY_NUM " || true
  if [ -f "$SINK_MODULE" ] && sound_server; then
    pactl unload-module "$(cat "$SINK_MODULE")" 2>/dev/null || true
    # The silent stand-in goes with the last desktop.
    if [ -f "$SILENT_MODULE" ] &&
      ! pactl list short sinks | awk '$2 ~ /^sunna_desktop_/ { found = 1 } END { exit !found }'; then
      pactl unload-module "$(cat "$SILENT_MODULE")" 2>/dev/null || true
      rm -f "$SILENT_MODULE"
    fi
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
  if [ -z "$(device_index sinks "$SINK")" ]; then
    pactl load-module module-null-sink sink_name="$SINK" \
      sink_properties=device.description="Sunna-desktop-${DISPLAY_NUM#:}" >"$SINK_MODULE"
    other="$(keep_defaults_elsewhere)"
    if [ -n "$other" ]; then
      # Streams moved here when the placeholder went away aren't this
      # desktop's (it hasn't started yet): send them back.
      sink="$(device_index sinks "$SINK")"
      pactl list short sink-inputs | awk -v sink="$sink" '$2 == sink { print $1 }' |
        while read -r input; do pactl move-sink-input "$input" "$other" || true; done
      monitor="$(device_index sources "$SINK.monitor")"
      pactl list short source-outputs | awk -v source="$monitor" '$2 == source { print $1 }' |
        while read -r output; do pactl move-source-output "$output" "$other.monitor" || true; done
    fi
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
