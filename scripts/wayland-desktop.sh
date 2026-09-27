#!/usr/bin/env bash
# A Wayland desktop without a screen, for developing and testing Sunna's
# Wayland support: KWin on its virtual backend with the Plasma shell, on a
# private D-Bus. It runs only when you start it (never at boot), and it
# doesn't touch the machine's real screens or its normal Plasma settings.
#
#   scripts/wayland-desktop.sh start [WxH]   # default 1710x1112
#   scripts/wayland-desktop.sh stop          # ends it and every app in it
#   scripts/wayland-desktop.sh status
#   scripts/wayland-desktop.sh run CMD...    # run a program inside it
#   eval "$(scripts/wayland-desktop.sh env)" # or join it from this shell
#
# Needs KDE Plasma 6 (kwin, plasma-workspace) and xdg-desktop-portal-kde.
set -euo pipefail

UNIT=sunna-wayland-desktop
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
STATE="$XDG_RUNTIME_DIR/sunna-wayland"
SOCKET=wayland-sunna

die() { echo "wayland-desktop: $*" >&2; exit 1; }
running() { systemctl --user is-active --quiet "$UNIT"; }

session_env() {
  [ -f "$STATE/env" ] || die "not running (scripts/wayland-desktop.sh start)"
  cat "$STATE/env"
}

cmd_start() {
  local size="${1:-1710x1112}"
  [[ "$size" =~ ^[0-9]+x[0-9]+$ ]] || die "size must look like 1920x1080"
  command -v kwin_wayland >/dev/null || die "kwin_wayland is missing (install kwin and plasma-workspace)"
  if running; then echo "Already running."; cmd_status; return; fi
  mkdir -p "$STATE"
  rm -f "$STATE/env"

  # What runs inside KWin: publish the session's environment, then the shell.
  cat >"$STATE/session.sh" <<'EOF'
#!/usr/bin/env bash
dbus-update-activation-environment --all
{
  for var in WAYLAND_DISPLAY DISPLAY DBUS_SESSION_BUS_ADDRESS XDG_RUNTIME_DIR \
             XDG_SESSION_TYPE XDG_CURRENT_DESKTOP XDG_SESSION_DESKTOP \
             KDE_FULL_SESSION KDE_SESSION_VERSION; do
    [ -n "${!var:-}" ] && printf 'export %s=%q\n' "$var" "${!var}"
  done
} >"$SUNNA_WAYLAND_STATE/env.tmp"
mv "$SUNNA_WAYLAND_STATE/env.tmp" "$SUNNA_WAYLAND_STATE/env"
kded6 &
plasmashell &
exec sleep infinity
EOF
  chmod 755 "$STATE/session.sh"

  systemd-run --user --unit="$UNIT" --collect --quiet \
    -p KillMode=control-group -p TimeoutStopSec=10 \
    --setenv=SUNNA_WAYLAND_STATE="$STATE" \
    --setenv=XDG_SESSION_TYPE=wayland \
    --setenv=XDG_CURRENT_DESKTOP=KDE \
    --setenv=XDG_SESSION_DESKTOP=KDE \
    --setenv=KDE_FULL_SESSION=true \
    --setenv=KDE_SESSION_VERSION=6 \
    dbus-run-session -- kwin_wayland --virtual --width "${size%x*}" --height "${size#*x}" \
    --socket "$SOCKET" --xwayland --no-lockscreen --exit-with-session "$STATE/session.sh"

  local i
  for i in $(seq 40); do
    [ -f "$STATE/env" ] && break
    running || break
    sleep 0.25
  done
  [ -f "$STATE/env" ] || {
    echo "It didn't start. Its log:" >&2
    journalctl --user -u "$UNIT" -n 30 -o cat --no-pager >&2 || true
    exit 1
  }
  echo "Started a $size Wayland desktop (KWin + Plasma, socket $SOCKET)."
  echo "Stop it with: $0 stop"
}

cmd_stop() {
  if running; then
    systemctl --user stop "$UNIT"
    echo "Stopped."
  else
    echo "Not running."
  fi
  rm -f "$STATE/env"
}

cmd_status() {
  if running; then
    echo "Running ($UNIT): $(systemctl --user show -p MainPID --value "$UNIT" | xargs -r ps -o etime= -p | xargs) so far."
    session_env | sed 's/^export /  /'
  else
    echo "Not running."
  fi
}

cmd_run() {
  [ "$#" -gt 0 ] || die "usage: $0 run CMD..."
  eval "$(session_env)"
  exec "$@"
}

case "${1:-}" in
  start) shift; cmd_start "$@" ;;
  stop) cmd_stop ;;
  status) cmd_status ;;
  env) session_env ;;
  run) shift; cmd_run "$@" ;;
  *) awk 'NR > 1 && /^#/ { sub(/^# ?/, ""); print; next } NR > 1 { exit }' "$0"; exit 2 ;;
esac
