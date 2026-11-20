#!/bin/bash
set -euo pipefail
[[ -e /dev/virtio-ports/org.qemu.guest_agent.0 && $(ls /sys/class/net) == lo ]]
manager=$(basename "$(readlink -f /etc/systemd/system/display-manager.service)" .service)
uid=$(id -u viewer1)
mark() { read -r t _ </proc/uptime; printf '%s %s\n' "$t" "$1"; }
# An independent client in the Xsession startup path records when XSync
# confirms the new session can draw. This hook lives only in the test VM.
install -m644 /root/draw-marker.py /usr/local/share/sunna-test-draw.py
hook=/etc/X11/Xsession.d/01sunna-test-draw
rm -f "/run/user/$uid/sunna-first-draw"
printf '%s\n' 'if [ "$(id -un)" = viewer1 ]; then python3 /usr/local/share/sunna-test-draw.py & fi' >"$hook"
observe() {
  local last='' state='' draw=no
  while :; do
    worker=$(pgrep -u "$uid" -x sunnad || true)
    sess=$(loginctl show-seat seat0 -p ActiveSession --value || true)
    if [[ "$sess" != "$last" ]]; then mark "logind active $sess"; last=$sess; fi
    if [[ "$worker" != "$state" ]]; then mark "worker pid $worker"; state=$worker; fi
    if [[ $draw == no && -s /run/user/$uid/sunna-first-draw ]]; then
      echo "$(cat "/run/user/$uid/sunna-first-draw") XSync in user session"
      draw=yes
    fi
    sleep .1
  done
}
observe >/tmp/sunna-observer.log &
watcher=$!
finish() {
  kill "$watcher" 2>/dev/null || true
  rm -f "$hook"
  cat /tmp/sunna-observer.log
  if ! grep -q XSync /tmp/sunna-observer.log && [[ -s /run/user/$uid/sunna-first-draw ]]; then
    echo "$(cat "/run/user/$uid/sunna-first-draw") XSync in user session"
  fi
}
trap finish EXIT
case $manager in
 lightdm) steps=$'click 0.56 0.45\nwait 200\nkey 7d\nkey 24\nwait 2500' ;;
 sddm) steps=$'click 0.72 0.49\nwait 500' ;;
 *) exit 2 ;;
esac
if [[ ${SUNNA_LOGIN_SELECTED:-} == viewer1 ]]; then steps='wait 500'; fi
if [[ $manager == lightdm && ${SUNNA_LOGIN_SELECTED:-} == viewer2 ]]; then
  steps=$'click 0.56 0.45\nwait 200\nkey 7e\nkey 24\nwait 2500'
fi
mark "client started"
printf '%s\n' "$steps" 'type viewer1' 'key 24' 'handoff /tmp/new-desktop.y4m' |
  SUNNA_MEASURE_LOGIN=1 timeout 160 /usr/local/bin/check-guest /tmp/before-login.y4m --steps
mark "client ended"
