#!/usr/bin/env bash
# Boot an actual seat without mounting any host devices, cgroups or directories.
set -euo pipefail
cd "$(dirname "$0")"
manager=${1:-lightdm}
release=${2:-24.04}
case "$manager" in lightdm|sddm|gdm-x11|gdm-wayland) ;; *) exit 2 ;; esac
[[ "$release" =~ ^[0-9]{2}\.[0-9]{2}$ ]]
prefix=${SUNNA_LOGIN_PREFIX:-sunna-login}
[[ "$prefix" =~ ^sunna-[a-z0-9-]+$ ]]
runner=$prefix-runner
guest=${SUNNA_LOGIN_IMAGE:-$prefix-guest-$manager-$release}
[[ $guest =~ ^sunna-[a-zA-Z0-9.-]+$ ]]
name=$prefix-$manager-$release
disk=$name-disk
if [[ -z ${SUNNA_LOGIN_IMAGE:-} ]]; then
  docker build -t "$guest" --build-arg RELEASE="$release" --build-arg MANAGER="$manager" -f vm.Dockerfile .
fi
docker build -t "$runner" -f runner.Dockerfile .
if docker volume inspect "$disk" >/dev/null 2>&1; then echo "$disk already exists" >&2; exit 1; fi
docker volume create "$disk" >/dev/null
docker create --name "$name-export" "$guest" true >/dev/null
trap 'docker rm "$name-export" >/dev/null' EXIT
if [[ -n ${SUNNA_LOGIN_FEED:-} ]]; then
  if [[ $manager == gdm-wayland ]]; then
    docker cp mutter-greeter.sh "$name-export:/usr/local/bin/mutter-greeter"
    docker cp wayland-proof.sh "$name-export:/usr/local/bin/wayland-proof"
    docker cp fixture-wayland.service "$name-export:/etc/systemd/system/fixture-wayland.service"
  fi
  docker cp install-guest.sh "$name-export:/usr/local/bin/install-guest"
  docker cp check-guest.sh "$name-export:/usr/local/bin/check-guest"
  docker cp "$SUNNA_LOGIN_FEED" "$name-export:/srv/sunna-test-feed"
  docker cp "${SUNNA_LOGIN_CLIENT:?supply a stripped login-test client}" "$name-export:/usr/local/bin/login-test"
fi
docker export "$name-export" | docker run --rm -i --name "$name-pack" --network none -v "$disk:/disk" "$runner" sh -ec '
    mkdir /tmp/root
    tar -xf - -C /tmp/root
    cd /tmp/root
    # A kernel boot must look like a VM to systemd and GDM.
    rm -f .dockerenv run/systemd/container
    if [ -d srv/sunna-test-feed ]; then
        if [ -e etc/systemd/system/fixture-wayland.service ]; then
            chown 0:0 usr/local/bin/mutter-greeter usr/local/bin/wayland-proof etc/systemd/system/fixture-wayland.service
            ln -s ../fixture-wayland.service etc/systemd/system/multi-user.target.wants/
        fi
        chown -R 0:0 srv/sunna-test-feed usr/local/bin/install-guest usr/local/bin/check-guest usr/local/bin/login-test
        chmod -R go-w srv/sunna-test-feed
    fi
    cp -L boot/vmlinuz-* /disk/vmlinuz
    cp -L boot/initrd.img-* /disk/initrd
    truncate -s 12G /disk/root.img
    mkfs.ext4 -q -F -d . /disk/root.img
'
docker run -d --name "$name" --network none -v "$disk:/disk" "$runner" \
  qemu-system-x86_64 -machine q35 -accel tcg,thread=multi -cpu max -smp 2 -m 3072 \
  -drive file=/disk/root.img,format=raw,if=virtio -kernel /disk/vmlinuz -initrd /disk/initrd \
  -append 'root=/dev/vda rw console=ttyS0' -vga virtio -display none -serial stdio -nic none \
  -device virtio-serial -chardev socket,path=/tmp/guest.sock,server=on,wait=off,id=agent \
  -device virtserialport,chardev=agent,name=org.qemu.guest_agent.0
docker cp guest.py "$name:/usr/local/bin/guest"
echo "Inspect the seat with: docker exec $name guest /usr/bin/loginctl seat-status seat0"
