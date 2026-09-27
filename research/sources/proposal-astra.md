# Sunna: complete architecture proposal

Author: astra. Date: 2026-09-25. Status: proposed architecture and acceptance contract; not a claim that the proposed performance has been achieved.

## Evidence, scope, and reading conventions

This proposal uses the owner's [context](context.md), the complete [Astra dossier](dossier-astra.md) (abbreviated **A**), [Fable dossier](dossier-fable.md) (**F**), and [Claude dossier](dossier-claude.md) (**C**). References such as “A §1.3” identify dossier sections, not measurements performed for this proposal. I also read Sunna's [README](../../README.md), all eight research documents [00](../../research/00-overview.md), [01](../../research/01-parsec-internals.md), [02](../../research/02-competitive-landscape.md), [03](../../research/03-engineering-reference.md), [04](../../research/04-market-gaps.md), [05](../../research/05-trackpad-gestures.md), [06](../../research/06-architecture.md), [07](../../research/07-v1-plan.md), and all Rust source and Cargo manifests in `crates/` and `apps/`.

The source baseline is HEAD `01ac41c201c3daa8fcfd8d93e483468893c6bf01` **plus the working-tree changes present when inspected**. The working tree was already modified, including the untracked research/07 document. This proposal does not modify it. A commit-only description would miss important fixes already present.

**Decision** means the implementation should follow the stated design. **Target** means a release gate to test. **UNVERIFIED** means the evidence does not establish a capability, integration, or projected result. All performance targets, development estimates, and proposed wire formats below are design choices, not observed competitor or Sunna results. Source snapshots establish behavior of those snapshots; they do not establish every released binary's behavior.

The product is a native remote computer with two complementary graphics paths: deadline-driven video for movement and exact pixel refinement for stationary content. Native capture, native GPU presentation, reliable device identity, and a small, comprehensible session model matter more than adding every codec or inventing another NAT traversal stack.

The preferred `reflib` design reference service was attempted, but its tool call was blocked by the environment's approval policy. The UI flows below are original specifications, not claims to have inspected Parsec or other screens through another design library.

## 1. Product definition and measurable competitive targets

### 1.1 Who Sunna is for and what it promises

Sunna is for someone working on their own Mac or Linux workstation from another computer, and for someone playing on a Windows or Linux gaming machine remotely. The first customer is the owner using the M1 Air and M2 Air across two networks. Remote assistance, spectators, and co-play are extensions of the same authenticated session; enterprise fleet management and a cloud gaming marketplace are not prerequisites.

The promise is: **“Your computer, with sharp text, responsive controls, and one-click access after pairing.”** At normal viewing scale, desktop text must settle to the captured pixels rather than remain permanently blurred by 4:2:0. Motion must not wait behind clipboard data, old video, or a refinement backlog. A user must be able to distinguish a slow network, a permission problem, and a sleeping computer without interpreting a codec log.

“Beats all of them” is a matrix of supported uses, not a single unverifiable superlative. Sunna must beat the best tested competitor in its relevant workload and have no material regression in connection reliability or usability. An unavailable competitor platform is a capability advantage, not a latency win. A 30 fps high-quality stream cannot win a 60 fps gaming comparison by reporting only its decode latency. A local predicted pointer cannot count as a host response.

The evidence supports attacking specific weaknesses: permanent chroma loss in many hardware video paths; queue age and presentation delay; repair that depends too heavily on new keyframes; difficult display and permission setup; and unreliable traversal/fallback. It does **not** establish that one optimized encoder configuration will universally outrun Parsec or Apple's private pipeline. [A §§1.3–1.4, 2.2–2.4, 9–11; F §§1B, 4–5; C §§1, 5–6.]

### 1.2 Standard test profiles and absolute latency gates

Every result names hardware, OS build, physical panel refresh, output resolution, source workload, codec, bitrate including repair, and path. “Retina” means backing pixels, not Cocoa points. Default reference profiles:

| Profile | Source and display | Network shaping, both directions | Purpose |
|---|---|---|---|
| L60 | 1920×1080, 60 Hz, SDR; wired LAN | RTT ≤2 ms, ≥1 Gbit/s, no injected loss | Common interactive baseline |
| R60 | M1/M2 native backing resolution, 60 Hz, SDR | Same LAN; then the owner's real WAN separately | First product's sharp desktop |
| W60 | 1920×1080, 60 Hz, SDR | 30 ms RTT, 50/10 Mbit/s host/client uplinks, 0.1% independent loss, ±2 ms jitter | Ordinary controlled WAN |
| H60 | 1920×1080, 60 Hz, SDR | 60 ms RTT, 15/5 Mbit/s, 1% loss plus separate burst-loss runs | Degraded WAN; quality/recovery rather than LAN claims |
| G120 | 1920×1080, 120 Hz; later 2560×1440 | L60 and W60 networks, actual 120 Hz panels | Gaming release gate; requires additional hardware |
| T443 | W60 geometry over an explicitly TCP-only relay path | 30 ms minimum relay-path RTT; 0%, 0.1%, and 1% outer TCP loss | Compatibility mode; reported separately |

Latency below means **physical client input to visible host response on the client**, using the flash-response harness in §11. A paired local run measures the host application's own input-to-photon baseline. The overlay's capture-to-present estimate is a different metric. WAN RTT alone is not the full path budget; input and video serialization, application response, capture phase, decoding, and scanout remain.

| Target at matched fidelity and delivered unique-content rate | p50 | p99 | Additional gate |
|---|---:|---:|---|
| L60 flash-response input-to-photon | ≤35 ms | ≤55 ms | ≥59 unique frames/s during 60 fps animation; no sustained queue growth |
| R60 native backing-pixel desktop | ≤45 ms | ≤70 ms | No forced downscale to meet the target |
| W60 flash-response input-to-photon | ≤65 ms | ≤100 ms | 10-minute run, normal repair enabled |
| H60 flash-response input-to-photon | ≤105 ms | ≤180 ms | Recovery from an isolated damaged reference ≤2 RTT + 50 ms at p99 |
| G120 final gaming target, LAN | ≤22 ms | ≤40 ms | ≥118 unique frames/s on an unconstrained 120 fps source |
| G120 final gaming target, W60 | ≤55 ms | ≤85 ms | Same resolution, source and delivered fps as competitors |

These are **UNVERIFIED engineering targets**, deliberately separated from measurements. G120 does not apply to the owner's 60 Hz Air panels. A particular game can add much more local latency; publish its absolute distribution and remote overhead instead of claiming the flash-response numbers apply to all games. No p99 subtraction is described as a per-event overhead unless events are actually paired.

Per-stage diagnostic budgets for L60, excluding source application response and display scanout: capture delivery after the source surface changes ≤3 ms p50/8 ms p99; conversion+encode ≤4/8 ms at 1080p; application send-queue residence ≤2/5 ms; reassembly residence after the last necessary symbol ≤0.3/1 ms; decode ≤3/6 ms; decoded-frame residence before GPU submission ≤1/3 ms. These stage quantiles cannot simply be summed to obtain the end-to-end p99. Native-resolution and 120 Hz profiles have their own measured budgets.

### 1.3 Relative targets against each competitor

Test the fastest usable settings **and** documented defaults. Retain screenshots and configuration exports. Both programs get the same virtual/physical display geometry and equivalent bandwidth. A superiority claim requires a bootstrap confidence interval that supports it, not a favorable single run.

| Competitor | Required advantage | Guardrails |
|---|---|---|
| RustDesk | L60 and W60 input-to-photon p50 ≤80% and p99 ≤75% of measured RustDesk; substantially better unique-content fps in motion at the same wire rate | Equal or better first-session completion; settled desktop pixels exact; compare both its hardware and software quality modes |
| Apple Screen Sharing High Performance | R60 p50 at least 5 ms lower and p99 at least 15% lower; exact static text at ≤50% of Apple's measured wire rate on the desktop corpus | Compare HP with HP, Apple Silicon both ends; same virtual display mode, color, scaling and WAN/Tailscale route; if this is not achieved, say so |
| Moonlight + Sunshine | L60/G120 p50 at least 2 ms lower and p99 at least 10% lower; lower freeze time under H60 at equal total bandwidth | No lowering fps, resolution, chroma fidelity, or quality to manufacture a latency win; also test best applicable Apollo configuration |
| Wolf / Moonshine | Match or improve latency on the Linux hardware they support, plus easier existing-desktop use | Their owned-compositor sessions are a separate test from existing-console capture; no blanket cross-platform claim |
| Parsec | L60 and W60 p50 at least 2 ms lower and p99 at least 15% lower; quality non-inferior at fixed wire rate; better stationary text at desktop bitrates | Match paid 4:4:4 capability when relevant and disclose edition; equal or better connection completion and first-session usability |

For a competitor already near a physical floor, these relative margins may be unattainable. The release may still be useful, but “faster than X” remains gated. Historical vendor figures and the owner's subjective Apple experience are reasons to investigate, not baseline values. [A §9; F §§4–5; C §§5.1–5.4.]

Quality protocol: at 5, 10, 20, and 40 Mbit/s **total host wire rate**, measure synchronized source/display crops, text edge error, OCR character error on known text, color error, and motion quality. Target ≥10% lower median perceptual error than the best video competitor on the desktop corpus at 10 Mbit/s, and non-inferiority within 2 VMAF points on the natural-motion corpus, accompanied by frame-aligned visual inspection. VMAF alone is not a text metric. After refinement, eligible SDR tiles must have zero byte differences from the negotiated canonical captured raster before client display color conversion. Motion never gets called lossless.

Time-to-sharp target: at 10 Mbit/s, after content stops changing, text-priority tiles settle within 250 ms p50/600 ms p99; an entire test screen settles within 1 second **when its pending compressed refinement is ≤512 KiB and ≥80% of the idle link is available**. A 32 MiB incompressible image cannot arrive losslessly in one second on this link; show actual remaining bytes and preserve the video preview.

### 1.4 Connection and usability gates

| Metric | Public native release target |
|---|---|
| Reachable, awake, permission-ready paired devices successfully connect | ≥99.5% across the declared NAT/firewall matrix; no case excluded merely because it needs a relay |
| Direct path established where test topology permits it | ≥98%; relay fallback is still a successful session |
| LAN paired reconnect, click to first **presented** frame | p50 ≤350 ms, p99 ≤1 s |
| W60 paired connect, direct route not cached | p50 ≤1 s, p99 ≤3 s |
| T443 paired connect | p50 ≤1.5 s, p99 ≤5 s; explicit degraded transport indicator |
| Handoff Wi-Fi→mobile or direct→relay | input released immediately on loss of lease; first fresh frame p50 ≤1 s, p99 ≤3 s when a replacement path exists |
| Initial two-Mac setup after apps are installed | Median ≤3 minutes and ≥90% unassisted completion in a 10+ person study; count OS actions honestly |
| Routine access after pairing | One device selection; no IP addresses, ports, codec selection, VPN setup, or repeated host approval for an explicitly authorized unattended peer |
| First-session task count | Mac host 5 setup tasks + 1 first-peer approval; client 2 tasks: 8 product tasks plus visible OS actions; Windows/Linux counts in §9 |

Connection success excludes powered-off machines and deliberate host denial, but reports those outcomes separately, along with relay unavailability, captive portals, permissions, authentication failures, and crashes. The denominator and confidence intervals are published. “99.5%” is a fleet/test target; ten successful connections cannot establish it. Fable's competitor step counts are useful hypotheses, not a controlled usability experiment. [F §§5.1–5.8.]

## 2. System overview, ownership, and bounded queues

### 2.1 Processes per machine

An ordinary installed desktop has a signed `Sunna` application, a per-user `sunna-agent`, and, while a viewer is open, a `sunna-viewer` process. Hosting at login is opt-in. Unattended operation adds a minimal privileged `sunna-supervisor` where necessary. The network broker runs as an unprivileged `sunna-broker`; in attended-only mode it can be embedded in the user agent to avoid a redundant service. The code boundary is the same in both deployments.

| Process | Authority and contents | Explicit boundary |
|---|---|---|
| Sunna UI | Tauri shell, device list, consent, diagnostics, settings | No decoded video through JavaScript, no access to arbitrary broker filesystem commands |
| Broker | Device transport identity, peer authentication, rendezvous, QUIC, ACL checks, session routing, audit writer | Cannot capture/inject by itself; no root; no arbitrary launch commands |
| User agent | Capture, display management, encode, audio capture, clipboard mediation, user-session input injection | Opens only after broker sends an authenticated local capability for a granted peer/session |
| Viewer | QUIC endpoint when independent, reassembly, decode, audio output, native window/input and GPU presentation | Per-session media isolation; malformed decoder input does not live in the root supervisor |
| Supervisor, optional | Install/start/stop approved signed components; bind agents to OS sessions; narrow device-handle requests | No internet listener, no codec, no clipboard, no remote shell, no invitation secret storage |
| Hosted infrastructure | Rendezvous/presence and relay service, optional account sync | Carries ciphertext; cannot issue device grants or replace pinned identities |

The broker and agent communicate over a local authenticated channel: Unix-domain socket plus peer credentials and code-signing checks on macOS where available; Unix socket with UID/seat checks on Linux; named pipe with explicit SID ACL and impersonation checks on Windows. Each message includes local session ID, granted capability ID, and generation. Never trust a PID alone. A hostile same-user process may already have major access to that user's desktop; the design still prevents accidental cross-user routing and privileged-helper escalation.

Separate processes cost IPC and complexity. GPU surfaces stay in the capture/encode process and decoder/render process. Encoded bytes cross broker IPC as bounded shared-memory slabs plus descriptors when profiling justifies it; initially bounded local sockets are acceptable. A network broker never receives uncompressed 4K frames. The optional privileged layer exists for lifecycle and OS-specific access, not to make a root network server convenient. This narrows C §3.4's suggested launch arrangement.

### 2.2 Data and control flow

```text
HOST USER SESSION                                      CLIENT
OS compositor/display                                  native input callbacks
  │ retained surface + source timestamp                     │ journal + motion state
  ▼                                                         ▼
capture mailbox[1] → GPU color/damage → hardware encoder    input scheduler
                        │                    │               │
                        └→ tile jobs         ▼               │
                            [32]       encoded access unit    │
                              │         + dependencies       │
                              ▼                │             │
                         refinement            ▼             │
                         streams       FEC / deadline pacer  │
                              └─────────────┬───┘             │
                                            ⇄ E2E QUIC ⇄─────┘
audio capture → Opus ────────────────────────┤
cursor shapes/state ────────────────────────┤
                                            └→ reorder/FEC → decoder → GPU surface
                                                                   │        │
                                                      refined tiles┘        ▼
                                                                    latest-present[1]
                                                                          │
                                                        cursor + color → native swapchain

UI ⇄ session actor ⇄ peer authorization ⇄ capability negotiation
                       │                         │
                local agent grants        display/codec epochs
                       │                         │
                  input injection         recovery / feedback
```

There are three clocks, all monotonic: host source time, client local time, and each audio device's sample clock. A session negotiates clock mapping with uncertainty; it never overwrites a capture timestamp with retransmission or recapture time. Frame identity distinguishes capture sample, encoded access unit, decoder sequence, and presented sample. Reusing a static surface is allowed but is labeled a repeat, not a newly captured frame.

### 2.3 Crate and module layout

Keep the present workspace recognizable while splitting policy from platform mechanisms:

```text
crates/
  core/          ids, monotonic time, geometry, color, capabilities, bounded ownership
  proto/         versioned wire schemas, validators, fuzz targets; no OS bindings
  session/       authorization/negotiation/display/lease/reconnect state machines
  identity/      keystore abstraction, pairing, peer grants, revocation, audit signing
  link/          iroh adapter, discovery, path events, relay configuration
  transport/     deadline scheduler, Noq/Quinn integration, feedback, congestion policy
  media/         packetization, FEC, NACK/RTX, frame dependency and recovery ledger
  capture/       generic retained-surface API + synthetic test source
  codec/         async encode/decode contracts, capability probes, bitstream inspectors
  quality/       damage generations, tile cache/refinement, mode and bandwidth policy
  input/         HID/semantic event model, leases, state reconciliation
  audio/         sample clocks, Opus, jitter buffer, resampler, device routing
  clipboard/     offers, limits, OS adapters, transfer cancellation
  platform-mac/  SCK, VT, CoreAudio, CGEvent, Metal, virtual-display shim
  platform-linux/portals, PipeWire, EIS, DRM/VAAPI/NVENC, Vulkan, optional helpers
  platform-win/  DDA/WGC, D3D11, NVENC/QSV/AMF, WASAPI, SendInput, IddCx adapter
  host/          host actor composition; no wire parsing in capture callbacks
  client/        client actor composition; no network awaiting in render callbacks
  metrics/       stage tracing, histograms, local overlay, redacted diagnostics
  testkit/       simulated clock/link/codec/display, replay fixtures, harness protocol
apps/
  sunna/         Tauri chrome + native application lifecycle
  sunna-viewer/  native video window and session frontend
  sunnad/        broker / user-agent modes with explicit launch authority
  sunna-supervisor/  optional narrow privileged helper
  sunna-cli/     diagnostics, headless test client, administrative local commands
services/
  rendezvous/    signed presence and invitation rendezvous, no media keys
  udp-relay/     optional low-latency forwarding extension
  deployment/    self-host bundles, certificates and quotas
```

Do not immediately create twenty empty crates. Extract a boundary when it gains an independent test surface or platform owner; `media`, `session`, and `identity` come first. `platform-*` exposes typed handles, not a universal `Vec<u8>` framebuffer. `CapturedSurface`, `EncoderInput`, and `DecodedSurface` carry dimensions, color, crop, ownership, completion fences, and timestamps. `Encoder::submit` returns an admission result; output arrives asynchronously with an explicit reference/recovery classification. Trait contracts prohibit hidden unbounded queues.

### 2.4 Threading and scheduling

One 2-worker Tokio runtime per broker/viewer handles network and lightweight control actors. This is a starting configuration, not two cores dedicated at all times. Capture callbacks only retain a surface and replace a mailbox entry. A serial capture/conversion executor owns platform context; a serial encoder submission executor owns each codec session; vendor completion callbacks publish output without blocking. Decoder work has its own serial executor and completion queue. Rendering stays on the platform-required main/render thread. Audio device callbacks use preallocated rings and do not allocate, lock a contended mutex, or await. Input injection has a serial high-priority user-session executor so ordering and release semantics are explicit.

Tile compression uses at most two low-priority workers on an Air, with a 20% aggregate CPU target while interactive video runs. FEC and packetization run in bounded work batches; packets return to the reactor between batches. Disk, clipboard conversion, logging, and app launch never execute on the network or audio callbacks. Request platform QoS appropriate for interactive/audio work; do not give every thread realtime priority or busy-spin an idle desktop.

No `VTCompressionSessionCompleteFrames` on each frame, no `waitUntilCompleted` in the ordinary Metal presentation loop, and no synchronous decoder inside the network receive select. Surface lifetimes end only after the owning GPU/codec completion fence. These directly address the current code and the queue/presentation evidence. [Sunna codec and client sources; A §§2.2–2.3; C §6.1.]

### 2.5 Queue inventory: count, byte, and age limits

All limits are defaults per session unless stated. Every queue exports occupancy, oldest age, dropped/replaced count, and policy reason. **A byte bound is not a latency bound.** Limits below include application-owned queues; driver/OS buffers that cannot be precisely bounded are measured and called out.

| Queue / owner | Bound | Full/expired behavior |
|---|---|---|
| SCK capture pool / OS | Request `queueDepth=3`; retained app references ≤2 outside conversion/encode | Release callback surfaces promptly; OS buffering measured, never claim zero internal buffering |
| Capture mailbox / agent | 1 latest surface per display; source age ≤1 frame interval before admission | Replace unencoded old surface, preserving damage generations |
| GPU conversion / agent | 1 submitted job plus 1 reusable output slot per active encoding rendition | Skip capture admission if both unavailable; never wait in callback |
| Encoder / codec adapter | 1 submitted frame initially; at most 2 only if measured to improve throughput without age regression | Skip before encode; watchdog at 100 ms, rebuild after 500 ms stalled completion |
| Encoded admission / media actor | 1 access unit under packetization plus 1 pending; aggregate ≤8 MiB | Drop only with reference-repair transition; never silently evict a reference frame |
| Ready video packets / scheduler | `max(2×MTU, R_wire×2 ms/8)`; ≤64 KiB; oldest ≤5 ms LAN/10 ms WAN | Keep remaining symbols attached to their access-unit deadline; abort expired unit, trigger repair if needed |
| Transport DATAGRAM staging / Noq | At most 2 application datagrams ahead of send permission; target no residence >2 ms | Application admits before enqueue; expiry notification must identify discarded frame |
| OS UDP send buffer | Requested 64 KiB, actual recorded | Backpressure; no assumption that kernel accepted equals on-wire; reduce batches on delayed sends |
| RTX cache / media actor | 16 MiB or 250 ms, whichever first | Evict expired whole groups; missing cache falls back to reference repair |
| Receive packet bookkeeping | 4,096 packet sequence entries; reorder window 256 | Older duplicates ignored; discontinuity starts fresh baseline, no giant allocation |
| Reassembly / client | 4 active frames, ≤16 MiB total; ≤8 MiB/object, ≤4,096 data symbols and ≤6,144 total symbols/object | Expire by display/recovery deadline; reject invalid geometry before allocation |
| Decoder admission | 2 complete units plus one executing; ordinary display deadlines still apply | Decode references needed for a newer frame; skip safe disposable frames; otherwise enter repair |
| Decoded presentation mailbox | 1 newest eligible surface; at most 3 GPU-owned surfaces including current drawables | Replace pending presentation, never discard decoder reference state |
| Swapchain / OS | macOS 3 drawables initially; Windows/Linux 2–3 by verified backend | Submit at most one frame for the next presentation opportunity; measure compositor queue separately |
| Reliable session/control | 128 messages, 256 KiB queued; individual payload ≤64 KiB | Apply backpressure; disconnect a peer flooding control; critical termination has reserved slot |
| Input transition journal | 128 events / 32 KiB; ≤250 ms unacknowledged age | Stop accepting remote control and release state if reconciliation cannot catch up |
| Motion / scroll accumulator | One cumulative state per device; emit ≤1,000 packets/s | Coalesce motion without losing accumulated relative delta; never erase a key/button transition |
| Host input injection | 64 transitions plus latest motion; ≤8 ms target age | Abort lease and release keys on overflow; do not execute stale typing seconds later |
| Audio capture/output rings | 20 ms each in app; callback quantum measured | Drop oldest whole capture frame / conceal output gap; report underrun |
| Audio jitter | 5–10 ms LAN, 10–25 ms WAN adaptive, hard 40 ms | Late frames discarded; no seconds-long recovery backlog |
| Cursor state/shape | 1 latest state; 64 shapes, 4 MiB cache; shape ≤256 KiB | Unknown shape requests bounded resend; fallback embedded cursor |
| Tile work / quality actor | 32 jobs, ≤8 MiB uncompressed pending per stream | Coalesce by tile ID+generation; cancel obsolete jobs |
| Refinement transfer | 4 independent streams, ≤256 KiB granted in flight total; one tile record ≤256 KiB | Reset obsolete streams; stop write grants while interactive backlog exists |
| Clipboard transfer | One offer and one active payload/direction; metadata 64 KiB, text 1 MiB, image 16 MiB | Larger content becomes explicitly approved file transfer; cancel on new offer |
| Bulk transfer | 2 streams/session, 64 KiB read/write chunks, 256 KiB granted ahead total | Backpressure to disk; cancel/retry by chunk; never read entire file into memory |
| Broker↔agent IPC | 64 descriptors, 8 MiB media slab budget plus reserved 64 KiB control | Stop capture admission; do not let a crashed broker retain all surfaces |
| Metrics / audit | Metrics ring 8,192 events; audit write queue 256 entries | Drop detailed metrics with counter; security-critical grant fails closed if durable audit cannot be committed |

The reassembly symbol ceiling makes the practical maximum object size about 4 MiB at 1,024-byte symbols, despite the separate 8 MiB absolute byte validation limit. Negotiation advertises the smaller effective limit, and an oversized access unit is re-encoded or causes an explicit configuration downgrade. The two limits protect different parser paths; neither is permission to emit an unbounded IDR.

