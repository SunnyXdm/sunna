# Share a Mac

Install Sunna.app with `scripts/install-mac-app.sh`, open Settings and turn on **Share this computer** under **This computer**. Use **Screen Recording…** to allow the picture and system sound and **Accessibility…** to allow mouse and keyboard input. On macOS 15 or later, allow local network access if prompted. Use the link or QR code in Settings to add the Mac on your viewer. The link contains its key.

Close the window. The background host owns the menu bar item. Its menu shows the listening address, whether permission is missing, and who is connected. **Open Sunna** brings back the window. **Stop Sharing** removes the login agent. **Quit Sunna Host** pauses the agent until the next login or until you enable sharing again.

Sunna prefers the Mac’s assigned Tailscale IPv4 address. Otherwise it uses the local IPv4 interface selected by the default route. If Tailscale comes up after login, automatic mode retries with increasing delays up to two minutes and moves the listener once the viewer disconnects. Choose a trusted network. [Network security](security.md)

Sharing starts when this user logs in to the graphical desktop. After a reboot someone must unlock FileVault, if enabled, and log in. A connected session asks macOS to prevent idle system sleep. Sharing alone lets the Mac sleep normally.

For foreground scripts, run `~/.local/bin/sunna host` or `scripts/run.sh host`. Use `sunna host --help` for options, including `--listen tailscale`, `--listen lan`, or an explicit `--listen IP:PORT`. CLI mode can use a private `SUNNA_TOKEN` environment variable or the key in Sunna’s Settings. The app switch and menu can stop a foreground host. The LaunchAgent always reads this Mac’s saved key. Restart sharing after changing the key or an explicitly selected interface.

The LaunchAgent is `~/Library/LaunchAgents/dev.sunna.host.plist`. Its stderr log is `~/Library/Logs/Sunna/host.stderr`. The prebuilt release and the background host still need a real Mac test, including unattended permission renewal and idle resource use. [Mac verification procedure](dev/mac-host-test.md)
