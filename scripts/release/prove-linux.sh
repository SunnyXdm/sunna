#!/usr/bin/env bash
# Runtime portability proof; local downloads only, no host services/displays.
# Usage: prove-linux.sh RELEASE_DIR
set -euo pipefail
release=$(realpath "${1:?local signed release directory}")
repo=$(cd "$(dirname "$0")/../.." && pwd)
work=$(mktemp -d /tmp/sunna-fix-runtime.XXXXXX)
trap 'rm -rf "$work"' EXIT
for distro in ubuntu22 ubuntu24 debian13 fedora arch; do
  case "$distro" in
    ubuntu22) base=ubuntu:22.04; packages='apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y curl ca-certificates libwebkit2gtk-4.1-0 libxdo3 libasound2 libpulse0 xvfb xauth dbus-x11';;
    ubuntu24) base=ubuntu:24.04; packages='apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y curl ca-certificates libwebkit2gtk-4.1-0 libxdo3 libasound2t64 libpulse0 xvfb xauth dbus-x11';;
    debian13) base=debian:13; packages='apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y curl ca-certificates libwebkit2gtk-4.1-0 libxdo3 libasound2t64 libpulse0 xvfb xauth dbus-x11';;
    fedora) base=fedora:latest; packages='dnf install -y curl-minimal ca-certificates webkit2gtk4.1 libxdo alsa-lib pulseaudio-libs xorg-x11-server-Xvfb xorg-x11-xauth dbus-daemon dbus-x11 procps-ng';;
    arch) base=archlinux:latest; packages='pacman -Syu --noconfirm curl ca-certificates webkit2gtk-4.1 xdotool alsa-lib libpulse xorg-server-xvfb xorg-xauth dbus procps-ng';;
  esac
  printf 'FROM %s\nRUN %s\n' "$base" "$packages" > "$work/Dockerfile"
  docker build -t "sunna-fix-runtime-$distro" "$work"
  [ "${SUNNA_PROOF_BUILD_ONLY:-0}" != 1 ] || continue
  docker run --rm --name "sunna-fix-smoke-$distro" --network host --tmpfs /noexec:rw,noexec,nosuid \
    -v "$release:/release:ro" -v "$repo/scripts/release/runtime-smoke.sh:/proof.sh:ro" \
    -e SUNNA_UPDATE_URL=http://127.0.0.1:48991 -e SUNNA_LOG_URL= -e SUNNA_LOG_TOKEN= \
    "sunna-fix-runtime-$distro" bash /proof.sh
done