The capture-age admission limit applies to fresh motion samples. A known unchanged surface may explicitly prime/recover a static scene with the `repeated-source` flag and its original timestamp; it is not rejected merely for being old, and it does not count as newly captured content. Its validity still depends on the capture source reporting no intervening change or a fresh validated snapshot.

Aggregate budgets matter too: default one host session with up to four viewers; ≤512 MiB application CPU media memory and ≤768 MiB GPU allocations for two 4K surfaces/renditions on desktop, with admission denial rather than process OOM. Those are **UNVERIFIED budget targets**, not a guarantee that opaque hardware allocations fit. Mobile starts at one 1440p stream and ≤256 MiB total media budget. The overlay reports actual process and GPU allocations where available. Multi-viewer limits reduce dynamically when encoder session limits or memory cannot satisfy them.

### 2.6 Control state machines

```text
Device:  Offline → Discovering → Authenticating → Authorized → Ready
                                      └→ Denied / KeyChanged / PermissionMissing

Session: Requested → ConsentPending → Negotiating → PreparingDisplay
        → PreparingCodec → Priming → Streaming ↔ Reconfiguring
                                      │             │
                                      └→ Suspended / Reconnecting
                                                    │
                                              Closing → Closed

Video:   AwaitConfig → AwaitRecoveryPoint → Healthy
                                           │ loss of needed reference
                                           ▼
                                      RepairPending
                                  ↙ valid repair    ↘ timeout/no capability
                              Healthy             AwaitRecoveryPoint
```

Every transition has an owner, deadline, and idempotent request ID. `Negotiating` cannot start capture before authorization. Display and codec reconfiguration create new epochs and retain the old presentation until the first valid new frame. `Closing` releases input before tearing down sockets and restores only display changes that this session still owns. A crash journal lets the next agent reconcile changes without blindly restoring over a user's newer display settings.

## 3. Host engines: macOS, Linux, then Windows

### 3.1 Common host contract and codec selection

A host reports **tested combinations**, not a codec-name checkbox: codec/profile, chroma, bit depth, maximum tested geometry/fps, hardware/software status, input surface formats, HDR metadata support, temporal-discard support, repair support, and encoder concurrency. Cache results by GPU identifier, OS build, driver version, application version, and test configuration; invalidate on changes. Probe with synthetic text, moving detail, and a deliberately lost reference. Parse the resulting bitstream and inspect decoder output. “The property setter succeeded” is necessary but not sufficient.

Default SDR negotiation is H.264 High or Constrained High, 8-bit 4:2:0, no B frames, limited-range BT.709 video plus full-resolution SDR refinement. HEVC Main becomes preferred on a tested Apple-to-Apple pair when it improves rate-distortion without exceeding the H.264 latency budget. HEVC Main10 is the initial HDR codec. AV1 is preferred only on hardware with tested low-delay encoding **and** decoding; the M1/M2 deployment cannot depend on AV1 hardware encoding. H.264 remains the baseline interchange codec; do not silently select a software AV1 decoder on an Air for a high-fps session. [A §§1.3, 2.2; C §§3.2, 4.5.]

Start desktop at 60 fps maximum with damage-driven idle behavior; use 30 fps only when explicitly negotiated or resource/network policy requires it. Start 1080p SDR at 12 Mbit/s video budget on a known roomy LAN, 6 Mbit/s after a fresh unknown-WAN connection, and native Air resolution at 20/10 Mbit/s respectively **subject to transport admission**. These are initial rate requests, not permission to exceed the congestion window. Suggested user caps: 1080p60 20 Mbit/s, 1440p60 35 Mbit/s, 4K60 60 Mbit/s; unconstrained LAN may raise them. Encoder rate excludes estimated audio, repair, headers, and refinement allocation. The target setting changes at most every 50 ms; large reductions can skip new submissions immediately.

All adapters use infinite/very long GOP with recovery on demand after startup, not a periodic multi-megabyte IDR every second. No B frames, no future-frame lookahead, no artificial filler data. An optional two-second recovery point is a diagnostic/compatibility setting, not the default. Required options fail the combination probe; optional options downgrade visibly in diagnostics. Every stream stores the actual accepted settings with its benchmark results.

### 3.2 macOS capture and virtual displays

**Decision: minimum macOS 14.4 for the first supported host, Apple Silicon first.** Older/Intel support is a later capability tier. Use ScreenCaptureKit (`SCShareableContent`, `SCContentFilter`, `SCStream`, `SCStreamConfiguration`), not the current CGDisplayStream. The latter is deprecated/obsolete in newer SDKs and has reported failures capturing virtual displays; SCK also gives the route to supported pre-login capture on 14.4+. [C §§3.1, 3.4–3.5.]

Initial desktop configuration:

```text
width,height               = exact negotiated backing-pixel geometry
minimumFrameInterval       = CMTime(1, negotiated_fps), with rational-rate handling
pixelFormat                = kCVPixelFormatType_32BGRA
queueDepth                 = 3
showsCursor                = false only when separate-cursor capture is operational
scalesToFit                = false
capturesAudio              = true only after session grants and audio opt-in
sampleRate/channelCount    = 48000 / 2
excludesCurrentProcessAudio= true
```

Accept only valid complete screen samples. Preserve `SCStreamFrameInfo` display time, content rect, scale, screen rect, and dirty rects. Convert the Mach timestamp to the process monotonic domain with overflow-safe timebase arithmetic. Idle/status callbacks do not increment unique-content fps. Dirty rectangles are hints for work reduction; the refinement correctness path also detects actual tile content changes on GPU. Reconfiguration is a transaction: prepare new encoder and color descriptors, update SCK, wait for an actual sample with the expected dimensions, then publish the new display/configuration epoch. A successful `updateConfiguration` callback alone does not prove frame geometry has changed.

Retain `CVPixelBuffer`/IOSurface through GPU conversion and encode completion. Desktop mode captures BGRA so lossless tiles retain the full captured RGB information. A Metal pass computes tile hashes/damage and produces NV12/P010 where needed. If VT accepts the input surface directly with correct conversion and timing, use it; measure whether a separate GPU conversion is better. Game mode may request native YUV capture to avoid conversion only when tiles are disabled and the resulting color path is verified. “Zero CPU frame copies” is the goal; GPU conversion and memory bandwidth still exist.

Virtual displays use a small, runtime-checked Objective-C module around `CGVirtualDisplay` and related private classes. This is a conscious distribution/maintenance risk, not a public-API guarantee. Derive a stable virtual display serial from a locally stored random display identity associated with the paired device, not its MAC address. Keep the display object alive in the user agent for the whole display lease. Default geometry follows the viewer's **backing pixels and intended logical scale**: e.g. 1512×982 points at 2× becomes 3024×1964 pixels. Advertise a safe mode list, 60 Hz first, 120 Hz only after probe. Round codec padding separately from logical display dimensions; crop padded pixels on the client.

Existing physical display is always available as a fallback. “Private display” is not advertised as a security curtain unless physical panels are demonstrably blanked on that OS and state. A virtual display alone does not isolate applications from the local user. Display creation, arrangement, main-display changes, and restoration have an ownership journal; retain a disconnected virtual display for 30 seconds during reconnect, then restore. Never detach somebody else's virtual display or revert user changes made during the session. [C §3.5; A §3.2.]

### 3.3 macOS VideoToolbox settings and recovery

Use `VTCompressionSessionCreate` with an encoder specification requiring hardware acceleration and requesting low-latency rate control. Call `VTSessionCopySupportedPropertyDictionary` / supported-encoder property queries where available; set and check each property, then verify the emitted samples. The public low-latency announcement specifically describes H.264; HEVC availability is a runtime capability, **UNVERIFIED across the proposed OS matrix**. Do not assume Apple Screen Sharing's observed HEVC 4:4:4 stream means Sunna can request the same implementation. [C §3.2; Apple [low-latency VideoToolbox session](https://developer.apple.com/videos/play/wwdc2021/10158/).]

| Setting | H.264 SDR desktop | HEVC SDR / HDR | Failure policy |
|---|---|---|---|
| `RequireHardwareAcceleratedVideoEncoder` | true | true | Exclude from normal interactive tier if no hardware session |
| `EnableLowLatencyRateControl` encoder specification | true | Try true on arm64 | H.264 fallback session benchmarked separately; never silently label it low-latency |
| `ProfileLevel` | Constrained High AutoLevel if accepted; otherwise High AutoLevel | Main AutoLevel / Main10 AutoLevel | Parse SPS/VPS; reject unexpected profile/chroma/depth |
| `RealTime` | true | true | Required |
| `AllowFrameReordering` | false | false | Required; reject reordered output |
| `ExpectedFrameRate` | Rational negotiated rate represented by supported property type | Same | Rate changes coordinated with capture/admission |
| `AverageBitRate` | Controller's video payload rate | Same | Required in low-delay session |
| `MaxKeyFrameInterval` | Largest supported long interval; no periodic request in Sunna | Same | Verify no unexpected frequent IDRs |
| `MaxFrameDelayCount` | Request 0; accept 1 only with measured bounded behavior | Same | Reject >1 for low-delay tier |
| `PrioritizeEncodingSpeedOverQuality` | true initially | true initially | Optional; benchmark quality tradeoff |
| `EnableLTR` | true if supported and repair probe succeeds | Probe, not assumed | Otherwise explicit IDR repair |
| `MaxAllowedFrameQP` | 34 desktop; 40 game, when supported | Initial 34 SDR / 38 HDR/game, when supported | A quality-floor policy; numeric QP is not comparable across codecs |
| `MinAllowedFrameQP` | Unset initially | Unset initially | Do not constrain clean/static quality without evidence |
| `DataRateLimits` | Probe `[1.5×R/8, 1.0 seconds]` as coarse burst bound | Same if accepted | Do not claim this is a one-frame VBV; application deadline/admission still enforces queue age |
| `ReferenceBufferCount` | Leave encoder default | Leave default initially; experiment separately | Specifically do not force H.264 to 1 reference |
| Color properties | BT.709 primaries/transfer/matrix, limited-range NV12 path; explicit source ICC elsewhere | Main10 HDR carries BT.2020/PQ or negotiated HLG | Reject contradictory bitstream/pixel-buffer metadata |

The H.264 reference-count exception is grounded in Sunshine's regression: forcing a one-reference configuration could produce every-frame IDRs and much higher bitrate. Apollo's older setting is not automatically preferable. Likewise, C §3.2's proposed one-frame `DataRateLimits` window is not a guaranteed hardware VBV. The proposal uses a supported coarse limit plus application pacing and tests smaller windows experimentally. [A §1.3; C §3.2.]

Encode asynchronously. Set `sourceFrameRefCon` to an owned metadata record; the callback resolves encoded bytes, sample attachments, source ID, and timestamps. Handle multiple NAL units per sample and arbitrary supported NAL length-prefix widths. Do not call `CompleteFrames` in the hot path; reserve drain for orderly shutdown/reconfiguration. An encoder-dropped input produces no wire frame ID and does not itself create a decoder reference gap.

LTR policy: retain the encoder's opaque acknowledgement token against the encoded frame ID; send it in bounded metadata. A client acknowledges it only after complete authenticated reassembly and successful decode. On the next encode, return acknowledged tokens with `AcknowledgedLTRTokens`. After a missing needed reference, request `ForceLTRRefresh`; if there is no usable acknowledged reference or the probe finds inconsistent behavior, force an IDR. These API concepts are documented by Apple; exact attachment/property availability is runtime-checked. Do not synthesize your own VT reference identifiers. Optional temporal scalability uses `BaseLayerFrameRateFraction=0.5` and `BaseLayerBitRateFraction=0.7`; only frames explicitly marked not depended on by others are eligible for reference-safe discard. Keep it off until its quality and repair behavior beat ordinary single-layer encoding on the Macs. [C §3.2; A §10.]

### 3.4 macOS audio, input, cursor, and clipboard

First ship SCK audio at 48 kHz stereo to minimize extra onboarding. Encode 5 ms Opus frames, `OPUS_APPLICATION_RESTRICTED_LOWDELAY`, 160 kbit/s constrained VBR for stereo, with a 192 kbit/s high-quality option. Query actual codec lookahead. Audio capture timestamps and sample counts survive resampling. Core Audio process taps become an optional backend for host-speaker muting and process selection: `CATapDescription`, `AudioHardwareCreateProcessTap`, private aggregate device, explicit audio capture usage description. A quiet tap is not proof of denied permission; offer a user-triggered test tone and explain uncertainty. Do not install BlackHole by default. [A §1.6; C §§3.3, 3.7.]

Opus in-band FEC is not assumed here: the codec's documented FEC applies to the LPC layer, whereas restricted-low-delay uses CELT. Recover low-delay audio with PLC and optional previous-packet redundancy under the policy in §5, or deliberately negotiate a different Opus mode. This corrects any “just enable Opus FEC” reading of the earlier research. [Official [Opus encoder controls](https://opus-codec.org/docs/opus_api-1.5/group__opus__encoderctls.html).]

Input is injected from the active GUI agent using a private `CGEventSource`, explicit per-peer modifier state, and tested event-tap placement. Map USB HID keyboard usages to host virtual keycodes, separate physical keys from committed Unicode text, and set modifier flags on every relevant key event. Mouse positions use the negotiated display transform, including negative global coordinates and rotation. Relative pointer mode uses deltas and pointer locking semantics, not an absolute-position emulation when a game requests relative input. Set click-state for double/triple clicks and track every mouse button separately.

Use `CGEventCreateScrollWheelEvent2` with fixed arguments, pixel or line units as negotiated, continuous flags, scroll phase and momentum phase. Do not multiply by both client and host acceleration curves. Carry pinch/rotate/swipe as semantic gestures with start/update/end/cancel; macOS has no supported general-purpose injection API for arbitrary physical multitouch. Default to explicitly supported shortcuts/scroll transforms or show “gesture unavailable.” Private gesture injection is an experimental capability, never an invisible promise of full trackpad emulation. Local-user co-control requires explicitly tested suppression settings; lease revocation releases only Sunna-held keys/buttons. [C §3.6; Sunna research/05.]

Separate cursor mode sends shape, hotspot, visibility, host position, and host-acknowledged input sequence. A client may display its local position immediately, but this is labeled cursor prediction in measurements. macOS cursor-shape detection currently needs private APIs in known implementations; isolate the seed/current-system-cursor path, cap shapes to 256×256 BGRA, test animated cursors, and fall back to SCK's embedded cursor if unsupported. Avoid rendering both. In relative game mode, hide the local cursor when the host cursor is hidden. [C §3.6; F §1B.]

Clipboard is an offer/request protocol, not perpetual eager copying. Poll pasteboard change count at 4 Hz while clipboard permission is granted and the session is foreground; publish MIME/type availability and a generation, read content only on paste or explicit copy transfer. Exclude concealed/transient/password-manager types. UTF-8 plain text first; HTML and PNG later with size limits and sanitization. Echo suppression uses origin peer+offer ID, not text hashing alone. New offers cancel stale fetches. Respect current pasteboard privacy APIs and user denial; exact future enforcement is **UNVERIFIED**. [C §3.7; F §1B.6.]

### 3.5 macOS services, permission onboarding, lock and sleep

The signed app/agent has stable bundle and Team identifiers. Onboarding asks for Screen Recording when hosting is enabled; posting events is requested only for control. Use `CGPreflightScreenCaptureAccess` and `CGPreflightPostEventAccess`, recheck on foreground and failed operation, and offer a one-action agent restart if the system requires it. The host does not request Input Monitoring merely to post input. The client requests extra shortcut-capture authority only if the user enables “Send system shortcuts.” [C §3.3.]

Use `SMAppService` for supported login/helper registration paths, with a narrow installer for any launchd configuration that the desired LoginWindow arrangement actually requires. **UNVERIFIED:** final packaging and service/TCC attribution of the pre-login configuration must be validated with signed builds; a development app launched from Terminal is not the test. The user-session agent runs in Aqua. A separately constrained LoginWindow agent may capture/inject the login screen on supported 14.4+ systems; it has no general remote shell or keychain-unlock function. The supervisor identifies the active console session and changes the broker's local route. The media session re-primes after login or fast-user switching without granting the next user the previous user's clipboard or app-launch authority. [C §3.4.]

| Host state | Required behavior |
|---|---|
| Unlocked, awake | Normal authorized capture/control |
| Screen locked | Freeze/replace last desktop image immediately; show lock state; only peers with `login_control` may enter the lock/login surface |
| User logs out | Destroy user clipboard/audio grants, clear decoded desktop tiles, switch to permitted LoginWindow path if available |
| Fast-user switch | Reauthorize against the destination OS user; no automatic cross-user authority |
| Permission revoked | Stop affected subsystem, release input, send specific diagnostic; view-only may continue if only control was revoked |
| Display asleep | On an authorized connect, request user activity/display wake; wait ≤5 s for a real complete sample, then show a sleep/permission diagnostic |
| Idle sleep during active session | Hold scoped power assertions for active use; release on close; show battery effect |
| “Always available” enabled | Explicit optional idle-system-sleep prevention policy, preferably on AC; do not silently keep every installed Mac awake |
| Lid closed | Report availability conservatively; clamshell behavior with a virtual display is **UNVERIFIED** on the two Airs |
| FileVault pre-boot | Unsupported: the OS and Sunna are not running in the normal user environment; do not promise a software KVM |

Apply for the persistent-content-capture entitlement, but do not make unattended reliability depend on an entitlement that has not been granted. OS consent may recur or require local action. Track supported OS versions with real long-idle/reboot tests; do not promise “always available” merely because a launch daemon restarts. The product can ship attended access before this gate, while honestly limiting unattended claims. [C §§3.3–3.4.]

### 3.6 Linux: existing desktop first, owned compositor later

**Decision: use the public desktop portals, PipeWire and libei/EIS for the first Wayland host.** Begin with supported GNOME and KDE versions, selected by actual portal capabilities. Do not make private Mutter interfaces the general Linux API. Those interfaces explain gnome-remote-desktop's excellent integration but are not a stable permission bypass for an unrelated application. [C §§2.1–2.3, 4.1–4.3.]

The portal transaction is `RemoteDesktop.CreateSession → SelectDevices → ScreenCast.SelectSources → Start`, with clipboard negotiation where the implemented portal supports it, then obtain the PipeWire remote and EIS connection. The exact ordering/options follow the selected portal interface version; generate bindings from versioned XML, and integration-test real backends. Request pointer/keyboard and one selected monitor, with persistent access only when the user chooses it. Store restore tokens in the user keystore and atomically replace them when the portal rotates them. Do not reuse a token that the backend treats as single-use. “Remember this device” is separate from “the compositor remembers this capture grant.” [C §§4.1–4.2, 4.6.]

PipeWire requests DMA-BUF first with explicit format/modifier negotiation, then shared-memory fallback. Import into the same GPU as the encoder when possible. Synchronize DMA-BUF access using the producer/consumer fence contract available on that compositor and driver; never assume an fd implies rendering is complete. A modifier incompatibility falls back to a GPU copy/linear intermediate before CPU readback. The fallback is reported with estimated copy cost. Configure a small buffer pool (request three; record actual negotiation), return buffers promptly, retain only the current admitted surface. Read damage, crop, timestamp, and cursor metadata. The owner's Linux VM validates portal/state/protocol behavior and software paths, **not** GPU zero-copy, HDR, or hardware latency. [C §§2.2, 4.1; A §§1.2, 4.1, 5.1.]

EIS is the preferred injection interface. It carries the compositor-approved devices and coordinate regions, so it also constrains what Sunna may inject. When unavailable, offer view-only with a clear reason. X11 compatibility uses XDamage/XShm or suitable GPU capture plus XTest for input, in an explicitly less isolated legacy tier. A wlroots compositor can get an adapter for its supported capture protocols, but is not marketed as supported because “Wayland” appears in its name.

For advanced existing-console access without portal support, an optional narrow helper can open `/dev/uinput` and selected DRM resources. It checks caller UID, active seat, session authorization, device allowlist and peer credentials; passes only necessary fds via `SCM_RIGHTS`; drops capabilities and applies syscall restrictions. Do **not** put `CAP_SYS_ADMIN` on the full networked Sunna binary. KMS capture requires an active usable output; it does not create a headless desktop. It can expose more than an ordinary application capture grant and therefore requires explicit local installation/authorization. Whether it can capture a particular greeter, protected surface, or NVIDIA setup remains a tested capability, not a universal fallback. [C §4.1; F §2.3.]

Virtual display policy:

1. Existing desktop: use a compositor-supported virtual monitor API only through a small, versioned adapter after testing permission and lifecycle behavior. GNOME and KDE adapters may differ; their private APIs are opt-in compatibility code.
2. Truly headless isolated session: later provide `sunna-session`, an owned Smithay compositor with a virtual output, libinput-compatible devices, PipeWire audio, and application launching. This is a new desktop/session, not the user's existing console.
3. Unsupported compositor: stream a selected physical output and explain the limitation. Do not install an invisible nested compositor and pretend the existing desktop moved into it.

Wolf and Moonshine demonstrate the controlled-compositor approach and its GPU path, but they do not eliminate desktop environment, session isolation, audio, or display-manager engineering. The first Linux release should work inside the user's real desktop before Sunna builds another desktop host. [A §§4–5; C §§4.3, 4.6.]

System audio uses a PipeWire monitor/capture node with negotiated consent and the common Opus pipeline. Route host playback muting only when the session owns that route; remember and restore the prior volume/mute state without overwriting later user changes. Input via EIS preserves keymaps, scroll axes/units, touch capabilities, and logical display transforms; uinput creates correctly classified virtual keyboard, relative pointer, absolute touch, or gamepad devices only for explicitly supported modes. A virtual touchpad that libinput actually accepts as such is **UNVERIFIED** until tested; raw contacts are not automatically native gestures. Cursor metadata comes from PipeWire/compositor if available, otherwise capture it embedded. Clipboard uses the portal/EIS integration where supported, compositor-specific adapters only with documented limitations, and X11 selections in the legacy backend. No generic Wayland clipboard scraping promise. [C §§4.2, 4.4–4.6; Sunna research/05.]

Use a `systemd --user` service and desktop autostart for ordinary hosting. A small system broker/supervisor is installed only for an explicitly supported unattended/login feature. For the first portal release, locking normally suspends the stream, releases input, and requires compositor-allowed resumption; it does not bypass the lock screen. A future GDM integration must distinguish a new remote-login session from physical-console unlocking. Inhibit idle sleep only while an active granted session requests it through the desktop's power-management interface. Headless sessions have their own idle timeout. F §2.3 and C §4 describe different snapshots/scopes of RustDesk/Wayland support; neither justifies a blanket “all Wayland unattended access works” conclusion.

### 3.7 Linux/Windows hardware encoder configurations

Use one policy model with separate adapters. Direct NVENC is the first non-Apple optimized path. Initial QSV/AMF/VAAPI adapters may use a narrowly configured FFmpeg build with hardware frames and an explicit option allowlist; extract direct APIs only where measurements or reference repair require it. A direct libva/oneVPL path is not a prerequisite for the first Linux desktop release. Record the linked library version, accepted options and actual format. The following are **Sunna starting settings**, informed by inspected implementations, not claims they already exist in Sunna. [A §1.3, §§4.2, 5.2, 6.3; C §§2.4, 4.5.]

