#!/bin/bash
# Run after logging viewer1 in through the fixture viewer.
set -euo pipefail
[[ -e /dev/virtio-ports/org.qemu.guest_agent.0 && $(ls /sys/class/net) == lo ]]
selected() { loginctl show-seat seat0 -p ActiveSession --value; }
await_frame() {
    local class=$1 flag=$2 file=$3 active
    for _ in {1..60}; do
        active=$(selected)
        if [[ -n $active && $(loginctl show-session "$active" -p Class --value 2>/dev/null) == "$class" &&
            $(cat /run/sunna-host/shares-greeter 2>/dev/null) == "$flag" ]] &&
            /usr/local/bin/check-guest "/tmp/$file.y4m"; then return 0; fi
        sleep 1
    done
    return 1
}
owner=$(selected)
[[ $(loginctl show-session "$owner" -p Name --value) == viewer1 ]]
await_frame user 0 desktop
busctl --expect-reply=no call org.freedesktop.DisplayManager /org/freedesktop/DisplayManager/Seat0 org.freedesktop.DisplayManager.Seat SwitchToGreeter
await_frame greeter 1 switched
loginctl activate "$owner"
await_frame user 0 reactivated
[[ $(selected) == "$owner" ]]
if [[ $(basename "$(readlink -f /etc/systemd/system/display-manager.service)") == sddm.service ]]; then
    # Terminating the logind scope skips KDE logout. SDDM can then leave two
    # online greeters with neither VT active.
    uid=$(id -u viewer1)
    runuser -u viewer1 -- env XDG_RUNTIME_DIR="/run/user/$uid" \
        DBUS_SESSION_BUS_ADDRESS="unix:path=/run/user/$uid/bus" \
        busctl --user --expect-reply=no call org.kde.ksmserver /KSMServer \
        org.kde.KSMServerInterface logout iii 0 0 0
else
    loginctl terminate-session "$owner"
fi
await_frame greeter 1 logout
echo 'Decoded the user, switch greeter, reactivated user and logout greeter.'
