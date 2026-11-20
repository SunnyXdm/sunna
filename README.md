<p align="center"><img src="docs/images/icon.png" width="112" height="112" alt="Sunna"></p>

<h1 align="center">Sunna</h1>

<p align="center">Your other computers, on the device in your hands.<br>A remote desktop for your own machines, over your own network.</p>

<p align="center"><a href="https://sunna.sunnydx.dev/">Website</a> · <a href="https://sunna.sunnydx.dev/guides/">Guides</a> · <a href="docs/troubleshooting.md">Troubleshooting</a></p>

<p align="center"><img src="docs/images/hero.webp" alt="In the Sunna app, a click on a computer in Gallery grows its screen into a session; a command is typed in its terminal, and the session closes back into Gallery" width="880"></p>

Sunna shows another computer's screen, plays its sound, and sends your keyboard, mouse, touch and clipboard back. Your devices talk to each other directly. There are no accounts and nothing passes through a server of ours.

## What it does

- **Opens a computer like opening a window.** The app lists your computers with the last picture of each. Click one and you're in it, full screen.
- **Typing that keeps up.** Video takes time to encode, most of all without a GPU. Small changes, like the letter you just typed, go ahead of the video as exact image tiles. On a host encoding on its CPU, a typed letter reached the viewer in about 2 ms where a video frame took about 35 ([the fast lane](docs/using.md#the-fast-lane)).
- **Checks it's really your computer.** A link or QR code carries the computer's identity. The app checks it before it sends anything, proves it knows the key without sending the key, and refuses a computer whose identity has changed ([Security](docs/security.md)).
- **Share a computer with a switch.** On Linux, Settings → Share this computer starts sharing and keeps it on after you close the window. The same switch is written for the Mac and waiting for its first test on one.
- **Reach it after a restart.** A Linux host can run for the whole machine, so after a reboot you get the login screen and sign in from where you are. Experimental; proven on LightDM and SDDM in test machines.
- **Updates itself.** Signed releases, a switch between versions that can't half-happen, and a return to the previous version if a new one doesn't start. Built and tested; the first public release hasn't been published yet.

## What runs where

| | Mac | Linux | Android | Windows |
|---|---|---|---|---|
| **Reach other computers** (the app) | yes ([guide](docs/app-mac.md)) | yes ([guide](docs/app-linux.md)) | yes ([guide](docs/app-android.md)) | planned |
| **Be reached** (the host) | yes ([guide](docs/host-mac.md)) | yes: X11, GNOME on Wayland, or a server with no screen ([guide](docs/host-linux.md)) | no | experimental |

**Status: early.** Mac → Mac and Mac → Linux are in daily use. The Linux and Android apps are newer. Sharing a GNOME desktop on Wayland is tested on GNOME 46, 49 and 50, not yet on a real laptop. The newest parts (the sharing switch, the Mac host in the menu bar, the whole-machine Linux host, and self-update) have been tested in containers and virtual machines on Linux, and not yet on a real Mac. [What has and hasn't been verified](docs/update-verification.md).

## Get started

Today you build Sunna from source. Each guide has the one script that does it.

1. **Put both devices on [Tailscale](https://tailscale.com)** (free for personal use). Sunna works on any network you trust, but Tailscale makes your computers reachable from anywhere and keeps everyone else out. Keep a host off the public internet.
2. **Share a computer**: [Linux](docs/host-linux.md) or [Mac](docs/host-mac.md). When sharing starts you get a **link** and a **QR code**:
   ```
   sunna://100.101.102.103?key=7f3a…&id=Qm9v…
   ```
   The link contains the key that grants access to that computer. Treat it like a password.
3. **Install the app** on the device you'll use: [Mac](docs/app-mac.md), [Linux](docs/app-linux.md) or [Android](docs/app-android.md).
4. **Add the computer** in the app: paste the link, or scan the QR code with your phone. It shows **Ready** when it can be reached. Click or tap it to connect.

## Updating

Until the first release is published, update the way you installed: `git pull`, then run the same install script again.

Signed downloads change that. A host checks for a release once a day and tells you, or installs it by itself if you turned that on; it never restarts under someone who is connected. The app shows which of your computers are out of date and can tell each one to update. `sunna-host update --check`, `--yes` and `--rollback` do the same from a terminal. All of this is in the code and [tested](docs/update-verification.md), and waits on that first release.

## Guides

| | |
|---|---|
| [Get started](docs/get-started.md) | the short version of everything below |
| [Share a Linux computer](docs/host-linux.md) | your desktop or laptop (X11, or GNOME on Wayland), a server with no screen, or the whole machine including its login screen |
| [Share a Mac](docs/host-mac.md) | the switch in Settings and the menu bar item |
| [The app on a Mac](docs/app-mac.md) · [on Linux](docs/app-linux.md) · [on Android](docs/app-android.md) | install it, and what's different on each |
| [Using Sunna](docs/using.md) | adding computers, connecting, the session menu, shortcuts |
| [Troubleshooting](docs/troubleshooting.md) | when something doesn't connect or look right |
| [Security and privacy](docs/security.md) | keys, identity, encryption, what's logged |
| [Building from source](docs/building.md) | for developers: the workspace, layout and tests |

## License

No license has been chosen yet; until one is, all rights are reserved.
