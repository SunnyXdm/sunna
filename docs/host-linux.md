# Share a Linux computer

Install the host on a Linux computer you want to use from somewhere else: your desktop from your laptop, a home server from your phone. Once set up, it runs as a service, so it's there whenever you connect, and it uses no CPU while nobody is connected.

[← Back to the README](../README.md)

## 1. Pick what to share

| Your computer | What gets shared | Setup |
|---|---|---|
| A desktop or laptop you also sit at, with an **X11** session | the desktop you're logged into | `sunna-host setup --desktop` |
| A desktop or laptop with **GNOME on Wayland** (Ubuntu's default) | the desktop you're logged into, after you approve it once | `sunna-host setup --desktop` |
| A **server or VM** with no screen, or another Wayland desktop (Hyprland, Sway…) | a separate desktop that runs in the background | `sunna-host setup --virtual` |

Not sure which session you're in? Run `echo $XDG_SESSION_TYPE` in a terminal on the desktop: it says `x11` or `wayland`.

If you don't choose, `sunna-host setup` picks for you: your own desktop when you run it in a terminal on it, a background desktop otherwise (over SSH, for example).

## 2. Install

There are no downloads yet, so you build the host on the computer itself (a few minutes). It's tested on x86-64; ARM64 should build but hasn't been tried. You need [Rust](https://rustup.rs), a C/C++ compiler and nasm (without nasm, video encoding is 3–4× slower). On Debian and Ubuntu:

```sh
sudo apt install build-essential nasm
```

Then, **as the user whose desktop you want to share** (not root):

```sh
git clone https://github.com/SunnyXdm/sunna.git && cd sunna
scripts/install-host-linux.sh
```

This installs `sunnad` (the host) and `sunna-host` (the command that runs it) into `~/.local/bin`, then runs `sunna-host setup`. To update later: `git pull` and run the script again. Your settings and key are kept.

**Or as a Debian/Ubuntu package**, for example to install it on several machines:

```sh
scripts/package-linux-deb.sh                # makes dist/sunna-host_<version>-<build>_<arch>.deb
sudo apt install ./dist/sunna-host_*.deb
sunna-host setup                            # as the user whose desktop to share
```

A package runs on the release it was built on and newer ones (build on Ubuntu 22.04 to cover Ubuntu 22.04+ and Debian 12+).

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

Nothing to approve. The host starts when you log in and shares what's on your screen. Someone has to be logged in.

### A server or VM (background desktop)

The background desktop needs Xvfb and a desktop environment; setup tells you what's missing. On Debian and Ubuntu:

```sh
sudo apt install xvfb x11-utils dbus-x11 xfce4 pulseaudio-utils
```

It uses KDE Plasma if installed, otherwise XFCE. It starts at boot, and keeps running when you disconnect, with your apps still open, like a real computer. `--size 1920x1080` sets its size (default 1710x1112).

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
| `--listen WHERE` | `tailscale` (only your tailnet; the default when Tailscale is running), `lan` (your local network), or an address like `192.168.1.20:48800` |
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
