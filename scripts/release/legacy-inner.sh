#!/bin/bash
set -euo pipefail
cd /src
cargo build --release -p sunna-client --example update-proof -j2
cargo build --release -p sunnad -j2
cp /target/release/sunnad /proof/old-sunnad
cp /target/release/examples/update-proof /proof/old-client
export HOME=/tmp/sunna-upd-legacy-home
export CARGO_HOME=/root/.cargo RUSTUP_HOME=/root/.rustup
export SUNNA_LOG_URL= SUNNA_LOG_TOKEN=
mkdir -p "$HOME"
# The isolated container has no desktop/user service manager. The original
# installer places the files before that setup step; never print its key/log.
set +e
scripts/install-host-linux.sh --virtual > /tmp/sunna-upd-legacy-install.log 2>&1
install_result=$?
set -e
[ -f "$HOME/.local/bin/sunnad" ] && [ ! -L "$HOME/.local/bin/sunnad" ]
[ -f "$HOME/.local/bin/sunna-host" ]
mkdir -p /proof/legacy
cp "$HOME/.local/bin/sunnad" "$HOME/.local/bin/sunna-host" /proof/legacy/
echo "Original installer placed plain files; desktop setup status=$install_result (no session manager in this container)."
