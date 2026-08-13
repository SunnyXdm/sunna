# Sunna

A low-latency game-streaming / remote-desktop system. Goal: faster where it counts (p99 on real networks), lighter, and more modern than Parsec and Moonlight — see [research/00-overview.md](research/00-overview.md) for the full research series and [research/06-architecture.md](research/06-architecture.md) for the architecture decisions and milestone plan.

## Status

**Milestone 0a done; 0b codec done; reliability layer v0 done.** The end-to-end pipeline runs: synthetic frame source → codec → QUIC datagrams (media) + reliable stream (control/input) → FEC reassembly → keyframe-gated decode → latency stats. On macOS the codec is real hardware **VideoToolbox low-latency H.264** (Annex B on the wire, SPS/PPS on keyframes, infinite GOP); elsewhere it falls back to raw passthrough.

Reliability v0 (`--simulate-loss` exercises it): per-frame XOR parity FEC (1 per 8 chunks, ~12.5% overhead) recovers single losses per group with zero feedback delay; frame-continuity tracking catches wholly-lost frames; a lost frame triggers a keyframe request and P-frames are skipped until the IDR arrives (no corrupt frames ever decode); per-second receiver reports drive AIMD bitrate adaptation applied to the *next* encoded frame — the seed of the encoder-coupled congestion controller. Ping/Pong estimates host↔client clock offset at min-RTT so cross-machine latency numbers are meaningful.

Loopback bench: 60 fps 0 drops clean; at 20% simulated loss still 0 drops (all FEC-recovered); at 35% the keyframe recovery path holds the stream together. Capture→decode ~5-7 ms p50 including hardware encode+decode. Still to come in 0b: ScreenCaptureKit capture and a wgpu render window (both need an interactive GUI session / Screen Recording permission).

## Quickstart

```sh
# In-process loopback benchmark (server + client in one process):
cargo run --release -p sunna-cli -- bench --seconds 5

# Or run the daemon and connect to it:
cargo run --release -p sunnad
cargo run --release -p sunna-cli -- connect 127.0.0.1:48800
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
