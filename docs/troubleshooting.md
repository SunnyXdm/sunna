# Troubleshooting

[← Back to the README](../README.md)

| What you see | What to do |
|---|---|
| **Not responding** or **Offline** | The host isn't running or can't be reached. On a Linux host, run `sunna-host status`. Check that Tailscale is connected on both devices, and that no firewall on the host blocks UDP port 48800. |
| **Key doesn't match** | The computer's key changed. `sunna-host link` on it shows the new link; edit the computer in the app and paste it. |
| **In use** | Someone else is connected (the app says who). A computer takes one viewer at a time. |
| **Needs an update** | The app and the host are from different versions. The message identifies which side is older. Use `sunna-host update` for a downloaded host and update the app too if needed. Older source-only hosts need one installation of the new downloads locally. |
| **Address not found** | The name doesn't resolve. Use the IP address or the full Tailscale name (`name.tailnet.ts.net`). |
| A session drops, often after a while | The app says why. "Lost the connection" usually means the network, or the other computer went to sleep. Turn off automatic suspend there (on GNOME: Settings → Power → Automatic Suspend). |
| Sharing was stopped on the computer | Someone pressed **Stop** in GNOME's sharing indicator. Connect again. |
| A GNOME (Wayland) desktop shows the screen but ignores the mouse and keyboard | **Allow Remote Interaction** was off when sharing was approved. Run `sunna-host setup --desktop` again at the computer and switch it on. |
| GNOME asks for permission when someone connects | GNOME forgot the approval. Run `sunna-host setup --desktop` at the computer and keep **Remember This Selection** on. |
| A Linux desktop won't share | On GNOME Wayland or X11: `sunna-host setup --desktop` at the computer. On other Wayland desktops (Hyprland, Sway…): `sunna-host setup --virtual`. |
| Waiting for permission on that Mac | Open Settings on the host Mac. Use Screen Recording and Accessibility under This computer. If Sunna is already listed as allowed, remove its old entry, add the installed Sunna.app again and grant access. If macOS requests a relaunch, stop sharing from the menu, quit the window, reopen Sunna and enable sharing. |
| No sound | Check **Play Sound** in the session menu and your own volume. On a Linux host, `sunna-host logs` says whether sound started (it needs PulseAudio or PipeWire). A background desktop created before sound support needs `sunna-host desktop-restart` once. |
| Choppy video from a Linux host without an NVIDIA GPU | Choose **30 fps** or a lower resolution in the session menu. If the host was built before nasm was installed, run the install script again. |
| ⌘Tab and ⌘Space stay on your Mac | Allow them in Sunna's Settings → **System shortcuts**, then reconnect. ⌃⌥G also switches them. |
| On Wi-Fi, the Mac's video hitches about once a second | That's AirDrop's radio (AWDL) scanning. `sudo ifconfig awdl0 down` turns it off until macOS turns it back on (or `sudo ifconfig awdl0 up`). |

## Logs

- **Linux host:** `sunna-host logs`, or `sunna-host report` for a summary to share (the key is left out).
- **Mac host:** the LaunchAgent writes to `~/Library/Logs/Sunna/host.stderr`. A foreground host writes to its Terminal. For permission and startup diagnostics, follow the [Mac test procedure](dev/mac-host-test.md).
- **Mac app:** each session's log is in `~/Library/Logs/Sunna`. For the app itself, quit Sunna and run `/Applications/Sunna.app/Contents/MacOS/Sunna` in Terminal.
- **Android app:** `adb logcat -s sunna`.

## Downloaded installations

- **Signature or SHA-256 mismatch:** nothing is switched. Retry from the official
  feed; do not disable verification. `SUNNA_UPDATE_URL` changes the feed, not the key.
- **Truncated download:** the partial file is retained for a Range retry. It is
  rehashed in full before installation; a partial archive is never unpacked.
- **Update queued:** a viewer is connected, or the host is still preparing the
  download. Disconnect to allow the switch and restart.
- **A release failed to start:** the supervisor restores the previous release
  after two attempts. The failed version stays in `state.json` and is skipped.
- **System package:** update with `apt`, `dnf` or `pacman`. The updater does not
  replace `/usr/bin` or other package-managed files.
- **Missing WebKitGTK:** install WebKitGTK 4.1 (`libwebkit2gtk-4.1-0` on
  Debian/Ubuntu, `webkit2gtk4.1` on Fedora, `webkit2gtk-4.1` on Arch).

Update state is under `~/.local/share/sunna/{host,app}/state.json`. The previous
release is kept for `sunna-host update --rollback`. Mac and Windows installation
paths still need testing on real machines; see [verification](update-verification.md).


## Linux answers the probe but cannot share its screen

A `not-sharing` result means the machine answered with the right key. Read
the accompanying `detail`. For an X11 host, check whether the selected X
server is running and supports XTEST. For the system host, run
`loginctl show-seat seat0 -p ActiveSession` and inspect that session with
`loginctl show-session SESSION -p Active -p Class -p Type -p Display`.
The host retries when the display returns. For a Wayland desktop, complete
desktop sharing approval as that user at the computer. A Wayland greeter
can remain unavailable while a signed-in desktop is shareable.

An older viewer closes on login, logout or user switch. Reconnect it to see
the new session. For system mode, check `/run/sunna-host/listening` for the
actual bound address and `/run/sunna-host/receipt` for the systemd invocation,
daemon child PID and observed state (`frame` or `empty`). Check the unit's
InvocationID after a restart. A saved portal approval is a candidate for a
new session. Readiness requires the backend to produce a picture.

**Identity changed.** Check with the owner before adding the computer again. After a reinstall or `sunna-host new-identity`, use its new link or QR code. Changing the address or the shared key in Edit keeps the saved identity.

**Needs an update.** Install a signed download on the computer by hand. For an older host, Edit offers “Allow this older computer” until it has verified an identity. This sends the sharing key to the endpoint that answers and marks the connection “not fully verified”. If the host asks for a stronger key, run `sunna-host new-key` and share the new link privately.

Restore the saved computer list and its identities directory together. A restored record with a pin that conflicts with a surviving newer pin refuses the connection. Restoring a complete older backup restores the trust decisions and keys from that date, including any older-computer exception. Re-add a rotated computer using its current link from the host.
