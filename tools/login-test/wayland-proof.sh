#!/bin/bash
# Boot-time proof for a QEMU guest. The serial log contains no credentials.
set -euo pipefail
[[ $(ls /sys/class/net) == lo && -f /srv/sunna-test-feed/0.1.0/install.sh ]]
echo SUNNA-GREETER-PROOF-BEGIN
if [[ ! -f /etc/sunna/host.key ]]; then /usr/local/bin/install-guest; fi
for _ in {1..180}; do
    sid=$(loginctl show-seat seat0 -p ActiveSession --value || true)
    if [[ -n $sid ]] && [[ $(loginctl show-session "$sid" -p Class --value) == greeter ]]; then break; fi
    sleep 1
done
/usr/local/bin/mutter-greeter
for _ in {1..8}; do
    result=$(/usr/local/bin/check-guest check)
    echo "$result"
    [[ $result == "not-sharing Nobody is logged in and the login screen can't be shared on this system." ]] && break
    sleep 1
done
echo SUNNA-GREETER-PROOF-END
