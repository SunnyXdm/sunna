#!/usr/bin/env bash
# Run after prove-update.py signed A and B at the local proof URL.
set -euo pipefail
proof=$(realpath "${1:?proof directory}")
here=$(cd "$(dirname "$0")" && pwd)
name=sunna-fix-systemd-$$
image=sunna-fix-user-manager-$$
context=$(mktemp -d)
cleanup() {
  docker rm -f "$name" >/dev/null 2>&1 || true
  docker image rm "$image" >/dev/null 2>&1 || true
  rm -rf "$context"
}
trap cleanup EXIT
cat >"$context/Dockerfile" <<'EOF'
FROM sunna-fix-builder
RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y systemd systemd-sysv dbus-user-session xvfb xauth && useradd -m fixture
ENV container=docker
STOPSIGNAL SIGRTMIN+3
CMD ["/sbin/init"]
EOF
docker build -q -t "$image" "$context" >/dev/null
docker run -d --name "$name" --privileged --cgroupns=private --network none \
  --tmpfs /run --tmpfs /run/lock "$image" >/dev/null
for _ in $(seq 30); do
  docker exec "$name" systemctl start user@1000.service 2>/dev/null && break
  sleep 1
done
docker exec "$name" mkdir -p /proof/releases /home/fixture/full
for release in A B; do
  docker cp "$proof/releases/$release" "$name:/proof/releases/$release"
done
# Dereference only the fixture's legacy-directory link, if present.
docker cp "$(realpath "$proof/legacy")" "$name:/proof/legacy"
for script in prove-input.sh prove-migration.sh prove-systemd.py prove-full-disk.py switch_states.py; do docker cp "$here/$script" "$name:/proof/"; done
as_fixture=(docker exec --user fixture -e HOME=/home/fixture -e SUNNA_FIX_CONTAINER=1 -e XDG_RUNTIME_DIR=/run/user/1000 "$name")
"${as_fixture[@]}" bash /proof/prove-migration.sh
"${as_fixture[@]}" bash /proof/prove-input.sh
"${as_fixture[@]}" python3 /proof/prove-systemd.py
docker exec "$name" mount -t tmpfs -o size=4m,nr_inodes=64,uid=1000,gid=1000,mode=0700 tmpfs /home/fixture/full
"${as_fixture[@]}" python3 /proof/prove-full-disk.py /home/fixture/full /proof/releases/A/sunna-update-linux-x86_64
