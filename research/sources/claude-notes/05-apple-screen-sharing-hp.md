# 05 — Apple Screen Sharing "High Performance" mode: what it is, how it works, where it's weak

Research date: 2026-09-25. Purpose: understand Sunna's v1 benchmark (macOS Screen Sharing HP mode, M1 Air <-> M2 Air over Tailscale).
Legend: [DOC] = Apple-documented fact; [COMMUNITY] = user/third-party claim; [INFER] = my inference; (UNVERIFIED) = not confirmed by a source I read.

## TL;DR
- The "goldmine" = **iShareScreen** (https://github.com/renegadelink/iShareScreen, AGPL) and its RFC-style RE spec `docs/apple_vnc_rfc.md`. HP = Apple RFB `003.889` over TCP 5900 (SRP/RSA auth, AES-128-CBC + SHA-1 control records, input, clipboard, local-cursor shapes, virtual-display config) **plus an AVConference (FaceTime stack) media session: HEVC RExt 4:4:4 8-bit (H.264 4:2:0 fallback bank) over SRTP AES-256-CM/UDP 5901, split into 4 horizontal-strip SSRCs with cross-strip refs; AAC-ELD stereo audio + RTCP on UDP 5900**; loss recovery via FIR, LTR acks and NACK retransmit; no FEC seen.
- Apple docs: Apple silicon + macOS 14+ both ends, UDP 5900-5902, 75 Mbps per 4K display, wired + low latency recommended, 1-2 virtual displays up to 4K / 1080p HiDPI, 30/60 fps, 4:4:4, HDR reference mode, stereo audio, one HP session per Mac.
- Weak spots Sunna can target: Wi-Fi/Internet/VPN paths (Apple designs for wired LAN), grey-screen failures when the media path breaks, TCP-borne input, encoder restart + IDR on every resize, fixed ports, 60 fps / 4K caps, dated crypto + a 2026 pre-auth bypass (fixed in 26.6.1). No public click-to-photon numbers exist for HP; Sunna should publish its own (Sec 5).
- Biggest open question for Sunna: can public VideoToolbox encode HEVC 4:4:4 on M1/M2 like ScreensharingAgent does? (Sec 4.)

## 1. Official Apple documentation

Sources:
- Apple Remote Desktop / Screen Sharing guide, "Use High Performance screen sharing": https://support.apple.com/guide/remote-desktop/use-high-performance-screen-sharing-apdf8e09f5a9/mac
- Mac User Guide, "Screen sharing type options on Mac": https://support.apple.com/en-gb/guide/mac-help/mchl1883115d/mac (related: "Change screen sharing connection settings" https://support.apple.com/en-gb/guide/mac-help/mchl67d5398b/27/mac/27 ; "Turn Mac screen sharing on or off" https://support.apple.com/en-gb/guide/mac-help/mh11848/27/mac/27 ; "Manage screen sharing connections" https://support.apple.com/en-gb/guide/mac-help/mchl89584923/27/mac/27)

[DOC] facts:
- Both Macs must be **Apple silicon** and **macOS Sonoma 14 or later**.
- Both Macs must reach each other over **UDP 5900, 5901, 5902** (plus the usual TCP 5900 for VNC/RFB control -- TCP 5900 is standard Screen Sharing, not repeated in the HP page).
- Bandwidth: "**75 megabits per second for a single 4K display**" and "consistent low network latency"; **wired network recommended**.
- Max virtual display size **4K (3840x2160) or 1920x1080 HiDPI**. **One or two virtual displays** (chosen at connect time). Dynamic resolution to match the viewer window.
- Supports **stereo audio, HDR video reference mode (requires the "HDR Video (P3-ST 2084)" preset in Displays settings), 4:4:4 chroma subsampling, 30 or 60 fps, "low latency"**.
- Standard mode, by contrast: "Adapt resolution and frame rate to network conditions."
- **Only one HP session per Mac at a time.**
- When the viewer authenticates as the same user logged in on the host, the host's **hardware displays are blanked** for privacy (the session uses virtual displays, not the physical panel).
- Not documented by Apple (searched pages): Pro Display XDR specifics, latency numbers, codec names, what happens over VPN/Internet, Wi-Fi guidance beyond "wired recommended". The "Pro Display XDR" association comes from Apple's WWDC23/Sonoma marketing (UNVERIFIED in this session -- not re-fetched).

[COMMUNITY] press coverage restating the above:
- Six Colors (June 2026): Standard = VNC pixel mirror; HP = "Apple's secret sauce on top", one or two virtual displays each with independent resolution; "75 Mbps per 4K display ... requires fast Wi-Fi with a gigabit-or-faster mesh or wired backbone". https://sixcolors.com/post/2026/06/high-performance-mode-allows-sharing-another-macs-display-as-if-your-own/
- 9to5Mac (Feb 2026) tested over gigabit Ethernet via Thunderbolt hub: "almost to the point where it feels like I'm controlling the Mac natively", used with Final Cut Pro / Logic Pro, stereo audio low enough latency for Logic. No numbers. https://9to5mac.com/2026/02/05/how-to-use-macos-high-performance-screen-sharing-for-lower-latency-and-better-color-video/
- iBoysoft explainer (secondary): https://iboysoft.com/wiki/high-performance-screen-sharing.html

[INFER] Implications for the owner's test (M1 Air <-> M2 Air, Tailscale, likely Wi-Fi): this is squarely outside Apple's stated envelope (wired, consistent low latency). "Not even that good" is consistent with HP being an LAN/wired feature whose RTC rate control backs off hard on Wi-Fi jitter. Also UDP 5902 is listed by Apple but the RE only saw 5900/5901 in use -- 5902 is likely the second video stream / second virtual display (UNVERIFIED).

## 2. Reverse engineering: protocol, codec, transport

### 2.0 The "goldmine": iShareScreen + its reverse-engineered RFC
- Repo: https://github.com/renegadelink/iShareScreen (fork: https://github.com/alakangas/iShareScreen). AGPL-3.0-or-later, Python, ~100 commits, last commit I cloned 2026-07-26 (9ab40d3). Cross-platform (macOS/Linux/Windows) *client* for HP mode: wgpu window, VideoToolbox / VA-API / libav decode, AAC-ELD via libfdk-aac, bidirectional clipboard, TUI showing per-tile fps/loss/throughput. Also ships a pcap dissector (`tools/dissector/`: SRTP, HEVC depay, RFB modes).
- Spec: `docs/apple_vnc_rfc.md` (1110 lines, RFC-style "Apple VNC High-Performance Extension"), https://github.com/renegadelink/iShareScreen/blob/main/docs/apple_vnc_rfc.md . Derived from pcaps, runtime traces and static disassembly of `screensharingd`, `ScreensharingAgent`, `AppleVNCServer`, `ScreenSharing.framework`, `AVConference`. Everything below in 2.1-2.8 is [COMMUNITY-RE] from that doc/code unless tagged otherwise; it is high-quality (byte-exact captures + disassembly addresses) but not Apple-confirmed.

### 2.1 Architecture in one paragraph
HP mode = classic Apple RFB (`RFB 003.889`) on **TCP 5900** for auth + control + cursor + clipboard + metadata, plus an **AVConference (FaceTime media stack) session** for the pixels: **HEVC over SRTP/UDP on 5901** and **AAC-ELD audio + RTCP on UDP 5900**. The screen-sharing code treats the per-stream offer blobs as opaque and hands them to AVConference's `AVCMediaStreamNegotiator` (same negotiator family used for FaceTime/Continuity). [INFER] So HP's congestion control, rate adaptation, jitter buffer, LTR/FIR recovery all come from AVConference/VideoConference (`VCAudioTierPicker`, `SSUDPSender`) -- a real-time-comms stack tuned for calls, not a game-streaming stack.
- Server-side processes: `screensharingd` (RFB daemon, auth, record layer), `ScreensharingAgent` (per-user agent: creates the virtual display via SkyLight `SLVirtualDisplaySettings` / `_SLSDisplaySetDynamicGeometryEnabled`, captures, drives AVConference encoder), viewer side `Screen Sharing.app` + private `ScreenSharing.framework` (`SSSession`).
- "High performance" is a **session property negotiated after auth**, not an auth type. Three session classes: framebuffer virtual-display session; HP with media-init but still framebuffer path; "Adaptive media" (HEVC/AAC over SRTP). Switch to compressed media happens only after the `0x1c` MediaStreamOptions offer/answer.

### 2.2 Handshake / auth / control channel crypto
- Version `RFB 003.889`. Security types advertised `{30, 33, 36, 35}`:
  - 30 = Diffie-Hellman (fixed 1024-bit group, g=2, key=MD5(shared), creds AES-128-CBC zero-IV -- legacy/weak).
  - 33 = RSA1/RSA-SRP: RSA-2048 PKCS#1v1.5 envelope (server keypair persisted in keychain, access group `com.apple.ARDAgent`), then SRP-6a 4096-bit, g=5, SHA-512, PBKDF2-HMAC-SHA512 password preprocessing, options string `mda=SHA-512,replay_detection,conf+int=ChaCha20-Poly1305,kdf=SALTED-SHA512-PBKDF2`. Only branch byte-confirmed by capture.
  - 35 = Kerberos GSS-API (server generates random AES key, sends over GSS).
  - 36 = direct SRP (cleartext username envelope).
- After ClientInit(0xC1)/ServerInit, cleartext Apple prelude: ViewerInfo `0x21` (256-bit capability bitmap), SetEncryption `0x12`, optional SetMode `0x0a`. Server rekeys with pseudo-encoding `0x44f` -> **AES-128-CBC record layer, continuous CBC chain per direction, plain SHA-1(seq||plaintext) trailer (not HMAC)**. The ChaCha20-Poly1305 in the SRP options is NOT used for the record layer. Security weaknesses: no server auth beyond TOFU of RSA key, downgradable to type 30, CBC + SHA-1 MAC-then-encrypt-ish.
- Input: standard RFB key/pointer events inside the record layer, or `0x10` EncryptedInputEvent (16-byte AES-ECB block). Keysyms are X11 style; `0x1a`/`0x455` sync keyboard input source (e.g. `com.apple.keylayout.ABC`) and a secure-event-input flag. [INFER] Input rides TCP -> head-of-line blocking under loss; Sunna can beat this with unreliable/QUIC-datagram input or at least a separate stream.
- Clipboard: `0x15` AutoPasteboard on/off, server announces change via `0x14 cmd=2`, client fetches with `0x0b`, server sends `0x1f` zlib-compressed multi-flavor pasteboard archive (UTIs + aliases). Heartbeat `0x14 cmd=12` ~2.1 s.
- Cursor: `0x450` shape-only cursor with STORE/SELECT cache (BGRA + separate alpha plane, zlib), client draws cursor locally at its own pointer position (i.e. **local cursor rendering** -> cursor latency hidden). Must re-arm with `0x09` AutoFrameBufferUpdate + `0x03` at every `0x451` layout change or cursor freezes (a native bug class).

### 2.3 Virtual display / resolution matching
- Client sends `0x1d` SetDisplayConfiguration: display descriptor with physical mm size, max w/h, mode table (width/height = backing, scaled_width/height = logical, refresh f64 e.g. 60.0, flag bit0 = HDR), `display_flags` bit0 = dynamic resolution, `display_type` 4 = virtual display. ScreensharingAgent `CreateVirtualDisplay` calls SkyLight (`SLVirtualDisplaySettings`, `_SLSDisplaySetDynamicGeometryEnabled`) [INFER: same private machinery as public-ish `CGVirtualDisplay`].
- Server answers with `0x451` AppleDisplayLayout (scaled + backing geometry, e.g. 1920x1080 scaled / 3840x2160 backing = HiDPI 2x). Backing canvas capped at host-dependent ceiling (observed 3840x2160) -- matches Apple doc's 4K cap.
- Live resize: viewer sends a new dynamic `0x1d`; server replies `0x451`; viewer must re-send `0x1c` media offer; server restarts the HEVC encoder with a new 4-SSRC group, new VPS/SPS/PPS and an IDR. No TCP reconnect. [INFER] Every window resize = encoder restart + full IDR burst -> visible hitch; Sunna can do better with an encoder that handles resolution changes cheaply, or by scaling during drags and committing after release.
- Legacy URL params in viewer: `quality`, `control`, `numVirtualDisplays`, `displayID`, `encrypt`, `auth`, `hdr`, `panning`.
- Legacy (non-HP) codecs still on the TCP path: zlib, Apple 0x3e8/0x3e9/0x3ea (4-bit / 8-bit YCoCg dither / RGB565) and `0x3f3` Multi-Variant Scaled (per-tile DCT). `0x3f2` announces media capability.

### 2.4 Media path (the part that matters for Sunna)
- `0x1c` MediaStreamOptions: fixed binary struct with flags (bit0 stream-1 60 fps, bit1 stream-2 60 fps, bit2 do-not-send-cursor, bit3 AVC client-name `RemoteDesktopScreenSharing` vs `AppleRemoteDesktop`), one UUID (session + CallID), **three streams (audio, video1, video2)** each with two 46-byte SRTP key blobs (client->server, server->client), and opaque zlib-protobuf AVConference offer blobs. NOTE: the doc says flags are host-endian (LE) while the code comment says empirically big-endian `00 00 00 07` is what works -- internal inconsistency in the RE; trust runtime.
- Keys for media are generated by the client and delivered inside the AES-CBC control channel -> media secrecy == control-channel secrecy.
- SRTP: **AES-256-CM + HMAC-SHA1-80**, RFC 3711 KDF, per-SSRC ROC. No SFrame.
- **Video codec: HEVC Range Extensions 4:4:4 8-bit** (decoder outputs `444YpCbCr8BiPlanarFullRange`). The native offer carries **two codec banks, HEVC 4:4:4 + an H.264 4:2:0 fallback** (Apple's byte-identical order), server picks; iShareScreen can force H.264 4:2:0 (with a weird inverted bank mapping). HDR is a per-mode flag.
- **Tiling: frame split into 4 horizontal strips (e.g. 3840x544 for 2160p), each a separate RTP SSRC, but one HEVC bitstream with cross-tile references** -> must be decoded by a single decoder; IDR only on base SSRC. `tilesPerFrame` is an offer field (field 6); 1 is allowed ("iOS CoreDevice defaults to 1"). [INFER] Strips let the encoder/packetizer pipeline slices (lower latency, parallel encode on the media engine) -- Sunna could do similar with HEVC/H.264 slices.
- RTP payload: Apple variant of RFC 7798 with DONL on every single-NAL/AP/FU packet (non-standard, breaks stock depacketizers).
- Audio: RTP PT 101, **AAC-ELD (SBR)**, stereo; screen-share audio has effectively one tier ~21-24 kbps on the wire (native offer requests 24191 bps); a sub-5 kbps request disables audio. Audio offer also contains a tier list with network bitrate caps 6/20/40/60 Mbps plus "HP tiers" 75 Mbps and 100 Mbps (iShareScreen comment: "75M and 100M are HP tiers Sequoia's stock AVConference omits") -- [INFER] these look like the overall session bandwidth caps; Apple's "75 Mbps for one 4K display" doc number matches the 75M tier.
- Observed bandwidth: iShareScreen comments cite **~300 Mbps / ~28k pps for a HiDPI 2x stream** under load (bursty per-frame/IDR bursts; needed a 16k-packet queue) and tests with a 60 Mbps "noise load" page. Cursor baked into video costs ~0.5-1 Mbps extra. (COMMUNITY, code comments; UNVERIFIED against a controlled measurement.)
- Loss recovery: RTCP rtcp-muxed on each media port. Native client sends RR + SDES, **legacy FIR (PT 192, RFC 2032)** for keyframes, and an **APP (PT 204, subtype 5) LTR-style per-frame ack**. Server also accepts AVPF FIR/PLI/**generic NACK**, and **does retransmit on NACK** (iShareScreen waits up to 75 ms for H.264 repairs; "LAN retransmits normally arrive within a few ms"). Long-Term Reference Pictures (`LTR;` capability) let the server re-anchor P-frames on an acked reference instead of sending an IDR. No FEC observed in the RE (UNVERIFIED -- AVConference does have FEC for FaceTime, but nothing in this doc shows it for screen sharing).
- Fallback: on `0x1c` failure the session stays on the framebuffer (TCP) path -- i.e. the "HP greyed-out / degraded" mode is literally the old VNC-style codecs.
- Session transitions (login window, lock, fast-user-switch) cause agent handoff, new SSRC groups, and cursor/stream re-arming -- iShareScreen has lots of recovery logic (stall watchdog 5 s, SSRC adoption, grey-patch "force IDR" key) -- [INFER] these are exactly the places native HP glitches too.

### 2.5 What this means for Sunna (INFER)
1. HP's pixels already travel over UDP with HEVC 4:4:4 from the Apple media engine at up to ~75-100 Mbps caps, 60 fps, local cursor. Beating it on *image quality per bit* is hard on Mac-to-Mac; beating it on **latency consistency, loss/jitter handling over the internet/Tailscale, resize behaviour, input path (TCP vs datagram), and session robustness** is realistic.
2. AVConference is an RTC stack: its rate controller/jitter buffer target conversational latency (tens of ms of buffer) and ramps conservatively; on a Tailscale path with MTU ~1280 and DERP relays it may shrink bitrate/quality aggressively. Measure it (Section 5).
3. The 4-strip + single-decoder design + LTR acks is a good idea to copy (slice-level pipelining, ack-based reference selection instead of IDR storms -- relevant to Sunna's recent keyframe re-request work).

## 3. Weaknesses, user reports, comparisons

### 3.1 Documented / reported failure modes
- **Grey screen with working cursor/input** in HP mode (intermittent), while Standard works: M1 MBP on Wi-Fi -> M1 Mac Studio wired, Sonoma 14.1.1. No resolution. https://discussions.apple.com/thread/255291758 [COMMUNITY]. Same symptom ("only a mouse cursor appears, clicks still work") surfaced in search summaries. [INFER] Consistent with the RE: media path (UDP/HEVC) failed or lost its reference chain while the TCP control path (cursor 0x450, input) kept working; iShareScreen even has a "force IDR (f)" key for "the rare gray patch the auto-recovery doesn't catch".
- **HP works only one way** (e.g. Mac Studio -> MBP fine, reverse gives grey screen): MacRumors thread https://forums.macrumors.com/threads/high-performance-screen-sharing-mode-works-only-one-way.2431136/ (title + search summary only; not read in full).
- **Two virtual displays in separate windows: clicks on the 2nd screen land on the 1st**, unusable: https://forums.macrumors.com/threads/screen-sharing-high-performance-mode-2-screen-not-working-with-separate-windows.2430426/ (search summary). General thread: https://forums.macrumors.com/threads/high-performance-screen-sharing.2430690/
- **macOS 26 beta 2 regression**: "This Mac was unable to start a High Performance connection to the Mac mini" (+ "Window Server failed" in Standard), M1 mini on 26b2 <-> M2 MBP on 15.5; no replies. https://developer.apple.com/forums/thread/789695
- Eclectic Light: HP "may not work at all in some circumstances"; recommends third-party VNC (Screens 5) for Internet use. https://eclecticlight.co/2024/02/13/share-your-macs-display-and-control/
- **VPN/Tailscale**: sing-box's Tailscale endpoint breaks HP (Standard works) on macOS 26.5 while the *official Tailscale client works* in identical conditions, direct P2P path, iperf3 identical. Open bug, no root cause. https://github.com/SagerNet/sing-box/issues/4155 [INFER] likely MTU/fragmentation or a UDP-port/5900-reuse (HP sends audio+RTCP from the *same* port number 5900 as TCP control and uses fixed well-known ports, which breaks some NAT/proxy implementations). Also https://github.com/hwdsl2/docker-ipsec-vpn-server/issues/416 (question whether HP works over IPsec VPN).
- **Security**: a pre-auth Screen Sharing authentication bypass ("improper state management during authentication", port 5900) fixed in **macOS Tahoe 26.6.1 / 15.7.9 / 14.8.9** (Aug 2026), reportedly exploited in the wild. Macworld https://www.macworld.com/article/3214311/macos-tahoe-26-6-1-fixed-a-pretty-major-screen-sharing-flaw.html ; MacRumors https://www.macrumors.com/2026/08/17/macos-screen-sharing-flaw-exploited/ ; CVE-2026-65400 per https://thecybersecguru.com/news/cve-2026-65400-macos-screen-sharing-authentication-bypass/ (CVE number UNVERIFIED -- secondary source). Plus the RE's structural weaknesses (Sec 2.2: downgradable auth, SHA-1 non-HMAC integrity, no server identity). **Sunna pitch**: modern QUIC/TLS 1.3 with pinned keys is a genuine differentiator.

### 3.2 Structural weaknesses (INFER from the RE + docs)
1. **Wired-LAN design point.** Apple itself says 75 Mbps/4K + consistent low latency + wired. The two Airs are Wi-Fi-only; over Tailscale, WireGuard adds ~60-80 B overhead and a 1280-byte utun MTU. The AVConference rate controller (built for calls) will back off quality on jitter/loss. Standard mode "adapts resolution and frame rate"; HP apparently does not degrade gracefully (fall back = grey screen / "unable to start").
2. **Wi-Fi AWDL stalls.** macOS AWDL (AirDrop/AirPlay/Universal Control/Sidecar discovery) parks the Wi-Fi radio causing ~50-100 ms stalls ~once per second; Location Services scans every ~5 min. Affects HP *and* Sunna. Workaround `sudo ifconfig awdl0 down` (macOS re-enables it on Ventura+). https://gist.github.com/kouwei32/c101be682fc2e433e153ea131798caec [COMMUNITY]. **Benchmark must control for this.**
3. **Input and cursor-shape on TCP** (RFB events inside an AES-CBC record stream) -> head-of-line blocking behind large control/clipboard messages and TCP retransmits on lossy Wi-Fi.
4. **Every resize = encoder restart + new SSRC group + IDR** (Sec 2.3). Session transitions (lock/login/fast-user-switch) = agent handoff + stream re-arm; a known source of grey screens/frozen cursor.
5. **Recovery relies on FIR (full keyframe) + LTR acks + NACK retransmit**; no FEC seen. On a long-RTT path (Tailscale via DERP, 50-150 ms), NACK repair costs >=1 RTT and IDR bursts at 4K 4:4:4 are large (iShareScreen had to buffer ~0.6 s of packets for per-frame/IDR bursts at ~300 Mbps peaks) -> latency spikes / freezes.
6. **Hard limits:** 4K (1080p HiDPI) per virtual display, max 2 displays, 60 fps max, one HP session per Mac, host panels blanked when same user, Apple-silicon + macOS 14+ only, Mac viewer only (no iPad/iPhone/Linux/Windows viewer from Apple).
7. Fixed well-known UDP ports 5900-5902 -> only one session, awkward through NAT/port-forwarding and VPNs that remap ports.

### 3.3 Comparisons with other tools (numbers are scarce -- treat as UNVERIFIED)
- No controlled published click-to-photon measurements of Apple HP were found in this pass (9to5Mac: "almost native" on gigabit Ethernet; no numbers). **This is a gap Sunna can fill with its own benchmark (Sec 5).**
- Jump Desktop Fluid: claims 60 fps at ~1/10 the bandwidth of RDP/VNC; Jump's own "very high performance scenarios" guide: https://support.jumpdesktop.com/hc/en-us/articles/360047204251-Fluid-Remote-Desktop-Recommendations-for-very-high-performance-scenarios ; https://support.jumpdesktop.com/hc/en-us/articles/216423983-General-Fluid-Remote-Desktop (not read in detail).
- Moonlight "sub-30 ms on LAN", Parsec "4K60 near-zero latency" -- marketing/secondary claims from search summaries (UNVERIFIED). Sidecar "14-18 ms input lag over Wi-Fi" appeared in a low-quality search summary -- UNVERIFIED, likely fabricated by an SEO page; do not cite.
- Qualitative community consensus (UNVERIFIED): HP beats Standard VNC decisively on LAN; Parsec/Moonlight feel better for games (lower buffering, 4:2:0 at high fps, 120+ fps options); HP wins on text fidelity (4:4:4) and macOS integration (virtual display, audio, clipboard, no install). Nothing Apple ships works well over the Internet.
- A 2026 overview comparing Mac remote-desktop tools: https://macky.dev/blog/remote-desktop-mac-options (not read).

## 4. Related Apple transports (what they reveal about Apple's stack)
(Short; mostly INFER / UNVERIFIED -- limited budget spent here.)
- **Common core = AVConference / avconferenced (FaceTime media stack)**. The HP RE shows screen-sharing video offers are built by `AVCMediaStreamNegotiator` and audio tiers picked by `VCAudioTierPicker` (Sec 2.4). iShareScreen code notes "iOS CoreDevice defaults to 1" tile-per-frame -- i.e. the same screen-share media negotiator is also used by CoreDevice (Xcode/device mirroring) [COMMUNITY-RE]. Sidecar logs reference avconferenced (search summary; UNVERIFIED). [INFER] Apple has one real-time screen-media pipeline (media-engine HEVC, SRTP, RTCP FIR/LTR, AAC-ELD) reused across FaceTime SharePlay screen share, Sidecar, iPhone Mirroring, CoreDevice, and Screen Sharing HP; the products differ in transport (Wi-Fi infra vs AWDL peer-to-peer vs USB) and control plane (RFB vs Continuity/IDS/RemotePairing).
- **Sidecar** (Catalina+): Mac -> iPad extended display over AWDL/USB, HEVC; iPad renders Apple Pencil/touch input back. Demonstrates Apple's virtual-display path (same class of virtual display as HP).
- **Universal Control** (Monterey+): input only (keyboard/mouse/drag-drop), no video; Continuity over BLE discovery + AWDL/Wi-Fi. Shows Apple separates low-latency input from media.
- **AirPlay to Mac** (Monterey+): AirPlay 2 receiver; screen mirroring is H.264/HEVC over TCP-ish RTSP-negotiated streams with FairPlay pairing, NTP/PTP clocking; buffering tuned for AV sync, not interactivity (higher latency). RE example: https://github.com/PoleTransformer/Airplay_Experiments
- **iPhone Mirroring** (macOS 15 Sequoia): Mac views/controls iPhone; Continuity-authenticated, AWDL/Wi-Fi; [INFER] likely the AVConference screen-share path with tilesPerFrame=1 (UNVERIFIED).
- Open-source Sidecar alternative for design reference: https://github.com/zhangsan-nb/opendisplay (H.264, HiDPI, USB/Wi-Fi).
- **Takeaway for Sunna**: Apple's advantage is media-engine HEVC 4:4:4 + tightly integrated virtual displays; its weakness is an RTC-style transport tuned for LAN/call conditions and a legacy RFB/TCP control plane. Sunna (Rust + QUIC) competes mainly on transport. **Open question (UNVERIFIED, important):** whether *public* VideoToolbox can encode HEVC RExt 4:4:4 8-bit on M1/M2 media engines the way ScreensharingAgent does (Sunna's codec crate currently has no 4:4:4 path; `kVTProfileLevel_HEVC_Main42210_AutoLevel` is 4:2:2 10-bit and may be M1 Pro/Max+ only). Test: create a VTCompressionSession fed `kCVPixelFormatType_444YpCbCr8BiPlanarFullRange` buffers, then parse the output SPS `chroma_format_idc` (3 = 4:4:4). If VT silently converts to 4:2:0, HP has a text-fidelity edge Sunna must answer differently (higher-res 4:2:0, chroma-upsampling-aware rendering, or lossless text/static-region refinement).

## 5. Reverse-engineering procedure for the owner's M1/M2 Airs + fair click-to-photon benchmark

Paths/subsystem names below are from memory/RE, not re-verified in this pass -- discover the real ones with the first commands. All read-only/observational. Run on **macOS 26.6.1+** (auth-bypass fix, Sec 3.1).

### 5.1 Find the processes and binaries
```sh
# during an HP session, on host and on viewer:
ps -axo pid,ppid,user,comm | grep -iE 'screensharing|ScreenSharing|avconf|ARDAgent|VNC'
# expected (UNVERIFIED paths): host: screensharingd (root), ScreensharingAgent (per user);
# viewer: "/System/Library/CoreServices/Applications/Screen Sharing.app"
for b in $(ps -axo comm | grep -iE 'screensharing'); do echo "== $b"; otool -L "$b"; done
# frameworks live in the dyld shared cache, so otool -L gives names only; to inspect them
# use `ipsw dyld extract` / `dyld_info` or Hopper on the extracted image.
strings -a "$(ps -axo comm | grep -m1 screensharingd)" | grep -iE 'HighPerformance|virtual ?display|AVC|HEVC|444|SRTP|5901|Adaptive' | sort -u | head -100
# Which processes map AVConference / VideoToolbox during HP?
sudo vmmap $(pgrep -f ScreensharingAgent) | grep -iE 'AVConference|VideoToolbox|SkyLight|ScreenSharing' | sort -u
sudo lsof -p $(pgrep -f ScreensharingAgent) -i -n -P      # sockets: expect UDP *:5900, *:5901 (+5902 for 2nd display?)
pgrep -lf avconferenced                                    # is the out-of-process daemon used, or in-process framework?
```
Compare with the iShareScreen findings (Sec 2): RFB 003.889 on TCP 5900; HEVC on UDP 5901; AAC-ELD + RTCP on UDP 5900.

### 5.2 Logging
```sh
# discover subsystems first (names UNVERIFIED):
log show --last 2m --info --debug --style ndjson \
  --predicate 'process CONTAINS[c] "screensharing" OR process == "avconferenced" OR process == "Screen Sharing"' \
  | jq -r '.subsystem + "  " + .category' | sort | uniq -c | sort -rn | head -40
# then stream live while connecting / resizing / toggling Wi-Fi:
log stream --info --debug --predicate 'process CONTAINS[c] "screensharing" OR subsystem CONTAINS[c] "avconference" OR subsystem CONTAINS[c] "screensharing" OR eventMessage CONTAINS[c] "virtual display"'
```
Look for: codec/profile selection, target bitrate changes (rate controller), FIR/keyframe requests, LTR, "High Performance" fallback reasons, virtual-display creation, `SetMediaStreamConfiguration`.

### 5.3 Virtual display and settings checks
```sh
system_profiler SPDisplaysDataType          # on host during HP: expect an extra virtual display (resolution/refresh)
ioreg -lw0 | grep -iE 'IODisplay|virtual' | head
defaults read com.apple.ScreenSharing 2>/dev/null   # viewer prefs (quality, HP choice, displays)
defaults read /Library/Preferences/com.apple.RemoteManagement 2>/dev/null
```
Also note which Displays preset is active (HDR needs "HDR Video (P3-ST 2084)").

### 5.4 Network: bitrate, packet sizes, MTU, path
```sh
tailscale status; tailscale ping <peer>            # direct vs DERP, RTT
ifconfig | grep -A1 utun | grep mtu                 # Tailscale utun MTU (usually 1280)
sudo tcpdump -i <utunN> -n -w hp_tailnet.pcap 'port 5900 or port 5901 or port 5902'
sudo tcpdump -i en0 -n -w hp_outer.pcap 'udp port 41641'   # WireGuard outer packets (Tailscale default port)
tshark -r hp_tailnet.pcap -q -z io,stat,1,"udp.port==5901","udp.port==5900","tcp.port==5900"   # per-second bytes
tshark -r hp_tailnet.pcap -T fields -e frame.len -Y 'udp.port==5901' | sort -n | uniq -c | tail   # max packet size -> fragmentation?
tshark -r hp_tailnet.pcap -Y 'ip.flags.mf==1 || ip.frag_offset>0' | wc -l                          # IP fragments present?
sudo nettop -m udp -P -d -L 0 -J bytes_in,bytes_out -s 1 | grep -iE 'screenshar|avconf'
```
- In Wireshark: Decode As RTP on 5901/5900 -> RTP stream analysis (loss, jitter, seq), count SSRCs (expect 4 video SSRCs), see RTCP FIR (PT 192) / APP (PT 204) / NACK frequency. Payload is SRTP-encrypted (keys are client-generated and sent in the encrypted control channel), so decode of pixels needs iShareScreen's own client/dissector (`tools/dissector/`), but sizes/timing/RTCP headers are enough for rate-control analysis.
- Run each capture in three conditions: (a) same LAN, direct IPs (no Tailscale), (b) Tailscale direct, (c) Tailscale forced via DERP (block direct UDP), and with Wi-Fi vs a USB-C Ethernet adapter if available. Watch how bitrate/fps respond to added delay/loss using **Network Link Conditioner** (Additional Tools for Xcode) on the viewer.
- Wi-Fi hygiene: `sudo wdutil info` (channel, band, RSSI), test once with `sudo ifconfig awdl0 down` to see if periodic ~1 s stalls vanish (Sec 3.2).
- Optional deep dive: run iShareScreen (`iss`) from Linux/Mac against the host -- its TUI shows per-tile fps, loss, throughput, UDP queue health; it's the easiest way to see HP's raw stream behaviour, and its pcap recorder + dissector can decode captures when it is the client.

### 5.5 Fair click-to-photon benchmark (HP vs Sunna)
Goal: same machines, same content, same path, same display config, many samples, report distributions.
1. **Rig (preferred, automated):** a USB-HID microcontroller (Arduino Leonardo / Pro Micro / RP2040) plugged into the *viewer* acts as mouse; a photodiode (or light sensor module) taped on the viewer screen over a test region. Firmware: send click, start timer, wait for luminance change, record us; randomize inter-trial delay 300-700 ms (de-correlates from 60 Hz vsync). >=200 trials per condition. (Same idea as NVIDIA LDAT / open-source "latency tester" builds.)
   **Fallback rig:** iPhone 240 fps slo-mo filming the mouse button and viewer screen in one frame (4.2 ms resolution), >=30 trials, count frames.
2. **Remote test app (on host):** a full-screen page/app that flips a black square to white on `mousedown` (and back on the next), no animations. Use the same app for HP and Sunna. Also a keyboard variant (keydown -> flip) since HP sends input over TCP.
3. **Baselines to subtract/report:** (a) local click-to-photon on the viewer running the same test app locally (the Air's own input+display pipeline, ~60 Hz panel); (b) host-local isn't measurable in HP when host panels are blanked -- skip or use a different user.
4. **Control variables:** same virtual-display size & HiDPI (e.g. 1470x956@2x or 1920x1080@2x) and 60 Hz for both; both Airs on AC power, Low Power Mode off, same Wi-Fi band/channel & distance, AWDL state recorded, Bluetooth state recorded, lid open, no other traffic; same network path (LAN-direct / Tailscale-direct / DERP) tested separately; HP set to High Performance, 1 display; Sunna at comparable bitrate cap (or report both at "their defaults" and at "matched ~75 Mbps"). Alternate HP/Sunna runs (ABAB) to cancel drift.
5. **Metrics per condition:** click-to-photon median / p95 / p99 / max; % trials > 100 ms; plus (i) **glass-to-glass smoothness**: play a 60 fps frame-counter video or scroll testufo.com-style pattern on host and film/record viewer to count dropped/duplicated frames per 10 s; (ii) bitrate (from 5.4 captures); (iii) text fidelity: screenshot of a colored-text test page on viewer, compare to host (4:4:4 vs 4:2:0 fringing); (iv) resilience: induce 1% / 5% loss and 50 ms / 100 ms added RTT with Network Link Conditioner, and a 2 s Wi-Fi drop -> time to recover a clean image and whether the session survives; (v) resize: time + visual hitch when resizing viewer window (HP restarts encoder + IDR each time); (vi) time-to-first-frame from connect.
6. **Reporting:** publish raw CSVs + rig description; state macOS versions (host/viewer), hardware (M1 Air host / M2 Air viewer, then swapped), network path, and settings. This gives the "beat HP" claim credibility because there are no good public numbers for HP today.
