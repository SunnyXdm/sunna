#!/bin/bash
# Exercise reconnect negotiation after a real X server changes dimensions.
set -euo pipefail
[[ -e /dev/virtio-ports/org.qemu.guest_agent.0 && $(ls /sys/class/net) == lo ]]
session=$(loginctl show-seat seat0 -p ActiveSession --value)
[[ $(loginctl show-session "$session" -p Name --value) == lightdm ]]
DISPLAY=$(loginctl show-session "$session" -p Display --value)
pid=$(pgrep -x Xorg)
[[ $pid =~ ^[0-9]+$ ]]
XAUTHORITY=$(python3 -I -c 'import pathlib,sys
a=pathlib.Path(sys.argv[1]).read_bytes().split(bytes([0]))
print(a[a.index(b"-auth")+1].decode())' "/proc/$pid/cmdline")
export DISPLAY XAUTHORITY
before=$(xrandr --current | awk '/\*/ {print $1; exit}')
[[ $before == 1280x800 ]]
/usr/local/bin/check-guest /tmp/size-before.y4m
head -n 1 /tmp/size-before.y4m | grep -q ' W640 H400 '
trap 'xrandr -s "$before"' EXIT
# Keep the viewer and its modifier down across XRandR. Query the same server
# independently so a reconnect cannot mask missing input cleanup.
shift_state() {
    python3 -I - "$1" <<'PYKEY'
import ctypes as c, sys, time
x = c.CDLL("libX11.so.6")
x.XOpenDisplay.restype = c.c_void_p
d = c.c_void_p(x.XOpenDisplay(None))
assert d.value
code = x.XKeysymToKeycode(d, 0xffe1)
assert code
bits = c.create_string_buffer(32)
for _ in range(240):
    x.XQueryKeymap(d, bits)
    if bool(bits.raw[code // 8] & (1 << (code % 8))) == (sys.argv[1] == "down"):
        break
    time.sleep(.1)
else:
    raise AssertionError("unexpected Shift state")
PYKEY
}
shift_state up
printf 'keydown 38\nwait 60000\n' | /usr/local/bin/check-guest /tmp/resize-held.y4m --steps >/tmp/resize-held.log 2>&1 &
viewer=$!
shift_state down
xrandr -s 1024x768
shift_state up
wait "$viewer" || true
for _ in {1..8}; do
    if /usr/local/bin/check-guest /tmp/size-after.y4m &&
        head -n 1 /tmp/size-after.y4m | grep -q ' W640 H480 '; then
        echo 'Reconnected at 640x480 after a 640x400 session.'
        exit 0
    fi
    sleep 1
done
exit 1
