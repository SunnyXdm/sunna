# Sunna Architecture Decisions

> Recorded 2026-08-13, from design discussion. Part 6 of the research series. These are working decisions — each lists what would cause us to revisit it.

## 1. The layering rule

Three layers, one hard rule: **the UI shell never touches a video frame or a hot-path input event.**

```
┌─────────────────────────────────────────────────────────┐
│ UI shells (thin, replaceable)                           │
│   desktop: Tauri webview   mobile: React Native         │
│   browser: WebCodecs + WebTransport client              │
├─────────────────────────────────────────────────────────┤
│ Native stream surfaces (owned by the engine)            │
│   desktop: winit + wgpu window (VRR/immediate present)  │
│   Android: SurfaceView + MediaCodec direct-to-surface   │
│   iOS: VideoToolbox → AVSampleBufferDisplayLayer/Metal  │
├─────────────────────────────────────────────────────────┤
│ Sunna engine (one Rust workspace, all platforms)        │
│   protocol · QUIC transport · capture · encode/decode   │
│   input capture/injection · audio · latency stats       │
└─────────────────────────────────────────────────────────┘
```

- **Tauri** (desktop): Rust backend composes naturally with the engine (same process, direct calls — no IPC on anything that matters). The webview renders chrome only: host list, settings, pairing. The stream window is a separate native surface created by the engine (winit/wgpu or embedded child HWND/NSView), which also owns raw input capture (Raw Input, CGEvent taps, pointer lock, scancodes). Known weak spot: WebKitGTK on Linux.
- **React Native** (mobile): shell for navigation/login/settings; the stream view is a Fabric **native component** — Android `SurfaceView` fed by low-latency MediaCodec decoding *directly to the surface* (never enters JS), iOS VideoToolbox into a display layer. Touch/gamepad handled in the native layer of that view, not the RN gesture responder. Engine bound via UniFFI/JNI.
  - Revisit if: the UI stays as thin as a host list — then SwiftUI/Compose shells over the same core are less dependency weight.
- **Browser**: WebCodecs + WebTransport client sharing the native protocol shape (the reason transport is QUIC). Never the core engine — see research/02 on WebRTC's latency tax.

## 2. Host is a service — always

The GUI is never the host; it's a control panel over local IPC to a UI-less host service. Decided day one because retrofitting session/service management into a GUI-embedded host is what makes competitors' hosting stories miserable.

Per-OS shape:

