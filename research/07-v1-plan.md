# Sunna v1 Plan: Beat Apple Screen Sharing, Mac to Mac

> **Amended by [08-architecture-plan.md](08-architecture-plan.md)** (2026-09-25), notably: authentication moves early, congestion control starts from a safe baseline, connectivity reuses iroh behind a seam. Its "What to build first" section replaces the step order below.

> Written 2026-09-25. Combines three independent reviews of the codebase: Claude (Opus 5.5), Codex (GPT-6-Astra, max reasoning), and Fable 5.1. Where they disagreed, the decision and its reason are recorded below. This updates the milestone order in [06-architecture.md](06-architecture.md) §5.

## Positioning (decided)

Quality-first remote access where the remote machine **feels natively installed**: your Mac(s) from your MacBook, your Linux box from any laptop. macOS and Linux hosts are first-class. Gaming uses the same low-latency pipeline but gaming-only work (gamepads, co-play, anticheat) comes later. The Windows/NVENC host is deprioritized.

**Benchmark to beat:** macOS Screen Sharing "High Performance" mode (Apple silicon, macOS 14+, UDP 5900–5902, ~75 Mbps recommended for 4K, 4:4:4, HDR, 60 fps, virtual display). The owner tested it between an M1 Air and an M2 Air over Tailscale and was not impressed. Sunna has to beat it on the LAN and clearly win on real internet paths.

**v1 scope:** one Mac desktop to one Mac client, SDR, native Retina resolution, faithful input, pixel-exact static text, clipboard, audio. Not in v1: per-app windows (Coherence-style), HDR, multi-user, gamepads, our own relay network (Tailscale covers connectivity for now).

## What the reviews found (ranked)

1. **Correctness bugs.**
   - The host drops an already-encoded frame under backlog without leaving a gap in the wire frame ids. The encoder's next P-frame references the dropped frame, the client sees no gap, and it decodes against the wrong reference (`host/src/lib.rs` `media_loop`).
   - The client's control reader is not cancellation-safe: `tokio::select!` can drop `ControlChannel::recv` mid-message, which desyncs the length-prefix framing (`client/src/lib.rs`).
   - `CGEventCreateScrollWheelEvent` is variadic in C but declared with fixed arguments. On Apple arm64, variadic arguments go on the stack, so `wheel2` (horizontal scroll) is garbage (`input/src/macos.rs`).
2. **Client presentation is the biggest latency gap.** VT decodes to BGRA, the frame is copied to a Vec, then converted pixel by pixel into softbuffer, then Core Animation uploads it. Presentation goes through winit's redraw cycle, which adds 0–2 vsyncs of waiting. The viewer scales with nearest-neighbour, which aliases text. Decode runs inside the network task, so it stalls datagram reads and input sends.
3. **The host copies every frame twice**: IOSurface → Vec, then Vec → a freshly allocated CVPixelBuffer. That is about 2 GB/s of memcpy at 2560×1664@60. A static screen is re-encoded at 60 fps with fresh timestamps.
4. **The reassembler can't tolerate reordering.** It holds one partial frame, so one out-of-order packet counts as a lost frame and triggers a keyframe request (an IDR storm on Wi-Fi and Tailscale paths).
5. **Congestion control reacts too slowly and fights itself.** quinn's default CUBIC with slow start sits under a 1-second app-level AIMD loop that has a never-decaying baseline. The sender backlog caps allow 160–560 ms of stale queue at 15 Mbps.
6. **XOR FEC is weak.** 1 parity per 8 fails about 56% of the time per group at 20% independent loss, and it breaks under bursts. There are no sequence numbers, so NACK and per-packet delay measurement are impossible.
7. **Input fidelity.** Input shares the reliable control stream (head-of-line blocking). Scroll phase and momentum are dropped. There are no modifier flags on posted keys, no autorepeat, Mac keycodes are on the wire, and nothing releases held keys on disconnect. The cursor is baked into the video.
8. **Encoder config is minimal.** Missing: `MaxFrameDelayCount`, `DataRateLimits`, QP bounds, High profile, colour tagging, and a hardware requirement. Property failures are only logged at debug level.
9. **Measurements are optimistic.** The capture timestamp is taken after the copy, static re-deliveries get fresh timestamps, latency stops at decode (not present), stalls are invisible, and it uses wall-clock time.

## Decisions where the reviewers disagreed

