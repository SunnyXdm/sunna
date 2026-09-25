#!/usr/bin/env bash
# Build and run Sunna between two Macs on the same tailnet, shipping logs to
# the dogfood collector (tools/logd).
#
#   scripts/dogfood.sh host           # on the Mac to control
#   scripts/dogfood.sh view <host>    # on the Mac you sit at; <host> is the
#                                     # host's Tailscale name or 100.x address
#
# One-time setup on each Mac: ~/.sunna/dogfood.env containing
#   SUNNA_TOKEN=<same value on both Macs>
#   SUNNA_LOG_URL=http://<collector tailnet IP>:48900
#   SUNNA_LOG_TOKEN=<collector token>
#
# First run of `host`: macOS asks for Screen Recording (and Accessibility for
# remote input) for your terminal app. Grant both, quit and reopen the
# terminal, run again.
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
cargo build --release

PORT=48800
case "${1:-}" in
  host)
    IP="$(tailscale_cli ip -4 | head -1)"
    [ -n "$IP" ] || { echo "no Tailscale IPv4 address; is Tailscale connected?" >&2; exit 1; }
    NAME="$(scutil --get ComputerName 2>/dev/null || hostname)"
    echo "Hosting on $IP:$PORT (tailnet only). Ctrl-C to stop."
    exec ./target/release/sunnad --source screen --listen "$IP:$PORT" --name "$NAME"
    ;;
  view)
    HOST="${2:?usage: $0 view <host Tailscale name or 100.x address>}"
    if [[ ! "$HOST" =~ ^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
      RESOLVED="$(tailscale_cli ip -4 "$HOST" 2>/dev/null | head -1 || true)"
      [ -n "$RESOLVED" ] || { echo "can't resolve $HOST on the tailnet (try its 100.x address)" >&2; exit 1; }
      HOST="$RESOLVED"
    fi
    echo "Viewing $HOST:$PORT. Close the window to end the session."
    exec ./target/release/sunna-cli view "$HOST:$PORT"
    ;;
  *)
    sed -n '2,17p' "$0"
    exit 2
    ;;
esac
