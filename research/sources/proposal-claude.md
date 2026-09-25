# Sunna architecture proposal (Claude)

> Phase 2, written 2026-09-25 by "claude". Independent proposal; the other two proposals were not read.
> Evidence base: `dossier-astra.md` (game-streaming engines, cited as **[A §x]**), `dossier-fable.md` (RustDesk, Tailscale, iroh, Parsec, ease of use, security, distribution, cited as **[F §x]**), `dossier-claude.md` (RDP, gnome-remote-desktop, macOS/Linux/Windows internals, Apple Screen Sharing HP, cited as **[C §x]**), plus the Sunna repo (`/home/sunny/projects/sunna`, cited as **[S path]**, e.g. [S research/07], [S crates/host/src/lib.rs]).
> Extra targeted checks done for this proposal (cited inline as **[V: ...]**): quinn's `Controller` trait and datagram queue/packing order, iroh's exported congestion hooks and custom-transport trait, iroh's quinn fork dependency.
> Conventions: **UNVERIFIED** = a claim I could not confirm against a primary source; **[INFER]** = my reasoning; **DISAGREE** = where I depart from a dossier or an earlier repo decision, with the reason. Numbers marked "target" are commitments to measure against, not measurements.

## How to read this

The proposal is organised by the 14 mandatory sections. Each section ends with the decisions it makes. Section 14 collects every key decision with the alternatives that were rejected. If you only read three things, read §1.3 (targets), §5 (transport, where most of the engineering risk and most of the advantage sit), and §13 (the roadmap).

