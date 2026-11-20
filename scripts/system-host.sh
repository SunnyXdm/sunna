#!/bin/bash
# Administration of the signed Linux system install. Invoked via a fixed path.
set -euo pipefail
export PATH=/usr/sbin:/usr/bin:/sbin:/bin
root=/opt/sunna/host
unit=sunna-host.service
settings=/etc/sunna
export HOME=/root SUNNA_UPDATE_ROOT=/opt/sunna
umask 077
case "${1:-}" in
  setup)
    [[ $(id -u) == 0 ]] || exit 1
    # stdin comes from the caller running as the previous owner. Read values
    # as data only. Never open the previous owner's configuration as root.
    IFS= read -r key
    IFS= read -r address
    IFS= read -r port || port=48800
    "$root/recovery" validate-listen "$address" "$port"
    exec 9>/run/sunna-host-setup.lock
    flock 9
    [[ ! -L "$settings" ]]
    if [[ -d "$settings" ]]; then
      [[ $(stat -c %u "$settings") == 0 && $((8#$(stat -c %a "$settings") & 022)) == 0 ]]
    fi
    for file in host.key host.env host.identity; do
      path="$settings/$file"
      [[ ! -L "$path" ]]
      [[ ! -e "$path" || ( -f "$path" && $(stat -c %u:%a "$path") == 0:600 ) ]]
    done
    [[ ! -L /etc/systemd/system/$unit ]]
    if [[ -f /etc/systemd/system/$unit ]]; then
      grep -Fxq 'Description=Sunna whole-machine host' "/etc/systemd/system/$unit" || {
        echo 'There is already a different system unit with this name.' >&2; exit 1;
      }
    fi
    if [[ -f "$settings/host.key" && -n "$key" && $(<"$settings/host.key") != "$key" ]]; then
      echo 'The existing system key differs from the user host key. The user host has been retained.' >&2
      exit 1
    fi
    if [[ -f "$settings/host.key" ]]; then key=$(<"$settings/host.key"); fi
    if [[ ! "$key" =~ ^([[:xdigit:]]{32}|[[:xdigit:]]{64})$ ]]; then
      if [[ -n "$key" ]]; then echo 'Rotating the old key. Re-share the new private link with your viewers.' >&2; fi
      key=$(od -An -N32 -tx1 /dev/urandom | tr -d ' \n')
    fi
    unit_path="/etc/systemd/system/$unit"
    backup=$(mktemp -d /run/sunna-setup.XXXXXXXX)
    trap 'rm -rf -- "$backup"' EXIT
    prior_dir=no
    if [[ -d $settings ]]; then prior_dir=yes; prior_mode=$(stat -c %a "$settings"); fi
    paths=("$settings/host.key" "$settings/host.env" "$settings/host.identity" "$unit_path")
    for path in "${paths[@]}"; do
      if [[ -e $path ]]; then cp -p -- "$path" "$backup/"; fi
    done
    enabled=$(systemctl is-enabled "$unit" 2>/dev/null || :)
    active=no
    if systemctl is-active --quiet "$unit"; then active=yes; fi
    started=no
    enabling=no
    restore() {
      local rc=$? path failed=no
      trap - EXIT
      set +e
      if [[ $rc != 0 ]]; then
        if [[ $started == yes ]]; then systemctl stop "$unit" || failed=yes; fi
        if [[ $enabling == yes && $enabled != enabled ]]; then systemctl disable "$unit" || failed=yes; fi
        for path in "${paths[@]}"; do
          if [[ -e "$backup/${path##*/}" ]]; then
            cp -p -- "$backup/${path##*/}" "$path" || failed=yes
          else
            rm -f -- "$path" || failed=yes
          fi
        done
        systemctl daemon-reload || failed=yes
        if [[ $enabling == yes && $enabled == enabled-runtime ]]; then systemctl enable --runtime "$unit" || failed=yes; fi
        if [[ $started == yes && $active == yes ]]; then systemctl start "$unit" || failed=yes; fi
      fi
      if [[ -n ${staging:-} ]]; then rm -rf -- "$staging"; fi
      if [[ $rc != 0 ]]; then
        if [[ $prior_dir == no ]]; then rmdir "$settings" || failed=yes; else chmod "$prior_mode" "$settings" || failed=yes; fi
      fi
      if [[ $failed == yes ]]; then
        echo "Could not fully restore system sharing. The previous files are in $backup." >&2
      else
        rm -rf -- "$backup"
      fi
      exit "$rc"
    }
    trap restore EXIT
    trap 'exit 1' HUP INT TERM
    install -d -m700 "$settings"
    staging=$(mktemp -d "$settings/.setup.XXXXXXXX")
    IFS= read -r identity || identity=
    if [[ -n $identity ]]; then
      [[ ${#identity} -le 22000 ]]
      imported="$staging/imported"
      touch "$imported"
      printf '%s' "$identity" | base64 -d > "$imported"
      "$root/current/bin/sunnad" validate-identity "$imported"
      if [[ -e "$settings/host.identity" ]]; then
        cmp -s "$settings/host.identity" "$imported" || { echo 'The system host has a different identity.' >&2; exit 1; }
      else
        mv -T "$imported" "$settings/host.identity"
      fi
    fi
    if [[ ! -f "$settings/host.key" || $(<"$settings/host.key") != "$key" ]]; then
      private="$staging/new-key"
      printf '%s' "$key" > "$private"
      mv -T "$private" "$settings/host.key"
    fi
    unset key
    config="$staging/new-env"
    printf 'SUNNA_LISTEN_CHOICE=%s\nSUNNA_PORT=%s\n' "$address" "$port" > "$config"
    mv -T "$config" "$settings/host.env"
    install -m644 /dev/stdin /etc/systemd/system/sunna-host.service <<'UNIT'
[Unit]
Description=Sunna whole-machine host
After=network-online.target systemd-logind.service
Wants=network-online.target systemd-logind.service

[Service]
Environment=HOME=/root SUNNA_UPDATE_ROOT=/opt/sunna
EnvironmentFile=/etc/sunna/host.env
ExecStart=/opt/sunna/host/recovery system-serve host --system-host --name %H
Restart=always
RestartSec=2
NoNewPrivileges=true
CapabilityBoundingSet=CAP_SETUID CAP_SETGID CAP_KILL CAP_DAC_OVERRIDE
RuntimeDirectory=sunna-host
RuntimeDirectoryMode=0755
UMask=0077
# The worker must see logind mount fresh /run/user directories after logout.
RestrictSUIDSGID=true

[Install]
WantedBy=multi-user.target
UNIT
    systemctl daemon-reload
    started=yes
    systemctl restart "$unit"
    ready=no
    for _ in {1..90}; do
      if { exec 8</run/sunna-host/receipt; } 2>/dev/null && read -r invocation pid state <&8 &&
         [[ $invocation == "$(systemctl show "$unit" -p InvocationID --value)" && $pid =~ ^[0-9]+$ ]] &&
         [[ $(awk '/^PPid:/ {print $2}' "/proc/$pid/status" 2>/dev/null) == "$(systemctl show "$unit" -p MainPID --value)" ]] &&
         [[ -s /run/sunna-host/listening && ( $state == frame || $state == empty ) && /proc/$$/fd/8 -ef /run/sunna-host/receipt ]]; then
        ready=yes
        if [[ $state == empty ]]; then echo 'Nothing to share yet on the active seat.'; fi
        break
      fi
      sleep 1
    done
    if [[ $ready != yes ]]; then
      echo 'System host failed to confirm a frame or an empty seat.' >&2
      exit 1
    fi
    enabling=yes
    systemctl enable "$unit"
    if [[ ! -e /usr/local/bin/sunna-host && ! -L /usr/local/bin/sunna-host ]]; then
      ln -s "$root/current/bin/sunna-host" /usr/local/bin/sunna-host
    fi
    ;;
  new-key)
    private=$(mktemp "$settings/.key.XXXXXXXX")
    od -An -N32 -tx1 /dev/urandom | tr -d ' \n' > "$private"
    mv -T "$private" "$settings/host.key"
    systemctl restart "$unit"
    echo "New key made. Share the new link with your viewers." ;;
  new-identity)
    "$root/current/bin/sunnad" new-identity --system-host
    systemctl try-restart "$unit" || { echo "Identity rotated, but the running host could not reload it." >&2; exit 1; } ;;
  link)
    [[ $(id -u) == 0 ]]
    # A link is a secret, requested explicitly by the local administrator.
    python3 -I - "$("$root/current/bin/sunnad" identity --system-host)" <<'PY'
import sys
from pathlib import Path
from urllib.parse import quote
address = Path("/run/sunna-host/listening").read_text().strip()
print("sunna://" + address + "?key=" + quote(Path("/etc/sunna/host.key").read_text(), safe="") + "&id=" + sys.argv[1])
PY
    ;;
  update)
    shift
    while IFS='=' read -r setting value; do
      case "$setting" in SUNNA_UPDATE_URL|SUNNA_UPDATE) export "$setting=$value" ;; esac
    done < "$settings/host.env"
    exec "$root/current/bin/sunna-update" update host "$@" ;;

  uninstall)
    systemctl disable --now "$unit"
    rm -f "/etc/systemd/system/$unit"
    systemctl daemon-reload
    if [[ -L /usr/local/bin/sunna-host && $(stat -c %u /usr/local/bin/sunna-host) == 0 && $(readlink /usr/local/bin/sunna-host || true) == "$root/current/bin/sunna-host" ]]; then rm /usr/local/bin/sunna-host; fi
    rm -rf -- "$root"
    if [[ ${2:-} == --purge ]]; then rm -f "$settings/host.env" "$settings/host.key" "$settings/host.identity"; fi
    ;;
  *) exit 2 ;;
esac
