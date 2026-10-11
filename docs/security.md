# Security and privacy

[← Back to the README](../README.md)

- **Keys.** A host only accepts viewers that present its key. Anyone with a computer's address and key can control it, so keep keys (and links and QR codes, which contain them) private. `sunna-host new-key` makes a new one. The Mac and Linux app keep the computers you add in `~/.sunna/machines.json`, readable only by you; the Android app keeps them in its private storage and leaves them out of backups.
- **Use Tailscale.** Connections are encrypted (QUIC with TLS 1.3), but the app does **not yet verify a host's identity** (hosts use self-signed certificates). On a network you don't control, someone in the middle could pretend to be your computer. Tailscale prevents that, since it authenticates both ends itself. `sunna-host setup` listens only on your tailnet when Tailscale is running, and warns you when it falls back to your local network. Never expose a host directly to the internet. Pinning each host's certificate is planned.
- **No cloud, no accounts.** Your devices connect to each other directly. Nothing is sent anywhere else.
- **Logs stay on the device.** Keystrokes and clipboard contents are never logged. (Development builds can send logs to a collector you run, only when `SUNNA_LOG_URL` is set.)
- **On GNOME (Wayland),** the desktop itself asks before anything is shared, and shows an indicator while someone is connected; the approval is stored by GNOME, and Sunna keeps its token in `~/.config/sunna/portal-token`, readable only by you.
- **Idle.** A host with nobody connected uses no CPU and only listens on one UDP port (48800).
