# Linux login-manager test bed

These fixtures boot a Linux kernel, systemd, logind, a display manager and a desktop in QEMU. The GPU and VT belong to the guest. The guest has no network interface except loopback. Docker stores the guest filesystem in a named volume. Guest commands and file transfers use a QEMU guest-agent socket inside the runner. The Sunna viewer uses the normal QUIC, authentication, input and H.264 decoding paths. A PNG in the evidence directory comes from a decoded viewer frame.

Use Docker, a Linux Rust toolchain and FFmpeg on the development machine. The image build downloads Ubuntu packages. Software CPU emulation makes these tests slow, especially with multiple desktops starting together.

```sh
export PATH="$HOME/.cargo/bin:$PATH"
cargo build -p sunna-client --example login-test
work=$(mktemp -d /tmp/sunna-login-proof.XXXXXXXX)
strip -o "$work/client" target/debug/examples/login-test
mkdir "$work/feed"
tools/login-test/test-releases.sh "$work/feed"
export SUNNA_LOGIN_FEED="$work/feed" SUNNA_LOGIN_CLIENT="$work/client"
tools/login-test/boot-vm.sh lightdm 24.04
vm=sunna-login-lightdm-24.04
docker exec "$vm" guest /usr/local/bin/install-guest
docker exec "$vm" guest /usr/bin/loginctl seat-status seat0
docker exec "$vm" guest /usr/local/bin/check-guest check
docker exec "$vm" guest /usr/local/bin/check-guest /tmp/boot.y4m
tools/login-test/save-frame.sh "$vm" boot "$work/boot.png"
```

The other manager arguments are `sddm`, `gdm-x11`, and `gdm-wayland`. For GDM Wayland use Ubuntu 24.04, 25.10 and 26.04. With a feed present, the Wayland guests run the greeter interface and fallback probe at boot. Read the lines between `SUNNA-GREETER-PROOF-BEGIN` and `SUNNA-GREETER-PROOF-END` in `docker logs "$vm"`. `mutter-greeter.sh` checks both real Mutter session interfaces from the greeter account under the installed compositor policy. `Dockerfile` and `lightdm.conf` describe the earlier Xvfb trial. Only the QEMU definition exercises an actual local VT.

`test-releases.sh` signs three host releases with a newly generated disposable key. The installer verifies the pinned helper, signed manifest and binary archive. The fixture enables the system unit, writes its key in the guest and serves updates on guest loopback. The `viewer1` and `viewer2` accounts have passwords equal to their fixture usernames. Keep these disconnected fixtures isolated.

Drive one viewer connection by piping commands into `check-guest FILE --steps`. Operations are `click X Y` (normalized coordinates), `type TEXT`, `key HEX` (Mac virtual key, for example `24` for Return), `wait MILLISECONDS`, and `save FILE`. The first filename and every subsequent saved frame are in the guest. Choose the greeter entry shown in the first capture before sending fixture credentials. The wrapper passes the key by environment and prints only the result of the probe or the frame size and elapsed time.

To run the update, interruption, security or package-removal checks, copy the runner into the guest. The package-removal check requires a tarball at `/root/restore.tar.gz` containing the installed versions of the packages for that manager. Use `apt-get download` in a matching Ubuntu fixture. For LightDM the packages are `lightdm`, `lightdm-gtk-greeter` and, if installed, `light-locker`. For SDDM use `sddm` and the Debian Maui theme if installed. For GDM use `gdm3` and `ubuntu-session`. The check matches the installed versions to the backup debs and compares apt's removal plan before proceeding.

```sh
docker cp tools/login-test/prove-guest.sh "$vm:/tmp/prove-guest"
docker exec "$vm" guest put /tmp/prove-guest /root/prove-guest
docker exec "$vm" guest /bin/bash /root/prove-guest security
docker exec "$vm" guest /bin/bash /root/prove-guest update
docker exec "$vm" guest /bin/bash /root/prove-guest recover
docker exec "$vm" guest /bin/bash /root/prove-guest desktop
docker cp tools/login-test/boot-proof.sh "$vm:/tmp/boot-proof"
docker exec "$vm" guest put /tmp/boot-proof /root/boot-proof
docker exec "$vm" guest /bin/bash /root/boot-proof
# After the guest reboots and the viewer writes its frame:
docker exec "$vm" guest /bin/cat /root/boot-frame.log
tools/login-test/save-frame.sh "$vm" reboot "$work/reboot.png"
```

