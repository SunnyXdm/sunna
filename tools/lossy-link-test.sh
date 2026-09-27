#!/usr/bin/env bash
# Run a Sunna host and a headless viewer across an emulated bad link: two
# network namespaces joined by a virtual cable, with netem adding delay and
# (bursty) loss on the host's side. Linux, needs sudo; nothing outside the
# two namespaces is touched, and they're removed afterwards.
#
#   tools/lossy-link-test.sh [seconds]          # default 30
#
# Settings (environment):
#   DELAY_MS   one-way delay each way, default 17 (round trip ~34 ms)
#   LOSS       netem loss on the host's side, default "gemodel 1% 30%":
#              Wi-Fi-like bursts, ~3% loss, ~3 packets in a row
#   NOISE      percent of each frame that is fresh noise, default 8
#   SIZE FPS KBPS CODEC   default 1280x720, 60, 12000, h264
#   HOST_ENV   extra environment for sunnad, e.g. "SUNNA_CC=bbr"
#   AUDIO      1 to send a test tone too, recorded on the viewer side and
#              checked for gaps (tools/audio-check.py)
#   OUT        where the logs go, default /tmp/sunna-lossy
set -euo pipefail
cd "$(dirname "$0")/.."

RUN_SECONDS="${1:-30}"
DELAY_MS="${DELAY_MS:-17}"
LOSS="${LOSS:-gemodel 1% 30%}"
NOISE="${NOISE:-8}"
SIZE="${SIZE:-1280x720}"
FPS="${FPS:-60}"
KBPS="${KBPS:-12000}"
CODEC="${CODEC:-h264}"
HOST_ENV="${HOST_ENV:-}"
AUDIO="${AUDIO:-0}"
VIEW_AUDIO=()
if [ "$AUDIO" = 1 ]; then
  HOST_ENV="$HOST_ENV SUNNA_AUDIO_SOURCE=synthetic"
  VIEW_AUDIO=(--audio)
fi
OUT="${OUT:-/tmp/sunna-lossy}"
NS_HOST=sunna-lossy-h
NS_VIEW=sunna-lossy-v
ADDR=10.231.0.1
PORT=48999
TOKEN=lossy-test

[ -x target/release/sunnad ] && [ -x target/release/sunna-cli ] ||
  { echo "build first: cargo build --release -p sunnad -p sunna-cli" >&2; exit 1; }

teardown() {
  [ -n "${HOST_PID:-}" ] && sudo kill "$HOST_PID" 2>/dev/null || true
  sudo ip netns del "$NS_HOST" 2>/dev/null || true
  sudo ip netns del "$NS_VIEW" 2>/dev/null || true
}
trap teardown EXIT
teardown

sudo ip netns add "$NS_HOST"
sudo ip netns add "$NS_VIEW"
sudo ip link add sunna-lossy0 type veth peer name sunna-lossy1
sudo ip link set sunna-lossy0 netns "$NS_HOST"
sudo ip link set sunna-lossy1 netns "$NS_VIEW"
sudo ip -n "$NS_HOST" addr add "$ADDR/24" dev sunna-lossy0
sudo ip -n "$NS_VIEW" addr add 10.231.0.2/24 dev sunna-lossy1
for ns in "$NS_HOST" "$NS_VIEW"; do sudo ip -n "$ns" link set lo up; done
sudo ip -n "$NS_HOST" link set sunna-lossy0 up
sudo ip -n "$NS_VIEW" link set sunna-lossy1 up
# shellcheck disable=SC2086
sudo ip netns exec "$NS_HOST" tc qdisc add dev sunna-lossy0 root netem delay "${DELAY_MS}ms" loss $LOSS limit 100000
sudo ip netns exec "$NS_VIEW" tc qdisc add dev sunna-lossy1 root netem delay "${DELAY_MS}ms" limit 100000