- **Windows:** SYSTEM supervisor service + per-session capture/encode worker (capture APIs require the interactive session; SYSTEM needed for secure-desktop/UAC input injection, login-screen access, unattended updates). Same structure as Parsec/Sunshine — proven.
- **Linux:** systemd unit; KMS capture works fully headless with no display attached. Our differentiator platform (research/04 gap #2).
- **macOS:** launchd agent in the user session (TCC Screen Recording/Accessibility grants are per-user and interactive — pure headless Mac hosting fights the OS; don't promise Linux-grade headless on Mac in v1). Privileged helper for login-window access is a later project.

## 3. Distribution artifacts (one workspace, three packagings)

1. **`sunna`** — desktop app: client + host controls in one download (the Parsec UX). **Hosting off by default, lazily provisioned**: only when the user enables "Allow connections" does it elevate once to register the service, install virtual-display/gamepad drivers, open firewall rules. Client-only users never carry drivers or a background service.
2. **`sunnad`** — headless host: same host crates, no GUI/webview. systemd unit / silent MSI / launchd agent; config file + CLI + optional localhost web-admin. Single static binary. Serves homelab/VM/cloud, enterprise fleet installs, and later a Wolf-style container image.
3. **Client-only builds** — handhelds, mobile, browser.

Anti-decision: no Sunshine-style two-product install as the *default* consumer experience (research/04: setup friction is the #1 reason casual users stay on Parsec). One symmetric app for humans, one daemon for machines, one engine underneath.

## 4. Workspace map

| Crate | Path | Contents | Status |
|---|---|---|---|
| `sunna-proto` | `crates/proto` | wire messages (control, input incl. gesture channel), media packetization/reassembly, latency stats | scaffolded |
| `sunna-transport` | `crates/transport` | QUIC via quinn: datagrams for media, one reliable control stream; dev TLS | scaffolded |
| `sunna-capture` | `crates/capture` | `FrameSource` trait; synthetic test source; platform backends (DDA/WGC, ScreenCaptureKit, KMS/PipeWire) | trait + synthetic; platforms stubbed |
| `sunna-codec` | `crates/codec` | `Encoder`/`Decoder` traits; passthrough (raw) codec; NVENC/AMF/QSV/VideoToolbox FFI backends | passthrough + **VideoToolbox low-latency H.264 (macOS, done)**; others stubbed |
| `sunna-input` | `crates/input` | capture (client) + injection (host) traits; platform backends | trait + log injector; platforms stubbed |
| `sunna-host` | `crates/host` | host pipeline: source → encode → packetize → send; control/input handling | scaffolded (single session) |
| `sunna-client` | `crates/client` | receive → reassemble → decode → stats; render surface later (winit/wgpu) | scaffolded (headless) |
| `sunna-audio` | — | WASAPI/SCK/PipeWire capture + low-delay Opus | planned (M1) |
| `sunnad` | `apps/sunnad` | headless host daemon | scaffolded (synthetic source) |
| `sunna-cli` | `apps/sunna-cli` | dev client: `connect`, `bench` (in-process loopback latency) | scaffolded |
| `sunna` (Tauri app) | `apps/sunna` | desktop shell | planned (after real capture/decode) |

## 5. Milestones (revises research/00 §5 with dev-machine reality)

- **M0a — synthetic end-to-end (this scaffold):** synthetic frame source → passthrough codec → QUIC datagrams → reassembly → per-stage latency stats. Runs on any OS today; `sunna-cli bench` is the day-one latency instrumentation.
- **M0b — real pixels on macOS** (dev machine is a Mac): ~~VideoToolbox low-latency H.264 encode/decode~~ (done 2026-08-13: hand-rolled FFI, Annex B wire format, hardware-tested ~3 ms encode / ~3 ms decode) → ScreenCaptureKit capture → wgpu render window. Capture and window need an interactive session: Screen Recording permission must be granted to the terminal/app when we first run it. Then: first real measurement vs Parsec/Moonlight on the same LAN.
- **M0c — Windows host:** DDA → NVENC ultra-low-latency (research/03 §2 settings). The primary product target.
- **M1 — reliability layer:** FEC + NACK + LTR/reference invalidation, encoder-coupled congestion control, Opus audio, full input (relative mouse, scancodes, clipboard).
- **M2 — connectivity:** rendezvous + hole punching + DERP-style relay, PIN/link pairing without accounts, all self-hostable.
- **M3 — differentiators:** Linux host (KMS headless-first), virtual display driver, HDR/AV1/4:4:4, multi-guest co-play, gesture channel (research/05).
- **M4 — reach:** browser client, macOS host parity, handheld UX, Tauri/RN shells.

## 6. Tech choices and their rationale

| Choice | Why | Revisit if |
|---|---|---|
| Rust core, C FFI to vendor encoder SDKs | memory safety on a network-facing service; ecosystem (quinn, wgpu, RustDesk precedent) | an encoder SDK binding proves too painful — isolate in a C++ shim |
| QUIC (quinn) over custom-UDP-from-scratch | crypto, connection migration (Wi-Fi→5G stream survival), datagrams (RFC 9221), WebTransport path to browsers — while we still own CC + FEC on top | userspace QUIC CPU cost at 100+ Mbps measures badly → custom UDP with the same frame format |
| postcard for control messages, hand-packed 21-byte header for media datagrams | compact, zero-fuss serde; media path stays allocation-light | protocol goes public/versioned → revisit framing |
| Timestamps = `SystemTime` unix micros | valid same-machine comparisons now (bench) | cross-machine latency work starts → in-protocol NTP-style clock sync (M1) |
| render-on-arrival, no jitter buffer | research/03 §4 — the entire genre's winning pattern | never (optional pacing buffer as user opt-in, like Moonlight) |
| tracing for logs/metrics | structured, cheap, spans map to pipeline stages | — |

## 7. Security posture of the scaffold (do not ship)

Dev TLS only: server generates a self-signed cert per run; `sunna-cli connect` uses **insecure skip-verify**. `sunnad` binds `127.0.0.1` by default. Before anything real: pairing (PIN/QR → pinned peer certs), authenticated sessions, rate limits. Tracked for M2.

## 8. Open questions

- **License/monetization** — evidence (research/04 §6) favors open-core; undecided.
- **Moonlight protocol compat mode** — instant client ecosystem vs protocol freedom; possible as a side-door later (Wolf/Apollo precedent).
- **Positioning** — gaming-first vs creative-workstation-first; affects M3 ordering (co-play vs 4:4:4/tablet).
- **Gesture channel v1 scope** — semantic-only vs raw contacts on the wire from day one (research/05 recommends carrying raw contacts in the format even if unused).
