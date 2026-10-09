#!/bin/bash
# Inside the container, as tester: a headless GNOME Shell session with its own
# D-Bus, PipeWire and portal. Writes env.sh for `docker exec` callers.
set -e
export XDG_RUNTIME_DIR=/tmp/xdg-1000
mkdir -p -m 700 $XDG_RUNTIME_DIR
export DBUS_SESSION_BUS_ADDRESS=unix:path=$XDG_RUNTIME_DIR/bus
export XDG_CURRENT_DESKTOP=GNOME XDG_SESSION_TYPE=wayland XDG_SESSION_DESKTOP=gnome XDG_SESSION_ID=c1
dbus-daemon --session --address=$DBUS_SESSION_BUS_ADDRESS --fork --nopidfile
pipewire > /tmp/pipewire.log 2>&1 &
sleep 1
wireplumber > /tmp/wireplumber.log 2>&1 &
sleep 1
flags="--headless --virtual-monitor ${SIZE:-1920x1080} --unsafe-mode"
gnome-shell --help 2>&1 | grep -q -- "--no-x11" && flags="$flags --no-x11"
gnome-shell --help 2>&1 | grep -q -- "--wayland" && flags="$flags --wayland"
gnome-shell $flags > /tmp/gnome-shell.log 2>&1 &
for i in $(seq 60); do [ -S $XDG_RUNTIME_DIR/wayland-0 ] && break; sleep 0.5; done
cat > /tmp/env.sh <<ENV
export XDG_RUNTIME_DIR=$XDG_RUNTIME_DIR DBUS_SESSION_BUS_ADDRESS=$DBUS_SESSION_BUS_ADDRESS
export XDG_CURRENT_DESKTOP=GNOME XDG_SESSION_TYPE=wayland XDG_SESSION_DESKTOP=gnome XDG_SESSION_ID=c1 WAYLAND_DISPLAY=wayland-0
ENV
ls $XDG_RUNTIME_DIR