Archive the desired frames before rebooting or removing the VM. `python3 tools/login-test/contact-sheets.py` regenerates the labeled X11 sheets with ImageMagick. Remove the named runner, its disk volume and the test images when finished. The release feed and temporary Rust build products can then be removed.

[Results and proof limits](evidence/REPORT.md)

## Review regression run

Use `SUNNA_LOGIN_PREFIX=sunna-fix2-test` to isolate names. The screenshot exporter accepts the same prefix. The timing fixture compares guest uptime in the viewer with a real Xsession drawing client. Use the same compiler optimization level for both host and client. These commands produce the two builds used for the optimized measurements.

```sh
export PATH="$HOME/.cargo/bin:$PATH"
export SUNNA_LOGIN_PREFIX=sunna-fix2-test
work=$(mktemp -d /tmp/sunna-fix2-proof.XXXXXXXX)
export CARGO_TARGET_DIR="$work/build" CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_DEV_OPT_LEVEL=2
mkdir "$work/feed"
tools/login-test/test-releases.sh "$work/feed"
cargo build -j4 -p sunna-client --example login-test
strip -o "$work/login-test" "$CARGO_TARGET_DIR/debug/examples/login-test"
export SUNNA_LOGIN_FEED="$work/feed" SUNNA_LOGIN_CLIENT="$work/login-test"
tools/login-test/boot-vm.sh lightdm 24.04
tools/login-test/boot-vm.sh sddm 24.04
```

Build the compatibility driver against the old library. The driver uses the old client’s real Probe, Hello and decoder.

```sh
mkdir "$work/old"
git archive 1c46c9d | tar xf - -C "$work/old"
mkdir -p "$work/old/crates/client/examples"
cp tools/login-test/old-viewer.rs "$work/old/crates/client/examples/"
CARGO_TARGET_DIR="$work/old-target" cargo build --manifest-path "$work/old/Cargo.toml" -p sunnad
CARGO_TARGET_DIR="$work/old-target" cargo build --manifest-path "$work/old/Cargo.toml" -p sunna-client --example old-viewer
strip -o "$work/old-client" "$work/old-target/debug/examples/old-viewer"
strip -o "$work/old-daemon" "$work/old-target/debug/sunnad"
```

In each manager’s runner, install the guest release and stage the fixtures. The two helper functions run from the repository root.

```sh
vm="$SUNNA_LOGIN_PREFIX-lightdm-24.04"  # repeat for sddm
guest() { docker exec "$vm" guest "$@"; }
put() { docker cp "$1" "$vm:/tmp/$2" && guest put "/tmp/$2" "/root/$2"; }
guest /usr/local/bin/install-guest
for f in review-guest.sh prove-guest.sh measure-handoff.sh draw-marker.py compatibility.sh boot-proof.sh changing-resolution.sh gui-reconnect.sh; do
  put "tools/login-test/$f" "$f"
done
put "$work/old-client" old-client
put "$work/old-daemon" old-daemon
guest /bin/chmod 755 /root/old-client /root/old-daemon
guest /bin/bash /root/review-guest.sh receipt
guest /bin/bash /root/measure-handoff.sh
guest /bin/bash /root/prove-guest.sh security
guest /bin/bash /root/review-guest.sh lifecycle
guest /bin/bash /root/review-guest.sh recovery
guest /bin/bash /root/review-guest.sh network
guest /bin/bash /root/review-guest.sh key
guest /bin/bash /root/review-guest.sh worker-failure
guest /bin/bash /root/compatibility.sh
guest /bin/bash /root/prove-guest.sh update
guest /bin/bash /root/review-guest.sh probation
guest /bin/bash /root/prove-guest.sh recover
# Stage the matching distribution debs as described above first.
guest /bin/bash /root/prove-guest.sh desktop
guest /bin/bash /root/boot-proof.sh
```

Start the timing measurement at a greeter with nobody logged in. Its default user-selection steps assume the fixture’s initial user choice. If LightDM remembers one of the test users, pass `SUNNA_LOGIN_SELECTED=viewer1` or `viewer2` with `guest /usr/bin/env … /bin/bash /root/measure-handoff.sh`. The `handoff FILE` step waits for the old connection to close and decodes on a fresh connection. With `SUNNA_MEASURE_LOGIN=1` it also waits for the four colored bars. The script prints both the first decoded frame and the saved marker time. During the measured runs the other emulator was paused. Stop the boot observer before starting other viewers on the same seat.

On the LightDM guest, start at the 1280×800 greeter and run the XRandR proof. The graphical viewer proof uses a guest-compatible build of `sunna-cli`, a guest Xvfb executable and `xwininfo`. It opens an independent authenticated display inside the VM and restarts its real login manager while the window remains open.

