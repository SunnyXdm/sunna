FROM ubuntu:26.04
ENV DEBIAN_FRONTEND=noninteractive container=docker
RUN apt-get update && apt-get install -y --no-install-recommends \
      systemd systemd-sysv dbus-user-session dbus-x11 at-spi2-core python3-pyatspi \
      xfce4-session xfwm4 xfdesktop4 xvfb xauth xdotool x11-utils x11-xserver-utils \
      gnome-shell xdg-desktop-portal-gnome wireplumber pipewire pipewire-pulse \
      gstreamer1.0-pipewire python3-dbusmock iproute2 imagemagick \
      libwebkit2gtk-4.1-0 libayatana-appindicator3-1 libpipewire-0.3-0t64 libasound2t64 \
      libgl1-mesa-dri libgles2 libvulkan1 && \
    usermod -l tester -d /home/tester -m ubuntu && \
    rm -rf /var/lib/apt/lists/*
CMD ["/sbin/init"]
