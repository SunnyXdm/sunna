# Competitive & Open-Source Landscape: Low-Latency Game Streaming / Remote Desktop

> Research compiled 2026-08-03. Part 2 of the "build something better than Parsec" research series.

Baseline for comparison: Parsec itself is a proprietary peer-to-peer stack — DXGI Desktop Duplication capture with a zero-copy GPU path into NVENC/AMF/QuickSync, H.264/HEVC (4:4:4 chroma on supported NVIDIA/Intel hosts), and a custom UDP protocol called **BUD (Better User Datagrams)** encrypted with DTLS 1.2, with a claimed 97% NAT-traversal success rate and a congestion controller co-designed with the encoder so bitrate reacts within a frame or two ([Parsec technology page](https://parsec.app/technology), [Parsec BUD blog post](https://parsec.app/blog/a-networking-protocol-built-for-the-lowest-latency-interactive-game-streaming-1fd5a03a6007)). Hosting is Windows and macOS only (no Linux host; Linux/RPi are client-only), the Virtual Display Driver is Windows-only, and 4:4:4 + virtual displays are gated behind the paid Warp tier; Unity acquired Parsec for $320M in 2021 ([TechCrunch](https://techcrunch.com/2021/08/10/unity-to-acquire-parsec-in-its-biggest-acquisition-to-date/amp/), [Parsec compatibility](https://support.parsec.app/hc/en-us/articles/32381568346644-Hardware-and-Software-Compatibility), [Parsec VDD docs](https://support.parsec.app/hc/en-us/articles/32381178803604-VDD-Overview-Prerequisites-and-Installation)). Everything below is what you'd be competing against or borrowing from.

---

## 1. Moonlight + Sunshine (the open-source reference stack)

**Architecture.** Moonlight is a family of open-source clients implementing NVIDIA's GameStream protocol; the shared protocol core is [moonlight-common-c](https://github.com/moonlight-stream/moonlight-common-c) (GPL-3.0), used by the Qt desktop client, Android, iOS/tvOS, and embedded ports. NVIDIA killed GameStream in GeForce Experience (end-of-service announced for Feb 2023), so [Sunshine](https://github.com/LizardByte/Sunshine) (LizardByte, GPL-3.0) reimplements the host side and is now the de facto server; the Moonlight FAQ explicitly steers users to it ([Moonlight FAQ](https://github.com/moonlight-stream/moonlight-docs/wiki/Frequently-Asked-Questions)).

**Transport.** The protocol is a hybrid: HTTPS for pairing (client certificates exchanged via PIN), RTSP for session negotiation, then **video and audio over RTP/UDP (ports 47998-48000), and a reliable control/input stream over a patched ENet** (UDP) — Moonlight bundles a forked ENet with IPv6 and retransmission-reliability changes as a submodule ([moonlight-common-c](https://github.com/moonlight-stream/moonlight-common-c)). Forward error correction is **Reed-Solomon** (the `nanors` submodule) applied per video frame — the encoder output is sharded into packets plus parity shards so a lost packet doesn't force a retransmit round-trip; audio FEC uses the same RS algorithm with audio-specific parameters. Sunshine has added its own protocol extensions (message types 0x5500–0x5506) beyond stock GameStream ([DeepWiki UDP media streaming analysis](https://deepwiki.com/qiin2333/foundation-sunshine/7.3-udp-media-streaming)).

**Capture/encode.** Sunshine's per-platform matrix ([DeepWiki: platform capture](https://deepwiki.com/LizardByte/Sunshine/5.3-platform-specific-capture-implementations), [encoding pipeline](https://deepwiki.com/LizardByte/Sunshine/5.2-video-encoding-pipeline)):

- **Windows:** DXGI Desktop Duplication (`AcquireNextFrame`) or Windows.Graphics.Capture (needed for reliable HDR), D3D11 zero-copy into encoders. NVENC is driven **directly via the NVENC SDK, bypassing FFmpeg's avcodec layer**, for lower latency and tighter control; AMF and QuickSync go through avcodec hardware frames.
- **Linux:** KMS grab (requires cap_sys_admin), X11, or Wayland portal capture; encode via VAAPI (AMD/Intel), CUDA/NVENC, with software x264/x265 fallback.
- **macOS:** CoreGraphics/AVCaptureScreen capture + VideoToolbox encode; **no gamepad emulation on macOS host** — the weakest platform ([Sunshine repo](https://github.com/LizardByte/Sunshine)).

**Codecs/HDR/AV1.** H.264, HEVC, and AV1 end to end; AV1 requires RTX 40-series / Intel Arc / RX 7000-class encoders. Moonlight clients do 4K120 with HDR10 where the decoder allows ([Moonlight setup guide coverage](https://techsngames.com/moonlight-pc-streaming-setup/), [moonlight-stream.org](https://moonlight-stream.org/)).

**NAT traversal.** The weak spot: **there is none built in.** WAN use means manual port forwarding, the semi-official Moonlight Internet Hosting Tool (UPnP), or a VPN overlay (ZeroTier/Tailscale) ([Moonlight FAQ](https://github.com/moonlight-stream/moonlight-docs/wiki/Frequently-Asked-Questions)). No relays, no ICE, no signaling service — this is exactly the gap Parsec's BUD + broker fills, and why the Tailscale section below exists.

**Latency reputation.** Best-in-class among self-hosted options: the project claims sub-15 ms end-to-end in ideal conditions; community tests report sub-10 ms capture-to-display on wired LAN with hardware decode — generally measured a frame or so better than Parsec, and much better than Steam Remote Play on the same hardware ([moonlight-stream.org](https://moonlight-stream.org/), [techsngames guide](https://techsngames.com/moonlight-pc-streaming-setup/), [Steam community comparison thread](https://steamcommunity.com/groups/homestream/discussions/0/3119298624516514270)). Moonlight also exposes an honest per-stage latency overlay (network/decode/queue/render).

**Complaints.** Setup friction is the number-one theme: pairing + port forwarding; headless hosts need a **dummy HDMI plug or a third-party virtual display driver**, and VDDs cause stuttering/frame-pacing issues for some users ([Virtual-Display-Driver issue #36](https://github.com/itsmikethetech/Virtual-Display-Driver/issues/36)); HDR breaks in ugly ways when the HDR monitor disappears mid-session (washed-out SDR output, [Sunshine issue #1853](https://github.com/LizardByte/Sunshine/issues/1853)); laptops with iGPU/dGPU muxes confuse capture-adapter selection ([issue #2078](https://github.com/LizardByte/Sunshine/issues/2078)); NVENC breakage after driver updates ([issue #2187](https://github.com/LizardByte/Sunshine/issues/2187)). Resolution/refresh matching between client and host historically required scripts — which is exactly what the Apollo fork fixed (section 5).

---

## 2. Steam Remote Play / Steam Link

**Architecture & protocol.** Valve's stack was thoroughly reverse-engineered by Thalium ([RCE writeup](https://blog.thalium.re/posts/achieving-remote-code-execution-in-steam-remote-play/), [SSTIC 2023 paper](https://www.sstic.org/media/SSTIC2023/SSTIC-actes/bug_hunting_in_steam_remote_play/SSTIC2023-Article-bug_hunting_in_steam_remote_play-ricotta.pdf)): a UDP discovery protocol on port 27036, then a session with handshake → session-key agreement → HMAC-MD5 authentication (magic string "Steam In-Home Streaming") → protobuf-based control negotiation. Control messages are AES-CBC encrypted with an HMAC-MD5-derived IV. The protocol multiplexes **32 channels** over one socket: channel 1 for ~100 control message types, channel 2 for stats, channels 3–31 dynamically allocated to audio/video streams. The client is `streaming_client.exe` (SDL-based); the server lives in SteamUI.

**Transport.** Four modes: **direct UDP, relayed UDP, WebRTC, and SDR (Steam Datagram Relay)** — Valve's private relay backbone, also used for game traffic ([Steamworks SDR docs](https://partner.steamgames.com/doc/features/multiplayer/steamdatagramrelay)) — built on a modified GameNetworkingSockets with STUN/TURN support. This gives Valve the best NAT-traversal story of any consumer product: it silently falls back to Valve-operated relays worldwide.

**Codecs & latency techniques.** Raw/VP8/VP9/H.264/HEVC per the RE work, with **AV1 now auto-selected when host and client both support it** (~30% more efficient than H.264; 4K120 at ~100 Mbps) ([XDA](https://www.xda-developers.com/steam-remote-play-works-better-over-lan-than-people-realize/)); Opus audio. A "Low latency networking" client toggle bypasses OS network buffering and pushes packets straight to the decoder ([Steam community](https://steamcommunity.com/app/353380/discussions/4/3874844033654329501/)). Valve integrates capture into the game process via the Steam overlay, so it captures the game surface rather than the desktop.

**Latency reputation & complaints.** Middling: community measurements put it at ~30–50 ms for light games and 80–100 ms for heavy ones ([Steam forum thread](https://steamcommunity.com/groups/homestream/discussions/0/540731690900745009/)), and users consistently measure higher display latency than Parsec/Moonlight ([comparison thread](https://steamcommunity.com/groups/homestream/discussions/0/3119298624516514270)). Its aggressive bitrate adaptation degrades image quality noticeably; it's tied to Steam being running, non-Steam apps need workarounds, and nothing is open source (though the [Steam Link hardware SDK](https://github.com/ValveSoftware/steamlink-sdk) is public). Strength: zero-config discovery, relays, Remote Play Together multi-user input over the same channel architecture.

---

## 3. WebRTC-based approaches

### What WebRTC gives you for free

- **Connectivity:** ICE/STUN/TURN NAT traversal and encrypted transport (DTLS-SRTP) out of the box — the single hardest part of Parsec's stack (BUD + brokers) handed to you ([Chrome Remote Desktop network guide](https://support.google.com/chrome/a/answer/16364503?hl=en)).
- **Congestion control:** GCC (Google Congestion Control) — a delay-gradient Kalman-filter estimator fed by **transport-wide CC (TWCC) feedback**, combined with a loss-based estimator, min of the two wins; feedback arrives 10–20×/second and the encoder target tracks it ([GCC analysis paper](https://c3lab.poliba.it/images/6/65/Gcc-analysis.pdf), [Flussonic TWCC explainer](https://flussonic.com/blog/news/transport-cc)).
- **Opus** audio, simulcast, FEC/NACK/PLI recovery machinery, and a browser client on every platform ever made.
- **Escape hatches:** the `playout-delay` RTP extension lets a sender request ~0ms jitter buffer (Chrome supports it; explicitly aimed at cloud gaming) ([WebRTC playout-delay docs](https://webrtc.googlesource.com/src/+/main/docs/native-code/rtp-hdrext/playout-delay/README.md)), and Insertable Streams + WebCodecs give partial control of the encode/decode pipeline in-browser — Chrome only, not Safari ([webrtcHacks pipelines](https://webrtchacks.com/real-time-video-processing-with-webcodecs-and-streams-processing-pipelines-part-1/), [Red5 insertable streams](https://www.red5.net/docs/development/webrtc/insertable-streams/)).

### Where it falls short for sub-20 ms

The receive-side **jitter buffer is adaptive and opaque** — research shows it materially degrades cloud-gaming QoE and that you can't fully tune it from the app layer ([IEEE study on WebRTC jitter buffer in cloud gaming](https://ieeexplore.ieee.org/document/11012899/), [JitBright, NOSSDAV'24](https://dl.acm.org/doi/10.1145/3651863.3651881)). In a browser you don't control the encoder (no per-frame QP, no intra-refresh scheduling, no FEC shaping tied to encoder output like Moonlight/Parsec do), pacing and frame-dropping policy are libwebrtc's, and GCC is tuned for videoconferencing ramp rates, not game bitrate spikes. Everyone serious ends up forking libwebrtc (Stadia, xCloud) or using WebRTC only for transport/signaling. Browser-side decode also can't guarantee the low-latency decoder path on all platforms.

### Implementations

- **Selkies** ([selkies-project/selkies](https://github.com/selkies-project/selkies), MPL-2.0 core): Linux X11 → HTML5 browser via GStreamer `webrtcbin`, Python signaling; NVENC or VA-API accelerated H.264 (plus VP8/VP9/software fallback), 60 fps at 1080p; originated from Google engineers' GPU-streaming work and is now the streaming layer in linuxserver.io "webtop" style containers, Kubernetes/HPC remote desktops ([project site](https://selkies-project.github.io/selkies/)). Optimized for containerized desktops, not gaming — latency is "good for a browser," not Moonlight-class.
- **Neko** ([m1k1o/neko](https://github.com/m1k1o/neko), Apache-2.0): Dockerized shared browser/desktop (Xorg + PulseAudio + GStreamer + Pion WebRTC, Go server, Vue client); its differentiator is **multi-user sessions with host-control handoff** (watch parties, collaborative browsing), not latency.
- **Chrome Remote Desktop:** free, proprietary Google service ("Chromoting" is now WebRTC; the legacy Chromotocol is being deleted). ICE with Direct/STUN/TURN modes, VP8 over UDP with TCP fallback; on Windows a multi-process host (SYSTEM daemon + capture process + sandboxed network process) ([Chromium docs](https://chromium.googlesource.com/chromium/src/+/ef559ab66624bd19bd82baa17309bce4ef1c7aa4/agents/prompts/templates/crd.md), [network guide](https://support.google.com/chrome/a/answer/16364503?hl=en), [Wikipedia](https://en.wikipedia.org/wiki/Chrome_Remote_Desktop)). Fine for admin work; frame rate, chroma quality, and input handling (no relative mouse, poor gamepad story) make it irrelevant for gaming.
- **Stadia (RIP 2023) — the WebRTC maximalist proof point:** Google ran a hardware-tuned VP9 encoder at the edge, used **WebRTC extensions to disable receive buffering and render frames on arrival**, a BBR-style per-packet feedback loop for bitrate, and Opus audio ([PCGamesN on Google's engineering talks](https://www.pcgamesn.com/stadia/google-game-streaming-technology-latency-quality)); traffic analysis confirmed standard DTLS/SRTP WebRTC with VP9/H.264 ([arXiv 2009.09786](https://arxiv.org/pdf/2009.09786)). "Negative latency" was Google's umbrella term for speculative techniques — running the game at higher fps server-side, predictive input, pre-rendered frames ([wccftech](https://wccftech.com/google-stadia-may-eventually-become-more-responsive-than-local-machines-thanks-to-negative-latency/), [Tom's Guide](https://www.tomsguide.com/news/google-claims-stadia-will-outperform-consoles-by-predicting-players-moves)). Lesson: even with a forked libwebrtc, dedicated edge encoders, and ISP peering, Stadia was "good but perceptibly not local" — and the business died anyway.
- **Xbox Cloud Gaming (xCloud):** WebRTC-derived — ICE (with Teredo support), DTLS-SRTP, RTP audio/video, NACK/PLI, plus **data channels for input/control/chat messaging**; the open-source [xbox-xcloud-player](https://github.com/unknownskl/xbox-xcloud-player) reimplements the client. Microsoft's 2025 "Direct Capture" grabs frames straight from the console rendering pipeline, skipping screen capture ([Microsoft GDC 2025 post](https://developer.microsoft.com/en-us/games/articles/2025/03/gdc-2025-xbox-cloud-gaming-beta-expanding-your-reach-enhancing-your-game/), [Cloud Dosage](https://clouddosage.com/how-xbox-is-quietly-fixing-xbox-cloud-gaming-latency/)).
- **GeForce NOW:** proprietary NVIDIA streamer (descended from the same GRID/GameStream lineage Moonlight cloned). Current state of the art for commercial streaming: AV1 with Reference Picture Resampling for seamless resolution shifts, 4:4:4 chroma + HDR10 "Cinematic Quality Streaming," Reflex integration, 360 fps at 1080p, and **L4S deployment with BT/Comcast/T-Mobile** achieving ~30 ms click-to-photon ([TechPowerUp](https://www.techpowerup.com/340035/nvidia-updates-geforce-now-with-rtx-5080-cinematic-streaming-new-low-latency-tech-and-more), [Cloud Dosage](https://clouddosage.com/geforce-now-av1/)). L4S (ECN-based low-latency queuing) is worth watching for any new protocol design.

---

## 4. Work-focused remote desktop incumbents

These optimize for **text/UI fidelity, security, scale, and WAN robustness** — not click-to-photon. Their common historical sin for gaming: TCP heritage, conservative frame cadence, and quality-first adaptation.

- **RDP:** Proprietary MS protocol, historically TCP. Modern stack: H.264/AVC with **AVC444 mode** (full 4:4:4 chroma, critical for text) and optional GPU hardware encoding ([MS Learn: AVD GPU acceleration](https://learn.microsoft.com/en-us/azure/virtual-desktop/graphics-enable-gpu-acceleration), [graphics encoding](https://learn.microsoft.com/en-us/azure/virtual-desktop/graphics-encoding)); **RDP Shortpath** adds direct UDP transport (with STUN/TURN relay via Azure) for consistent latency ([MS Learn Shortpath module](https://learn.microsoft.com/en-us/training/modules/implement-manage-networking-azure-virtual-desktop/8-plan-implement-remote-desktop-protocol-shortpath)). RemoteFX vGPU was deprecated for security reasons. Real-world AVC444 RDP still measures ~30–100 ms — 2–3× Moonlight/Parsec on identical hardware ([SuperRenders comparison](https://superrendersfarm.com/article/moonlight-parsec-rdp-remote-desktop-gpu-rendering-2026)) — because frame delivery is dirty-region-driven and cadence isn't vsync-locked. Strengths Parsec lacks: session multiplexing, device/printer/drive redirection, credential security, ubiquity.
- **NICE DCV (now Amazon DCV):** proprietary, free on EC2. Default transport is **WebSocket/TCP with an optional QUIC/UDP mode** recommended for lossy/high-latency links ([AWS docs](https://docs.aws.amazon.com/dcv/latest/adminguide/enable-quic.html)); GPU-accelerated H.264 encode server-side, browser or native clients. Optimized for VDI/HPC/3D CAD in AWS; solid but nobody claims sub-20 ms.
- **HP Anyware (Teradici PCoIP):** UDP-based, host-rendered, **multi-codec**: it classifies screen regions and encodes text differently from video/imagery, with "build-to-lossless" progressive refinement (lossy first pass, refined to pixel-exact) ([HP session planning guide](https://anyware.hp.com/products/hp-anyware/2025.06/documentation/session-planning-guide/pcoip-session-planning), [architecture guide](https://anyware.hp.com/products/hp-anyware/2025.03/documentation/architecture-guide/about-pcoip-technology)). PCoIP Ultra adds AVX2 CPU offload and NVENC H.264 paths. Designed for media/VFX workstations and zero clients: color accuracy, Wacom tablets, security (pixels-only leave the host) — not frame pacing for games.
- **Citrix HDX:** ICA protocol multiplexing many virtual channels; **EDT (Enlightened Data Transport)** is a proprietary reliable-UDP transport with TCP fallback ("Adaptive Transport") that mainly helps on long-haul, lossy WANs — Citrix's own guidance notes it can *degrade* performance on sub-100 ms LANs ([Citrix docs](https://docs.citrix.com/en-us/citrix-virtual-apps-desktops/2303/hdx-transport/adaptive-transport.html), [community thread](https://community.citrix.com/topic/226465-hdx-adaptive-transportedt-over-udp-high-latencyrtt-vs-tcp/)). HDX 3D Pro adds GPU H.264/HEVC for CAD. Enterprise-management-first, gaming-irrelevant.
- **VNC/RFB:** open protocol ([RFC 6143](https://datatracker.ietf.org/doc/html/rfc6143)), structurally incapable of low latency: **client-pull framebuffer updates** (server only sends when the client requests), server-side framebuffer polling at ~5–15 fps in common servers, rectangle-based encodings rather than a video codec, TCP transport ([RFB protocol, Wikipedia](https://en.wikipedia.org/wiki/RFB_protocol), [linuxvox analysis](https://linuxvox.com/blog/linux-screen-desktop-video-capture-over-network-and-vnc-framerate/)). Its virtue is universality and simplicity.

---

## 5. Rainway and the newer entrants

**Rainway** (2017–2022): browser-first game streaming from Windows hosts — the client was a web app (plus iOS/Android/Xbox One), streaming over WebRTC with a C#/C++ host; launched v1.0 in Jan 2019 ([Wikipedia](https://en.wikipedia.org/wiki/Rainway)). In April 2021 it partnered with Microsoft — its tech was integrated into **Xbox Cloud Gaming's browser client** — and pivoted fully to a B2B SDK ("Web Runtime runs in any modern browser," Windows-host-only native runtime, [docs](https://docs.rainway.com/docs/what-is-rainway)). The consumer service shut down Oct 31, 2022; CEO Andrew Sampson cited unsustainability against xCloud-class competition and infrastructure costs ([Grokipedia summary](https://grokipedia.com/page/Rainway)). Lessons: web-first distribution was genuinely differentiating (Microsoft bought into it), but consumer P2P streaming had no business model, and the SDK market was too thin to sustain a company.

**Sunshine-ecosystem forks — where the energy is now:**

- **Apollo** ([ClassicOldSong/Apollo](https://github.com/ClassicOldSong/Apollo), GPL-3.0): Sunshine fork whose headline feature is an integrated **SudoVDA virtual display driver**: a virtual monitor is created at stream start matching the client's exact resolution/refresh rate and destroyed at exit — eliminating dummy plugs and resolution scripts. Has an accompanying Android client fork, **Artemis** ("Moonlight Noir"), and is intentionally diverging from upstream Sunshine/Moonlight protocol compatibility.
- **Duo** ([DuoStream/Duo](https://github.com/DuoStream/Duo)): multi-seat Windows streaming — multiple users each get an independent RDP-session-based desktop with its own Sunshine instance, HDR-compatible, without occupying the physical console ([wiki](https://github.com/DuoStream/Duo/wiki)). Freemium (Patreon-gated management app).
- **Wolf** ([games-on-whales/wolf](https://github.com/games-on-whales/wolf), MIT): Linux-first **Moonlight-protocol-compatible server in C++** that spins up per-user Docker containers (Steam, desktops) with virtual Wayland displays and virtual input devices — any resolution/fps with no monitor or dummy plug, multi-GPU aware, multiple concurrent users. Effectively "self-hosted GFN node" and the most architecturally interesting open host.
- Also notable: **Sunshine-Foundation** fork (qiin2333) focused on HDR-enhanced Windows hosting ([DeepWiki](https://deepwiki.com/qiin2333/foundation-sunshine)), and the general pattern that all innovation is happening in forks because upstream Sunshine moves conservatively.

---

## 6. Tailscale + Moonlight/Sunshine (the DIY meta-stack)

The most common Reddit/forum recommendation for remote Moonlight is "just put Tailscale on both ends." Reasons ([Tailscale: How it works](https://tailscale.com/blog/how-tailscale-works), [NAT traversal improvements](https://tailscale.com/blog/nat-traversal-improvements-pt-1), [connection types](https://tailscale.com/docs/reference/connection-types)):

- Moonlight/Sunshine has **no NAT traversal**; Tailscale supplies it: WireGuard tunnels with aggressive STUN-style hole punching (>90% direct-connection success), coordinated via a control plane, with **DERP relays (TLS/443)** as automatic fallback — so worst case it still works, just with relay latency.
- The host gets a stable 100.x.x.x IP / MagicDNS name; no port forwarding, no exposure of Sunshine's HTTPS/RTSP ports to the internet, encryption for the otherwise LAN-trust-model GameStream protocol. Free for personal use (up to 100 devices). Setup guides are everywhere ([example gist](https://gist.github.com/andygrundman/b46ebfff67d340fa5556955b226446b2), [Level1Techs walkthrough](https://forum.level1techs.com/t/sunshine-server-setup-with-tailscale-and-moonlight-surface-pro-client-config/205362)).
- Cost: one extra encryption layer (WireGuard is cheap; ChaCha20 overhead is ~negligible on modern CPUs, adds ~0-2 ms), MTU reduction, and pathological cases where carrier NAT blocks UDP and you silently land on a TCP-ish DERP path with bad latency ([community setup guide noting T-Mobile issues](https://carthageelectronics.com/sunshine-moonlight-tailscale-game-streaming-guide/)).
- Strategic takeaway: users are voting that **connectivity should be a separate, general-purpose layer**. A "better Parsec" either builds Tailscale-quality traversal in (as Parsec/Steam did with brokers/SDR) or explicitly embraces the overlay-network pattern.

---

## 7. Synthesis: where the openings are

1. **Nobody open-source owns the connectivity layer.** Moonlight+Sunshine wins on latency and openness but punts on NAT traversal; Parsec and Valve win on "it just works" with proprietary brokers/relays. An open stack with built-in ICE-quality traversal + optional relays (or first-class Tailscale/WireGuard integration) closes the biggest UX gap.
2. **Virtual displays are table stakes now.** Dummy-plug pain is the top Sunshine complaint; Apollo/Wolf solved it with driver-level virtual displays that adopt the client's exact mode. Do this natively on all host platforms.
3. **Protocol-wise, the winning pattern is consistent** across Parsec BUD, GameStream, and Stadia: UDP + per-frame Reed-Solomon FEC (retransmission only as backstop), congestion control co-designed with the encoder (per-frame bitrate reaction, intra-refresh instead of I-frame spikes), zero receive-side buffering, and vsync-aware pacing. WebRTC gives you 80% of the plumbing but the jitter buffer, encoder opacity, and conferencing-tuned GCC cost you the last 10-15 ms — use it for reach (browser clients, signaling, TURN), not as the core engine.
4. **Codec direction:** AV1 (with RPR for glitchless resolution shifts, per GFN) + HDR10 + optional 4:4:4; HEVC as the compatibility tier; L4S/ECN awareness in congestion control is the forward-looking bet.
5. **Linux host is an underserved flank:** Parsec never shipped one; Wolf shows demand for containerized multi-user Linux hosting (the GFN-at-home shape).

## Sources

- https://github.com/moonlight-stream/moonlight-common-c
- https://moonlight-stream.org/
- https://github.com/moonlight-stream/moonlight-docs/wiki/Frequently-Asked-Questions
- https://github.com/LizardByte/Sunshine
- https://deepwiki.com/LizardByte/Sunshine/5.2-video-encoding-pipeline
- https://deepwiki.com/LizardByte/Sunshine/5.3-platform-specific-capture-implementations
- https://deepwiki.com/qiin2333/foundation-sunshine/7.3-udp-media-streaming
- https://techsngames.com/moonlight-pc-streaming-setup/
- https://github.com/LizardByte/Sunshine/issues/1853
- https://github.com/LizardByte/Sunshine/issues/2078
- https://github.com/LizardByte/Sunshine/issues/2187
- https://github.com/itsmikethetech/Virtual-Display-Driver/issues/36
- https://blog.thalium.re/posts/achieving-remote-code-execution-in-steam-remote-play/
- https://www.sstic.org/media/SSTIC2023/SSTIC-actes/bug_hunting_in_steam_remote_play/SSTIC2023-Article-bug_hunting_in_steam_remote_play-ricotta.pdf
- https://partner.steamgames.com/doc/features/multiplayer/steamdatagramrelay
- https://www.xda-developers.com/steam-remote-play-works-better-over-lan-than-people-realize/
- https://steamcommunity.com/groups/homestream/discussions/0/540731690900745009/
- https://steamcommunity.com/groups/homestream/discussions/0/3119298624516514270
- https://steamcommunity.com/app/353380/discussions/4/3874844033654329501/
- https://github.com/selkies-project/selkies
- https://selkies-project.github.io/selkies/
- https://github.com/m1k1o/neko
- https://chromium.googlesource.com/chromium/src/+/ef559ab66624bd19bd82baa17309bce4ef1c7aa4/agents/prompts/templates/crd.md
- https://support.google.com/chrome/a/answer/16364503?hl=en
- https://en.wikipedia.org/wiki/Chrome_Remote_Desktop
- https://www.pcgamesn.com/stadia/google-game-streaming-technology-latency-quality
- https://wccftech.com/google-stadia-may-eventually-become-more-responsive-than-local-machines-thanks-to-negative-latency/
- https://www.tomsguide.com/news/google-claims-stadia-will-outperform-consoles-by-predicting-players-moves
- https://arxiv.org/pdf/2009.09786
- https://developer.microsoft.com/en-us/games/articles/2025/03/gdc-2025-xbox-cloud-gaming-beta-expanding-your-reach-enhancing-your-game/
- https://clouddosage.com/how-xbox-is-quietly-fixing-xbox-cloud-gaming-latency/
- https://github.com/unknownskl/xbox-xcloud-player
- https://www.techpowerup.com/340035/nvidia-updates-geforce-now-with-rtx-5080-cinematic-streaming-new-low-latency-tech-and-more
- https://clouddosage.com/geforce-now-av1/
- https://webrtc.googlesource.com/src/+/main/docs/native-code/rtp-hdrext/playout-delay/README.md
- https://ieeexplore.ieee.org/document/11012899/
- https://dl.acm.org/doi/10.1145/3651863.3651881
- https://c3lab.poliba.it/images/6/65/Gcc-analysis.pdf
- https://flussonic.com/blog/news/transport-cc
- https://webrtchacks.com/real-time-video-processing-with-webcodecs-and-streams-processing-pipelines-part-1/
- https://www.red5.net/docs/development/webrtc/insertable-streams/
- https://learn.microsoft.com/en-us/azure/virtual-desktop/graphics-enable-gpu-acceleration
- https://learn.microsoft.com/en-us/azure/virtual-desktop/graphics-encoding
- https://learn.microsoft.com/en-us/training/modules/implement-manage-networking-azure-virtual-desktop/8-plan-implement-remote-desktop-protocol-shortpath
- https://superrendersfarm.com/article/moonlight-parsec-rdp-remote-desktop-gpu-rendering-2026
- https://docs.aws.amazon.com/dcv/latest/adminguide/enable-quic.html
- https://anyware.hp.com/products/hp-anyware/2025.06/documentation/session-planning-guide/pcoip-session-planning
- https://anyware.hp.com/products/hp-anyware/2025.03/documentation/architecture-guide/about-pcoip-technology
- https://docs.citrix.com/en-us/citrix-virtual-apps-desktops/2303/hdx-transport/adaptive-transport.html
- https://community.citrix.com/topic/226465-hdx-adaptive-transportedt-over-udp-high-latencyrtt-vs-tcp/
- https://datatracker.ietf.org/doc/html/rfc6143
- https://en.wikipedia.org/wiki/RFB_protocol
- https://linuxvox.com/blog/linux-screen-desktop-video-capture-over-network-and-vnc-framerate/
- https://en.wikipedia.org/wiki/Rainway
- https://docs.rainway.com/docs/what-is-rainway
- https://grokipedia.com/page/Rainway
- https://github.com/ClassicOldSong/Apollo
- https://github.com/DuoStream/Duo
- https://github.com/games-on-whales/wolf
- https://parsec.app/technology
- https://parsec.app/blog/a-networking-protocol-built-for-the-lowest-latency-interactive-game-streaming-1fd5a03a6007
- https://support.parsec.app/hc/en-us/articles/32381568346644-Hardware-and-Software-Compatibility
- https://support.parsec.app/hc/en-us/articles/32381178803604-VDD-Overview-Prerequisites-and-Installation
- https://techcrunch.com/2021/08/10/unity-to-acquire-parsec-in-its-biggest-acquisition-to-date/amp/
- https://tailscale.com/blog/how-tailscale-works
- https://tailscale.com/blog/nat-traversal-improvements-pt-1
- https://tailscale.com/docs/reference/connection-types
- https://gist.github.com/andygrundman/b46ebfff67d340fa5556955b226446b2
- https://forum.level1techs.com/t/sunshine-server-setup-with-tailscale-and-moonlight-surface-pro-client-config/205362
- https://carthageelectronics.com/sunshine-moonlight-tailscale-game-streaming-guide/
