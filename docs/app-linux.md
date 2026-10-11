# The Sunna app on Linux

The same app as on the Mac, for reaching your computers from a Linux PC: the Computers window, and sessions in a window of their own, full screen by default. Works on X11 and Wayland.

[← Back to the README](../README.md)

## Install

You need [Rust](https://rustup.rs) 1.95 or later, a C/C++ compiler, WebKitGTK 4.1, and FFmpeg's development files (for decoding video on the GPU and for HEVC). On Debian and Ubuntu:

```sh
sudo apt install build-essential nasm libwebkit2gtk-4.1-dev libavcodec-dev libavutil-dev
```

Then:

```sh
git clone https://github.com/SunnyXdm/sunna.git && cd sunna
scripts/install-app-linux.sh
```

It installs Sunna in `~/.local/bin` for your user (no root needed) and adds it to your app menu. On other distributions, the script names the packages you're missing. To update: `git pull` and run it again.

## What's different from the Mac

| | |
|---|---|
| Hotkeys | Ctrl+Alt instead of ⌃⌥: **Ctrl+Alt+M** session menu, **Ctrl+Alt+F** full screen, **Ctrl+Alt+S** stats, **Ctrl+Alt+Q** disconnect |
| Your desktop's shortcuts | while the session window is focused, Alt+Tab, Super and other shortcuts go to the other computer (on Wayland, your desktop may ask you once). **Ctrl+Alt+G** gives them back |
| A Mac on the other end | Ctrl works as ⌘ there (Ctrl+C copies) and Super as ⌃. Set `SUNNA_SWAP_CTRL_CMD=0` to keep them as they are |
| Video | decoded on the GPU (NVIDIA, Intel, AMD) when it can; otherwise in software, asking for H.264, which is cheaper to decode |
| Clipboard | goes through X11, which on Wayland means XWayland (on by default on GNOME, KDE and Hyprland) |

## Next

- [Using Sunna](using.md): adding computers, connecting, the session menu.
- To share this Linux computer too, see [Share a Linux computer](host-linux.md).