The one-paragraph version: Sunna is a **QUIC-datagram media engine with an application-owned, encoder-coupled rate controller, Reed-Solomon FEC with repair-on-demand, and LTR-based reference repair**, feeding a **native presenter per client OS**. On top of the video it carries an **RDP-style lossless refinement layer** so desktop text converges to pixel-exact, which no game streamer and not Apple's HP mode do. Hosts use a **broker/worker process split** on every OS so the login window, lock screen and user switching work without reconnecting. Connectivity **reuses iroh** (dial-by-key QUIC, hole punching, self-hostable relays) behind a seam, instead of building NAT traversal from scratch. Identity is **account-optional**: devices pair with a PAKE code or QR and pin each other's keys; an optional account only syncs the device list and is never the trust root. Delivery order: **Mac↔Mac first** (the owner's hardware and the HP benchmark), then **Linux host**, then **Windows host + gaming peripherals**, then iPad/browser/Android clients.

---

## 1. Product definition

### 1.1 Who it is for

Four user groups, in priority order. The order sets what ships first; it doesn't drop anyone.

1. **"My other computer" users (primary).** People with two or more machines who want the remote one to feel installed locally: a Mac mini or Mac Studio at home reached from a MacBook, a Linux workstation reached from a Mac, a desktop reached from a laptop across town. They care about sharp text, colour, a cursor that doesn't lag, correct keyboard and trackpad behaviour, clipboard, audio, and "it just connects". This is the owner's own use case and the one where every incumbent is weakest on macOS and Linux hosts [S research/04 §5; F §4.6; A §10.4].
2. **Home gamers streaming their own PC.** Moonlight/Sunshine and Parsec users. They care about input-to-photon latency, frame pacing, 120 Hz+, HDR, gamepads, and working through NAT without port forwarding [S research/04 §3–4; A §10.4].
3. **Helpers and co-players.** Someone sends a link; a friend or family member joins (view, control, or gamepad-only). Parsec's guest model is the reference [F §4.2, §4.6].
4. **Self-hosters and small teams.** People who want no cloud dependency, their own relay and rendezvous, and an audit trail. RustDesk's reason for existing [S research/04 §5; F §1A.7].

Not targeted: enterprise VDI fleets (RDP/Citrix/DCV territory), unattended support-desk tools for strangers (the scam vector, §8.6), cloud-gaming providers.

### 1.2 The promise

> **"Your other computer, as if it were this one."** Pixel-exact text, true colour, a cursor and keyboard that behave natively, and sub-frame added latency on a LAN. It connects from anywhere with no ports, no accounts required, no IDs to read out. It degrades gracefully on bad Wi-Fi instead of freezing, and it's free and self-hostable.

Four properties turn the promise into engineering constraints:

- **Native feel:** a virtual display sized to the client window's exact backing pixels [C §3.5, §6.2]; a client-side cursor [C §1.1, S9]; faithful keys, scroll phases and gestures [S research/05]; clipboard and audio.
- **Quality-first:** static content converges to lossless [C §1.5, S1–S4]; 4:4:4 where the hardware allows it; colour-managed end to end.
- **Low latency where it matters, and robust everywhere:** tail latency (p99) and recovery time are first-class metrics, not just p50 [A §10.4–10.5].
- **Zero-config and safe:** hole punching plus always-available relays [F §8.3], PAKE/QR pairing with key pinning [F §6.1], and unattended access that is off by default [F §6.5].

### 1.3 Measurable targets versus each competitor

**Definitions (used everywhere in this document):**

- **Added latency (AL):** streamed click-to-photon minus local click-to-photon, measured on the *same client machine, display and test app*, with a photodiode + USB-HID rig (§11.4). This is the metric Parsec reports (~7 ms at 1080p60 on a wired LAN [S research/01 §9]) and the only one that is honest across systems [A §9.2].
- **Freeze:** an interval over 100 ms during which the test pattern is changing on the host but the client displays no new content.
- **Recovery time:** from the first unrecoverable packet loss to the first clean frame on screen.
- **TTFF:** time to first frame, from the user clicking "Connect" to the first decoded frame presented.
- **Connection success:** the fraction of connection attempts that reach a streaming session within 10 s, across a fixed matrix of NAT and network types (§11.6).
- **Steps-to-first-session:** user actions (clicks, typed fields, OS toggles, installer screens) from nothing installed on either machine to controlling the remote machine from a different network, counted the way [F §5] counts them.

**Test matrix codes:** LAN-E (both machines wired), LAN-W (5 GHz Wi-Fi, AWDL state recorded), WAN-20 (20 ms RTT, 0.1% random loss), WAN-B (40 ms RTT, 1% bursty loss, Gilbert–Elliott mean burst 3), RELAY (forced relay path), HP-MAC (M1 Air ↔ M2 Air, the owner's pair).

**Table 1: latency and robustness targets.** Baselines are measured on the same rig at the same time (§11.6). Where a competitor has no public number, the target is relative to our own measurement of it.

| Metric | Condition | Sunna target | Apple Screen Sharing HP | Parsec | Moonlight + Sunshine (and forks) | RustDesk |
|---|---|---|---|---|---|---|
| AL p50 | LAN-E, 60 Hz client, 1440p-class | **≤ 10 ms** | no public data [C §5.3]; beat by ≥ 10 ms [S research/07] | 7 ms claimed at 1080p60 on Windows+NVENC [S research/01 §9]; on a Mac host, **≤ Parsec** | sub-10 ms reported on Windows hosts [S research/02 §1]; on a Mac host **≤ Sunshine − 5 ms** | 18–30 ms (UNVERIFIED vendor blog) to 120 ms (#6888) [F §2.1]; beat by ≥ 2× |
| AL p99 | LAN-E | **≤ 20 ms** | ≤ HP p50 [S research/07] | ≤ Parsec p99 − 10 ms | ≤ Sunshine p99 | — |
| AL p50 / p99 | LAN-W | **≤ 14 / ≤ 35 ms** | beat p99 by ≥ 2× (HP is designed for wired [C §5.3]) | ≤ Parsec | ≤ Moonlight | — |
| AL p50 | WAN-20 | **≤ RTT/2 + 12 ms** | — | ≤ Parsec | ≤ Moonlight+Tailscale | — |
| AL p99 | WAN-B | **≤ RTT/2 + 35 ms** | beat by ≥ 2× | ≤ Parsec | beat by ≥ 1.5× (fixed 20% FEC, no NACK, no rate control [A §1.4]) | beat by ≥ 3× (TCP head-of-line blocking [F §1B.14]) |
| Freezes per 10 min | WAN-B | **0 visible corruption; ≤ 1 freeze** | report | report | report | report |
| Recovery time p95 | WAN-B | **≤ RTT + 2 frame intervals, no IDR** | FIR/IDR-bound [C §5.3] | report | RFI on NVENC only [A §1.3] | TCP stall |
| TTFF p50 / p95 | LAN / WAN / RELAY | **≤ 0.8 / 1.5 / 1.5 s; p95 ≤ 3 s** | report | report | report | report |

**Table 2: quality at fixed bandwidth.** Test clips (§11.5): "Desktop" (typing and scrolling in a code editor, a web page with coloured text, window drags, 10 s static), "Mixed" (a browser playing 1080p video in a window), "Game" (a 60 fps game capture replayed as synthetic input where possible). Metrics are computed on the *client-presented* frames against the host source: SSIMULACRA2 and PSNR-Y for video content, and **pixel-exact convergence** for text.

| Metric | Condition | Sunna target | Competitor reference |
|---|---|---|---|
| Static text converges to pixel-exact | Desktop clip, any bandwidth ≥ 3 Mbps | **100% of static tiles byte-identical within 500 ms of the last change** | No video-only system can do this: HP, Parsec, Moonlight and RustDesk are all pure video [C §5.4, S1] |
| SSIMULACRA2 mean / 5th percentile, Desktop clip | 10 and 20 Mbps, 2560×1600 at 60 fps | **≥ the best competitor + 5 / + 10** | Parsec Warp 4:4:4, HP 4:4:4, Sunshine 4:2:0/4:4:4 as available |
| Game clip, VMAF | 20 and 40 Mbps, same codec | **within 2 points of Sunshine** on the same host GPU | Same hardware encoder, so parity rather than a win; the win comes from tail latency and recovery |
| Colour accuracy | Display P3 test chart | **ΔE2000 < 2 mean** after colour management | HP: 4:4:4 and HDR [C §5.1]; Parsec: none documented |

**Table 3: connectivity and ease.** Competitor numbers come from [F §5.7].

| Metric | Sunna target | Parsec | RustDesk | Moonlight + Sunshine | Apple HP |
|---|---|---|---|---|---|
| Connection success (NAT matrix) | **≥ 99.5% (relay fallback)**; direct ≥ 90% of pairs | "97%" traversal, no consumer relay [F §4.1] | relay fallback exists (TCP) | none built in | none built in |
| Steps-to-first-session, Mac host ↔ Mac client, different networks | **≤ 8 steps, 3 OS toggles, 1 short-code transcription (0 with LAN, QR or account)** | ~11 steps, 4 prompts + restart | 9 steps, 4–5 prompts, 2 transcriptions | 8–11 steps + VPN or port forwarding | 5–7 steps + VPN |
| Unattended after reboot, including the login window | yes (opt-in) | yes (custom installer on Mac) | yes (daemon install) | yes | yes |
| Account required | **no** (optional) | yes | no (the public server now requires login for controllers [F §2.4]) | no | Apple ID optional |

### 1.4 Explicit non-goals for the first 12 months

- Per-application window streaming (Coherence-style). Macs don't expose a mirrored window tree cheaply [S research/07 decisions table].
- Beating the NVENC-based incumbents on Windows *LAN p50*. On a clean LAN the incumbents are already efficient [A §10.4]. The Windows target is parity at p50 and a win at the tail and on WAN.
- Mac host virtual gamepads (no sanctioned API [S research/03 §6]; research spike only, §3.1.9).
- Anti-cheat-compatible input drivers [S research/04 constraint].

**Decisions in §1:** the primary user is "my other computer", with games on the same pipeline. Metrics are *added* latency with p99 and recovery time, not host-overlay numbers [A §9.2]. Every target is relative to a same-rig measurement of the competitor, because public numbers are scarce and incomparable [A §9.1, C §5.3].

---
## 2. System overview

### 2.1 Processes per machine

Every host OS uses the same three-role split. Every client is a single app process with a dedicated media core. This is the shape Apple DTS recommends on macOS [C §3.4], the shape g-r-d uses on Linux [C §2.1], and the shape Parsec, RustDesk and Sunshine use on Windows [F §1B.10, §6.3]. Choosing it on day one removes the "GUI is the host" retrofit problem [S research/06 §2].

| Role | macOS | Linux | Windows | Responsibilities |
|---|---|---|---|---|
| **Broker** (one per machine, always running when hosting is enabled) | `sunnad`: LaunchDaemon registered with `SMAppService.daemon` | `sunnad.service`: systemd system unit, runs as user `sunna` with narrowly scoped helpers | `SunnaService`: SCM service, LocalSystem | Owns the network endpoint (QUIC/iroh), device identity keys, the trust store and permissions, pairing, the session table, the media transport (FEC, scheduler, pacer, rate controller), the audit log, the updater. Spawns and supervises workers. **Never touches pixels.** |
| **Session worker** (one per GUI session, including the login window) | `SunnaAgent.app` (LSUIElement bundle): LaunchAgent with `LimitLoadToSessionType = [Aqua, LoginWindow]` [C §3.4] | `sunna-worker`: user unit in the graphical session; greeter instance via the KMS helper path (§3.2) | `sunna-worker.exe`, launched into the console session with the winlogon/user token [F §6.3] | Capture, virtual display lease, encode, tile engine, cursor, audio capture, input injection, clipboard. Talks to the broker over shared memory and a Unix socket or named pipe. |
| **Privileged helpers** (optional, opt-in) | none in v1 | `sunna-kms-helper` (CAP_SYS_ADMIN only, seccomp, dmabuf over socketpair), `sunna-uinput-helper` [C §4.1, S16] | virtual display driver (IddCx), virtual gamepad driver | Only the capability each one needs. |
| **App (UI shell)** | `Sunna.app` (SwiftUI) | `sunna` desktop app (Tauri) | `sunna.exe` (Tauri) | Device list, pairing, settings, onboarding, and the **client**: the stream window, input capture, decode and presentation. Talks to the local broker over local IPC for host settings. |

**Why the broker owns the network rather than the worker.** It keeps one stable listening identity per machine (one endpoint ID, one port, one relay registration). Sessions also survive worker restarts and GUI-session transitions: login window → user session, lock, fast user switching. The broker keeps the QUIC connection and re-attaches it to the new worker, and the client sees one IDR (or LTR refresh) instead of a reconnect. This is better than g-r-d's redirect-and-reconnect [C §2.1]. The cost is one shared-memory hop for encoded frames (worker → broker) and for input (broker → worker), about 10–50 µs each [INFER]. That is budgeted in §2.5.

**Client process.** `Sunna.app` (or the Tauri app on Linux and Windows) embeds `sunna-client-core` in-process. The stream window is a native surface created and driven by the core: `NSWindow` + `CAMetalLayer` on macOS, a Wayland/X11 Vulkan surface on Linux, an HWND + DXGI swapchain on Windows. The UI toolkit never touches frames or hot-path input events [S research/06 §1]. A machine can be host and client at the same time, and the app is also a client of its own local broker.

### 2.2 Crate and module layout

This is the target workspace. Existing crates are kept and grow into it (§12.3 has the migration map).

```text
crates/
  proto/         sunna-proto      wire formats: datagram headers, frame descriptor, feedback,
                                  control messages (postcard), input state. Pure, #![forbid(unsafe)], fuzzed.
  fec/           sunna-fec        Reed-Solomon GF(2^16) wrapper (reed-solomon-simd), repair-on-demand.
  ratectl/       sunna-ratectl    rate controller + FEC policy + frame admission. Sans-IO, deterministic,
                                  driven by the simulator in tests.
  transport/     sunna-transport  QUIC integration: MediaConnection trait, scheduler, pacer, quinn/noq
                                  congestion "follower", stream priorities, datagram sizing.
  net/           sunna-net        connectivity: iroh endpoint, discovery (mDNS + directory), relay config,
                                  path events, custom UDP relay transport (later).
  identity/      sunna-identity   device keys, trust store, pairing (CPace), permissions, audit log.
  media/         sunna-media      frame model, colour metadata, timestamps/clock domains, damage rects.
  capture/       sunna-capture    FrameSource trait + backends: sck (macOS), mutter, kwin, portal,
                                  wlr, kms, x11, dda, wgc, synthetic.
  vdisplay/      sunna-vdisplay   virtual display leases: cgvirtualdisplay, mutter-virtual, kwin-virtual,
                                  iddcx.
  codec/         sunna-codec      Encoder/Decoder traits + vt, vaapi (via vulkan), nvenc, amf, qsv,
                                  vulkan-video, x264, openh264 (decode fallback), dav1d.
  tiles/         sunna-tiles      lossless refinement: tile damage map, hashing, tile codec, client cache
                                  protocol, scroll detection (v2).
  audio/         sunna-audio      capture (Core Audio tap, PipeWire, WASAPI), Opus, playout + drift.
  input/         sunna-input      HID-usage model, keymaps, injection backends (CGEvent, libei, uinput,
                                  SendInput/synthetic pointer), client capture helpers, gamepads.
  cursor/        sunna-cursor     shape capture, hash cache, position/visibility.
  clipboard/     sunna-clipboard  lazy multi-format clipboard, file transfer chunks.
  host/          sunna-host       session worker pipeline (capture → encode → shm ring), control.
  broker/        sunna-broker     daemon core: session table, worker supervision, shm IPC, policy.
  client/        sunna-client     client core: receive → FEC → decode → present scheduling, input path.
  present/       sunna-present    presenters: metal, vulkan (wayland/x11), d3d11 (+ presentedTime).
  telemetry/     sunna-telemetry  stage clocks, per-frame records, flight recorder, overlay model.
  sim/           sunna-sim        network + pipeline simulator (virtual time), trace replay.
  ffi/           sunna-ffi        UniFFI bindings for shells (Swift, TypeScript via Tauri commands).
apps/
  sunnad/        broker binary (all OSes)
  sunna-worker/  session worker binary (macOS: packaged inside SunnaAgent.app)
  sunna-cli/     dev client + bench + doctor
  sunna-relay/   UDP media relay (later; iroh custom transport)
  sunna-dir/     directory/rendezvous service (iroh-dns-server based) + pairing mailbox
  mac/           Xcode project: Sunna.app (SwiftUI), SunnaAgent.app wrapper, Sparkle
  desktop/       Tauri shell for Linux/Windows
  bench-rig/     RP2040 photodiode firmware + host analysis scripts
```

Dependency rule: `proto`, `fec` and `ratectl` depend on nothing platform-specific and never allocate on the hot path after warm-up. `transport` depends on `proto`, `fec` and `ratectl`. Platform crates never depend on `transport`. The broker/worker boundary is a versioned shared-memory protocol defined in `sunna-proto::ipc`.

### 2.3 Data flow (host → client video)

```text
HOST: session worker (per GUI session)                     HOST: broker (sunnad)
┌──────────────────────────────────────────────┐          ┌─────────────────────────────────────────────┐
│ [T0] display refresh (virtual display lease)  │          │                                             │
│  │ SCStream (queueDepth 3)  / PipeWire / DDA   │          │  shm ring "enc" (8 slots, 8 MiB)            │
│  ▼ [T1] frame callback (displayTime)          │          │   │                                          │
│ admission gate ◄── rate/queue state (shm)     │─────────►│   ▼ packetizer + RS FEC (sunna-fec)          │
│  │ (drop BEFORE encode if over budget)        │ encoded  │   ▼ scheduler: P0 ctl │P1 audio│P2 repair│   │
│  ├──► tile engine (desktop mode, BGRA)        │  AU +    │     P3 video│P4 discardable video        │   │
│  │     └─► lossless tiles ───────────────────►│  desc    │   ▼ pacer (token bucket)                    │
│  ▼ [T2] VT/NVENC/VAAPI submit                 │          │   ▼ quinn/noq send_datagram  (queue ≈ 0)    │
│  ▼ [T3] encoder output callback               │          │   ▼ [T5] socket send (GSO)                  │
│  └─► write AU + frame descriptor [T4] ───────►│          │  tiles/cursor shapes/clipboard: QUIC streams│
│ cursor seed poll (120 Hz) ─► shape/pos ──────►│          │  (priorities 90/10/0, app-level byte budget)│
│ Core Audio tap ─► Opus 10 ms ────────────────►│          │                                             │
└──────────────────────────────────────────────┘          └─────────────────────────────────────────────┘
                                   network (direct UDP / iroh relay / sunna-relay)
CLIENT (single process)
┌──────────────────────────────────────────────────────────────────────────────────────────────────────┐
│ net thread: [T6] recv (GRO) → demux kind → video reassembler (per frame, RS decode) → [T7] frame done │
│   ├─► feedback builder (every 10 ms: TSN arrivals, NACKs, LTR acks, decode/present stats) → host      │
│   └─► decode queue (≤ 2 AUs) ──► decode thread: VTDecompressionSession async → [T8] decoded IOSurface │
│                                   └─► present slot (latest-wins, 1 pending) ─► present thread:         │
│                                        arrival mode or CAMetalDisplayLink → [T9] commit → [T10]       │
│                                        presentedTime (MTLDrawable)                                     │
│ tiles stream → tile cache (LRU, 256 MiB) → overlay texture + validity mask ─► composited in present  │
│ cursor stream/datagram → NSCursor (hardware cursor, zero latency)                                     │
│ audio → Opus decode → playout ring (target 20 ms, adaptive) → CoreAudio                               │
└──────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

### 2.4 Control flow (client → host input and feedback)

```text
CLIENT main thread (AppKit/Wayland/Win32 events, mouse coalescing OFF)
  → input normalizer (HID usages, cumulative motion/scroll counters, key-state bitmap)
  → lock-free SPSC → net thread → INPUT datagram (P0, sent immediately, redundant tail)
HOST broker net thread → input validator (permissions per peer) → shm ring "input" (256 events)
  → worker input thread (QOS_CLASS_USER_INTERACTIVE) → CGEventPost / libei / uinput / SendInput  [I3]
Key/button state reconciliation: every INPUT datagram carries the full key-down bitmap (32 bytes)
  → host releases or presses to match (lost packet ≠ stuck key).
Feedback (client → host, every 10 ms while video flows): FEEDBACK datagram (P0) → broker ratectl
  → new target bitrate/fps/FEC → shm "rate" block → worker applies to the NEXT encode.
Control messages (reliable, stream 0): session setup, config epochs, permissions, resize,
  keyframe/LTR-refresh requests (duplicated on datagrams for speed), clipboard offers.
```

### 2.5 Threading model

**Host worker (macOS; the same shape elsewhere):**

| Thread | QoS / priority | Work | Must never |
|---|---|---|---|
| SCK sample-handler queue (serial dispatch queue) | `QOS_CLASS_USER_INTERACTIVE` | receive `CMSampleBuffer`, read `SCStreamFrameInfo` (status, displayTime, dirtyRects), run the admission gate, submit to VT (async), hand BGRA to the tile engine | block on the network, allocate per frame after warm-up |
| VT output callback (VT-owned) | VT's | copy the encoded AU into shm ring slot "enc" (single memcpy, ~50–300 KB), write the descriptor, signal the broker | encode, block |
| Tile worker (1 thread) | `UTILITY` | settled-tile hashing and lossless encoding from the IOSurface (read-only lock; unified memory, so no PCIe copy) | run ahead of the settle timer; hold IOSurfaces more than 2 frames |
| Input injector | `USER_INTERACTIVE` | drain the input ring, `CGEventPost` immediately | batch to frame ticks |
| Cursor poller | `USER_INITIATED` | `CGSCurrentCursorSeed` at 120 Hz, send the shape on change [C §3.6] | — |
| Audio tap IOProc (Core Audio RT thread) | real-time | copy PCM into a lock-free ring | encode (Opus runs on a separate thread) |
| Opus encoder | `USER_INITIATED` | 10 ms frames → shm ring "audio" | — |
| Control (tokio current-thread) | default | IPC with the broker, lifecycle, permissions | — |

**Host broker:**

| Thread | Priority | Work |
|---|---|---|
| Media transport thread (tokio current-thread runtime, pinned to a performance core where the OS allows; `QOS_CLASS_USER_INTERACTIVE`) | high | owns the `Connection`s; drains the shm rings; packetize + FEC; scheduler; pacer; `send_datagram`; receives datagrams (input, feedback) and dispatches immediately |
| Control/async runtime (multi-thread, 2 workers) | default | control streams, pairing, discovery, updates, audit log, relay/directory I/O |
| iroh internals | per iroh | path management, relay connections |

Rule: **nothing in the media transport thread waits on a lock held by the control runtime.** State shared between them uses atomics or `arc-swap` snapshots.

**Client:**

| Thread | Priority | Work |
|---|---|---|
| Main/UI thread | UI | window events and raw input capture (`NSEvent.isMouseCoalescingEnabled = NO` [C §6.3]) → SPSC to the net thread |
| Net thread (tokio current-thread, `USER_INTERACTIVE`) | high | recv, reassembly, RS decode, feedback, input send, audio demux |
| Decode thread | high | `VTDecompressionSessionDecodeFrame` (async); output callback posts to the present slot |
| Present thread | high | arrival mode: present when a new frame lands. Display-link mode: `CAMetalDisplayLink` callback, pick the latest frame, render late |
| Audio output (Core Audio RT) | real-time | pull from the playout ring |
| Tile/cursor/clipboard (tokio multi-thread, 1–2 workers) | default | reliable streams, cache |

### 2.6 Every queue and its bound

Latency lives in queues, so each queue has an explicit bound in *time*, not just count [A §10.3 item 1, §5.1 caveat]. "Policy on overflow" says what happens when the bound is hit.

| # | Queue | Location | Bound | Policy on overflow |
|---|---|---|---|---|
| Q1 | SCK frame queue | worker | `queueDepth = 3` (Apple minimum; ≤ 8 allowed) [C §3.1] | SCK stalls if we hold surfaces. We release each surface within 2 frames; the tile engine copies settled tiles or drops them |
| Q2 | Pre-encode admission | worker | 0 queued frames: the gate is a decision, not a queue | drop the capture before encode when projected send time exceeds the budget (the reference chain stays intact) [A §1.1] |
| Q3 | VT in-flight encodes | worker | ≤ 2 frames (`MaxFrameDelayCount = 0` [C §3.2]; low-latency mode is one-in-one-out) | stop submitting and count the stall |
| Q4 | shm ring "enc" | worker → broker | 8 slots / 8 MiB; **age bound 1 frame interval** | the broker reads immediately. An AU older than the bound is still sent (it is a reference), but the next capture is gated |
| Q5 | Scheduler video queue (P3/P4) | broker | **time bound: projected drain ≤ max(1 frame interval, 8 ms) on LAN, ≤ 2 frame intervals on WAN** | P4 (discardable) frames are dropped first; the P3 overflow causes pre-encode gating (Q2) and a rate cut. Never drop a P3 frame after partial send without flagging it lost |
| Q6 | Scheduler repair queue (P2) | broker | ≤ 1 RTT of repair bytes, deadline-tagged | expired repair is discarded (its frame deadline passed) |
| Q7 | quinn/noq datagram send buffer | broker | configured 1 MiB capacity (as today [S transport]), **held at ≤ 1 pacing quantum (~2 ms of bytes) by the scheduler** | we check `datagram_send_buffer_space()` before each hand-off; quinn's drop-oldest behaviour never engages in steady state [V: quinn-proto `datagrams.rs` `make_space_for`] |
| Q8 | Kernel socket buffer | both | OS default + GSO batches ≤ 64 KiB / ≤ 64 segments [A §1.4] | the pacer prevents build-up |
| Q9 | Client datagram receive buffer (quinn) | client | 2 MiB; drained continuously by the net thread | quinn drops the oldest datagrams when full [V: `datagrams.rs` `received`] and we count them as local loss (reported separately from network loss) |
| Q10 | Frame reassembly table | client | frames within `[newest_complete − 0, newest_seen + 8]`; **each frame expires at its deadline** (capture + latency budget) | expired → declare lost → reference repair |
| Q11 | Decode queue | client | ≤ 2 AUs (normally 0–1) | if the decoder falls behind by more than 1 frame, still decode every AU (references), but **skip presenting** all but the newest [A §2.3] |
| Q12 | Present slot | client | 1 pending decoded frame (latest wins) + 1 on screen + 1 in flight to the compositor | an older pending frame is replaced (counted as "superseded", not as a drop) |
| Q13 | Drawable pool | client | `maximumDrawableCount = 3` by default; 2 as a measured option [C §6.1] | `nextDrawable` blocks → recorded as a present stall |
| Q14 | Audio capture ring | worker | 40 ms | overwrite the oldest, count the event |
| Q15 | Audio playout | client | target 20 ms, adaptive 10–60 ms | time-stretch ±0.5%; drop above 80 ms |
| Q16 | Input ring | broker → worker | 256 events | never overflows in practice; on overflow coalesce motion (sum relative, keep the latest absolute) [A §1.5] |
| Q17 | Client input SPSC | client | 1024 events | coalesce motion |
| Q18 | Tile stream send | broker | app-level byte budget = rate-controller "refinement share" (≤ 20% of target, 0 in DRAIN) | tiles re-queue by recency; stale tiles (re-damaged) are cancelled |
| Q19 | Bulk streams (clipboard/files) | broker/client | budget ≤ 10% of target while video is active, 90% when idle | quinn stream priority 0 + app budget |
| Q20 | Encoded-frame history (for repair-on-demand and LTR) | broker | max(250 ms, 3 × RTT) of AUs, ≤ 64 MiB | the oldest entries are evicted; a NACK for an evicted frame → LTR refresh |

**Decisions in §2:** a broker/worker split on all hosts, with the broker owning the network. One media transport thread per broker. Explicit time bounds on every queue. The scheduler keeps quinn's own datagram queue near-empty so priority and deadline decisions stay in our code.

---
## 3. Host engine per OS

### 3.1 macOS host (first)

Minimum OS: **macOS 14.4** (ScreenCaptureKit works pre-login from 14.4 [C §3.4]; Core Audio process taps are reliable from 14.4 [C §3.7]; `CAMetalDisplayLink` needs 14 [C §6.1]). Apple silicon only for hosting in v1. Intel Macs can be clients.

#### 3.1.1 Capture: ScreenCaptureKit only

CGDisplayStream is obsoleted in the macOS 15 SDK, delivers **no frames for CGVirtualDisplay displays on 15+**, and triggers the Sequoia re-prompt [C §3.1, A5]. Sunna currently uses it [S README]. It is replaced before the virtual-display step, as 07-plan step 3 already intends.

`SCStreamConfiguration` per mode:

| Field | Desktop mode | Game mode | Reason |
|---|---|---|---|
| `width`/`height` | virtual display backing pixels (e.g. 2940×1912 for a 1470×956 pt window at 2×) | same, or reduced by the rate controller | 1:1 pixels = sharp text [C §6.2] |
| `captureResolution` / `scalesToFit` | `.best` / `false` | same | no host-side resampling [C §3.1] |
| `pixelFormat` | `kCVPixelFormatType_32BGRA` | `kCVPixelFormatType_420YpCbCr8BiPlanarFullRange` (`420f`) | BGRA feeds the lossless tile engine and 4:4:4; `420f` halves memory traffic at 4K120 when tiles are off. **DISAGREE (mild) with [S research/07] "BGRA always":** keep BGRA for desktop, but game mode doesn't need RGB |
| `colorSpaceName` | `kCGColorSpaceDisplayP3` (fallback `sRGB`) | same | Macs are P3; tag the bitstream to match [C §6.2 rule 4] |
| `minimumFrameInterval` | `1/client_refresh` (60 or 120) | `1/client_refresh` | caps the rate at what the client can show |
| `queueDepth` | 3 | 3 | Apple minimum; holding surfaces stalls capture [C §3.1] |
| `showsCursor` | `false` | `false` (the cursor is drawn by the client; §3.1.6) | zero-latency client cursor [C S9] |
| filter | `SCContentFilter(display: vd, excludingWindows: [Sunna overlays])` | same | hide our own indicators |
| audio | off (a Core Audio tap handles audio, §3.1.5) | off | separate TCC bucket, no re-prompt coupling [C §3.7] |

Per-frame handling: read `SCStreamFrameInfo.status`. `.idle` → no image and no encode, but the tile settle timers keep running. `.complete` → use `displayTime` (mach absolute time) as **T0/T1**, the true capture timestamp [C §3.1]. Keep `dirtyRects` (in backing pixels) for the tile engine and the frame descriptor's damage list. Handle `SCStreamErrorUserStopped` (-3817) and `NoCaptureSource` → session event → client banner. Handle `didStopWithError` on permission revocation → the host reports `PERMISSION_REVOKED(screen)` to the client (§9.5).

Resolution change: `SCStream.updateConfiguration` plus the virtual display mode change (§3.1.2). No stream restart [C §3.1].

#### 3.1.2 Virtual display: `CGVirtualDisplay` leases

A private API, isolated in `sunna-vdisplay::cgvirtualdisplay`, runtime-checked (class exists, selectors respond), with a fallback to capturing the physical main display [C §3.5].

- **Descriptor:** `name = "Sunna – <client device name>"`, `maxPixelsWide/High = 6144×3456` (UNVERIFIED maximum; DeskPad uses 5120×2160 [C §3.5]; HP caps backing at 3840×2160 [C §5.2]), `sizeInMillimeters` computed from the client panel's physical DPI, `vendorID = 0x5354` ("ST"; any stable value), `productID = hash16(client device key)`, `serialNum = hash32(client device key)`. **The stable serial per client device** makes macOS remember arrangement and scaling per client [C §3.5, INFER].
- **Modes:** `hiDPI = 1`. The mode list is the exact client view size in points × {2, 1} plus 5 common sizes. The refresh rate equals the client display refresh (60 or 120; 120 Hz virtual modes are UNVERIFIED, test E-VD120 in §14.2).
- **Lease semantics** (from SudoVDA's watchdog [C §4.7, S11]): the display lives exactly as long as the worker holds the object and the session is in `ACTIVE` or `GRACE` (§10.6). If the worker crashes, macOS removes the display automatically (the process owns it). A 30 s grace period after disconnect keeps windows in place for reconnection.
- **Placement policy** (a per-host setting, default "Replace"):
  - *Replace:* the virtual display becomes main and physical displays are mirrored to it or blanked, which is HP's behaviour when the same user connects [C §5.1] and the only robust curtain on macOS [F §6.3].
  - *Extend:* add the virtual display next to the physical ones.
  - *Mirror physical:* no virtual display; capture the physical panel (fallback, and for helping someone at the machine).
- **Resize protocol:** the client sends `ResizeRequest{points_w, points_h, scale, refresh, epoch}` after 300 ms of no further window-size change. During a drag the client scales the current stream locally with a good filter [C §6.2 rule 2]. The host applies the new mode (`applySettings:` with the new mode added), `updateConfiguration` on SCK, and recreates the encoder session (a new resolution needs a new sequence) → **one IDR per committed resize, never one per drag step**, fixing HP's per-resize hitch [C §5.3, A4]. The config epoch increments, and frames carry the epoch.
- **Clamshell/headless:** whether a CGVirtualDisplay keeps a lid-closed MacBook awake as an external display is UNVERIFIED (test E-CLAM, §14.2). Headless Mac minis work (DeskPad precedent).

#### 3.1.3 Encode: VideoToolbox, exact settings

Session creation: `VTCompressionSessionCreate` with encoder specification `{EnableLowLatencyRateControl: true, RequireHardwareAcceleratedVideoEncoder: true}` [C §3.2]. **Every property is set individually, its `OSStatus` checked, and failures logged at warn level with the property name.** A startup probe (`sunna-cli doctor codecs`) dumps `VTCopySupportedPropertyDictionaryForEncoder` for H.264/HEVC at the session resolution, including the supported `ProfileLevel` values [C §3.2].

| Property | Desktop H.264 | Desktop HEVC | Game HEVC | Notes |
|---|---|---|---|---|
| `ProfileLevel` | `H264_High_AutoLevel` (fallback `ConstrainedHigh`) | `HEVC_Main_AutoLevel`; `Main444` if the probe proves 4:4:4 (§6.3) | `HEVC_Main_AutoLevel` (`Main10` for HDR, later) | The repo uses Main today [S codec/videotoolbox/mod.rs:237] |
| `RealTime` | true | true | true | |
| `AllowFrameReordering` | false | false | false | no B-frames |
| `MaxFrameDelayCount` | 0 | 0 | 0 | [C §3.2] |
| `MaxKeyFrameInterval` / `…Duration` | `INT32_MAX` / 0 | same | same | IDR on demand only |
| `AverageBitRate` | rate controller target (per frame) | same | same | LL mode requires an explicit bitrate [C §3.2] |
| `DataRateLimits` | `[target_Bps × 1.5 × (1/fps), 1/fps]` if supported, else unset | same if supported (historically not for HEVC [C §3.2]) | `[target_Bps × 2 × (1/fps), 1/fps]` | caps single-frame spikes; probe-gated |
| `MaxAllowedFrameQP` | 36 | 36 | 44 | desktop: text stays legible during motion and lossless tiles fix static regions; game: let quality dip rather than drop frames. **Tunable starting points** [INFER from C §3.2] |
| `MinAllowedFrameQP` | 14 | 14 | 10 | don't burn bits refining what the tiles will replace |
| `EnableLTR` | true | true | true | reference repair (§5.5); per-codec support is UNVERIFIED and probed at startup |
| `BaseLayerFrameRateFraction` | unset (off) on LAN; 0.5 on WAN paths | same | 0.5 at ≥ 90 fps or on WAN | enhancement frames become droppable [C §3.2 S6] |
| `BaseLayerBitRateFraction` | — | — | 0.7 | Apple recommends 0.6–0.8 [C §3.2] |
| `PrioritizeEncodingSpeedOverQuality` | false | false | true | |
| `SpatialAdaptiveQPLevel` | disabled | disabled | default | flat UI dislikes AQ [INFER]; measure |
| `ExpectedFrameRate` | client refresh | same | same | |
| Colour tags (`ColorPrimaries`, `TransferFunction`, `YCbCrMatrix`) | P3-D65 / sRGB (IEC 61966-2-1) / ITU-R 709; full range | same | same (HDR: BT.2020/PQ later) | must match the capture colourspace; VT acceptance of the P3 + sRGB-transfer combination is UNVERIFIED, fallback BT.709 throughout with capture in sRGB |
| Input | BGRA IOSurface (zero-copy) | BGRA, or `xf44` if 4:4:4 is proven | `420f` IOSurface | zero-copy retained IOSurface → CVPixelBuffer [S research/07 step 0] |

Per-frame options: `ForceKeyFrame` (rare), `AcknowledgedLTRTokens` (every frame that has new acks), `ForceLTRRefresh` (on loss) [C §3.2]. The output callback reads the attachments `RequireLTRAcknowledgementToken` and the dropped-frame flag (a normal event, counted, never fatal [S commit 0f0b359]). The packetizer handles **N NAL units per sample** (the iOS 26 HEVC change [C §3.2]).

Default codec on Mac↔Mac: **H.264 High until the HEVC measurement is done**, then pick by measurement: text sharpness at equal bitrate and encode/decode ms on M1 and M2 [S research/07 decisions]. My expectation is that HEVC wins, because HP uses it [C §5.2] [INFER].

Parameter-set changes (new session, resize): the frame descriptor has `CONFIG` + `config_epoch`, and the client recreates the decoder when the parameter sets differ [S research/07 step 3].

#### 3.1.4 Tile engine (desktop mode refinement), host side

Full design in §6.2. Host specifics on macOS: dirty rects from SCK map onto a 64×64-pixel tile grid in backing pixels. A tile is "settled" when it has been undamaged for **100 ms**. g-r-d uses 60 ms for chroma upgrade [C §2.5]; lossless tiles cost more, so I wait slightly longer [INFER, tunable]. Settled tiles are read from the retained IOSurface (read-only lock). On Apple silicon's unified memory this is a cache-coherent read, not a GPU→CPU copy. They are hashed (xxh3-128) and either sent as a cache reference or encoded losslessly (§6.2.3). The worker retains at most 2 IOSurfaces for tile reads; if the tile worker lags, it copies the settled tile rects into a CPU buffer (≤ 1 frame of tiles) and releases the surface, so SCK's `queueDepth` is never starved (Q1).

#### 3.1.5 Audio

- **Capture:** a Core Audio process tap, `CATapDescription initStereoGlobalTapButExcludeProcesses:[sunna pids]` → `AudioHardwareCreateProcessTap` → a private aggregate device with `kAudioAggregateDeviceTapListKey` → IOProc [C §3.7; Sunshine `av_audio.mm:629-699`]. `muteBehavior = CATapMuted` when the host setting "Mute this Mac while streaming" is on (default on in Replace placement). TCC: `kTCCServiceAudioCapture` ("System Audio Recording Only"). `NSAudioCaptureUsageDescription` is in the agent's Info.plist. **Silent-zeros detection:** RMS == 0 for 5 s while the output device is active → report `PERMISSION_MISSING(audio)` [C §3.7]. Fallback: SCK audio (covered by the Screen Recording grant).
- **Encode:** Opus, 48 kHz stereo, `OPUS_APPLICATION_RESTRICTED_LOWDELAY`, **10 ms frames** (5 ms in game mode on LAN), VBR with 128 kbit/s target (desktop) or 160 kbit/s (game), complexity 5. **No Opus in-band FEC:** LBRR is a SILK-layer feature and the restricted-low-delay application is CELT-only (UNVERIFIED against the libopus source; the design doesn't depend on it). Redundancy comes from packet-level RED-style duplication instead (§5.1.3), as Selkies does [A §6.6]. **DISAGREE with [S research/03 §7]** ("enable in-band FEC").
- **Surround:** 5.1/7.1 multistream later, with Sunshine's bitrate table [A §1.6].
- **Microphone passthrough (client → host):** needs a virtual audio input device on macOS (an AudioServerPlugIn HAL driver). Deferred to Phase 5.

#### 3.1.6 Cursor

The cursor is never in the video [C S9]. The worker polls the private `CGSCurrentCursorSeed()` at 120 Hz. On change it reads `[NSCursor currentSystemCursor]` image + hotspot (the RustDesk method [C §3.6]), downsamples to ≤ 256×256 physical pixels, hashes it (xxh3-64), and sends `CursorShape{hash, w, h, hot_x, hot_y, scale, rgba (zstd)}` once per hash on the reliable cursor stream. After that it sends only `CursorUse{hash}`. The client caches 64 shapes (RustDesk uses a 64-entry cache [F §1B.1]). Position: the host sends `CursorPos{x, y, visible, seq}` datagrams **only when the host moves the cursor by itself** (app warps, the host-local user, relative-mode games), not as an echo of client motion. The client cursor is the client's own hardware cursor [C §6.3]. "Hidden + pinned" host cursor for 500 ms → offer pointer-lock (relative) mode (§4.2.1).

#### 3.1.7 Input injection

- **Event sources:** a **private** `CGEventSource` for the keyboard (injected modifiers don't mix with local state; RustDesk issue #9729 [C §3.6]); `HIDSystemState` for mouse/scroll; post at `kCGHIDEventTap` for mouse/scroll and `kCGSessionEventTap` for keys (libvirtualhid's placement [C §3.6]). Local-events suppression interval set to 0 when co-driving (default value UNVERIFIED [C §3.6]).
- **Keys:** HID usage (page 0x07) → macOS virtual keycode table (ANSI/ISO/JIS aware). Modifier flags are set on every posted key event. Autorepeat is synthesized host-side from the host's `KeyRepeat`/`InitialKeyRepeat` defaults with `kCGKeyboardEventAutorepeat = 1`, because CGEventPost does not auto-repeat [INFER, S research/07 finding 7]. Text commits (IME, emoji, dead-key results) go through `CGEventKeyboardSetUnicodeString` [C §3.6].
- **Layout policy:** default "positional + layout sync". The client sends its active layout ID (e.g. `com.apple.keylayout.German`) at session start and on change. The host worker switches its input source for the session with `TISSelectInputSource` and restores it on disconnect. Cross-OS mapping tables are used when the host lacks the layout. Option: "translate" mode (unicode injection) for games-off text-heavy use.
- **Mouse:** absolute positions in virtual-display coordinates (points), `kCGMouseEventClickState` from the system double-click interval [C §3.6]. Relative mode: `CGWarpMouseCursorPosition` + delta events (`kCGMouseEventDeltaX/Y`) for games.
- **Scroll:** `CGEventCreateScrollWheelEvent2` (non-variadic; fixes the arm64 variadic bug [S research/07 finding 1]), pixel units, `kCGScrollWheelEventIsContinuous = 1`, with `ScrollPhase` and `MomentumPhase` fields set per event so apps rubber-band natively [C §3.6, S research/05 §4].
- **Gestures:** semantic channel [S research/05 §4]. Pinch/rotate/smart-zoom use private forged gesture CGEvents (experimental, behind a flag). Mission Control, Spaces and App Exposé use shortcut replay (Ctrl+↑/←/→). Swipe-between-pages uses Cmd+[ / ].
- **Pen (from iPad/Wacom clients):** tablet-point mouse events with pressure/tilt fields (`kCGMouseEventSubtype = tabletPoint`, `kCGTabletEventPointPressure`, `kCGTabletEventTiltX/Y`) [INFER: public CGEvent fields; app-level support varies].
- **Permission checks:** poll `CGPreflightPostEventAccess()` every 5 s during sessions; if false → `PERMISSION_MISSING(control)` to the client, because CGEventPost silently drops events without the grant [C §3.3].
- **Release-all:** on disconnect, on focus-loss notification from the client, on permission revocation, and on worker exit.

#### 3.1.8 Clipboard

Lazy, CLIPRDR-style [C S18]. The worker polls `NSPasteboard.changeCount` every 250 ms. On change it sends `ClipOffer{seq, types[], sizes[]}`, skipping `org.nspasteboard.ConcealedType`/`TransientType` and anything over 64 MiB. Content moves only when the peer pastes (`ClipRequest` → a bulk stream). Types: `public.utf8-plain-text`, RTF, HTML, PNG/TIFF, file URLs (file promise → file transfer stream). Plan for the macOS 15.4+ pasteboard-privacy prompt (one-time "Allow") [C §3.7].

#### 3.1.9 Gamepads on a Mac host

No sanctioned virtual game-controller path [S research/03 §6; A §1.5]. Spike (1 week, Phase 4): check whether a CoreHID `HIDVirtualDevice` with an Xbox/DualSense descriptor is recognised by GameController.framework (UNVERIFIED; the negative trackpad result [S research/05 §2] is a warning sign). If yes, ship it. If no, Mac hosts support gamepads only for games that read keyboard/mouse mappings, and we say so in the UI.

#### 3.1.10 Service model, permissions, login window, sleep

```text
Sunna.app (in /Applications; SwiftUI; Sparkle)
 ├─ Contents/Library/LaunchDaemons/app.sunna.daemon.plist   → sunnad (broker)   [SMAppService.daemon]
 ├─ Contents/Library/LaunchAgents/app.sunna.agent.plist     → SunnaAgent.app     [SMAppService.agent]
 │     LimitLoadToSessionType = [Aqua, LoginWindow]; KeepAlive; ProcessType = Interactive;
 │     AssociatedBundleIdentifiers = app.sunna.Sunna
 └─ Contents/Helpers/SunnaAgent.app (LSUIElement, own bundle ID app.sunna.agent, same Team ID)
```

- **Registration:** `SMAppService` (macOS 13+) from inside the bundle. No osascript and no admin-password dialog for the agent [F §7.1]. The daemon needs the user's approval in System Settings › Login Items (status `.requiresApproval` → deep-link with `SMAppService.openSystemSettingsLoginItems()`). Whether `SMAppService.agent` honours `LimitLoadToSessionType = LoginWindow` is **UNVERIFIED**. Fallback: a signed `.pkg` that installs the agent plist into `/Library/LaunchAgents` (RustDesk's model [F §1B.9], but with a pkg instead of an AppleScript).
- **Code identity:** everything Developer ID signed with hardened runtime, notarised, stable bundle IDs and Team ID. TCC grants attach to the agent's designated requirement and survive updates [C §3.3, A6]. Dev builds use a dedicated signing identity too, so grants survive rebuilds.
- **Entitlements:** apply now for `com.apple.developer.persistent-content-capture` to remove the 30-day re-prompt [C §3.3, F §7.1]. Until it's granted, the host warns in the UI ("unattended access may require re-approval after 30 days of non-use"), and MDM users can set `forceBypassScreenCaptureAlert`.
- **TCC buckets the agent needs:** Screen Recording (`kTCCServiceScreenCapture`), Accessibility/PostEvent (control), System Audio Recording (audio). Input Monitoring is not needed on the host [C §3.3].
- **Login window:** the agent's LoginWindow instance runs as root in the LoginWindow session, captures with SCK (14.4+) and injects input to type the password. The broker routes the live connection to it. After login, the Aqua agent starts and the broker re-attaches the connection: the client sees a config-epoch change (new display, new encoder) but no reconnect.
- **Lock screen:** the user's agent keeps capturing (the lock screen belongs to the user session). UNVERIFIED whether SCK frames of the lock screen are delivered; test E-LOCK.
- **Fast user switching:** one agent per GUI session. The broker attaches the viewer to the console session's agent, per Apple DTS's per-session-agent model [C §3.4].
- **Sleep and wake:** while a session is active, `IOPMAssertionCreateWithName(kIOPMAssertionTypePreventUserIdleDisplaySleep)` plus `NSProcessInfo.beginActivity([.userInitiated, .latencyCritical])` [C §3.4]. On connect, declare user activity (`IOPMAssertionDeclareUserActivity`) to wake the display. With "Always available" on (opt-in), hold `PreventUserIdleSystemSleep` while on AC power; otherwise rely on "Wake for network access". Whether an incoming UDP/QUIC packet wakes a sleeping Mac through the Bonjour Sleep Proxy is UNVERIFIED; WoL from a LAN peer is the fallback [F §6.3].
- **Privilege note:** the broker runs as root in v1 (SMAppService daemons run as root unless `UserName` is set; whether a non-root `UserName` works for an SMAppService daemon without a pkg-created user is UNVERIFIED). Mitigation: all network parsing is in `sunna-proto` (no `unsafe`, fuzzed), and dropping privileges after startup is a tracked hardening task (§14).

### 3.2 Linux host (second)

The Linux problem is consent and session ownership, not encoding [C §4]. Sunna uses a **capture ladder**, chosen per session automatically and reported in the UI:

| Rank | Route | When | Capture | Input | Unattended | Greeter |
|---|---|---|---|---|---|---|
| L1 | **Mutter private API** (`org.gnome.Mutter.ScreenCast.RecordVirtual` + `org.gnome.Mutter.RemoteDesktop` with EIS) | GNOME 46+ | PipeWire dmabuf from a client-sized virtual monitor | libei via Mutter's EIS, with keymap [C §2.2, §2.6] | yes | via GDM remote display (GNOME's own flow) [C §2.1] |
| L2 | **KWin private** `zkde_screencast_unstable_v1.stream_virtual_output` | KDE Plasma 6 | PipeWire from a virtual output with DPR [C §2.7] | portal `ConnectToEIS` or KWin fake input | yes | no (use L5) |
| L3 | **xdg-desktop-portal** ScreenCast/RemoteDesktop, `persist_mode = 2`, restore-token rotation, `ConnectToEIS` | any portal backend | PipeWire (`SelectSources` types incl. VIRTUAL=4 where supported) | libei | after one human approval [C §4.1] | no |
| L4 | **wlroots** `ext-image-copy-capture` / screencopy + `zwlr_virtual_pointer_v1`/`zwp_virtual_keyboard_v1` | Sway, Hyprland, … | dmabuf | virtual pointer/keyboard | yes | no |
| L5 | **KMS helper** (opt-in) + uinput helper | greeter, unattended on any compositor, VMs | `drmModeGetFB2` dmabuf, polled at the client refresh rate; the cursor plane is read separately | uinput | yes | **yes** [C §4.1, S16] |
| L6 | X11 (XShm + XDamage + XFixes, XTest) | Xorg sessions | CPU | XTest | yes | Xorg greeters |

Whether Mutter's private D-Bus APIs accept callers other than gnome-remote-desktop in current GNOME releases is **UNVERIFIED and a real risk**. If it is restricted, L1 degrades to L3 on GNOME, plus L5 for unattended access. Spike S-L1 runs in Phase 3 week 1.

- **Process model:** `sunnad` (system, user `sunna`) + `sunna-worker` (a systemd user unit in the graphical session, started via `systemctl --user` activation when a session attaches) + the optional helpers. The KMS helper follows RustDesk's merged design: CAP_SYS_ADMIN only, `PR_SET_NO_NEW_PRIVS`, a seccomp allowlist of about 20 syscalls, dmabuf fds over a socketpair, detiling in the unprivileged worker, binary mode 0750 with group `sunna-capture` [C §4.1, A15].
- **PipeWire negotiation** (copied from g-r-d [C §2.2, §4.1]): BGRx/BGRA with dmabuf modifiers, MemFd fallback, 2–8 buffers, `maxFramerate` = client refresh, `SPA_META_Header` (drop CORRUPTED), `SPA_META_Cursor` up to 384² (feeds the cursor channel), `SPA_META_SyncTimeline` explicit sync when supported, and target by `pipewire-serial`.
- **GPU pipeline** (g-r-d pattern [C §2.4, S12]): dmabuf → Vulkan import (`VK_EXT_image_drm_format_modifier`) → **one compute pass** that emits 16×16-block damage + NV12 (or a 4:4:4 layout) + tile hashes → VAAPI encode from the same Vulkan device. NVIDIA: dmabuf→CUDA via EGL interop (Sunshine's path [A §1.2]), or Vulkan Video encode where the driver is mature.
- **VM / no GPU** (the owner's Linux box is a cloud VM; the kernel string `6.8.0-1069-gcp` suggests GCP [INFER]): x264 in software. On a headless VM there's no compositor to capture, so Sunna starts its **own headless Wayland session**: a Smithay- or wlroots-based headless output (Wolf/Moonshine prove the pattern [A §4.1, §5.1]) sized to the client. This is where the lossless tile path matters most, because desktop content is mostly static [C §4.3].
- **Audio:** PipeWire monitor of the default sink (or a null sink created for headless sessions) [C §4.5].
- **Input:** libei (L1–L3) with keymap-aware keysym → keycode search (g-r-d's `setup_xkb_keymap` approach, so Unicode works regardless of host layout [C §2.6, S14]); otherwise uinput. **Trackpad gestures:** a uinput virtual multitouch touchpad (`INPUT_PROP_POINTER | INPUT_PROP_BUTTONPAD`, `ABS_MT_SLOT`, `BTN_TOOL_*TAP`, `ID_INPUT_TOUCHPAD=1`) so GNOME/KDE run their native gesture recognisers on raw contacts from a Mac trackpad [C S15, S research/05 §5]. Linux hosts get *real* gestures, which no competitor does [S research/05 §7].
- **Clipboard:** the portal Clipboard interface (GNOME), or `ext-data-control-v1`/wlr-data-control (wlroots/KWin) [C §4.5].
- **Gamepads:** uinput Xbox/DualSense-class pads, inputtino-style, with rumble and motion [A §4.4, §5.4].
- **Packaging:** deb/rpm + systemd units (full host); Flatpak for client-only (host features need the native package) [F §7.3].

### 3.3 Windows host (third)

Mostly proven designs from Sunshine, Apollo and Parsec [A §1–3]; Sunna's contribution is the transport and the quality layer, not new capture tricks.

- **Process model:** `SunnaService` (LocalSystem, SCM) + `sunna-worker.exe` launched into the active console session with `CreateProcessAsUser` and a winlogon-derived token for the secure desktop. `SetThreadDesktop`/`OpenInputDesktop` before injecting, so UAC and the lock screen work [F §1B.10, §6.3].
- **Capture:** DDA (`DuplicateOutput1`, FP16 for HDR) primary with dirty/move rects (move rects feed scroll blits later [C §4.7]); WGC fallback (24H2 MPO regressions [S research/03 §1]; `IsBorderRequired = false`; 2-buffer free-threaded pool; `MinUpdateInterval` requested [A §1.2]).
- **Virtual display:** an IddCx driver. Build from SudoVDA's design: IOCTL `ADD {w, h, refresh_mHz, guid, serial}`, `SET_RENDER_ADAPTER`, a watchdog lease pinged every timeout/3, HDR via IddCx 1.10 [C §4.7, A §3.2]. License terms of SudoVDA for redistribution are UNVERIFIED. The alternative is our own IddCx driver with attestation signing. Stable per-client-device serials, as on macOS. Handle Apollo's readiness pitfalls: creation ≠ enumeration ≠ capture-ready [A §8.2 #1531].
- **Encode, NVENC direct** (Sunshine's reference config [A §1.3]): `NV_ENC_TUNING_INFO_ULTRA_LOW_LATENCY`, preset **P1 (game) / P4 (desktop)**, CBR, VBV = `bitrate/fps` (game) or `1.5 × bitrate/fps` (desktop), `frameIntervalP = 1`, infinite GOP/IDR period, `zeroReorderDelay = 1`, lookahead off, AQ off, repeat SPS/PPS, DPB 5 (H.264/HEVC) / 8 (AV1) for RFI, `nvEncInvalidateRefFrames` on loss with a post-RFI marker, IDR fallback; H.264/HEVC 4:4:4 when caps allow; AV1 on Ada+. Slices: 1 (the whole AU is emitted at once anyway [A §10.3 item 7]).
- **AMF:** ultra-low-latency usage, latency-constrained VBR, preanalysis 0, VBAQ off in desktop mode, HRD 0, async depth 1 (Sunshine [A §1.3]). No RFI → LTR if exposed (UNVERIFIED), else IDR with size limits.
- **QSV (oneVPL):** TargetUsage 7 (game) / 4 (desktop), AsyncDepth 1, LowPower on, LowDelayBRC, 4:4:4 where supported [A §1.3].
- **Input:** SendInput (scancodes, relative/absolute mouse, hi-res wheel). Windows 11 **`CreateSyntheticPointerDevice2(PT_TOUCHPAD)`** for native touchpad gestures from Mac clients [S research/05 §5]. `PT_PEN` for pen. **Gamepads:** ViGEmBus fallback (archived, still works [S research/03 §6]), then our own UMDF/VHF driver in Phase 4+.
- **Audio:** WASAPI loopback, event-driven [A §1.6].
- **Packaging:** MSI, Azure Trusted Signing, WinGet [F §7.2].

**Decisions in §3:** SCK-only capture on macOS; CGVirtualDisplay leases with stable per-client serials; an exact VT property set with per-property probing; Core Audio taps; a client-drawn cursor; HID-usage keys with layout sync; SMAppService daemon+agent with a LoginWindow agent. On Linux, a capture ladder with a privilege-separated KMS fallback and uinput trackpad gestures. On Windows, reuse Sunshine/Apollo designs, with NVENC P1 (game) / P4 (desktop).

---
## 4. Clients per OS (plus browser and mobile)

The client is where Sunna currently loses the most. VT decodes to BGRA, then there's a CPU copy, a pixel-by-pixel softbuffer blit, winit's redraw hop and nearest-neighbour scaling [S research/07 finding 2; A §10.4]. The client design follows one principle: **measure presentation, not decode** [C S20, A §2.4], and pick the presentation mode by measured `presentedTime`.

### 4.1 macOS client (first; also the basis for iPadOS)

#### 4.1.1 Decode

- `VTDecompressionSession` with `RealTime = true` (its default [C §6.1]), output `kCVPixelFormatType_420YpCbCr8BiPlanarFullRange` (or `xf44`/`444f` for 4:4:4 streams) as IOSurface-backed buffers (`kCVPixelBufferIOSurfacePropertiesKey`, `kCVPixelBufferMetalCompatibilityKey`). **Asynchronous decode** (`kVTDecodeFrame_EnableAsynchronousDecompression`) on the decode thread, so the net thread never blocks [S research/07 finding 2].
- The stream is B-frame-free with VUI `max_num_reorder_frames = 0`. If an encoder ever omits it, patch the SPS as Moonlight does for macOS [A §2.2].
- Recreate the decoder when `config_epoch` changes or the SPS/PPS/VPS differ. Keep the previous decoder until the new one produces a frame (no black flash).
- Every AU is decoded, including late ones, because references matter. Presentation is a separate decision [A §10.2 Moonlight].
- Hardware decode capability probe at startup (HEVC Main/Main10/RExt 4:4:4, AV1 on M3+), cached per OS build.

#### 4.1.2 Presentation

- **Surface:** `NSWindow` → `NSView` with a `CAMetalLayer`: `opaque = YES`, `pixelFormat = .bgra8Unorm` (or `.bgr10a2Unorm`/`rgba16Float` for EDR later), `colorspace` = the stream colourspace (P3 or sRGB, so macOS colour-manages to the panel [C §6.2 rule 4]), `framebufferOnly = YES`, `displaySyncEnabled` per mode, `maximumDrawableCount = 3` (2 as a measured option [C §6.1]).
- **Render pass:** import the IOSurface planes with `CVMetalTextureCacheCreateTextureFromImage` (R8 + RG8; no CPU copy). One fragment shader does YUV→RGB (matrix/range from the descriptor), **siting-correct chroma upsampling** (bilinear with H.264 type-0 left-centre siting [C §6.2 rule 3]), and composites the **lossless tile overlay** (a texture + a per-tile validity bitmask, §6.2). It samples with **nearest filtering at exactly 1:1** (texel-centre aligned) and a Lanczos-2 path when scaling (window drags, non-matching sizes) [C §6.2 rules 1–2]. Output is an RGB drawable, which direct-to-display requires [C §6.1].
- **Two present modes, chosen automatically and overridable:**
  1. **Arrival mode** (default in native fullscreen on Apple silicon): present as soon as a frame is decoded; `displaySyncEnabled = NO`; the frame goes to the direct-to-display path when macOS grants it. Verify with `presentedTime` deltas and the Metal HUD [C §6.1].
  2. **Display-link mode** (default windowed): `CAMetalDisplayLink` (macOS 14+) with `preferredFrameLatency = 1`, latest-frame-wins, render as late as possible (sample the present slot about 2 ms before the deadline; this is Moonlight's approach [C §6.1]).
  The client A/B-measures `presentedTime − decode_done` for both modes for 2 s after entering fullscreen and on every window-state change, and keeps the lower p95. Never `waitUntilCompleted` on the present thread; release textures in `addCompletedHandler` [C §6.1, A12].
- **Frame pacing policy:** no jitter buffer by default [S research/03 §4]. Optional "smooth" mode (user toggle, default off): hold at most 1 frame to align with vsync when the arrival jitter p95 exceeds half a refresh interval (Moonlight's opt-in pacing [S research/03 §4]).
- **Fullscreen:** native fullscreen Space (direct-to-display needs it [C §6.1]). The menu bar/Dock auto-hide. A "fill vs 1:1" setting; 1:1 is the default, and the host virtual display follows the client size, so these coincide.
- **Telemetry:** `MTLDrawable.addPresentedHandler` → `presentedTime` (T10) for every frame; a decode-to-present histogram in the overlay (§11.2).

#### 4.1.3 Input capture fidelity

| Input | Capture | Wire (§5.1.4) | Notes |
|---|---|---|---|
| Keyboard | `NSEvent` keyDown/keyUp/flagsChanged (focused), `event.keyCode` → HID usage via a table (ANSI/ISO/JIS aware, `KBGetLayoutType`) | HID usage + ordered event list + key-state bitmap | Never characters for key events [S research/03 §6] |
| System shortcuts (Cmd-Tab, Cmd-Space, Mission Control keys) | opt-in "Capture system keys" while focused/fullscreen: an active `CGEventTap` (needs Accessibility on the client) [C §6.3] | as keys | escape chord Ctrl+Opt+Cmd+Esc (configurable) releases capture |
| IME / text input | `NSTextInputClient` on the stream view; committed strings are sent as `TextCommit`; marked text is shown locally (host-IME is an option when the host's layout-synced IME is preferred) | `TextCommit{utf8}` | covers CJK and emoji. The default for CJK users is "host IME" (keys forwarded) with a toggle |
| Mouse | `NSEvent` mouse events with `isMouseCoalescingEnabled = NO`; absolute position in view points → normalised to virtual-display points | absolute (latest wins) | the local hardware cursor shows the host shape |
| Relative (games) | `CGAssociateMouseAndMouseCursorPosition(false)` + raw `deltaX/Y` | **cumulative** i64 counters (loss-proof, §5.1.4) | toggled by a host hint or a hotkey (Cmd+Ctrl+G, RustDesk-like [F §1B.7]) |
| Scroll | `scrollingDeltaX/Y` with `hasPreciseScrollingDeltas`, `phase` and `momentumPhase` | cumulative pixel counters + phase transitions | enables native rubber-banding on Mac hosts [S research/05 §4] |
| Gestures | magnify/rotate/swipe `NSEvent`s when focused; optional private MultitouchSupport raw contacts (experimental) [S research/05 §2–3] | semantic gesture + optional raw contacts | "Send trackpad gestures to host" mode, **fn-hold to route locally** (Citrix pattern [S research/05 §3]) |
| Pen | NSEvent tablet point/proximity (Wacom) | pen events: pressure, tilt, rotation, buttons | |
| Gamepads | GameController.framework (`GCController`): extended gamepad, DualSense touchpad, motion (`GCMotion`, 100 Hz), battery; haptics via `GCDeviceHaptics` | gamepad state snapshots (§5.1.4) | up to 16 pads |

#### 4.1.4 UI

A SwiftUI app (the reason is in §12.2): a menu bar extra (status, quick connect, hosting toggle); a main window with the device list (online dot, path type, last seen); pairing sheets; host settings (hosting on/off, unattended, permissions per peer, placement, audio mute); onboarding checklist (§9). **In-session HUD:** a thin auto-hiding top bar in the stream window with connection quality (a colour dot plus one-line reason), a quality/latency toggle, displays, keyboard-capture toggle, clipboard/file, disconnect. The overlay (§11.2) is toggled by Ctrl+Opt+Cmd+O.

### 4.2 Linux client (second)

- **Decode:** VAAPI (Intel/AMD) or NVDEC via FFmpeg hwcontexts, or Vulkan Video decode where available. Output is a dmabuf.
- **Presentation:** a native Wayland `wl_surface` with **Vulkan** (via `ash`) importing the dmabuf (`VK_EXT_external_memory_dma_buf` + modifiers). Default: FIFO + `presentation-time` feedback. Low-latency mode: `tearing-control-v1` async flips where the compositor supports them; `fifo-v1`/`commit-timing-v1` for pacing [C §6.4]. Direct scanout: a `linux-dmabuf-v1` subsurface on overlay planes when the stream is 1:1 and needs no composite (tiles off). Kiosk/handheld: a DRM plane via an atomic commit (Moonlight's drm renderer [A §2.2]). X11 fallback: Vulkan on an Xlib surface.
- **Why not wgpu:** wgpu has no `presentedTime`/presentation feedback and needs HAL escapes for dmabuf import [C §6.4]. Rejected for the stream surface (decision D-18).
- **Input:** Wayland `wl_keyboard` (evdev keycodes → HID usage), `zwp_relative_pointer_v1` + `zwp_pointer_constraints_v1` for relative mode, `wl_pointer.axis_value120`/`axis_source`/`axis_stop` for scroll phases, `zwp_pointer_gestures_v1` (swipe/pinch/hold) for gestures, `zwp_tablet_v2` for pens, `zwp_keyboard_shortcuts_inhibit_v1` for capturing system keys. SDL3 for gamepads.

### 4.3 Windows client (with the Windows host, Phase 4)

- **Decode:** D3D11VA (DXVA2 fallback) [A §2.2].
- **Presentation:** a flip-model swapchain (`FLIP_DISCARD`), `DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT` + `SetMaximumFrameLatency(1)`, `ALLOW_TEARING` + `Present(0, DXGI_PRESENT_ALLOW_TEARING)` in fullscreen/VRR, independent flip/MPO in borderless fullscreen [C §6.4]. Moonlight's caution about max-frame-latency blocking applies [A §2.2]: the waitable object avoids it. `DXGI_FRAME_STATISTICS` gives present times.
- **Input:** Raw Input (`WM_INPUT`) for unaccelerated relative motion at device rate, low-level keyboard hook for system keys in capture mode, `WM_POINTER` for pen/touch, Precision Touchpad raw contacts via HID for gestures (UNVERIFIED feasibility), SDL3/GameInput for gamepads [S research/03 §6].

### 4.4 iPad (Phase 5) and iPhone (later)

The same Swift + Metal client code. VT decode, `CAMetalLayer` on `UIView`. Input: hardware keyboard (`UIPress` HID usages), trackpad/mouse (`UIPointerInteraction`, pointer lock with `prefersPointerLocked`), **Apple Pencil** (pressure, altitude/azimuth, hover on M2 iPads) → host tablet events, and touch → absolute tap or trackpad-emulation modes. Stage Manager and external displays are supported. iPad is the "creative" wedge against Parsec Warp's Wacom feature [S research/04 §4 item 5].

### 4.5 Browser client (Phase 5): guest links and zero-install

- **Stack:** WebTransport (datagrams + streams) + WebCodecs (`optimizeForLatency: true`) + a WebGPU/WebGL2 presenter + a WebAudio worklet. The same wire format as native.
- **Connectivity reality:** browsers can't hole-punch arbitrary UDP. They connect over WebTransport to a **Sunna relay** (with a WebTransport front end) or directly to hosts with a public address, using `serverCertificateHashes` (self-signed certificates valid ≤ 14 days) (UNVERIFIED current browser support matrix; Safari support for WebTransport is the key unknown).
- **E2E through the relay:** the relay terminates WebTransport TLS, so an **inner E2E layer** runs over it: a Noise `NK` handshake to the host's static key, which is taken from the guest link's URL fragment (never sent to servers) [INFER]. Media is AEAD-sealed per datagram inside that layer. This costs about 1–3 ms of WASM crypto per frame at 20 Mbps (UNVERIFIED).
- **Scope:** guests, quick support, Chromebooks. Never the primary client (browser presentation adds uncontrollable buffering [A §6.6]).

### 4.6 Android (Phase 6)

MediaCodec low-latency decode (`KEY_LOW_LATENCY`) direct to `SurfaceView`, `setFrameRate`, gamepads via `InputDevice` [S research/06 §1]. Rust core via JNI/UniFFI; a Kotlin/Compose shell.

**Decisions in §4:** a native presenter per OS (Metal/`CAMetalLayer`, Vulkan on Wayland, D3D11 flip model), not a cross-platform renderer. Presentation mode is chosen by measured `presentedTime`. Decode is async and separate from the net thread. Loss-proof cumulative counters carry relative motion and scroll. The browser is for guests only, and has an inner E2E layer.

---
## 5. Media transport

### 5.0 Principles

1. **Media never rides a reliable, ordered stream.** Video, audio, input, cursor position and feedback are QUIC DATAGRAMs (RFC 9221). Head-of-line blocking is what makes RustDesk, RDP and HP's input stall [F §1B.14, C §1.4, C §5.3, A1].
2. **One controller owns the rate.** The application rate controller (`sunna-ratectl`) owns the target bitrate, the pacing rate and the FEC budget. The QUIC congestion controller is a *follower* that only enforces a safety window. Two controllers fighting is the failure mode [A §10.3 item 5].
3. **Deadlines, not retries.** Every video frame has a deadline (capture time + latency budget). Repair (FEC, repair-on-demand, reference refresh) is chosen by whether it can still arrive in time [A §10.4].
4. **Reference-aware loss handling.** Losing a frame triggers a reference repair (LTR refresh / RFI) that is sized like a P-frame, not an IDR [C S5, A §10.3 item 3].
5. **Priority is ours, not quinn's.** quinn keeps one FIFO of outgoing datagrams with drop-oldest [V: quinn-proto `connection/datagrams.rs` `make_space_for`], and packs DATAGRAM frames ahead of STREAM frames in each packet [V: quinn-proto `connection/mod.rs` DATAGRAM block ~L3434 before STREAM ~L3489]. So Sunna runs its own priority scheduler and pacer and hands quinn only what should go out now.

### 5.1 Packet formats (byte layouts)

All multi-byte integers are **big-endian** (as the current media header is [S crates/proto/src/media.rs]). Sizes are in bytes. Every datagram starts with the same 8-byte prefix, so feedback covers all kinds with one sequence space (transport-wide sequence numbers, as in TWCC/RFC 8888 [S research/03 §4]).

#### 5.1.1 Common prefix (8 bytes)

```text
off len field      meaning
0   1   vk         bits 7..6 = protocol version (01); bits 5..0 = kind
1   1   flags      kind-specific
2   2   tsn        transport sequence number, per direction, wrapping u16; receiver extends to u64
4   4   send_us    sender monotonic clock, µs since session epoch, mod 2^32; stamped by the pacer
                   at hand-off to QUIC (not at packetization)
```

Kinds: `0x01 VIDEO`, `0x02 AUDIO`, `0x03 INPUT` (c→h), `0x04 FEEDBACK` (c→h), `0x05 CURSOR` (h→c), `0x06 PROBE`, `0x07 FAST_CTL` (both directions), `0x08 DEVICE_FB` (h→c: rumble/LED/haptics), `0x09 HOST_ACK` (h→c: input acks and host receive stats).

#### 5.1.2 VIDEO shard (20-byte header)

```text
off len field         meaning
0   1   vk            0x41
1   1   flags         b0 REPAIR (shard_index ≥ k) · b1 KEY · b2 DISCARDABLE (non-reference / enhancement
                      layer) · b3 ONDEMAND (repair sent in answer to a NACK) · b4 BURST_END (last shard of the
                      initial send of this frame) · b5 LTR_REFRESH · b6,b7 reserved
2   2   tsn
4   4   send_us
8   1   stream_id     video stream (one per host display in the session), 0..15
9   1   layer         temporal layer id (0 = base)
10  4   frame_id      per-stream wire frame counter; increments for every frame that left the encoder,
                      including ones dropped at the sender (their gap is real) [S host media_loop]
14  2   shard_index   0..k−1 data, k.. repair (Leopard RS indices; up to 65535 total)
16  2   k             data shards for this frame (1..32768)
18  2   shard_len     bytes per shard (even; all shards of a frame are equal length)
20  …   shard payload (shard_len bytes)
```

The **frame payload** before sharding is `descriptor || bitstream`, zero-padded to `k × shard_len`. Because the descriptor sits inside the protected payload, FEC recovers it as well.

**Frame descriptor (v1):**

```text
off len field               meaning
0   1   desc_ver            1
1   1   codec               1 H.264 · 2 HEVC · 3 AV1
2   2   dflags              b0 KEY · b1 LTR_MARK (ack requested) · b2 LTR_REFRESH · b3 CONFIG (parameter
                            sets in-band) · b4 NON_REF · b5 DAMAGE_FULL · b6 HDR_META · b7 PROBE_FRAME
4   4   bitstream_len
8   8   capture_us          T1: host monotonic µs (session epoch); SCK displayTime converted
16  2   encode_d            T3 − T1 in 10 µs units (saturating)
18  2   enqueue_d           T4 − T1 in 10 µs units (saturating)
20  2   config_epoch        must match the client's active decoder configuration
22  1   temporal_layer
23  1   damage_count        0..64 (0 with DAMAGE_FULL)
24  8   ltr_token           encoder LTR token for LTR_MARK frames (VT token / NVENC LTR index), else 0
32  4   ref_frame_id        for LTR_REFRESH: frame_id of the acknowledged reference used; else 0
36  8×n damage rects        {x u16, y u16, w u16, h u16} in backing pixels
36+8n   bitstream           Annex B (N NAL units allowed)
```

Overhead at 1200-byte datagrams: 20 bytes of Sunna header + about 30–40 bytes of QUIC (short header with an 8-byte CID, packet number, AEAD tag, DATAGRAM frame type and length) → about 5%.

#### 5.1.3 AUDIO (18-byte header)

```text
0   1   vk          0x42
1   1   flags       b0 RED (redundant frames follow) · b1 DTX (silence)
2   2   tsn
4   4   send_us
8   4   seq         audio frame sequence
12  4   cap_us_lo   low 32 bits of capture µs
16  2   primary_len
18  …   primary Opus frame; then 0..2 × {seq_back u8, len u16, opus bytes} redundant copies of
        frames seq−seq_back (RED-style; default 2 back on WAN, 1 on LAN)
```

#### 5.1.4 INPUT (client → host)

```text
0   1   vk          0x43
1   1   flags       b0 HAS_STATE
2   2   tsn
4   4   send_us
8   4   first_seq   input_seq of the first event in this datagram
12  1   n_events
13  1   n_resent    how many leading events are retransmissions (not yet acked by HOST_ACK)
14  64  state       (if HAS_STATE; sent in every datagram by default)
          [0..32)   keys_down bitmap, 256 bits, indexed by HID usage (page 0x07, usages 0..255)
          32        modifiers (HID bit order) · 33 mouse_buttons · 34 locks (caps/num/scroll) · 35 reserved
          [36..44)  abs_x, abs_y  i32 each, virtual-display points × 16
          [44..52)  rel_total_x, rel_total_y  i32 each, wrapping, device units × 64 (cumulative since session start)
          [52..60)  scroll_total_x, scroll_total_y  i32 each, wrapping, pixels × 16 (cumulative)
          60 scroll_phase · 61 momentum_phase · 62 gesture_phase · 63 reserved
78  …   events: n_events × {type u8, len u8, payload}
          0x01 KEY {usage u16, down u8}           0x02 TEXT {utf8}        0x03 BUTTON {btn u8, down u8}
          0x04 SCROLL_PHASE {phase u8, momentum u8}  0x05 GESTURE {kind, phase, fingers, dx, dy, vx, vy,
          scale_log, rot}  0x06 CONTACTS {n, [id, x, y, major, minor, pressure, state]}  0x07 PEN {x, y,
          pressure u16, tilt_x i16, tilt_y i16, rot u16, buttons, proximity}  0x08 GAMEPAD {pad u8,
          buttons u32, lx ly rx ry i16, lt rt u16, touch[2], gyro i16×3, accel i16×3, t u16}
          0x09 GAMEPAD_ARRIVE {pad, kind, caps}  0x0A GAMEPAD_LEAVE  0x0B FOCUS_LOST  0x0C LAYOUT {id str}
```

The semantics make input **loss-proof without head-of-line blocking**:

- Relative motion and scroll are carried as cumulative totals; the host applies `wrapping_sub(new, last)`. A lost datagram loses no motion, and a duplicate adds none. This is the "unreliable relative motion with explicit loss accounting" that Moonlight still has as a TODO [A §1.5].
- Key, button and text events carry `input_seq`. The host processes them in order and skips any `seq ≤ last_applied`. The client keeps unacked events and re-sends them at the head of the next datagram (`n_resent`) until `HOST_ACK.input_ack ≥ seq`. Retransmission happens by piggybacking on the next input packet, or within 1 RTT + 5 ms if input goes quiet. Transitions end up reliable and ordered; motion is never delayed behind them.
- The `keys_down` bitmap in every datagram lets the host reconcile: any key the host believes is down but the client says is up gets released within one packet. This fixes stuck keys [S research/07 finding 7] without RustDesk's periodic sweep [F §1B.7].
- Absolute position: latest wins. Gamepad state: a full snapshot per change plus a 50 ms heartbeat (idempotent).

**HOST_ACK (host → client, every 20 ms while input flows, 16 bytes):** prefix + `input_ack u32` + `host_rx_loss_permille u16` + `inject_latency_p95_us u16`.

#### 5.1.5 FEEDBACK (client → host)

Sent every **10 ms** while video flows, immediately when a NACK or loss decision is made, and every 100 ms otherwise.

```text
0   1   vk          0x44
1   1   flags       b0 URGENT (contains NACK/LOSS)
2   2   tsn (client's)       4 4 send_us (client clock)
8   2   base_tsn    first host tsn covered
10  2   count       number of host packets covered (≤ 1024)
12  4   base_arrival_us   client clock arrival time of base_tsn (or of the first received after it)
16  ⌈count/4⌉  status  2 bits/packet: 00 missing · 01 received, delta in 1 byte · 10 received, delta in 2 bytes
        deltas      per received packet: u8 (250 µs units) or i16 (250 µs units) relative to the previous arrival
        frame block:
          last_complete_frame u32 · last_decoded_frame u32 · last_presented_frame u32
          decode_queue u8 · present_mode u8 · present_p95_us u16 · local_drops u16
          n_ltr_ack u8 + n × {stream u8, frame_id u32, token u64}          (decoded successfully)
          n_nack u8 + n × {stream u8, frame_id u32, shards_needed u16}      (repair-on-demand)
          n_loss u8 + n × {stream u8, first_lost_frame u32}                 (request reference refresh)
```

#### 5.1.6 CURSOR (host → client, 28 bytes)

`prefix · flags b0 visible, b1 host_moved, b2 relative_hint · seq u16 · x i32, y i32 (points × 16) · shape_hash u64 · stream_id u8 · pad u8`. Shapes themselves travel on the cursor stream (§5.1.8).

#### 5.1.7 FAST_CTL and PROBE

`FAST_CTL`: prefix + `msg_id u32` + postcard `FastControl {LtrRefreshRequest{stream, first_lost}, KeyframeRequest{stream}, PauseVideo, ResumeVideo, EpochNotice{stream, epoch}}`. The same message is also sent on control stream 0; the receiver dedupes by `msg_id`. Latency-critical control therefore never waits behind a retransmission, but still arrives if datagrams are lost. `PROBE`: prefix + `probe_id u16` + padding (used only when no FEC bytes can serve as a probe, §5.4.4).

#### 5.1.8 QUIC streams

| Stream | Direction | quinn priority | Content |
|---|---|---|---|
| 0 (bidi, client-opened) | both | 100 | **Control**: length-prefixed postcard messages (as today [S transport]), read by a dedicated task because `recv` is not cancellation-safe [S research/07 finding 1] |
| uni h→c "cursor" | h→c | 90 | `CursorShape{hash, w, h, hot, scale, rgba_zstd}` |
| uni h→c "tiles" | h→c | 10 | lossless tile records (§6.2.3) |
| bidi per transfer | both | 0 | clipboard contents, file chunks (BLAKE3-verified, resumable) |
| uni c→h "telemetry" (optional) | c→h | 5 | per-frame client records for the host's flight recorder |

Control message set (postcard enum, versioned by `proto_ver`): `Hello{proto_ver, device_id, caps}`, `HelloAck`, `SessionOffer{displays, codecs, chroma, hdr, max_fps, modes}`, `SessionAccept`, `ConfigEpoch{stream, epoch, codec, w, h, fps, colour, chroma}`, `ResizeRequest`, `DisplayLayout`, `PermissionState{screen, control, audio, …}`, `HostStatus`, `Ping/Pong` (clock sync at min-RTT [S client]), `ClipOffer/ClipRequest`, `FileOffer`, `AppList/AppLaunch`, `ModeHint{desktop|motion|game}`, `Stats`, `Bye{reason}`, `Error{code, detail}`.

### 5.2 FEC, repair-on-demand and retransmission policy

**Code:** systematic Reed–Solomon over GF(2^16) (the Leopard construction) via the `reed-solomon-simd` crate. There is one block per frame, k ≤ 32768 data shards, and any k of the k+r shards decode the frame. This replaces the current XOR parity, which recovers one loss per 8 and fails about 56% of groups at 20% loss [S research/07 finding 6]. It also avoids Sunshine's 255-shard, 4-block limit, under which FEC is disabled for huge IDRs [A §1.4], and the XOR-subset weakness of FlexFEC [A §6.4]. MDS codes recover bursts within a frame as well as random loss. (Crate constraints, e.g. an even `shard_len`, and throughput are UNVERIFIED; benchmark B-FEC in Phase 1. The fallback is a GF(2^8) Cauchy RS for k+r ≤ 255.)

**Initial parity** `r` per frame, from the smoothed packet-loss rate `L` (EWMA, 1 s) and the mean burst length `B` (from FEEDBACK status runs):

| Regime | r (normal P-frame) | Key / LTR-refresh frames | Discardable frames |
|---|---|---|---|
| L < 0.1% and RTT < 0.5 × frame interval (clean LAN) | `k ≤ 16: 1`; else `⌈0.02k⌉` | `max(2, ⌈0.05k⌉)` | 0 |
| L < 1% | `⌈0.05k⌉ + 1` | `⌈0.10k⌉ + 2` | 0 |
| 1% ≤ L < 3% | `⌈0.10k⌉ + max(2, B)` | `⌈0.20k⌉ + B` | `⌈0.05k⌉` |
| 3% ≤ L < 10% | `⌈0.25k⌉ + B` | `⌈0.35k⌉ + B` | `⌈0.10k⌉` |
| L ≥ 10% | `⌈0.50k⌉ + B` (and the rate controller cuts, §5.4) | same | 0 (layer dropped) |

The FEC bytes come out of the rate budget: `video_target = total_target × 1/(1 + r/k) − audio − refinement_share` [A §10.3 item 4]. **Send order:** data shards first, then repair, paced as one train (§5.4.5). A frame then completes at the earliest possible moment when nothing is lost.

**Repair-on-demand (instead of classic RTX).** When the client's reassembler sees `BURST_END` (or the next frame's first shard) and the frame still lacks `m` shards beyond what the received repair can fill, it checks the deadline:

```text
deadline(frame) = capture_us + budget
budget = LAN:  max(3 × frame_interval, 40 ms)      WAN: max(3 × frame_interval, srtt + 2 × frame_interval)
can_repair = now_host_est + srtt + 1.2 × m × shard_len × 8 / pacing_rate  <  deadline
```

If `can_repair`, it sends `NACK{stream, frame_id, shards_needed = m + 1}` (urgent FEEDBACK). The host generates `m + 1` **new** RS repair shards (fresh indices; any of them fills any hole), sends them at priority P2 with `ONDEMAND`, and keeps the encoded frame in the history (Q20). Because any repair shard fills any hole, one round trip suffices whatever the loss pattern, which is better than per-packet retransmission. If not `can_repair`, the frame is declared lost → reference repair (§5.3). On LAN (RTT ~1–3 ms) repair-on-demand is almost free and FEC stays minimal. On WAN, FEC carries most recovery, as the RTT-vs-frame-interval rule predicts [S research/03 §4].

**Reorder tolerance.** The reassembler holds up to 8 frames in flight (Q10) and never declares loss on a gap until `BURST_END` + a reorder guard of `max(1 ms, 2 × jitter_p50)` has passed. This fixes the single-partial-frame design that turns reordering into IDR storms [S research/07 finding 4].

**Audio:** RED-style redundancy (1 back on LAN, 2 back on WAN). With 10 ms frames, one lost packet costs nothing, and a burst of 3 falls back to Opus PLC. No NACK for audio.

**Input:** reliable by acknowledgement and piggyback, as described in §5.1.4.

### 5.3 Reference repair

**Host side, VideoToolbox (Mac hosts):**

1. `EnableLTR = true`. The encoder marks some frames with `RequireLTRAcknowledgementToken`, and the host sets `LTR_MARK` + `ltr_token` in the descriptor.
2. The client acknowledges a token **after that frame has decoded successfully** (not when displayed) [A §2.1 note on ACK semantics]. The host passes new tokens to VT in the next frame's options as `AcknowledgedLTRTokens` [C §3.2].
3. On a LOSS report (or a NACK deadline miss) the host sets `ForceLTRRefresh` on the next frame. VT emits a P-frame predicted from an acknowledged LTR. The descriptor gets `LTR_REFRESH` and `ref_frame_id`. If nothing is acknowledged, VT falls back to an IDR [C §3.2], and the rate controller treats the IDR as a planned burst (§5.4.6).
4. Refresh requests are coalesced: at most one per `srtt + frame_interval`; later losses before the refresh arrives are covered by it.
5. Per-codec LTR availability on M1/M2 (H.264 vs HEVC) is UNVERIFIED. The startup probe tests it, and the fallback is IDR with the size cap and priority boost.

**Host side, other encoders:** NVENC uses RFI (`nvEncInvalidateRefFrames` over the lost range with a DPB of 5–8, as Sunshine does [A §1.3]), with IDR fallback. Vulkan Video uses Moonshine-style short-term reference invalidation [A §5.2]. VAAPI and AMF have no established RFI [A §1.3], so they use IDR with a size cap, plus `DataRateLimits`-style spike control where available.

**Client decoder state machine:**

```text
            ┌─────────────── frame decodes OK ───────────────┐
            ▼                                                │
   ┌──────────────┐  frame lost (deadline) / decode error  ┌─┴───────────────┐
   │   DECODING   │ ─────────────────────────────────────► │  AWAIT_REPAIR   │
   └──────────────┘   send LOSS{first_lost} (urgent)        │ drop every frame│
          ▲           keep showing last good frame          │ whose reference │
          │                                                 │ chain includes  │
          │  LTR_REFRESH with ref_frame_id we hold,         │ a lost frame    │
          └──────── or KEY frame ◄──────────────────────────┤ re-send LOSS    │
                                                            │ every srtt+10ms │
                                                            └─────────────────┘
   AWAIT_REPAIR for > 1 s  ⇒ request KEY via FAST_CTL; > 5 s ⇒ decoder reset + KEY.
```

"Drop every frame whose reference chain includes a lost frame": with VT LTR the dependency isn't explicit in the bitstream the client parses. So in AWAIT_REPAIR the client decodes *nothing* until an `LTR_REFRESH` or `KEY` arrives. The one exception is DISCARDABLE frames, which never serve as references. The gap is short: about 1 RTT + 1 frame. The screen shows the last good frame (never corruption), and the overlay notes "repairing".

### 5.4 Rate control coupled to the encoder (`sunna-ratectl`)

A sans-IO state machine driven by FEEDBACK (every 10 ms), by frame events from the encoder, and by path events. Design lineage: per-frame delivery trains as capacity probes (SQP [S research/03 §4]), a delay-gradient signal (GCC), the fast drain and hold of Selkies' pacer [A §6.4], and g-r-d's ack-clocked window [C §2.5, S7]. The current 1 s AIMD with a never-decaying baseline [S research/07 finding 5] is replaced.

#### 5.4.1 Signals

- **One-way queuing delay** `q`: `owd_i = arrival_i − send_us_i` (with an unknown constant offset). `q = owd − min(owd over the last 10 s)`. The min-window is reset on path change. `q_trend` = least-squares slope of owd over the last 100 ms.
- **Delivery rate per frame train**: for trains of ≥ 8 packets, `rate = bytes(2..n) / (arrival_n − arrival_1)`. `capacity_est` = max-filter over 2 s of train rates when the train was paced at ≥ 1.2 × the measured rate (so it was not application-limited). A static desktop sends tiny frames, so it produces no capacity samples, and it is treated as **app-limited**, never as low capacity [A §6.4 Selkies].
- **Loss** `L` and burst `B` from FEEDBACK status bits. Loss is **congestive** if `q > q_hi` or `q_trend > 0` over the 50 ms before it, and **random** otherwise.
- **ECN CE marks** (quinn reports `is_ecn` congestion events [V: quinn-proto `congestion.rs` `on_congestion_event`]) are treated as congestive. This makes Sunna L4S-ready where paths mark [S research/03 §4].
- **Client pressure**: `decode_queue > 1` or `present_p95` rising → receiver-limited (don't send faster than the client can present) [C §1.4 S7].

#### 5.4.2 Thresholds (starting values, tuned in the simulator)

| | LAN path (srtt < 5 ms) | WAN path | Relay path |
|---|---|---|---|
| `q_lo` (grow below) | 2 ms | 4 ms | 6 ms |
| `q_hi` (drain above) | max(6 ms, 0.4 × frame interval) | max(12 ms, 0.75 × frame interval) | 25 ms |
| growth per 10 ms in STEADY | +1.0% (≈ +100%/s ceiling) | +0.5% | +0.3% |
| decrease on DRAIN | target = 0.85 × min(recv_rate_100ms, target) | 0.8 × | 0.7 × |
| hold after DRAIN | srtt + 50 ms | srtt + 100 ms | srtt + 200 ms |

#### 5.4.3 States

```text
STARTUP ──(first capacity sample or 500 ms)──► STEADY ◄──(hold expired, q < q_lo)── DRAIN
   │ start at min(cap, 8 Mbps LAN / 4 Mbps WAN);                ▲                      ▲
   │ ×1.5 per 100 ms while q < q_lo and no congestive loss      │                      │
   └──(q > q_hi or congestive loss)────────────────────────────►┴──(q > q_hi, CE, or congestive loss)
STEADY: target += growth (table) if q < q_lo and target < 0.9 × capacity_est (or capacity unknown & PROBING ok)
        hold if q_lo ≤ q ≤ q_hi
PROBE (sub-state of STEADY, app-limited desktop): every 2 s send the next frame's FEC parity (or PROBE
        padding) as a train at 1.5 × target to refresh capacity_est; abort if q rises > q_lo.
DRAIN:  target cut (table); pause the tile stream; drop DISCARDABLE layers; admission budget halved;
        exit after hold.
Random loss (non-congestive): no rate cut; raise FEC regime (§5.2); if L_random > 10% for 2 s, cut 20% anyway
        (circuit breaker).
```

The initial target and ceiling are user-configurable. Defaults: ceiling 150 Mbps on LAN (HP recommends 75 Mbps per 4K display [C §5.1]; Moonlight allows 500 [S research/04 §3]) and 40 Mbps on WAN. The ceiling is auto-raised if `capacity_est` supports it.

#### 5.4.4 Coupling to the encoder (next frame, not next second)

Every frame, the worker reads the shared rate block (an atomic snapshot written by the broker) *before* submitting to the encoder:

- `video_bps = target × k/(k+r) − audio_bps − refinement_share` → `AverageBitRate` (and `DataRateLimits` where supported). The encoder reacts per frame [S research/03 §4 integration rule].
- **fps policy by mode:** *Desktop*: keep quality, drop fps. If `video_bps / fps` falls below the bits needed for QP ≤ `MaxAllowedFrameQP` (inferred when VT reports drops at max QP), step the SCK `minimumFrameInterval` through 60→45→30→20→15 fps, and restore when headroom returns for 2 s. *Game*: keep fps and let QP rise to 44. Only below 3 Mbps at 1440p does it drop to 0.75× render scale (a config epoch plus IDR, at most once per 10 s).
- **Admission gate (pre-encode drop; Q2):** `projected_ms = (bytes queued in P2..P4 + ewma_frame_bytes[type]) × 8 / pacing_rate`. If `projected_ms > budget` (LAN: max(frame interval, 8 ms); WAN: 2 × frame interval), skip this capture. The encoder never sees it, so references stay intact [A §1.1].
- **Receiver-limited:** if the client reports `decode_queue > 1` for 3 consecutive reports, lower the fps cap to the client's measured present rate.

#### 5.4.5 Pacing

A token bucket in the broker's media thread. `pacing_rate = max(1.25 × target, min(capacity_est, 4 × target))`. The bucket depth is `max(2 ms × pacing_rate, 2 × MTU)`. Hand-off to quinn happens in GSO-sized batches of ≤ 16 packets when the bucket allows. The frame train therefore leaves in `size / pacing_rate` (a 60 KB frame at 50 Mbps target with capacity 300 Mbps leaves in about 2 ms). That is fast enough to add no latency, and it doesn't burst at line rate into shallow Wi-Fi buffers [S research/03 §3]. Sunshine's fixed 800 Mbit/s budget is the anti-pattern [A §1.4].

#### 5.4.6 Large frames (IDR, LTR refresh after long outage, resize)

Planned bursts. The admission gate reserves `2.5 × ewma_P_frame` of budget, the following one or two captures are skipped if needed, the frame gets key-frame FEC, and `DataRateLimits` is set to allow 3 frame intervals of bytes for that frame only.

### 5.5 Stream prioritisation

Strict priority in the broker scheduler (datagrams), plus quinn stream priorities and app-level byte budgets for streams:

| Class | What | Mechanism | Budget |
|---|---|---|---|
| P0 | FEEDBACK, INPUT, HOST_ACK, CURSOR, FAST_CTL, control stream 0 | datagram head-of-queue; control stream priority 100 | unmetered (tiny) |
| P1 | AUDIO | datagram | its bitrate |
| P2 | on-demand repair shards | datagram, deadline-tagged | ≤ 1 RTT of bytes |
| P3 | video base layer (current frame train) | datagram | `video_bps` |
| P4 | video DISCARDABLE layers | datagram; dropped first under pressure | part of `video_bps` |
| P5 | refinement tiles | uni stream priority 10 | `refinement_share` = 20% of target in STEADY with headroom; 0 in DRAIN |
| P6 | clipboard, files | bidi streams priority 0 | ≤ 10% of target while video is active; all spare capacity when idle |

Streams are written only when the app budget allows, because QUIC stream bytes consume the same congestion window [A §7.1]. That quinn packs datagrams ahead of stream frames in a packet [V] also keeps bulk from delaying media inside a packet.

### 5.6 Encryption

QUIC TLS 1.3 with **raw public keys** (Ed25519, RFC 7250) for **both** peers (mutual authentication), verified against the pinned trust store (§8.2), never a CA and never skip-verify [S transport dev TLS]. Cipher preference: `TLS_AES_128_GCM_SHA256` (hardware AES on Apple silicon and x86); ChaCha20 for devices without AES. Session keys are per connection, giving forward secrecy. There is no second media-encryption layer (QUIC protects every datagram). Unique nonces come from QUIC packet numbers, so Wolf's repeated-GCM-nonce class of bug can't arise [A §4.3]. Relays see only QUIC ciphertext [F §6.2]. Browser guests use an inner Noise layer (§4.5).

### 5.7 MTU handling

- quinn/noq `initial_mtu = 1200`, DPLPMTUD on, `upper_bound = 1452` (Ethernet minus IPv6+UDP). On paths through tunnels (Tailscale utun MTU 1280), the effective max datagram payload settles around 1200 − QUIC overhead [F §3; C §5.4 net notes].
- `shard_len = even(max_datagram_size() − 20)`, read **per frame** at packetization, so an MTU change applies at the next frame. If quinn drops an oversized queued datagram after the MTU shrinks, that counts as a local loss, and the frame is repaired by FEC or on demand. Moonshine's MTU-1280 tunnel issue is the lesson [A §8.2 #210].
- No IP fragmentation ever (QUIC forbids it). The CURSOR, INPUT and FEEDBACK datagrams are sized to stay under 1200 bytes.

### 5.8 QUIC implementation details (quinn/noq customisation)

- **Crate:** Sunna's `MediaConnection` trait wraps a QUIC connection with datagrams, streams, stats, `max_datagram_size`, `datagram_send_buffer_space` and path events. Implementations: upstream quinn (LAN/dev, direct addresses, today's code) and **iroh's noq** (production connectivity, §7). Both expose the same congestion `Controller`/`ControllerFactory` trait [V: quinn-proto `congestion.rs:17`; iroh re-exports `noq_proto::congestion::{Controller, ControllerFactory}` in `iroh/src/endpoint/quic.rs:69-72`].
- **`SunnaFollower` congestion controller:** `window() = max(2 × target_Bps × srtt, 16 × MTU)`. `on_congestion_event` forwards `(lost_bytes, is_ecn, persistent)` to ratectl and only **halves the window on persistent congestion** (safety). `on_ack` feeds RTT. Pacing is left to Sunna's pacer. Because the window is about 2 × BDP at the target rate, quinn's internal pacer and window never bind before Sunna's pacer does. This fulfils the "custom controller" decision [S research/07 decisions] without two controllers fighting.
- **Transport config:** ALPN `sunna/1`; `max_idle_timeout 15 s`; `keep_alive_interval 1 s` during sessions (NAT bindings); `datagram_receive_buffer_size 2 MiB`; `datagram_send_buffer_size 1 MiB` (kept near-empty); stream receive window 8 MiB (tiles/bulk); `max_concurrent_bidi_streams 64`, uni 16; `initial_rtt 30 ms`; MTU discovery as §5.7; GSO/GRO via quinn-udp. On macOS evaluate noq's `fast-apple-datapath` feature [V: iroh `Cargo.toml:158`]. ECN enabled.
- **Datagram hand-off:** always `send_datagram` (never `send_datagram_wait`) and only when `datagram_send_buffer_space() ≥ batch_bytes`. Otherwise the batch stays in Sunna's scheduler, where priority and deadline logic apply [A §7.1].
- **0-RTT / resumption:** reconnection uses a Sunna session-resume token in the first control message (1 RTT). QUIC 0-RTT is not relied on (UNVERIFIED with RPK).

### 5.9 Relay-aware behaviour

`sunna-net` publishes path events: `{kind: LAN | DIRECT | RELAY_TCP | RELAY_UDP, rtt, mtu}`. iroh multipath can move traffic between paths inside one connection [F §3b.1].

| Path kind | FEC | Repair-on-demand | Rate ceiling | ratectl thresholds | Other |
|---|---|---|---|---|---|
| LAN | minimal (clean-LAN row) | yes | 150 Mbps default | LAN column | tiles at full share |
| DIRECT (WAN) | loss-adaptive | yes, deadline permitting | 40 Mbps default, auto | WAN column | temporal layers on |
| RELAY_TCP (iroh relay over HTTPS/WebSocket) | **off** (TCP hides loss as delay) | **off** | **20 Mbps** (UNVERIFIED relay capacity; DERP-class relays drop above small queues [F §3.8]) | relay column; delay-only | DISCARDABLE layers always on; tiles at half share |
| RELAY_UDP (sunna-relay) | loss-adaptive | yes | 40 Mbps | WAN column with srtt | — |

**On a path switch:** reset `min_owd`, halve `capacity_est` and re-enter STARTUP from 0.7 × the current target. **Do not** reset the decoder, because the QUIC connection persists. A path switch is invisible except for a short quality dip. When a direct path appears, iroh moves traffic; `upgrade` is logged and shown in the UI ("Direct connection established").

**Decisions in §5:** datagram-only media with a common 8-byte prefix and transport-wide sequence numbers; GF(2^16) RS per frame with repair-on-demand (new parity instead of retransmission); VT LTR (and NVENC RFI) reference repair with a strict decoder state machine; an app-owned delay/train-based rate controller coupled per frame to the encoder, with quinn/noq as a follower; our own priority scheduler and pacer; mutual RPK TLS; per-path policy for relays.

---
