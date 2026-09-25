# Sunna Architecture Proposal (Fable)

> Phase 2 deliverable, written 2026-09-25 by Fable. Inputs: the three phase-1 dossiers (`dossier-astra.md` = game-streaming engines, `dossier-fable.md` = RustDesk/Tailscale/iroh/Parsec/ease-of-use/security/distribution, `dossier-claude.md` = RDP graphics, gnome-remote-desktop, macOS/Linux/Windows platform internals, Apple Screen Sharing HP), the Sunna repo (`README.md`, `research/00..07`, `crates/*`, `apps/*` at HEAD 01ac41c plus the uncommitted working tree), and small targeted checks of the cloned repos. Citations use `[A §x]`, `[F §x]`, `[C §x]` for the Astra, Fable and Claude dossiers, `research/NN` for the repo's research series, and `path:line` for code. Anything I could not verify against a primary source is marked **(UNVERIFIED)**. Where I disagree with a dossier or with `research/07-v1-plan.md` I say so explicitly and give the reason.
>
> This document makes decisions. Alternatives are recorded in §14 (decision log), not left open in the body.

## Contents

1. Product definition
2. System overview
3. Host engine per OS (macOS, Linux, Windows)
4. Clients per OS (+ browser, + mobile/tablet stance)
5. Media transport
6. Quality system
7. Connectivity
8. Security and trust
9. Ease of use
10. Session model
11. Observability and measurement
12. Engineering
13. Roadmap
14. Risks, open questions, decision log

---

## 1. Product definition

### 1.1 Who it is for

Ranked; the order is the build order.