| Vendor/API | Exact initial low-delay policy | Codec/profile and recovery |
|---|---|---|
| NVIDIA NVENC, Linux/Windows | Preset P1, tuning `ULTRA_LOW_LATENCY`; `rateControlMode=CBR`; `gopLength=NVENC_INFINITE_GOPLENGTH`; `frameIntervalP=1`; `enableLookahead=0`; `zeroReorderDelay=1`; multipass disabled initially; spatial/temporal AQ off initially; filler off; weighted prediction off; `lowDelayKeyFrameScale=1`; `vbvBufferSize=ceil(R_video/fps)` bits, `vbvInitialDelay` same, subject to SDK acceptance | H.264 High 8-bit; HEVC Main/Main10; AV1 Main only on probed hardware. Use hardware-frame registration and async completion where supported. DPB target 5 H.264/HEVC, 8 AV1 when available for RFI; one active prediction reference initially. No automatic IR until repair tests pass |
| Intel QSV/oneVPL | Target usage equivalent to FFmpeg `preset=medium`; `async_depth=1`; no B frames; `look_ahead=0`; `low_delay_brc=1` when supported; low-power mode only if it supports requested profile/rate; CBR target/max equal initially; request one-frame VBV but log actual driver acceptance | H.264 High, HEVC Main/Main10, AV1 on capable hardware. IDR recovery initially. `NO_RC_BUF_LIMIT` or similar driver behavior means no claim of exact one-frame hardware buffering |
| AMD AMF | `usage=ultralowlatency`; `quality=balanced`; rate control latency-constrained VBR with target `R_video`, peak `1.1×R_video`; no B frames; preanalysis off; VBAQ off initially; HRD enforcement/filler/frame-skip off; one frame submitted at a time; request VBV `ceil(1.5×R_video/fps)` bits where accepted | H.264 High, HEVC Main/Main10, AV1 only on reported hardware. IDR recovery until a tested API provides stronger guarantees. Benchmark VBAQ on as a quality variant; do not assume Sunshine's setting is best for text |
| VAAPI / libva | Hardware NV12/P010 input, `async_depth=1`; no B frames; long GOP 32767 where the wrapper needs a finite value; block-level RC off initially; driver-supported CBR or tight VBR with target/max `R_video/1.1×R_video`; request one-frame HRD buffer, inspect accepted parameters | H.264 High, HEVC Main/Main10, AV1 when capability probe succeeds. IDR recovery. CQP22 desktop is a measured fallback only with admission/fps control; it does not inherently honor a network bitrate |
| Software fallback | H.264 x264 ultrafast, zerolatency, B frames 0, lookahead 0, sliced threads where safe, long GOP, repeat headers at IDR; cap to 1280×720 at 30 fps initially | CPU/VM/debug and compatibility tier. Explicitly labeled; not included in hardware performance promises. Lossless static tiles can still make stationary content sharp |

NVENC P1 can cost quality. Run P1 versus P3 and quarter-resolution two-pass at fixed latency/bitrate; promote P3 or multipass only if p99 encode completion stays inside the profile budget. Use a single-slice access unit initially. Multiple slices can improve scheduling only if the API actually exposes incremental output and the decoder path consumes it; a four-slice bitstream returned after complete-frame encode has not saved encode latency. AV1 tiles have the same caveat. Sunshine and Selkies' exact settings are evidence for experiments, not universal optima. [A §§1.3, 6.3; C §5.2.]

