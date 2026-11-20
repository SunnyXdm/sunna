#!/bin/bash
# Run ONLY inside the disposable Debian container described in README.md.
set -eu
apt-get update
DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
 xvfb xauth dbus-x11 plasma-desktop plasma-workspace kwin-x11 plasma-integration breeze \
 konsole kate dolphin breeze-icon-theme gnome-shell gnome-session gnome-settings-daemon \
 nautilus gnome-text-editor gnome-terminal gnome-backgrounds desktop-base adwaita-icon-theme \
 xfce4 xfce4-terminal firefox-esr mate-desktop-environment-core libreoffice-calc libreoffice-gtk3 \
 krita htop xdotool wmctrl imagemagick python3-pil fonts-dejavu fonts-liberation fonts-noto-core \
 procps sudo mesa-utils x11-xserver-utils
