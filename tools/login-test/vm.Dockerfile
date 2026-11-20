ARG RELEASE=24.04
FROM ubuntu:${RELEASE}
ARG MANAGER=lightdm
RUN set -eu; case "$MANAGER" in \
 lightdm) packages='lightdm lightdm-gtk-greeter xfce4' ;; \
 sddm) packages='sddm plasma-desktop plasma-workspace' ;; \
 gdm-x11|gdm-wayland) packages='gdm3 ubuntu-session gnome-shell pipewire wireplumber xdg-desktop-portal-gnome' ;; \
 *) exit 2 ;; esac; \
 apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends linux-image-virtual systemd-sysv qemu-guest-agent libpam-systemd dbus-user-session udev kmod xserver-xorg xauth x11-utils mesa-utils libgl1-mesa-dri fonts-dejavu-core $packages && rm -rf /var/lib/apt/lists/*
RUN set -eu; case "$MANAGER" in \
 gdm-x11) sed -i 's/^#*WaylandEnable=.*/WaylandEnable=false/' /etc/gdm3/custom.conf; grep -qx 'WaylandEnable=false' /etc/gdm3/custom.conf ;; \
 gdm-wayland) if test -f /etc/gdm3/custom.conf; then sed -i '/^\[daemon\]/a PreferredDisplayServer=wayland' /etc/gdm3/custom.conf; fi ;; \
 sddm) mkdir -p /etc/sddm.conf.d; printf '[General]\nDisplayServer=x11\n' >/etc/sddm.conf.d/test.conf ;; esac; \
 useradd -ms /bin/bash viewer1 && useradd -ms /bin/bash viewer2 && systemctl set-default graphical.target && test -L /etc/systemd/system/display-manager.service
RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends initramfs-tools && update-initramfs -c -k "$(ls /lib/modules | head -1)" && rm -rf /var/lib/apt/lists/*

RUN apt-get update && extra="" && if [ "$MANAGER" = lightdm ]; then extra=light-locker; fi && DEBIAN_FRONTEND=noninteractive apt-get install --no-install-recommends -y curl python3 $extra && rm -rf /var/lib/apt/lists/*
