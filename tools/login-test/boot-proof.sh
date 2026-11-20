#!/bin/bash
# Install a one-boot observer inside a disconnected fixture.
set -euo pipefail
[[ -e /dev/virtio-ports/org.qemu.guest_agent.0 && -f /srv/sunna-test-feed/0.1.0/install.sh ]]
[[ $(ls /sys/class/net) == lo ]]
install -m755 /dev/stdin /usr/local/bin/observe-boot <<'PROOF'
#!/bin/bash
set -eu
for i in $(seq 180); do
  if [[ -f /run/sunna-host/shares-greeter ]] && /usr/local/bin/check-guest /tmp/reboot.y4m &&
      python3 -I -c 'p=open("/tmp/reboot.y4m","rb"); h=p.readline().split(); p.readline(); w,h=(int(v[1:]) for v in h[1:3]); y=p.read(w*h); exit(0 if sum(v>60 for v in y)>len(y)//100 else 1)'; then
    awk '{print "Boot to first saved frame (upper bound):", $1, "seconds"}' /proc/uptime
    exit 0
  fi
  sleep 1
done
exit 1
PROOF
cat > /etc/systemd/system/fixture-boot-proof.service <<'UNIT'
[Unit]
Description=Measure boot to first authenticated viewer frame
After=sunna-host.service
[Service]
Type=simple
ExecStart=/usr/local/bin/observe-boot
StandardOutput=file:/root/boot-frame.log
StandardError=journal
[Install]
WantedBy=multi-user.target
UNIT
systemctl daemon-reload
systemctl enable fixture-boot-proof
sync
systemd-run --on-active=3 /sbin/reboot
