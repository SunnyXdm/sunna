#!/bin/bash
# Copy an already-decoded viewer picture from the isolated guest.
set -euo pipefail
vm=${1:?fixture VM}
frame=${2:?guest Y4M basename}
output=${3:?output PNG}
prefix=${SUNNA_LOGIN_PREFIX:-sunna-login}
[[ $prefix =~ ^sunna-[a-z0-9-]+$ && $vm == "$prefix-"* ]]
[[ ${vm#"$prefix-"} =~ ^(lightdm|sddm|gdm-(x11|wayland))-[0-9]{2}\.[0-9]{2}$ ]]
[[ $frame =~ ^[a-zA-Z0-9_-]+$ ]]
work=$(mktemp /tmp/sunna-login-frame.XXXXXXXX)
trap 'rm -f "$work"' EXIT
docker exec "$vm" guest get "/tmp/$frame.y4m" "/tmp/$frame.y4m"
docker exec "$vm" cat "/tmp/$frame.y4m" > "$work"
ffmpeg -nostdin -v error -y -i "$work" -frames:v 1 "$output"
