# The Sunna app on a Mac

The app lists your computers with a live preview of each one and opens sessions in their own window, full screen by default.

[← Back to the README](../README.md)

## Install

You need macOS 13 or later (tested on Apple silicon), the Xcode command line tools (`xcode-select --install`) and [Rust](https://rustup.rs).

```sh
git clone https://github.com/SunnyXdm/sunna.git && cd sunna
scripts/install-mac-app.sh
```

This builds **Sunna.app**, installs it in `/Applications` (or `~/Applications`) and opens it. To update: `git pull` and run the script again.

**The first time,** the script makes a code-signing certificate for Sunna in your login keychain, and macOS asks for your password once to trust it. If macOS then asks whether `codesign` may use its key, choose **Always Allow**. Why: macOS remembers permissions by an app's signature, so with this certificate the permissions you give Sunna stay through every update. No Apple developer account is involved.

## One permission

Sunna needs **Accessibility** for one thing: sending ⌘Tab, ⌘Space and other system shortcuts to the other computer. Without it, those stay on your Mac and everything else works. Sunna's Settings → **System shortcuts** shows whether it's allowed, and **Allow…** asks for it.

If System Settings shows Sunna as allowed but Sunna says it isn't, that entry is from an older build: remove it with **Reset** there, then allow Sunna again.

## Next

- [Using Sunna](using.md): adding computers, connecting, the session menu and shortcuts.
- This Mac can be shared too: see [Share a Mac](host-mac.md).

## Give it to someone else

`scripts/install-mac-app.sh --dmg` also makes `dist/Sunna.dmg`. It's built for your Mac's chip and signed with your own certificate, which other Macs don't know, so the first time macOS blocks it: open it, then click **Open Anyway** in System Settings → Privacy & Security (on macOS 14 and earlier, right-click the app and choose **Open**).

## Remove it

Quit Sunna and move it to the Trash. Your computers and their keys are in `~/.sunna`; delete that folder to remove them too.
