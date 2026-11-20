# The Sunna app on Linux

The same app as on the Mac, for reaching your computers from a Linux PC: the Computers window, and sessions in a window of their own, full screen by default. Works on X11 and Wayland.

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

## Build and install

Install Git, Rust 1.95 or later, a C/C++ compiler, pkg-config and WebKitGTK 4.1 development files. On x86-64, install NASM too. The script checks for missing tools. Install FFmpeg development files for system video decoding.

```sh
git clone https://github.com/SunnyXdm/sunna.git
cd sunna
scripts/install-app-linux.sh
```

Open **Sunna** from the application menu.

## Signed installations (in testing)

The download needs WebKitGTK 4.1 from your distribution: `libwebkit2gtk-4.1-0`
on Debian/Ubuntu, `webkit2gtk4.1` on Fedora, or `webkit2gtk-4.1` on Arch. It
includes a matching LGPL FFmpeg build for H.264 and HEVC software decoding.
The release build does not include FFmpeg hardware decoders; source builds can
use your distribution's FFmpeg for GPU decoding. No compiler is needed to install.

The installer adds Sunna to the app menu and `~/.local/bin`. It stores versions
in `~/.local/share/sunna/app/versions`, separately from the host. The app checks
at launch and daily, then stages a verified download. In Settings → Updates, Automatically switches at the next launch. Ask first waits for Restart to update. Never leaves checks to you. Disconnect an active viewer before restarting.

For development, use `scripts/install-app-linux.sh` with Rust, a C compiler,
WebKitGTK development files and optional FFmpeg development files. It uses the
same version layout. See [verification](update-verification.md) for tested paths.

## What's different from the Mac

| | |
|---|---|
| Hotkeys | Ctrl+Alt instead of ⌃⌥: **Ctrl+Alt+M** session menu, **Ctrl+Alt+F** full screen, **Ctrl+Alt+S** stats, **Ctrl+Alt+Q** disconnect |
| Your desktop's shortcuts | while the session window is focused, Alt+Tab, Super and other shortcuts go to the other computer (on Wayland, your desktop may ask you once). **Ctrl+Alt+G** gives them back |
| A Mac on the other end | Ctrl works as ⌘ there (Ctrl+C copies) and Super as ⌃. Set `SUNNA_SWAP_CTRL_CMD=0` to keep them as they are |
| Video | the prebuilt app decodes H.264 and HEVC in software; a source build can use the system FFmpeg for GPU decoding |
| Clipboard | goes through X11, which on Wayland means XWayland (on by default on GNOME, KDE and Hyprland) |

## Next

- [Using Sunna](using.md): adding computers, connecting, the session menu.
- To share this Linux computer too, see [Share a Linux computer](host-linux.md).
