#!/bin/bash
# Exercise the official interfaces on the compositor's own bus, as its UID.
set -euo pipefail
if [[ ${1:-} != --greeter ]]; then
  sid=$(loginctl show-seat seat0 -p ActiveSession --value)
  [[ -n $sid && $(loginctl show-session "$sid" -p Class --value) == greeter ]]
  [[ $(loginctl show-session "$sid" -p Type --value) == wayland ]]
  uid=$(loginctl show-session "$sid" -p User --value)
  loginctl show-session "$sid" -p Class -p Type -p Name -p User -p Seat -p Active
  gnome-shell --version
  user=$(getent passwd "$uid" | cut -d: -f1)
  exec setpriv --reuid "$uid" --regid "$(id -g "$user")" --clear-groups env -i \
    PATH=/usr/bin:/bin XDG_RUNTIME_DIR="/run/user/$uid" /bin/bash -s -- --greeter < "$0"
fi
[[ $(id -u) != 0 ]]
for _ in {1..60}; do
  # GNOME 46 can start its greeter with dbus-run-session on a private bus.
  bus=$(python3 -I - <<'PY'
from pathlib import Path
import os
for entry in Path("/proc").glob("[0-9]*"):
    try:
        if entry.stat().st_uid != os.getuid() or (entry/"comm").read_text().strip() != "gnome-shell":
            continue
        fields = (entry/"environ").read_bytes().split(b"\0")
        for field in fields:
            if field.startswith(b"DBUS_SESSION_BUS_ADDRESS=unix:"):
                print(field.partition(b"=")[2].decode())
                raise SystemExit
    except OSError:
        pass
PY
)
  export DBUS_SESSION_BUS_ADDRESS=${bus:-unix:path=$XDG_RUNTIME_DIR/bus}
  if gdbus call --session --dest org.freedesktop.DBus --object-path /org/freedesktop/DBus \
      --method org.freedesktop.DBus.NameHasOwner org.gnome.Mutter.DisplayConfig 2>/dev/null | grep -q true; then break; fi
  sleep 1
done
for iface in RemoteDesktop ScreenCast; do
  dest=org.gnome.Mutter.$iface
  endpoint=(--session --dest "$dest" --object-path "/org/gnome/Mutter/$iface")
  gdbus introspect "${endpoint[@]}" | sed -n "/interface $dest {/,/^  };/p" || true
  args=()
  if [[ $iface == ScreenCast ]]; then args+=('{}'); fi
  if ! gdbus call "${endpoint[@]}" --method "$dest.CreateSession" "${args[@]}"; then
    echo "$iface session request failed"
  fi
done
