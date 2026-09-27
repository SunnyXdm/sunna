<p align="center"><img src="docs/images/icon.png" width="128" height="128" alt="Sunna"></p>

<h1 align="center">Sunna</h1>

<p align="center">Your other computers, on your Mac. A fast, native remote desktop for Mac and Linux, over your own network.</p>

<p align="center"><img src="docs/images/app.png" alt="The Sunna app, showing five computers" width="880"></p>

Sunna shows another computer's screen on your Mac and sends your keyboard, mouse, trackpad and clipboard back. It uses the hardware video encoders and decoders where they exist and sends small changes like typing as exact, lossless tiles ahead of the video, so text appears as fast as your network allows. There are no accounts and no cloud service: computers talk to each other directly, ideally over [Tailscale](https://tailscale.com).

**Status: early, and in daily use by its developer.** It works well Mac → Mac and Mac → Linux (X11). Not yet: Windows, viewing from Linux (dev tool only), and sharing a Wayland desktop (see [Linux](#share-a-linux-computer)). Read [Security](#security-and-privacy) before using it outside your own network.

## Quick start

1. **Share a computer** (the *host*). On Linux: install the host and run `sunna-host setup` ([details](#share-a-linux-computer)). On a Mac: see [Share a Mac](#share-a-mac). Either way you get an address, a key and a link:

   ```
       Address   100.101.102.103
       Key       7f3a…

     In the Sunna app, choose Add a Computer and paste:
       sunna://100.101.102.103?key=7f3a…
   ```

2. **Install the Sunna app on your Mac** ([details](#install-the-mac-app)):

   ```sh
   git clone https://github.com/SunnyXdm/sunna.git && cd sunna
   scripts/install-mac-app.sh
   ```

3. **Open Sunna, choose Add a Computer and paste the link.** The computer shows up with its screen lit when it can be reached; click it to connect.

## Install the Mac app

`scripts/install-mac-app.sh` builds **Sunna.app**, installs it in `/Applications` and opens it. Run it again to update. You need:

- macOS 13 or later (so far tested on Apple silicon)
- the Xcode command line tools: `xcode-select --install`
- Rust: [rustup.rs](https://rustup.rs)

`scripts/install-mac-app.sh --dmg` also makes `dist/Sunna.dmg` to give to someone else. The app is signed ad hoc (there's no Apple developer account behind it), so on another Mac they need to right-click it and choose **Open** the first time.

The first time you send ⌘Tab or other system shortcuts to a remote computer, macOS asks to give Sunna **Accessibility** access (System Settings → Privacy & Security). That's only for capturing those shortcuts; everything else works without it.

## Share a Linux computer

The Linux host is `sunnad` plus `sunna-host`, a small command that sets it up as a service: it starts by itself (at login, or at boot on machines without a screen), restarts if it crashes, and logs to the system journal.

There are no prebuilt downloads yet, so you build it on the computer itself; it takes a few minutes. You need Rust ([rustup.rs](https://rustup.rs)), a C/C++ compiler and, on x86-64, nasm, which the video encoder needs to be fast (`sudo apt install build-essential nasm` on Debian and Ubuntu). The scripts tell you if one is missing.

**Any distribution** (Ubuntu, Debian, Arch, Fedora…), as the user whose desktop to share:

```sh
git clone https://github.com/SunnyXdm/sunna.git && cd sunna
scripts/install-host-linux.sh
```

This installs into `~/.local/bin` for your user, with no root needed, and runs `sunna-host setup`. Run it again to update.

**As a package, on Debian and Ubuntu** (22.04+ and Debian 12+), for example to install it on several machines:

```sh
scripts/package-linux-deb.sh            # makes dist/sunna-host_<version>_<arch>.deb
sudo apt install ./dist/sunna-host_*.deb
sunna-host setup                        # as the user whose desktop to share
```

### Which desktop gets shared

| Mode | What you see | Use it for |
|---|---|---|
| `desktop` | the X11 desktop you're logged into | a PC or laptop you also use directly |
| `virtual` | a separate desktop that runs without a screen (Xvfb with Plasma or XFCE) | servers and VMs, and machines whose own desktop is Wayland |

`sunna-host setup` picks for you: `desktop` if you run it from an X11 session, otherwise `virtual`. Choose explicitly with `--desktop` or `--virtual`.

**Wayland:** Sunna can't capture a Wayland desktop yet; that includes Ubuntu's default session, and GNOME, Plasma or Hyprland on Wayland. Either log in with an X11 session (on Ubuntu, pick "Ubuntu on Xorg" on the login screen) or use a virtual desktop. Wayland support is planned.

**A virtual desktop keeps running** after you disconnect, with any apps you opened in it, like a real computer would. `sunna-host desktop-stop` stops it and everything in it.

### Commands

| Command | |
|---|---|
| `sunna-host setup` | choose how to share, then start. Options: `--desktop`, `--virtual`, `--listen tailscale\|lan\|IP[:PORT]`, `--name NAME`, `--key KEY`, `--size WxH` |
| `sunna-host link` | show the address, key and link again |
| `sunna-host status` | running? listening where? is someone connected? |
| `sunna-host start`, `stop`, `restart` | |
| `sunna-host logs` | follow the log |
| `sunna-host new-key` | make a new key (then edit the computer in the app and paste it) |
| `sunna-host desktop-restart`, `desktop-stop` | restart or stop the virtual desktop (and its apps) |
| `sunna-host uninstall [--purge]` | remove the service (and with `--purge`, the settings and key) |

Settings live in `~/.config/sunna/host.env` (readable only by you). Edit it, then `sunna-host restart`.

**Video:** with an NVIDIA GPU (GTX 10-series or newer, driver 530+) the host encodes HEVC on the GPU. Otherwise it encodes H.264 on the CPU, which is fine for desktop work on a modern CPU; the session menu's **30 fps** option halves the load on slower machines.

## Share a Mac

For now a Mac host runs from a Terminal window:

```sh
git clone https://github.com/SunnyXdm/sunna.git && cd sunna
scripts/dogfood.sh host
```

It needs a `~/.sunna/dogfood.env` with the key (`SUNNA_TOKEN=<a long random string>`), listens on the Mac's Tailscale address, and prints the address, key and link. The first time, macOS asks for **Screen Recording** and **Accessibility** for your terminal app: allow both, quit the terminal, and run it again. An installable Mac host (a menu in the app to share this Mac) is planned.

## Using Sunna

![Adding a computer](docs/images/add.png)

- **Add a computer** with **+** (⌘N): type its address (an IP, a name, or `name.tailnet.ts.net`) and key, or paste its `sunna://` link. The preview lights up as soon as the computer answers. **Find on Tailscale** lists the computers on your tailnet that are sharing.
- **Connect** by clicking a computer whose screen is lit. Its screen grows to fill the window and the session opens in its own full-screen window.
- **Right-click** a computer (or use its ••• button) to edit it, copy its address or remove it.
- The app never scans your network by itself: it checks only the computers you've added, every few seconds while it's open.

### During a session

Open the session menu with the **•••** button in the corner or **⌃⌥M**:

| | |
|---|---|
| Keyboard | send ⌘ shortcuts to the remote, and **Send Keys**: shortcuts for the remote's OS (⌘Tab, Spotlight, Force Quit, Lock Screen on a Mac; Alt+Tab, Super, Terminal, Lock Screen on Linux) |
| Clipboard | turn clipboard sharing on or off; **Type Clipboard Text** types it out key by key, for login screens and password prompts that won't take a paste |
| Video | codec (HEVC, H.264), resolution, frame rate, bitrate limit, and the fast lane |

| Shortcut | |
|---|---|
| ⌃⌥M | session menu |
| ⌃⌥G | give ⌘Tab and other shortcuts back to this Mac (or send them to the remote again) |
| ⌃⌥F | full screen or window |
| ⌃⌥S | stats bar (latency, frame rate, bitrate, codec) |
| ⌃⌥Q | disconnect |

**The fast lane** sends small screen changes (a typed letter, a blinking cursor) as lossless tiles on their own stream, ahead of the video frame that will also contain them. On a host that encodes on the CPU, typed text arrives in about 2 ms plus the network; the video took 35–37 ms in the same test (a 6-core VM, viewer on the same machine). It's on by default for Linux hosts encoding on the CPU; turn it on or off in the session menu under Video.

## Security and privacy

- **Keys.** A host only accepts viewers that present its key. Keep keys secret; anyone with a computer's address and key can control it. The app stores the keys you add in `~/.sunna/machines.json`, readable only by you.
- **Encryption.** Connections use QUIC, encrypted with TLS 1.3. The app does **not yet verify the host's identity** (hosts use self-signed certificates), so on a network you don't control, someone in the middle could impersonate a host. Use Sunna over **Tailscale** (which authenticates both ends with WireGuard) or a network you trust, and never expose a host to the internet. `sunna-host` listens only on Tailscale when Tailscale is up. Pinning each host's certificate is planned.
- **No cloud, no accounts.** Computers connect to each other directly. Nothing is sent anywhere else.
- **Logs stay local.** The host logs to the system journal and the app to its terminal output. Keystrokes and clipboard contents are never logged. (Development builds can ship logs to a collector you run, only if `SUNNA_LOG_URL` is set.)
- **Idle cost.** A host with nobody connected uses no CPU and holds one listening UDP port (48800 by default).

## Requirements

| | |
|---|---|
| The app | macOS 13 or later |
| A Mac host | macOS 13 or later; Screen Recording and Accessibility permission |
| A Linux host | x86-64 (ARM64 untested); an X11 desktop, or Xvfb with Plasma or XFCE for a virtual one; glibc 2.34+ (Ubuntu 22.04+, Debian 12+, any current Arch or Fedora) |
| Network | the host's UDP port 48800 reachable from the viewer; [Tailscale](https://tailscale.com) recommended |

## Building from source

Everything is one Rust workspace:

```sh
cargo build --release          # sunnad (host), sunna-cli (dev client), sunna (the app)
cargo test --workspace
```

| Path | |
|---|---|
| `apps/sunna` | the Mac app (Tauri): the launcher UI in `ui/`, and `sunna viewer`, the native session window |
| `apps/sunnad` | the host daemon |
| `apps/sunna-cli` | developer client: `view`, `connect` (headless), `bench` (loopback benchmark) |
| `crates/transport` | QUIC (quinn): media datagrams, reliable control stream, TLS |
| `crates/proto` | wire messages, packetizing and reassembly, fast-lane tiles |
| `crates/capture` | screen capture: macOS (CGDisplayStream), X11 (MIT-SHM); fast-lane tile detection |
| `crates/codec` | encoders and decoders: VideoToolbox, NVENC, OpenH264 |
| `crates/host`, `crates/client` | the host and client pipelines |
| `crates/viewer` | the session window: presentation, keyboard capture, session menu |
| `crates/input`, `crates/clipboard` | input injection and clipboard sync |
| `scripts/` | installers and packaging (`install-mac-app.sh`, `install-host-linux.sh`, `package-linux-deb.sh`, `sunna-host`), and the developer loop (`dogfood.sh`) |
| `research/` | the research and architecture notes behind the design; start at [`00-overview.md`](research/00-overview.md) |

The launcher UI can be worked on in a browser with a stand-in for the Rust side: `python3 -m http.server 8766 -d apps/sunna` and open `localhost:8766/ui/` (see `apps/sunna/dev/`).

## License

No license has been chosen yet; until one is, all rights are reserved.
