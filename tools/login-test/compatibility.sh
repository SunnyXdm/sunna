#!/bin/bash
set -euo pipefail
export HOME=/root
[[ -e /dev/virtio-ports/org.qemu.guest_agent.0 && $(ls /sys/class/net) == lo ]]
export SUNNA_TOKEN="$(</etc/sunna/host.key)"
addr=127.0.0.1:48800
if /root/old-client probe "$addr"; then exit 1; fi
if /root/old-client hold "$addr" 1; then exit 1; fi
dropin=/run/systemd/system/sunna-host.service.d/sunna-id-test.conf
[[ ! -e $dropin ]]
mkdir -p "${dropin%/*}"
printf '[Service]\nEnvironment=SUNNA_REQUIRE_PROOF=0\n' >"$dropin"
old=
watcher=
first=
cleanup() {
  [[ -z $old ]] || kill "$old" 2>/dev/null || true
  [[ -z $watcher ]] || kill "$watcher" 2>/dev/null || true
  [[ -z $first ]] || rm -rf "$first"
  rm -f "$dropin"
  systemctl daemon-reload
  systemctl restart sunna-host
  systemctl start display-manager
}
trap cleanup EXIT
systemctl daemon-reload
systemctl restart sunna-host
for _ in {1..40}; do
  if /root/old-client probe "$addr"; then break; fi
  sleep .25
done
/root/old-client hold "$addr" 1
/root/old-daemon --source synthetic --codec h264 --listen 127.0.0.1:48801 --no-audio --no-clipboard >/tmp/old-sunna.log 2>&1 &
old=$!
for _ in {1..40}; do
  if /usr/local/bin/login-test 127.0.0.1:48801 check | grep '^update-needed'; then break; fi
  sleep .25
done
/usr/local/bin/login-test 127.0.0.1:48801 check | grep '^update-needed'
SUNNA_TEST_ALLOW_OLDER=1 /usr/local/bin/login-test 127.0.0.1:48801 check | grep '^ready not fully verified'
SUNNA_TEST_ALLOW_OLDER=1 /usr/local/bin/login-test 127.0.0.1:48801 /tmp/from-old.y4m
first=$(mktemp -d)
timeout 90 /root/old-client watch "$addr" "$first/frame" &
watcher=$!
for _ in {1..80}; do [[ -e $first/frame ]] && break; sleep .5; done
[[ -e $first/frame ]]
systemctl stop display-manager
wait "$watcher"
for _ in {1..30}; do
  answer=$(/usr/local/bin/login-test "$addr" check)
  [[ $answer == not-sharing* ]] && break
  sleep .5
done
[[ $answer == not-sharing* ]]
echo "$answer"
/root/old-client probe "$addr"
if /root/old-client hold "$addr" 1; then exit 1; fi
if /usr/local/bin/login-test "$addr" /tmp/unavailable.y4m; then exit 1; fi
