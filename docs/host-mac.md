# Share a Mac

Run the host on a Mac you want to use from another device. For now it runs in a Terminal window: it shares while the window is open and the Mac is awake. An installable Mac host is planned.

[← Back to the README](../README.md)

## You need

- macOS 13 or later
- the Xcode command line tools: `xcode-select --install`
- [Rust](https://rustup.rs)
- [Tailscale](https://tailscale.com), connected: the Mac host listens only on its Tailscale address

## Start sharing

```sh
# Once: make this Mac's key (leaves an existing one alone).
grep -qs '^SUNNA_TOKEN=' ~/.sunna/sunna.env || (mkdir -p ~/.sunna && umask 077 && echo "SUNNA_TOKEN=$(openssl rand -hex 16)" >> ~/.sunna/sunna.env)

git clone https://github.com/SunnyXdm/sunna.git && cd sunna
scripts/run.sh host
```

It builds the host (the first time takes a few minutes), then prints the address, key, link and a QR code. Add the Mac in the app with the link, or scan the code with your phone. It shares until you press Ctrl-C or close the window.

**The first time,** macOS asks for **Screen Recording** and **Accessibility** for your terminal app (Terminal, iTerm…). Allow both in System Settings → Privacy & Security, quit the terminal completely, and run `scripts/run.sh host` again. Screen Recording is for the picture and the sound; Accessibility is for the mouse and keyboard.

## Good to know

- **The key** is the same one the Sunna app on this Mac shows in Settings as this computer's key. Its Settings also show the link and its QR code.
- **Sound** is shared too, through the same Screen Recording permission. It's new and not yet tested on a real Mac: if a session has no sound, the Terminal window says why.
- **Updating:** `git pull`, then `scripts/run.sh host` again.
- **On Wi-Fi, the video hitches about once a second?** That's AirDrop's radio (AWDL). `SUNNA_KEEP_AWDL_DOWN=1 scripts/run.sh host` holds it off for the session (it asks for your password once).
