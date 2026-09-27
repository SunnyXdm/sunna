#!/usr/bin/env bash
# Build and run Sunna between two machines on the same tailnet, shipping
# logs to the dogfood collector (tools/logd).
#
#   scripts/dogfood.sh host           # on the Mac or Linux box to control
#   scripts/dogfood.sh view <host>    # on the Mac you sit at; <host> is the
#                                     # host's Tailscale name or 100.x address
#   scripts/dogfood.sh app            # the Sunna app: pick a machine, connect
#
# One-time setup on each Mac: ~/.sunna/dogfood.env containing
#   SUNNA_TOKEN=<same value on both Macs>
#   SUNNA_LOG_URL=http://<collector tailnet IP>:48900
#   SUNNA_LOG_TOKEN=<collector token>
#
# First run of `host`: macOS asks for Screen Recording (and Accessibility for
# remote input) for your terminal app. Grant both, quit and reopen the
# terminal, run again.
#
# SUNNA_KEEP_AWDL_DOWN=1 (env or dogfood.env): hold AWDL (AirDrop/Continuity
# Wi-Fi) down for the session. macOS re-enables awdl0 on its own; while it's
# up, Wi-Fi stalls ~200 ms about once a second. Needs sudo; AWDL is restored
# when the script exits.
set -euo pipefail
cd "$(dirname "$0")/.."

ENV_FILE="${SUNNA_ENV:-$HOME/.sunna/dogfood.env}"
if [ -f "$ENV_FILE" ]; then
  set -a
  # shellcheck disable=SC1090
  . "$ENV_FILE"
  set +a
fi
: "${SUNNA_TOKEN:?set SUNNA_TOKEN in $ENV_FILE (same value on both Macs)}"
[ -n "${SUNNA_LOG_URL:-}" ] || echo "note: SUNNA_LOG_URL not set; logs stay local"

tailscale_cli() {
  if command -v tailscale >/dev/null 2>&1; then
    tailscale "$@"
  elif [ -x /Applications/Tailscale.app/Contents/MacOS/Tailscale ]; then
    /Applications/Tailscale.app/Contents/MacOS/Tailscale "$@"
  else
    echo "Tailscale CLI not found" >&2
    return 1
  fi
}

export SUNNA_GIT_HASH="$(git rev-parse --short HEAD)$(git diff --quiet || echo -dirty)"
if [ "${1:-}" = app ]; then
  cargo build --release -p sunna
else
  cargo build --release
fi

AWDL_LOOP=""
cleanup() {
  if [ "${1:-}" = host ] && [ "$(uname)" = Linux ] && [ "${SUNNA_DISPLAY:-:40}" != :0 ]; then
    echo "The virtual desktop on ${SUNNA_DISPLAY:-:40} is still running, with any apps opened in it."
    echo "Stop it with: scripts/linux-desktop.sh stop"
  fi
  if [ -n "$AWDL_LOOP" ]; then
    kill "$AWDL_LOOP" 2>/dev/null || true
    sudo -n ifconfig awdl0 up 2>/dev/null && echo "AWDL restored."
  fi
}
trap 'cleanup "${1:-}"' EXIT INT TERM
if [ "${SUNNA_KEEP_AWDL_DOWN:-0}" = "1" ] && ifconfig awdl0 >/dev/null 2>&1; then
  echo "Holding AWDL down for this session (sudo password may be asked once)."
  sudo -v
  # `sudo -n -v` keeps the sudo timestamp fresh past its 5-minute default.
  ( while true; do sudo -n -v 2>/dev/null; sudo -n ifconfig awdl0 down 2>/dev/null; sleep 1; done ) &
  AWDL_LOOP=$!
elif ifconfig awdl0 2>/dev/null | grep -q "status: active"; then
  echo "warning: AWDL is active; expect ~200 ms Wi-Fi stalls every second."
  echo "         Rerun with SUNNA_KEEP_AWDL_DOWN=1 to hold it down for the session."
fi

PORT=48800
case "${1:-}" in
  host)
    IP="$(tailscale_cli ip -4 | head -1)"
    [ -n "$IP" ] || { echo "no Tailscale IPv4 address; is Tailscale connected?" >&2; exit 1; }
    NAME="$(scutil --get ComputerName 2>/dev/null || hostname)"
    if [ "$(uname)" = Linux ]; then
      # X11 host. Without a real screen (servers, the dev VM) this starts a
      # virtual XFCE desktop; SUNNA_DISPLAY=:0 hosts an existing session.
      export DISPLAY="${SUNNA_DISPLAY:-:40}"
      scripts/linux-desktop.sh "$DISPLAY"
    fi
    # What another computer needs to add this one in the Sunna app.
    cat <<EOF

  Sharing $NAME with Sunna (tailnet only). Ctrl-C to stop.

    Address   $IP
    Key       $SUNNA_TOKEN

  In the Sunna app on the other computer, choose Add a Computer and paste:
    sunna://$IP?key=$SUNNA_TOKEN

EOF
    ./target/release/sunnad --source screen --listen "$IP:$PORT" --name "$NAME"
    ;;
  app)
    echo "Opening Sunna."
    ./target/release/sunna
    ;;
  view)
    HOST="${2:?usage: $0 view <host Tailscale name or 100.x address>}"
    NAME="$HOST"
    if [[ ! "$HOST" =~ ^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
      RESOLVED="$(tailscale_cli ip -4 "$HOST" 2>/dev/null | head -1 || true)"
      [ -n "$RESOLVED" ] || { echo "can't resolve $HOST on the tailnet (try its 100.x address)" >&2; exit 1; }
      HOST="$RESOLVED"
    fi
    echo "Viewing $HOST:$PORT. Close the window to end the session."
    ./target/release/sunna-cli view "$HOST:$PORT" --name "$NAME"
    ;;
  *)
    sed -n '2,18p' "$0"
    exit 2
    ;;
esac
