# Sunna

A low-latency game-streaming / remote-desktop system. Goal: faster where it counts (p99 on real networks), lighter, and more modern than Parsec and Moonlight — see [research/00-overview.md](research/00-overview.md) for the full research series, [research/06-architecture.md](research/06-architecture.md) for the architecture decisions, and [research/08-architecture-plan.md](research/08-architecture-plan.md) for the full architecture plan (sources in [research/sources/](research/sources/)); [research/07-v1-plan.md](research/07-v1-plan.md) is the earlier near-term plan it amends.

## Status

**Milestone 0a done; 0b codec done; reliability layer v0 done.** The end-to-end pipeline runs: synthetic frame source → codec → QUIC datagrams (media) + reliable stream (control/input) → FEC reassembly → keyframe-gated decode → latency stats. On macOS the codec is real hardware **VideoToolbox low-latency H.264** (Annex B on the wire, SPS/PPS on keyframes, infinite GOP); elsewhere it falls back to raw passthrough.

Reliability v0 (`--simulate-loss` exercises it): per-frame XOR parity FEC (1 per 8 chunks, ~12.5% overhead) recovers single losses per group with zero feedback delay; frame-continuity tracking catches wholly-lost frames; a lost frame triggers a keyframe request and P-frames are skipped until the IDR arrives (no corrupt frames ever decode); per-second receiver reports drive AIMD bitrate adaptation applied to the *next* encoded frame — the seed of the encoder-coupled congestion controller. Ping/Pong estimates host↔client clock offset at min-RTT so cross-machine latency numbers are meaningful.

Loopback bench: 60 fps 0 drops clean; at 20% simulated loss still 0 drops (all FEC-recovered); at 35% the keyframe recovery path holds the stream together. Capture→decode ~5-7 ms p50 including hardware encode+decode.

**Testable end-to-end on macOS**: `sunnad --source screen` captures the real display (CGDisplayStream; SCK backend later), `sunna-cli view` opens a viewer window (winit + softbuffer CPU blit; wgpu presenter later) and forwards mouse/scroll/keyboard, injected host-side via CGEventPost. Both directions of permission apply: Screen Recording for the host's capture, Accessibility for the host's input injection.

## Try it (macOS)

**Stream your screen and control it from a window:**

```sh
cargo build --release

# Host: stream the main display (macOS prompts once for Screen Recording
# permission for your terminal; grant it and run again). For remote input to
# work, also grant Accessibility permission to the terminal.
./target/release/sunnad --source screen

# Client (same machine, or another Mac on the LAN with --listen 0.0.0.0:48800
# on the host): opens a viewer window; mouse, scroll and keyboard are
# forwarded to the host while the window is focused.
./target/release/sunna-cli view 127.0.0.1:48800
```

Same-machine viewing is a hall-of-mirrors (you're seeing your own screen) — it's still the fastest way to sanity-check latency and input. The real test is two machines on one LAN.

**Headless checks (no permissions needed):**

```sh
# In-process loopback benchmark (synthetic source, hardware H.264 on macOS):
cargo run --release -p sunna-cli -- bench --seconds 5

# With simulated packet loss to watch FEC + keyframe recovery work:
cargo run --release -p sunna-cli -- bench --seconds 5 --simulate-loss 0.2

# Synthetic daemon + headless client:
cargo run --release -p sunnad
cargo run --release -p sunna-cli -- connect 127.0.0.1:48800 --seconds 5
```

`sunnad` binds 127.0.0.1 by default. There is **no authentication yet** (dev TLS is self-signed + skip-verify) — do not expose it beyond localhost/LAN you trust. Pairing/auth lands in Milestone 2.

## Layout

| Path | Crate | Purpose |
|---|---|---|
| `crates/proto` | `sunna-proto` | wire messages (control + input incl. gestures), media packetization/reassembly, stats |
| `crates/transport` | `sunna-transport` | QUIC (quinn): unreliable datagrams for media, reliable control stream, dev TLS |
| `crates/capture` | `sunna-capture` | `FrameSource` trait, synthetic source; platform capture backends (stubs) |
| `crates/codec` | `sunna-codec` | `Encoder`/`Decoder` traits, passthrough codec; hardware backends (stubs) |
| `crates/input` | `sunna-input` | input injection/capture traits; platform backends (stubs) |
| `crates/host` | `sunna-host` | host pipeline: source → encode → packetize → send; control handling |
| `crates/client` | `sunna-client` | client pipeline: receive → reassemble → decode → stats (render window later) |
| `apps/sunnad` | `sunnad` | headless host daemon |
| `apps/sunna-cli` | `sunna-cli` | dev client: `connect`, `bench` |
| `research/` | — | the research series (start at `00-overview.md`) |

## Design rules (the short version)

- Never wait on a clock: every stage is event-driven off the previous stage's completion.
- Zero receive-side buffering: render-on-arrival, latest-frame-wins.
- The UI shell never touches a video frame or hot-path input event.
- Measure everything: per-stage timestamps ride the pipeline from capture to present.