1. **People who own more than one computer and want the remote one to feel local.** A developer or creative with a Mac Studio / Mac mini / Linux box at home and a MacBook on the road; a homelab owner with a Linux workstation. They want native-resolution Retina text, their trackpad to behave, their clipboard to just work, and it must work from a hotel Wi-Fi, a phone hotspot, or a friend's LAN without touching a router. This is the "remote machine feels natively installed" positioning already decided in `research/07` and the memory note, and it is where every incumbent is weakest on macOS/Linux hosts ([F §4.6], [F §5.7], [C §5.3]).
2. **Self-hosters and small teams** who refuse cloud-dependent tools (Parsec bricks without its auth servers even on a LAN, `research/04 §2`; RustDesk's public-server login requirement, [F §2.4]). They want the connectivity of Parsec with the trust model of RustDesk: hosted relays by default, self-hostable rendezvous and relay binaries, account optional.
3. **Gamers streaming their own PC/Mac** and, later, **couch co-play guests** (Parsec's last unique consumer moat, `research/04` gap #5). Same pipeline, plus gamepads, game mode, HDR. This segment is served after the first two because it needs a Windows host and a virtual-gamepad driver story ([A §1.5]: Sunshine's pinned Mac backend advertises keyboard and mouse only; [F §4.2]: Parsec's Mac host has no guest gamepads).
4. **Explicitly not a target for v1:** helpdesk/"let a stranger control my PC" support. That use case is the scam vector ([F §6.5]) and drives the ID+password UX we are rejecting. It can be added later on top of the guest-link model with the safeguards in §8.7.

### 1.2 The promise

"Your other computer, at native quality, from anywhere, in under a minute, without an account." Concretely:

- **Native quality:** text on a Retina host is pixel-identical on the client within 500 ms of going still; moving content is hardware-encoded H.264/HEVC at a bitrate the path can actually carry; the cursor never lags because the client draws it.
- **Native input:** trackpad scroll with phases and momentum, correct keyboard layout and dead keys, Cmd/Ctrl semantics, relative mouse for games, gamepads (later), trackpad gestures where the host OS can consume them (`research/05`).
- **Anywhere:** direct connection on ≥ 90 % of NAT pairs (Tailscale/iroh-class traversal, [F §3b.1]), a UDP relay for the rest so the session still runs at 60 fps, first frame in under a second because the relay carries the first packets ([F §8.1]).
- **Under a minute, no account:** host shows a code; client types it (or scans a QR); keys are pinned; the machine appears in the client's list forever after. An optional account only syncs that list.
- **Secure by default:** end-to-end encrypted even against our own relay and rendezvous (dial-by-key, [F §6.2]); unattended access is an explicit host-side choice with a visible indicator; signed builds with an independently keyed updater.
- **Open and self-hostable:** the engine, the relay and the rendezvous are one workspace; a single `sunna-relay` binary gives a team its own infrastructure.

### 1.3 Measurable targets versus each competitor

All numbers are **added** latency (remote click-to-photon minus the local click-to-photon baseline of the same viewer machine running the same test app), measured with the rig in §11.5 (photodiode + USB-HID microcontroller, ≥ 200 trials, ABAB alternation), because no other definition survives comparison ([A §9.2], [C notes 05 §5.5]). Competitor figures below are the best available evidence and are labelled with their provenance; none is a controlled Mac-to-Mac measurement, which is itself the point: Sunna publishes the first one ([C §5.3], [A §9.1]).

| Metric | Sunna target (v1, Mac→Mac) | Apple Screen Sharing HP | Parsec | Moonlight+Sunshine | RustDesk |
|---|---|---|---|---|---|
| Added click-to-photon, **wired LAN**, p50 / p99 | **≤ 12 ms / ≤ 30 ms** | no public numbers ([C §5.3]); designed for wired LAN | ~7 ms p50 claimed at 1080p60 on Windows host (vendor 1000-fps camera, `research/00 §1`) | 12.5 ms display difference in one specialized RK3588 setup ([A §9.1]); no stock Mac number | 18–30 ms LAN (vendor blog, UNVERIFIED methodology, [F §2.1]); 120 ms wired reported in #6888 |
| Added click-to-photon, **Wi-Fi LAN**, p50 / p99 | **≤ 20 ms / ≤ 45 ms**, and p99 must be ≤ HP's p50 (`research/07` target) | degrades on Wi-Fi jitter (owner's test; [C §5.3]) | — | — | — |
| Added latency, **WAN direct** (Tailscale-direct, 20–40 ms RTT), p50 / p99 | **≤ one-way delay + 15 ms / ≤ one-way delay + 40 ms** | "not even that good" over Tailscale (owner) | 25–35 ms inter-city (vendor blog, [F §2.1]) | needs Tailscale; no built-in traversal | 50–70 ms inter-city (same blog); collapses to ~1.5 s under LAN load (#10938) |
| **WAN via relay** (both sides hard NAT) | session runs at ≥ 30 fps, p50 ≤ one-way + 25 ms | not possible (no relay; UDP 5900-5902 must be reachable) | fails: error 6023/6024, "port forward or buy Enterprise HPR" ([F §4.1]) | not possible | TCP relay, 32 Mbit/s cap after 30 min ([F §1A.4]) |
| **Quality at fixed bandwidth**, 2560×1664 desktop at 10 Mbit/s | static text pixel-exact within 500 ms of stillness; scrolling text ≥ 0.95 SSIM vs host at 60 fps | HEVC 4:4:4 needs ~75 Mbit/s per 4K display ([C §5.1]); pure video, never lossless | H.264/HEVC 4:2:0 default; 4:4:4 paid (Warp) ([F §4.4]) | 4:2:0 default; 4:4:4 experimental on Intel/NVIDIA hosts only; fixed 20 % FEC ([A §1.4]) | ≈2–5 Mbit/s presets, software VP9 unless client votes ([F §1B.2]) |
| **Connection success rate** (any two machines, no manual config) | **≥ 99 %** overall; **≥ 90 % direct** (Tailscale/iroh figures, [F §3b.1]) | LAN/VPN only | ~97 % claimed, no relay ([F §4.1]) | 0 % without port-forward/VPN | high via hbbs/hbbr, but self-host "never ready" failures ([F §2.4]) |
| **Time-to-first-frame** from tapping the machine | **< 1 s** WAN (relay-first), **< 400 ms** LAN | ~2–3 s (encoder + virtual display bring-up; UNVERIFIED) | 1–2 s | 2–4 s (RTSP handshake + app launch) | 2–5 s (punch, 3/6/9 s deadlines, [F §1A.3]) |
| **Steps to first session** (both machines from nothing, different networks; counted per [F §5]) | **host 4, client 3, transcriptions 1** (a 6-character code) | 5–7 + VPN ([F §5.4]) | 7 Windows / ~11 Mac host ([F §5.1]) | 8–11 ([F §5.3]) | 9 ([F §5.2]) |
| **OS prompts on a macOS host** | 2 (Screen & System Audio Recording; Accessibility) + 1 admin prompt, on one page, polled live | 1 | 4 + restart | 2–4 | 4–5 |
| **Recovery after 2 s Wi-Fi drop** | clean image within 1 RTT + 1 frame after connectivity returns, no grey screen, session survives 60 s outages | grey screen with live cursor reported ([C §5.3]) | reconnect dialog | stream restart | stall then reconnect |
| **Resize hitch** (viewer window resize) | scale during drag, ≤ 1 visible hitch on commit, no IDR storm | encoder restart + IDR on every resize ([C §5.2]) | virtual display re-mode | — | — |

Two honest caveats. (1) On a clean wired LAN the incumbents are already efficient; Sunna's margin there comes from presentation and scheduling, not transport ([A §10.4]). (2) HP's HEVC 4:4:4 from the media engine is hard to beat on bits-per-pixel on a LAN ([C §0.3]); Sunna wins on text (lossless tiles), consistency (FEC + deadline NACK + LTR), input (datagrams, client cursor), resize, and every non-wired path.

---

## 2. System overview

### 2.1 Processes per machine

**Host machine** (three processes; the split is Apple's recommended shape and RustDesk's proven one, [C §3.4], [F §1B.9]):

| Process | Runs as | Lifetime | Responsibilities |
|---|---|---|---|
| `sunnad` (broker) | root LaunchDaemon (macOS) / systemd system service (Linux) / SYSTEM SCM service (Windows) | boot → shutdown | QUIC endpoint (one UDP socket per address family), identity keys, pairing state, rendezvous registration, NAT traversal, relay client, session admission and policy, routing a live connection to the agent that owns the console, updates, audit log. **Never touches pixels or input.** |
| `sunna-agent` | per-GUI-session LaunchAgent with `LimitLoadToSessionType = [Aqua, LoginWindow]` (macOS) / per-session worker spawned by the broker with the session's env (Linux) / worker launched into the console session with the winlogon token (Windows) | one per GUI session, `KeepAlive` | capture, virtual display, encode, static tiles, audio capture, input injection, cursor, clipboard. Talks to the broker over local IPC (XPC on macOS, Unix socket + fd passing on Linux, named pipe on Windows). |
| `Sunna.app` (shell) | user | when the user opens it | machine list, pairing UI, hosting toggle and permission wizard, session status, settings. Control panel over the broker's IPC; also the **client**: the stream window is a native surface owned by the engine inside this process. |

**Client machine**: `Sunna.app` alone (engine in-process). A headless `sunna-cli` remains for benchmarks and scripting.

**Infrastructure** (self-hostable, one binary each, also run publicly by the project):

- `sunna-rendezvous`: device registry keyed by public key, endpoint exchange, "call-me" signalling, STUN on the same UDP port. Stateless apart from an in-memory map; optional account backend behind it.
- `sunna-relay`: authenticated UDP datagram relay (DERP's role, but UDP-native so it can carry 60 fps, [F §3.8], [F §8.2]) with a TCP/443 fallback listener.

### 2.2 Crate layout

The workspace grows from today's 7 crates + 2 apps (`Cargo.toml`) to the following. Names are final; boundaries are the important part.

```
crates/
  proto        wire formats (datagram headers §5.2, control messages, tile records), versioning, capability negotiation
  media        packetizer/depacketizer, RS-FEC, NACK history, jitter-free reassembler, deadline logic, feedback encoding
  cc           congestion controller: delay/loss estimator, encoder target, pacer schedule, quinn Controller adapter
  transport    quinn endpoint setup, RPK TLS, mapped-address AsyncUdpSocket (path mux), stream scheduler, datagram sender
  netpath      candidate gathering, STUN/QAD client, disco pings, port mapping (PCP/NAT-PMP/UPnP), relay client, path selection
  rendezvous   client for sunna-rendezvous (register, lookup, call-me), DNS-SD LAN discovery
  identity     ed25519 device keys, pairing (CPace), peer store, permissions model
  capture      FrameSource trait; capture-macos (SCK), capture-linux (Mutter/KWin/portal/KMS), capture-windows (DDA/WGC)
  display      virtual display leases: CGVirtualDisplay (macOS), Mutter/KWin virtual outputs (Linux), IddCx control (Windows)
  codec        Encoder/Decoder traits; codec-vt, codec-nvenc, codec-amf, codec-vpl, codec-vaapi, codec-x264/openh264, codec-dav1d
  tiles        static-content classifier, tile hashing, lossless tile codec, client tile cache and compositor rules
  audio        capture (Core Audio taps / PipeWire / WASAPI loopback) → Opus; playout with drift control
  input        InputInjector/InputCapture traits; input-macos, input-linux (libei/uinput), input-windows (SendInput/PT_TOUCHPAD)
  cursor       host cursor observer (shape+hotspot cache), client hardware-cursor presenter
  clipboard    lazy clipboard protocol, platform pasteboards, file transfer
  host         session engine: state machines, per-viewer encoder contexts, scheduler across flows
  client       receive engine: demux, reassembly, decode thread, present hand-off, input send, overlay data
  present      Presenter trait; present-metal (CAMetalLayer), present-vulkan (Wayland/DRM), present-d3d11
  ipc          broker↔agent↔shell protocol (XPC / Unix socket / named pipe) with fd passing for surfaces where possible
  telemetry    stage timestamps, ring buffers, overlay model, JSON export, tracing integration
  sim          in-process network simulator (loss/delay/jitter/bandwidth models) for tests and CI
apps/
  sunna-macos  SwiftUI shell + Swift glue (SCK, CGVirtualDisplay, SMAppService, XPC), links the Rust engine via a C ABI
  sunna-desktop Tauri shell for Linux/Windows (later)
  sunnad       broker daemon
  sunna-agent  per-session agent (on macOS it is an LSUIElement .app bundle so TCC has a stable identity)
  sunna-relay, sunna-rendezvous
  sunna-cli    bench / connect / doctor / harness driver
  sunna-web    browser client (WebCodecs + WebTransport), later
```

### 2.3 Data flow and control flow

Host, one viewer, desktop mode:

```
 SCK dispatch queue          frame gate (1 slot,        encode thread              packetize/FEC + sender thread
 (compositor cadence)        latest wins)               (VT async, 1 in flight)    (pacer, 1 ms ticks)
 ┌──────────────┐  surface  ┌──────────────┐  surface  ┌──────────────┐  AU+meta ┌──────────────┐  QUIC datagrams
 │ SCStream cb  │──────────▶│ latest frame │──────────▶│ VTCompress   │─────────▶│ chunk→RS FEC │────────────────▶ socket
 │ dirtyRects   │           │ + dirty acc. │           │ LTR, layers  │          │ NACK history │
 │ displayTime  │           └──────┬───────┘           └──────────────┘          │ priority mux │◀── audio (Opus 10 ms)
 └──────────────┘                  │ stable regions                               │              │◀── cursor pos
                                   ▼                                              └──────▲───────┘
                            ┌──────────────┐  lossless tiles on a low-priority stream      │ retransmit / rate / IDR / LTR-refresh
                            │ tile engine  │────────────────────────────────────────────▶  │
                            └──────────────┘                                     ┌────────┴────────┐
 input datagrams ───────▶ injector thread (CGEventPost, private source)          │ control task    │◀── FEEDBACK datagrams (20 ms)
 control stream  ───────▶ control task (tokio): hello, caps, clipboard, shapes   │ cc::Controller  │
                                                                                  └─────────────────┘
```

Client:

```
 socket thread            demux                   video reassembly         decode thread         present thread
 ┌────────────┐  bytes   ┌────────────┐  VIDEO   ┌────────────────┐  AU   ┌────────────┐  tex  ┌─────────────────┐
 │ recv (GRO) │─────────▶│ by kind    │─────────▶│ RS repair,     │──────▶│ VT decode  │──────▶│ CAMetalLayer    │──▶ photons
 └────────────┘          │            │  FEC     │ NACK decision, │       │ → IOSurface│       │ tiles composite │
                         │            │─────────▶│ deadline drop  │       └────────────┘       │ hw cursor       │
                         │            │  AUDIO   └────────────────┘                            │ presentedTime   │
                         │            │─────────▶ Opus decode → ring (≤ 30 ms) → CoreAudio cb  └─────────────────┘
                         │            │  CURSOR  ─▶ NSCursor set / position
                         └────────────┘
 UI thread: NSEvent monitors → input coalescer → INPUT datagrams (every 4 ms or on discrete event)
 control stream: hello/caps/tiles/clipboard/shapes; FEEDBACK datagrams from the reassembler every 20 ms
```

Control flow summary: the **control task** on each side owns the reliable stream and all state machines (§10); the **media threads** are lock-free-ish producers/consumers with single-slot or bounded queues; the **congestion controller** lives on the host control task and publishes an atomic `target_bitrate`, `fec_ratio` and `frame_budget` that the encode and sender threads read per frame (this is the `SessionSignals` seam already in `crates/host/src/lib.rs:45-48`, kept).

### 2.4 Threading model

Host (per session):

| Thread | Priority / QoS | Wakes on | Work |
|---|---|---|---|
| capture callback (SCK dispatch queue) | `QOS_CLASS_USER_INTERACTIVE` | compositor frame | stamp `displayTime`, hold the surface, publish to the frame gate, accumulate dirty rects |
| encode | user-interactive, `NSProcessInfo.beginActivity(.latencyCritical)` | frame gate condvar | apply CC targets, choose LTR/layer flags, `VTCompressionSessionEncodeFrame` (async; output callback hands AU to sender) |
| sender/pacer | user-interactive, 1 ms timer only while a frame is being paced | AU ready, retransmit request, audio frame, pacer tick | packetize, RS parity, priority mux, `send_datagram` with GSO batches |
| tile engine | utility | stable-region timer (150 ms) | hash, encode, write tiles to the low-priority stream |
| injector | user-interactive | INPUT datagram | decode, modifier reconciliation, CGEventPost |
| audio IOProc (Core Audio) | real-time (system) | 10 ms buffer | copy to ring; Opus encode on a separate user-interactive thread |
| control (tokio) | default | stream data / FEEDBACK / timers | state machines, CC update, IPC to broker |

Client: socket thread (user-interactive), decode thread (user-interactive), present thread (user-interactive, driven by `CAMetalDisplayLink` or arrival), audio callback (system real-time), UI/main thread (input capture only; never blocks on the network).

Rule inherited from `research/06 §1` and kept: **the UI shell never touches a frame or a hot-path input event**.

### 2.5 Queue inventory (every queue, its bound, and its drop policy)

| # | Queue | Where | Bound | Policy when full | Why this bound |
|---|---|---|---|---|---|
| Q1 | SCK surface pool | host capture | `queueDepth = 3` (Apple minimum; never > 8) [C §3.1] | SCK stalls delivery | we hold ≤ 2 surfaces (gate + encoding) |
| Q2 | frame gate | capture → encode | **1** | replace (latest wins), count `gate_replaced` | discard before encode so no reference is created ([A §1.1] Sunshine, [A §5.1] Moonshine) |
| Q3 | encoder in-flight | inside VT | 1 (`MaxFrameDelayCount = 0`, low-latency spec) | encode blocks gate | one-in-one-out |
| Q4 | AU queue | encode → sender | **2 AUs or 40 ms of age**, whichever first | drop the older AU, burn its wire id, mark next frame `needs_ltr_refresh` | the sender-drop bug class from `research/07` finding 1; a burned id is a real gap |
| Q5 | pacer queue | sender | one frame's datagrams + retransmits | new frame preempts: remaining chunks of the previous frame are sent only if the frame can still meet its deadline, else dropped as a whole and id burned | never pace stale video |
| Q6 | quinn datagram send buffer | transport | `datagram_send_buffer_size = 256 KiB` (down from 1 MiB, `crates/transport/src/lib.rs:29`) | our pacer never exceeds cwnd so this should never fill; if it does, drop frame + local cut | 1 MiB is 419 ms at 20 Mbit/s ([A §7.1]) |
| Q7 | NACK history | sender | 400 ms or 4 MiB of sent datagrams | oldest evicted | beyond this a retransmit cannot meet any deadline |
| Q8 | tile stream | host | QUIC stream flow control, 1 MiB | tiles are regenerated from the current frame, never queued beyond one generation | low priority, loses to video |
| Q9 | input coalescer | client UI → sender | 1 pending motion + list of discrete events | motion replaced (relative deltas summed), discrete never dropped | [A §1.5] Moonlight/Sunshine coalescing |
| Q10 | reassembly slots | client | **3 frames** (current + 2 newer) keyed by frame_id, plus a 2-frame tolerance for reordering | oldest frame abandoned when a 4th arrives or its deadline passes | one partial (today, `media.rs:209-220`) turns reordering into loss (`research/07` finding 4) |
| Q11 | decode input | reassembler → decode thread | **2 AUs** | if a *keyframe* or LTR-anchored AU is queued behind two others, drop the older P-frames only if the newer one does not reference them (it always does) → therefore: never drop here; instead the CC reduces rate and the host's Q4 sheds | decoded frames may be dropped, encoded ones may not ([A §2.3]) |
| Q12 | decoded → present | decode → present | **1** (latest wins) plus the frame being presented | replace | render-on-arrival |
| Q13 | audio playout ring | client | 30 ms target, 60 ms max | time-stretch (Opus PLC + 2 % resampling drift), drop to target if > 60 ms | Moonlight's 30/50 ms thresholds ([A §2.5]); RustDesk's 3 s is the anti-pattern ([F §1B.5]) |
| Q14 | control stream | both | QUIC flow control | back-pressure only; no realtime data rides here | |
| Q15 | relay per-client queue | sunna-relay | 64 datagrams or 20 ms | drop oldest | DERP's 32-packet queue is sized for keepalives ([F §3.8]); ours must hold ~1 frame |

Every queue exports `depth`, `replaced`, `dropped` counters to telemetry (§11); a queue without a counter is a bug.

---

## 3. Host engine per OS

Order of investment: macOS first (flagship, owner's hardware), Linux second (differentiator platform, `research/04` gap #2), Windows third (gaming reach). The engine is one Rust workspace; per-OS code lives behind the `FrameSource`, `Encoder`, `Display`, `InputInjector`, `AudioSource`, `Clipboard` traits already sketched in `crates/capture/src/lib.rs:84-89`, `crates/codec/src/lib.rs:57-66`, `crates/input/src/lib.rs:16-21`.

### 3.1 macOS host

#### 3.1.1 Capture: ScreenCaptureKit, not CGDisplayStream

CGDisplayStream (`crates/capture/src/macos.rs`) is replaced. It is obsoleted in the macOS 15 SDK, delivers **no frames for CGVirtualDisplay displays on 15+**, hangs when two same-named processes use it, and is one of the "deprecated capture technologies" that trigger the Sequoia re-prompt ([C §3.1], [C A5]). The virtual-display plan (§3.1.4) is impossible without SCK. Minimum macOS for the host becomes **14.4** (login-window SCK fix, [C §3.4]); 13.x gets a client only.

`SCStreamConfiguration` (values chosen per [C §3.1] and `research/07` decisions table):

| Property | Value | Reason |
|---|---|---|
| `width/height` | backing pixels of the captured (virtual) display; `scalesToFit = false`; `captureResolution = .best` | 1:1 Retina; no resampling anywhere ([C §6.2]) |
| `pixelFormat` | `BGRA` (`kCVPixelFormatType_32BGRA`) in desktop mode; `420v` in game mode; `64RGBAHalf`/`xf44` in HDR game mode (15+, probe) | BGRA feeds lossless tiles and any 4:4:4 path; VT's BGRA→YUV is hardware ([C §3.1] implication 1); NV12 in game mode saves the conversion when no tiles are produced. **Measure both** on the Airs (task in §13 phase 0). |
| `queueDepth` | 3 | Apple minimum; we never hold more than 2 |
| `minimumFrameInterval` | `1/60` desktop, `1/120` when the client display is ProMotion and game mode is on | caps rate, SCK is damage-driven anyway |
| `showsCursor` | `false` | client-side cursor (§3.1.6) |
| `colorSpaceName` / `colorMatrix` | the display's (`kCGColorSpaceDisplayP3` on Apple panels) and `kCVImageBufferYCbCrMatrix_ITU_R_709_2`; the same values are written into the bitstream VUI | mismatched tagging is the classic washed-out bug ([C §3.1]) |
| `captureDynamicRange` | SDR by default; HDR preset only in game-mode HDR (15+) | |
| filter | `SCContentFilter(display:excludingWindows: [our overlay windows])` | hides Sunna's own "controlled by" overlay from the stream |
| `includeMenuBar` | true | |

Frame handling rules: `SCStreamFrameInfo.status == .idle` means no image buffer, not an error, and is **not** encoded (today's `next_frame` re-delivers the last frame at fps with a fresh timestamp, `macos.rs:456-467`, which inflates bitrate and lies to the latency stats; `research/07` finding 9). `displayTime` (mach absolute) is the capture timestamp, converted to the host monotonic microsecond clock. `dirtyRects`, `contentRect` and `contentScale` are attached to the frame and feed the tile engine (§6.2) and the region-masked apply (§6.2). Errors: `SCStreamErrorUserStopped` (user clicked stop in the menu-bar indicator) ends the session with a user-visible reason; `SCStreamErrorNoCaptureSource` (display removed, e.g. lid closed) triggers display re-selection; `didStopWithError` after a TCC revocation surfaces "permission revoked" to the client ([C §3.1]).

Static screen policy: with no dirty rects, the encoder receives nothing and the tile engine finishes refinement; the client keeps presenting the last frame. A 1 Hz "heartbeat" P-frame is sent only while the client is `awaiting_first_frame` or after a resize. This gets the static-screen bitrate below 50 kbit/s (`research/07` step 3 acceptance).

The `FrameSource` trait gains `fn poll(&mut self, timeout) -> Frame | Idle | Stopped(reason)` and frames carry `dirty: Vec<Rect>`, `display_time_us`, `content_scale`. The IOSurface hold (`SurfaceFrame`, `macos.rs:163-219`) is kept: it is the right zero-copy handle and VT accepts the wrapped CVPixelBuffer directly.

Multi-display: one `SCStream` per captured display, each its own `stream_id` (§5.2), each its own encoder session. Display enumeration via `SCShareableContent` and `CGDisplay` reconfiguration callbacks; changes are versioned by a `display_epoch` the client sees in `HelloAck`/`DisplayLayout` messages.

#### 3.1.2 Encode: VideoToolbox, exact settings

Current code (`crates/codec/src/videotoolbox/mod.rs:175-260`) sets the low-latency spec, RealTime, no reordering, Main profile, average bitrate, infinite GOP, expected fps, and logs property failures at debug level. The v1 encoder sets the following, **checks every OSStatus, and logs unsupported keys at `warn` once per session with the OS version** ([C §3.2], `research/07` finding 8):

| Property | H.264 (default codec today) | HEVC (negotiated, default after measurement per `research/07`) | Notes |
|---|---|---|---|
| encoder spec `EnableLowLatencyRateControl` | true | true (arm64; FFmpeg only enables it on arm64, [C §3.2]) | one-in-one-out, hardware only; requires explicit bitrate |
| `RequireHardwareAcceleratedVideoEncoder` | true | true | never fall back to software silently; the software path is a separate, logged choice |
| `ProfileLevel` | `H264_High_AutoLevel` (today Main) | `HEVC_Main_AutoLevel`; `HEVC_Main10` for HDR; RExt 4:4:4 if the probe in §6.3 finds a usable constant | High gives CABAC + 8×8 transforms: measurably better text at equal bitrate (UNVERIFIED magnitude on VT; measure) |
| `RealTime` | true | true | |
| `AllowFrameReordering` | false | false | no B-frames |
| `MaxKeyFrameInterval` | `i32::MAX`; `MaxKeyFrameIntervalDuration` unset | same | IDR on demand only |
| `AverageBitRate` | from CC, applied per frame | same | |
| `DataRateLimits` | `[bytes = target/8 × 1.5 / fps, seconds = 1/fps]` i.e. a 1-frame window at 1.5× | same (historically unsupported for HEVC on old releases; log if rejected) | bounds keyframe/scene-cut bursts; the VBV analogue of Sunshine's 1-frame VBV ([A §1.3]) |
| `ExpectedFrameRate` | session fps | same | |
| `MaxFrameDelayCount` | 0 | 0 | |
| `MaxAllowedFrameQP` | 34 desktop / 40 game | 34 / 40 | keeps text legible; the encoder drops frames instead of smearing when the budget is exhausted ([C §3.2]) |
| `MinAllowedFrameQP` | 12 | 12 | stops spending bits on static frames |
| `EnableLTR` | true | true | §5.6 |
| `BaseLayerFrameRateFraction` | 0.5 in game mode and whenever fps ≥ 60 | same | makes half the frames droppable at the sender without breaking references ([C S6]); `IsDependedOnByOthers` attachment read per output sample |
| `BaseLayerBitRateFraction` | 0.7 | 0.7 | |
| `ColorPrimaries` / `TransferFunction` / `YCbCrMatrix` | P3-D65 / sRGB (or 709) / 709, full range off (video range) unless BGRA→444 path is used | same | must equal the SCK tagging |
| `PrioritizeEncodingSpeedOverQuality` | false desktop / true game | same | |
| `ConstantBitRate` | not set (conflicts with low-latency RC, `research/03 §2`) | not set | rate is driven by AverageBitRate + DataRateLimits + QP caps |

Per-frame options: `ForceKeyFrame` (only from the recovery state machine), `ForceLTRRefresh` + `AcknowledgedLTRTokens` (§5.6). Output handling: the callback runs on VT's thread and hands the AU to the sender queue directly (no `CompleteFrames` per frame as today, `mod.rs:359`, which serializes encode and blocks the capture thread); the encoder is asynchronous with exactly one frame in flight. Parameter sets are re-sent with every keyframe (today's behaviour, correct) and additionally cached so a late-joining viewer gets them. The packetizer must accept **N NAL units per sample buffer** (iOS 26 HEVC change, [C §3.2] parser gotcha).

Bitrate policy is the CC's (§5.7); the encoder never picks a bitrate. Resolution/fps changes recreate the session under a new `codec_epoch` (§5.2), with the client keeping the previous decoder alive until the first AU of the new epoch decodes, so a resize shows one clean cut instead of HP's grey hitch ([C §5.2]).

Software fallback (x264 `ultrafast`/`zerolatency`, or openh264) exists only for the Linux VM without a GPU and for tests; on macOS the hardware requirement is absolute.

#### 3.1.3 The one experiment that decides 4:4:4 (do it first)

[C §0.4] and [C §3.2] describe it: on both Airs dump `VTCopySupportedPropertyDictionaryForEncoder(w, h, kCMVideoCodecType_HEVC, spec)`, enumerate the `ProfileLevel` supported values (undocumented `Main444`-style constants appear there per developer-forum reports, UNVERIFIED), then encode `kCVPixelFormatType_444YpCbCr8BiPlanarFullRange` frames with each candidate and parse `chroma_format_idc` from the SPS. Outcome A (=3): desktop mode gets HEVC 4:4:4 like Apple HP on M-series ([C §5.2]). Outcome B: 4:2:0 + lossless tiles is the desktop mode, and AVC444-style dual view is a later optional (§6.3). This is a half-day task and it is in phase 0 (§13).

#### 3.1.4 Virtual display: `CGVirtualDisplay`, isolated and leased

Private API (`CGVirtualDisplayDescriptor`/`CGVirtualDisplay`/`CGVirtualDisplaySettings`/`CGVirtualDisplayMode`, layouts from DeskPad, [C §3.5]). Rules:

- Lives in `crates/display/macos.rs` behind a runtime probe (`NSClassFromString`, `respondsToSelector`), and the whole feature degrades to "capture the physical main display" if the probe fails on a future macOS. Never crash on a private-API change.
- Descriptor: `name = "Sunna <client name>"`, `vendorID = 0x53554E` ('SUN'), `productID` fixed, `serialNum = hash(client device key)` so macOS remembers window arrangement **per client device** ([C §3.5] inference), `maxPixelsWide/High = 5120×2880`, `sizeInMillimeters` chosen to give ~110 ppi at 1×, `hiDPI = 1`.
- Mode table: exactly two modes: the client's drawable size in points at 2× (HiDPI) and the same at 1×, both at the client display's refresh (60 or 120). New sizes re-apply settings (`applySettings:`) with a new mode; SCK follows with `updateConfiguration` (no stream restart).
- Lease semantics as SudoVDA's watchdog ([C §4.7], [C S11]): the display exists only while the agent process holds the object and a session (or a 5 s grace after disconnect) is active. Agent crash → display disappears → macOS rearranges windows back; that is acceptable and better than a stranded desktop.
- Placement: by default the virtual display is added as an additional display and made **main** for the session (menu bar and windows move there); the physical panels are put to sleep with `CGDisplay` power management when the host has no local user and the session opted into privacy ([C §3.5], HP blanks panels for the same user, [C §5.1]). Mirroring the physical panel is an option for "help me on my screen" sessions (`CGConfigureDisplayMirrorOfDisplay`).
- Clamshell: whether a CGVirtualDisplay satisfies clamshell mode is **UNVERIFIED**; the phase-0 checklist tests it on the M1 Air with the lid closed on power.
- Resize policy (§10.5): client resize events are debounced 300 ms; during the drag the client scales the current stream with a bicubic filter; on commit the host applies the new mode and starts a new `codec_epoch`.

#### 3.1.5 Audio: Core Audio process taps

`CATapDescription initStereoGlobalTapButExcludeProcesses:[our pid]` + `AudioHardwareCreateProcessTap` + a private aggregate device with `kAudioAggregateDeviceTapListKey`; `muteBehavior = CATapMuted` when the session asks for "mute host speakers" ([C §3.7], Sunshine `av_audio.mm:629-699` as the reference). API 14.2, reliable from 14.4; separate TCC bucket `kTCCServiceAudioCapture` with `NSAudioCaptureUsageDescription`; **silent zeros on denial** are detected by RMS == 0 for 3 s with a "system audio permission missing" diagnostic. SCK audio (13+) is the fallback and is covered by the Screen Recording grant. Format: 48 kHz stereo float → Opus `OPUS_APPLICATION_RESTRICTED_LOWDELAY`, 10 ms frames, CBR 128 kbit/s stereo (Sunshine's 96–512, Selkies' 128 default, [A §1.6], [A §6.6]), in-band FEC off, RED with 1 previous frame on lossy paths (§5.4). Microphone passthrough (client mic → host virtual input device) is deferred: it needs a virtual audio driver on macOS.

#### 3.1.6 Input injection and cursor

`CGEventPost` from a **signed, TCC-granted agent** (unsigned daemons drop modifier+key combos on 26.x per one report, [C §3.6]). Details:

- **Event source:** a private `CGEventSource` (`kCGEventSourceStatePrivate`) for keyboard so injected modifier state never mixes with the local user's ([C §3.6], Sunshine libvirtualhid); `kCGSessionEventTap` for keyboard, `kCGHIDEventTap` for mouse/scroll. `CGEventSourceSetLocalEventsSuppressionInterval(source, 0)` so a local user can co-drive.
- **Keyboard:** the wire carries **USB HID usages** (`research/07` step 4; today the wire carries mac keycodes, `input/src/macos.rs:7-9`), mapped host-side to virtual keycodes through the host's current layout (`TISCopyCurrentKeyboardLayoutInputSource` + `UCKeyTranslate` for the reverse map); modifier flags set on every posted event from the injector's own modifier state; autorepeat generated host-side (`kCGKeyboardEventAutorepeat`) from the client's held-key timing, not by re-sending keydowns; text that has no key on the host layout arrives as `Text { utf8 }` events and is injected with `CGEventKeyboardSetUnicodeString`; IME composition is left to the host. Held keys are released on disconnect, on client focus loss, and on a 30 s "no heartbeat" watchdog (release-all exists today, `macos.rs:293-307`; the watchdog does not).
- **Mouse:** absolute moves in host display pixels (per display, `stream_id` qualified) with `kCGMouseEventClickState` from the system double-click interval (`NSEvent.doubleClickInterval`, not a hard-coded 500 ms as in `macos.rs:241`); relative mode uses `kCGMouseEventDeltaX/Y` and `CGAssociateMouseAndMouseCursorPosition` semantics on the client; back/forward buttons as `OtherMouse` 3/4 (kept).
- **Scroll:** `CGEventCreateScrollWheelEvent2` (fixed in the working tree, `macos.rs:63-73`) with pixel units, `kCGScrollWheelEventIsContinuous = 1`, and the **scroll phase and momentum phase fields** set from the client's `NSEvent.phase`/`momentumPhase` (`kCGScrollWheelEventScrollPhase`, `kCGScrollWheelEventMomentumPhase`), scaled by the host's `com.apple.scrollwheel.scaling` ([C §3.6]). This is the single highest-fidelity gesture and it must be perfect (`research/05 §4`).
- **Gestures:** semantic channel (`InputEvent::Gesture`, `messages.rs:83-95`) replayed as configured shortcuts (Mission Control, Spaces, App Exposé) and, behind an "experimental" toggle, as forged gesture CGEvents for app-level magnify/rotate (`research/05 §2`). Raw contacts are added to the wire format now (§5.2) even though the macOS host ignores them.
- **Cursor:** poll `CGSCurrentCursorSeed()` at 120 Hz (private, cheap; RustDesk method, [C §3.6]); on change read `NSCursor.currentSystemCursor` image + hotspot, hash it, send `CursorShape { hash, w, h, hotspot, rgba(zstd) }` on the control stream if the client does not have the hash, and `CURSOR` position datagrams whenever the host cursor moves for a reason other than the client's own input (local user, app warp). The client draws the cursor itself (§4.1).

#### 3.1.7 Clipboard

Poll `NSPasteboard.changeCount` at 250 ms (no notification API, [C §3.7]); announce `ClipboardTypes { generation, types[] }` lazily and transfer contents only on the peer's paste (RDP CLIPRDR / HP style, [C S18]); never sync `org.nspasteboard.ConcealedType` or `TransientType`; plan for the 15.4+ pasteboard-privacy prompt by using `detectPatterns` metadata first and reading contents only on demand. Files: `NSFilePromise`-style lazy transfer over a bulk stream (§5.9), 4 MiB chunks, resumable by hash.

#### 3.1.8 Service model, permissions, onboarding

- **Bundle shape:** `Sunna.app` (Developer ID signed, hardened runtime, notarised, stapled) contains `Contents/Library/LaunchDaemons/app.sunna.broker.plist` + `sunnad`, and `Contents/Library/LaunchAgents/app.sunna.agent.plist` pointing at `Contents/Library/LoginItems/Sunna Agent.app` (an `LSUIElement` bundle with its own stable bundle ID `app.sunna.agent` and the same Team ID, so TCC keys the grants to a stable identity and Tahoe 26.1 lists it, [C §3.3], [F §7.1]). `AssociatedBundleIdentifiers` groups it under Sunna in Login Items.
- **Registration:** `SMAppService.daemon(plistName:)` and `SMAppService.agent(plistName:)` from the shell; one admin prompt for the daemon (macOS 13+; replaces RustDesk's osascript, [F §7.1]).
- **Agent plist:** `LimitLoadToSessionType = [Aqua, LoginWindow]`, `KeepAlive = true`, `ProcessType = Interactive`; the LoginWindow instance is bootstrapped with `launchctl bootstrap loginwindow/…` (RustDesk uses `load -S LoginWindow`, [C §3.4]). The pre-login agent runs as root in the LoginWindow session and captures with SCK (14.4+).
- **Broker ↔ agent:** XPC (Mach service registered by the daemon plist). The agent connects on launch and reports `{session_type, uid, console_owner, display_epoch, permissions}`. Media and input must never cross a process boundary per frame, and quinn cannot hand a live connection to another process, so the **agent owns the QUIC endpoint for media sessions**, on a UDP socket that the broker binds and passes over XPC (fd passing), which keeps the port stable across agent restarts and login/logout. The broker owns everything that is not per-frame: rendezvous registration, relay path maintenance, pairing, the peer store, admission. Before the agent completes a QUIC handshake it asks the broker `Admit { peer_key } -> { allowed, permissions, session_token }` over XPC. When the console moves to a different agent (login, logout, fast user switch) the broker passes the socket to the new agent and the client **reconnects under the same session token** (§10.6; target < 1 s, no re-pairing, no re-auth prompt). This is honest about the quinn limitation and still beats g-r-d's RDP redirect-and-reconnect ([C §2.1]); a reconnect-free handover is an open question (§14.2).
- **Permissions page** (one screen, live-polled every 500 ms): Screen & System Audio Recording (`CGPreflightScreenCaptureAccess`, then `CGRequestScreenCaptureAccess` once), Accessibility/PostEvent (`CGPreflightPostEventAccess`/`CGRequestPostEventAccess`; the three buckets are independent, [C §3.3]), System Audio Recording (tap probe). The page knows that the Screen Recording grant requires a relaunch of the *agent* (not the app) and restarts it via the broker automatically. Input Monitoring is **not** requested on the host (it is a client-side need for Cmd-Tab capture, opt-in).
- **Sequoia re-prompt:** apply for `com.apple.developer.persistent-content-capture` before shipping ([C §3.3], Jump Desktop has it); until granted, the agent detects the re-approval state (SCK start fails with a TCC error while preflight says granted) and pushes a "needs re-approval on the host" notice to paired clients and the host's menu-bar item. MDM `forceBypassScreenCaptureAlert` documented for managed fleets.

#### 3.1.9 Sleep, lock, login window, fast user switch

- **Sleep:** while listening, do not hold assertions; rely on "Wake for network access" (documented in onboarding). On an incoming connection the broker wakes the display (`IOPMAssertionDeclareUserActivity`) and holds `PreventUserIdleDisplaySleep` for the session; `PreventUserIdleSystemSleep` only if "Always available" is on. Lid-closed hosting requires power + (UNVERIFIED) the virtual display counting for clamshell; otherwise the onboarding says "keep the lid open or attach a display/dummy plug".
- **Lock screen:** the Aqua agent keeps capturing the lock screen (SCK does); input works; the client can type the password. The "controlled by" overlay is shown on the lock screen as well.
- **Login window:** served by the LoginWindow agent; after login the broker switches the socket to the new user's Aqua agent (reconnect, §10.6).
- **Fast user switching:** same as login: the console-owning agent is the one holding the socket; `SCK` in the inactive session stops delivering, which the agent reports as `Suspended`.
- **Display sleep / lid close mid-session:** SCK reports `.suspended`/error; the agent re-selects a display or (virtual display present) continues; the client sees a "host display sleeping" state, never a frozen frame without explanation ([A §8.1] Sunshine #5509 is the anti-pattern).
- **Thermals:** fanless Airs at 4K60 + our GPU work can throttle; the CC reads `ProcessInfo.thermalState` and caps fps at `.serious` (UNVERIFIED magnitude, [C §3.4]).

### 3.2 Linux host

The Linux problem is consent and session ownership under Wayland, not encoding ([C §4]). The design is the one in [C §4.6] with the following specifics.

#### 3.2.1 Process model

`sunnad` (system service, dedicated `sunna` user with narrowly scoped helpers) + `sunna-agent` per session (spawned by the broker via `systemd-run --user`/`pam_systemd` env, or as root at the greeter). Same broker↔agent contract as macOS over a Unix socket with `SCM_RIGHTS` fd passing (UDP socket, dmabuf fds from helpers).

#### 3.2.2 Capture ladder (tried in order, chosen per session, reported to the client)

| Rank | Route | When | Details |
|---|---|---|---|
| 1 | **Mutter private API** `org.gnome.Mutter.ScreenCast.RecordVirtual` + `org.gnome.Mutter.RemoteDesktop` (EIS) | GNOME, agent inside the user session | virtual monitor sized to the client, cursor mode METADATA, PipeWire stream negotiated as in g-r-d: BGRx, dmabuf with modifiers when not NVIDIA (MemFd on NVIDIA), 2–8 buffers, `maxFramerate = client refresh`, `SPA_META_Header` (drop CORRUPTED), `SPA_META_Cursor` ≤ 384², explicit sync via `SPA_META_SyncTimeline` when available ([C §2.2]). No dialogs: the agent is a trusted component; enablement is the consent. |
| 2 | **KWin** `zkde_screencast_unstable_v1.stream_virtual_output(name, w, h, dpr, cursor_mode)` | KDE Plasma 6 | needs `X-KDE-Wayland-Interfaces=zkde_screencast_unstable_v1` in the agent's .desktop ([C §2.7]) |
| 3 | **xdg-desktop-portal** ScreenCast (`VIRTUAL=4` source type where supported) + RemoteDesktop `ConnectToEIS`, `persist_mode = 2`, restore token stored and **rotated after every Start** | any compositor with a portal backend; Flatpak | one interactive approval, then silent; backend-dependent ([C §4.1]); `pipewire-serial` targeting (v6) |
| 4 | **DRM/KMS scanout** via `sunna-kmshelper` | greeter, headless boxes, unattended without a compositor API, the owner's virtio-gpu VM | RustDesk PR #15420 design: separate binary with only `CAP_SYS_ADMIN`, `PR_SET_NO_NEW_PRIVS`, seccomp allowlist, `drmModeGetFB2` → dmabuf fd over a socketpair, detiling (Intel CCS) in the unprivileged agent, cursor plane read separately, 60 fps poll ([C §4.1], [C S16]). Binary mode 0750, group `sunna`, IPC authenticated by peer credentials ([C A15]). |
| 5 | X11 | Xorg sessions | XShm + XDamage + XFixes cursor; XTest input |

Damage detection: use PipeWire `SPA_META_VideoDamage` when present, otherwise the GPU pass below computes per-16×16-block damage ([C §2.3]). Static screen policy as macOS.

#### 3.2.3 GPU pass and encode

One Vulkan compute pass per frame (g-r-d's `grd-avc-dual-view.comp` is the reference, [C §2.4], [C S12]): dmabuf import (`VK_EXT_image_drm_format_modifier`) → BGRX → NV12 main view (+ optional 4:4:4 aux view) + damage bitmap + "chroma needed" bitmap, then encode from the same device:

| Encoder | Settings |
|---|---|
| VAAPI (Intel/AMD) | H.264 High / HEVC Main; `VAEntrypointEncSliceLP` on Intel when available; rate control **VBR with 1-frame HRD buffer** (Intel and AV1) else CBR ([A §1.3]); `intra_period = 0`, `ip_period = 1`, `max_num_ref_frames = 4` (LTR needs a DPB; g-r-d uses 1 because it has no LTR), one slice, SEI off; QP bounds 12–34 desktop; we write our own SPS/PPS like g-r-d (`grd-nal-writer.c`) only if the driver's VUI tagging is wrong. **Disagreement with g-r-d:** constant-QP + fps throttling is not acceptable on thin links ([C A3]); bitrate comes from the CC. |
| NVENC (NVIDIA) | direct SDK: `NV_ENC_TUNING_INFO_ULTRA_LOW_LATENCY`, preset P3 (Selkies' 3.7 vs 5.2 ms P3/P4 at ~0.001 SSIM, [A §6.3]; Sunshine's P1 is unnecessary), CBR, `vbvBufferSize = bitrate/fps × 1.5` (Selkies' 1.5-frame policy, [A §6.3]; Sunshine uses 1 frame), `frameIntervalP = 1`, `zeroReorderDelay = 1`, lookahead off, AQ off, `enableLTR = 1, ltrNumFrames = 2`, DPB 8, infinite GOP, intra-refresh **off by default** (LTR covers recovery; enable period 300/count 299 on paths where feedback RTT > 150 ms, [A §1.3]), repeat SPS/PPS, 4 slices H.264/HEVC for encode parallelism (not for transmission latency: NVENC returns a completed AU, [A §10.3] item 7), CUDA import of dmabuf on pure-NVIDIA, MemFd + CUDA upload on hybrid (g-r-d/Sunshine evidence, [C §2.4], [A §1.2]). Chroma siting: use a centered NV12 conversion kernel, not NVENC's built-in RGB conversion (pixelflux finding, [A §6.2]). |
| Vulkan Video | H.264/HEVC/AV1 via `VK_KHR_video_encode_*` (Moonshine/PixelForge reference, [A §5.2]): `LowLatency` tuning, CBR, GOP 0, VBV `1000/fps` ms, explicit reference-state modelling for RFI. Behind a feature flag until driver maturity is measured (UNVERIFIED 2026 Mesa/NVIDIA status, [C §4.4]). |
| x264 / openh264 | VMs without a GPU (the owner's Linux VM): `preset superfast, tune zerolatency, rc-lookahead 0, bframes 0, ref 1, keyint infinite, intra-refresh off, vbv-maxrate = target, vbv-bufsize = target/fps × 1.5, threads = sliced`; 4:4:4 via High 4:4:4 Predictive is possible but the lossless tile path matters more here ([C §4.3]). |

#### 3.2.4 Input, cursor, audio, clipboard

- **Input:** libei/EIS when the session route is 1–3 (`reis` crate; separate devices for relative pointer, absolute pointer with regions per stream, keyboard **with the compositor's keymap fd**, touch; smooth scroll `scroll_delta` + `scroll_stop`, discrete `scroll_discrete`; keys with monotonic timestamps; unicode → keycode by keymap-level search exactly as g-r-d, [C §2.6], [C S14]). uinput via `sunna-inputhelper` (udev rule, group `sunna`) for route 4/5 and for the **virtual multitouch trackpad** (`ABS_MT_SLOT`, `BTN_TOOL_QUINTTAP`, `INPUT_PROP_POINTER|BUTTONPAD`, inputtino `trackpad.cpp:42-71`) that turns the Mac client's raw contacts into native libinput gestures ([C S15], `research/05 §5`; end-to-end UNVERIFIED, test on the VM with a nested compositor). Gamepads: uinput Xbox/DualSense/Switch profiles (inputtino/libvirtualhid designs, [A §4.4], [A §1.5]).
- **Cursor:** PipeWire cursor metadata (route 1–3) or the KMS cursor plane (route 4) → same shape cache protocol as macOS.
- **Audio:** PipeWire monitor of the default sink; in headless sessions a null sink per session (`pactl load-module module-null-sink`); Opus as macOS.
- **Clipboard:** Clipboard portal (v1) on GNOME (`RequestClipboard` before `Start`, then `SetSelection`/`SelectionRead`/`SelectionWrite`), `ext-data-control-v1`/`wlr-data-control` on KDE/wlroots, X11 selections on Xorg ([C §4.5]).

#### 3.2.5 Service, permissions, login, headless

- Packaging: deb/rpm with `sunnad.service` (system) and a `sunna-agent@.service` user template; Flatpak build for clients and portal-only hosting (no system service, documented reduced mode, [F §7.3]).
- Login screen: route 4 (KMS helper + uinput) at the greeter, opt-in at install ("Allow connections at the login screen"), because portals cannot capture the greeter and Mutter's remote-login creates separate headless sessions, not the console ([C §2.1]). On GNOME 46+ an optional integration with GDM's RemoteDisplayFactory for headless remote login is a later feature.
- Headless: route 1/2 virtual monitors need a running compositor; for boxes with no monitor, `vkms`/EDID override + KMS, or a headless Mutter/KWin session started by `sunnad` for the auto-login user (documented recipe; Wolf-style owned compositor per session is a later, containerized product, [A §4]).
- Sleep: `systemd-logind` inhibitor (`idle`, `sleep`) during sessions; `Wake-on-LAN` relay through a LAN peer (§7.6).

### 3.3 Windows host

Third; needed for the gaming segment and Parsec parity. Everything here is the proven Parsec/Sunshine shape ([F §1B.10], [A §1.2]) and is written so that the Rust core stays identical.

- **Service model:** `sunnad` as an SCM service (LocalSystem), launches `sunna-agent.exe` into the active console session with the winlogon token (`WTSQueryUserToken`/`CreateProcessAsUser`), tracks `WTS_SESSION_CHANGE`, calls `SetThreadDesktop` on the input thread before injecting so UAC/secure desktop and the login screen work ([F §1B.10], [F §6.3]); `uiAccess=true` manifest (signed, under Program Files) for elevated windows.
- **Capture:** DDA (`DuplicateOutput1`, `AcquireNextFrame`, dirty **and move rects** which feed the scroll-blit path, pointer shape separately, QPC timestamps) primary on the adapter driving the output; WGC (`GraphicsCaptureItem` for the monitor, free-threaded frame pool of 2, `IsBorderRequired = false`, `MinUpdateInterval = 4 ms` where available) for hybrid-GPU and per-window capture ([A §1.2], [C §4.7]). VRAM stays on the GPU: `ID3D11Texture2D` → NVENC `NvEncRegisterResource` / AMF surface / VPL D3D11 surface. HDR: `R16G16B16A16_FLOAT` capture → GPU shader → P010 BT.2020 PQ → HEVC Main10/AV1 10-bit + HDR10 static metadata (`research/03 §2`).
- **Encoders:** NVENC as §3.2.3 (D3D11 input; note YUV444 is not exposed via D3D12, `research/03 §2`); AMF `ULTRA_LOW_LATENCY`, latency-constrained VBR, preanalysis off, VBAQ on, HRD enforcement off, forced IDR, async depth 1, no 4:4:4 ([A §1.3]); QSV via libvpl `TargetUsage 7`, `AsyncDepth 1`, `LowPower on`, `LowDelayBRC`, `NO_RC_BUF_LIMIT`, HEVC 4:4:4 on Arc ([A §1.3]). Capability probing by **real test encodes** at session start (Sunshine's practice, [A §1.8]).
- **Virtual display:** an IddCx driver with SudoVDA's control plane (`ADD_VIRTUAL_DISPLAY {w,h,refresh mHz, guid, serial}`, `REMOVE`, `SET_RENDER_ADAPTER`, `PING` watchdog 3 s, generated EDID with the preferred mode, hardware cursor declared, HDR via IddCx 1.10 on 24H2, [C §4.7], [A §3.2]). **Decision:** ship SudoVDA (or a fork) under its licence with attestation signing rather than write a new driver; per-client stable serials so Windows remembers scaling/HDR per client ([A §3.2]). Signing budget is a real cost (§14.1).
- **Input:** `SendInput` with scancodes (HID usage → scancode table), `MOUSEEVENTF_MOVE` relative and `ABSOLUTE` normalized per monitor rect (the Sunshine #5733 wrong-monitor bug is a coordinate-space bug to test for, [A §8.1]); trackpad contacts via `CreateSyntheticPointerDevice2(PT_TOUCHPAD)` + `InjectSyntheticPointerInput` on Windows 11 (`research/05 §5`; pre-release banner, test per build); gamepads via a **ViGEmBus successor** (ViGEmBus is archived; libvirtualhid and inputtino approaches, [A §1.5]) or our own VHF-based driver later. Kernel anticheat blocks virtual input for everyone; documented, not solved (`research/04`).
- **Audio:** WASAPI loopback (`AUDCLNT_STREAMFLAGS_LOOPBACK`, event-driven, 10 ms) → Opus.
- **Clipboard:** `AddClipboardFormatListener` + lazy formats; files via delayed rendering.
- **Packaging:** MSI (WiX) with the service; Azure Trusted Signing for the binaries ([F §7.2]); WinGet manifest.
---

## 4. Clients per OS

### 4.1 macOS client (first, and the reference implementation)

**Decode:** `VTDecompressionSession` on a dedicated decode thread, asynchronous (`kVTDecodeFrame_EnableAsynchronousDecompression`), output `kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange` (or `444`/`P010` per stream) into **IOSurface-backed CVPixelBuffers** (`kCVPixelBufferIOSurfacePropertiesKey`), never a CPU copy (today `mod.rs:445-484` copies BGRA to a Vec; `research/07` finding 2). `RealTime` default; `EnableTemporalProcessing` off; streams are B-frame-free with `max_num_reorder_frames = 0` in the VUI so the decoder never buffers (Moonlight patches the SPS for this on macOS, [A §2.2]; we emit it correctly at the encoder). The decoder is recreated on `codec_epoch` change; the old one stays alive until the first new AU decodes.

**Presentation:** the presenter trait (`present::Presenter { submit(frame, tiles, cursor) ; presented_time() }`) with a direct `CAMetalLayer` implementation on a dedicated present thread ([C §6.1], `research/07` decision "Direct CAMetalLayer"):

- Textures via `CVMetalTextureCacheCreateTextureFromImage` (Y as `r8Unorm`, CbCr as `rg8Unorm`; 4:4:4 as three `r8` planes); one fragment shader does YUV→RGB with correct chroma siting and composites the lossless tile atlas and the region mask over it (§6.2); output `bgra8Unorm` into an **opaque** layer so fullscreen can go direct-to-display (Apple: fullscreen Space + opaque CAMetalLayer + RGB + Apple silicon; measured 29–55 ms fullscreen vs bimodal 42/80–122 ms windowed, [C §6.1]).
- Two pacing modes, A/B-able from the overlay: **arrival** (present immediately, `displaySyncEnabled = false`, fullscreen only) and **display-link** (`CAMetalDisplayLink`, `preferredFrameLatency = 1`, latest-frame-wins, render as late as possible; what moonlight-qt does on Apple silicon, [C §6.1]). Default: display-link in windowed mode, arrival in fullscreen game mode.
- `maximumDrawableCount = 3` default, 2 measured (Apple forums: 2 is hard to make work with the compositor, [C §6.1]); never `waitUntilCompleted` on the present thread ([C A12]); textures released in `addCompletedHandler`; `MTLDrawable.presentedTime` is the end-of-pipeline timestamp for telemetry ([C S20]).
- Sharpness: drawable size = stream size when 1:1 (nearest sampling); during window drags scale with bicubic in the shader, then request a host resize (§10.5); layer `colorspace` tagged to the stream's (P3 or sRGB) so nothing is double-gamma'd ([C §6.2]).
- Cursor: hardware cursor via `NSCursor` from host shapes (`NSCursor(image:hotSpot:)`, cached by hash) while the pointer is inside the stream view; host-sent positions move it only when the host moved it (§3.1.6). Zero-latency pointer feel is the biggest perceived-latency win per line of code ([C S9]).
- HDR: EDR on (`wantsExtendedDynamicRangeContent`, `CAEDRMetadata.hdr10`) when the stream is Main10/PQ, as moonlight-qt's Metal renderer ([A §2.2]).

**Input capture fidelity** (the client side of §3.1.6):

- `NSEvent` local monitors on the stream window; `NSEvent.isMouseCoalescingEnabled = false` so every sample arrives ([C §6.3]); events are coalesced by *our* 4 ms input tick (relative deltas summed, absolute latest, discrete events queued in order, Q9).
- Keyboard: `keyCode` → USB HID usage via the fixed Apple table; `characters`/`charactersIgnoringModifiers` carried as `Text` only when the host requests layout-independent typing (a per-session toggle: "type using my layout" vs "send physical keys"); dead keys and IME handled locally then sent as `Text`. Modifier state is sent as flags on every key event *and* as a `ModifierSync` event every 250 ms while any modifier is down (the host reconciles).
- Cmd/Ctrl semantics: default "as-is" (Cmd on the Mac client is Cmd on the Mac host); a per-host "swap Cmd/Ctrl" for Linux/Windows hosts, applied at the client.
- System shortcuts (Cmd-Tab, Space, Mission Control): captured only in "capture system keys" mode (opt-in, requires the client's Input Monitoring/Accessibility grant, escape chord `Ctrl+Option+Cmd+Esc`, released on focus loss; `research/05 §3` Citrix pattern, `fn`-hold routes a gesture locally).
- Scroll: `scrollingDeltaX/Y` in pixels (`hasPreciseScrollingDeltas`), `phase`, `momentumPhase` forwarded verbatim; line-based mice converted with the host's line height.
- Gestures: `NSEvent` magnify/rotate/swipe with phases → semantic events; raw contacts from `NSTouch` (`allowedTouchTypes = [.indirect]`) when the host advertises `native_contacts` (Linux/Windows hosts) — carried as `TouchFrame` events (§5.2).
- Pointer lock (relative mode): `CGAssociateMouseAndMouseCursorPosition(false)` + hide cursor + warp to centre; exit chord `Ctrl+Option+Cmd+G`.
- Pen/tablet: `NSEvent.tabletPoint` pressure/tilt → `Pen` events (host support later; Parsec Warp's Wacom feature is a paid gate, `research/04 §1`).
- Gamepads: `GameController.framework` (`GCController`) → `GamepadState` at 250 Hz max with per-controller slot ids; rumble/haptics back via `GCDeviceHaptics`.

**UI:** SwiftUI shell (§12.2). Windows: a machine list (online dot, path badge LAN/direct/relay, last seen), the stream window (native, per display, with a thin hover toolbar: fullscreen, displays, quality, overlay, disconnect), Settings, pairing sheet.

### 4.2 Linux client

Decode: VAAPI (`libva`) primary with dmabuf export; Vulkan Video decode where available (via the same `codec-vulkan` crate as the encoder); `dav1d` for AV1 software decode; `openh264`/ffmpeg-free path is a non-goal (we link vendor APIs directly, `research/00 §4`). Presenter: `present-vulkan` on Wayland: dmabuf → `VK_EXT_external_memory_dma_buf` import → composite shader → swapchain with `MAILBOX`/`IMMEDIATE` when available, `FIFO` otherwise; `presentation-time` feedback gives `presentedTime`; `tearing-control-v1` for game mode; `fifo-v1`/`commit-timing-v1` when the compositor offers them ([C §6.4]). Kiosk/handheld variant: DRM atomic commit to a plane (moonlight `drm.cpp` pattern). X11: Vulkan surface as usual. Input: `libinput` via the toolkit (winit) for pointer/keyboard, `evdev` direct for gamepads and for raw trackpad contacts (opt-in, needs `input` group). Hardware cursor via `wl_pointer.set_cursor` with host shapes. UI: Tauri shell (§12.2), later.

### 4.3 Windows client

Decode: D3D11VA (DXVA2 fallback). Presenter: `present-d3d11`: flip-model swapchain (`DXGI_SWAP_EFFECT_FLIP_DISCARD`, 3 buffers), frame-latency waitable object + `SetMaximumFrameLatency(1)`, `DXGI_PRESENT_ALLOW_TEARING` in borderless fullscreen (independent flip/MPO), `GetFrameStatistics` for presented time ([C §6.4]; moonlight-qt avoids naive max-latency=1 in windowed mode because Present can block, [A §2.2], so windowed uses 2). Input: Raw Input (`WM_INPUT`) for relative mouse and scancodes, `WM_POINTER` for touch/pen, XInput/GameInput for gamepads. Hardware cursor via `SetCursor` from host shapes.

### 4.4 Browser client (`sunna-web`, phase 4)

WebTransport (HTTP/3) to the host's QUIC endpoint (same ALPN family, `sunna-web/1`; the host runs a WebTransport-capable listener behind the same UDP socket by ALPN demux) with datagrams for media and streams for control; WebCodecs `VideoDecoder` with `optimizeForLatency: true`, H.264/HEVC/AV1 as the browser supports; rendering into a `<canvas>` via WebGPU/WebGL from `VideoFrame` objects; audio via `AudioWorklet`. Constraints acknowledged: browser compositor buffering cannot be bypassed, Safari support for WebTransport is the gating item (UNVERIFIED as of 2026-09), and the certificate must be accepted via `serverCertificateHashes` (pinned host key, works with RPK-style self-signed certs valid ≤ 14 days, so the host rotates a web cert daily) — this is exactly why the pairing model pins keys, not CA chains. Guest links (§9.3) open the web client first when no app is installed. Selkies' lesson: WebSocket fallback is a materially different loss model; we do not ship it ([A §6.5]).

### 4.5 Mobile and tablet stance

iPad first among mobile: VideoToolbox decode + `CAMetalLayer`, Apple Pencil pressure/tilt, external keyboard, trackpad (`UIPointerInteraction`), gamepads via `GameController`. iPhone/Android after. **Decision:** native Swift (iPad) and Kotlin (Android) shells over the Rust engine via UniFFI, not React Native (`research/06 §1` "revisit if the UI stays a host list" — it does). Android: `MediaCodec` low-latency (`KEY_LOW_LATENCY`) to a `SurfaceView` directly. Mobile networks: QUIC connection migration + the mapped-address socket means Wi-Fi↔cellular handover is transparent to the session (§7.7). Not before phase 4.

---

## 5. Media transport

### 5.1 Principles (all inherited from the dossiers, none new)

1. Realtime data is **never** on a reliable in-order stream (RustDesk TCP/KCP/SCTP, RDP GFX, HP input over TCP are the anti-patterns, [F §8.2], [C A1]).
2. **One controller** owns pacing, encoder target and FEC budget; quinn's congestion controller is an adapter for it, not a second opinion ([A §10.3] item 5).
3. Every encoded picture drop is a **codec state transition**, never silent ([A §10.3] item 2).
4. Repair is chosen by whether the result can still arrive before its deadline: FEC (zero RTT) → NACK (1 RTT, if time remains) → LTR refresh (1 RTT + encode) → IDR (last resort) ([C A2], [A §10.4]).
5. Priorities: **input > audio > video base layer > video enhancement layer > refinement tiles > bulk** (owner's order; implemented by the sender mux and QUIC stream priorities).

### 5.2 Packet formats (byte layouts)

All datagrams are QUIC DATAGRAM frames (RFC 9221). Byte 0 is the kind; all multi-byte integers are big-endian (as today's `MediaHeader`, `proto/src/media.rs:43-50`). Timestamps are the sender's monotonic microsecond clock truncated to 32 bits (wraps every 71 minutes; receivers unwrap). Clock offset between host and client is estimated by the existing Ping/Pong at min-RTT (`client/src/lib.rs:284-294`), now carried in `PROBE` datagrams so it is not delayed by the control stream.

**Kinds:** `0x01 VIDEO`, `0x02 VIDEO_FEC`, `0x03 AUDIO`, `0x04 INPUT`, `0x05 FEEDBACK`, `0x06 CURSOR`, `0x07 PROBE`, `0x08 RTX` (a retransmitted VIDEO/VIDEO_FEC datagram, byte-identical after the kind byte), `0x09 GAMEPAD_FB` (rumble/LED host→client).

**VIDEO (0x01), 28-byte header** (replaces the 25-byte v0 header):

```
off  size  field           meaning
0    1     kind            0x01
1    1     flags           bit0 KEY (IDR/keyframe), bit1 SOF (first chunk), bit2 EOF (last chunk),
                           bit3 LTR (this frame is an LTR reference; ack it), bit4 DISCARDABLE (enhancement layer,
                           no frame references it), bit5 LTR_REFRESH (predicted only from an acked LTR),
                           bit6 REFINE (low-QP refinement pass for a region; apply with region mask), bit7 reserved
2    1     stream          low nibble: display/surface id 0..15; high nibble: temporal layer 0..3
3    1     codec_epoch     increments on any encoder reconfigure (size, codec, parameter sets); receiver resets decoder
4    4     seq             transport sequence number, one counter shared by VIDEO, VIDEO_FEC, AUDIO, RTX(orig seq kept) —
                           loss and reordering are detected on this counter regardless of kind
8    4     frame_id        wire frame id (burned on every sender-side drop, as today host/src/lib.rs:169-172)
12   2     chunk_index     0..chunk_count-1
14   2     chunk_count     data chunks in this frame
16   4     frame_len       bytes of the AU (Annex B); chunk sizes derive from it (balanced chunking, as today)
20   4     send_ts         µs, when this datagram left the pacer
24   4     capture_ts      µs, host capture clock (SCK displayTime), same for all chunks of the frame
28   …     payload         AU bytes
```

**VIDEO_FEC (0x02), 32-byte header:** bytes 0–27 as VIDEO with `chunk_index` = parity index within its block, then:

```
28   2     block_first     chunk_index of the first data chunk covered by this block
30   1     block_k         data chunks in the block (≤ 64)
31   1     block_r         parity chunks in the block (≤ 32)
32   …     payload         Reed–Solomon (GF(2^8), systematic, Cauchy/Vandermonde matrix as in `reed-solomon-simd`) parity
                           symbol over the block's chunks, each zero-padded to the block's standard chunk size
```

Blocks are per frame: chunks are grouped into blocks of `k ≤ 64` (a 100 KiB keyframe at 1150-byte chunks is 90 chunks → 2 blocks), so a lost chunk only delays its own block's repair and the 255-shard RS limit never bites (Sunshine's "> 4 blocks → FEC disabled for enormous IDRs" failure mode, [A §1.4], cannot happen).

**AUDIO (0x03), 16-byte header:**

```
0 1 kind=0x03 | 1 1 flags (bit0 RED present, bit1 silence/DTX) | 2 1 stream (audio channel set id) | 3 1 reserved
4 4 seq | 8 4 audio_seq (per 10 ms frame; timestamp = audio_seq × 10 ms) | 12 4 send_ts
16 … payload: [u16 len][Opus frame audio_seq] then, if RED, [u16 len][Opus frame audio_seq-1]
```

**INPUT (0x04), 12-byte header + events:**

```
0 1 kind=0x04 | 1 1 flags | 2 2 input_seq (u16, per datagram) | 4 4 send_ts | 8 4 first_pending_discrete_seq
12 … events: repeated { u8 type, u8 len, u16 event_seq, body }
```

Event types: `0x01 KEY {hid_usage u16, down u8, modifiers u8 (bitfield L/R Ctrl/Shift/Alt/Meta packed as 8 bits: LCtrl,LShift,LAlt,LMeta,RCtrl,RShift,RAlt,RMeta)}`, `0x02 TEXT {utf8}`, `0x03 MOUSE_ABS {stream u8, x u16, y u16 in 1/65535 of the display}`, `0x04 MOUSE_REL {dx i16, dy i16 (1/8 px)}`, `0x05 BUTTON {button u8, down u8, click_count u8}`, `0x06 SCROLL {dx i16, dy i16 (1/8 px), unit u8 (0 pixel,1 line), phase u8 (none/began/changed/ended/cancelled/may-begin), momentum u8 (none/began/changed/ended)}`, `0x07 GESTURE {kind u8, phase u8, fingers u8, dx/dy i16, vx/vy i16, log2scale i16 (1/256), rot i16 (1/256 rad)}`, `0x08 TOUCH_FRAME {device u8, indirect u8, n u8, then n × {id u8, x u16, y u16, major u8, minor u8, pressure u8, state u8}}` (raw trackpad/touchscreen contacts, `research/05 §4`), `0x09 PEN {x u16, y u16, pressure u16, tilt_x i8, tilt_y i8, buttons u8, state u8}`, `0x0A GAMEPAD {slot u8, buttons u32, lt u8, rt u8, lx i16, ly i16, rx i16, ry i16}`, `0x0B GAMEPAD_MOTION {slot u8, gyro 3×i16, accel 3×i16}`, `0x0C MODIFIER_SYNC {modifiers u8, locks u8}`, `0x0D RELEASE_ALL`.

Reliability by redundancy, not by stream: **discrete** events (KEY, TEXT, BUTTON, SCROLL phase transitions, GESTURE begin/end, GAMEPAD button changes, TOUCH id changes, MODIFIER_SYNC) are re-included in every INPUT datagram until the host's FEEDBACK acknowledges `input_ack_seq ≥ event_seq` (Moonlight uses reliable ENet for these and unreliable for touch/pen moves, [A §2.5]; we get the same semantics without head-of-line blocking). **Continuous** events (MOUSE_REL summed, MOUSE_ABS latest, SCROLL deltas summed within a phase, GAMEPAD axes latest, TOUCH_FRAME latest) are sent once. An INPUT datagram is sent every 4 ms while there is anything to send and immediately on a discrete event; every datagram ≤ 1 MTU. The host applies events in `event_seq` order and ignores duplicates.

**FEEDBACK (0x05), client → host every 20 ms (or immediately on loss detection / LTR arrival):**

```
0 1 kind=0x05 | 1 1 flags | 2 2 feedback_seq | 4 4 send_ts
8 4 base_seq          first seq described by the arrival vector
12 2 count            entries (≤ 128)
14 2 input_ack_seq    highest input event_seq applied (host→client direction: input_ack lives in the host's FEEDBACK)
16 4 last_decoded_frame_id | 20 4 last_presented_frame_id | 24 4 last_presented_ts (client clock)
28 1 decode_queue_depth | 29 1 present_queue_depth | 30 2 reserved
32 … arrival vector: count × u16 = arrival offset from send_ts of base_seq in units of 8 µs (0xFFFF = not received)
   then u8 n_nack, n_nack × u32 seq
   then u8 n_ltr_ack, n_ltr_ack × u32 frame_id (frames flagged LTR that were fully decoded)
   then u8 n_stream_state, n × {u8 stream, u8 state (0 ok, 1 awaiting_keyframe, 2 awaiting_ltr_refresh), u32 last_good_frame_id}
```

The arrival vector is RFC 8888-style per-packet feedback and is what the delay estimator consumes (§5.7). The host sends its own FEEDBACK (same layout) for the INPUT direction with `input_ack_seq` filled and the vector empty.

**CURSOR (0x06):** `kind, flags (bit0 hidden, bit1 shape_hash present), stream u8, reserved u8, seq u32, x u16, y u16, shape_hash u64`. Sent when the host cursor moves for reasons other than client input, at most every 4 ms.

**PROBE (0x07):** `kind, flags (bit0 request, bit1 reply, bit2 padding), seq u32, t_send u32, t_echo_recv u32, t_echo_send u32, then padding bytes` — RTT/clock-offset ping (replaces the control-stream Ping/Pong) and bandwidth-probe padding (§5.7).

**Control stream (bidi, opened by client):** length-prefixed postcard `ControlMessage` as today (`transport/src/lib.rs:214-294`), with the reader on its own task (already done in the working tree, `client/src/lib.rs:154-164`). Message set: `Hello`, `HelloAck` (now with capability sets and `display_layout`), `Caps`, `DisplayLayout`, `StreamConfig` (per-stream codec/size/fps/epoch), `Resize`, `CursorShape`, `ClipboardTypes/Request/Data`, `Permissions`, `SessionState`, `Bye`. **Never** input, keyframe requests, or receiver reports (those moved to datagrams).

**Tile streams (uni, host → client, QUIC stream priority = low):** records `TileBatch { epoch u8, stream u8, tile_size u8 (log2), n u16, n × { x u16, y u16, hash u64, codec u8, len u32, payload } }` (§6.2). **Bulk streams (uni, lowest priority):** clipboard payloads, file transfer chunks.

### 5.3 Reassembly and deadlines (client)

Per stream, a map of up to 3 in-progress frames keyed by `frame_id` (Q10). A frame is **complete** when all data chunks are present or RS-recoverable per block (recovery runs as soon as any block has `k` of `k+r`). A frame is **abandoned** when (a) a frame with id ≥ current+3 starts, or (b) its deadline passes: `deadline = capture_ts + target_latency(stream)` where `target_latency` is the CC's current one-way estimate + 1 frame interval + 5 ms (typically 15–35 ms LAN). Reordering tolerance: chunks from `frame_id − 2 .. frame_id + 2` are accepted; older ones counted `stale`. Abandonment of a frame that is not DISCARDABLE sets the stream state to `awaiting_ltr_refresh` (or `awaiting_keyframe` if no LTR is acked) and is reported in the next FEEDBACK; subsequent non-recovery frames are not decoded (as today, `client/src/lib.rs:248-251`). DISCARDABLE frames (temporal enhancement layer) are dropped silently.

### 5.4 FEC policy (adaptive, per block)

`r = clamp(ceil(k × ratio), r_min, 32)` where:

- `ratio = 2 × loss_ewma + 1.5 × burst_factor` with `loss_ewma` the EWMA (α = 0.3, as Selkies, [A §6.4]) of per-feedback loss fraction and `burst_factor` = fraction of losses that were part of runs ≥ 2 (bursts need more parity per block than independent losses).
- `r_min = 1` when `k ≥ 4` and the path RTT > 1 frame interval (NACK cannot help in time); `r_min = 0` on LAN with RTT < 5 ms (NACK is cheaper than parity); keyframes always get `r ≥ max(1, k/8)`.
- Cap: FEC bytes ≤ 30 % of the video budget; beyond that the CC lowers the encoder target instead (FEC, RTX and IDR bytes are all charged to the same path budget, [A §10.3] item 4).
- Audio: RED with 1 previous frame when `loss_ewma > 0.5 %`, 2 when > 3 %; Opus in-band FEC off (RED is cheaper at 10 ms frames); Sunshine's fixed 4+2 RS ([A §1.6]) is rejected as unadaptive.
- Interoperability: parity count is transmitted explicitly in `block_r`; the Moonshine floor/ceil mismatch class ([A §5.3]) cannot occur.

Today's XOR (1 per 8) is removed: it fails ~56 % of groups at 20 % independent loss and has no sequence numbers (`research/07` finding 6).

### 5.5 NACK / RTX

Receiver: on a gap in `seq` observed for ≥ `reorder_window` (2 ms or 2 packets) and while `now + rtt + 2 ms < deadline(frame)` for the affected frame, add the seq to the next FEEDBACK's NACK list (sent immediately, not at the 20 ms tick); a seq is NACKed at most twice. Sender: keeps Q7 (400 ms / 4 MiB), resends as `RTX` at priority above new video, with `send_ts` refreshed but the original `seq` retained. Video RTX is rare on LAN (FEC covers it) and valuable on 20–40 ms paths where one RTT still meets a 60 ms deadline; str0m's 33 ms NACK spacing is too slow for this ([A §7.2]).

### 5.6 Reference repair: LTR-ack recovery, IDR last

- **VideoToolbox:** `EnableLTR = true`; every frame that the encoder tags with `RequireLTRAcknowledgementToken` is sent with `flags.LTR`; the client acks fully decoded LTR frames in FEEDBACK; the host passes acked tokens back via `AcknowledgedLTRTokens` on the next encode. On an unrecoverable loss the client's stream state becomes `awaiting_ltr_refresh`; the host sets `ForceLTRRefresh = true` on the next frame, which VT predicts from an acknowledged LTR and marks `LTR_REFRESH`; the client resumes decoding at that frame. If no LTR is acknowledged (session start, > 2 s of loss), the host forces an IDR. This is the mechanism Apple HP uses (APP-packet LTR acks, [C §5.2]) and WWDC21 documents ([C §3.2]); it is the fix for the keyframe re-request storms in commits d815426/01ac41c ([C S5]).
- **NVENC:** `enableLTR`, mark every 8th frame LTR (2 LTR slots), same ack protocol; on loss `NvEncInvalidateRefFrames` for the lost range and, if a usable LTR is acked, force reference to it; IDR fallback only if the DPB has no survivor ([A §1.3], `research/03 §2`). Intra-refresh optional on long-RTT paths.
- **VAAPI / Vulkan Video:** explicit reference-list control (PixelForge's MMCO/RPS repair, [A §5.2]) with an LTR slot; where the driver cannot, IDR.
- **Client:** decoded frames may be dropped for pacing but every non-DISCARDABLE frame is decoded when its references are intact, even if it will never be presented (Moonlight's separation of decoding and displaying, [A §10.2]).
- Keyframe requests are coalesced host-side: at most one IDR per 500 ms per stream; a second request within the window is answered with the pending one.
- Temporal layers (`BaseLayerFrameRateFraction = 0.5`) make sender-side drops safe by construction: under congestion the pacer drops DISCARDABLE frames first ([C S6]).

### 5.7 Congestion and rate control coupled to the encoder

**Owner:** `cc::SunnaController`, one instance per connection, running on the host control task, updated on every FEEDBACK (20 ms) and on every local event (encoder output size, pacer drop).

**Signals:**
1. One-way delay gradient from the FEEDBACK arrival vectors (send_ts vs arrival offsets; the trendline/Kalman slope of GCC, `research/03 §4`), with the receiver's clock skew removed by the PROBE offset estimate.
2. Loss fraction and burstiness (§5.4).
3. Local: Q4/Q5 drops, quinn `datagram_send_buffer_space`, encoder over-budget frames (AU bytes vs target/fps).
4. Receiver: `decode_queue_depth`, `present_queue_depth`, decoded-vs-presented id gap (a slow client is not a slow network; g-r-d's decode-acked window is the model, [C S7]).
5. Application-limited detection: if the encoder produced < 70 % of the target for the last 500 ms (static screen), the estimate is **frozen**, not grown, and periodic **probe bursts** (PROBE padding, 100 ms at 1.5× current target every 2 s, paced) keep the estimate fresh so the first motion after a still period is not blurry (Selkies' "application-limited idle-screen throughput is not capacity", [A §6.4]).

**Control law (per feedback):**
- `delay_state ∈ {underuse, normal, overuse}` from the gradient with adaptive thresholds (GCC-style).
- overuse or loss > 2 %: `target = max(0.85 × measured_receive_rate, target × 0.8)`, once per RTT; hold 1 RTT.
- normal: `target += max(2 % × target, 200 kbit/s)` per feedback interval, capped by the user ceiling and by `1.2 × measured_receive_rate` unless probing.
- underuse (queue draining): hold.
- Receiver overload (decode depth ≥ 2 for 3 feedbacks): drop the **fps** (temporal layer off first, then 60→30) before bitrate; desktop mode prefers fewer sharper frames.
- Startup: relay-first connections start at 4 Mbit/s and ramp with a 1-second padded probe to the ceiling or the first overuse (no slow-start from 1 packet; the current code starts at `min(max, 15 Mbit/s)`, `host/src/lib.rs:113`, which congests thin paths for seconds).
- Floors: 1 Mbit/s desktop (tiles keep text readable), 3 Mbit/s game.
- Ceilings by path class: LAN 80 Mbit/s, WAN direct 40, relay 25 (relay budget), user-adjustable.

**Coupling:** the encoder reads `target_bitrate` (atomic) before each frame and applies `AverageBitRate` + `DataRateLimits`; the FEC ratio and `frame_budget_bytes = target/fps × 1.3` are read by the packetizer; a frame whose AU exceeds `2 × frame_budget` and is not a keyframe is sent anyway but the CC cuts the target immediately (encoder overshoot is a congestion signal).

**quinn adapter:** `cc::QuinnAdapter` implements `quinn_proto::congestion::Controller` and reports `window = target_bitrate × (srtt + 20 ms) × 1.5` and ignores QUIC-level loss events for datagram-only packets (it still honours them for stream packets). This makes quinn's pacer and window follow our decision instead of CUBIC's, which today underlies a 1 s app AIMD and fights it (`research/07` finding 5). Pacing itself is done by our sender (below); quinn's pacer is left enabled but effectively never binding (UNVERIFIED that quinn's pacer can be disabled outright; the adapter's large window makes it moot).

### 5.8 Pacing and the sender mux

The sender thread drains a priority mux at 1 ms ticks while there is work: `INPUT-ack/FEEDBACK > RTX > AUDIO > VIDEO base layer > VIDEO enhancement > PROBE`. A video frame is spread over `min(0.6 × frame_interval, 8 ms)` (2–5 ms per-frame pacing, `research/03 §3`), with a burst ceiling of 16 datagrams per `send_datagram` batch (GSO via quinn's `quinn-udp`). Sunshine's ~800 Mbit/s "pacing" is a burst limiter, not pacing ([A §1.4]); ours paces at the CC's rate. Tiles and bulk ride QUIC streams with `SendStream::set_priority` (tiles −1, bulk −2) so they yield to datagrams in quinn's scheduler; additionally the tile engine checks `cc.headroom()` and pauses when the path is saturated.

### 5.9 Streams, priorities, and flow control

- Control: priority 0. Tile streams: −1, one stream per batch, closed after the batch. Bulk: −2.
- quinn transport config: `max_idle_timeout = 30 s` (today 10 s: too short for a Wi-Fi drop we want to survive), `keep_alive_interval = 3 s` on the active path (Tailscale's heartbeat, [F §3.2]), `datagram_receive_buffer_size = 4 MiB`, `datagram_send_buffer_size = 256 KiB`, `initial_mtu = 1200`, `mtu_discovery` enabled with `upper_bound = 1450`, `stream_receive_window = 4 MiB`, `send_window = 8 MiB`, `max_concurrent_uni_streams = 64`.
- ALPN `sunna/1`; TLS 1.3 only; 0-RTT **disabled** (input replay).

### 5.10 Encryption

QUIC packet protection (AES-128-GCM / ChaCha20-Poly1305 negotiated) with **RFC 7250 raw public keys** on both sides (mutual authentication in the handshake; verifier checks the pinned peer key from pairing, iroh's `tls/verifier.rs` is the template, [F §3b.1], [F §6.1]). Relays and rendezvous see ciphertext only. Key update every 2^20 packets or 1 hour (QUIC key update). No application-layer crypto on media (Sunshine's per-shard AES-GCM and Wolf's repeated-nonce bug are avoided by construction, [A §1.4], [A §4.3]).

### 5.11 MTU handling

Video chunk size = `min(connection.max_datagram_size(), 1200) − 28` until PMTUD has validated a larger MTU for ≥ 10 s and the path is direct (not relay, not a tunnel interface: Tailscale's utun is MTU 1280 and Moonshine #210 shows oversized sends over WireGuard/iOS simply vanish, [A §8.2]). On any PMTU black-hole signal from quinn, fall back to 1200 for the session. Over relays the relay's framing overhead (4 bytes) is subtracted.

### 5.12 QUIC usage details (quinn customization)

- Upstream `quinn 0.11`, no fork. Customizations: (1) `AsyncUdpSocket` implementation `netpath::MappedSocket` that presents one stable fake address per peer (`fd15:70a:510b::/64`-style ULA, iroh's scheme, [F §3b.1]) and resolves it to the current real path on send / rewrites the source on receive, so QUIC never migrates when the path changes; (2) `congestion::Controller` adapter (§5.7); (3) `TransportConfig` above; (4) RPK verifiers; (5) `Endpoint::rebind` on interface changes.
- Datagram send: `send_datagram` (never `send_datagram_wait`; a full buffer is a CC event, [A §7.1]).
- Receive: one blocking socket thread with GRO, feeding quinn; datagrams dispatched by kind directly on that thread for VIDEO/AUDIO (reassembly is cheap), INPUT to the injector thread, FEEDBACK/PROBE to the CC.

### 5.13 Relay-aware behaviour

Path class is known to both sides (`netpath` reports `Lan | Direct | Relay`). On `Relay`: bitrate ceiling 25 Mbit/s, FEC `r_min = 1`, HEVC preferred, tiles allowed, probing at 1.25× not 1.5×, keep-alive 5 s to the relay. Direct-path upgrade (typically within 5 s, §7.4) is transparent to QUIC through the mapped socket; the CC resets its delay baseline on path change and ramps within 1 s. If a direct path dies (no heartbeat for 6.5 s, Tailscale's `trustUDPAddrDuration`), the mux dual-sends to the old address and the relay until one answers ([F §3.3]).
---

## 6. Quality system

### 6.1 Two modes, one pipeline

| | Desktop mode (default) | Game mode |
|---|---|---|
| Goal | text pixel-exact, colour-accurate, cursor instant | lowest latency, highest fps, HDR |
| Capture format | BGRA (tiles + 4:4:4 need RGB) | `420v`/`P010`/`64RGBAHalf` |
| Codec | HEVC (H.264 until measured); 4:4:4 if §3.1.3 succeeds | HEVC/AV1 4:2:0, Main10 for HDR |
| fps | display rate, but static = 0 fps; refinement passes when idle | 60/120, no static suppression (`minimumFrameInterval` only) |
| QP bounds | 12–34 | 16–40 |
| Static tiles | on | off |
| Region-masked apply | on | off |
| Temporal layers | on above 60 fps | on |
| Target latency | one-way + 1 frame + 5 ms | one-way + 0.5 frame + 3 ms |
| Presenter | display-link, windowed OK | arrival, fullscreen direct-to-display |
| Mouse | absolute, hardware cursor | relative (pointer lock), cursor hidden or host-drawn |
| Audio buffer | 30 ms | 20 ms |

### 6.2 Desktop mode: the three-layer picture

RDP's insight is that text must never depend on the video codec ([C §1.3]: ~80 % of session graphics is text, GPU-only H.264 "performs worse" on text). Sunna rebuilds RDP mixed mode on a modern transport ([C §1.5] design sketch, [C S1–S4]):

1. **Video layer:** hardware H.264/HEVC of the whole display, but the client applies decoded pixels **only inside the union of dirty rects** the host attaches per frame (`regionRects` semantics of AVC420, [C S2]). Static regions on the client are therefore never touched by codec drift or noise. The host sends the mask as a compact rect list in the frame's SOF chunk (`u8 n, n × {x u16, y u16, w u16, h u16}` in 8-px units, ≤ 64 rects, else "whole frame").
2. **Static layer (lossless tiles):** the tile engine tracks 64×64-pixel tiles (128 on 4K+) and a per-tile `stable_since` timestamp reset by every dirty rect. A tile that has been stable for **150 ms** and whose current lossless hash (xxh3-64 of the BGRA pixels) differs from what the client is known to hold is encoded losslessly and sent on the tile stream. Codec: our own planar+RLE+zstd (RDP Planar-style scanline delta then byte RLE, zstd level 1 over a batch; typical UI tiles compress 10–40×; UNVERIFIED ratio on Retina text, measure) with QOI as a fallback for photographic tiles (bounded: a tile whose lossless size exceeds 4× its video cost is skipped, the video is good enough for photos). Budget: tiles use at most `cc.headroom()` (the difference between the CC's estimate and the current video rate), so on a thin link refinement takes longer but never hurts motion. Client: an LRU tile cache keyed by hash (64 MiB), so re-shown content (window switch, scroll-back, tab switch) is replayed from cache without transfer, exactly RDP's `CACHE_TO_SURFACE` ([C §1.4]); the host keeps a mirror of the client's cache index (server-owned slots) to know what to skip.
3. **Scroll blits:** when SCK reports a large dirty rect and the previous frame's rows match the new frame's rows at a vertical offset (row-hash search within ±256 px, on the GPU or on a 1/8-scale luma copy), the host sends a `Blit { src_rect, dst_rect }` in the mask list; the client copies from its previous composite, and the video only carries the exposed strip. This is `SURFACE_TO_SURFACE` and DDA's move rects ([C S3]); on macOS the detection is ours because SCK has no move rects.

Composite order on the client: video (masked) → static tiles (override video where present and still valid) → blits → cursor. A tile is invalidated the moment a dirty rect intersects it, so a stale tile can never hide a live change.

**Refine when idle:** 60 ms after a region stops changing (g-r-d's `FRAME_UPGRADE_DELAY_US`, [C §2.5]) and before the tile pass at 150 ms, the encoder sends one **low-QP refinement frame** for the union of recently changed rects (`flags.REFINE`, region-masked): `MaxAllowedFrameQP = 18` for that frame. Between motion (QP ≤ 34), refinement (QP 18) and tiles (lossless) the picture converges in three visible steps within ~200 ms; HP has none of this because it is pure video ([C §5.4]).

**4:4:4 strategy (decision tree):** (a) HEVC RExt 4:4:4 via VideoToolbox if the §3.1.3 probe succeeds and the client can decode it (M-series, [C §5.2]); (b) otherwise 4:2:0 + tiles is the desktop mode, with **siting-correct chroma upsampling in the client shader** (H.264 default siting is left-centre; nearest is worse, [C §6.2]); (c) AVC444-style dual view (main 4:2:0 picture + aux picture carrying the remaining chroma, luma-now/chroma-later with `LC=1/2`, [C §1.2]) is **deferred and optional**: it doubles decoder work and needs separate reference management to avoid the alternating-chain penalty ([C A11]). I disagree with `research/07`'s "both, after lossless tiles": only build (c) if the probe fails *and* measurement shows chroma fringing on moving coloured text is a real complaint after tiles exist.

**Colour management:** capture in the display's colour space (P3 on Apple panels), tag the bitstream VUI (primaries P3-D65 / transfer sRGB / matrix 709), tag the client layer the same; tiles are raw display-space BGRA. sRGB clients viewing P3 hosts get a shader conversion. HDR is game-mode only in v1.

### 6.3 Game mode

- fps: match the client display (60/120); temporal layers on; `minimumFrameInterval = 1/120`; the CC prefers dropping the enhancement layer over lowering QP when overloaded.
- Latency: arrival presentation, fullscreen direct-to-display ([C §6.1]), pointer lock, no tiles, `target_latency` tighter.
- HDR: SCK `captureDynamicRange` HDR preset (15+) → `P010`/`64RGBAHalf` → HEVC Main10 with `TransferFunction = PQ`, `ColorPrimaries = BT.2020`, mastering/content-light metadata from the display; client EDR layer with HDR10 metadata for OS tone mapping (moonlight-qt's Metal path, [A §2.2]). Mac-host HDR fidelity is **UNVERIFIED** end-to-end ([A §1.2], [A §11]); it ships behind a flag until the Airs measure correctly (they are SDR panels, so the test needs an external HDR display; deferred).
- Audio: 20 ms buffer, no RED unless loss.
- AV1: NVENC/QSV/AMF AV1 on Windows/Linux hosts when the client decodes it (M1/M2 do **not** hardware-decode AV1, [A §2.2]; M3+ do); never 4:4:4 (no vendor supports AV1 4:4:4, `research/03 §2`).

### 6.4 Automatic mode switching

Signals: (1) a fullscreen app owns the captured display (SCK `SCShareableContent` window list / `NSWorkspace` frontmost app fullscreen state; Windows: `SHQueryUserNotificationState` == `QUNS_RUNNING_D3D_FULL_SCREEN`); (2) motion ratio: > 60 % of the display dirty for > 1 s; (3) a gamepad is active on the client; (4) the user's per-app override list. Rule: game mode when (1) and ((2) or (3)), or on explicit toggle; back to desktop 2 s after (1) clears. The switch is a `StreamConfig` change (new `codec_epoch` only if the pixel format changes), so it costs one clean cut, not a reconnect. Motion-heavy desktop content (video playback in a window) stays in desktop mode: tiles simply never stabilise there and the video layer does its job.

### 6.5 Multi-monitor

Each host display is a `stream` (0..15) with its own encoder, CC share (the CC allocates the connection's budget across streams by visible area × activity), tile engine and cursor. The client opens one window per display (or one fullscreen Space per display), each with a hardware cursor; `MOUSE_ABS` events carry the `stream`. Virtual displays: one per client window by default ("your laptop screen becomes the remote's main display"), plus "also show host display N" on demand; with two physical client monitors the host gets two virtual displays laid out like the client's. Layout changes are versioned by `display_epoch` and applied atomically on both ends. RustDesk's per-display service and `CaptureDisplays{add,sub,set}` is the right shape ([F §1B.8]); Apple HP's one-session, two-display cap is the limit to beat ([C §5.1]).

---

## 7. Connectivity

### 7.1 Build-vs-reuse decision

| Option | Verdict | Why |
|---|---|---|
| **Own stack on upstream quinn, copying the iroh/Tailscale magicsock design** | **Chosen** | Keeps upstream quinn (no fork drift), full control of relay semantics (UDP relay that carries media, not a TCP DERP), identity model ours; iroh is the reference implementation and a fallback if time runs out ([F §3b.4] row 3) |
| Embed iroh as-is | rejected for the core; kept as a documented fallback | Pulls `iroh-quinn` (fork), its relay is WebSocket/TCP (bad media fallback), identity types leak into ours ([F §3b.1]) |
| Tailscale tsnet / libtailscale | optional transport, never the core | Go runtime, identity tied to a tailnet, double encryption; but "bind to the Tailscale IP" stays the v1 dev path and a supported user option ([F §3.12]) |
| libjuice | rejected | Owns the socket; C in a Rust build; ICE conformance is not needed because both ends are ours ([F §3b.2]) |
| str0m ICE | rejected | No TURN, we need a relay anyway; sans-IO ICE brings little once we do our own pings ([F §3b.3]) |
| WebRTC (webrtc-rs / libwebrtc) | rejected for native; browser reach comes via WebTransport instead | 10–15 ms tax, opaque jitter buffer, no encoder control (`research/02`, `research/00 §2`) |

**Where I revise my own dossier:** [F §8.1] suggested using QUIC PATH_CHALLENGE/RESPONSE as the ping protocol. Upstream quinn 0.11 has no multipath and no NAT-traversal extension (iroh forked quinn for exactly this, [F §3b.1]), so Sunna uses a **separate authenticated "disco" ping** under the mapped socket instead, and revisits QUIC-native probing when upstream quinn gains multipath.

### 7.2 Identity and addressing

- Every device has an ed25519 key pair generated at first launch (`identity` crate); `device_id = base32(pubkey)` (52 chars) and a human label. The `sunna://<device_id>` URI is the only address ever needed. No MAC-derived IDs (RustDesk's ID collisions/`UUID_MISMATCH` renumbering, [F §1A.2]).
- Rendezvous records: `{pubkey, endpoints[], relay_home, caps, ts, sig}` signed by the device key; the rendezvous stores whatever the device signs, so a malicious rendezvous can only withhold, never substitute (dial-by-key, [F §6.2]).
- Accounts (optional, §8.3) map an account to a set of device pubkeys; devices still authenticate each other by pinned keys.

### 7.3 Rendezvous (`sunna-rendezvous`)

- Protocol: QUIC (same crate) with the rendezvous' own pinned key (shipped in the app; self-hosters paste theirs into a config or a `sunna://…?rv=<key>@host` link). Also STUN (RFC 5389 binding) on the same UDP port so every rendezvous and relay is a STUN server too.
- Messages: `Register { record }` every 25 s and on endpoint change (Tailscale's `endpointsFreshEnoughDuration = 27 s`, "NAT mappings typically expire at 30 s", [F §3.2]); `Lookup { pubkey } -> record | offline`; `CallMe { to, from_record, nonce }` forwarded to the target's registration connection (Tailscale's CallMeMaybe via DERP, [F §3.3]); `RelayToken { for }` for relay admission.
- No payload relaying, no accounts required; abuse limits: 20 registrations/min per key, 10 lookups/s per IP, 128 KiB pre-auth.
- Public instances run by the project; self-host with one binary and a DNS name. The client keeps a list (project + user-added) and registers with all.

### 7.4 Path establishment (state machine)

```
IDLE ──connect(pubkey)──▶ RESOLVING (rendezvous Lookup; DNS-SD LAN lookup in parallel)
   ├─ LAN record found ──▶ DIRECT_LAN attempt (QUIC handshake to LAN addr, 300 ms)
   └─ record ──▶ RELAY_FIRST: open QUIC via the target's relay_home (relay path is the initial real path
                    under the mapped address) ── handshake ── SESSION UP (t ≈ 1 RTT + relay RTT; target < 1 s)
                    │ in parallel: send CallMe with our candidates; on receipt of the peer's candidates, PUNCHING
PUNCHING: every 200 ms for 3 s, then every 5 s: disco Ping to every candidate {local, STUN-reflexive, port-mapped, v6};
          the peer does the same (simultaneous open through both NATs). Pong carries the observed source → learn own
          reflexive per destination (hard-NAT detection: mapping varies by dest IP, netcheck's rule, [F §3.5]).
   └─ Pong on a direct candidate ──▶ UPGRADE: MappedSocket switches the peer's real path to the lowest-RTT
          direct address (prefer IPv6 on ties, [F §3.3]); CC resets baseline; relay path kept warm (5 s keepalive)
STEADY: heartbeat disco ping every 3 s on the active path; trust 6.5 s; full re-probe every 60 s unless RTT ≤ 5 ms
   ├─ heartbeat lost 6.5 s ──▶ dual-send to last-good + relay until a Pong (never a stall, [F §3.3])
   └─ interface change (NWPathMonitor / netlink / NotifyAddrChange) ──▶ rebind sockets, re-STUN, PUNCHING again
```

Disco ping format: `magic "SNDC" (4) | sender pubkey (32) | nonce (24) | NaCl box { type u8 (1 ping / 2 pong / 3 callme), txid u96, observed_addr (pong), candidates (callme) }` with a per-pair disco key derived from the session (HKDF from the QUIC TLS exporter, so pings are bound to the authenticated session) — Tailscale's format ([F §3.4]) minus the separate disco identity, because we only ping peers we already have a QUIC session with (relay-first guarantees that).

Hard NAT: (1) IPv6 first; (2) port mapping PCP → NAT-PMP → UPnP with 250 ms probes and 2 h leases ([F §3.7]); (3) easy-side-pings-hard-side ordering via CallMe; (4) both hard → relay, then a **peer relay** later (Tailscale's 2025 addition, [F §3.9]). No birthday spraying (Tailscale's math, [F §3.6]).

### 7.5 Relays (`sunna-relay`)

- UDP on 3478 (shares STUN) and TCP/443 fallback (framed, for UDP-blocked networks; the media path degrades to TCP semantics there and the client says so).
- Auth: challenge/response with the device key (DERP/iroh style, [F §6.2]); a rendezvous-issued `RelayToken` allows a relay to be used by two named keys for a session; per-key rate limits (default 30 Mbit/s per session, 100 Mbit/s per key, configurable); no payload inspection possible (QUIC ciphertext).
- Framing on the wire: `[u32 session_id][u8 flags][QUIC packet]`; the relay pairs two authenticated bindings into a session and copies datagrams; per-client queue Q15 (64 datagrams / 20 ms, drop oldest).
- Hosted: the project runs relays in ≥ 3 regions; clients pick by latency (STUN RTT). Self-hosted: `sunna-relay --key … --listen :3478` and the pubkey pasted into either side (or served by a self-hosted rendezvous). **The relay is the reliability feature, not the upsell** ([F §8.2]: Parsec gates it behind $45/user/mo).
- Peer relay (later): any paired device with a good NAT can relay for its owner's other devices (Tailscale kb/1411 model, [F §3.9]).

### 7.6 LAN discovery, IPv6, direct IP, Wake-on-LAN

- DNS-SD `_sunna._udp.local` with TXT `k=<pubkey> n=<name> v=<proto>`; the client shows LAN peers instantly and prefers the LAN path even when the rendezvous is unreachable (Parsec's "bricks without auth servers" is the failure to avoid, `research/04 §2`).
- IPv6: both socket families bound; candidates include global v6; ties go to v6.
- Direct IP: `sunna://<pubkey>@host:port` for users who port-forward; the key is still required (no IP-only trust).
- Wake-on-LAN: a paired LAN device (or the relay via a paired peer) sends the magic packet for a sleeping host whose MAC is recorded at pairing; "Wake for network access" on macOS is documented in onboarding.

### 7.7 Mobile networks

CGNAT is the common case: relay-first gives a working session in < 1 s; punching usually fails on both-hard pairs, so the session stays on the relay (25 Mbit/s ceiling) unless a peer relay or the home side has IPv6. Wi-Fi ↔ cellular: `NWPathMonitor` triggers `rebind` + re-STUN; QUIC never migrates because the mapped address is stable; the CC re-probes. Battery: heartbeats 3 s only while a session is active, 45 s idle timeout on paths (Tailscale's `sessionActiveTimeout`, [F §3.2]).

### 7.8 Connection diagnostics (what the user sees)

The status line shows: path (`LAN`, `Direct v6`, `Direct v4`, `Relay eu-1`, `Relay (TCP)`), RTT, loss, send/receive rate, and a one-line reason when not direct: "Both sides are behind symmetric NAT; enable UPnP/PCP on either router or IPv6", "UDP is blocked on this network; using TCP relay (video quality reduced)". `sunna-cli doctor` prints the full netcheck (NAT type per family, mapping variance, STUN RTTs, port-mapping availability, relay latencies, captive portal) — Tailscale's `netcheck` report fields ([F §3.5]).
---

## 8. Security and trust

### 8.1 Device identity and E2E guarantees

- ed25519 device key in the OS keystore (Keychain with `kSecAttrAccessibleAfterFirstUnlock` for the broker; libsecret/TPM-sealed file on Linux (g-r-d's credential backends, [C §2.1]); DPAPI on Windows). Rotation = re-pair.
- QUIC/TLS 1.3 with mutual RFC 7250 raw public keys (§5.10). The client verifier accepts only the pinned key of the machine it dialled; the host verifier accepts only keys in its peer store. **No CA, no server-signed identities** (RustDesk's hbbs-signed pk and Parsec's brokered keys make the server the trust root, [F §1A.5], [F §4.5]).
- Consequence: end-to-end confidentiality and authenticity against the rendezvous, the relay, and a passive or active network attacker. The rendezvous learns pubkeys, endpoints and online state (as Tailscale control does); the relay learns pubkeys and traffic shape.
- Forward secrecy per session from TLS 1.3; key update hourly.

### 8.2 Pairing flows

Pairing binds two device keys with mutual pinning. It is always **initiated on the host** (the machine that will be controlled), never by typing a stranger-supplied ID into it ([F §6.5] lesson 2).

**Flow A — code (default):** the host's Sunna window shows a 6-character code from a 32-symbol alphabet (`ABCDEFGHJKLMNPQRSTUVWXYZ23456789`, ~30 bits) and its `device_id` is published to the rendezvous with `pairing_open = true` for 120 s. The client user types the code (or picks the host from the LAN list and types the code). Both run **CPace** (the IETF balanced PAKE; SPAKE2 is the fallback if a vetted Rust CPace crate is not available, UNVERIFIED crate maturity; Chrome Remote Desktop uses SPAKE2 for the same reason, [F §6.1]) over a pre-pairing QUIC connection made with **ephemeral** keys; the PAKE output authenticates both long-term keys and the transcript. One guess per attempt, 3 attempts per code, then a new code; the host rate-limits pairing attempts to 5/min/IP. A 30-bit code with 3 online guesses is safe; the code never leaves the two machines except as a PAKE message.

**Flow B — QR / link:** the host shows a QR encoding `sunna://<device_id>?p=<code>&rv=<rendezvous>`; the client scans it (iPad/phone) or opens the link (desktop app registered for the scheme); same PAKE underneath, zero typing.

**Flow C — account sync (optional):** if both machines are signed into the same account, the account server distributes the *device list* and each device's pubkey **signed by a per-account key that lives only on the user's devices** (the server stores the signed blob; it cannot mint devices — Tailscale's tailnet-lock property, [F §6.1]). First login on a new device still needs one code from an existing device. This gives Parsec's "computers just appear" ([F §4.6]) without making the account server a trust root.

**Flow D — guest link:** a paired owner device mints a time-limited invitation (`sunna://<host_id>?g=<token>`, token = signed capability listing permissions and expiry, 24 h default) and sends it by any channel; the guest's device presents the token; the host owner sees an Accept dialog with the guest's name and permission set (attended by default). Used for co-play and one-off help (`research/04` gap #5).

Pairing produces a **peer record** on both sides: `{pubkey, name, role (owner | guest), permissions, paired_at, paired_via, unattended: bool, last_seen}`. Unpair on either side revokes immediately (the host closes any live session for that key).

### 8.3 Account-optional model

Everything works without an account: pairing, LAN, WAN via the public relays, self-hosted infrastructure. An account adds device-list sync (Flow C), naming, and, for teams, a management plane (later). The account server never sees session keys; it stores signed device-list blobs and relay tokens. Losing the account server degrades to "the list doesn't sync"; never to "cannot connect" (`research/04 §2`, Parsec).

### 8.4 Permissions per peer

Capability bits stored per peer record and enforced in the agent on every event (Apollo's per-client permission mask is the model, [A §3.3]): `view`, `control_pointer`, `control_keyboard`, `gamepad`, `audio_out`, `audio_in` (mic, later), `clipboard_read`, `clipboard_write`, `file_get`, `file_put`, `launch_apps`, `login_window`, `unattended`, `change_display_layout`. Owner devices default to all except `login_window` and `unattended` (explicit opt-in); guests default to `view + gamepad` (Parsec's guest ladder, [F §4.2]), upgradable per session by the host user clicking the guest's avatar. Revocation is live: losing `view` disconnects the peer.

### 8.5 Unattended access and consent UX

- Default: **attended**. A connection from a paired device shows a host-side dialog "<name> wants to control this Mac" with Accept / Deny / "Always allow this device" (the last sets `unattended` for that peer), 60 s timeout → deny.
- `unattended` is per peer, visible as a list in the host UI under "Devices that can connect any time", with a menu-bar indicator while hosting is enabled and a persistent banner while any device has unattended access. No hidden mode, no silent install.
- During a session: a small always-on-top overlay on the host ("Controlled by <name> · Cmd+Option+Shift+Esc to disconnect") excluded from capture via the SCK filter; a local user's physical input is never suppressed.
- Login-window access is a separate permission and a separate host toggle.

### 8.6 Abuse and scam mitigation

- Pairing is host-initiated with a short-lived code; the scammer's script "read me your ID, type this password" has no equivalent.
- **Scam interstitial:** the first inbound pairing or session on a host installed < 24 h ago, or from a never-seen peer, shows a full-screen warning ("If someone you don't know asked you to install this, stop") with a 10 s countdown before Accept is enabled (RustDesk's Android countdown generalized, [F §2.6]; Microsoft's Quick Assist warnings, [F §6.5]).
- Rate limits: 64 unauthenticated connections total, 16 per IP, 128 KiB pre-auth (RustDesk's numbers, [F §1B.12]); exponential backoff on failed PAKE attempts.
- Relays and rendezvous require authenticated keys and enforce per-key quotas so Sunna cannot become an open reflector ([F §6.2]).
- No config import from the command line without an admin prompt; no executable-name-based configuration (RustDesk's MSP trick, [F §1A.7]).

### 8.7 Audit log

Append-only local log on the host (`~/Library/Logs/Sunna/audit.jsonl` / `/var/log/sunna/audit.jsonl`), rotated: pairing events, session start/end with peer key, path type, permissions used, clipboard/file transfers (metadata only), permission changes, updates applied. Optional forwarding to the team management plane. Shown in the host UI as "Recent connections".

### 8.8 Update security

- macOS: Sparkle-style appcast signed with an **EdDSA key independent of the Developer ID certificate** (AnyDesk's leaked signing key is the cautionary tale, [F §6.5]); the broker (root) installs updates atomically (rename-swap), restarts agents when the GUI session is idle, keeps the previous version for rollback ([F §7.4]). Staged rollout by device-key hash buckets (1 % → 10 % → 100 %).
- Windows: Azure Trusted Signing for binaries ([F §7.2]) + the same signed manifest; MSI in-place upgrade.
- Linux: distro packages carry their own signing; the in-app updater is off for packaged installs.
- Relay/rendezvous keys can be rotated by shipping new keys in an update and honouring both for 90 days.

---

## 9. Ease of use

### 9.1 First-run flows, step by step

Definitions as in [F §5]: steps counted from nothing installed to controlling the remote machine from a different network; "prompt" = OS permission/security dialog; "transcription" = something typed from one machine to the other.

**macOS host (4 steps, 3 prompts, 0 transcriptions):**
1. Download `Sunna.dmg`, drag to Applications, open (signed + notarised: no Gatekeeper warning).
2. Click **Allow connections to this Mac**. One admin prompt registers the daemon and agent (`SMAppService`). The permissions page appears with three rows and live status dots: *Screen & System Audio Recording* → **Grant** (system prompt → System Settings toggle; Sunna relaunches the agent automatically and the dot turns green within 1 s of the toggle), *Accessibility* → **Grant**, *System Audio* → **Grant** (only if audio is on). Each row explains what breaks without it.
3. The window now shows "This Mac is reachable as **Studio**" and a **Pair a device** button that displays the 6-character code and a QR. Nothing else to configure: rendezvous registration, STUN, port mapping and the relay path are already up (status dot: "Reachable: direct/relay").
4. (Optional, later) "Always available" toggle explains "Wake for network access" and lid/power requirements.

**macOS client (3 steps, 0 prompts, 1 transcription):**
1. Install and open Sunna.
2. **Add a machine** → type the code (or scan the QR with the iPad app, or the machine is already listed via LAN discovery and asks only for the code).
3. Click the machine. First frame in < 1 s; the host user sees the Accept dialog (attended) or nothing (unattended owner device).

**Linux host (deb/rpm, 4 steps, 1–2 prompts):** install package (polkit prompt) → `sunna` app or `sunnactl enable-hosting` → on GNOME/KDE nothing else is asked (private compositor API, agent is trusted); on other compositors one portal dialog with "remember" (persist_mode 2) → pair. Login-screen hosting asks once at install ("Allow connections at the login screen: installs a privileged capture helper").

**Windows host (later, 4 steps, 1 prompt):** installer (UAC) → Allow connections → (driver install for virtual display/gamepad happens inside the same UAC-elevated step) → pair.

**Steps-to-first-session versus [F §5.7]:** Sunna 7 total (4 + 3), 3–4 prompts, 1 transcription, no account, works across NAT, unattended optional, ≈ 3 minutes. Parsec Mac host: ~11; RustDesk: 9; Moonlight+Sunshine: 8–11; HP: 5–7 + VPN.

### 9.2 Making permission onboarding painless (macOS specifics)

- One page, all permissions, polled every 500 ms (`CGPreflightScreenCaptureAccess`, `CGPreflightPostEventAccess`, a tap probe), never serial cards (RustDesk's four cards + manual re-check, [F §1B.9]).
- The agent (not the app) is the TCC client; the page says "granting to Sunna Agent" and deep-links to the exact System Settings pane (`x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture`).
- Relaunch-required state is detected and handled (Screen Recording is cached per process; the broker restarts the agent).
- Re-approval (Sequoia monthly) is detected on the host and **pushed to paired clients** ("Studio needs you to re-approve screen recording; here's how") so the user learns before they need the machine, not when they are away ([F §2.2], [C §3.3]); the entitlement request to Apple removes it for good.
- `tccutil reset` instructions are shown only in the diagnostics page, never as a first-line fix.

### 9.3 Sharing and guest links

`Share access…` on a host or an owner device creates a guest link (Flow D) with a permission preset ("Watch", "Play with a controller", "Full control"), an expiry and an optional single-use flag. The link opens the desktop app if installed, otherwise the web client (phase 4), otherwise shows install instructions with the link preserved. Steam Remote Play Together's zero-account guest is the bar ([F §5.6]).

### 9.4 Failure diagnostics

Every failure has a code, a one-line cause, and a next action, surfaced in the UI and in `sunna-cli doctor`:

| Situation | What the user sees |
|---|---|
| host offline at rendezvous | "Studio hasn't checked in for 3 min. Last seen from home Wi-Fi. Wake-on-LAN: [Try]" |
| relay only | path badge "Relay" + reason (NAT types on both sides) + what would fix it |
| UDP blocked | "This network blocks UDP; using TCP relay, expect reduced quality" |
| TCC revoked mid-session | client: "Studio's screen recording permission was revoked; re-grant in System Settings on the host" |
| display asleep / lid closed | "Studio's display is asleep (lid closed on battery?)" |
| decoder unsupported | "Your device cannot decode HEVC 4:4:4; switched to H.264" (automatic) |
| thermal throttling | overlay tag + "fps capped due to host temperature" |

The overlay (§11.2) is one keystroke away in every session, and "Copy diagnostics" produces a JSON blob with the last 60 s of stage timestamps, path history and CC state.

---

## 10. Session model

### 10.1 Objects and state machines

- **Peer** (paired device) → **Session** (one QUIC connection, one `session_token`, one peer, one permission set) → **Streams** (per display) + audio + input + tiles.
- Host session states: `Admitting → Consent(attended) → Active(desktop|game) → Suspended(display sleep | agent handover) → Closed(reason)`. Client: `Resolving → Connecting(relay|direct) → Handshake → FirstFrame → Streaming → Reconnecting → Closed`.
- Per-stream recovery states (client): `ok | awaiting_ltr_refresh | awaiting_keyframe`, reported in FEEDBACK (§5.2).

### 10.2 Multi-viewer

A host runs one agent per GUI session and one **encoder context per viewer** by default (each viewer gets its own resolution, bitrate and recovery state; RustDesk's per-connection fan-out of one encode makes the slowest viewer hurt everyone, [F §1B.4]; Selkies' shared-display CC couples viewers, [A §6.4]). Capture is shared. Optional "shared encode" mode for many low-bandwidth viewers (classroom) where all get the same stream and the CC follows the slowest, explicitly chosen. Control ownership: exactly one **controller** per stream at a time (owner devices pre-empt guests); others are viewers, except gamepad input which is per-slot and concurrent (co-play).

### 10.3 Co-play and gamepads

Each guest gets a **gamepad slot** (1–4); the host creates a virtual gamepad per active slot (Linux uinput profiles; Windows ViGEm-successor; macOS: **none in v1** — there is no supported virtual game controller on macOS ([A §1.5], [A §3.3]); the roadmap lists a DriverKit HID investigation as UNVERIFIED feasibility). Rumble/LED feedback flows host→client on `GAMEPAD_FB` datagrams with its own flow control. Guests default to gamepad-only; the host user can grant keyboard/mouse. Per-session audio for guests is the same stream as the owner's.

### 10.4 App launching

Optional host-side app list (Sunshine/Apollo's launch model, [A §1.7], [A §3.1]): `apps.toml` with name, command, pre/post hooks, per-app permissions; launching an app is a control message with `launch_apps` permission; the host runs it in the agent's session and, in game mode, may put the virtual display into the app's requested mode. "Desktop" is always the first entry. Not before phase 3.

### 10.5 Resolution follow

The client window's drawable size (points × scale) drives the host's virtual display mode. Debounce 300 ms; during the drag the client scales; on commit the host applies the mode, SCK `updateConfiguration`, the encoder starts a new `codec_epoch` at the new size with one IDR, and the client swaps decoders on the first new AU. Physical-display hosts without a virtual display scale in the encoder (`SCStreamConfiguration.width/height`) instead, keeping 1:1 whenever the client is at least as large as the host. HP's per-resize encoder restart with visible hitch ([C §5.2]) becomes one clean cut.

### 10.6 Reconnection semantics

- The `session_token` (32 bytes, issued at admission, valid 10 min after loss) lets a client resume without a new consent dialog or re-pairing.
- Path loss: dual-send + relay fallback (§5.13) handles most Wi-Fi blips without the QUIC connection dying (idle timeout 30 s).
- Connection loss (host reboot, agent handover at login, > 30 s outage): the client reconnects with the token; the host answers with the current `display_epoch`/`codec_epoch` and an IDR; input state is reset (`RELEASE_ALL` both sides); the tile cache survives (hashes are content-addressed, so reconnect replays static content from cache instantly).
- Agent handover (login window → user session): broker passes the socket to the new agent, closes the old connection with `SUNNA_HANDOVER`, the client reconnects with the token; target < 1 s, no dialog.
- Client-side: on reconnect, the last presented frame stays on screen with a "Reconnecting…" badge; never a grey screen.

### 10.7 Clipboard and files in sessions

Lazy clipboard both directions per §3.1.7 with `clipboard_read/write` permissions; a "clipboard sync" toggle in the session toolbar; file copy-paste via bulk streams with progress, resumable by content hash (RustDesk's job/digest model, [F §1B.6]).
---

## 11. Observability and measurement

### 11.1 Stage timestamps (the pipeline carries them end to end)

Every frame carries a `FrameTrace` record through the host and the client; the host's part is sent in the SOF chunk's optional trailer (16 bytes) so the client can compute per-stage numbers with the clock offset from PROBE. All timestamps are monotonic microseconds (`CLOCK_MONOTONIC_RAW` / `mach_continuous_time`), never wall clock (`research/07` finding 9; today `sunna_proto::now_us` is `SystemTime`, `research/06 §6`).

| Stage | Timestamp | Source |
|---|---|---|
| T0 `display` | compositor refresh that produced the frame | SCK `displayTime` / DDA QPC / PipeWire `SPA_META_Header.pts` |
| T1 `captured` | callback entry | host |
| T2 `encode_start` / T3 `encode_done` | around VT/NVENC | host |
| T4 `first_sent` / T5 `last_sent` | pacer | host |
| T6 `first_recv` / T7 `complete` (after FEC/NACK) | client socket thread / reassembler | client |
| T8 `decode_start` / T9 `decode_done` | decode thread | client |
| T10 `submitted` | present thread | client |
| T11 `presented` | `MTLDrawable.presentedTime` / Wayland `presentation-time` / DXGI frame statistics | client |

Derived per frame: capture age at encode (T2−T0), encode (T3−T2), pacing (T5−T4), network one-way (T6−T5 + offset), repair wait (T7−T6), decode (T9−T8), present wait (T11−T10), **glass-to-glass (T11−T0)**. Aggregates: p50/p95/p99 over 1 s and 10 s windows, longest freeze (max gap between T11s), freeze count > 100 ms, unique frames presented per second (repeated presents do not count; Wolf #502's lesson, [A §8.2]).

Input has the same treatment: `event_seq` timestamps at client capture, INPUT send, host receive, injection; the host echoes `injected_ts` per `event_seq` in FEEDBACK so the client can compute input one-way delay.

### 11.2 Latency overlay

A one-keystroke overlay in the client (Moonlight's is the reference for honesty, [A §2.4]; Parsec's for explaining network state, [F §4.6]):

```
Studio · HEVC 2560×1664 · 58 fps unique (60 sent) · 18.4 Mbit/s (+1.1 FEC, +0.3 tiles)
path Direct v6 · RTT 4.1 ms · loss 0.0 % · CC normal · target 22 Mbit/s
glass-to-glass p50 21.3  p95 27.9  p99 34.0 ms   [capture 1.2 | encode 3.4 | pace 2.1 | net 2.8 | repair 0 | decode 2.9 | present 8.9]
input one-way p50 1.9 ms · freezes 0 · LTR acked 3 frames ago · tiles 94 % converged
```

Colours: green/amber/red per field against the mode's budget; a red field names the cause ("present: windowed compositor path, go fullscreen").

### 11.3 Benchmark harness

- `sunna-cli bench` (exists) becomes the in-process loopback benchmark with the `sim` crate: synthetic or recorded-screen sources, real codecs, and a network model (loss i.i.d. and Gilbert–Elliott bursts, delay + jitter distributions, bandwidth caps, reordering, MTU). Every PR runs it in CI (§12.5) and fails on regressions of p95 glass-to-glass or freeze count against a stored baseline.
- `sunna-cli flash`: the click-to-photon mode: the host runs a full-screen test page that flips black↔white on mousedown/keydown; the client records T11 for the flip frame; with the photodiode rig the external measurement is compared against the in-pipeline one to calibrate the overlay's "present" stage (`research/07` step 1).
- `sunna-cli replay`: replays a recorded capture (IOSurface sequence with dirty rects) through the host pipeline for deterministic codec/tile experiments.
- Recorded scenes: `text-scroll` (editor scrolling), `terminal-typing`, `web-browsing`, `video-window` (30 fps video in a window), `game-60`, `game-120`, `static-desktop` (idle 30 s), `resize`, `tab-switch` (tile cache replay).

### 11.4 Network simulation

- Linux box in the middle (`tc qdisc netem` on the VM: delay 20±5 ms, 1 % loss, burst loss via `netem loss gemodel`, rate 20 Mbit/s with `tbf`) for real-socket tests between the Airs; macOS Network Link Conditioner for single-machine tests; the `sim` crate for CI.
- Standard profiles: `lan-wired`, `lan-wifi` (2–10 ms jitter, 0.1 % loss, AWDL-style 60 ms stalls every 1 s optional), `wan-good` (20 ms, 0.1 %), `wan-lossy` (40 ms ± 10, 1 % bursty), `relay` (60 ms), `tunnel-1280` (MTU 1280), `cgnat-tcp` (TCP relay semantics), `overload-decoder` (client decode slowed 3×).

### 11.5 Competitive test protocol

Same machines (M1 Air host, M2 Air viewer, then swapped), same test app, same virtual-display size and HiDPI (1470×956@2× and 1920×1080@2×), 60 Hz, AC power, Low Power Mode off, same Wi-Fi band/channel/distance, AWDL and Bluetooth state recorded, lid open, no other traffic; ABAB alternation of products per condition; ≥ 200 trials per condition with the USB-HID + photodiode rig (fallback: 240 fps iPhone, ≥ 30 trials) — [C notes 05 §5.5] verbatim as the procedure.

Conditions: wired LAN (USB-C Ethernet), Wi-Fi LAN, Tailscale-direct, Tailscale-DERP, netem `wan-lossy`, plus a 2 s Wi-Fi drop and a window resize.

Products and configurations recorded by version: Apple Screen Sharing HP (26.6.1+, one display, HP mode), Parsec (free tier defaults + 4:4:4 if available on the Mac host), Moonlight-qt 6.1 + Sunshine current release on the Mac host (and the tuned config the community recommends), RustDesk 1.5 (Best quality, H.265 preference), Sunna.

Metrics per condition: click-to-photon median/p95/p99/max (remote minus local baseline), % trials > 100 ms, unique-frame cadence and repeated-frame count over 10 s of a 60 fps counter, bitrate on the wire (tcpdump on utun/en0), text fidelity (screenshot diff of a coloured-text page vs host: fraction of pixels exact, mean ΔE), resilience (time to clean image after loss/drop, session survival), resize hitch (ms of stale/blank), time-to-first-frame, CPU/GPU/energy on both machines (powermetrics). Raw CSVs and the rig description are published with every release ([C S20]: no public HP numbers exist; ours are the marketing asset).

---

## 12. Engineering

### 12.1 Languages

- **Rust** for everything that moves bytes or pixels: proto, media, cc, transport, netpath, identity, host, client, tiles, audio, capture/codec/input/display glue, relay, rendezvous, CLI. Reasons unchanged from `research/06 §6` (memory safety on a network-facing service, quinn/rustls/objc2/windows-rs ecosystem, RustDesk precedent).
- **Swift** where Apple APIs are Swift-first or where ObjC bridging from Rust is more fragile than the code it wraps: ScreenCaptureKit (`SCStream` delegate, `SCContentFilter`), `CGVirtualDisplay` (private ObjC classes), `SMAppService`, XPC service definitions, `NSPasteboard` privacy APIs, `CAMetalDisplayLink`, `GameController`, the SwiftUI shell. The Swift code is thin, exposes a C ABI (`@_cdecl`) consumed by Rust, and never touches pixels beyond passing `IOSurfaceRef`/`CVPixelBufferRef` handles. VideoToolbox stays in Rust (the hand-rolled FFI in `crates/codec/src/videotoolbox/ffi.rs` works and is hardware-tested).
- **C/C++** only as vendor SDK glue: NVENC (`nvEncodeAPI.h` via bindgen), AMF, libvpl headers; libei via `reis` (pure Rust); no FFmpeg anywhere on the hot path (we call vendor APIs directly, `research/00 §4`; FFmpeg-free also avoids the GPL/LGPL packaging tangle on macOS).
- **Kotlin/Swift** mobile shells (phase 4); **TypeScript** for the web client and the Tauri shells.

### 12.2 UI toolkit

- **macOS: SwiftUI menu-bar app + windows** (disagreeing with `research/06 §1`'s Tauri-everywhere): the flagship host must feel native (Settings-style permission wizard, Login Items integration, menu-bar status, Accept dialogs, overlay windows excluded from capture), and Swift is already required for SCK/CGVirtualDisplay/SMAppService/XPC, so a webview shell would add a runtime without removing any Swift. The stream window is an `NSWindow` with a `CAMetalLayer`-backed view created by the engine.
- **Linux and Windows: Tauri** shell over the same engine (webview for the panel, native winit window for the stream), when those hosts/clients ship (phase 2/3). Known weak spot WebKitGTK on some distros; egui is the fallback if it bites.
- Rule kept: the shell never touches frames or hot-path input (`research/06 §1`).

### 12.3 FFI strategy

- Rust ↔ Swift: a single C header generated by `cbindgen` for the engine (`sunna_engine_*` functions, opaque handles, callbacks for frames/events) plus Swift `@_cdecl` entry points for the Swift-side services (capture, display, pasteboard). No UniFFI on macOS (the surface is small and performance-critical); UniFFI for the mobile shells where the surface is UI-shaped.
- Rust ↔ ObjC runtime: `objc2` + `block2` (already used for CGDisplayStream's handler, `capture/src/macos.rs:19`) for the few ObjC calls that stay in Rust (NSCursor polling, CGEvent sources).
- Rust ↔ vendor SDKs: `bindgen` at build time against vendored headers; each backend behind a cargo feature and a runtime probe.
- Process boundaries: XPC (macOS), Unix sockets + `SCM_RIGHTS` (Linux), named pipes + `DuplicateHandle` (Windows), with one `ipc` crate defining the messages (postcard) and the fd-passing helpers.

### 12.4 Crate and module boundaries (what may depend on what)

```
proto ← media ← host/client          (media knows formats, not sockets)
cc     ← host                        (cc is pure: inputs are numbers, outputs are targets; testable without a network)
transport ← netpath                  (netpath owns real sockets and paths; transport owns QUIC; the MappedSocket is the seam)
identity ← transport (verifiers), rendezvous, host, client
capture/codec/display/input/audio/cursor/clipboard: platform traits + per-OS impls; depend only on proto types
tiles ← host, client                 (pure functions over pixel buffers + a cache)
present ← client
telemetry ← everything (types only); sim ← tests
apps depend on host/client/ipc/identity; never on platform crates directly
```

Hard rules enforced by `cargo deny`/CI: no `std::thread::sleep` in hot-path crates (clippy `disallowed_methods`), no wall-clock in telemetry, no `unwrap` in the agent's session code, `unsafe` confined to `*-ffi` modules with `// SAFETY:` comments.

### 12.5 Testing

- **Unit:** proto round-trips, RS-FEC recovery matrices (every k/r, every erasure pattern up to r), reassembler under reordering/duplication/deadline, CC control law against recorded feedback traces, tile hashing/invalidation, input coalescing, PAKE vectors.
- **Simulation:** `sim` crate drives host + client in-process with the network models of §11.4; property tests (proptest) that no P-frame is ever decoded against a missing reference and that no queue exceeds its bound; golden latency baselines per profile.
- **Hardware-in-the-loop on the owner's Macs:** a self-hosted GitHub Actions runner on the M1 Air (`macos-arm64`, tags `hil`) runs: VT encode/decode round-trips (`codec/tests/vt_roundtrip.rs` exists), the SCK capture smoke test (requires the pre-granted TCC for the runner's agent bundle), the loopback bench, and nightly the two-Air LAN bench with the M2 as the client (`sunna-cli bench --remote m2.local`), publishing overlay aggregates as artifacts. The Linux VM runs netem and the Linux host tests; type-checking of the macOS code on Linux stays via the rustup + Zig wrapper (memory note `dev-env-cross-check.md`).
- **Interop/regression:** recorded scenes replayed through every codec backend; screenshot diffs for tile convergence (byte-identical within 1 s on `static-desktop`).
- **Security:** fuzzing of datagram parsers and the control decoder (`cargo fuzz`), PAKE and verifier tests with wrong keys, relay auth abuse tests.

### 12.6 CI and packaging

- GitHub Actions: Linux runners (fmt, clippy, unit, sim, fuzz smoke, cross-type-check for macOS/Windows targets), the `hil` Mac runner (above), Windows runner for the Windows crates once they exist.
- macOS packaging: `Sunna.app` (Developer ID, hardened runtime, notarised with `notarytool`, stapled DMG) embedding `sunnad`, `Sunna Agent.app`, plists; Sparkle-style updater with an independent EdDSA key; Homebrew cask as the secondary channel. Entitlements: `com.apple.security.device.audio-input` (mic later), `com.apple.developer.persistent-content-capture` when granted. Not sandboxed (CGEventPost + daemons; direct distribution only, [F §7.1]).
- Linux: deb/rpm with systemd units + udev rule + the capability helper (0750, group `sunna`); AppImage client; Flatpak client (host reduced).
- Windows (phase 3): MSI (WiX) with service, drivers, WinGet manifest, Azure Trusted Signing.
- Infrastructure: `sunna-relay` and `sunna-rendezvous` as static binaries + container images + a `docker-compose.yml`; the public instances deployed from the same artifacts.
---

## 13. Roadmap

Assumption: one developer plus AI agents; each phase is a set of PR-sized steps with acceptance tests; the measurement rig runs after every step from phase 0 step 2 onward. Durations are estimates, not commitments. The order preserves `research/07`'s intent (beat HP Mac-to-Mac first) but moves LTR and the transport rewrite earlier and pushes 4:4:4 later, for the reasons in §5.6 and §6.2.

### Phase 0 — "Honest and correct" (from the current tree; ~3 weeks)

0. Land the working tree: the sender-drop wire-id fix, split control reader, `CGEventCreateScrollWheelEvent2`, retained-IOSurface path (`git status` shows these in progress).
1. **Half-day experiments on both Airs** (block everything else): VT HEVC 4:4:4 profile probe (§3.1.3); VT `EnableLTR`/`ForceLTRRefresh`/`AcknowledgedLTRTokens` behaviour and token semantics; `BaseLayerFrameRateFraction` support and `IsDependedOnByOthers` attachments; `DataRateLimits` on HEVC; CGVirtualDisplay + SCK on macOS 15/26 (frames arrive? clamshell?); `MaxAllowedFrameQP` presence. Record results in `research/08-vt-probe.md`.
2. Monotonic per-stage timestamps + `FrameTrace`, freeze accounting, `sunna-cli flash`, rig procedure written, **baseline HP vs Sunna** on the Airs over Wi-Fi and Tailscale-direct (§11.5 subset).
3. SCK capture (BGRA, dirty rects, `displayTime`, `.idle` handling, static = no encode) replacing CGDisplayStream; full VT property set with warn-logging; async encode with one frame in flight.
4. Client presenter: decode to IOSurface NV12, `CVMetalTextureCache`, `CAMetalLayer` on a present thread, display-link and arrival modes, `presentedTime`, bicubic scaling.
   *Exit:* capture→encode p50 ≤ 6 ms; decode→present p50 ≤ 3 ms; static screen < 50 kbit/s; no CPU pixel copies; `flash` rig produces ≥ 200-trial distributions; baseline numbers for HP recorded.

### Phase 1 — "Never stalls" (transport v1; ~4 weeks)

5. Datagram formats of §5.2 (VIDEO/FEC/AUDIO/INPUT/FEEDBACK/CURSOR/PROBE), RS-FEC blocks, reorder-tolerant reassembler with deadlines, NACK/RTX with the 400 ms history, input on datagrams with redundancy, FEEDBACK every 20 ms.
6. `cc::SunnaController` + quinn `Controller` adapter + sender pacer/mux; application-limited probing; per-path ceilings.
7. LTR recovery on VT (from the phase-0 probe results); temporal layer drops; IDR coalescing.
8. Client-side cursor (shape cache + hardware cursor), HID usages on the wire, modifier flags, autorepeat, scroll phases/momentum, release-all watchdog.
   *Exit (netem 30 ± 10 ms, 1 % bursty loss, 20 Mbit/s cap):* zero corrupt frames; zero keyframes caused by reordering; ≤ 1 IDR per minute; p95 glass-to-glass within 15 ms of the clean floor; input one-way p99 < 10 ms under video congestion; a 2 s Wi-Fi drop recovers within 1 RTT + 1 frame of connectivity returning.

### Phase 2 — "Feels native" (quality + session; ~5 weeks)

9. Virtual display module (lease, per-client serial, resize follow with epochs, physical-panel sleep option).
10. Static tile engine + region-masked apply + refine-when-idle + client tile cache; scroll blits.
11. Audio (Core Audio taps → Opus 10 ms → 30 ms playout with drift control); lazy clipboard (text, images, files).
12. 4:4:4 per the phase-0 probe result (HEVC RExt if available; otherwise siting-correct chroma only) and colour tagging end to end.
13. Multi-display streams; game-mode switch (manual toggle first; automatic per §6.4).
    *Exit:* `static-desktop` scene byte-identical within 1 s; `text-scroll` ≥ 0.95 SSIM at 10 Mbit/s; audio skew < 40 ms, no underruns over 30 min; resize = one clean cut; **the `research/07` v1 target met:** on the Airs over Wi-Fi, click-to-photon p50 ≥ 10 ms better than HP and p99 ≤ HP's p50; over Tailscale-direct with 1 % bursty loss no visible corruption and pixel-exact static text while HP stays soft.

### Phase 3 — "Ships to users" (connectivity, identity, ship shape; ~6 weeks)

14. `identity`: device keys, RPK TLS both ways, peer store; CPace pairing (code + QR); permissions; attended/unattended consent UX; scam interstitial; audit log.
15. `netpath` + `rendezvous`: MappedSocket, STUN, disco pings, port mapping, relay-first connect, direct upgrade, dual-send, interface-change handling; `sunna-rendezvous` and `sunna-relay` binaries + public instances (2 regions to start); DNS-SD; `doctor`.
16. Broker/agent/shell split on macOS: `Sunna.app` (SwiftUI) + `sunnad` + `Sunna Agent.app`, SMAppService, LoginWindow agent, permissions page with live polling and auto-relaunch, re-approval push; Sparkle-style updater with independent key; signed + notarised DMG; entitlement request filed with Apple.
17. Session model: tokens, reconnection, agent handover at login, multi-viewer with per-viewer encoders.
    *Exit:* steps-to-first-session 4 + 3 measured with a fresh user; connect success ≥ 99 % across the test matrix (home NAT, CGNAT hotspot, corporate UDP-blocked Wi-Fi, IPv6-only); direct ≥ 90 % on the pairs where at least one side is easy NAT; TTFF < 1 s WAN; upgrade to direct < 5 s; first-connection scam interstitial verified; **first public release (macOS host + macOS client)**.

### Phase 4 — Linux host (~6 weeks)

18. `sunnad`/agent on Linux; Mutter and KWin private virtual monitors + libei; portal route with restore tokens; KMS helper (privilege-separated) + uinput helper; Vulkan compute pass + VAAPI; NVENC on Linux; x264 for the VM; PipeWire audio; clipboard portal/data-control; virtual multitouch trackpad (gesture fidelity from the Mac client); deb/rpm; Linux client with `present-vulkan`.
    *Exit:* GNOME 46+ and Plasma 6 hosts stream at the same phase-2 quality bar; login-screen access on the KMS route; the owner's VM streams via KMS + x264 with tiles at ≤ 4 Mbit/s for desktop work; Mac-client trackpad gestures produce native GNOME workspace swipes (or the finding is documented as UNVERIFIED-failed with the reason).

### Phase 5 — Gaming and Windows (~8 weeks)

19. Game mode polish: 120 fps, HDR (needs an HDR display for validation), relative mouse, gamepads (client side everywhere; host side Linux uinput first), guest links and co-play slots, app launching.
20. Windows host: SCM service + session worker, DDA/WGC, NVENC/AMF/QSV, IddCx virtual display (SudoVDA lineage, attestation-signed), SendInput, PT_TOUCHPAD injection, virtual gamepad driver decision executed, MSI + Trusted Signing; Windows client with `present-d3d11`.
    *Exit:* Windows→Mac and Mac→Windows at the phase-2 bar; co-play with 2 guests on a Linux or Windows host; published benchmark vs Parsec (Windows host) and Moonlight+Sunshine (tuned) on the same hardware.

### Phase 6 — Reach (~ongoing)

21. Browser client (WebTransport + WebCodecs), iPad client, Android client; peer relays; team management plane (accounts, SSO, audit forwarding) as the open-core paid layer (`research/04 §6`); Moonlight-protocol compatibility mode as a side door if the client ecosystem justifies it (`research/06 §8`).

What ships first to users: **phase 3's macOS-only release** (host + client), because it is the segment where every incumbent is weakest and the owner can validate it daily on his own machines. Deferred until after that: Linux host, Windows host, gamepads, browser, mobile, HDR, AVC444 dual view, mic passthrough, per-app window streaming, peer relays, accounts.

---

## 14. Risks, open questions, decision log

### 14.1 Risks

| # | Risk | Likelihood / impact | Mitigation |
|---|---|---|---|
| R1 | VideoToolbox LTR/temporal-layer APIs behave differently from the WWDC21 description on M1/M2 or on macOS 26 (e.g. token semantics, HEVC support) | medium / high (recovery design depends on it) | phase-0 probe before building; fallback = NVENC-style "IDR on unrecoverable loss" with IDR coalescing + FEC/NACK carrying the load, plus intra-refresh where available |
| R2 | Private `CGVirtualDisplay` API changes or gets blocked | medium / medium | runtime probe + graceful degradation to physical display capture; Apple HP uses the same SkyLight machinery ([C §3.5]) so it is unlikely to vanish, but every macOS beta is tested |
| R3 | `persistent-content-capture` entitlement not granted | medium / medium for unattended Macs | re-approval detection + client push; MDM profile for fleets; the 15.1+ behaviour (refresh on use) means daily-used hosts never see it |
| R4 | quinn cannot be bent as assumed (Controller adapter, pacer interaction, datagram buffer behaviour) | low–medium / medium | the `cc` crate is pure and the sender owns pacing; worst case a custom UDP framing with the same packet formats (`research/06 §6` revisit clause) — the formats in §5.2 do not depend on QUIC |
| R5 | NAT traversal edge cases (corporate NATs, mobile CGNAT) keep sessions on relays | high / low (relay works) | relay is a first-class path; peer relays later; port mapping; publish the netcheck report so users can fix routers |
| R6 | Relay bandwidth cost for the public service | medium / medium | per-key quotas, 25 Mbit/s ceiling on relays, self-host encouraged, paid tier for higher relay budgets (open-core) |
| R7 | Lossless tiles cost too much CPU or bandwidth on Retina photographic content | medium / low | tile skip rule (4× video cost), budget from CC headroom, GPU hashing later |
| R8 | Wayland fragmentation (compositor-specific APIs, portal backends) | high / medium | the capture ladder degrades explicitly and reports the route; GNOME + KDE first-class, others via portal/KMS |
| R9 | Windows driver signing and anticheat | high / medium for gaming | SudoVDA lineage + attestation signing; anticheat documented as unsolvable; gaming is phase 5 |
| R10 | One developer; scope | high / high | phases with exits; macOS-only public release first; AI agents on well-specified crates (proto, media, cc, tiles, sim are ideal agent tasks: pure, testable) |
| R11 | Fanless Air thermal throttling in long sessions | medium / medium | thermal-state-aware CC; HEVC over H.264 where cheaper per bit; measure 30-minute sessions |
| R12 | Pasteboard privacy enforcement on macOS 26.x | medium / low | lazy clipboard + metadata-first; a one-time Allow is acceptable |

### 14.2 Open questions (with the experiment or decision date that closes each)

1. VT HEVC 4:4:4 via public API on M1/M2 — phase-0 probe. Decides desktop-mode codec.
2. VT LTR token/refresh semantics; HEVC low-latency + LTR together — phase-0 probe.
3. CGVirtualDisplay: SCK delivery on 15/26 and clamshell behaviour — phase-0 probe.
4. Reconnect-free agent handover: can the broker keep the QUIC connection and proxy datagrams to the agent over a shared-memory ring at < 100 µs per packet without copying pixels (media is already encoded at that point, so the copy is small)? If yes, the login→desktop transition needs no reconnect. Revisit in phase 3 step 17.
5. BGRA vs `420v` capture cost on the Airs (SCK GPU conversion vs VT conversion) — phase 0 step 3 measurement.
6. H.264 High vs HEVC Main quality-per-bit and encode/decode latency on M1/M2 — phase 0/1 measurement decides the default (`research/07` left this open).
7. Whether a vetted Rust CPace implementation exists; else SPAKE2 (`spake2` crate) — phase 3 step 14.
8. quinn pacer disable/override specifics — phase 1 step 6.
9. Native GNOME/KDE gestures from the uinput virtual trackpad — phase 4 step 18 (end-to-end UNVERIFIED).
10. macOS virtual gamepad via DriverKit HID — no supported path known; investigate after phase 5.
11. Mac-host HDR end-to-end fidelity — needs an HDR display; phase 5.
12. Safari WebTransport status for the browser client — phase 6.

### 14.3 Decision log

| # | Decision | Alternatives considered | Why |
|---|---|---|---|
| D1 | macOS host first, Linux second, Windows third; first public release is macOS-only | Windows-first (`research/06` M0c); all three in parallel | Owner's hardware and daily use; every incumbent is weakest on Mac hosts (`research/04` gap #7, [F §4.6]); one developer |
| D2 | ScreenCaptureKit replaces CGDisplayStream; minimum host macOS 14.4 | keep CGDisplayStream (small C API, works today) | obsoleted in 15 SDK, no frames from virtual displays, re-prompt trigger ([C §3.1]) |
| D3 | Datagram media + input, one controller (`cc`) owning rate, FEC and pacing, quinn adapted via `Controller` | keep CUBIC + app AIMD; str0m/WebRTC; custom UDP | HOL blocking and two-controller oscillation are the documented failure modes ([F §8.2], [A §10.3]); QUIC gives crypto, migration, WebTransport path |
| D4 | RS-FEC per block, adaptive ratio; NACK with deadline check; LTR-ack recovery; IDR last | fixed 20 % RS (Sunshine); XOR (today); NACK-only (HP) | [A §1.4], [C A2], `research/07` finding 6 |
| D5 | Lossless static tiles + region-masked video + scroll blits + refine-when-idle as the desktop mode | pure video at high bitrate (HP); 4:4:4 only; RemoteFX/DWT codecs | RDP evidence: text must not depend on the video codec ([C §1.3], [C S1–S4]); 4:4:4 alone cannot be lossless |
| D6 | 4:4:4 only via HEVC RExt if the probe succeeds; AVC444 dual view deferred and optional | `research/07`: "both, after tiles" | dual view doubles decode and needs reference management ([C A11]); tiles cover the text case; decide on measured need |
| D7 | Client-side hardware cursor with shape cache; host never bakes the cursor | cursor in video (today) | zero-latency pointer ([C S9]); every serious product does it |
| D8 | Own connectivity stack on upstream quinn, iroh/Tailscale design (mapped socket, relay-first, disco pings, UDP relay) | embed iroh; tsnet; libjuice; str0m ICE | [F §3b.4]; relay must carry media; no quinn fork; revise QUIC-native pings later |
| D9 | Identity = ed25519 device keys, RPK TLS, host-initiated CPace pairing with a 6-char code/QR; account optional for list sync only | ID + password (RustDesk); account as trust root (Parsec); PIN + cert pinning (Sunshine) | E2E against our own servers; no scam-shaped UX ([F §6.1], [F §6.5]) |
| D10 | Attended by default; unattended per peer with visible indicators; scam interstitial on first inbound | unattended by default with a permanent password | abuse evidence ([F §6.5]) |
| D11 | Broker (root daemon) + per-session agent (`[Aqua, LoginWindow]`) + SwiftUI shell; agent owns the media QUIC endpoint on a broker-passed socket; handover = token reconnect | agent-only LaunchAgent (`research/06 §2`); daemon capture (unsupported); broker proxying media | Apple DTS guidance ([C §3.4]); login window needs the LoginWindow agent; quinn cannot migrate a live connection across processes |
| D12 | SwiftUI shell on macOS; Tauri on Linux/Windows | Tauri everywhere (`research/06`) | native feel is the product; Swift is required anyway for SCK/CGVirtualDisplay/SMAppService |
| D13 | VT settings: low-latency spec, High/HEVC Main, LTR, temporal layers, `DataRateLimits` 1.5×/frame, QP 12–34, infinite GOP; property failures logged at warn | today's minimal set; FFmpeg VT wrapper (Sunshine) | [C §3.2] recommended set; Sunshine's FFmpeg path hides the properties that matter |
| D14 | Per-viewer encoder contexts by default | one encode fanned out (RustDesk) | slowest viewer must not degrade others ([F §1B.4]) |
| D15 | Audio: Core Audio taps, Opus 10 ms CBR 128 kbit/s, RED on loss, 30 ms playout | SCK audio only; BlackHole; Opus in-band FEC; 3 s buffers | [C §3.7], [A §1.6], [F §1B.5] |
| D16 | Input wire = USB HID usages + Text + raw contacts + semantic gestures; redundancy-based reliability on datagrams | mac keycodes (today); reliable stream (today); ENet-style channels | cross-OS correctness; no HOL; Moonlight's per-event reliability semantics ([A §2.5]) |
| D17 | Virtual displays are leases with per-client stable serials on all three OSes | persistent virtual monitors | never strand a desktop ([C S11]); per-client remembered layouts ([A §3.2]) |
| D18 | Linux capture ladder: Mutter/KWin private → portal (persist 2) → KMS helper; input libei → uinput helper | portal only (RustDesk pre-2026); KMS only (Sunshine) | [C §4.6]; unattended and greeter need KMS; consent needs the polite routes |
| D19 | Windows virtual display = SudoVDA lineage + attestation signing; gamepad driver decision deferred to phase 5 | write our own IddCx and VHF drivers now | signing cost and time; Apollo proves the control plane ([A §3.2]) |
| D20 | Measurement is presentation-based (`presentedTime`) with a photodiode rig and a published protocol | overlay sums; decode-time claims | [A §9.2], [C S20]: nothing else is comparable |
| D21 | No Moonlight-protocol compatibility in the core; possible side door later | speak GameStream for instant clients (Wolf/Apollo) | the encoder-coupled transport, LTR acks and tiles have no expression in GameStream ([A §1.4]); revisit in phase 6 |
| D22 | quinn transport config: idle 30 s, keep-alive 3 s, datagram send buffer 256 KiB, MTU 1200 with bounded PMTUD | today's 10 s / 2 s / 1 MiB | survive Wi-Fi drops; bound stale queue ([A §7.1]); tunnels ([A §8.2]) |
| D23 | Relay is free-tier, self-hostable, UDP-native | TCP DERP; relay as paid feature (Parsec) | [F §8.2]: the relay is the reliability feature |
| D24 | Game mode is a stream-config switch on the same pipeline, auto-detected with manual override | separate gaming product; always-on game mode | `research/07` positioning; §6.4 signals are cheap and reversible |
