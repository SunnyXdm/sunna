# Sunna Research: Building Something Better Than Parsec

> Synthesis compiled 2026-08-03 from four deep-dive reports in this directory:
>
> 1. [01-parsec-internals.md](01-parsec-internals.md) — how Parsec actually works (BUD protocol, video pipeline, drivers, NAT traversal, SDK)
> 2. [02-competitive-landscape.md](02-competitive-landscape.md) — Moonlight/Sunshine/Apollo/Wolf, Steam Remote Play, WebRTC systems, Stadia/xCloud/GFN, enterprise remote desktop
> 3. [03-engineering-reference.md](03-engineering-reference.md) — the 2026 component-by-component engineering reference (capture, encode, transport, FEC/CC, NAT, input, audio, render, latency budget)
> 4. [04-market-gaps.md](04-market-gaps.md) — Parsec's weaknesses, why users defect, the 10 biggest exploitable gaps
> 5. [05-trackpad-gestures.md](05-trackpad-gestures.md) — why trackpad gestures die over remote sessions and how to fix them (semantic gesture channel; native injection on Windows 11/Linux; macOS options)

---

## 1. How Parsec works, in one page

Parsec's whole design is **vertical integration with zero buffering anywhere**:

- **Capture:** DXGI Desktop Duplication (Windows) delivers frames in VRAM at compositor cadence; the frame never touches system memory again ("zero-copy GPU pipeline").
- **Encode:** hardware-only policy — NVENC / AMD AMF / Intel QuickSync / VideoToolbox called **directly via vendor SDKs, no wrapper layers**. H.264 baseline, HEVC optional, 4:4:4 chroma paywalled (Warp/Teams), **no AV1, no HDR**. Combined encode+decode < 10 ms.
- **Transport:** **BUD** ("Better User Datagrams") — custom UDP protocol with DTLS 1.2, TCP-like reliability semantics only where needed, and a **predictive congestion controller co-designed with the encoder**: on congestion signals, the *next frame's* bitrate changes. Priority order: latency > frame rate > quality.
- **Connectivity:** STUN-based UDP hole punching (claimed 97% success) + optional UPnP, brokered by their cloud API ("Kessel") over TCP 443. **No consumer relay** — if hole punching fails, the connection just fails (error 6013). Enterprise gets a self-hosted High Performance Relay.
- **Client:** render-on-arrival — no jitter buffer, latest-frame-wins frame dropping, flip-model presentation. Input rides the same unbuffered UDP channel as tiny messages.
- **Audio:** Opus, 20 ms frames @ 48 kHz, independent stream, no A/V mux or lipsync buffering.
- **Drivers (the moat):** a signed IddCx **Virtual Display Driver** and a signed kernel **Virtual USB driver (VUSB)** that emulates a USB host controller with Xbox 360 / DualSense / Wacom tablet / virtual microphone / YubiKey passthrough devices — this is what makes co-play gamepads and pen tablets "just work."
- **Result:** ~7 ms added click-to-photon on LAN at 1080p60 (their own 1000fps-camera measurement).

