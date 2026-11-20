#!/bin/bash
# Run the production desktop viewer against the installed system host in QEMU.
set -euo pipefail
[[ -e /dev/virtio-ports/org.qemu.guest_agent.0 && $(ls /sys/class/net) == lo ]]
viewer=${1:?path to sunna-cli}
xvfb=${2:?path to Xvfb}
work=$(mktemp -d)
gui=
server=
cleanup() {
    status=$?
    if (( status != 0 )); then
        grep -Ei 'presenter|failed|error|panic|viewer window' "$work"/{x,viewer}.log 2>/dev/null |
            grep -Evi 'token|cookie|auth=|key=|sunna://' | tail -8 || true
    fi
    if [[ -n $gui ]]; then kill "$gui" 2>/dev/null || true; fi
    if [[ -n $server ]]; then kill "$server" 2>/dev/null || true; fi
    systemctl start display-manager
    rm -rf "$work"
}
trap cleanup EXIT
export DISPLAY=:100 XAUTHORITY="$work/auth" XDG_RUNTIME_DIR="$work"
[[ ! -e /tmp/.X11-unix/X100 ]]
touch "$XAUTHORITY"
printf 'add %s . %s\n' "$DISPLAY" "$(mcookie)" | xauth -q
"$xvfb" "$DISPLAY" -nolisten tcp -auth "$XAUTHORITY" -screen 0 640x480x24 >"$work/x.log" 2>&1 &
server=$!
for _ in {1..40}; do xdpyinfo >/dev/null 2>&1 && break; sleep .25; done
export SUNNA_TOKEN="$(</etc/sunna/host.key)" SUNNA_WINDOWED=1 SUNNA_VIEW_SCALE=0.25
export RUST_LOG='warn,sunna_viewer=info' SUNNA_LOG_URL=
"$viewer" view 127.0.0.1:48800 --name fixture-window --no-audio >"$work/viewer.log" 2>&1 &
gui=$!
await_present() {
    for _ in {1..90}; do
        kill -0 "$gui" || return 1
        if [[ $(grep -c 'viewer present' "$work/viewer.log" || true) -gt "$1" ]]; then return; fi
        sleep .5
    done
    return 1
}
window() { xwininfo -root -tree | awk '/fixture-window/ {print $1; exit}'; }
await_present 0
first=$(window)
[[ -n $first ]]
systemctl stop display-manager
for _ in {1..40}; do
    [[ $(/usr/local/bin/check-guest check) == not-sharing* ]] && break
    sleep .5
done
[[ $(/usr/local/bin/check-guest check) == not-sharing* && $(window) == "$first" ]]
count=$(grep -c 'viewer present' "$work/viewer.log")
systemctl start display-manager
for _ in {1..90}; do
    [[ $(/usr/local/bin/check-guest check) == busy* ]] && break
    sleep .5
done
await_present "$count"
[[ $(/usr/local/bin/check-guest check) == busy* && $(window) == "$first" ]]
echo "Viewer PID $gui presented both sessions in X window $first."