mkdir -p "$OUT"
# shellcheck disable=SC2086
sudo ip netns exec "$NS_HOST" sudo -u "$USER" env NO_COLOR=1 SUNNA_SYNTHETIC_NOISE="$NOISE" $HOST_ENV \
  ./target/release/sunnad --source synthetic --width "${SIZE%x*}" --height "${SIZE#*x}" --fps "$FPS" \
  --codec "$CODEC" --bitrate-kbps "$KBPS" --listen "$ADDR:$PORT" --token "$TOKEN" --name lossy \
  >"$OUT/host.log" 2>&1 &
HOST_PID=$!
sleep 1
sudo ip netns exec "$NS_VIEW" sudo -u "$USER" env NO_COLOR=1 RUST_LOG=info,sunna_client=debug SUNNA_TOKEN="$TOKEN" \
  SUNNA_AUDIO_OUTPUT="$OUT/audio.raw" ./target/release/sunna-cli connect "$ADDR:$PORT" --seconds "$RUN_SECONDS" \
  "${VIEW_AUDIO[@]}" >"$OUT/viewer.log" 2>&1 || true

python3 - "$OUT/host.log" "$OUT/viewer.log" <<'EOF'
import re, sys
def windows(path, name):
    rows = []
    for line in open(path, errors="replace"):
        if name not in line:
            continue
        rows.append(dict(re.findall(r'(\w+)=("[^"]*"|\S+)', line)))
    return rows
def num(value):
    try: return float(str(value).strip('"'))
    except ValueError: return None
def p50(field_value):
    m = re.search(r"p50=([0-9.]+)ms", field_value or ""); return float(m.group(1)) if m else None
host = windows(sys.argv[1], "host window")[2:]      # skip warm-up
view = [w for w in windows(sys.argv[2], " window ") if "dropped" in w][2:]
view_lat = [float(m) for m in re.findall(r"latency=n=\d+ mean=[0-9.]+ms p50=([0-9.]+)ms", open(sys.argv[2], errors="replace").read())][2:]
freezes = [float(m) for m in re.findall(r"stalled_ms=([0-9.]+)", open(sys.argv[2], errors="replace").read())]
def mean(xs): xs = [x for x in xs if x is not None]; return sum(xs) / len(xs) if xs else 0
def total(rows, key): return sum(num(r.get(key, 0)) or 0 for r in rows)
print(f"host    {mean([num(h.get('mbps')) for h in host]):5.1f} Mbps (target {mean([num(h.get('target_mbps')) for h in host]):.1f}), "
      f"{mean([num(h.get('fps')) for h in host]):.0f} fps sent, keyframes {total(host, 'keyframes'):.0f}, "
      f"sender drops {total(host, 'sender_drops'):.0f}")
print(f"quic    lost {total(host, 'quic_lost'):.0f} of {total(host, 'quic_sent'):.0f} packets, "
      f"backoffs {total(host, 'quic_backoffs'):.0f}, window {mean([num(h.get('quic_cwnd_kb')) for h in host]):.0f} KB, "
      f"rtt {mean([num(h.get('quic_rtt_ms')) for h in host]):.0f} ms")
print(f"viewer  {mean([num(v.get('fps')) for v in view]):.0f} fps shown, frames lost {total(view, 'dropped'):.0f}, "
      f"latency p50 {mean(view_lat):.0f} ms, "
      f"freezes {len(freezes)} totalling {sum(freezes)/1000:.1f} s (longest {max(freezes, default=0):.0f} ms)")
EOF
if [ "$AUDIO" = 1 ]; then
  grep -o 'audio_packets=[^ ]* audio_recovered=[^ ]* audio_concealed=[^ ]* audio_underruns=[^ ]* audio_buffered_ms=[^ ]*' "$OUT/viewer.log" | tail -1 | sed 's/^/sound   /'
  python3 tools/audio-check.py "$OUT/audio.raw" | sed 's/^/sound   /'
fi
