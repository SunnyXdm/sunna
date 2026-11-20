#!/bin/bash
set -euxo pipefail
export DEBIAN_FRONTEND=noninteractive
cd /
if command -v apt-get >/dev/null; then
  fmt=deb; source=/etc/apt/sources.list.d/sunna.sources; key=/etc/apt/keyrings/sunna.asc
  host=/web/apt/pool/main/s/sunna/sunna-host_${version}_amd64.deb
  app=${host/sunna-host_/sunna_}
  install_one() { apt-get install -y --no-install-recommends "$1"; }
  erase() { apt-get remove -y "$@"; }
  # For a production package, point the isolated client's generated source at
  # the loopback mirror. Preserve its Signed-By and all verification settings.
  localize() { sed -i "s|^URIs: .*|URIs: $proofurl/apt|" "$source"; }
else
  fmt=rpm; source=/etc/yum.repos.d/sunna.repo; key=/etc/pki/rpm-gpg/sunna.asc
  host=/web/rpm/sunna-host-$version-1.x86_64.rpm
  app=${host/sunna-host-/sunna-}
  install_one() { dnf install -y --setopt=install_weak_deps=0 "$1"; }
  erase() { dnf remove -y --no-autoremove "$@"; }
  localize() { sed -i "s|^baseurl=.*|baseurl=$proofurl/rpm|" "$source"; }
fi
if [[ $1 == install ]]; then
  if [[ $fmt == deb ]]; then
    apt-get update
    apt install -y "$host" "$app"
    python3 -m http.server "${proofurl##*:}" --bind 127.0.0.1 -d /web >/server.log 2>&1 &
    apt-get install -y --no-install-recommends xvfb xauth dbus-x11
  else
    rpmkeys --import /public.asc
    rpm -Kv "$host" "$app" | tee /verify
    [[ $(grep -ic 'signature.*: OK' /verify) -ge 2 ]]
    dnf install -y "$host" "$app" --setopt=localpkg_gpgcheck=1
    python3 -m http.server "${proofurl##*:}" --bind 127.0.0.1 -d /web >/server.log 2>&1 &
    localize
    dnf install -y xorg-x11-server-Xvfb xorg-x11-xauth dbus-x11 shadow-utils
  fi
  for pair in app:sunna-real:sunna host:sunnad:sunnad; do
    IFS=: read -r component src dst <<< "$pair"
    tar -xOf "/input/sunna-$component-linux-x86_64.tar.gz" "./bin/$src" | cmp - "/usr/bin/$dst"
  done
  for pkg in sunna sunna-host; do
    component=app; [[ $pkg != sunna-host ]] || component=host
    for lib in /usr/lib/"$pkg"/lib/*; do
      tar -xOf "/input/sunna-$component-linux-x86_64.tar.gz" "./lib/${lib##*/}" | cmp - "$lib"
    done
    test ! -e "/usr/lib/$pkg/bin/sunna-update"
  done
  test -f /usr/bin/sunna-host && test ! -L /usr/bin/sunna-host
  test "$(stat -c %u /usr/bin/sunna-host)" = 0
  sunnad --version | grep -F "$version"
  sunna --version | grep -F "$version"
  useradd -m fixture
  user() { runuser -u fixture -- "$@"; }
  user sunna-host status >/status 2>&1 && exit 1
  grep -F 'not set up yet' /status
  user sunna-host status --json | python3 -c 'import json,sys; assert json.load(sys.stdin)["state"]=="off"'
  for exe in sunna-host sunnad; do
    user "$exe" update >/update 2>&1 && exit 1
    grep -F "This copy was installed as a system package. Update it with your system's software updates (apt or dnf)." /update
  done
  user sunnad --source synthetic --codec h264 --no-audio --no-clipboard --listen 127.0.0.1:0 >/host.log 2>&1 &
  host_pid=$!
  echo "$host_pid" >/host.pid
  sleep 12
  kill -0 "$host_pid"
  grep -F 'sunnad listening' /host.log
  user Xvfb :97 -nolisten tcp -screen 0 1024x768x24 >/xvfb.log 2>&1 &
  for _ in {1..50}; do [[ ! -S /tmp/.X11-unix/X97 ]] || break; sleep 0.1; done
  rc=0
  user env DISPLAY=:97 WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS=1 dbus-run-session -- timeout 12 sunna >/app.log 2>&1 || rc=$?
  [[ $rc == 124 ]] || { cat /app.log; exit 1; }
  cmp /public.asc "$key"
  localize
  if [[ $fmt == deb ]]; then
    apt update 2>&1 | tee /apt-update
    grep -F "$proofurl/apt stable InRelease" /apt-update
    ! grep -E '^(W:|E:)' /apt-update || exit 1
  else
    grep -Fx gpgcheck=1 "$source"
    grep -Fx repo_gpgcheck=1 "$source"
    dnf -y makecache --refresh --repo=sunna
  fi
  exit
fi
if [[ $fmt == deb ]]; then
  apt update
  apt upgrade -y
  for p in sunna sunna-host; do test "$(dpkg-query -W -f='${Version}' "$p")" = "$higher"; done
  dpkg -L sunna sunna-host > /inventory
else
  dnf -y --refresh upgrade sunna sunna-host
  for p in sunna sunna-host; do test "$(rpm -q --qf '%{VERSION}' "$p")" = "$higher"; done
  rpm -ql sunna sunna-host > /inventory
fi
kill -0 "$(cat /host.pid)"
runuser -u fixture -- python3 - <<'LISTENER'
import re
from pathlib import Path
log = Path("/host.log").read_text()
port = re.search(r"sunnad listening.*?127\.0\.0\.1:(\d+)", log)[1]
# Sunna uses QUIC over UDP. Match the bound socket to the running daemon.
endpoint = f"0100007F:{int(port):04X}"
inode = next(row.split()[9] for row in Path("/proc/net/udp").read_text().splitlines()[1:]
             if row.split()[1] == endpoint)
assert any(proc.joinpath("comm").read_text().strip() == "sunnad" and
           any(str(fd.readlink()) == f"socket:[{inode}]" for fd in proc.joinpath("fd").iterdir())
           for proc in Path("/proc").glob("[0-9]*"))
print("Host retains its original QUIC listener after upgrade")
LISTENER
# Administrator opt-out survives a reinstall. The active key is repaired.
rm "$source"
echo damaged > "$key"
if [[ $fmt == deb ]]; then
  apt-get install -y --reinstall "/web/apt/pool/main/s/sunna/sunna-host_${higher}_amd64.deb"
else
  dnf reinstall -y "/web/rpm/sunna-host-$higher-1.x86_64.rpm"
fi
test ! -e "$source"
cmp /public.asc "$key"
cat /inventory
erase sunna sunna-host
for first in "$app" "$host"; do
  second=$app; name=sunna-host; other=sunna
  if [[ $first == "$app" ]]; then second=$host; name=sunna; other=sunna-host; fi
  install_one "$first"
  localize
  install_one "$second"
  erase "$name"
  cmp /public.asc "$key"; test -s "$source"
  erase "$other"
  test ! -e "$source"; test ! -e "$key"
done
if [[ $fmt == deb ]]; then
  dpkg --purge sunna sunna-host
  test ! -e /var/lib/sunna-packages
fi
while IFS= read -r path; do
  [[ -d "$path" && $path != *sunna* ]] && continue
  [[ ! -e "$path" && ! -L "$path" ]]
done < /inventory
echo "PASS $(cat /etc/os-release | head -1) upgrade and removal"
