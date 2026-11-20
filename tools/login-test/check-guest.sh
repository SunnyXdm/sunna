#!/bin/bash
set -euo pipefail
export HOME=/root
[[ $(id -u) == 0 && -f /srv/sunna-test-feed/0.1.0/install.sh ]]
SUNNA_TOKEN=$(< /etc/sunna/host.key)
export SUNNA_TOKEN
exec /usr/local/bin/login-test 127.0.0.1:48800 "$@"
