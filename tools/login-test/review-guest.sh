#!/bin/bash
# Run only inside a disconnected login-test QEMU guest.
set -euo pipefail
root=/opt/sunna/host
unit=sunna-host.service
[[ -e /dev/virtio-ports/org.qemu.guest_agent.0 && $(ls /sys/class/net) == lo && -f /srv/sunna-test-feed/0.1.0/install.sh ]]
await_frame() {
  for _ in {1..45}; do
    if /usr/local/bin/check-guest "$1"; then return; fi
    sleep 1
  done
  return 1
}
case "$1" in
 probation)
   # Run after prove-guest.sh update. Recreate the persisted instant where
   # signed B has been selected and A is the previous confirmed release.
   systemctl stop display-manager
   trap 'systemctl start display-manager' EXIT
   systemctl stop "$unit"
   if [[ ! -d $root/versions/0.1.0 ]]; then
     mkdir "$root/versions/0.1.0"
     tar xzf "/srv/sunna-test-feed/0.1.0/sunna-host-linux-$(uname -m).tar.gz" --no-same-owner -C "$root/versions/0.1.0"
   fi
   python3 -I - "$root" <<'PY'
import json, sys
from pathlib import Path
p = Path(sys.argv[1])
s = json.loads((p/"state.json").read_text())
assert s["current"] == "0.2.0"
s.update(previous="0.1.0", healthy=False, starts=0, request=None, pending=None)
(p/"state.json").write_text(json.dumps(s))
(p/"known-good").write_text("0.1.0")
PY
   systemctl start "$unit"
   for _ in {1..45}; do grep -q ' empty$' /run/sunna-host/receipt 2>/dev/null && break; sleep .5; done
   grep -q ' empty$' /run/sunna-host/receipt
   [[ $(readlink "$root/current") == versions/0.2.0 ]]
   "$root/current/share/sunna/system-host.sh" update --rollback
   for _ in {1..30}; do [[ $(readlink "$root/current") == versions/0.1.0 ]] && break; sleep .5; done
   [[ $(readlink "$root/current") == versions/0.1.0 ]]
   echo 'Rollback request selected signed A while B was still waiting for a desktop.' ;;
 worker-failure)
   # Bind still works. Block the child credential change needed to launch
   # a session worker in this invocation only, with a known graphical seat.
   await_frame /tmp/pre-migration.y4m
   dir=/run/systemd/system/sunna-host.service.d
   dropin=$dir/fixture-deny-worker.conf
   [[ ! -e $dropin ]]
   temp=$(mktemp -d)
   cp /etc/sunna/host.env "$temp/env"
   mkdir -p "$dir"
   trap 'rm -f "$dropin"; cp "$temp/env" /etc/sunna/host.env; rm -rf "$temp"; systemctl daemon-reload; systemctl enable --now "$unit"' EXIT
   printf '[Service]\nSystemCallFilter=~setuid setresuid setreuid\n' >"$dropin"
   systemctl daemon-reload
   if printf '\n127.0.0.1:48800\n48800\n' | "$root/current/share/sunna/system-host.sh" setup >"$temp/setup" 2>&1; then
     echo 'Setup incorrectly accepted a listener without its worker.' >&2
     exit 1
   fi
   grep -q 'failed to confirm a frame' "$temp/setup"
   [[ ! -s /run/sunna-host/receipt ]]
   echo 'The signed setup failed with a live graphical seat and a denied worker launch.' ;;
 network)
   for bad in '::1:48800' '[fe80::1%eth0]:48800' '127.0.0.1:0'; do
     if "$root/recovery" validate-listen "$bad" 48800; then exit 1; fi
   done
   "$root/recovery" validate-listen '[::1]:48800' 48800
   # This disconnected guest has no Tailscale installation. Supply only the
   # address query and let the real unit retry until an address is available.
   [[ ! -e /usr/local/bin/tailscale ]]
   if command -v tailscale >/dev/null; then exit 1; fi
   temp=$(mktemp -d)
   cp /etc/sunna/host.env "$temp/env"
   trap 'cp "$temp/env" /etc/sunna/host.env; rm -rf "$temp"; rm -f /usr/local/bin/tailscale; systemctl restart "$unit"' EXIT
   printf '#!/bin/sh\ntest -f "%s/up" && echo 127.0.0.1\n' "$temp" >/usr/local/bin/tailscale
   chmod 755 /usr/local/bin/tailscale
   sed -i 's/^SUNNA_LISTEN_CHOICE=.*/SUNNA_LISTEN_CHOICE=tailscale/' /etc/sunna/host.env
   systemctl restart "$unit"
   for _ in {1..15}; do
     [[ $(systemctl show "$unit" -p NRestarts --value) -gt 0 ]] && break
     sleep 1
   done
   [[ $(systemctl show "$unit" -p NRestarts --value) -gt 0 && ! -s /run/sunna-host/listening ]]
   touch "$temp/up"
   await_frame /tmp/late-network.y4m
   [[ $(cat /run/sunna-host/listening) == 127.0.0.1:48800 ]] ;;
 key)
   # Rotate a fixture's short legacy credential via the real root setup.
   # Keep the already installed guest identity for the remaining proofs.
   temp=$(mktemp -d)
   cp /etc/sunna/host.{key,env} "$temp/"
   trap 'cp "$temp/host.key" /etc/sunna/host.key; cp "$temp/host.env" /etc/sunna/host.env; rm -rf "$temp"; systemctl restart "$unit"' EXIT
   printf x >/etc/sunna/host.key
   printf 'x\n127.0.0.1:48800\n48800\n' | "$root/current/share/sunna/system-host.sh" setup >"$temp/setup" 2>&1
   grep -q 'Re-share' "$temp/setup"
   grep -Eq '^[[:xdigit:]]{64}$' /etc/sunna/host.key
   await_frame /tmp/rotated.y4m ;;
 recovery)
   systemctl stop "$unit"
   ln -sfn versions/absent "$root/current"
   systemctl start "$unit"
   for _ in {1..40}; do [[ -x $root/current/bin/sunnad ]] && break; sleep .5; done
   [[ -x $root/current/bin/sunnad ]]
   await_frame /tmp/recovered.y4m
   systemctl stop "$unit"
   helper=$(realpath "$root/current/bin/sunna-update")
   stash=$(mktemp -d)
   mv "$helper" "$stash/helper"
   trap 'mv "$stash/helper" "$helper"; rmdir "$stash"' EXIT
   systemctl start "$unit"
   await_frame /tmp/without-versioned-supervisor.y4m ;;
 lifecycle)
   worker() { pgrep -u "$(loginctl show-session "$(loginctl show-seat seat0 -p ActiveSession --value)" -p User --value)" -x sunnad; }
   await_frame /tmp/before-logind.y4m
   old=$(worker)
   systemctl restart systemd-logind
   for _ in {1..40}; do new=$(worker || true); [[ -n $new && $new != "$old" ]] && break; sleep .5; done
   [[ -n $new && $new != "$old" ]]
   await_frame /tmp/logind.y4m ;;
 receipt)
   await_frame /tmp/before-receipt.y4m
   for path in listening receipt shares-greeter; do
     [[ $(stat -c %a "/run/sunna-host/$path") == 644 ]]
     runuser -u viewer2 -- test -r "/run/sunna-host/$path"
   done
   read -r invocation pid state </run/sunna-host/receipt
   [[ $invocation == "$(systemctl show "$unit" -p InvocationID --value)" && $state == frame ]]
   [[ /proc/$pid/exe -ef $root/current/bin/sunnad ]] ;;
esac
