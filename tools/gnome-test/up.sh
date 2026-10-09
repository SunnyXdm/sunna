#!/bin/bash
# A fresh GNOME (Wayland) test session in a container, on a machine with Docker:
#   tools/gnome-test/up.sh      build the image if needed, start GNOME Shell
#   tools/gnome-test/down.sh    remove the container
# Inside: GNOME Shell headless (1920x1080, unsafe mode for the test helpers),
# PipeWire, the GNOME portal, and a mock logind (as GNOME's own tests use).
# This directory is /test in the container and the repository is /sunna.
# Helpers (run on the host): shot.sh NAME (a screenshot to /tmp/gnome-NAME.png),
# and `docker exec sunna-gnome bash /test/click.sh X Y` or `/test/key.sh Return`.
set -e
HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
# UBUNTU=24.04, 25.10 or 26.04 runs Ubuntu's own GNOME instead of Arch's newest.
IMAGE=sunna-gnome-test${UBUNTU:+-ubuntu-$UBUNTU}
if ! docker image inspect "$IMAGE" >/dev/null 2>&1; then
  if [ -n "${UBUNTU:-}" ]; then
    docker build -t "$IMAGE" -f "$HERE/ubuntu.Dockerfile" --build-arg RELEASE="$UBUNTU" "$HERE"
  else
    docker build -t "$IMAGE" "$HERE"
  fi
fi
docker rm -f sunna-gnome >/dev/null 2>&1 || true
docker run -d --name sunna-gnome --shm-size=1g -v "$HERE":/test:ro -v "$REPO":/sunna:ro "$IMAGE" sleep infinity >/dev/null
docker exec -u root sunna-gnome bash /test/system.sh >/dev/null
# The portal names apps by their .desktop file.
docker exec sunna-gnome bash -c 'mkdir -p ~/.local/share/applications && printf "[Desktop Entry]\nType=Application\nName=Sunna host\nExec=/usr/bin/true\nNoDisplay=true\n" > ~/.local/share/applications/dev.sunna.Host.desktop'
docker exec sunna-gnome bash /test/session.sh >/dev/null
sleep 5
docker exec sunna-gnome pgrep -x gnome-shell >/dev/null && echo "GNOME is up in the sunna-gnome container" || { echo "GNOME didn't start:"; docker exec sunna-gnome tail -20 /tmp/gnome-shell.log; exit 1; }