| Topic | Options | Decision |
|---|---|---|
| Capture pixel format | NV12 from SCK (Fable) vs BGRA (Codex) | **BGRA.** Pixel-exact text tiles and any 4:4:4 scheme need full-chroma RGB, and VT's RGB→YUV conversion is done in hardware. |
| Client presenter | wgpu + hal texture import (Fable) vs direct `CAMetalLayer` (Codex) | **Direct `CAMetalLayer` on macOS behind a small presenter trait**, driven from a dedicated present thread (Fable's point: remove the winit redraw hop). wgpu can come later for Linux/Windows clients. |
| Default codec | HEVC (Fable) vs H.264 with HEVC measured (Codex) | **H.264 High now; add HEVC as a negotiated option and choose the default by measurement.** |
| QUIC congestion control | Custom quinn `Controller` early (Fable) vs keep CUBIC and add an app controller (Codex) | **Custom controller** (no slow start, window derived from the app's target rate), inside the transport step. Measure first. |
| 4:4:4 | RDP AVC444-style dual stream (Fable) vs undocumented `Main444` profiles (Codex, per a July 2026 dev-forum report) | **Both, after lossless tiles.** Probe the undocumented profiles on both Airs first (cheap). Otherwise use the dual-stream aux-chroma scheme, which works on any decoder. |
| When to add auth | Early (Codex) vs last (Fable) | **Last, with pairing and pinned certs.** Until then, bind `sunnad` to the Tailscale IP only; tailnet ACLs are the access control. |
| Whole desktop vs per-app windows | Both reviewers agree | **Whole desktop on a client-sized virtual display.** Per-app streaming needs a mirrored window tree that macOS doesn't expose. Revisit after v1. |

## Ordered steps

Each step is one PR (or a small series) with an acceptance test. Run the measurement rig (step 1) after every step from then on.

0. **Correctness + finish zero-copy capture → encode.** Fix the sender-drop reference bug (admission check before encode; after-encode drops leave a wire-id gap and force a keyframe). Split the control channel so a dedicated task owns the reader. Switch to `CGEventCreateScrollWheelEvent2`. Finish `FrameData::Surface`: a retained IOSurface (use count held) wrapped in a CVPixelBuffer and handed straight to VT.
   *Accept:* builds on both Airs; no CPU pixel copy between capture and encode; horizontal scroll works; forcing a sender drop never shows corrupted frames.
1. **Honest measurement.** Monotonic per-stage timestamps (capture → encode → send → receive → decode → present), stall/freeze accounting (longest freeze, count), a `flash` test mode for click-to-photon with a photodiode or 240 fps camera, and a written procedure. Baseline Apple HP mode and Sunna on the same Airs: same Wi-Fi, Tailscale direct (check `tailscale ping` for direct vs DERP), and a `tc netem` path through the Linux box.
2. **Client presenter.** Decode on its own thread into IOSurface-backed NV12; `CVMetalTextureCache` gives zero-copy textures; `CAMetalLayer` presents on arrival from a present thread (`displaySyncEnabled=false` as an option, `maximumDrawableCount=2`); good scaling filter; `presentedTime` telemetry.
   *Accept:* decode→present p50 ≤ 3 ms; no CPU pixel pass; client CPU < 30% of a core at Retina 60.
3. **Host capture/encode done right.** ScreenCaptureKit (`SCStream`, BGRA, `queueDepth=3`, `displayTime` as the capture timestamp, idle frames skipped, dirty rects kept). Full VT low-latency property set: hardware required, `MaxFrameDelayCount=0`, `DataRateLimits`, QP bounds, H.264 High, colour tags; validate supported properties and log the ones that fail. HEVC as a negotiated option. The decoder is recreated when parameter sets change.
   *Accept:* static screen < 50 kbps; capture→encode p50 ≤ 6 ms; keyframes bounded by the rate limit.
4. **Client-side cursor + input fidelity.** Cursor shape side channel (probe `NSCursor.currentSystem` from a background agent), `showsCursor=false`. USB HID usages on the wire, modifier flags on posted events, autorepeat, scroll phase and momentum via an NSEvent monitor plus pixel-precise scroll injection, release-all on focus loss or disconnect, and host IME for composition.
5. **Transport v1.** Media header v1 with a transport-wide sequence number and send timestamp; reassembler that tolerates reordering and expires by deadline; adaptive Reed–Solomon FEC; NACK when time remains before the deadline; 20–50 ms feedback; delay-based rate control coupled to the encoder; custom quinn congestion controller; input and pings on datagrams.
   *Accept:* netem 30±10 ms, 1% bursty loss: no corrupt frames, no keyframe requests caused by reordering, p95 within 15 ms of the clean floor.
6. **LTR recovery.** VideoToolbox `EnableLTR`, ack tokens, `ForceLTRRefresh`; keyframe only as the last resort.
7. **Virtual display + resolution follow.** Isolated `CGVirtualDisplay` module (private API, checked at runtime, falls back to the physical display), HiDPI-matched to the client's view size, debounced resize, configuration epochs.
8. **Build-to-lossless static tiles.** 64–128 px RGB tiles, sent losslessly once a region is stable, invalidated by dirty rects, on a low-priority reliable stream.
   *Accept:* a static page is byte-identical to the host within 1 s.
9. **4:4:4** (see the decisions table) and end-to-end colour management (P3/sRGB tagging, Metal layer colourspace).
10. **Clipboard + audio.** Text and images first; SCK audio → Opus → datagrams → CoreAudio.
11. **Ship shape.** Signed `Sunna.app` with `sunnad` as an `SMAppService` LaunchAgent, TCC onboarding and re-approval detection (apply for `com.apple.developer.persistent-content-capture`), PIN pairing with pinned certs.

**Target before calling v1 done:** on the two Airs over LAN Wi-Fi, click-to-photon p50 at least 10 ms better than Apple HP mode and p99 no worse than Apple's p50. Over Tailscale-direct with 1% bursty loss: no visible corruption, and static text converges to pixel-exact while Apple's stays soft.
