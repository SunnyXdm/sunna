# Get started with Sunna

Install a host on the computer you want to reach and the app on the device in your hands. One computer can do both jobs. There are signed downloads for Linux on x86-64 (a `.deb`, an `.rpm` and an install command) and for Android (an APK). On a Mac and on ARM Linux you build Sunna from source for now.

## Share a computer

Connect both devices to the same Tailscale network or a trusted local network. Sunna connects directly, so the app needs to reach the host's UDP port 48800. Keep the host off the public internet. See [Security and privacy](security.md).

- [Share a Linux computer](host-linux.md) for an X11 desktop, GNOME on Wayland, or a virtual desktop on a server.
- [Share a Mac](host-mac.md) to share its screen while Terminal is open.

Windows hosting is experimental. The setup guides above cover the Mac and Linux hosts.

When the host starts, keep its `sunna://` link or QR code handy. It contains the key that grants access to that computer.

## Connect from another device

Install the app on [macOS](app-mac.md), [Linux](app-linux.md), or [Android](app-android.md). Open **Add a Computer** and paste the host's link. On Android you can also scan its QR code with your phone's camera. Select the computer when it shows **Ready**.

A Windows desktop client is planned. There is no Windows client installer yet.

## Use a session

[Using Sunna](using.md) covers keyboard shortcuts, sound, clipboard sharing, screen pictures and update settings. If the host stays offline or the session opens without a picture, start with [Troubleshooting](troubleshooting.md).

## Builds and updates

For build dependencies and tests, see [Building from source](building.md). For the signed installer, update and rollback test results and outstanding platform checks, see [Update verification](update-verification.md).
