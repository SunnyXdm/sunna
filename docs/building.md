# Building from source

For working on Sunna itself. To just install it, follow the guides in the [README](../README.md): prebuilt downloads need no toolchain after the first public release. The source install scripts remain for development.

Everything is one Rust workspace:

```sh
cargo build --release              # sunnad (the host) and sunna-cli (a developer client)
cargo build --release -p sunna     # the desktop app; on Linux this needs WebKitGTK, like any Tauri app
cargo test                         # add --workspace to include the app
```

The Android app builds with Gradle, which builds its Rust library through cargo-ndk: see [apps/android/README.md](../apps/android/README.md).

## Layout

| Path | |
|---|---|
| `apps/sunna` | the desktop app (Tauri): the Computers window in `ui/`, and `sunna viewer`, the native session window |
| `apps/sunnad` | the host |
| `apps/sunna-cli` | developer client: `view`, `connect` (headless; `--click` and `--type` test a host's input), `bench` (loopback benchmark) |
| `apps/android` | the Android app (Kotlin, no other libraries) |
| `crates/android` | the Android app's native side: sessions, MediaCodec decoding, AAudio sound |
| `crates/transport` | QUIC (quinn): media datagrams, the control stream, TLS |
| `crates/proto` | wire messages, packetizing and reassembly, fast-lane tiles |
| `crates/capture` | screen capture: macOS (CGDisplayStream), X11 (MIT-SHM); fast-lane tile detection |
| `crates/portal` | Wayland: capture and input through the desktop portal and PipeWire ([design](../research/10-gnome-wayland.md)) |
| `crates/codec` | encoders and decoders: VideoToolbox, NVENC, OpenH264, FFmpeg, MediaCodec |
| `crates/host`, `crates/client` | the host and client pipelines |
| `crates/viewer` | the session window: presentation, keyboard capture, session menu |
| `crates/input`, `crates/clipboard`, `crates/audio` | input injection, clipboard sync, sound |
| `crates/telemetry` | logging, and shipping logs to a development collector |
| `scripts/` | installers and packaging, `sunna-host`, `run.sh` (the Mac host and the developer loop) |
| `tools/` | the log collector (`logd`), `gnome-test/` (GNOME on Wayland in a container, for testing the portal path), and proof-of-concept scripts |
| `docs/` | these guides, and the website |
| `research/` | design notes; start at [`00-overview.md`](../research/00-overview.md) |

## Working on the Computers window

It can run in a browser with a stand-in for the Rust side: `python3 -m http.server 8766 -d apps/sunna`, then open `localhost:8766/ui/` (see `apps/sunna/dev/`).

## Releases and updater proofs

The workspace version is shared by all Cargo packages. `scripts/release.sh X.Y.Z`
requires a clean tree, updates Cargo/Tauri/Android versions, commits those version
fields and creates a **local** tag. It does not push. Do not run it while another
person or agent has uncommitted work in this checkout.

`.github/workflows/release.yml` calls the scripts in `scripts/release/`. Linux
builds use an Ubuntu 22.04 container and bundle a minimal dynamically linked LGPL
FFmpeg. Its source archive and license accompany the libraries. WebKitGTK is a
system dependency. `manifest.py` hashes each artifact and signs the exact manifest
bytes with Ed25519; `sunna-update` verifies those bytes before parsing them.

`keygen.sh` creates the release key once at `~/.config/sunna/release.key` with
mode 0600, and writes only the public half to `crates/update/release.pub`.
Do not print, commit or move the private key into the repository. Test releases
use a separate temporary key and the `sunna-update/test-key` feature, which
requires a raw public-key path in `SUNNA_TEST_PUBLIC_KEY` at compile time.
Production binaries have no runtime key override.

The installer is the trust bootstrap: download it over HTTPS or inspect it.
The generated script pins the helper's SHA-256; that helper verifies the signed
manifest. Changing `SUNNA_UPDATE_URL` never changes the trusted public key.

The updater exposes a local-only `adopt` command for the source installers.
It does not claim a source build is signed. Network and remote update paths
always require the signed feed.

See [update verification](update-verification.md) for exact local proof commands,
platform limitations, and the steps required before publishing. Windows packaging
is deliberately excluded from CI until the separate host/driver work is committed
and tested. No driver is installed or updated by the self-updater.

## Build the website and guides

From the repository root, run the following with Node.js, npm and ImageMagick 7 installed.

```sh
npm ci --prefix scripts/site
node scripts/site/build.mjs
```

Serve `dist/site/` at the domain root. The build renders the Markdown guides as HTML under `/guides/` and generates ThumbHash previews from the screenshots in `docs/images/`. On the Pangolin deployment, Caddy reads the homepage, stylesheet and images from `docs/`. Copy the built `guides/` directory into the web server’s public directory after editing the Markdown or screenshots.
