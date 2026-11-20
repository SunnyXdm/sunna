#!/bin/bash
# Exercise both sides of the installer transaction with real system and user managers.
set -euo pipefail
[[ -e /dev/virtio-ports/org.qemu.guest_agent.0 && $(ls /sys/class/net) == lo ]]
feed=/srv/sunna-test-feed
unit=sunna-host.service
user=viewer1
home=/home/$user
uid=$(id -u "$user")
as_user() {
    runuser -u "$user" -- env HOME="$home" XDG_RUNTIME_DIR="/run/user/$uid" "$@"
}
if ! command -v sudo >/dev/null; then dpkg -i /root/sudo.deb; fi
printf '%s ALL=(root) NOPASSWD: ALL\n' "$user" > /etc/sudoers.d/sunna-fixture
chmod 440 /etc/sudoers.d/sunna-fixture
trap 'rm -f /etc/sudoers.d/sunna-fixture /run/systemd/system/sunna-host.service.d/fixture.conf' EXIT
loginctl enable-linger "$user"
systemctl start "user@$uid.service"
as_user mkdir -p "$home/.config/systemd/user" "$home/.config/sunna" "$home/fixture/bin"
as_user tar -xzf "$feed/0.1.0/sunna-host-linux-x86_64.tar.gz" -C "$home/fixture"
config="$home/.config/sunna"
umask 077
{
    printf 'SUNNA_LISTEN=127.0.0.1:48800\nSUNNA_PORT=48800\nSUNNA_MODE=desktop\nSUNNA_TOKEN='
    cat /etc/sunna/host.key
    printf '\n'
} >"$config/host.env"
install -m600 /etc/sunna/host.identity "$config/host.identity"
chown "$user" "$config/host.env" "$config/host.identity"
cp "$config/host.identity" /root/prior-identity
as_user tee "$home/.config/systemd/user/$unit" >/dev/null <<'UNIT'
[Unit]
Description=Previous Sunna desktop fixture
[Service]
EnvironmentFile=%h/.config/sunna/host.env
ExecStart=%h/fixture/bin/sunnad --listen 127.0.0.1:48800 --source synthetic --codec raw
[Install]
WantedBy=default.target
UNIT
systemctl stop fixture-boot-proof.service || true
/opt/sunna/host/current/share/sunna/system-host.sh uninstall --purge
as_user systemctl --user daemon-reload
as_user systemctl --user enable --now "$unit"
dropin=/run/systemd/system/$unit.d
mkdir -p "$dropin"
printf '[Service]\nExecStartPre=/bin/false\n' >"$dropin/fixture.conf"
systemctl daemon-reload
install_host() { as_user sh "$feed/0.1.0/install.sh" host --system; }
if install_host; then exit 1; fi
as_user systemctl --user is-active --quiet "$unit"
[[ -f "$config/host.env" && ! -e /etc/systemd/system/$unit && ! -e /etc/sunna/host.key ]]
cmp -s /root/prior-identity "$config/host.identity"
if systemctl is-enabled --quiet "$unit"; then exit 1; fi
rm "$dropin/fixture.conf"
systemctl daemon-reload
install_host
systemctl is-active --quiet "$unit"
systemctl is-enabled --quiet "$unit"
[[ ! -e "$config/host.env" && ! -e "$config/host.identity" ]]
cmp -s /root/prior-identity /etc/sunna/host.identity
/usr/local/bin/check-guest /tmp/after-migration.y4m
echo 'Failed candidate restored the user service. Successful migration retained its identity and delivered a frame.'
