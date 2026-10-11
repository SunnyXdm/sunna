# Building from source

For working on Sunna itself. To just install it, follow the guides in the [README](../README.md): the install scripts build everything for you.

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
