# Share a Linux computer

Install the host on a Linux computer you want to use from somewhere else: your desktop from your laptop, a home server from your phone. Once set up, it runs as a service, so it's there whenever you connect, and it checks desktop availability while it waits for a viewer.

[← Back to the README](../README.md)

## Share this computer from the Linux app

Open **Settings → Share this computer** at the desktop you want to reach.
Sunna installs the signed host if needed and starts it with your graphical
session. On GNOME or KDE Wayland, approve the desktop’s permission request.
Settings shows the address and offers **Copy** and **Show QR Code**. The link
includes the connection key. Share it privately. See [Network security](security.md).

Closing Sunna leaves sharing on. Turn the switch off to stop the host and
its automatic start. The switch also controls a user host configured with
`sunna-host setup`. A system host that shares the login screen is managed
by an administrator.

`sunna-host status --json` exposes local status for the app. While sharing,
its `link` field contains the connection key. Keep the output out of logs and
support reports.

## 1. Pick what to share

| Your computer | What gets shared | Setup |
|---|---|---|
| A desktop or laptop you also sit at, with an **X11** session | the desktop you're logged into | `sunna-host setup --desktop` |
| A desktop or laptop with **GNOME on Wayland** (Ubuntu's default) | the desktop you're logged into, after you approve it once | `sunna-host setup --desktop` |
| A **server or VM** with no screen, or another Wayland desktop (Hyprland, Sway…) | a separate desktop that runs in the background | `sunna-host setup --virtual` |

Not sure which session you're in? Run `echo $XDG_SESSION_TYPE` in a terminal on the desktop: it says `x11` or `wayland`.

If you don't choose, `sunna-host setup` picks for you: your own desktop when you run it in a terminal on it, a background desktop otherwise (over SSH, for example).

### Whole-machine sharing (experimental signed Linux host)

A build containing the system installer can share the active local session on
`seat0` from boot. Run `sunna-host setup --system` when that installer is
available, or run the corresponding signed installer with `host --system`. Choose
`--listen tailscale`, `--listen lan`, or a numeric socket address such as
`--listen '[::1]:48800'`. The unit resolves the named network at startup.
With Tailscale selected, it retries if the interface comes up late.

The signed host lives under `/opt/sunna/host`. Its system unit starts the
recovery supervisor outside the version directories. The key and settings
live under `/etc/sunna`. Run the installer as the owner of an existing
per-user desktop host to migrate it. The installer keeps a generated-format
key, rotates other keys, and waits for a frame or an explicitly reported empty
seat before enabling the system unit and removing the old settings. An
unsuccessful setup restores the previous unit, settings and service state.
If the seat has a selected session that Sunna cannot share, migration waits
for a usable frame and restores the previous configuration on timeout. After rotation, re-share the link from
`/opt/sunna/host/current/bin/sunna-host link`.

**Anyone with the sharing key can try passwords at the login screen.** Use
the private link to pair with the host’s identity. Current viewers verify that
identity during connection and prove possession of the sharing key. A
proof-required host refuses older viewers. Use your tailnet to limit who can
reach the login screen.

The host watches logind’s active-session notifications. Switching away and
back retires the original worker and closes its viewer connection.
The desktop viewer keeps its window and reconnects to negotiate the new
screen size. An older viewer can reconnect manually. Each connection has
its own video history, audio encoder, clipboard and input destination.
Session workers supply the remote pointer shape. Sound and clipboard are
suppressed on greeters. A media IPC failure triggers worker replacement.

System mode shares only `seat0`. It has no fast-lane tiles or separate
virtual desktop. Sharing pauses if one account has multiple graphical
sessions because its portal and sound server can belong to either desktop.
See the [guest measurements and verification limits](../tools/login-test/evidence/REPORT.md).

On the GNOME 46, 49 and 50 GDM Wayland greeters, Mutter refused both session
requests. Sunna reports that its login screen cannot be shared. A machine owner can select a supported X11 login manager or configure
automatic login if appropriate for the physical security of the computer.
Installation leaves login-manager configuration to its owner. To grant the
system host access to a Wayland user desktop, sign in locally as that user,
run `/opt/sunna/host/current/bin/sunnad --authorize` in a graphical terminal
and approve the screen and remote interaction in the portal. Each user grants
their own permission. Existing per-user desktop and virtual modes require
no administrator access.

## 2. Install

**On an x86-64 computer**, install the signed download. No compiler is needed. As the user who will share the desktop, run:

```sh
curl -fsSL --proto '=https' --proto-redir '=https' https://sunna.sunnydx.dev/install.sh -o sunna-install.sh &&
  test -s sunna-install.sh && sh sunna-install.sh host
```

The file it downloads is a short shell script you can read first. It fetches a small helper, checks the helper against a hash written in the script, and the helper checks the release's signature before it installs anything.

**On ARM, or to build it yourself**, install Git, Rust and a C/C++ compiler, plus NASM on x86-64, then run:

```sh
git clone https://github.com/SunnyXdm/sunna.git
cd sunna
scripts/install-host-linux.sh
```

The script builds the host and runs setup. Pass `--desktop` or `--virtual` to the script to choose explicitly. The setup section below describes the prompts.

### What the download installs

The host and its helper scripts live under
`~/.local/share/sunna/host/versions`; `current` points to the selected version,
and commands in `~/.local/bin` point through it. The app has a separate
`sunna/app` directory, so updating one cannot roll back the other.

Old source installations in `~/.local/bin` are saved as a previous version before
the commands become links. Settings and host keys are kept. A host already
running an older build needs one restart to load the new supervisor. Pass `--restart` after host when installing to restart and verify the migrated service. To choose the time yourself, run `~/.local/bin/sunna-host migrate-service` afterwards.

For development, `scripts/install-host-linux.sh` still builds from source and
installs into the same layout. A `.deb` uses `apt` for updates; it does not
replace system files through the self-updater.

## 3. Set it up

Run `sunna-host setup` (with `--desktop` or `--virtual` if you want to choose). It starts sharing, sets itself to start again on its own, and prints the address, key, link and a QR code:

```
  Address   100.101.102.103
  Key       7f3a…
  In the Sunna app, choose Add a Computer and paste:
    sunna://100.101.102.103?key=7f3a…
  Or scan this with your phone's camera: (QR code)
```

Add it in the app with that link. `sunna-host link` shows it again any time.

### Your own desktop on GNOME (Wayland)

Wayland doesn't let programs read the screen on their own, so GNOME asks you once. Run `sunna-host setup --desktop` **at the computer** (not over SSH). GNOME shows a **Remote Desktop** dialog:

1. Switch on **Allow Remote Interaction**. It's off by default, and without it you'd see the screen but couldn't use the mouse or keyboard.
2. Keep **Remember This Selection** on, so later connections don't ask again.
3. Choose the screen and click **Share**.

After that, connecting just works. While someone is connected, GNOME shows an orange sharing icon in the top bar; **Stop** there ends the session. If GNOME ever forgets the permission, run `sunna-host setup --desktop` again at the computer.

KDE Plasma on Wayland should work the same way but hasn't been tested yet. Other Wayland desktops need `--virtual`.

### Your own desktop on X11

The host starts when you log in and shares your X11 screen. Capture, input
and startup checks use the local filesystem socket and verify the server
before using Xauthority. With graphical session metadata they check the
session scope or the root display server’s VT. A per-user virtual service
can lack logind session metadata. In that mode the configured display number
and verified root or same-user X server select the desktop. Separate Unix
accounts are needed to isolate mutually untrusted desktops.

### A server or VM (background desktop)

The background desktop needs Xvfb and a desktop environment; setup tells you what's missing. On Debian and Ubuntu:

```sh
sudo apt install xvfb dbus-x11 xfce4 pulseaudio-utils
```

It uses KDE Plasma if installed, otherwise XFCE. It starts at boot, and keeps running when you disconnect, with your apps still open, like a real computer. `--size 1920x1080` sets its size (default 1710x1112).

Running `sunna-host new-identity` replaces the identity and reloads an active
host. A stopped host stays stopped. Re-add its new private link on each viewer.

## Updates

```sh
sunna-host update --check
sunna-host update --yes
sunna-host update --rollback
```

A running host queues changes until nobody is connected. Downloads are staged,
verified, and checked with `sunnad --version` before switching. The supervisor
rolls back a new version after two failed startup attempts; failed versions are
not automatically retried. The current and previous releases are retained.

Daily checks default to **notify**. Use `install.sh host --auto-update`, or set
`SUNNA_UPDATE=auto`, `notify`, or `off` in `~/.config/sunna/host.env` and restart.
An explicit environment setting takes precedence over policy chosen from the
app. `SUNNA_UPDATE_URL` overrides the manifest base URL for self-hosting; it
never changes the trusted release key. Remote update requests require the host
key and cannot supply a download URL.

## Everyday commands

| Command | What it does |
|---|---|
| `sunna-host status` | is it running, where is it listening, is someone connected |
| `sunna-host link` | show the address, key, link and QR code again |
| `sunna-host start` / `stop` / `restart` | start or stop sharing |
| `sunna-host logs` | follow the log |
| `sunna-host new-key` | make a new key (devices that saved the old one need the new link) |
| `sunna-host desktop-restart` | restart the background desktop (closes its apps) |
| `sunna-host desktop-stop` | stop the background desktop, its apps and the host |
| `sunna-host report` | a summary of the system and recent log to send when asking for help (the key is left out) |
| `sunna-host uninstall [--purge]` | stop sharing and remove it; `--purge` also deletes the settings and key. For the package, follow with `sudo apt remove sunna-host` |
| `sunna-host help` | all commands and options |

## Setup options

| Option | |
|---|---|
| `--desktop` | share the desktop you're logged into |
| `--virtual` | share a background desktop |
| `--listen WHERE` | `tailscale` (only your tailnet, the default for new setups with the Tailscale CLI installed), `lan` (your local network), or an address like `192.168.1.20:48800` |
| `--name NAME` | the name shown in the app (default: the hostname) |
| `--key KEY` | use this key instead of making one |
| `--size WxH` | the background desktop's size |

Settings are in `~/.config/sunna/host.env` (readable only by you). After editing it, run `sunna-host restart`.

## Good to know

- **Sound:** the host shares what's playing. On your own desktop that's what goes to the speakers (they keep playing too); a background desktop has a silent output of its own. It needs PulseAudio or PipeWire, which nearly every desktop has. It never shares a microphone.
- **Video:** with an NVIDIA GPU (GTX 10-series or newer, driver 530+) video is encoded on the GPU, in about 4 ms a frame. Otherwise on the CPU, about 8–9 ms a frame at 1080p on a 6-core machine. On slow computers, pick **30 fps** in the session menu.
- **Sleep:** while someone is connected, the host keeps the computer awake if it's allowed to. When nobody is connected, the computer can still go to sleep, and then you can't reach it: on a machine you want to reach any time, turn off automatic suspend (on GNOME: Settings → Power → Automatic Suspend). On a laptop, closing the lid suspends it unless you set `HandleLidSwitch=ignore` in `/etc/systemd/logind.conf` and reboot.
- **Network:** the host listens on UDP port 48800. A firewall on the host must allow it.

## Running sunnad yourself

`sunna-host` is a convenience. To run the host directly, from your own service or a container, give it a display, an address and a key:

```sh
DISPLAY=:0 sunnad --source screen --listen 100.101.102.103:48800 --name office --token "$KEY"
```

On a Wayland session it uses the desktop portal instead of `DISPLAY` (run `sunnad --authorize` once at the computer first; `SUNNA_CAPTURE=x11` or `=portal` forces one or the other). `sunnad --help` lists the rest: stream size, frame rate, codec, bitrate, `--no-audio`, `--no-clipboard`.