For NVENC RFI, retain a ledger of the unique `inputTimeStamp` used for each encoded picture, its reference status, and client decode acknowledgements. On loss, invalidate all potentially corrupted references from the first missing needed frame through the last dependent output, not merely the packet's immediate frame. `NvEncInvalidateRefFrames` uses those input timestamps; if no usable reference survives, request IDR. Do not describe this as VT's acknowledged-LTR mechanism: they have different contracts. If invalidation fails, the peer decoder reports corruption, or the ledger exceeds the hardware DPB horizon, transition to IDR recovery. [A §1.3; NVIDIA [NVENC programming guide, reference invalidation](https://docs.nvidia.com/video-technologies/video-codec-sdk/13.0/nvenc-video-encoder-api-prog-guide/index.html).]

H.264/HEVC 4:4:4 is a separately probed profile, e.g. NVENC High 4:4:4/FRExt where supported; do not turn on a profile because a GPU brand supports some 4:4:4 mode. Verify capture→encoder→decoder→shader end to end, including 10-bit storage and full chroma planes. No 4:4:4 profile is allowed to silently switch the client to an unbounded software decode path. AV1 4:4:4 is not a baseline claim. For vendors without trustworthy reference classification, post-encode dropping any non-IDR frame conservatively poisons the reference chain until recovery.

### 3.8 Windows capture, input, services, and displays

**Decision: support current Windows 11 builds first, with runtime capability checks; add older Windows only if demand justifies another test matrix.** Use DXGI Desktop Duplication for a low-overhead whole-output path where it is reliable; use Windows Graphics Capture for app/window capture and as a tested fallback for difficult desktop/HDR/hybrid-GPU cases. The adapter selection is recorded and can be overridden in diagnostics. [A §1.2; C §4.7.]

DDA uses `AcquireNextFrame` with a short bounded wait, reads dirty/move rectangles and pointer metadata, and releases each acquired frame promptly. Treat `DXGI_ERROR_ACCESS_LOST` as an epoch change requiring recreation, not an endless busy retry. WGC uses `Direct3D11CaptureFramePool.CreateFreeThreaded` with two buffers initially and recreates on size changes; event callbacks enqueue retained textures only. Color conversion occurs on the selected D3D11 device. Register textures directly with the hardware encoder where possible. Hybrid GPU cross-adapter sharing/copies are measured; fall back explicitly if handles/modifiers/fences cannot interoperate. `Flush` is not a substitute for a correct completion fence, and mapped CPU staging textures are a compatibility path.

Virtual monitors use a signed UMDF/IddCx driver, initially through a compatible maintained SudoVDA adapter if licensing/distribution permit. Ship this as an optional component after hardware/signing validation, not in the first Windows capture build. Assign stable monitor identity per authorized client, negotiate millihertz refresh, wait for output enumeration and an actual capturable frame before codec creation, and restore only owned changes. Use a 1-second liveness heartbeat and roughly 3-second disconnect watchdog with a longer 30-second broker-held reconnect lease when the agent remains healthy. HDR virtual-display support is gated by OS/IddCx version and verified EDID/color behavior; the dossier's 24H2/API 1.10 path is evidence, not support for every Windows 11 build. [A §3.2; C §4.7.]

Audio: WASAPI loopback on the selected render endpoint, event-driven capture, 48 kHz common pipeline; handle device changes without restarting video. Application-specific capture is a later tested capability. Keep speaker muting optional and restore only the state Sunna changed. Microphone forwarding is a separate grant and a later virtual-device integration, not automatically included with system audio.

Input: `SendInput` with scan-code/extended-key handling, explicit up/down state, UTF-16 committed-text fallback, absolute virtual-desktop coordinates and a relative pointer path. Client raw-input deltas must not be accelerated twice. Respect UIPI/integrity boundaries; an ordinary desktop process cannot promise control over elevated UI or the secure desktop. Pen/touch use supported synthetic pointer APIs with pressure, tilt, contact geometry and pointer IDs where available. Synthetic precision-touchpad injection is **UNVERIFIED across shipping Windows builds**; resolve the newer API at runtime and expose it only after real gesture tests. The common protocol preserves more information than every host can replay. [Sunna research/05; A §1.5; C §4.7.]

Use a tested signed virtual gamepad backend for XInput-compatible controllers, with HID/DS4-style profiles only when supported and needed. Existing ViGEm-based implementations establish feasibility but not a maintenance promise for Sunna; driver ownership, signing, security and distribution are an explicit Windows gaming milestone. First Windows preview can ship view/control without gamepads; it cannot claim Sunshine-equivalent co-play until the gamepad gate passes. Never require kernel anti-cheat bypass or claim universal compatibility with protected games. [A §§1.5, 3.3.]

The service runs as a minimally privileged account where possible; a Session-0 supervisor spawns/routes a signed user-session agent using approved Windows session APIs. It never captures Session 0 as if it were the interactive desktop. Named-pipe ACLs bind operations to the correct user/session. UAC secure-desktop and login-screen control require a separately reviewed installed service/agent path and explicit peer `login_control` permission. Until that is validated, pause with a clear “Windows is showing a protected screen” message. Clipboard uses change notifications and delayed rendering, the common offer protocol, format filtering, and limits. Cursor shapes come from DDA metadata or the supported cursor APIs; sanitize dimensions and hotspot.

Sleep uses scoped power requests during an active session; Wake-on-LAN is an optional LAN/site capability, never a promise across arbitrary internet routers. Lock/logoff/user switching follows the same privacy rule as macOS: release input, clear prior-user clipboard/refinement caches, reauthorize the destination session, then re-prime. A display-driver crash must not leave a permanently detached physical monitor or an invisible consent prompt.

## 4. Clients, presentation, input fidelity, and UI

### 4.1 Common decoder/presenter contract

The network actor authenticates, validates and assembles objects; it does not call a blocking decoder. The decoder owns reference state and produces retained GPU surfaces tagged with stream epoch, frame ID, source time, color and crop. The presenter holds only the latest eligible frame for the next display opportunity. Dropping an already decoded picture is safe for the decoder's reference state; dropping an encoded reference is not. Decode an old reference only if it is necessary to display a newer admitted frame before that frame's deadline. If it cannot catch up within two frame intervals, repair instead of silently building a queue. [A §§2.1–2.3.]

Decoders are recreated when codec configuration, parameter-set hash, geometry, chroma/depth, or incompatible color configuration changes. The current `ensure_session` behavior that returns an existing session without handling changed parameters is not sufficient. A new epoch cannot reference old decoder pictures. Keep the last valid display image while the new session primes, with an explicit reconnect/reconfigure indicator after 150 ms. Reject pictures exceeding negotiated dimensions, allocation limits, or reference count before allocating decoder surfaces where possible.

Presentation policy has two explicit settings:

* **Responsive**, default: newest complete eligible picture, no intentional extra frame buffering; one submission for the next usable presentation opportunity. Match source rate to the actual display when possible. Repeat the last frame if no new frame arrives.
* **Smooth**, optional for video watching: one additional frame of bounded jitter allowance, maximum 16.7 ms at 60 Hz or 8.3 ms at 120 Hz, exposed in the latency overlay. It is not the default for desktop control or gaming.

Fixed-refresh displays still impose a phase/scanout delay. VRR may reduce it only if the full platform/window/monitor path supports it. Do not advertise “zero-vsync latency” based on a swapchain flag. Actual present callbacks describe OS presentation timing, not photon emission. [C §§6.1, 6.4; A §2.3.]

### 4.2 macOS client

Decode with `VTDecompressionSession` to native NV12/P010 buffers where supported. Request hardware decoding for the normal tier, disable frame reordering where the stream permits it, submit with asynchronous decompression, and keep callbacks small. Wrap planes with `CVMetalTextureCacheCreateTextureFromImage`; perform YUV conversion, crop, refinement compositing, cursor and color conversion in a Metal pass. Avoid the current BGRA lock/copy and softbuffer scaling path. Use three `CAMetalLayer` drawables initially, one outstanding intended presentation, and asynchronous command-buffer completion to release buffers. Benchmark two drawables; use it only if it improves tails without starvation. [A §2.2; C §6.1; Sunna codec/viewer sources.]

Drive pacing with `CAMetalDisplayLink` on the supported baseline and a tested `CVDisplayLink` fallback. Request preferred frame latency 1 where available. Use target presentation time to select the freshest frame; do not wake an encoder with a stale frame merely to satisfy a nominal fps counter. Log GPU scheduled/completed times and `presentedTime` separately. Fullscreen, opaque layer and native pixel geometry are preferred where appropriate, but direct-to-display/compositor bypass is an OS decision and **UNVERIFIED per device/OS**.

The native view receives `NSEvent`/appropriate raw input callbacks, preserving keyboard hardware code, modifiers, repeat information, high-resolution scrolling phases/momentum, magnification, rotation, swipe and tablet pressure/tilt when delivered. Use a local IME/text-input client for composed text mode; raw physical mode sends HID usages without converting to characters. A visible toolbar toggle chooses “Keyboard follows this device” versus “Physical keys on host”; advanced settings can choose host-layout translation. System shortcut capture is optional, permission-gated, and always has a local release shortcut. No promise to seize every OS-reserved gesture.

### 4.3 Linux and Windows clients

Linux uses hardware decode via VAAPI or the best tested vendor path, exports/imports DMA-BUF into Vulkan with explicit synchronization, and presents through the native Wayland/X11 surface. The initial implementation may use a constrained FFmpeg decoder plus Vulkan renderer; it must preserve hardware surfaces. Wayland frame callbacks and presentation-time feedback determine submission; direct scanout is opportunistic. Under X11, report compositor/presentation timing limitations. A CPU fallback is available for the VM and unsupported GPUs, with explicit resolution/fps limits. [A §2.2; C §6.4.]

Windows decodes into D3D11 textures via the tested hardware decoder path and renders with a flip-model DXGI swapchain. Start with two buffers, waitable frame-latency object, `SetMaximumFrameLatency(1)`, and wait before scheduling the next present. Benchmark three buffers for devices that otherwise stall. Tearing/VRR is opt-in only where the application/OS/display combination supports it; default desktop mode preserves stable text. HDR uses a capable 10-bit/scRGB output path and correct DXGI color-space/HDR metadata calls. The swapchain does not infer HDR from a codec name. [A §2.2; C §6.4.]

Linux input uses the compositor-approved relative-pointer/pointer-lock and keyboard interfaces, libxkbcommon for layouts and composition, and native touch/tablet protocols where available. Windows uses Raw Input for high-rate mice and physical keys, normal text services for committed text, pointer events for pen/touch, and game-controller APIs for pads. Never sample input only when a video frame arrives. Coalesce 8 kHz mouse reports to a negotiated maximum of 1 kHz while preserving their cumulative movement; ship 250 Hz pointer packets by default and increase in game mode when useful. This saves packets without imposing video-frame-rate input latency.

### 4.4 Fidelity contract across all clients

| Input | Wire meaning | Host behavior / limits |
|---|---|---|
| Physical keyboard | USB HID usage page+usage, press/release, modifiers, location, sequence, client monotonic time | Host maps physical position; no guessing from Unicode; support ISO/ANSI/JIS, left/right modifiers and keypad |
| Text/IME | UTF-8 committed string, composition transaction ID, optional editing metadata | Inject committed text through supported OS path; do not also replay character-producing raw events |
| Shortcuts | Physical sequence by default; optional explicit semantic action | Preserve Cmd/Ctrl policy chosen by user; reserve emergency release locally |
| Absolute pointer | Display ID, normalized Q16.16 coordinate, transform epoch | Map to physical/logical display geometry and rotation; reject stale epoch |
| Relative pointer | Signed 64-bit cumulative Q16.16 deltas per input lease/device | Difference from last applied state; loss and duplication do not lose or repeat motion |
| Scroll | Signed cumulative axes, pixel/line unit, precise flag, phase and momentum phase | Preserve gesture boundaries; do not turn a trackpad into coarse wheel ticks |
| Pinch/rotate/swipe | Gesture ID, phase, semantic type, cumulative scale/angle/translation | Replay only advertised semantics; offer mapping where native injection is unavailable |
| Pen | Contact ID, position, pressure 0–65535, tilt ±90° in centidegrees, rotation, buttons, proximity | Windows/Linux supported paths first; macOS capability limited by available injection APIs |
| Touch | Stable contact IDs, down/move/up/cancel, geometry, tool type; max 10 contacts initially | Direct-touch and indirect-touchpad are different device capabilities |
| Gamepad | Stable slot ID, full button bitset, 16-bit axes/triggers, sample sequence; feedback channel | Up to 4 pads initially; reconnect keeps slot ownership for 30 s; rumble/output only to originating peer |

Host input leases expire after 250 ms without authenticated keepalive/state on a healthy active path; a known transport disconnect revokes immediately. For high-jitter connections the lease can be negotiated to 500 ms with a visible degraded indicator. Focus loss, local emergency shortcut, window close, denied permission, controller handoff, and host-local override send `ReleaseAll` and revoke the generation. Reliable late packets from an old generation cannot re-press a key. Key-repeat policy is negotiated: default host-generated repeat with transitions only; client repeat is an explicit alternative, never both. Store key states as usages, not an 8-bit OS virtual-key index.

### 4.5 Browser client decision

Browser access is a later **guest compatibility client**, not the reference performance client. Use standards-based WebRTC media with a native `str0m` adapter, ICE and TURN where necessary, H.264/Opus baseline, data channels for authorized input and clipboard metadata. `str0m` supplies protocol machinery but not a TURN client or encoder; that integration is explicit work. Use one host encoder rendition when it can serve both native and WebRTC formats; otherwise allocate a separate rendition under the same host bandwidth/resource budget. Do not tunnel native media over a reliable WebSocket and label it the same low-latency path. [A §§6–7.2; F §3b.]

The initial browser relies on its native WebRTC decoder/rendering path. It does not promise native tile-overlay alignment, private keyboard shortcuts, arbitrary clipboard access, HDR, 4:4:4, or bit-exact static text. A later WebCodecs/WebGPU renderer with explicit frame IDs may add refinement if browser capability and timing tests pass. WebTransport alone is not the selected arbitrary peer-to-peer traversal solution; HTTPS/WebSocket remains signaling and the TURN/TCP/TLS fallback exists for blocked UDP. Browser media performance is measured separately, including its internal jitter buffering.

Bind SDP/DTLS fingerprints and peer keys to the invitation/session transcript. A relay should forward encrypted DTLS-SRTP, not terminate media. However, a browser loads executable code from a web origin: a compromised origin can alter the client and steal displayed content or invite secrets. Therefore the strong threat model for a malicious Sunna service applies to independently installed native clients; browser security additionally trusts the delivered web application. This is stated plainly in the sharing UI. No misleading blanket E2E claim erases that dependency.

### 4.6 Mobile and tablet stance

iPadOS/iOS and Android are native **clients only** in the planned product. Background, whole-device unattended hosting is not promised. Implement thin SwiftUI/UIKit and Kotlin/Compose shells over the same Rust session/media core, with VideoToolbox/Metal and MediaCodec/Surface presentation respectively. Do not put raw frames through React Native or a webview; a later shared UI layer is acceptable for device lists but provides little benefit before desktop stabilizes.

Support external keyboards/mice, touch-as-trackpad and direct-touch modes, pen where the host can inject it, game controllers, rotation, safe-area handling and dynamic resolution. Touch-as-trackpad uses explicit tap/drag/right-click interactions and a visible keyboard button. A two-finger gesture can be reserved locally for the session menu; the reserve is visible and configurable. Mobile network changes use the same authenticated reconnect semantics; foreground the app to resume, release input when backgrounded, and do not claim to stream indefinitely through OS suspension. Begin at SDR 60 fps, one display; 120 Hz/HDR and multiple displays are later measured capabilities. Prefer HEVC only when the device decoder and thermal tests support it; cap resource use on battery.

### 4.7 UI structure

The main window has three primary destinations: **Computers**, **Share this computer**, and **Settings**. A computer card shows user-chosen name, availability and last successful connection; select it to connect. The first connection screen has a concise progress state (“Finding computer”, “Waiting for approval”, “Preparing display”), a cancel action, and a specific recoverable error if needed. Networking jargon appears in expandable diagnostics.

The native viewer has a small auto-hiding toolbar: connection health, monitor selection, keyboard capture, clipboard direction, quality mode, fullscreen and disconnect. Host sharing always has a visible participant indicator and stop-control/stop-sharing controls. Advanced bitrate/codec settings live behind diagnostics/advanced controls; defaults are negotiated from capabilities and measurements. A user who only wants to connect should not be asked to choose NV12 versus BGRA, an FEC ratio, or a relay region.

## 5. Media transport, recovery, congestion control, and wire formats

### 5.1 Transport choice and channels

Native sessions use one authenticated iroh endpoint connection per peer with QUIC DATAGRAM for deadline-bearing media and input state, plus several independently flow-controlled reliable streams. The production connection is built on the **Noq fork used by the selected iroh version**, not an imaginary drop-in upstream Quinn socket. Keep `sunna-link` as the adapter boundary. The inspected iroh manifest currently names `noq`, `noq-proto`, and `noq-udp` 1.3.0; pin the complete dependency graph and source revision. [F §3b; [iroh Cargo manifest](/home/sunny/research-repos/iroh/iroh/Cargo.toml:44).]

ALPN is `sunna/1`. Application protocol major 1 is incompatible with the prototype's postcard/25-byte media protocol; there is no unauthenticated legacy fallback. Minor capabilities negotiate forward-compatible extensions. Transport TLS encrypts control, headers, media, FEC and repair. No duplicate application AES mode or custom nonce construction is needed for normal native packets. QUIC DATAGRAM is unreliable and congestion controlled; it does not fragment an oversized application datagram or escape congestion control just because a packet is called input. [A §7.1; [RFC 9221](https://datatracker.ietf.org/doc/html/rfc9221).]

| Channel | Transport | Scheduling and semantics |
|---|---|---|
| Authentication/session/permissions | One bidirectional reliable stream, framed, strict small-message limits | Highest control class; permissions checked before capture or allocation |
| Input transitions | Dedicated reliable journal stream plus immediate datagram copy | Deduplicate by lease/device/event sequence; datagrams avoid waiting for stream retransmission |
| Pointer/scroll/gamepad state | DATAGRAM | Latest cumulative state; short deadline; state snapshots repair loss |
| Audio | DATAGRAM | 5 ms frame deadline with adaptive jitter and optional redundant previous frame |
| Video base/aux | DATAGRAM | Frame/group identity, FEC, selective RTX and reference repair |
| Network feedback / NACK | DATAGRAM; important configuration/LTR acknowledgements also reliable | Feedback can be superseded; decoder/epoch acknowledgements are idempotent |
| Cursor shapes | Separate small reliable stream or small datagram with repeat-until-ack | Positions latest-only; shape must be authenticated and bounded |
| Refinement | At most 4 cancellable unidirectional streams | One/few current tile records per stream; small write credits; no bulk HOL with session control |
| Clipboard | Dedicated bidirectional stream per approved offer | Offer first; fetch on use; length limits and cancellation |
| Files | At most 2 bulk streams, explicit user grant | Chunked, checksummed, resumable, lowest priority |

Reliable stream separation avoids application-level head-of-line blocking between a file and control, but all streams still share congestion capacity. Stop granting bulk bytes before congestion becomes a huge transport send queue. The client-to-host and host-to-client directions have separate send budgets; input priority is enforced wherever that direction also carries other traffic.

### 5.2 Common datagram header: exactly 64 bytes

All multibyte integers are unsigned big-endian unless explicitly signed. No native struct layout is serialized. All reserved bits must be zero in version 1. Payload is inside QUIC authentication.

| Byte offset | Width | Field |
|---:|---:|---|
| 0 | 2 | Magic `0x5355` (`SU`) |
| 2 | 1 | Application datagram version = 1 |
| 3 | 1 | Kind: 1 video, 2 audio, 3 input state/event, 4 recovery feedback, 5 arrival feedback, 6 cursor state |
| 4 | 2 | Flags: bit0 parity, bit1 retransmission, bit2 recovery-point object, bit3 disposable; other bits reserved |
| 6 | 2 | Header length = 64; extension only after negotiated minor capability |
| 8 | 4 | Session epoch, changed on new logical session/reconnect generation |
| 12 | 2 | Stream ID; allocated by authenticated session control |
| 14 | 2 | FEC group ID within object; zero for unsharded kinds |
| 16 | 8 | Packet sequence, unique monotonically increasing per connection direction |
| 24 | 8 | Object ID: encoded-frame ID, audio sequence, or input-state sequence according to kind |
| 32 | 8 | Source monotonic timestamp in microseconds; never replaced on RTX |
| 40 | 8 | Application send timestamp in microseconds, taken at scheduler release |
| 48 | 4 | Total unpadded object length |
| 52 | 4 | Byte offset of this FEC group's first data symbol within object |
| 56 | 2 | Fixed symbol size `S` for this group |
| 58 | 1 | Symbol index, data `0..k-1`, parity `k..k+r-1` |
| 59 | 1 | Number of data symbols `k` |
| 60 | 1 | Number of parity symbols `r` |
| 61 | 1 | Temporal layer ID; 0 unless negotiated |
| 62 | 2 | Actual payload length following header |

For an unsharded packet, group/offset/symbol fields are zero, `object_length=payload_length`, and the entire object fits the path's application datagram limit. Video uses `1≤k≤32`, `0≤r≤8`, `symbol_index<k+r`, `256≤S≤1024`. A final data symbol may be short and is zero-padded for FEC. Parity payload length is exactly `S`. Validate object length, ranges, group geometry, flags, IDs, and total allocation before creating a buffer. All symbols in a group must agree on geometry and object metadata; conflicting duplicates are a protocol error, not a buffer resize request.

Use `D_max=min(current_transport_max_datagram_size, 1180)` for application datagrams. Choose `S=min(1024, floor((D_max-64)/16)×16)`. If `S<256`, pause/reprobe rather than send malformed fragments. Typical media datagrams are 1,088 bytes (64+1,024) before QUIC overhead. Query the transport limit on path changes, not once at connection startup. A new MTU requires new groups; never change `S` in the middle of an already announced group. The object can be re-packetized with a new group allocation/version only after discarding conflicting partial assembly through an explicit recovery/config event.

Packet sequence identifies network delivery and RTX separately. Encoded-frame ID identifies decoder order and is allocated for actual encoder outputs only. Capture ID identifies unique source samples; an encoder-skipped input does not produce a missing wire frame. An RTX packet has a new packet sequence/send time but the same frame/group/symbol/source time. Sequence wrap closes/rekeys the logical epoch long before ambiguity; do not compare wrapping counters with ordinary subtraction.

### 5.3 Video objects, codec configuration, and reliable framing

A video object is a 64-byte prefix, an optional reference-ID list, bounded metadata, and the codec access unit. This **whole object**, including metadata, is FEC-protected.

| Offset | Width | Video object prefix |
|---:|---:|---|
| 0 | 4 | Codec epoch |
| 4 | 4 | Display/color epoch |
| 8 | 8 | Capture ID |
| 16 | 4 | Frame flags: IDR/random-access, recovery, disposable, opaque-dependencies, repeated-source |
| 20 | 4 | Encoded codec byte length |
| 24 | 8 | Source presentation timestamp in host monotonic microseconds |
| 32 | 4 | Intended duration in microseconds; zero for event-driven still |
| 36 | 2 | Metadata byte length, maximum 16 KiB |
| 38 | 1 | Explicit reference count, maximum 8 |
| 39 | 1 | Reserved = 0 |
| 40 | 16 | First 128 bits of BLAKE3 hash of authenticated codec configuration |
| 56 | 4 | Damage serial |
| 60 | 4 | Invalidation baseline serial, described in §6 |

Following the prefix: `reference_count` × 8-byte frame IDs; metadata; then `encoded_byte_length` bytes. Their sum must exactly equal outer object length. Metadata is canonical CBOR with integer keys and bounded depth, carrying LTR token (≤512 bytes), invalidation ranges, HDR dynamic metadata if supported, and codec-specific recovery proof. Unknown optional keys may be ignored; unknown mandatory capability IDs reject negotiation. An encoder that does not expose trustworthy reference lists sets `opaque-dependencies`; Sunna conservatively treats its non-disposable pictures as a dependent chain. Do not infer safe discard from “not a keyframe.”

H.264/HEVC access units use a negotiated canonical sequence of 4-byte big-endian NAL lengths followed by NAL bytes; no start-code scanning across network fragments. Parameter sets are carried reliably in `CodecConfig` and repeated within a recovery object where supported. AV1 uses negotiated low-overhead OBU framing with explicit sizes. Every configuration has width, height, crop, bit depth, chroma, transfer, primaries, matrix, range, reference limit and codec initialization bytes; cap initialization bytes at 48 KiB so the complete configuration fits the 64 KiB control-record limit. Larger ICC profiles use a separately bounded profile-transfer stream, with their hash/descriptor ID in control. A configuration acknowledgement precedes ordinary dependent frames. A recovery object can bootstrap after reconnect only against the matching authenticated config hash.

Reliable records have exactly this 16-byte header:

```text
offset 0:  u32 payload_length (excludes header; ≤65536 for control)
offset 4:  u16 message_type
offset 6:  u16 flags (mandatory/response/error; reserved bits zero)
offset 8:  u64 request_id (0 allowed for notifications)
offset 16: payload_length bytes of canonical, bounded CBOR
```

Initial types: `Hello=1`, `Auth=2`, `Grant=3`, `SessionOpen=4`, `Capabilities=5`, `DisplayConfig=6`, `CodecConfig=7`, `Ready=8`, `DecodeAck=9`, `RecoveryRequest=10`, `InputLease=11`, `ReleaseAll=12`, `TileOffer=13`, `ClipboardOffer=14`, `Close=15`, `Error=16`, `ClockSample=17`, `PresentedDamageAck=18`. Bulk streams have their own chunk limits; they cannot exploit the control record allowance to allocate a whole file. CBOR nesting ≤8, strings ≤16 KiB unless the typed schema permits more, collection counts bounded per message, no indefinite-length items. The parser rejects trailing data and duplicate keys. One dedicated reader owns each framed stream through partial length/body reads; cancellation never abandons half a message and starts reading a new length. This preserves a fix already present in Sunna's split control reader.

Every reliable stream starts with an 8-byte preface: magic/u16 `0x5353`, purpose/u8, version/u8 (=1), session epoch/u32. Purposes are session-control=1, input-journal=2, cursor-shapes=3, refinement=4, clipboard=5, file=6, color-profile=7. The initial session-control stream uses epoch 0 until `SessionOpen` establishes the logical session; all data streams must match the authorized live epoch. Unknown/unauthorized stream purposes are reset before large allocations. Cursor/profile/file records use their purpose-specific limits (256 KiB shape, 1 MiB total profile, streamed file chunks), not a larger control-message allowance. Profile/clipboard/file content is length-bounded and chunked; a file's declared total size never becomes a single memory allocation.

Normative initial negotiation maps use these integer keys; omitted optional fields take the stated baseline, and enum values are registry-controlled rather than arbitrary strings:

| Record | Required keys and value types |
|---|---|
| `Hello` | 0: major/u16 (=1), 1: minor/u16 (=0 initially), 2: role bits/u8 (host=1, viewer=2), 3: nonce/32 bytes, 4: feature bitmap/16 bytes, 5: limits map, 6: transport-authenticated endpoint ID/32 bytes; a differing claimed ID rejects the message |
| `Hello.limits` | 0: maximum control bytes/u32, 1: maximum media object bytes/u32, 2: maximum data symbols/u16, 3: maximum streams/u16, 4: maximum pixels/u32; negotiated values are minima, never increases to local hard limits |
| `Auth` | 0: method/u8 (existing grant=1, full invite=2, PAKE=3, compared transcript=4), 1: grant/invite ID/16 bytes where applicable, 2: method-specific bounded proof/≤4 KiB; proof verification uses the TLS peer identity and current transcript |
| `Grant` | 0: grant ID/16 bytes, 1: version/u64, 2: permission bits/u64, 3: OS-user scope/opaque ≤128-byte ID, 4: expires-at/u64 Unix seconds (0 only for explicitly persistent local grants), 5: approved display IDs/list≤8; the local host record remains authoritative |
| `Capabilities` | 0: encoder/decoder combination list≤32, 1: input capability bits/u64, 2: audio formats/list≤8, 3: color descriptors/list≤16, 4: maximum simultaneous renditions/u8, 5: refinement codecs/list≤8; sent after authorization, not as an unauthenticated hardware fingerprint |
| `DisplayConfig` | 0: display epoch/u32, 1: display ID/u32, 2/3: backing width/height/u32, 4/5: logical-scale numerator/denominator/u32, 6/7: refresh numerator/denominator/u32, 8: rotation/u16 (0/90/180/270), 9: color descriptor ID/u32, 10: ownership lease ID/16 bytes |
| `CodecConfig` | 0: codec epoch/u32, 1: display epoch/u32, 2: codec ID/u16 (H264=1, HEVC=2, AV1=3), 3: profile/u16, 4: bit depth/u8, 5: chroma/u8 (420=1, 422=2, 444=3), 6: init bytes, 7: max references/u8, 8: coded width/u32, 9: coded height/u32, 10: crop/[u32;4], 11: color descriptor ID/u32, 12: repair capability bits/u32 |
| `Ready` | 0: display epoch/u32, 1: codec epoch/u32, 2: accepted config hash/16 bytes, 3: first useful display deadline budget/us/u32 |
| `InputLease` | 0: generation/u32, 1: permission bits/u64, 2: duration-ms/u16 (250 or 500), 3: device IDs/list≤16; only the host can issue a controlling lease |

The feature bitmap initially allocates bits 0 RS, 1 RTX, 2 LTR-token feedback, 3 reference invalidation feedback, 4 exact tiles, 5 temporal-disposable frames, 6 HDR, 7 multiple displays, 8 input snapshots; unsupported optional features are off, never inferred from version alone. Video prefix flag bits are 0 random-access/IDR, 1 codec-verified repair, 2 disposable, 3 opaque dependencies, 4 repeated source. Authentication method 1 still requires an active host grant for the authenticated key; a guessed grant ID is not a bearer credential. Preauthorization allows only the small handshake/auth stream. Negotiation must finish within 5 seconds after consent, display preparation within 3 seconds, and initial codec priming within 2 seconds, with explicit retryable errors; waiting for a human consent prompt is a separate bounded 90-second state and does not allocate a media pipeline.

### 5.4 Input, feedback, and audio payloads

Input datagram payload begins with a 24-byte header: `lease_generation:u32`, `device_id:u16`, `event_kind:u16`, `event_sequence:u64`, `display_epoch:u32`, `body_length:u16`, `flags:u16`. Then a typed body. Physical key body is 8 bytes: usage page/u16, usage/u16, modifiers/u16, action/u8, location/u8. Relative motion body is two signed i64 cumulative Q16.16 values (16 bytes). Absolute motion is display ID/u32 and x/y signed i32 Q16.16 (12 bytes). Scroll body is cumulative x/y i64, phase/u8, momentum/u8, unit/u8, precise/u8 (20 bytes). A state snapshot includes last transition sequence, key bitmap for page 0x07, bounded extra usage list, button bitmap, and cumulative motion/scroll counters. Every size and capability is checked before host dispatch.

Transition events are sent immediately as datagrams and appended to the reliable journal. Apply each sequence once and in order; a missing transition requests immediate resend, while cumulative pointer state can continue independently. Send a full state snapshot every 50 ms and on focus/lease changes. A snapshot reconciles state but does not invent a missed text insertion; committed-text operations are reliable, idempotent transactions. Keep only 250 ms of unapplied transitions. If that deadline expires, release and revoke rather than replay delayed typing. Gesture begin/end/cancel are transitions; updates are cumulative and coalescible. Gamepad packets carry full state so one lost axis report does not stick a button forever.

Recovery feedback payload: 32-byte prefix (`report_seq:u32`, `codec_epoch:u32`, `last_received_packet:u64`, `last_decoded_frame:u64`, `entry_count:u16`, `flags:u16`, reserved/u32), followed by at most 16 entries of 20 bytes: frame ID/u64, group ID/u16, k/u8, r/u8, missing-symbol bitmap/u64. Only the low `k+r` bits may be set. Decoder corruption/no-reference flags request repair even if packet receipt appeared complete. LTR acknowledgements include exact token and codec epoch on the reliable `DecodeAck` path; duplicate acknowledgements are harmless.

Arrival feedback prefix is 60 bytes: report sequence/u32, path ID/u32, client receive time/u64, largest packet sequence/u64, 256-bit receipt bitmap/32 bytes, sample count/u8, flags/u8, reserved/u16. Each of at most 16 samples is 8 bytes: sequence delta from largest/u16, signed receive-time delta from report time/i32, received application bytes/u16. Send every 10 ms on low-RTT active links or 20 ms on WAN, with 50 ms idle reports, and cap feedback to its budget. QUIC ACKs remain the source for actual transport congestion accounting; this feedback supplies media ordering, arrival patterns and receiver deadlines. Authenticated peer feedback is still untrusted input and cannot raise sender caps beyond local limits.

Audio payload prefix is 16 bytes: sample counter/u64 at 48 kHz, sample count/u16, channel count/u8, flags/u8, primary Opus bytes/u16, redundant bytes/u16; followed by the primary packet and optionally the immediately previous Opus packet. Initial primary duration is 240 samples (5 ms). On very low bandwidth, negotiate 480 samples (10 ms) and 96–128 kbit/s stereo before sacrificing input. At 5 ms, headers/QUIC/IP make wire rate materially larger than the 160 kbit/s codec rate; the budget uses measured wire bytes. Redundancy is included only when it is likely to arrive within the audio playout deadline. No long RS audio block waits for many future audio frames.

### 5.5 Reordering, FEC, NACK and RTX

Replace the current single-partial-frame assembler with four bounded active frames. Accept reordered groups and packets without declaring every arrival of a newer frame a loss. Maintain separate counters for network missing symbols, FEC recovered symbols, deliberately skipped captures, encoder drops, pre-send frame drops, late complete frames, decoder failures, and presentation repeats. The same missing frame must not increment three independent “loss” estimators.

FEC is systematic Reed–Solomon over GF(256), primitive polynomial `0x11d`, applied to equal-length padded symbols. Define the exact matrix in golden vectors: Vandermonde rows at distinct field elements `1..k+r`, multiplied by the inverse of its first k rows so the first k output rows are identity. Sender and receiver use the transmitted integer k/r, never recompute them by rounding a percentage. A vetted, SIMD-capable Rust implementation must pass those vectors on x86 and arm64. This avoids the advertised-ratio versus actual-parity rounding failure found in Moonshine. [A §§1.4, 5.3.]

Concrete interoperability vector: for k=3, r=2, S=256, use data symbols whose first four bytes are `00 01 02 03`, `10 11 12 13`, and `20 21 22 23`, respectively, with all remaining bytes zero. The systematic generator's parity rows are `[7,9,15]` and `[7,8,14]` in the defined field. Parity prefixes are `6d 6c 6f 6e` and `5d 5c 5f 5e`; their remaining bytes are zero. All ten choices of three out of the five symbols recover the originals. This vector was arithmetically checked while preparing the proposal; it is not a performance benchmark or a substitute for implementation tests.

Select `k≤32` to cover at most about 4 ms of the current paced wire rate: `k≈floor(R_video_wire×0.004/(8×(S+64+estimated_transport_overhead)))`, clamped to 1..32 and remaining object symbols. This is a wire-time bound, not permission to hold a partially captured future frame. For tiny k, parity can be too expensive; prefer deadline-eligible RTX or smaller symbols. Parity is ready as soon as the complete group data is available. Interleave symbols from at most two groups to spread short bursts, without delaying an earlier deadline behind later video.

FEC policy defaults:

* Clean LAN, measured loss <0.05% over 5 s: r=0 on ordinary pictures; recovery points may use modest protection.
* WAN ordinary video: choose the smallest r giving an estimated independent-loss block failure probability ≤10^-4, using a conservative recent loss estimate, then cap parity/data at 25% and r≤8. If the target cannot be met within the cap, report residual risk and use repair/adaptation; do not assert the model covers correlated bursts.
* IDR/acknowledgeable reference recovery: allow parity/data up to 50%, still r≤8, by choosing k≤16 where needed. This is a bounded transient within the same total rate, not free bandwidth.
* Sustained >5% loss or bursts exceeding group protection: first reduce offered rate and frame size, consider path change; do not endlessly increase FEC into congestion.

Loss estimation excludes late application discards and intentionally unsent enhancement packets. Track burst length and correlate losses with sender/receiver queue delay. Parity/data `r/k` differs from parity/total `r/(k+r)`; show both correctly. At 20 Mbit/s with 20% parity/data, the data share is at most 16.67 Mbit/s before other overhead, not 20 Mbit/s plus “free” parity. [A §§1.4, 6.4, 9.3.]

NACK after the earlier of sufficient sequence-gap evidence and a bounded reorder wait: start at 2 ms LAN / 3 ms WAN, adapt up to 8 ms from observed reordering. Do not NACK before a known scheduled parity symbol could arrive if it can still meet the deadline. RTX is permitted only if `now + estimated_return_RTT + decode_budget + presentation_margin < useful_deadline`. There are two useful deadlines: the current picture's presentation deadline and the later reference-recovery deadline for descendants. A picture too late to display may still be worth repairing for future references, but this is labeled recovery traffic and bounded to 100 ms after source time or 2 RTT+50 ms, whichever is larger, with an absolute 250 ms cache horizon.

Initial video capture-to-presentation expiry budgets are 50 ms L60/R60, 80 ms W60, 140 ms H60, and 30/60 ms G120 LAN/WAN. These are late-frame guards, not promised normal latency. The sender's last-send deadline subtracts a conservative one-way path estimate (initially one RTT), measured p99 decode time and one display interval; queue admission uses host-local monotonic time. The receiver applies the mapped source deadline only while clock uncertainty is bounded; otherwise it uses a conservative receiver-local arrival/reorder budget and flags absolute age as unknown. No stale frame is presented over a newer eligible one merely because its timeout has not expired.

At most two retransmissions per symbol, separated by measured RTT/reorder evidence, total RTX initially capped at 10% of send budget. Recovery-point RTX may temporarily use the video allocation but never overrun audio/input budgets. If the needed symbol is no longer cached or no useful deadline remains, immediately choose LTR/RFI/IDR repair. Do not repeatedly request a keyframe once per missing packet. A single outstanding recovery request has a generation and cooldown `max(1 RTT, 50 ms)`; escalation after two failed repair attempts forces a fresh codec epoch if necessary.

### 5.6 Reference repair state machine

```text
Healthy
  ├─ loss of disposable picture → skip presentation; remain Healthy
  ├─ recoverable missing symbols → AwaitRepairSymbols(deadline)
  │       ├─ reconstructed + decoded → Healthy
  │       └─ expired → RepairReference
  └─ decoder corruption / sender dropped needed reference → RepairReference

RepairReference
  ├─ acknowledged VT LTR exists → request LTR refresh
  ├─ NVENC valid surviving reference ledger → invalidate damaged reference chain
  └─ no verified repair capability / failed repair → request IDR

AwaitRecoveryPoint
  ├─ authenticated matching-config recovery decoded → Healthy; acknowledge
  └─ timeout → lower frame-size/rate pressure, retry; rebuild codec if stalled
```

The sender's own admission/drop decision participates in this state machine. If an encoded reference was not fully sent, future dependent frames cannot be assumed good. An API's “frame dropped by encoder before output” is different and need not poison the chain. With opaque dependency APIs, be conservative. A recovery frame is displayed only if its codec-specific semantics establish it decodes against a reference the client actually has; receiving a flag from our own packetizer is not enough to prove the vendor honored the request.

The next successfully sent frame repeats bounded disposition ranges for deliberately omitted **verified disposable** outputs since the last decode acknowledgement, so their frame-ID gaps do not look like lost references. If disposition history exceeds 32 ranges, reset at a recovery point rather than send unbounded metadata. Unknown gaps remain potentially damaging. A repair-generation barrier stops new dependent submissions, drains/discards already in-flight damaged outputs, applies the vendor repair request, and marks the first proven recovery output; callbacks from the old repair generation cannot re-enter the healthy send queue.

IDR size is bounded by a wire-time budget. Default recovery admission is ≤64 ms serialization and ≤512 KiB, whichever is smaller, at the current allocated rate. If the encoder cannot satisfy that at the current geometry, raise recovery QP, use a temporary lower-resolution rendition with a new epoch, or explicitly show “recovering” while a longer recovery transfer is negotiated. Never cut off an IDR NAL and call it a frame, and never claim a 100 KiB frame crosses a 20 Mbit/s bottleneck in 2 ms: data alone takes roughly 40 ms. Quality-first mode restores full resolution as soon as stable; it does not silently remain downscaled. [A §9.3.]

### 5.7 One congestion budget coupled to encoder and pacer

There is one authoritative congestion controller per active network path, with an aggregate host uplink cap across peers. Its rate includes video, FEC, RTX, audio, input/control, refinement, QUIC/IP overhead and relay envelope overhead. The codec does not independently chase a configured bitrate while the transport buffers the excess. Start the private alpha with the transport's conservative controller plus a strict application pacer and queue-age admission; the custom media controller is a measured later replacement, not a prerequisite for secure first users.

The proposed production controller is a delay-sensitive rate controller with transport congestion-window enforcement. **UNVERIFIED:** it must pass fairness, stability and path-change simulations and hardware tests before replacing the baseline controller. Initial implementation policy:

1. Gather transport ACK/loss/ECN, delivered bytes, RTT, app-limited status, local queue residence, receiver arrival deltas, frame decode/present feedback and path identity. Keep a rolling 10-minute minimum RTT, reset on verified path change, and age out old baselines; the current forever-best RTT is not a valid permanent route baseline.
2. Update state every 20 ms, but apply a congestion reduction at most once per measured RTT for the same evidence. Use receiver inter-arrival minus sender inter-departure deltas to estimate changes in forward queueing; avoid asserting absolute one-way delay from unsynchronized clocks. RTT inflation supplements that estimate and is labeled ambiguous on asymmetric links.
3. Queue-delay targets: 2 ms known LAN, 8 ms ordinary WAN, 15 ms TCP relay compatibility path. If estimated queue exceeds target by 5 ms for two windows, or loss/ECN indicates congestion, set rate to 0.8× current; if application/transport queue age exceeds 20 ms, use 0.7× and stop refinement immediately. Hardware encoder overload is not network congestion; it reduces capture admission separately.
4. When queues remain below target, loss is low and traffic is not app-limited, probe upward by 5% per RTT, capped at 25% per second in steady state. Startup has a separate bounded probe phase, respecting the transport initial window; it may grow up to 2× per RTT with immediate exit on delay/ECN/loss. Do not disable slow-start safety and blast the remembered bitrate onto an unknown mobile link.
5. Use a delivery-rate estimate from multiple windows, marking app-limited samples so an idle desktop does not collapse its own capacity estimate. Remembered path capacity is a startup hint, not proof. After silence, restart with bounded bursts and probe.
6. Request congestion window around `R_wire×(RTT_min + 2×queue_target)/8`, with a floor of four path packets and transport-approved startup/growth/loss rules. This is a controller design to validate, not an API knob that overrides QUIC recovery. A congestion controller must respond to ECN and persistent congestion, including events with zero reported lost bytes.
7. At no feedback for `max(3×RTT, 200 ms)`, suspend new video and bulk, keep bounded authenticated path probes/lease handling, and enter reconnect if necessary. Never continue filling a socket for ten seconds because the transport idle timeout is ten seconds.

Encoder target is recomputed as:

```text
video_payload_budget = max(0,
    total_wire_budget
  - measured_transport_and_IP_overhead
  - input_control_reservation
  - audio_wire_budget
  - admitted_refinement_budget
  - admitted_bulk_budget) / (1 + parity_per_data + expected_RTX_per_data)
```

This is a first allocation estimate; actual measured byte accounting corrects it. Rate changes use an EWMA to avoid property thrash, except urgent downward admission cuts. Desktop policy preserves pixel geometry and readability first, reducing frame admission/fps when necessary. Game policy preserves cadence initially, accepting more quantization before a negotiated resolution step. Neither policy can manufacture capacity. Local encoder output overruns trigger immediate admission pressure and adjust rate/quality settings before the next submission.

The pacer is a monotonic token bucket at the approved wire rate with at most two-packet burst credit. No fixed 800 Mbit/s “pacing” regardless of bottleneck. Wake on actual next send permission; use platform high-resolution timers without permanent busy-spin. Batch at most four packets and aim for ≤250 µs of serialization per batch, with a minimum of one packet when a single packet already exceeds that interval. Recheck high-priority work at every batch boundary; an already transmitted packet cannot be preempted. On macOS, precision and power cost are measured together. [A §§1.4, 4.3, 6.4–6.5.]

Strict application order is **input and essential control > audio > base video and useful recovery > refinement > bulk**. Disposable video layers are lower than necessary base/recovery video. Cursor positions/lease releases are essential control; cursor shapes are small interactive metadata. Reserve up to 10% of available direction capacity, capped at 512 kbit/s, for interactive control/state and at least two packet opportunities where the congestion window permits. Rate-limit/coalesce a malicious or 8 kHz input source; priority is not infinite bandwidth. On sub-2-Mbit/s links reduce pointer packet frequency toward 60–125 Hz and use longer audio frames. A fully occupied congestion window can still delay urgent data: prevent that situation with admission and small staging, rather than claiming priority bypasses congestion control.

Refinement gets 10–20% while a desktop is active only if interactive queue age stays below target; after 100 ms of visual quiescence it may use up to 80% of the available rate. Bulk gets at most 5% during interaction and up to 80% only when media/refinement are idle, always preemptible at a chunk/write-credit boundary. Audio has a bounded minimum allocation; if the entire link cannot carry it plus control, negotiate reduced audio or mute with an explicit status instead of degrading input invisibly.

### 5.8 What specifically changes in Quinn/Noq

The inspected iroh configuration exposes a custom congestion-controller factory. Upstream Quinn's controller hooks cover sends, ACKs, end-of-ACK processing, congestion events, MTU changes, window and metrics. They do **not** by themselves implement per-datagram deadlines, application priorities or a new packet scheduler. Its ordinary `send_datagram` can evict older queued datagrams, while waiting for capacity can preserve stale media. These details are why setting “CUBIC off” is not an architecture. [A §7.1; [iroh controller factory](/home/sunny/research-repos/iroh/iroh/src/endpoint/quic.rs:415); [Quinn controller interface](/home/sunny/research-repos/quinn/quinn-proto/src/congestion.rs).]

The application layer can immediately implement bounded queues, frame admission, FEC/RTX, rate hints and chunked reliable writes. Keep transport datagram staging tiny and treat an unexpected eviction as a correctness fault. The production patch set, if required after profiling, is deliberately limited:

* Add an internal metadata-bearing enqueue operation, conceptually `send_datagram_with_meta(bytes, class, frame_id, expires_at)`. This is a **proposed Sunna API**, not an existing Quinn method.
* Expire application datagrams before packetization and report the exact discarded frame/symbol back to the media actor. Never silently erase a reference-bearing packet.
* Expose application bytes queued, oldest age, congestion send permission, path ID/RTT/loss/ECN and actual pacing decisions. Distinguish a packet accepted by the kernel from one acknowledged by the peer.
* Schedule eligible application data by priority/deadline while preserving QUIC-required ACK, handshake, crypto and loss-recovery behavior. Limit how far reliable stream data is admitted. Do not starve retransmitted control behind refinement.
* Connect the approved rate to the actual transport pacer if the selected Noq API does not expose that control. A `metrics().pacing_rate` value is not assumed to change the sender's pacing.
* Keep connection migration and per-path congestion state owned by Noq/iroh; do not secretly stripe ordinary video across two paths with unrelated delivery order and no budget accounting.

Before committing to a fork, run a two-week spike against the pinned iroh/Noq revision: can tiny datagram staging plus application pacing meet p99 queue-age gates without modifying packetization? If yes, avoid the scheduler fork for that release. If no, carry the minimal patch with upstream tests and an automated rebase/build lane. **UNVERIFIED:** source compatibility of every upstream Quinn controller hook with Noq must be established by this spike. The decision is reuse traversal and secure transport, own media scheduling, and accept a small measured patch—not replace QUIC wholesale.

### 5.9 Encryption, MTU, migration and relay behavior

Use the TLS 1.3/QUIC AEAD suites supported by the vetted transport provider, preferring hardware-efficient AES-GCM or ChaCha20-Poly1305 through normal negotiation. Peer identity is pinned as described in §8. No plaintext media mode, no accepting all certificates, no server-substitutable peer key. Disable 0-RTT for authentication grants, input, launching, clipboard and display changes; initially disable application 0-RTT entirely. Standard transport key updates and replay handling remain in the library. FEC operates on application bytes before QUIC encryption, so relays cannot inspect or repair it. Headers and feedback are encrypted too.

Start with a 1,200-byte QUIC UDP payload ceiling for conservative paths and the 1,024-byte media symbol above. Use transport path-MTU discovery and its current DATAGRAM maximum; never depend on IP fragmentation. On ICMP black holes or send errors, reduce to the safe baseline and reprobe. Tunnels, VPNs and relay envelopes reduce the effective path MTU; their overhead is part of the path capability. QUIC's minimum packet requirements must still be met—an underlying path unable to carry the minimum is not repaired by arbitrarily shrinking only the media payload.

On direct↔relay or Wi-Fi↔mobile migration, retain authenticated peer identity but reset path capacity/delay baselines, shrink initial offered video rate to at most 50% of the previous route until feedback arrives, cancel obsolete bulk/refinement writes, and issue a fresh useful recovery point if the delivery gap broke reference continuity. Duplicate only tiny lease/control messages during a bounded ≤1-RTT transition; do not duplicate an entire 40-Mbit/s stream for seconds. Path selection uses RTT, jitter, loss, queue age and stability, with hysteresis: require a candidate to improve estimated interactive delay by ≥5 ms or resolve loss for three probes before a non-emergency switch.

A TCP/WebSocket relay imposes outer reliable head-of-line blocking; native QUIC datagrams do not remove it. Bound application writes before the outer socket, measure relay queue residence, reduce bitrate aggressively on stalls, and present “Relay compatibility mode” when it materially affects latency. Data already in the TCP/kernel path cannot be unsent; if backlog exceeds 100 ms for 500 ms, abandon stale media and rebuild the relay path/session rather than drain seconds of history. Exact iroh relay queue visibility/cancellation is an integration gate; if it cannot expose it, ship conservative caps and disclose the limitation. A UDP forwarding relay improves hard-NAT cases where UDP is available, but cannot solve a network that blocks all UDP. [F §§3.8–3.10, 3b; A §7.]

## 6. Quality system: text, motion, color, HDR and multiple monitors

### 6.1 Desktop mode is video plus exact current tiles

Video gives an immediate complete view during scrolling, dragging and animations. Refined tiles replace its stationary regions with lossless canonical pixels. This adopts the useful RDP principle—different content deserves different representation—without reimplementing Windows drawing orders, fonts, application APIs or the whole RDP protocol. RDP's region-aware updates and gnome-remote-desktop's auxiliary chroma path are evidence for the approach, but AVC444 itself is not necessarily lossless. [C §§1.1–1.5, 2.3–2.5.]

Use a 128×128 backing-pixel tile grid, with smaller edge tiles. A GPU pass compares full-content hashes or equivalent collision-safe change detection over each complete capture; OS dirty rectangles reduce work but cannot alone establish correctness. Use a fast GPU change signature for scheduling, then a full cryptographic hash of extracted canonical bytes for cache/transfer integrity. If the GPU signature is not collision-free, periodically verify or use conservative full comparison so it cannot indefinitely mask changed pixels. A bytewise/tile reduction comparison against a retained prior canonical surface is the preferred correctness baseline; optimize only after proving equivalent behavior.

After a tile is unchanged for 80 ms, it becomes eligible for refinement. High-contrast text/caret-adjacent tiles may become eligible after 40 ms if queue budget permits. Prioritize the focused/pointed region, high edge density, and visible tiles; then large flat areas and photos. Initial classifier is deterministic edge/variance analysis, not a large ML model. Read back only selected tiles into pinned bounded buffers and compress on the low-priority workers. First format is raw canonical BGRA8, row-major without padding, compressed with Zstandard level 1. Send raw bytes if compression does not save at least 32 bytes. PNG, reversible color transforms, and deltas are later optimizations only if they improve bandwidth/CPU on the corpus.

Do not read back and compress the whole display every 16 ms. Desktop idle sends no repeated video pictures merely to sustain a nominal 60 fps counter. Keepalive, cursor and input continue independently. Send an occasional explicitly labeled still/recovery sample only when a decoder, reconnect or color change needs one. Target idle wire traffic <50 kbit/s after refinement with no audio, input or bulk transfer, including transport keepalives and reports.

### 6.2 Refinement consistency protocol

Each display has `display_epoch:u32`, `damage_serial:u32`, and each tile a `tile_generation:u32`. Any canonical pixel change increments that tile's generation and advances the display's damage serial; generation overflow creates a fresh display epoch and clears caches. A frame carries the current damage serial and the cumulative set of changed tile IDs **since the last presentation serial acknowledged by that client**, with each affected tile's current generation. A lost or discarded video frame therefore cannot hide an invalidation from a later frame.

Client rules:

1. Before presenting a new base frame, atomically apply its cumulative invalidations and update the known generation map. Clear any overlay whose generation no longer matches. Compositing and invalidation happen in the same render transaction.
2. Accept a refinement only if epoch, color descriptor, tile geometry and full BLAKE3 hash validate, the tile generation matches the known current generation, and the base already presented is at least as new as the tile's capture damage serial. A patch for a future base may wait for at most one pending frame/100 ms; otherwise request/reschedule it.
3. A delayed patch for an old generation is discarded even if it is perfectly lossless. A patch is never stretched across a resolution change.
4. If the frame's invalidation baseline is newer than the client's known applied serial, clear all overlays and require a full generation snapshot. Never guess what changed in the gap.
5. Send `PresentedDamageAck` only after the render transaction becomes the client's committed presentation state and its GPU work completes, using presentation feedback where available. Submission alone is insufficient; physical photon timing is not required for this consistency acknowledgement. The host may retire older cumulative invalidation history only after that acknowledgement. Decode ACK is insufficient for this purpose.

A full tile-generation snapshot is sent at startup, epoch changes and repair, and at least every two seconds while damage occurs. At the 32-megapixel cap, a 128-pixel tile grid needs at most approximately 2,048 tile entries; 6 bytes per `(tile_id:u16, generation:u32)` is roughly 12 KiB and fits the negotiated metadata budget. If cumulative invalidation grows beyond that budget, send “invalidate all” plus the full snapshot, not a truncated list. Per-client invalidation state is necessary because viewers present different subsets of frames.

Tile record format, after its reliable-stream preface, has an 80-byte fixed header: version/u16, codec/u16, display epoch/u32, damage serial/u32, tile ID/u32, generation/u32, x/u16, y/u16, width/u16, height/u16, color descriptor ID/u32, uncompressed length/u32, compressed length/u32, source capture ID/u64, BLAKE3 hash/32 bytes. The body length equals compressed length. Codec 0 is raw; codec 1 is Zstd without external dictionary. Uncompressed size must equal `width×height×bytes_per_pixel` under the negotiated canonical format; cap a tile record at 256 KiB, decompressor window at 1 MiB and expansion at the exact expected size. Reject trailing decompressed bytes. Future codecs require negotiation.

Native decoded tile cache starts at 128 MiB/session, mobile 32 MiB, browser only when its renderer supports the protocol. Cache key includes peer/OS-user security scope, epoch or reusable canonical color/geometry descriptor, and full content hash. Cache contents are memory-only by default and wiped on user switch/session close. A host may send a hash-only cache reference only after that client has acknowledged possessing it; a miss requests the tile. Never use cross-peer hash probing as a content-existence oracle. These rules are more important than squeezing a few percent from compression.

### 6.3 4:4:4 strategy and moving text

There are three ordered capability tiers:

1. **Baseline:** hardware 4:2:0 movement plus lossless static RGB tiles. This ships on the Macs without depending on undocumented hardware profiles. Text may transiently lose chroma while moving; the UI and benchmarks acknowledge that limitation.
2. **Native 4:4:4:** select a public, tested hardware encode/decode profile when both ends support it with acceptable latency/power. Inspect SPS/profile/chroma and decoded test charts. A hardware-acceleration boolean is not proof that the chroma plane stayed full resolution.
3. **Dual-stream chroma:** if moving text remains the decisive gap and native 4:4:4 is unavailable, implement an AVC444v2-style packing/reconstruction path from the published RDP layout, with **independent primary and auxiliary encoder reference chains**, matched frame IDs, and separately discardable auxiliary data. This is an explicit later milestone, not an assumed property of every H.264 decoder.

The auxiliary path consumes another encoder/decoder session, memory and bandwidth. Missing auxiliary data must fall back to the current primary picture, never reuse mismatched chroma from another frame. Reconstruction may still be quantized; only exact tiles get the lossless label. Run a cost/benefit gate on both Airs before committing to it. Avoid a fragile undocumented VT profile as the only way to achieve readable text. Apple's observed HEVC RExt output proves the observed application emitted such a stream; it does not prove a public hardware interface or license/distribution path exists for Sunna. [C §§1.2, 3.2, 5.2; A §10.]

### 6.4 Color management and HDR

First release defines a canonical SDR working raster: BGRA8 with explicit sRGB transfer/primaries, alpha treated consistently (opaque desktop by default), and a named transform from the captured display profile. “Lossless” means exact bytes in this negotiated canonical raster before client output color conversion; it does not mean a screen capture can recover an application's pre-compositor 16-bit assets. Display P3 preservation is a subsequent negotiated SDR color descriptor, with color-managed capture→working→display transforms and corresponding video primaries/matrix. Do not silently label converted sRGB as unmodified P3.

Every display/codec epoch carries source ICC/profile hash, canonical pixel format, primaries, transfer, matrix, full/limited range, chroma siting, bit depth, mastering metadata where applicable, and client tone-map policy. ICC payloads are capped at 1 MiB and parsed with a maintained color library under resource limits. Use ColorSync/OS color APIs where appropriate, with a tested shared transform implementation for cross-platform consistency. The shader converts decoded YUV according to its tags, composites tiles/cursor in the correct working color space, then transforms to the client display. Do not apply the sRGB transfer twice or treat limited-range YUV as full-range RGB. Pixel-center sampling and a 1:1 backing-pixel mode are mandatory; use nearest sampling for exact 1:1 alignment, and a measured high-quality scaler for intentional resizing. [C §§3.1–3.2, 6.2.]

HDR game mode negotiates capture that actually contains HDR values, P010/Main10 (initially HEVC), BT.2020 with PQ or explicitly supported HLG, correct metadata, 10-bit/float GPU storage, and an HDR-capable client presentation path. A mixed SDR/HDR desktop is composited under a declared SDR white level, initially 203 nits as a configurable policy target, not an assertion about every source OS's convention. A non-HDR client receives a host/client-selected tested tone map; report the conversion. Captured HDR metadata changes create an updated color epoch/configuration, and metadata survives recovery.

Lossless HDR refinement requires a separate canonical 10/16-bit format and substantially more memory/bandwidth; defer it until SDR exactness is proven. Do not describe an 8-bit BGRA tile over an HDR picture as lossless HDR. HDR, 4:4:4 and high refresh are independent capability axes, not one “best quality” switch. M1/M2 HDR capture/presentation combinations, virtual-display HDR and sustained thermals remain **UNVERIFIED until hardware tests**. [A §§1.2–1.3, 3.2, 5.2; C §§3.2, 4.7.]

### 6.5 Automatic desktop/game behavior

The user can choose Desktop, Game, or Auto. Auto starts in Desktop. Every 250 ms evaluate fraction of changed tiles, motion persistence, fullscreen/relative-pointer state, audio activity, encoder pressure and network headroom. Initial thresholds: enter motion policy after >35% changed tiles for 500 ms or an explicitly launched game requests it; return to desktop allocation after <10% changed tiles for 2 seconds. These are tunable hypotheses validated against video playback, scrolling code, slides and games. A scrolling editor should get responsive video without losing its high-resolution desktop geometry.

Switch bitrate allocation, tile scheduling and capture admission immediately within the same codec. Do **not** restart the codec every time a page scrolls. Change codec, chroma, bit depth or resolution only after a ≥5-second sustained need or explicit user choice, with a new epoch and recovery point. Keep a minimum 10-second interval between automatic resolution changes unless the session would otherwise fail. The overlay shows why a change occurred. Game mode disables expensive idle tile work while motion persists, prioritizes cadence and low encode latency, and uses optional verified temporal layers. Desktop mode restores exact tiles and chooses fps reduction before permanent text degradation on constrained links.

If an optimization changes the captured canonical representation itself (for example BGRA desktop capture to YUV-only game capture), invalidate all refinement overlays and reset their generation state even if video geometry stays the same. Returning to full RGB capture starts a fresh refinement snapshot. Mode switching cannot leave RGB patches from an earlier capture representation over a newer base stream.

### 6.6 Multiple monitors

Each output is an independent display ID, stream ID, color descriptor, damage map and codec epoch. Initially show one selected monitor; a later desktop release supports two simultaneous monitors up to the aggregate pixel/memory budget. Keyboard/pointer ownership is session-wide, coordinates display-specific, and the display arrangement includes negative origins and scale. A client window may span monitors only with explicit mapping; do not rescale the host desktop every time a toolbar appears.

Bandwidth weights default to active input monitor 70%, secondary visible monitor 25%, background previews 5%, adjusted by actual damage. A secondary static monitor quickly refines and consumes little ongoing video. Cap total capture/encode sessions according to hardware probes; decline an additional 4K60 rendition rather than overcommit. There is no cross-monitor wait barrier for ordinary independent work. For synchronized spanning animation, optionally align presentation within one refresh interval and show the latency cost. Hot unplug sends an epoch change, releases affected pointer confinement, and selects a surviving output without moving the cursor to an unrelated coordinate.

## 7. Connectivity: identity, discovery, traversal and relays

### 7.1 Build-versus-reuse decision

**Decision: reuse iroh's endpoint identity, address discovery, authenticated traversal, multipath/path maintenance and self-hostable relay support; own Sunna's application authorization, media protocol, rate policy and optional UDP relay extension.** Pin a tested revision behind `sunna-link`. This rejects F §3b.4's preference for building a new mapped UDP socket around upstream Quinn as the first production architecture. A single developer should not simultaneously reproduce hard-NAT traversal, roaming, relay fallback, media recovery and three OS engines. The inspected iroh code already provides a controller factory and an unstable custom-transport seam; the limitations are visible and manageable. [F §§3b.1–3b.4; [custom transport API](/home/sunny/research-repos/iroh/iroh/src/socket/transports/custom.rs:24).]

| Alternative | Decision and reason |
|---|---|
| Own rendezvous + STUN/puncher + Quinn | Reject as first implementation: too much path-state/security/rebinding work for one developer; reconsider only if measured iroh constraints cannot be isolated |
| iroh / Noq | Choose for native endpoints; Rust integration, authenticated device identity and migration fit the product; accept pinned-version/patch maintenance and current TCP-relay tradeoff |
| Embed tsnet/WireGuard | Do not require it: adds Go/runtime/overlay/account and tailnet concepts to a remote-computer product; excellent optional existing network, not the default identity/UX |
| Require user's installed Tailscale | Support as a direct-address network and first owner's test route, but not a public-product prerequisite; encrypted overlay does not replace Sunna peer authorization |
| WebRTC ICE + libjuice/str0m for all native media | Reject as primary native stack: useful standards compatibility, but still requires TURN integration, identity binding and media tuning; adds a second media model before native quality is solved |
| WebRTC ICE for browser | Choose when browser client ships; its compatibility value justifies the separate adapter |

Iroh's current architecture uses Noq and its authenticated NAT/path mechanisms, including QAD discovery; do not implement an obsolete STUN assumption against it. Tailscale's inspected hard-NAT path is also not the birthday-paradox port spray claimed by some older Sunna notes. These are explicit corrections to the early research, not features to recreate from memory. [F §§3.6, 3b.1; Sunna research/02–03.]

### 7.2 Addressing and discovery

Canonical device identity is a random Ed25519 public key/iroh EndpointId. A user-assigned name is a label, never an authenticator. Do not derive IDs from MAC addresses, serial numbers, email addresses, or IPs. Peer records contain identity, verified display name, grants, optional address hints, last successful path and revocation status. Network addresses are disposable hints attached to the stable identity.

Support three discovery modes from the first connectivity release:

1. **Nearby:** mDNS `_sunna._udp` with protocol version and a rotating opaque advertisement ID. Reveal a friendly name while “Share this computer” is open or the user opts into nearby visibility. Paired devices can recognize privacy-preserving paired tags and use cached pinned identity. mDNS discovery never auto-pairs a device.
2. **Internet:** signed presence and invitation rendezvous, with iroh relay/address hints. The service may learn public IP, online status, timing and routing metadata; disclose this rather than implying encryption hides all metadata.
3. **Manual/self-hosted:** import a signed device card/QR or enter an address with identity fingerprint. Direct IPv4, IPv6, LAN, VPN and Tailscale addresses all feed the same authenticated dial path. Plain `IP:port` still requires a pairing ceremony or known key.

A device card is canonical CBOR containing protocol version, 32-byte identity, optional device label, up to 8 addresses, up to 3 relay URLs, expiry and signature. Limit it to 4 KiB. A shared connection link carries a pinned host key and an independent invitation capability in its fragment; the URL's routing ID is not the secret. Default invitation validity is 10 minutes, one successful guest, view-only. A signed card with a bad address can cause a failed connection; it cannot authorize a different key.

Sunna rendezvous is a small HTTPS service, not a new global trust root. Proposed endpoints: `PUT /v1/presence/<opaque_lookup_capability>`, `GET /v1/presence/<capability>`, and `POST /v1/invites/<random_id>/signal`. Presence is ≤4 KiB, signed by the device, sequence-numbered, TTL 60 seconds, refreshed every 20 seconds while advertised. Lookup capabilities are random 256-bit values exchanged only with approved peers/account devices; do not expose a globally enumerable device directory. Rate-limit IP and device identities. The service checks signatures and freshness but clients still verify signatures and key pins. Address records are hints; signed old records cannot roll back a peer grant or identity.

Optional account sync stores encrypted address-book labels, peer cards and settings; the account makes discovery convenient but cannot grant control of an existing host by itself. Bootstrap a new account device by approval from an existing authorized device or a locally held recovery key. Password reset at the hosted service does not silently reset peer trust. Accounts, hosted relays, and paid infrastructure may have commercial limits without gating local pairing or self-hosting.

### 7.3 Dial state machine and timing

```text
HavePeerIdentity
  → collect cached/LAN/IPv6/IPv4/relay candidates (bounded)
  → start authenticated direct probes and relay connection concurrently
  → first authorized viable path starts session preparation
  → continue direct upgrade in background
  → stabilize selected path; periodically revalidate alternatives
  → migrate on failure / materially better path
```

At time 0, use cached working candidates and available LAN/IPv6 addresses; begin rendezvous and relay setup in parallel when internet discovery is needed. Do not wait 2.5 seconds for a doomed direct TCP attempt before creating a relay path. If a direct path is not ready by roughly 150–250 ms, allow a ready relay path to start the session; continue punching for at least 3 seconds and low-rate retries thereafter. These are Sunna policy targets around iroh's actual state machine, not assumptions that every internal timer is already configurable. Maintain at most the library's supported bounded path set, expose its chosen path, and do not multiply probes without a cap. [F §§1A.1, 3.3, 3b.1.]

Prefer a validated lower-delay IPv6 path, with IPv4 attempts delayed by no more than 50 ms when both families are available. A broken IPv6 route must not cost seconds. UDP/443 is a useful hosted relay/discovery port where allowed but is not indistinguishable from HTTPS and not a guarantee against filtering. Honor explicit proxies for HTTPS relay/control where supported; surface captive portals and authentication requirements. Enable PCP/NAT-PMP mapping as an optional convenience under a user/network policy; avoid mandatory router configuration and do not enable broad UPnP mappings without a clear setting. Local firewall rules are scoped to the signed app/service.

Mobile and CGNAT networks are ordinary cases, not error categories. Path change invalidates old capacity estimates, not identity. Keepalive frequency is chosen from library defaults and path needs, relaxed while idle to protect battery; inspect actual selected values rather than blindly layering another 1-second ping loop. Relay online status does not prove a sleeping host can capture. Presence reports “awake/agent available” separately from “device recently registered.”

### 7.4 Hosted and self-hosted relay service

First deploy an authenticated iroh-compatible HTTPS/WebSocket relay on TCP/443 in two independent regions. This provides a fallback when UDP is blocked. Choose the region minimizing measured combined host→relay and client→relay interactive path cost; users can pin a region/self-host address. Fail over control/relay availability without changing trusted peer keys. Do not log invite fragments, media payloads, clipboard data or screen-derived hashes. Collect aggregate load, authenticated allocation count, RTT buckets and queue-age metrics under the telemetry policy.

For hard NAT with usable UDP, add a **Sunna UDP forwarding relay** through iroh's `unstable-custom-transports` interface. This is deferred until the adapter spike proves it preserves Noq's migration/congestion semantics. The interface is explicitly unstable in the inspected source, so version pinning, integration tests and a migration budget are required. Do not promise a generic upstream Quinn socket wrapper can substitute for it. [F §3b; [iroh custom transport feature](/home/sunny/research-repos/iroh/iroh/Cargo.toml:163).]

Proposed UDP relay envelope is 32 bytes: allocation ID/u64, per-hop packet sequence/u64, truncated HMAC-SHA256 tag/16 bytes, then an opaque complete inner QUIC UDP payload. Per-hop MAC keys are independently random 256-bit secrets provisioned over authenticated HTTPS allocation channels to each endpoint; the relay verifies one hop and rewraps for the other. MAC input binds protocol/version, allocation generation, direction, sequence and complete inner payload. The inner QUIC ciphertext and endpoint authentication remain unchanged. This is a new security-sensitive protocol extension requiring review; it is not described as already provided by iroh.

The 32-byte envelope plus a 1,200-byte inner QUIC UDP payload plus ordinary IPv6/UDP headers (48 bytes) totals 1,280 bytes, fitting IPv6's baseline MTU without extension headers. Start at that inner ceiling and account for additional tunnels explicitly. No IP fragmentation. Use a 1,024-packet replay window per allocation/direction, rekey/reset only with a new allocation generation, and reject unknown IDs before expensive forwarding. A source-address change requires a bounded authenticated return-path challenge before forwarding to that address; possession of an observed envelope must not become a reflection primitive.

Allocation requires both peers' authenticated consent to the session, 30-second renewal, 120-second expiry, per-user/session bitrate and byte quotas, source-address validation, and approximately 1:1 forwarding volume. No unauthenticated internet packet forwarding. Relay queues are bounded per allocation to `max(2 packets, 5 ms of granted rate)`, capped at 64 KiB; drop expired datagrams rather than backpressure them indefinitely. A client needs an explicit indication of relay drops to distinguish congestion from codec faults, but never trusts it as a peer-authentication signal.

Self-host bundle contains the compatible relay, optional UDP forwarder, rendezvous service, configuration generator and health endpoint. One documented container deployment plus a native binary is sufficient initially. TLS certificate setup is automated when the operator owns a public hostname; LAN-only/self-signed deployment uses a pinned trust bundle, never “disable verification.” Import a self-host configuration as data; it cannot grant unattended access, execute scripts or replace already pinned peer keys. Existing trusted peers remain usable on LAN if hosted Sunna services are down. A hosted-service outage can prevent discovery/relay when no alternate route is known; encryption is not availability.

## 8. Security, identity and trust

### 8.1 Threat model and invariants

Protect screen/audio/input/clipboard content against network observers, malicious relays and a compromised rendezvous service; prevent an unpaired party from obtaining capture or input; constrain an authorized guest to explicit capabilities; limit damage from malformed media and platform helpers. A compromised endpoint OS, authorized viewer recording the screen, or malicious replacement of browser-delivered code is outside the same E2E guarantee. A host owner granting control to a scammer is an abuse problem that cryptography alone cannot solve.

Non-negotiable invariants:

* Transport encryption without authenticated peer identity is insufficient. Remove the prototype's accept-all certificate verifier and unauthenticated host before any non-private distribution.
* A directory/relay cannot replace a previously pinned device key. Key changes stop automatic connection and explain the device identity change.
* No capture, decoder allocation beyond small negotiation limits, input, clipboard, launch or display mutation before authentication and authorization.
* View, control, audio, clipboard, files, launch, display management and login control are separate grants enforced by the host agent as well as the broker.
* No permission is inferred from being on the LAN, in the same account, or reachable over Tailscale.

These choices explicitly avoid the directory-key/plaintext-fallback weaknesses traced in RustDesk. They do **not** imply that Parsec has those weaknesses: the public evidence does not establish its entire key exchange. F §6.2's suggestion of a specific active-service attack on Parsec goes beyond what its public documents prove; treat that as unknown. [F §§1A.5, 4.5, 6.2.]

### 8.2 Identity storage and first pairing

Generate a device Ed25519 seed with the OS CSPRNG. Store it in the platform credential store or encrypted under a credential-store wrapping key, with access limited to the signed broker/service identity. macOS Keychain, Windows DPAPI/appropriate service credential protection, and Linux Secret Service or a protected service keystore are adapters, not assumed identical security properties. For unattended pre-login operation, test the selected service identity's ability to access the device key while the user keychain is locked. Do not claim Ed25519 is automatically hardware-backed by Secure Enclave. Root/admin compromise can defeat the local host; avoid exporting seeds into logs or webview state.

Pairing options, in order:

1. **QR or full link:** carries pinned host EndpointId and a 256-bit random one-time invitation secret, default 10-minute lifetime. Native client verifies the expected host key during connection, then presents the secret only inside that authenticated channel. Host shows the requesting device name and requested capabilities and approves; the host records the client's authenticated key. A guest link defaults to one session, view-only, no persistent trust. A link can skip another PIN without skipping host consent unless its creator explicitly chose an unattended, scoped invitation.
2. **Nearby/manual comparison:** discover the candidate, show a short authentication string derived from the full handshake/transcript and both keys on both devices, and require matching confirmation. Use six words from a fixed 2,048-word list (66 bits), with the full fingerprint available. This is a human verification ceremony, not trust-on-first-use hidden behind an “OK” button.
3. **Short code fallback:** a routing/mailbox component plus an 8-digit random secret, protected by a reviewed SPAKE2 exchange; three failed attempts invalidate the invite, expiry 10 minutes, global and per-device/IP throttles. No bare password hash challenge-response. QR/full links can ship before this implementation passes review.

For the short-code protocol, choose RFC 9382's P-256/SHA-256/HKDF/HMAC suite, client/host roles fixed, canonical identities and TLS exporter/application version/invitation ID bound as authenticated context. Both sides require key confirmation before pinning keys or granting access. Password-to-scalar derivation, point validation, transcript serialization and test vectors must come from a maintained reviewed implementation; do not write curve arithmetic or subtly modify the RFC in Sunna. **UNVERIFIED:** the final Rust dependency/audit status and interoperable binding are an explicit pre-ship gate for this optional flow. An informational RFC is not itself an implementation audit. [F §6.1; [RFC 9382](https://www.rfc-editor.org/rfc/rfc9382.html).]

The hosted rendezvous only routes pairing messages. A malicious service may deny service or show a different candidate, but cannot complete the out-of-band-secret-bound exchange without the secret. Full invitation links keep secret material in the URL fragment, use no third-party scripts/analytics on the join page, set a strict content-security policy, and never send fragments in diagnostic reports. Links are bearer capabilities until redeemed; revocation, expiry, single use and explicit capabilities are essential. A “remember this computer” action creates a peer grant; merely viewing once does not.

### 8.3 Grants, unattended access, and revocation

Host grant record fields: grant ID/128 bits, host device ID, host OS user/seat scope, peer identity, creation/expiry, issuer identity, permission bitset, allowed displays/apps/directories, unattended flag, optional login-control flag, and monotonically increasing grant version. Sign exports with the host identity; store the authoritative local record transactionally. A capability token is valid only while its grant/version remains active. Recheck it on every privileged action, not just connection establishment.

Initial permission bits: `view`, `control_pointer`, `control_keyboard`, `gamepad`, `hear_audio`, `send_microphone`, `clipboard_read_host`, `clipboard_write_host`, `file_read`, `file_write`, `launch_allowlisted_app`, `manage_owned_display`, `unattended`, `login_control`. No default remote shell. Co-play can grant gamepad without keyboard/mouse. Clipboard direction is explicit; unattended view does not imply clipboard extraction. File permissions use selected directories/file pickers and deny path traversal/symlink escape; never equate a file-transfer grant with arbitrary filesystem access.

Unattended setup is a local authenticated owner action, with OS authentication where supported, naming the exact peer and scope. Optional account passkey/second factor protects account/device enrollment; it does not replace a device signature. Never store the user's OS login password as a Sunna unattended password. Login control transmits the user's interaction to the actual login screen under an explicit grant; no OS authentication bypass is promised.

Revocation increments grant version, terminates active leases, sends `ReleaseAll`, closes affected sessions and invalidates resume tickets. Offline peers discover revocation on the next connection because the host remains authoritative. Lost host identity requires a fresh pairing or a previously configured owner recovery procedure; the cloud account cannot silently certify a replacement host. Planned key rotation is cross-signed and locally owner-approved, with an audit entry and a bounded overlap period; unexpected changes require user review.

### 8.4 Abuse resistance and local visibility

Default guest access is attended and view-only. The host consent card says who is requesting access, what they can do and whether access lasts beyond this session. Show a persistent participant indicator while sharing, an immediate local stop-control shortcut, and an accessible stop-sharing button. Local physical input can reclaim control under the configured policy. Do not provide a hidden mode that conceals an active remote viewer from the local user.

Add a concise warning when a new guest requests keyboard/control or persistent access: “Only allow someone you know and trust. They can operate this computer.” Do not show repetitive scam dialogs on every established trusted connection. Refuse link/config imports that grant unattended access automatically. Rate-limit invitation attempts and unauthenticated handshakes; use transport address validation/cookies and cap pending unauthenticated state to 32 KiB per candidate, 32 candidates globally, and 4 per source prefix before authentication. Defaults are tuning choices and must be tested under shared-NAT legitimate use. No screen thumbnails, clipboard contents or typed keys in telemetry.

### 8.5 Audit and update security

Local audit events include pair/unpair, grants/revocation, session start/end, peer identity, path category, requested/approved control, app launch ID, file-transfer metadata if enabled, display changes, permission failures, updater actions and helper errors. Do not log keystrokes, text clipboard contents, invitation secrets or full screen-derived hashes. Store bounded append-only records with a hash chain and periodic device signatures, rotate at 50 MiB/90 days by default, and expose export/delete controls. This detects accidental modification and supports review; a fully compromised administrator can replace local logs unless they were independently exported. Audit is not a fictitious tamper-proof guarantee.

Updates use signed artifacts plus platform signing/notarization and TUF-style metadata: offline root keys with a 2-of-3 threshold, delegated platform/channel targets, online short-lived timestamp/snapshot roles, version/expiry checks, and explicit rollback protection. The operator must actually maintain independent offline key storage; three files on the same build machine do not create meaningful separation. Pin the shipped root, verify metadata before download/install, verify artifact hashes and platform signatures, stage atomically, and retain a known-good local rollback only within the allowed security version floor. [F §7.4; [TUF specification](https://theupdateframework.github.io/specification/latest/).]

Initial metadata policy: timestamp expires after 24 hours, snapshot after 7 days, targets after 90 days, root after 1 year with scheduled rotation well before expiry. These are Sunna deployment choices. Expired update metadata prevents accepting an unverified update; it does not by itself stop a valid installed offline/LAN session. Artifact version floors and emergency revocation are separate signed policies with explicit user-visible reasons.

Do not let a remote peer supply an updater URL, executable, launch command, or privileged helper request. Install updates when sessions are idle by default; urgent security updates notify and use a declared policy, not a surprise mid-keystroke restart. A compromised hosted relay/rendezvous without signing keys cannot ship executable updates or mint peer grants. Reproducible build metadata, SBOMs, dependency pinning, release provenance and an external review of pairing/helper/packet parsing are part of the public release gate.

## 9. Ease of use: concrete first-run and failure flows

### 9.1 Counting rules

Count installation separately from post-install setup; count actions on **both** machines; count OS permission dialogs separately rather than pretending they are one Sunna click. A product task can contain an OS transition, so publish both task completion and physical action counts during usability tests. The exact Sunna labels/steps below are normative design. OS labels and raw click counts vary by OS release, prior grants and administrator policy and are **UNVERIFIED until tested with signed builds**. Do not claim a universal three-click first Mac session.

The standard first-session scenario below is two installed native apps, an awake unlocked host, no existing Sunna grants, one selected desktop, QR/full-link pairing, and attended approval. An optional account is never on the critical path. Download/install adds approximately two user tasks per machine (obtain package; install/open), with platform security dialogs measured separately. Those installation task estimates are not a measured benchmark.

### 9.2 macOS host and client

Mac host has **five setup tasks and one first-peer approval**, in this order:

1. Select **“Use this Mac remotely.”** Explain: “Sunna will share the screen you choose. You can stop sharing at any time.” No service or permission requests before this choice.
2. Select **“Allow Screen Recording.”** Explain: “macOS needs your permission to send this Mac's screen.” Invoke the supported request and open the relevant Settings location if required. The card stays visible with a live status, an exact next action and a return-to-Sunna link. If the OS requires restart, offer **“Restart Sunna and continue”**, retaining setup state; no manual terminal command.
3. Select **“Allow keyboard and mouse control”** for a control-capable host, or **“View only for now.”** Explain: “Only people you approve can control this Mac.” Request posting/Accessibility authority, never unrelated Input Monitoring. A deny keeps view-only setup usable. This is one product task with a capability choice; the system toggle/authentication actions are counted separately.
4. Select a previewed display and choose **“Continue.”** Options are **“This display”** and, when supported, **“A separate display for the remote device.”** Show the actual geometry/scale and whether local people can see it. System audio is an explicit visible toggle using the available SCK grant, not a surprise separate driver install. No HDR/codec menus here.
5. Select **“Create link”** or display the QR. The card states **“One session · View only · Expires in 10 minutes.”** Changing it to control or repeated/unattended access is an explicit scope change, not a hidden default.
6. On the first authenticated request, select **“Allow once.”** Consent copy: **“[Device name] wants to view [display].”** Requested extras appear as plain-language toggles. “Remember this device” pairs its key; **“Allow when I'm away”** is a separate local-authenticated action with named scope.

Mac client has **two tasks**: open/scan the link on the intended device, then select **“Connect”** after seeing the host name and requested session scope. The host approval supplies the first media grant. Basic viewing/control capture within the Sunna window requires no host screen-recording permission on the client. **“Send system shortcuts”** is a later optional client task and its own OS grant, with a persistent local escape shortcut displayed before enabling it.

Thus the reference attended flow has **8 product tasks total** (host 5+1, client 2). A view-only host follows the same five-task flow but skips the actual control permission operation. Enabling persistent unattended access adds **one explicit product task**, plus OS authentication and any service registration approval. A compulsory agent restart adds **one Sunna action**; it does not restart the wizard. Screen and control Settings typically add several system actions; record them, with a target of ≤8 combined permission-related OS actions on the supported baseline. This is a usability target, not an assertion that Apple permits us to collapse those dialogs.

If the user selects “Use only as a viewer,” there are no host permissions or background hosting service. If they later enable hosting, continue at the first missing capability. Permission cards use actual preflight/capture tests and show **Allowed**, **Needs permission**, or **Could not verify**, not a permanently green checkbox after the first click. TCC behavior, responsible-process attribution and persistent-capture limitations make signed real-app testing essential. [C §§3.3–3.4; F §§1B.9, 5.2, 5.8.]

### 9.3 Linux flow

For the supported portal backend, **three host setup tasks plus one peer approval**:

1. **“Use this computer remotely.”** Select existing desktop; do not suggest a root helper as the normal route.
2. **“Choose a screen and allow control.”** Open the compositor's portal chooser. The user selects the monitor and approves the requested devices. Offer the compositor's remember/persistence option only if supported. The portal's source selection and Allow are **two system tasks/actions to measure separately**, with any additional backend prompts recorded. Save/rotate the returned restore token atomically.
3. **“Create link.”** Show the same one-session view-only scope and audio availability; no codec/IP configuration.
4. **“Allow once”** for the authenticated peer request.

Client tasks are the same two as macOS, for **6 product tasks total** in this reference flow. A restore token can reduce later compositor prompts, but unattended service/login access is not silently promised. If EIS is missing, say **“Viewing is available. This desktop does not provide remote keyboard and mouse control to Sunna.”** An advanced, separately installed helper is a distinct setup flow with clear elevated authority, not an opaque “fix Linux” button. The first Linux release supports named compositor/version combinations; use capability detection to give specific explanations for others. [C §§4.1–4.6.]

### 9.4 Windows flow

For ordinary user-session hosting, **three host setup tasks plus one peer approval**:

1. **“Use this PC remotely.”** The signed application registers only needed network access. If Windows shows a firewall prompt, explain the application's name and intended network scope without telling users to disable the firewall.
2. Choose **“This display”** and select **“Continue.”** The optional virtual-display driver has its own later **“Add a separate display”** installation task, local admin approval and recovery check; it is not required to share a physical monitor.
3. **“Create link.”** Same scoped guest default.
4. **“Allow once.”** Same verified peer and permissions card.

With the client's two tasks, **6 product tasks total**, plus a firewall system action if shown. **“Available after restart”** adds one explicit local task and service/UAC approval; protected-screen control is a separately described capability. Gamepad driver installation adds one setup task and its OS approval when co-play is first enabled. Do not impose these installations on a user who only wants to view a desktop.

### 9.5 Own devices, guests, and routine sessions

For the owner's two computers, pairing ends with a clear optional card: **“Allow this device to connect when I'm away”**, listing view/control/audio and OS-user scope. After local authentication/approval, each device appears in **Computers** and routine connection is one selection. No account is required; signing in later can synchronize the verified device list without silently authorizing new keys. When the host is offline, show the last contact and likely sleep state, with Wake offered only when actually configured.

A guest link opens a page offering **“Open in Sunna”** and, once implemented, **“Continue in browser.”** The browser button states its limitations and follows the same host consent. The host can revoke an unused link or disconnect/revoke a participant from one visible control. Share links created during a session inherit no broader rights than their creator is permitted to delegate; initially only the local owner can create invites. A “request control” action prompts the host without interrupting existing viewing.

For an installed iOS/iPadOS or Android client, the basic flow is **two client tasks**: open the invite link, then **“Connect.”** A QR route substitutes **“Scan a code”** for opening the link and may add a camera system-permission action; permission copy is “Use the camera to scan a Sunna invitation.” **“Nearby computers”** is an optional additional task that requests local-network/discovery access only when needed, with “Find your computers on this network.” No screen-capture, microphone or blanket input-monitoring permission is requested for ordinary viewing. External controllers use the OS's paired-device path; additional platform permissions are requested only for an implemented feature that needs them. Exact mobile OS prompt counts remain part of platform acceptance testing.

The browser guest flow is **three client tasks** when starting from the shared link: open the link, choose **“Continue in browser,”** then **“Connect.”** The explicit Connect gesture also starts permitted audio playback. Host approval remains a separate host task. Fullscreen/pointer lock, clipboard paste and microphone each require their own user action/browser permission when used; they are not silently included in the three-task viewing count. If the browser cannot support a requested control capability, show the limitation and an **“Open in Sunna”** action while keeping view-only access available.

First-frame preparation begins while safe independent work is possible: resolve routes and inspect local decoder capabilities while awaiting approval; after authorization prepare capture/encoder and configuration concurrently where dependencies permit. Do not capture the host desktop speculatively before consent to make the stopwatch look better. The first-frame timer ends on actual presentation, not TCP connect or decoder creation.

### 9.6 Failure diagnostics that tell the user what to do

| Failure code / evidence | User-facing message | Recovery |
|---|---|---|
| `HOST_ASLEEP`, recent broker presence but no user agent/capture | “This Mac is asleep or unavailable.” | Wake if configured; otherwise explain local power setting, preserve retry state |
| `SCREEN_PERMISSION`, negative preflight or SCK permission error | “Allow Screen Recording on [host] to continue.” | Host notification and exact Settings action; no reconnect loop |
| `CONTROL_PERMISSION`, failed post-event preflight | “You can view this computer. Keyboard and mouse control needs permission on the host.” | Continue view-only; host grant card |
| `PORTAL_CANCELLED` | “Screen sharing was not approved on the host.” | One retry action, no hidden helper fallback |
| `KEY_CHANGED` | “This computer's identity changed.” | Stop; compare/re-pair with owner; never offer an automatic insecure bypass |
| `UDP_BLOCKED`, verified relay fallback | “Connected through a compatibility relay.” | Continue with measured quality adjustment; details show path and latency |
| `RELAY_UNREACHABLE`, no alternate candidate | “No route to this computer is available.” | Retry, choose configured alternate/self-host relay, inspect proxy/captive portal |
| `ENCODER_UNAVAILABLE` / resource limit | “This computer cannot start the requested display quality.” | Offer a named lower resolution/software tier; preserve other viewers |
| `DECODE_UNSUPPORTED` | “This device cannot display the selected video format.” | Renegotiate baseline codec; no repeated crash/reconnect |
| `AUDIO_SILENT`, no definite permission error | “No sound is arriving.” | Check selected output and test audio; do not assert permission denial from zero RMS |
| `NETWORK_CONGESTED`, growing path/queue delay | “The connection has less bandwidth available.” | Explain fps/quality adjustment and current resolution; expandable evidence |
| `PROTECTED_SCREEN` | “The host is showing a protected login or permission screen.” | Use supported authorized login path or request local action |

Diagnostic export includes app/OS/GPU versions, accepted codec settings, permission capability states, route category, stage histograms and redacted error IDs. It excludes keys, invitation secrets, clipboard, screen content and precise remote addresses by default. An advanced user can opt into a short encrypted packet/timing trace with an explicit scope. The default UI shows a useful next action, not a dump of all possible network advice.

## 10. Session model, co-play, launching, resolution and reconnection

### 10.1 Session and viewer ownership

A host session owns an OS user/seat, selected displays, audio route and display leases. Each attached viewer has its own authenticated peer, grant, transport, receiver feedback, input lease and quality allocation. The broker can run multiple allowed sessions, but the initial product has one active console session per OS user and at most four viewers, subject to resource admission. This replaces the prototype's serial accept/serve lifetime without requiring a multi-user OS compositor in v1.

Two encoder renditions per display are the initial maximum: a controller/high-quality rendition and an optional lower-bandwidth spectator rendition. Share an encoded stream only among viewers whose codec/configuration and reference knowledge are compatible. Each viewer still gets its own packet pacing/FEC/RTX budget. For a shared encoder, acknowledged LTR availability is the intersection of the participating receivers' valid references; one lossy spectator must not silently force the controller into repeated global recovery. Detach that spectator to the lower rendition, recover it at a bounded shared recovery point, or suspend its video with a clear reason. Do not solve the problem by claiming one encoder can generate arbitrary per-viewer reference chains.

The controlling viewer's latency and quality take precedence over spectators under a constrained uplink. Aggregate rates, including duplicate encrypted copies, cannot exceed host capacity. A new spectator receives codec config and a valid recovery point, not arbitrary recent P frames. Limit join-triggered recovery to once per 500 ms across a rendition and coalesce simultaneous joins. When hardware capacity is full, offer a lower tier or deny the new stream without disturbing existing users. No server-side media mixing/decryption service is required.

### 10.2 Control leases and co-play

One remote keyboard/mouse controller at a time by default; viewers can request handoff and the local host can reclaim it. A handoff releases all old-generation pressed state before activating the new lease. Co-play grants independent gamepad slots, initially four, and no keyboard/mouse by default. Slot ownership is stable for a 30-second reconnect lease, but a disconnected pad is immediately neutralized at the host. Reconnecting resumes with a full state snapshot, not the previous held trigger.

Haptic/LED feedback includes destination pad slot, originating peer, sequence, intensity and bounded duration; drop stale feedback after 100 ms and cap a single rumble command to 1 second unless refreshed. A guest cannot send feedback to another guest's physical device. Local controllers remain local and are not renumbered merely because a remote viewer joins. The Windows/Linux backend reports real available virtual-controller capacity; macOS gamepad injection remains a separate unsupported/experimental capability until a reviewed usable backend exists. Generic gamepad protocol support does not make the inspected macOS virtual-HID backend support it. [A §§1.5, 3.3.]

Collaborative simultaneous keyboard/mouse control can be added later with per-peer held-state accounting, but is not a default. Merging two users' modifiers can create surprising shortcuts and makes emergency release harder; exclusive leases are the safer, clearer first model. Native touch/pen ownership follows the same grant and contact-generation rules.

### 10.3 Application launching

Local owner chooses launchable applications in the host UI. The remote API carries an opaque app ID and an optional pre-approved profile ID; the host resolves these to an allowlisted executable/bundle/desktop entry and fixed or schema-validated arguments. Use platform launch APIs (`NSWorkspace`, appropriate Windows process/application activation APIs, desktop-entry launch on Linux) with argument arrays. Never concatenate a remote command into a shell. Executable paths, working directory, environment overrides, pre/post scripts and elevated launch are not guest-supplied fields.

The host sends launch-started/running/exited/error state, PID only as local diagnostic metadata, and the expected display/session assignment. A game may take seconds to load; video begins on the desktop while launch progress appears. Launch permission does not grant termination of arbitrary processes. An optional “close app when session ends” policy applies only to the process/session Sunna launched and only with owner opt-in. Owned Linux compositor sessions can contain processes in a user scope/cgroup and terminate that scope; existing desktop sessions cannot assume every child process is safe to kill. [A §§1.7, 3.3, 4.4; F §3c.]

### 10.4 Resolution follow and display leases

Default is **fit view without changing the host display** for physical-monitor sharing, and **follow viewer** for a session-owned virtual display. For follow mode, debounce window resize for 250 ms, require ≥5% geometry change unless an explicit fullscreen/monitor move occurred, and negotiate intended logical scale separately from pixel dimensions. Supported initial bounds: minimum 640×360, maximum 8,192 pixels per axis and 32 megapixels total, with hardware-specific lower limits. Rate is a rational value, allowing 59.94/119.88 Hz; do not truncate all refresh rates to integer 60.

Transaction: propose geometry → host verifies ownership/capacity → apply display mode → wait for real capture dimensions → create/validate new codec config → client prepares decoder → emit recovery frame with new display/codec epoch → client presents and acknowledges → release old surfaces. Keep the old frame visibly scaled during the transition, but never inject using the old coordinate transform into a new display. New pointer input carries the new epoch only after the client has installed the new mapping. Timeout after 3 seconds rolls back the owned mode or reports the failed configuration, without looping forever.

A viewer disconnect retains its virtual display for 30 seconds while the broker/agent remains healthy. An agent crash invokes the platform driver's shorter safety watchdog or supervisor reconciliation, accepting that private macOS virtual displays may vanish and rearrange windows. On final close, restore the prior owned mode only if the current state still matches Sunna's last applied state; user modifications win. Do not repeatedly toggle physical display power to manufacture privacy or responsiveness.

### 10.5 Reconnection and resumption semantics

QUIC path migration that preserves the live authenticated connection normally preserves the session epoch and decoder if reference continuity remains valid. A new transport connection performs fresh peer authentication and grant checks, then may present a 256-bit random resume ticket bound to both device identities, OS user, grant version, session ID and 60-second expiry. Ticket possession never replaces peer-key proof. Revocation invalidates it immediately.

On a full reconnect: create a new session/input generation; release old injected state; flush stale datagrams, audio jitter and pending refinement; resend configuration; prime with a valid recovery point; restart audio sample mapping; keep the selected display lease if still valid. Do not replay old key presses, text commits, wheel deltas or launch requests. Idempotent launch request IDs allow reporting an already completed launch without launching twice. Clipboard offers are refreshed rather than auto-pasted. File transfers may resume verified chunks only while their explicit grant remains valid.

The UI immediately shows connection loss, retains the last image with a dim overlay after 150 ms, and distinguishes reconnecting from a responsive live desktop. Retry with bounded backoff (0, 250 ms, 500 ms, 1 s, 2 s, then 5 s), concurrently exploring allowed paths. Stop automatic retries after 60 seconds and offer a clear Retry action; preserve device selection. No transient path failure revokes permanent pairing, and no permanent revocation is treated as a transient retry condition.

## 11. Observability, benchmark harness and competitive protocol

### 11.1 Stage timestamps and honest overlays

Each capture/encoded/presented identity carries trace context. Instrument these events without allocating a new string on every frame:

```text
CLIENT INPUT CLOCK                 HOST CLOCK
I0 hardware/native event observed  H0 authenticated input received
I1 input scheduled                 H1 injection dispatched/completed
                                   H2 source surface timestamp
                                   H3 capture callback delivered
                                   H4 GPU conversion submitted/completed
                                   H5 encoder submitted
                                   H6 encoded output callback
                                   H7 frame admitted / FEC ready
                                   H8 first/last symbol scheduled
CLIENT CLOCK                       H9 transport send/ACK events
C0 first symbol received
C1 last necessary symbol / FEC reconstruction complete
C2 decoder submitted
C3 decoder callback
C4 GPU render submitted/completed
C5 OS presentation callback / timestamp
P0 physical photon measurement (external common clock only)
```

H1 is not “application processed the input.” H2 is not necessarily the instant every application pixel changed. C5 is not P0. Preserve OS source timestamps even when a static surface is reused, and show content age separately from packet age. The present prototype's wall-clock `now_us`, refreshed timestamps on repeated surfaces, decode-finished estimate, and clamping of negative derived delays cannot establish input-to-photon latency. [Sunna capture/client/stats sources; A §§2.4, 9; C §6.1.]

Clock mapping uses four-timestamp exchanges: client send t1, host receive t2, host send t3, client receive t4. Estimate offset `((t2-t1)+(t3-t4))/2` and network round trip `(t4-t1)-(t3-t2)` after converting to common units. Fit offset plus drift to a sliding set of low-delay samples, reject scheduling outliers, and report uncertainty at least consistent with half the unexplained round trip/asymmetry bound. Sample at 10 Hz for the first 2 seconds, then 1 Hz; reset/freeze mapping on sleep or clock discontinuity. Do not clamp impossible negative stage delays to zero and hide them; mark the estimate unavailable and retain raw measurements. One-way estimates on asymmetric WANs can be wrong even when minimum-RTT fitting looks stable.

The compact overlay shows current path/direct-or-relay, input RTT, estimated capture-to-present with uncertainty, unique-content fps / presented fps / repeated frames, codec/profile/chroma/depth, backing resolution, wire/video/FEC/RTX rates, queue age, and recovery/late-frame counts. Expandable view shows stage p50/p95/p99, actual accepted codec properties, memory/thermal/copy fallbacks, audio jitter/underruns and tile exactness percentage. Default UI uses a simple health indicator; detailed overlay is one toggle. Record histograms over 10-second rolling and whole-session windows, not a single average hiding long stalls.

### 11.2 Audio and audiovisual measurement

Audio has its own sample timeline. The host stamps sample counter and monotonic capture anchor; the client estimates clock drift and uses a small adaptive resampler, initial correction bounded to ±500 ppm with smooth changes, unless device-rate mismatch requires explicit resampling. Jitter depth adapts within §2's bounds; route changes reset the anchor. Target audio capture-to-output p50 ≤30 ms LAN / ≤55 ms W60, p99 ≤50/85 ms, with audiovisual offset within ±20 ms for the test workload. These are **UNVERIFIED targets**, not a sum of nominal Opus frame lengths.

Interactive video is not delayed by a large audio jitter buffer. Where video consistently leads audio, reduce audio buffering or apply only a bounded ≤10 ms video alignment in Smooth mode; show the tradeoff. For desktop/game mode, responsiveness takes precedence over perfect lip sync during a transient stall. Measure with a host-generated simultaneous flash/chirp, an audio loopback/capture device, and the optical rig. Run 30-minute drift/device-change tests and count audible concealment/underruns. A nominal 48 kHz label does not prove two physical audio devices share a clock.

### 11.3 Harnesses and reproducible artifacts

Build three complementary harnesses:

1. **Deterministic virtual-time simulator:** fake capture/encoder/decoder/display, exact queue ownership, synthetic dependency graphs, path changes, packet loss/reorder/duplication, and clock skew. It verifies invariants and controller stability without expensive hardware or wall-clock sleeps.
2. **Software/GPU workload harness:** a deterministic native host app draws frame counters, moving bars/checkerboards, code editor pages, colored small text, terminal scroll, browser-like mixed content, photographs, video motion and HDR patches where supported. Capture source canonical rasters and known content IDs. Client records matched decoded/composited frames and stage traces under an explicit test-only content-capture mode.
3. **Physical input-to-photon rig:** a USB HID microcontroller generates an input with a common-clock trigger; a photodiode on a fixed client-screen region records the host application's flash response. Sample at ≥10 kHz or use an equivalent calibrated acquisition device. A 240 fps camera gives ~4.17 ms frame quantization and cannot alone substantiate a 2 ms advantage. Use high-speed video for visual diagnosis and verify optical thresholds/scanout position.

Place the optical patch at a fixed normalized screen position, record brightness/HDR settings and panel refresh, randomize event timing relative to refresh, and calibrate trigger/display detection. Repeat a local baseline on the same host/application where geometry permits. Physical versus virtual display paths differ; do not subtract an unmatched physical-panel baseline from a virtual-display stream and call the result exact transport overhead. Primary competitive claims use the directly measured end-to-end response under matched remote geometry.

Each run emits a manifest with build/source revision, executable hashes, OS/driver/GPU/encoder/decoder versions, network shaping seed, display EDID/mode/scale, accepted settings, thermal/power state and test scenario; time-series JSONL/Parquet or equivalent; histograms; raw optical trace; selected matched image crops; and a generated report. Save configurations and failures, not only winning screenshots. The harness must work without instrumenting a closed-source competitor internally.

### 11.4 Network simulation matrix

Use a dedicated Linux bridge/router or controlled UDP forwarder between the actual Macs; `tc netem`/traffic shaping and a scripted NAT/firewall lab reproduce scenarios. Avoid shaping the same traffic twice through Tailscale and a second unrecorded tunnel. Run direct LAN, direct public/IPv6, Tailscale, UDP relay, and TCP relay as distinct paths. The Linux VM is useful for emulation and rendezvous tests but its virtual GPU is not the Mac media engine.

| Axis | Required cases |
|---|---|
| RTT | 2, 15, 30, 60, 120 ms; asymmetric 5/45 ms one-way case |
| Capacity | 2, 5, 10, 20, 50, 100 Mbit/s; abrupt 50→10→30 steps and competing bulk traffic |
| Random loss | 0, 0.1, 0.5, 1, 3, 5% |
| Bursts | Gilbert–Elliott traces plus explicit 3/8/20 packet drops, 20/50/100 ms outage |
| Reordering/duplication | 1/5/20 packets, 0–10 ms reorder, 0.1% duplicate, repeated RTX |
| MTU | 1280/1400/1500 underlying IP, tunnel overhead, PMTU black hole and migration to smaller path |
| NAT/firewall | Same LAN, hairpin, two ordinary NATs, endpoint-dependent mapping/filtering, CGNAT, IPv6-only/NAT64 where supported, UDP blocked, TCP/443 proxy |
| Roaming | Wi-Fi↔Ethernet, Wi-Fi↔mobile, address/port rebind, relay failure/region change |
| Endpoints | Sleep/wake, lock/login/logout, permission revoked, app/agent crash, encoder timeout, display hotplug, audio-device change |

The full Cartesian product is unnecessary. Use pairwise coverage plus targeted worst cases, deterministic regression seeds, and a small nightly stress set. Congestion tests include competing ordinary TCP and QUIC flows, not just an empty link. Exit requires no persistent bandwidth hogging, no positive-feedback FEC congestion spiral, bounded queues, and recovery without input stuck-down state. Verify fairness quantitatively for equal-RTT bulk competitors and document deliberate low-delay tradeoffs; do not assert a new controller is fair because its mean throughput looks good.

### 11.5 Competitive test protocol

Pin currently tested versions of Parsec, Sunshine/Moonlight, applicable Apollo, RustDesk and Apple Screen Sharing; record edition and settings. Apple's Standard/VNC and High Performance are separate entries. Run both M1→M2 and M2→M1. Repeat after 30 minutes of sustained load on AC and on battery, noting fanless thermal throttling, display brightness, power mode and background tasks. Test physical and matched virtual display modes separately. [A §§8–9; C §5.]

For each primary latency comparison, collect at least 10,000 triggered response events per condition across at least three independent runs, with randomized A/B order and warmup excluded under a declared rule. Use block bootstrap intervals to account for temporal correlation, especially p99. Long freezes/timeouts remain in the result as censored failures or explicit freeze-duration metrics; do not drop them as “bad samples.” Publish p50/p90/p95/p99, maximum freeze, stutter events/minute, unique-content fps and failure count. A claimed 2 ms win requires the measurement uncertainty and confidence interval to support it.

For quality, replay the same deterministic corpus at the same total wire budget, including repair/relay overhead. Align by embedded source-frame IDs before image comparison. Evaluate text at native scale and multiple font sizes, colored text/subpixel-sensitive patterns, scrolling text, motion/video, gradients and photographs. Report OCR character error, edge/chroma reconstruction error, appropriate SSIM/PSNR/perceptual metrics, lossless tile equality, time-to-sharp and blinded paired viewing. Do not allow a lower source fps to masquerade as higher per-frame quality at equal bandwidth. Closed-source products without exact bitrate enforcement are rate-limited at the network boundary and their actual wire rates reported.

Connection reliability uses at least 1,000 automated attempts across a declared weighted topology matrix for an initial gate, then a larger beta fleet sample before publishing a broad 99.5% claim. A thousand attempts is still a confidence-limited estimate; show binomial intervals and per-topology failures. Measure cold/warm launch, cached/uncached path, relay needed/not needed, permission-ready/not-ready and first pairing separately. TTFF ends at first presented useful frame, and permission-denied/host-offline cases remain visible in the overall funnel.

Usability study: at least ten participants unfamiliar with the setup, randomized competitor order, identical task cards, signed clean installs/permission state, and no coaching unless counted as assistance. Count actions on both devices, time, permission failures, wrong-device approvals and successful unattended reconnection. The target is equal or better than Parsec in completion time and errors, not merely fewer product buttons. Dossier step estimates are not substituted for this experiment. Performance regressions block a claim even if the architecture is elegant.

## 12. Engineering, FFI, testing, CI and packaging

### 12.1 Languages and UI decision

Use Rust for protocol parsing, transport/media policy, security/session state, cross-platform host/client composition, metrics and the simulator. Keep ownership and queue admission in Rust. Use the tested stable compiler required by pinned dependencies; the inspected iroh snapshot's MSRV is a compatibility constraint, not a reason to float toolchains automatically. Pin Rust, SDK, Swift/Xcode, Windows SDK and native-library versions in reproducible build manifests. [F §3b.1.]

Use Swift for compact ScreenCaptureKit/CoreAudio/AppKit lifecycle bridges where SDK availability and delegate semantics are clearer than large handwritten Rust Objective-C bindings. Expose a narrow C ABI: create/configure/start/stop, retained opaque handles, explicit callbacks and error/status structs. Objective-C is justified for the runtime-checked private virtual-display/cursor shims and selected event APIs. Metal shaders implement conversion, damage comparison, compositing and color. On Windows prefer Rust `windows` bindings for COM/D3D/Win32; a C++ shim is justified for vendor SDKs that require C++ object interfaces, notably AMF, or carefully isolated native interoperability. Linux uses Rust with C ABI bindings for PipeWire/libei/libva/FFmpeg and small adapter shims as needed.

**Choose Tauri 2 for desktop application chrome and native platform rendering windows for streaming.** This follows the useful separation in Sunna research/06 without putting the hot path in a webview. Tauri handles device lists, settings, onboarding and diagnostics; the viewer toolbar and raw input remain in the native streaming window. The broker exposes typed narrow commands, not arbitrary filesystem/process APIs. Disable unused webview capabilities, remote navigation and untrusted HTML; use a strict content-security policy. Display peer-provided names as text. The webview cannot call privileged helper methods directly.

Alternatives rejected: three complete SwiftUI/WinUI/GTK applications would multiply UI work for one developer; a single web-rendered video surface complicates color/input/presentation guarantees; Qt/SDL remain viable tools but do not justify rewriting the current Rust policy around another framework. Tauri's Linux WebKit/runtime packaging and platform look are real costs, tracked in the Linux milestone. If its shell becomes the dominant accessibility/packaging obstacle, replace the shell without changing native session/media interfaces.

### 12.2 FFI and ownership rules

Every opaque surface/session handle has one documented owner and one destructor. A retained handle crossing FFI explicitly transfers or borrows ownership; no undocumented “the callback probably keeps it alive.” Mark thread affinity in Rust types and assert executor identity in debug builds. Do not mark raw platform pointers `Send`/`Sync` merely to satisfy a channel. GPU resources carry completion fences; no release until the consumer's callback/fence fires. Cancellation invalidates a generation and waits for bounded callbacks or safely quarantines the session before destruction.

No Rust panic or C++ exception crosses C/Objective-C/Swift boundaries. Callbacks validate pointers/lengths, return structured errors, and cannot reenter arbitrary session mutations. Use generated bindings for fixed signatures, especially non-variadic scroll APIs; test ABI size/alignment in native compilation. Swift async tasks report completion to Rust actors rather than blocking a Tokio worker. Encoded buffers are immutable refcounted slabs; duplicate viewers can share payload bytes in-process while each transport gets independent encryption and pacing.

Keep unsafe code in platform/codec/FFI modules with safety comments about lifetime, thread and buffer bounds. Protocol/session/quality policy should remain safe Rust. Vendor decoders are security-sensitive even when wrapped safely; place them in an unprivileged viewer process and limit negotiated resources before calling them. Root helpers never load codec libraries.

### 12.3 Testing that exercises actual invariants

Unit/property tests cover: byte layouts/endian/overflow; FEC golden vectors and all recoverable erasure patterns for small groups; adversarial lengths/group conflicts; frame dependency/recovery transitions; input exactly-once transitions and cumulative motion; lease expiry/release; tile invalidation across skipped/lost/reordered frames; color/range conversions; pairing/grant/revocation state; and cancellation-safe framed reads. These are meaningful behavioral tests, not assertions that a struct field equals the value just assigned.

Fuzz every unauthenticated/control/media parser, codec-config parser and compressed tile boundary. Bound time, memory and decompressor expansion. Run Miri/sanitizers where applicable to unsafe abstractions, native AddressSanitizer/ThreadSanitizer lanes where supported, and concurrency/model tests for mailbox replacement, shutdown and surface lifetime. Simulate encoder callback-after-cancel, full queues, disconnection during a partial control record, malformed cursor sizes, repeated scene cuts, and every state-machine transition interrupted by sleep or revocation.

Hardware-in-loop on the owner's Macs:

* A signed host/client test bundle, stable TCC identities, one-time local setup, and a local control API restricted to the test account.
* Both directions, 1080p and native backing pixels, H.264/HEVC, physical/virtual display, cursor/scroll/keyboard layouts, static/motion transitions, audio/clipboard, sleep/lock/revoke/relaunch, thirty-minute thermal runs.
* Bitstream probes for actual chroma/depth, hardware use, frame reorder, dropped outputs, LTR acknowledgement/refresh and parameter-set changes. Failing a probe removes the capability.
* Nightly short non-disruptive media runs; reboot/lock/login/TCC tests in explicitly scheduled local test windows, not while the owner is using the computer. An AI agent does not silently reboot the owner's Mac to satisfy CI.

Linux VM covers build, protocol, portal software fallback, service packaging and network simulation. Add real GNOME/KDE Wayland GPU machines for zero-copy/VAAPI/NVENC claims. Windows needs a real NVIDIA-capable Windows 11 host and suitable client/display; Intel/AMD hardware is required before those vendor profiles are marketed. Borrow/rent compatible systems where possible, but virtual/cloud GPU access must be recorded and not treated as identical to a local consumer setup. A 120 Hz external display and optical rig are explicit prerequisites for G120 claims.

### 12.4 CI and resource contracts

PR checks: formatting/lints, pure-Rust unit/property tests, protocol golden vectors, simulator regression seeds, Linux build, macOS/Windows cross-platform compile lanes, dependency/license review and unsafe-code review for affected modules. Native SDK tests run on their OS; cross-compilation alone is not validation of TCC, D3D surfaces or EIS behavior. Nightly: fuzz budget, sustained simulated congestion, selected hardware benchmarks and package-install smoke tests. Release: signed-package install/update/rollback/uninstall on clean machines, full primary hardware matrix, relay outage drill, authentication/helper review and reproducible artifact manifest.

The resource contract applies to hidden adapters too. Add inventory entries beyond §2: transport incoming DATAGRAM buffer 256 KiB with immediate drain; reliable receive credit 1 MiB/connection initially, per-control stream 64 KiB, at most 16 accepted streams; stream-write staging ≤32 KiB total and further limited by the scheduler's age/rate budget; frame/reference ledger 512 records or 2 seconds; clock-fit history 128 samples; tile generation maps capped by negotiated tile count; UI status channel 32 replaceable updates at ≤10 Hz; blocking filesystem/keystore worker queue 16 operations; native outstanding callbacks capped by the admitted surface/session counts. QUIC internal retransmission, handshake and ACK history are bounded by transport flow/congestion limits and library safeguards, instrumented where exposed; Sunna does not falsely claim ownership of the OS compositor's queue.

At startup, a debug/test resource registry records every configured queue and its owner/drop policy. Reject new streams or reduce negotiated resources before aggregate limits are exceeded. No production `unbounded_channel` in a media/control/input path. A ring can overwrite expendable telemetry; it cannot silently overwrite a permission change or key-up event. Metric detail can drop under load, but dropped-metric counters remain visible.

### 12.5 Packaging and operational maintenance

macOS: universal package only after both architectures are supported; first release is Apple Silicon Developer ID signed, notarized and stapled, with nested helpers correctly signed and stable bundle IDs. Ship a drag-install app for attended use and a clearly authorized helper installation for unattended features. Do not use ad-hoc builds to infer production permission persistence. Private virtual-display/cursor APIs are isolated, runtime-disabled on unsupported systems, and covered by a physical-display fallback. App Store hosting is not the initial distribution strategy; distribution requirements and API policy must be reviewed before any store submission. [F §7.1; C §3.]

Windows: signed installer/app/service, per-user attended mode where possible, optional machine service and separately signed virtual-device drivers, uninstall that removes only Sunna-owned firewall/service/driver state. Code signing improves provenance; do not promise a certificate automatically eliminates reputation prompts. Linux: signed repository packages for supported distributions, user service/desktop integration, and a portal-only sandboxed package when practical; privileged helpers cannot be assumed to work from every sandbox/AppImage. Self-host services have pinned container digests, non-root defaults, health checks, resource quotas and documented upgrade/rollback.

Dependencies remain separately licensed; review actual pinned licenses before copying code, linking, or redistributing a driver. Dossier descriptions of GPL/AGPL projects are research, not permission to copy their implementation into a differently licensed product. Prefer permissive protocol/library reuse and implement Sunna-specific policy independently; retain notices and source obligations for whatever components are actually shipped. Codec patent/distribution obligations and private-API acceptance are external release risks, not resolved by choosing Rust. Maintain an SBOM and a small dependency budget; do not bundle an entire general-purpose transcoding stack just for one convenience conversion.

Crash handling restarts a failed user agent with exponential backoff (maximum three restarts in five minutes), releases input through the surviving supervisor/OS cleanup path, reconciles display ownership, and tells the viewer the real failure. A crash dump is opt-in and redacted; full memory dumps may contain screens/keys and are not silently uploaded. Maintain protocol compatibility for the current and previous supported minor release, with explicit minimum secure version enforcement only for documented vulnerabilities. Never downgrade to unauthenticated protocol v0 to make an old client connect.

## 13. Roadmap from this working tree to the competitive release

### 13.1 What already exists, and what actually needs replacing

Do not start by rewriting the whole workspace. The current prototype has useful ownership and correctness work, but its timing/security/media contracts are still a prototype. This inventory refers to the inspected working tree, not only its README or older research plan.

| Area | Current evidence | Next architectural change |
|---|---|---|
| Mac capture | Retained IOSurface handles with CF/IOSurface lifetime management; native backing-pixel sizing; latest-surface behavior. [capture/macOS](../../crates/capture/src/macos.rs:156) | Preserve lifetime work; replace CGDisplayStream with SCK, use source monotonic timestamps, cancellation-safe stop, honest idle/repeat semantics |
| Encode | VT low-delay request, async callback internally but synchronous per-frame completion; `Option` output handles normal encoder drops. [VT adapter](../../crates/codec/src/videotoolbox/mod.rs:324) | Async submission contract, accepted-property report, hardware proof, no per-frame `CompleteFrames`, LTR/dependency metadata |
| Decode/viewer | Decoder copies BGRA to CPU; softbuffer scales/presents. [decoder callback](../../crates/codec/src/videotoolbox/mod.rs:445), [viewer](../../apps/sunna-cli/src/viewer.rs) | Retained NV12/P010 GPU surfaces, Metal presentation, resize/config-aware decoder recreation, present timestamps |
| Sender admission | Already skips capture before encoding when backlog is high; separates wire IDs from capture IDs; burns an ID and forces recovery after an encoded whole-frame drop. [host media loop](../../crates/host/src/lib.rs:162) | Replace 300 KiB threshold with rate/age budget; handle partial-send failures immediately; bounded packet scheduler and repair ledger |
| Framed control | Dedicated split reader already owns partial records, addressing cancellation-related framing corruption. [client](../../crates/client/src/lib.rs:156), [transport](../../crates/transport/src/lib.rs:251) | Keep ownership pattern; replace unbounded queues and add authenticated, versioned bounded schemas |
| Media/FEC | Actual media header is 25 bytes, not the older 21-byte description; XOR groups and one active partial frame. [media protocol](../../crates/proto/src/media.rs) | New v1 protocol, reorder-aware multi-frame assembly, exact RS k/r, deadline feedback/RTX/reference repair |
| Congestion | One-second receiver reports/AIMD plus a sender-local rate cut; default transport CC and 1 MiB send staging. [host control](../../crates/host/src/lib.rs:301), [transport config](../../crates/transport/src/lib.rs:62) | Fine-grained observations, tiny staging, shared rate budget, bounded pacing; custom controller only after simulation evidence |
| Input | Fixed-arity scroll API and held-key release on session exit already present; basic native coordinate setup. [input/macOS](../../crates/input/src/macos.rs), [host shutdown](../../crates/host/src/lib.rs:135) | HID usages, explicit modifiers, true relative mode, phase/momentum, all-button state, focus/lease release, multi-display transforms |
| Trust | Loopback bench can trust its generated certificate, but `view`/`connect` call `connect_insecure`; no host peer authorization. [CLI](../../apps/sunna-cli/src/main.rs:95) | Persistent device identity, authenticated pairing/grants, no internet-exposed insecure compatibility path |
| Lifecycle | Host accepts/serves sessions serially; blocking capture/join can prevent clean shutdown; CLI exits process on session end. [host](../../crates/host/src/lib.rs:51), [CLI](../../apps/sunna-cli/src/main.rs:124) | Session actors, cancellable capture, error return/UI recovery, multi-viewer resource admission |
| Platforms/features | Linux/Windows capture placeholders; no production audio, clipboard, NAT, virtual-display service or installer | Add capabilities in the order below; do not infer platform support from trait stubs |

### 13.2 Delivery assumptions and sequencing

Assume one developer owns architecture, integration, releases and physical testing. AI agents can implement bounded modules, generate fixtures, inspect traces, compare source/API changes and review diffs; they do not remove hardware availability, code-signing, driver approval, entitlement review or human usability work. Limit work in progress to one main integration milestone plus one isolated research/probe task. Do not ask agents to independently rewrite host, client and protocol contracts at once.

The schedule below is an **UNVERIFIED effort estimate** in focused developer weeks, not a promise. The broad product is roughly 18–30 calendar months once maintenance, hardware procurement, security review and release support are included. A useful Mac product ships much earlier. If the measured superiority gates fail, optimize the evidenced bottleneck or narrow the claim; completion of a milestone checklist does not itself prove “beats all of them.”

| Milestone / effort | Concrete deliverable | Exit criteria and who can use it |
|---|---|---|
| **M0 — trustworthy baseline and probes, 2–3 weeks** | Preserve current working tree; reproducible benchmark artifact format; source/present tracing skeleton; device identity and direct-address iroh adapter spike; remove insecure normal connect; protocol/resource limits and cancellation fixes | Signed/manual-paired direct connection on both Macs; unauthorized peer cannot view/control; malformed packets bounded; current baseline measured both directions; iroh/Noq controller/staging feasibility recorded. Developer-only, no speed claims |
| **M1 — native Mac media path, 4–6 weeks** | SCK, async VT, Metal decode/presentation, clock semantics, capability probe, exact display transform, permission cards | 1080p60 and native backing pixels run 30 minutes without unbounded memory; no full-frame CPU readback in ordinary video; source/present trace valid; L60 provisional p50 ≤40 ms/p99 ≤80 ms or a measured bottleneck report; source stop and sleep cancellation bounded. Signed technical preview |
| **M2 — media correctness and repair, 4–6 weeks** | Protocol v1, bounded reorder assembler, exact RS, adaptive RTX, VT LTR where verified, recovery ledger, pacer, rate/age admission, input leases | Loss/reorder/partial-send simulations pass; 0.1–1% WAN loss does not trigger continual IDRs; no stuck input across 1,000 disconnect/lease tests; queues satisfy bounds; H.264 recovery works when LTR is disabled too. Owner can test over existing Tailscale/direct paths |
| **M3 — useful Mac desktop alpha, 5–7 weeks** | Exact static tiles with invalidation, audio, plain-text clipboard, physical/layout/scroll fidelity, private virtual-display adapter, native window UX, basic signed update path | Tile bytes exact after settle; no stale overlays in adversarial tests; audio/drift targets approached and failures documented; first-session flow usable; native-resolution desktop retains readability. First invited external users, authenticated Mac↔Mac, awake signed-in hosts |
| **M4 — easy internet Mac beta, 6–8 weeks** | Hosted/self-hostable rendezvous and TCP relay, direct upgrade, QR/full links, optional account/device list, revocation/audit, diagnostics, install/update/uninstall; short code only after review | ≥99% successful permission-ready connects in beta lab matrix of ≥500 attempts, with failures published; W60 TTFF p50 ≤1 s/p99 ≤3 s when path permits; ≥90% unassisted setup in initial study; external security review of pairing/parser/helper boundaries. Public Mac beta; no universal performance claim |
| **M5 — Linux existing-desktop release, 6–10 weeks** | GNOME/KDE portal+PipeWire+EIS host, Linux hardware client, VAAPI/NVENC SDR adapters, user-service packaging, software VM tier | Supported GNOME/KDE hardware matrix passes capture/control/clipboard capability behavior; restore token rotation correct; DMA-BUF/copy fallbacks measured; Mac↔Linux and Linux↔Linux benchmarked; clear lock/login limitations. Linux users join beta |
| **M6 — WAN performance and relay repair, 4–8 weeks** | Tested media congestion controller if it wins, minimal Noq scheduling patch if necessary, UDP forwarding relay extension, migration/fairness/MTU hardening | W60/H60 stage and recovery gates; no persistent bufferbloat/FEC spiral; UDP relay improves relevant hard-NAT p99 versus TCP fallback; fallback still works with all UDP blocked; 24-hour mixed-path soak and relay outage drill. Roll out by capability/telemetry cohort |
| **M7 — Windows host/client and gaming foundation, 8–12 weeks** | DDA/WGC, D3D decode/presentation, direct NVENC/RFI, WASAPI, raw input, optional signed virtual display/gamepad components, safe user-session service | 1080p60/120 hardware runs, RFI fault injection passes, protected-screen behavior explicit, virtual-display crash cleanup verified, four-pad co-play works on supported games. Windows preview first; full gaming label only after pad/display gates |
| **M8 — advanced quality and sessions, 6–10 weeks** | Two-monitor policy, controller/spectator renditions, HDR HEVC, verified native 4:4:4 where available; dual-stream chroma only if probe/corpus proves value | No controller latency regression from spectator; two-display pixel/coordinate correctness; HDR color/metadata and SDR fallback charts pass; 30-minute thermals; G120 final target tested on suitable hardware. Feature-specific stable releases |
| **M9 — additional clients, 16–22 weeks total** | iPad/iPhone client first, Android next, browser WebRTC guest client last | Native mobile input/audio/power/roaming tests; browser permissions/DTLS identity binding and origin trust disclosed; no unsupported claims of native refinement or shortcut equivalence. Each platform ships separately |
| **M10 — competitive release gate, 5–8 weeks plus unresolved optimization** | Full reproducible competitor matrix, independent usability/security review, operational hardening, published capability/performance report | §1 absolute and relative claims supported by §11 evidence; ≥99.5% connection goal supported with intervals/fleet data; no unresolved critical trust/input/data-loss issues. Only achieved cells are marketed as superior |

M0's direct iroh adapter deliberately avoids building a temporary custom internet puncher. M4 adds discovery/relay/productization around that adapter. The first controller can remain the library default through M3 if application pacing meets the alpha's stability goals. M6 replaces it only when there is measured benefit. The order prioritizes Mac quality and Linux usefulness while making room for eventual Windows gaming; Windows driver/signing feasibility should be investigated early as a bounded side task because external approval can take calendar time.

### 13.3 What ships first, and what is deferred

First useful external alpha: signed, authenticated Mac↔Mac SDR, one selected display, native backing resolution, responsive video plus exact static tiles, system audio, plain-text clipboard, good keyboard/mouse/trackpad scrolling, honest diagnostics and clean shutdown. Existing LAN/Tailscale/direct routing is acceptable for this explicitly invited alpha; it is not the final Parsec-ease story. Public Mac beta adds automatic internet discovery/relay and one-selection routine access. Ship view/control of an awake signed-in host before claiming reliable pre-login/unattended access across every OS release.

Defer: browser hosting, mobile hosting, arbitrary remote shell, full enterprise fleet policy, a Linux compositor as the primary host, multi-user Windows desktop virtualization, Mac private multitouch promises, microphone virtual devices, HDR lossless tiles, universal 4:4:4, arbitrary app streaming/window redirection, surround-sound permutations, 8+ co-play users, recording, and every codec/vendor combination. These may be useful later; none is needed to validate the central quality/latency/connectivity design. The first macOS virtual display is optional because its private API is a real risk; physical display sharing remains a complete useful product.

Stop/go gates are concrete. If Metal presentation removes little latency, do not keep polishing the renderer while network queues dominate. If HEVC LTR fails on an OS version, ship H.264 repair rather than guess. If dual-stream chroma costs more than it improves moving text, retain static refinement and investigate native profiles. If iroh cannot meet send-age limits with a small patch, revisit the transport adapter with measured traces, not a speculative rewrite. If the owner cannot obtain Windows/Linux GPU hardware, those performance claims remain unverified and their release is limited accordingly.

### 13.4 Operating cost and solo-developer sustainability

At a sustained 20 Mbit/s, one relayed viewer consumes about 9 GB of relay egress per hour, before additional overhead. That is arithmetic, not a current cloud price quote. Direct connectivity and idle desktop refinement therefore materially affect operating cost. Measure relay fraction, bytes/session, regional occupancy and failure rate from beta; set transparent hosted relay quotas rather than surprise users with degraded quality. Self-hosting and existing VPN routes remain available.

Start with two regions, automated health checks and a documented outage path; do not advertise an enterprise availability SLA without staffing and tested operations. Security updates, OS beta compatibility checks, signing renewals and driver maintenance are recurring work. Reserve roughly 25–30% of post-beta capacity for maintenance/support rather than scheduling every week for a new platform. AI assistance is most valuable in deterministic simulator cases, parser fuzz harnesses, API-change reviews and report generation; performance judgment and physical acceptance remain evidence-driven human work.

## 14. Risks, open questions, disagreements and decision log

### 14.1 Risk register with evidence and an exit experiment

| Risk / uncertainty | Consequence | Decision, experiment or stop condition |
|---|---|---|
| Public Apple Silicon hardware 4:4:4 is **UNVERIFIED** | Moving colored text may remain below Apple's best path | Probe supported public properties and SPS/chroma/hardware status on both Airs; baseline exact tiles does not depend on success. No private profile becomes mandatory. [C §§3.2, 5.2] |
| VT LTR/HEVC low-delay behavior varies by OS | Repair may fail or latency rise silently | Encode/decode fault-injection matrix per OS/config; advertise only proven repair; IDR fallback remains correct. [C §3.2] |
| Custom congestion controller is unproven | Unfairness, oscillation, worse tail latency | Keep library baseline until simulator, competing-flow and hardware A/B gates pass; own rate policy must not bypass transport safety. [A §7.1] |
| Iroh/Noq scheduler and custom-transport integration | Fork churn, hidden relay queues | Early two-week spike, tiny patch budget, pinned graph, queue telemetry; reopen adapter choice only with a concrete failing requirement. [F §3b; local source] |
| TCP-only paths | Outer HOL can prevent gaming latency targets | Explicit compatibility tier, bounded offered load, UDP relay where usable, no universal WAN p99 promise. [F §§3.8–3.10] |
| SCK/TCC periodic consent and entitlement approval | Unattended host may require local intervention | Signed long-idle/reboot tests, apply for entitlement, document unsupported states; never suppress OS consent through private database edits. [C §3.3] |
| Private Mac virtual display/cursor APIs | OS update breaks separate display/cursor | Isolated runtime shim, OS beta smoke tests, physical-display/embedded-cursor fallback. [C §§3.5–3.6] |
| macOS LoginWindow/key storage service arrangement | Privilege mistakes or inability to unlock after reboot | Dedicated signed test accounts and explicit local approval; keep network broker unprivileged; FileVault pre-boot unsupported. [C §3.4] |
| Full-frame memory bandwidth and fanless Air thermals | Latency/power degrade after short demo | 30-minute AC/battery runs, tile readback budget, low-priority refinement, no false “zero-copy means zero cost” claim. [A §§1.2, 2.2] |
| Lossless refinement consistency | Stale UI text/buttons, serious usability error | Property-test cumulative invalidation, generations, skipped/reordered frames, resize and late patches; clear overlays on uncertainty |
| Static quality entropy lower bound | Impossible time-to-sharp claims on photographs | Declare compressed-byte/workload conditions and measure remaining bytes; keep instant video preview |
| Linux compositor/portal diversity | “Linux supported” becomes misleading | Named GNOME/KDE/version matrix first, capability-based fallbacks, no private Mutter dependency for normal setup. [C §4] |
| Virtual-device/driver distribution on Windows | Co-play/display feature delayed | Early license/signing/maintenance evaluation, optional driver packages, hardware gate; do not count generic protocol support as working gamepad injection. [A §§1.5, 3.2] |
| Pen/gesture injection limits | Remote device does not feel native in every app | Capability negotiation and semantic mappings; no fake raw multitouch guarantee. [C §3.6; research/05] |
| Browser-origin trust and buffering | Different security/performance envelope | Native reference client, signed fingerprint binding, browser trust disclosure and separate benchmarks. [A §§6–7.2] |
| Shared encoders across lossy viewers | One spectator degrades controller | At most two renditions initially, shared-reference intersection, detach/throttle slow spectator, controller-first aggregate allocation |
| Hardware decoder/parser vulnerabilities | Remote process compromise | Authenticate before media, strict limits, fuzz own parsers, unprivileged decoder process, current vendor/OS updates |
| Relay abuse and hosted cost | Open proxy/reflection or unsustainable bill | Authenticated allocations, return-path proof, quotas, short leases, bounded queues, observed direct/relay economics |
| Key loss/recovery complexity | Account reset becomes a trust backdoor | Host-authoritative grants, explicit recovery device/key, no server-issued replacement peer identity |
| Scope for one developer | A polished Mac product never ships | First useful desktop milestone, WIP limit, staged platforms, maintenance budget, defer low-value breadth |
| Insufficient benchmark rigor | “Faster” claim measures the wrong thing | Optical input-to-photon, actual unique frames, wire budgets, confidence intervals, reproducible defaults/tuned comparisons. [A §9] |

### 14.2 Open questions with a default decision

1. **Can VT expose useful 4:4:4 through supported APIs on the two Airs?** Default no dependency; run the probe in M0/M1, retain results by OS. Reject undocumented constants as the sole production route.
2. **Does HEVC meet H.264's low-delay/repair budget on each Mac direction?** Default H.264; promote HEVC per capability profile after measured A/B. The decision is per hardware/OS, not a universal codec preference.
3. **Can application pacing avoid a Noq scheduler fork?** Default tiny application queues and conservative transport CC; measure send age with saturated refinement/control. Patch only the missing behavior described in §5.8.
4. **Can the UDP relay extension integrate cleanly with current iroh path selection?** Default TCP compatibility relay plus direct paths for beta; build the UDP extension only after authenticated migration/congestion tests. Its unstable API is acknowledged up front.
5. **Which Mac unattended states are supportable with the granted entitlements?** Default awake signed-in access; expand support state-by-state after signed reboot/lock/idle tests. Never imply FileVault pre-boot support.
6. **Is dual-stream chroma worth its sessions/power/complexity?** Default defer. The gate is visibly better moving text at the same total bitrate while retaining latency/thermal budgets, not merely reconstructing a 4:4:4 format.
7. **Which Windows gamepad/display driver can Sunna responsibly distribute long term?** Default optional driver milestone with an explicit maintenance/signing owner. A source repository existing today is not that commitment.
8. **What performance advantage is actually available over Parsec/Apple on these Macs?** Unknown until the physical rig runs. Keep absolute usefulness gates and publish narrower wins if relative margins are not achieved.

These are bounded empirical questions with safe defaults. They are not reasons to postpone the whole architecture or send the owner a menu of unresolved choices.

### 14.3 Explicit disagreements and evidence corrections

| Source position | This proposal's position and reason |
|---|---|
| F §3b.4 favors owning the native traversal socket around upstream Quinn | Reuse iroh/Noq first. The single-developer constraint and authenticated migration/relay state outweigh the appeal of an initially smaller custom socket. Keep a replaceable adapter and measure actual limitations |
| F's relay recommendation can be read as UDP/QUIC relay solving UDP-blocked networks | UDP forwarding helps hard NAT only when UDP works. TCP/443 fallback remains necessary for the blocked-UDP case, with acknowledged HOL latency |
| F §6.2 suggests an active-service MITM weakness for Parsec beyond public details | Public encryption statements do not establish the complete key exchange. Treat Parsec's exact service trust boundary as unknown; design Sunna's verifiable pins without inventing a competitor vulnerability |
| C §3.2 infers Apple's observed HEVC 4:4:4 proves publicly accessible media-engine capability | The observation establishes the emitted stream, not a public VT hardware route available to Sunna. Probe hardware/profile behavior; do not make baseline quality depend on that inference |
| C §3.4 proposes a root daemon holding the listener/pairing keys | Use an unprivileged broker and minimal privileged supervisor. Root is reserved for the OS-specific lifecycle/handle operation that actually needs it |
| C §3.7 suggests silent RMS can detect denied audio capture | Silence is also legitimate content. Use actual error/permission evidence where available and a user-triggered test; label uncertainty instead of diagnosing denial from zeros |
| C §3.2 recommends a roughly one-frame VT `DataRateLimits` bound and `MaxFrameDelayCount=0` | Treat support/semantics as probes. Use accepted 0/1 delay settings plus measured age; a rate property is not proof of a one-frame hardware queue |
| C §6's presentation terminology can imply `presentedTime` is true end-to-end latency | It is an OS presentation observation. Input-to-photon needs the external optical measurement, and host application response is a separate stage |
| Apple strip/slice observations invite assuming incremental encoding/decoding | Bitstream slices/strips do not prove public APIs deliver partial frames early. Do not count a strip latency benefit without measured callback and decoder behavior |
| Early research/02–03 describes Tailscale birthday punching, generic headless KMS, fixed WebRTC latency tax, or effectively zero queues | The newer source dossiers qualify or contradict these. Use actual traversal code, active-output requirements, measured browser/native buffering and explicit bounded queues |
| research/07 leaves stronger auth toward the end of private iteration | Authenticated identity and resource bounds precede external distribution, including “only over Tailscale.” An encrypted overlay is not application peer authorization |
| research/07 lists several code fixes as future work | The current dirty tree already contains retained IOSurface capture, encoder-drop handling, decoupled wire IDs, pre-encode admission, split control-reader ownership, fixed-arity scroll and release-on-exit. Preserve and extend them |
| A/C list specific competitor encoder configurations worth stealing | Use them as tested starting hypotheses, not universal defaults. In particular P1 versus P3, two-pass, reference counts, slices and quality floors need Sunna's own latency/quality evidence |

Where F and C differ on Linux login-screen/Wayland support, the likely scopes and snapshots differ; the proposal deliberately avoids resolving that disagreement by pretending one universal behavior. It specifies Sunna's own supported compositor/session states and tests them.

### 14.4 Decision log

| ID | Decision | Alternatives rejected and reason | Reopen only when |
|---|---|---|---|
| D01 | Native remote computer, Mac/Linux first, Windows gaming third | Gaming-only stream loses desktop text/usability; remote-admin-only product misses responsiveness/gamepads | Owner changes product scope |
| D02 | Hardware video plus exact static tiles | Video-only leaves chroma/text gaps; full RDP/drawing-order clone is too broad and platform-dependent | A verified native lossless/4:4:4 path beats the combined system without its cost |
| D03 | SCK macOS 14.4+ baseline | CGDisplayStream carries lifecycle/virtual-display compatibility debt | Supported OS requirements change and a better public API exists |
| D04 | Native GPU decode/presentation per OS | CPU BGRA/softbuffer and webview rendering prevent the target path | Measured simpler backend meets all color/input/latency gates |
| D05 | Iroh/Noq behind `sunna-link` | Own NAT stack too much work; mandatory tsnet adds product dependencies; WebRTC everywhere complicates native path | A specific measured requirement cannot be met with a small maintainable adapter/patch |
| D06 | QUIC DATAGRAM media, separate bounded reliable purposes | All-reliable video builds stale data; raw custom UDP crypto/traversal duplicates mature work | Transport limitation is demonstrated on required platforms |
| D07 | One path budget for encoder, FEC, RTX and pacing | Independent bitrate/transport queues cause bufferbloat; fixed FEC/rate ignores path | A replacement controller demonstrably improves stability/fairness |
| D08 | LTR/RFI when verified, always retain IDR fallback | Keyframe-on-every-loss wastes bandwidth; guessing safe reference discard corrupts decoder state | Vendor exposes stronger verified recovery APIs |
| D09 | Device-key pairing, account optional, host-local grants | Server-trusted mutable peer keys and reusable plaintext-like passwords weaken trust | No routine reopening; invariant |
| D10 | Unprivileged broker and user-session media agent | Root network+codec process increases attack impact | A narrow OS operation demonstrably requires privilege, handled in helper |
| D11 | Public Wayland portals first | Private Mutter as universal API, blanket KMS privilege, owned compositor as default overreach | Supported portal gaps justify a clearly scoped optional backend |
| D12 | Tauri chrome, native viewer, Rust policy | Three full native UI stacks exceed solo capacity; one web hot path loses control | Shell accessibility/packaging cost outweighs replacement cost |
| D13 | Exclusive keyboard/mouse lease, independent gamepad slots | Implicit merged modifiers and sticky devices create unsafe confusing control | A tested collaboration model adds clear value |
| D14 | Physical-display fallback; virtual display is an owned optional lease | Mandatory private/driver-dependent display creation makes setup fragile | A stable universal OS facility is available |
| D15 | Browser/mobile clients later, no general mobile host | Breadth before native correctness delays a useful product; browser constraints differ | Native core and measured demand justify prioritization change |
| D16 | Capability probes and explicit degraded tiers | Codec/vendor names alone do not establish real hardware, HDR, 4:4:4 or gesture support | No routine reopening; invariant |
| D17 | Optical competitive release gates | Overlay-only measurements and vendor marketing figures are incomparable | A calibrated equivalent end-to-end measurement method is available |
| D18 | Small early useful releases with honest scope | Waiting for every OS/codec/driver before users provides no feedback; claiming all goals early is unsupported | Evidence supports expanding the release matrix |

The architecture's central bet is testable: remove avoidable capture/presentation and queue delay, repair references without routine bitrate explosions, restore exact stationary pixels, and make authenticated connectivity ordinary. If those measured gains are insufficient to outrun a competitor on a given workload, the report says so. The product earns each superiority claim from the running implementation; this proposal supplies the contracts and experiments needed to get there.