```sh
guest /bin/bash /root/changing-resolution.sh
put "$work/window-viewer" window-viewer
put "$work/window-xvfb" window-xvfb
guest /bin/chmod 755 /root/window-viewer /root/window-xvfb
guest /bin/bash /root/gui-reconnect.sh /root/window-viewer /root/window-xvfb
```

The local Rust and static checks for this review were:

```sh
cargo test -j4 -p sunnad -p sunna-host -p sunna-client -p sunna-proto -p sunna-input -p sunna-capture -p sunna-portal -p sunna-clipboard -p sunna-update -p sunna-viewer
cargo check -j4 -p sunnad -p sunna-viewer
CC_aarch64_apple_darwin="$HOME/.local/bin/zcc-aarch64-macos" CARGO_INCREMENTAL=0 cargo check --target aarch64-apple-darwin -p sunna-viewer
python3 tools/login-test/review-contracts.py
```

The remaining commands below ran inside the private systemd container. Its A and B directories contained the signed guest releases, with the update proof client at `/proof/releases/A/.proof-client`. The legacy directory held the old daemon and desktop helper. The live migration script used the test feed’s port 48991. The system-manager sweep runs as container root with only loopback, under `SUNNA_FIX_CONTAINER=1`, and requires an unused `/opt/sunna/host`. Stage `scripts/release/switch_states.py` beside both sweep drivers. The remaining recovery and Xvfb tests run as the ordinary fixture user, with the user systemd bus in `XDG_RUNTIME_DIR`.

```sh
python3 /proof/system-recovery-sweep.py
# Ordinary fixture user
python3 /proof/prove-systemd.py
SUNNA_TEST_HELPER=/proof/releases/A/sunna-update-linux-x86_64 python3 /proof/test-recovery.py
bash /proof/prove-input.sh
bash /proof/migration-local.sh
python3 /proof/prove-full-disk.py /home/fixture/full /proof/releases/A/sunna-update-linux-x86_64
python3 /proof/repo/tools/login-test/review-bootstrap.py
SUNNA_TEST_HELPER=/proof/releases/A/sunna-update-linux-x86_64 python3 /proof/repo/tools/login-test/review-probation.py
SUNNA_TEST_HELPER=/proof/latest-recovery bash /proof/repo/scripts/release/test-layout.sh
xvfb-run -n 94 /proof/input-test --ignored
python3 /proof/reserved-x11-name.py /proof/input-test
xvfb-run -n 95 python3 /proof/prove-worker-media.py /home/fixture/managed/host/versions/0.1.0/bin/sunnad
bash /proof/old-display-loss.sh /proof/old-daemon /proof/login-test
```

The input test executable comes from `cargo test -p sunna-input --lib --no-run`. The full-disk fixture uses a private tmpfs with 64 inodes. The per-user virtual run used `recovery run-desktop host :97 640x480` and `recovery run-user host`, then decoded a frame on guest loopback. [Update fixture setup](../../docs/update-verification.md) · [Observed results](evidence/REPORT.md)

## Final review regression run

Use the optimized build settings above, a fresh test feed and
`SUNNA_LOGIN_PREFIX=sunna-fix4`. The login, switch and logout checks run
against the same disconnected LightDM and SDDM guests. Stage the scripts with
the existing `put` helper. Run the migration check after the initial signed
system install. This fixture temporarily grants viewer1 sudo inside the guest,
installs a synthetic prior per-user host, and tests the real installer twice.
If the guest lacks sudo, stage its distribution's sudo package as
`/root/sudo.deb` first. The migrated host retains the fixture identity.

```sh
put tools/login-test/migration-guest.sh migration-guest.sh
guest /bin/bash /root/migration-guest.sh
put tools/login-test/switch-logout.sh switch-logout.sh
guest /bin/bash /root/measure-handoff.sh
guest /bin/bash /root/switch-logout.sh
# LightDM, after returning to the greeter
guest /bin/bash /root/changing-resolution.sh
```

Use a network-isolated container without systemd for
`bash tools/login-test/setup-rollback.sh`. It installs a fixture systemctl
inside that container. The X11 cases need a private container with Xvfb,
Xauthority tools and a mode-1777 `/tmp/.X11-unix`. Run as an ordinary user.

```sh
python3 tools/login-test/reserved-x11-name.py /proof/input-test
bash tools/login-test/virtual-display-check.sh /proof/sunnad scripts/linux-desktop.sh
```
