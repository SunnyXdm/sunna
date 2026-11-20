#!/bin/bash
# Inside container: session.sh USER DISPLAY WIDTHxHEIGHT DESKTOP
set -eu
user=$1; display=$2; size=$3; desktop=$4
uid=$(id -u "$user")
Xvfb "$display" -screen 0 "${size}x24" -ac +extension GLX +render -noreset >"/tmp/x-${user}.log" 2>&1 &
sleep 2
case "$desktop" in
 kde) command='export KDE_FULL_SESSION=true XDG_CURRENT_DESKTOP=KDE; kwriteconfig6 --file startkderc --group General --key systemdBoot false; startplasma-x11';;
 gnome) command='export XDG_CURRENT_DESKTOP=GNOME; gsettings set org.gnome.desktop.interface color-scheme default; gsettings set org.gnome.desktop.session idle-delay 0; gnome-shell --x11';;
 xfce) command='export XDG_CURRENT_DESKTOP=XFCE; startxfce4';;
 mate) command='export XDG_CURRENT_DESKTOP=MATE; mate-session';;
esac
runuser -u "$user" -- env DISPLAY="$display" XDG_RUNTIME_DIR="/run/user/$uid" LANG=C.UTF-8 LIBGL_ALWAYS_SOFTWARE=1 NO_AT_BRIDGE=1 dbus-run-session -- bash -c "env > /tmp/env-$user; $command" >"/tmp/session-$user.log" 2>&1 &
wait
