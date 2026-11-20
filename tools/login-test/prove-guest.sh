#!/bin/bash
# Run as root via guest.py in boot-vm.sh's disposable, disconnected guest.
set -euo pipefail
export HOME=/root
root=/opt/sunna/host
feed=/srv/sunna-test-feed
[[ -e /dev/virtio-ports/org.qemu.guest_agent.0 && -f $feed/0.1.0/install.sh ]]
[[ $(ls /sys/class/net) == lo ]]
capture() { /usr/local/bin/check-guest "/tmp/$1.y4m"; }
state() {
  python3 -I - "$root/state.json" "$1" "${2:-}" <<'PY'
import json,sys
s=json.load(open(sys.argv[1]))
sys.exit(0 if s["current"]==sys.argv[2] and s["healthy"] and (not sys.argv[3] or sys.argv[3] in s["failed"]) else 1)
PY
}
wait_state() {
  for _ in {1..180}; do if state "$@"; then return; fi; sleep 1; done
  echo "Release health timeout" >&2; return 1
}
await_screen() {
  for _ in {1..40}; do if capture "$1"; then return; fi; sleep 2; done
  return 1
}
case "${1:-}" in
  update)
    wait_state 0.1.0
    ln -sfn 0.2.0 "$feed/selected"
    "$root/current/share/sunna/system-host.sh" update --yes
    wait_state 0.2.0
    echo "Signed B healthy"
    capture updated
    ln -sfn broken "$feed/selected"
    "$root/current/share/sunna/system-host.sh" update --yes || true
    wait_state 0.2.0 0.3.0
    echo "Signed broken candidate recorded and B healthy again"
    capture rolled-back ;;
  recover)
    for process in worker supervisor display; do
      case "$process" in
        worker) target=$(pgrep -u "$(loginctl show-session "$(loginctl show-seat seat0 -p ActiveSession --value)" -p User --value)" -x sunnad) ;;
        supervisor) target=$(systemctl show sunna-host -p MainPID --value) ;;
        display) target=$(pgrep -x Xorg) ;;
      esac
      [[ $target =~ ^[0-9]+$ && $target -gt 1 ]]
      kill -KILL "$target"
      echo "Killed $process pid $target"
      /usr/local/bin/check-guest check
      sleep 3
      await_screen "after-$process"
    done ;;
  desktop)
    mkdir -p /root/fixture-debs
    tar xzf /root/restore.tar.gz -C /root/fixture-debs
    manager=$(basename "$(readlink -f /etc/systemd/system/display-manager.service)" .service)
    case "$manager" in
        lightdm) names=(lightdm lightdm-gtk-greeter light-locker) ;;
        sddm) names=(sddm sddm-theme-debian-maui) ;;
        gdm3|gdm) names=(gdm3 ubuntu-session) ;;
        *) exit 2 ;;
    esac
    packages=()
    restore=()
    for name in "${names[@]}"; do
        if [[ $(dpkg-query -W -f='${db:Status-Abbrev}' "$name" 2>/dev/null || true) != 'ii ' ]]; then continue; fi
        version=$(dpkg-query -W -f='${Version}' "$name")
        found=false
        for deb in /root/fixture-debs/"${name}"_*.deb; do
            if [[ -f $deb && $(dpkg-deb -f "$deb" Version) == "$version" ]]; then
                packages+=("$name")
                restore+=("$deb")
                found=true
                break
            fi
        done
        [[ $found == true ]] || { echo "Missing restore deb for $name $version" >&2; exit 1; }
    done
    (( ${#packages[@]} > 0 ))
    printf 'Removing and restoring %s\n' "${packages[@]}"
    apt-get -s --no-auto-remove remove "${packages[@]}" | awk '/^Remv/{print $2}' | sort > /tmp/removals
    diff -u <(printf '%s\n' "${packages[@]}" | sort) /tmp/removals
    old=$(systemctl show sunna-host -p MainPID --value)
    listener=$(pgrep -u root -x sunnad)
    systemctl stop "$manager"
    DEBIAN_FRONTEND=noninteractive apt-get -y --no-auto-remove remove "${packages[@]}" >/dev/null
    for _ in {1..6}; do
      result=$(/usr/local/bin/check-guest check)
      echo "$result"
      [[ $result == "not-sharing Its desktop isn't running." ]] && break
      sleep 1
    done
    [[ $result == "not-sharing Its desktop isn't running." ]]
    DEBIAN_FRONTEND=noninteractive dpkg --force-confold -i "${restore[@]}" >/dev/null
    if ! systemctl is-active --quiet "$manager"; then systemctl start "$manager"; fi
    await_screen restored
    [[ $old == "$(systemctl show sunna-host -p MainPID --value)" ]]
    [[ $listener == "$(pgrep -u root -x sunnad)" ]]
    echo "Supervisor PID $old and listener PID $listener survived desktop removal and restoration" ;;
  security)
    u=$(loginctl show-session "$(loginctl show-seat seat0 -p ActiveSession --value)" -p User --value)
    worker=$(pgrep -u "$u" -x sunnad)
    listener=$(pgrep -u root -x sunnad)
    [[ $worker =~ ^[0-9]+$ && $listener =~ ^[0-9]+$ ]]
    runuser -u viewer2 -- /bin/bash -ec '
      for f in /etc/sunna/host.{env,key} /proc/"$1"/environ /proc/"$2"/environ; do
        if head -c 1 "$f" >/dev/null 2>&1; then exit 1; fi
      done
      for f in /opt/sunna/host/current/{bin/sunnad,bin/sunna-update,share/sunna/system-host.sh} /run/sunna-host; do
        test ! -w "$f"
      done
    ' sh "$listener" "$worker"
    awk '/^(Uid|Gid|Groups|CapEff|NoNewPrivs):/' /proc/"$worker"/status
    python3 -I - "$worker" <<'PY'
import sys
keys={v.partition(b'=')[0] for v in open('/proc/'+sys.argv[1]+'/environ','rb').read().split(b'\0') if v}
allowed={b'PATH',b'HOME',b'USER',b'DISPLAY',b'XDG_RUNTIME_DIR',b'DBUS_SESSION_BUS_ADDRESS',b'XDG_SESSION_TYPE',b'SUNNA_WORKER_SESSION',b'XDG_VTNR'}
assert keys <= allowed, "worker inherited unexpected variables"
print("Worker environment matches allowlist")
PY
    echo 'Protected key, settings, process environments, executable paths and runtime directory passed' ;;
  *) exit 2 ;;
esac
