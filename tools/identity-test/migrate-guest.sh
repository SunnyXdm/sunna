#!/bin/bash
# Run after the update proof inside the private LightDM guest with sudo installed.
set -euo pipefail
[[ -e /dev/virtio-ports/org.qemu.guest_agent.0 && $(ls /sys/class/net) == lo ]]
root=/opt/sunna/host/current
home=/home/viewer1
addr=127.0.0.1:48800
umask 077
saved=$(mktemp -d /root/sunna-id-migration.XXXXXXXX)
systemctl stop sunna-host
for file in host.env host.identity host.key; do mv "/etc/sunna/$file" "$saved/"; done
cleanup() {
    systemctl stop sunna-host
    if [[ -d "$saved/previous-install" ]]; then
        if [[ -e /opt/sunna ]]; then mv /opt/sunna "$saved/migrated-install"; fi
        mv "$saved/previous-install" /opt/sunna
    fi
    gpasswd -d viewer1 sudo >/dev/null
    rm -f /etc/sudoers.d/sunna-id-migration
    for file in host.env host.identity host.key; do mv "$saved/$file" "/etc/sunna/$file"; done
    rm -rf "$saved"
    systemctl restart sunna-host
}
trap cleanup EXIT
loginctl enable-linger viewer1
uid=$(id -u viewer1)
systemctl start "user@$uid.service"
user=(runuser -u viewer1 -- env HOME="$home" XDG_CONFIG_HOME="$home/.config" XDG_RUNTIME_DIR="/run/user/$uid" DBUS_SESSION_BUS_ADDRESS="unix:path=/run/user/$uid/bus")
# This is the private test account installing its guest system service.
usermod -aG sudo viewer1
printf 'Defaults:viewer1 !authenticate\n' > /etc/sudoers.d/sunna-id-migration
chmod 440 /etc/sudoers.d/sunna-id-migration
export SUNNA_TOKEN
SUNNA_TOKEN=$(od -An -N16 -tx1 /dev/urandom | tr -d ' \n')
"${user[@]}" /bin/bash -ec '
    umask 077
    mkdir -p "$HOME/.config/sunna"
    printf "SUNNA_MODE=desktop\nSUNNA_TOKEN=%s\nSUNNA_LISTEN=127.0.0.1:48800\nSUNNA_PORT=48800\n" "$SUNNA_TOKEN" >"$HOME/.config/sunna/host.env"
'
pin=$("${user[@]}" "$root/bin/sunnad" identity)
# Start a per-user listener and save its pin using a separate viewer profile.
"${user[@]}" systemd-run --user --unit=sunna-id-source --setenv=SUNNA_TOKEN -- "$root/bin/sunnad" --source synthetic --listen "$addr" --codec h264 --no-audio --no-clipboard
for _ in {1..20}; do
    if HOME="$saved" /usr/local/bin/login-test "$addr" check | grep -q '^ready'; then break; fi
    sleep 1
done
HOME="$saved" /usr/local/bin/login-test "$addr" "$saved/before.y4m"
"${user[@]}" systemctl --user stop sunna-id-source
# Migration starts with no system installation. Preserve the separate rollback
# fixture, including its accepted version floor, for the remaining guest tests.
mv /opt/sunna "$saved/previous-install"
"${user[@]}" /bin/sh /srv/sunna-test-feed/0.2.0/install.sh host --system
[[ $("$root/bin/sunnad" identity --system-host) == "$pin" ]]
[[ $(stat -c '%u:%a' /etc/sunna/host.identity) == 0:600 ]]
"${user[@]}" test ! -e "$home/.config/sunna/host.identity"
"$root/share/sunna/system-host.sh" link | python3 -c '
import os, sys
from urllib.parse import urlsplit, parse_qs
query=parse_qs(urlsplit(sys.stdin.read().strip()).query)
assert query["id"] == [sys.argv[1]] and query["key"] == [os.environ["SUNNA_TOKEN"]]
' "$pin"
HOME="$saved" /usr/local/bin/login-test "$addr" "$saved/after.y4m"
echo "The same saved viewer verified the per-user host and the migrated LightDM host."
