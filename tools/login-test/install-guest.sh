#!/bin/bash
set -euo pipefail
export HOME=/root
# Run through the private guest agent in the generated VM.
[[ $(id -u) == 0 && -f /srv/sunna-test-feed/0.1.0/install.sh ]]
cat >/etc/systemd/system/sunna-login-feed.service <<'UNIT'
[Unit]
Description=Sunna fixture release feed
[Service]
ExecStart=/usr/bin/python3 -m http.server 48991 --bind 127.0.0.1 --directory /srv/sunna-test-feed
[Install]
WantedBy=multi-user.target
UNIT
systemctl daemon-reload
systemctl enable --now sunna-login-feed
for _ in {1..60}; do
    curl -fsS -o /dev/null http://127.0.0.1:48991/0.1.0/install.sh 2>/dev/null && break
    sleep 1
done
chmod 755 /usr/local/bin/login-test
# This guest has no NIC. These public credentials are fixture data only.
for u in viewer1 viewer2; do printf '%s:%s\n' "$u" "$u" | chpasswd; done
sh /srv/sunna-test-feed/0.1.0/install.sh host --system --listen 127.0.0.1:48800
echo SUNNA_UPDATE=off >>/etc/sunna/host.env
echo SUNNA_UPDATE_URL=http://127.0.0.1:48991/selected >>/etc/sunna/host.env
ln -sfn 0.1.0 /srv/sunna-test-feed/selected
systemctl restart sunna-host