The same recipe — UDP + encoder-coupled congestion control + zero receive buffering + hardware codec pipelines — shows up in every low-latency winner (Moonlight/GameStream, Stadia's tuned WebRTC, GeForce NOW). It is the **table-stakes architecture**, not the differentiator.

## 2. The state of the field (2026)

| System | Latency | Connectivity UX | Openness | Quality ceiling |
|---|---|---|---|---|
| **Parsec** | ~7 ms LAN added | Best-in-class (zero config, 97% traversal) | Closed, account-required, cloud-dependent | Weak: no HDR/AV1, 4:4:4 paywalled, ~50 Mbps cap |
| **Moonlight + Sunshine** | Best-in-class (sub-10 ms LAN reported) | Worst: port forwarding / Tailscale DIY | GPL, no account | Best: AV1, HDR10, 4:4:4, 500 Mbps, 4K120 |
| **Apollo/Artemis (Sunshine fork)** | Same as Sunshine | Same (LAN-first) | GPL | + auto-matching virtual display w/ HDR, per-client permissions |
| **Wolf** | Good | LAN/overlay | MIT | Linux-first multi-user containerized hosting (GFN-at-home shape) |
| **Steam Remote Play** | Middling (30–100 ms) | Great (SDR relay network) | Closed | AV1 now, adaptive quality degrades image |
| **RDP/DCV/PCoIP/Citrix** | 30–100 ms | Enterprise-managed | Closed | Text-optimized, not frame-paced for games |

Key structural facts:

- NVIDIA killed GameStream → Sunshine became the de facto open host; **all the innovation is happening in Sunshine forks** (Apollo's virtual display, Duo's multi-seat, Wolf's containers).
- **WebRTC gets you 80% of the plumbing but costs the last 10–15 ms** (opaque jitter buffer, no encoder control, conferencing-tuned GCC). Everyone serious forks it or bypasses it; use it only for browser reach.
- The community's revealed preference: **Sunshine + Tailscale** — users bolt a WireGuard overlay onto the best-latency open stack because nobody open owns connectivity.
- Parsec post-Unity is in **maintenance mode on consumer features** (no HDR/AV1/Linux host after 3+ years of requests) while Sunshine ships majors every 1–3 months.

## 3. Where "better than Parsec" is actually winnable

Ranked synthesis of the gap analysis (details and sources in [04-market-gaps.md](04-market-gaps.md)):

1. **Zero-config connectivity + self-hostability in one product** — Parsec's UX with RustDesk's trust model (login-optional, hosted relays by default, self-hostable rendezvous/relay). Nobody offers both. This is the #1 structural opening.
2. **Linux host** — 8 years of unanswered requests; SteamOS/Bazzite/homelab tailwind; Wolf proves the demand.
3. **LAN-only / offline mode, no account** — Parsec bricks without its auth servers even on LAN.
4. **Modern quality free by default** — HDR10 + AV1 + 4:4:4 + 120fps+ + high bitrate. Parsec paywalls or lacks all of these; OSS gives them away.
5. **Multi-user co-play** — Parsec's last unique consumer moat (guest links + per-guest virtual gamepads). Sunshine is single-session by design; shipping this openly removes Parsec's final differentiator.
6. **Virtual display done right** — Apollo/SudoVDA model: auto-created virtual monitor matching the client's exact resolution/refresh/HDR, destroyed on disconnect. Headless-first.
7. **Apple Silicon host** — everyone is weak here (Parsec: no guest gamepads/service mode; Sunshine: least mature platform). High-willingness-to-pay creative segment.
8. **Honest team pricing / open-core admin plane** — Parsec Teams is $30–45/user/mo + $25 per guest invite, with Unity trust-deficit tailwind.
9. **Real browser client** — WebCodecs + WebTransport are mature now; Parsec's Chrome-only, no-HW-decode, can't-reach-Mac-hosts web app is beatable.
10. **Handheld-native UX** — Steam Deck/ROG Ally auto controller profiles, quick resume; today this requires the MoonDeck plugin hack.
11. **Trackpad gestures over the wire** — no streaming product forwards them (only Citrix, 2026, Mac-to-Mac). Windows 11 now has a real synthetic-touchpad injection API and Linux has uinput; a semantic gesture channel covers macOS hosts. See [05-trackpad-gestures.md](05-trackpad-gestures.md).

Constraint to respect: kernel anticheat (Vanguard) blocks virtual input for everyone; signed drivers + vendor whitelisting effort is the only mitigation and would itself be a differentiator.

## 4. Recommended architecture sketch

The consistent winning pattern, updated with 2026 tech (full engineering detail in [03-engineering-reference.md](03-engineering-reference.md)):

- **Language:** Rust core (RustDesk proves viability; quinn/str0m/windows-capture/screencapturekit-rs ecosystem) with thin C FFI to vendor encoder SDKs. Study Sunshine (host) and moonlight-qt (client) throughout.
- **Capture:** Windows DDA primary + WGC fallback (24H2 realities); macOS ScreenCaptureKit; Linux KMS (headless-first) + PipeWire portal fallback. All zero-copy GPU surfaces into the encoder.
- **Encode:** direct NVENC/AMF/libvpl/VideoToolbox. NVENC reference config: ultra-low-latency tuning, CBR + 1-frame VBV, no B-frames, infinite GOP + intra-refresh, LTR + reference-frame invalidation. Codec ladder AV1 > HEVC > H.264; HEVC 4:4:4 for desktop/text mode; 10-bit P010 path for HDR.
- **Transport:** QUIC (quinn) as the base — datagrams (RFC 9221) for media/input, one reliable stream for control — giving crypto, connection migration, and a WebTransport-shaped path to browsers for free; own the congestion controller (SCReAM/SQP-style, L4S/ECN-aware) and couple it to per-frame encoder reconfiguration. Per-frame RS parity FEC sized to measured loss + NACK when RTT < frame interval + RFI backstop. sendmmsg/GSO batching, 2–5 ms per-frame pacing.
- **Connectivity:** ICE-style traversal (libjuice-equivalent logic) with IPv6-first, birthday-paradox probing for hard NATs, and DERP-style always-works relays over 443; self-hostable rendezvous + relay binaries from day one.
- **Client:** render-on-arrival, zero jitter buffer, VRR-aware presentation (mailbox/immediate), hardware decode everywhere, per-stage latency overlay (Moonlight-style honesty).
- **Input/peripherals:** scancode-based keyboard, first-class relative mouse; uinput (Linux), SendInput (Windows), CGEventPost (macOS); virtual gamepad driver strategy decided early (own KMDF driver vs ViGEmBus fork — ViGEmBus is archived; signing budget required).
- **Audio:** Opus 5–10 ms low-delay frames, independent stream, in-band FEC.
- **Virtual display:** IddCx driver with SudoVDA-style auto-mode-matching (+ HDR via IddCx 1.10/24H2); Linux headless via KMS/gamescope; macOS dummy-plug fallback.
- **Latency budget target:** ~4–15 ms added on LAN, ~15–45 ms same-metro internet (per-stage table in the engineering reference).

## 5. Suggested build order

1. **Milestone 0 — LAN MVP (Windows host → any client):** DDA → NVENC ULL → QUIC datagrams → hardware decode → render-on-arrival. Measure with per-stage timestamps from day one; the latency overlay is a feature, not a debug tool.
2. **Milestone 1 — reliability layer:** FEC + NACK + LTR/RFI, encoder-coupled congestion control, Opus audio, full input (relative mouse, scancodes, clipboard).
3. **Milestone 2 — connectivity:** rendezvous service + hole punching + DERP-style relay; pairing without accounts (PIN/QR like Sunshine, links like Parsec); everything self-hostable.
4. **Milestone 3 — the differentiators:** Linux host (KMS headless-first), virtual display driver, HDR/AV1/4:4:4 free, multi-guest co-play with virtual gamepads.
5. **Milestone 4 — reach:** browser client (WebCodecs + WebTransport), macOS host parity, handheld UX.

## 6. Open strategic questions

- **Positioning:** gaming-first (couch co-op + your-own-PC streaming) vs work-first (creative workstations) — the tech is 90% shared but onboarding, pricing, and drivers priorities differ.
- **License/monetization:** evidence favors open-core (GPL core; paid: managed relay network, team admin plane, signed-driver builds, support). JetKVM ($4.4M+ Kickstarter, ~100k units) proves the homelab crowd pays for open products.
- **Protocol compatibility:** speak Moonlight's protocol for instant client ecosystem (like Wolf/Apollo) vs own protocol for the encoder-coupled transport advantages — or both (native protocol + Moonlight-compat mode).
