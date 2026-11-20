# Security and privacy

[← Back to the README](../README.md)

- **Keys.** A viewer needs the sharing key to connect. Anyone with a computer's address and key can control it, so keep keys (and links and QR codes, which contain them) private. `sunna-host new-key` makes a new one. The Mac and Linux app keep the computers you add in `~/.sunna/machines.json`, readable only by you; the Android app keeps them in its private storage and leaves them out of backups.
- **Verify the computer.** A new link or QR code carries the host's public-key fingerprint. The viewer checks it during TLS. Each side then proves it knows the sharing key using an HMAC bound to that connection. A computer added without a fingerprint saves the first one that completes the proof. A different fingerprint stops the connection.
- **Older computers.** Even the first probe requires proof. If the host needs an update, install a signed download on it by hand. Edit offers “Allow this older computer” only until a fingerprint is supplied or a proof succeeds. With that switch on, even a background check sends the sharing key inside TLS to whoever answers as an old host. Someone impersonating the computer can obtain it. Older apps still send the key to new hosts unless the host requires proof. Set `SUNNA_REQUIRE_PROOF=1` to require it. Whole-machine hosts require it by default.
- **Network access.** Use Tailscale to limit which devices can reach the host. Keep the host off the public internet.
- **No cloud, no accounts.** Screen, sound, input and clipboard data travel directly between your devices. Signed installations also contact the configured release server to check for and download updates.
- **Logs stay on the device.** Keystrokes and clipboard contents are never logged. (Development builds can send logs to a collector you run, only when `SUNNA_LOG_URL` is set.)
- **On GNOME (Wayland),** the desktop itself asks before anything is shared, and shows an indicator while someone is connected; the approval is stored by GNOME, and Sunna keeps its token in `~/.config/sunna/portal-token`, readable only by you.
- **Idle.** A waiting host listens for connections on UDP port 48800. With no viewer connected, the Linux whole-machine worker and the Mac sharing host take a short local test frame to check that the display can deliver pictures. Streaming and video encoding start with a viewer session.


## Linux whole-machine host

The machine key authorizes access to the login screen as well as the desktop.
Anyone holding it can submit passwords to the local login manager. Use an
authenticated network when entering login credentials. Sunna relays keyboard
events and the login manager performs authentication.

The system service loads its key from root-private settings in `/etc/sunna`.
Only the root-owned signed installation under `/opt/sunna` supplies worker
code. Workers inherit a private socket, a freshly constructed environment
and the selected session user's UID and primary GID. They have no supplementary
groups. Xauthority discovery, portal requests and input injection take place
after the UID change. Local status excludes the machine link and key.


Linux X11 capture, input, clipboard and pointer handling connect through the
filesystem display socket. The system worker checks the Unix peer before X11
authentication, including the logind session scope or the root display
server’s VT. Unauthenticated connections share an eight-second deadline,
eight slots per source IP and 64 slots across the host. Failed machine-key
checks trigger temporary backoff. Operating-system password policy still
applies at the login screen.
