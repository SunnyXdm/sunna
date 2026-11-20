FROM ubuntu:24.04
ENV container=docker
RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends systemd systemd-sysv libpam-systemd dbus-user-session lightdm lightdm-gtk-greeter xfce4-session xfwm4 xfce4-panel xfdesktop4 xterm xvfb xauth x11-utils dbus-x11 fonts-dejavu-core libgl1-mesa-dri && rm -rf /var/lib/apt/lists/*
COPY lightdm.conf /etc/lightdm/lightdm.conf.d/90-test.conf
COPY xvfb-seat /usr/local/bin/xvfb-seat
RUN chmod 755 /usr/local/bin/xvfb-seat && useradd -m -s /bin/bash viewer1 && useradd -m -s /bin/bash viewer2 && systemctl enable lightdm && systemctl set-default graphical.target
STOPSIGNAL SIGRTMIN+3
CMD ["/sbin/init"]
