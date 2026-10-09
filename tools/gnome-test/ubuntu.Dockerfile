# The test bed on Ubuntu's own GNOME: RELEASE=24.04 (GNOME 46), 25.10 (49) or
# 26.04 (50). Built by up.sh when UBUNTU is set.
ARG RELEASE=26.04
FROM ubuntu:${RELEASE}
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends gnome-shell xdg-desktop-portal xdg-desktop-portal-gnome \
      pipewire wireplumber dbus dbus-user-session libglib2.0-bin python3 python3-dbus python3-dbusmock python3-gi \
      gir1.2-gstreamer-1.0 gstreamer1.0-pipewire gstreamer1.0-plugins-base procps && rm -rf /var/lib/apt/lists/*
RUN (userdel -r ubuntu 2>/dev/null || true) && useradd -m -u 1000 tester
USER tester
WORKDIR /home/tester
