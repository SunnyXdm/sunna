# Sunna's GNOME (Wayland) test bed: a headless GNOME Shell with PipeWire and
# GNOME's desktop portal, isolated from the host's own sessions. See up.sh.
FROM archlinux:latest
RUN pacman -Syu --noconfirm --needed gnome-shell mutter xdg-desktop-portal xdg-desktop-portal-gnome \
      pipewire wireplumber dbus python python-dbus python-dbusmock python-gobject \
      gstreamer gst-plugins-base gst-plugin-pipewire procps-ng && pacman -Scc --noconfirm
RUN useradd -m -u 1000 tester
USER tester
WORKDIR /home/tester
