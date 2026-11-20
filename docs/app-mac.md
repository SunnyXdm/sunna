# The Sunna app on a Mac

The app lists your computers with a live preview of each one and opens sessions in their own window, full screen by default.

[← Back to the README](../README.md)

## Build and install

To build locally:

```sh
git clone https://github.com/SunnyXdm/sunna.git && cd sunna
scripts/install-mac-app.sh
```

That needs Rust and the Xcode command line tools (`xcode-select --install`).

## Signed installations (in testing)

The installer verifies the release, then signs the bundle using this Mac's
**Sunna Local Code Signing** identity. If needed it creates that identity in
the login keychain, following the source installer's procedure. macOS asks you
to trust the certificate and allow codesign to use its key. Updates reuse that
identity to preserve Accessibility and Screen Recording permissions. There is
no paid Apple developer identity or notarization.

Versions live in `~/.local/share/sunna/app`; `/Applications/Sunna.app` (or
`~/Applications/Sunna.app` when needed) points
through the atomic `current` link. A previous ordinary bundle is retained on
first migration. The installer uses `~/Applications` when `/Applications` is not writable.
Settings → Updates offers Automatically, Ask first and Never. Automatically applies a staged release on the next launch. Ask first waits for Restart to update. Local signing, permission preservation,
Gatekeeper behavior, relaunch and rollback still need proof on macOS.

## Permissions

For viewing, Sunna uses **Accessibility** for sending ⌘Tab, ⌘Space and other system shortcuts to the other computer. Without it, those stay on your Mac and everything else works. Sunna's Settings → **System shortcuts** shows whether it's allowed, and **Allow…** asks for it.

If System Settings shows Sunna as allowed but Sunna says it isn't, that entry is from an older build: remove it with **Reset** there, then allow Sunna again.

For hosting, Screen Recording allows the picture and system sound, and Accessibility allows remote input. The sharing buttons ask the running menu bar host to request its own grants. The host checks for a screen frame before reporting that it can share. Permission attribution across updates still needs verification on a Mac. Close the window to leave only the host. [Share a Mac](host-mac.md)

## Next

- [Using Sunna](using.md): adding computers, connecting, the session menu and shortcuts.
- This Mac can be shared too: see [Share a Mac](host-mac.md).

## Give it to someone else

`scripts/install-mac-app.sh --dmg` also makes `dist/Sunna.dmg`. It's built for your Mac's chip and signed with your own certificate, which other Macs don't know, so the first time macOS blocks it: open it, then click **Open Anyway** in System Settings → Privacy & Security (on macOS 14 and earlier, right-click the app and choose **Open**).

## Remove it

Turn sharing off first to remove its login agent. Quit Sunna and move it to the Trash. Your computers and their keys are in `~/.sunna`; delete that folder to remove them too.
