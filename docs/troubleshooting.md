# Troubleshooting

[← Back to the README](../README.md)

| What you see | What to do |
|---|---|
| **Not responding** or **Offline** | The host isn't running or can't be reached. On a Linux host, run `sunna-host status`. Check that Tailscale is connected on both devices, and that no firewall on the host blocks UDP port 48800. |
| **Key doesn't match** | The computer's key changed. `sunna-host link` on it shows the new link; edit the computer in the app and paste it. |
| **In use** | Someone else is connected (the app says who). A computer takes one viewer at a time. |
| **Needs an update** | The app and the host are from different versions. Update both: `git pull`, then run the install scripts again. |
| **Address not found** | The name doesn't resolve. Use the IP address or the full Tailscale name (`name.tailnet.ts.net`). |
| A session drops, often after a while | The app says why. "Lost the connection" usually means the network, or the other computer went to sleep. Turn off automatic suspend there (on GNOME: Settings → Power → Automatic Suspend). |
| Sharing was stopped on the computer | Someone pressed **Stop** in GNOME's sharing indicator. Connect again. |
| A GNOME (Wayland) desktop shows the screen but ignores the mouse and keyboard | **Allow Remote Interaction** was off when sharing was approved. Run `sunna-host setup --desktop` again at the computer and switch it on. |
| GNOME asks for permission when someone connects | GNOME forgot the approval. Run `sunna-host setup --desktop` at the computer and keep **Remember This Selection** on. |
| A Linux desktop won't share | On GNOME Wayland or X11: `sunna-host setup --desktop` at the computer. On other Wayland desktops (Hyprland, Sway…): `sunna-host setup --virtual`. |
| No sound | Check **Play Sound** in the session menu and your own volume. On a Linux host, `sunna-host logs` says whether sound started (it needs PulseAudio or PipeWire). A background desktop created before sound support needs `sunna-host desktop-restart` once. |
| Choppy video from a Linux host without an NVIDIA GPU | Choose **30 fps** or a lower resolution in the session menu. If the host was built before nasm was installed, run the install script again. |
| ⌘Tab and ⌘Space stay on your Mac | Allow them in Sunna's Settings → **System shortcuts**, then reconnect. ⌃⌥G also switches them. |
| On Wi-Fi, the Mac's video hitches about once a second | That's AirDrop's radio (AWDL) scanning. `sudo ifconfig awdl0 down` turns it off until macOS turns it back on (or `sudo ifconfig awdl0 up`). |

## Logs

- **Linux host:** `sunna-host logs`, or `sunna-host report` for a summary to share (the key is left out).
- **Mac host:** its Terminal window.
- **Mac app:** each session's log is in `~/Library/Logs/Sunna`. For the app itself, quit Sunna and run `/Applications/Sunna.app/Contents/MacOS/Sunna` in Terminal.
- **Android app:** `adb logcat -s sunna`.
