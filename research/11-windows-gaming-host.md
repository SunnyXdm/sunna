# 11. A Windows gaming host that can beat Parsec

Plan written 2026-10-11 from four research documents in [sources/windows/](sources/windows/):
two deep dives (Astra on capture, encoding and latency; Fable on drivers, signing, the service and input)
and a cross-check where each verified the other's claims against primary sources. Where the cross-check
corrected a claim, this plan uses the corrected version.

The rule this plan follows: **nothing may depend on how the owner's PC is set up.** Valkyris has Parsec,
ViGEm and Voicemeeter installed and Secure Boot, Memory Integrity and Smart App Control switched off. A
typical new Windows 11 PC has none of those apps and has the protections on.

---

## 1. The short answer

Yes, Sunna can have a Windows host that is better than Parsec for gaming, but "better" has to be specific:

| Where Sunna can clearly win | Why |
|---|---|
| **HDR** | Parsec documents HDR as unsupported (support article for error 15106, updated 2026-07). |
| **Sharp text and full colour (4:4:4), free** | Parsec sells 4:4:4 only in Warp/Teams, and only on NVIDIA or Intel hosts. Sunna can offer it free wherever the GPU can encode it. |
| **Every host OS** | Parsec has no Linux host and a weaker Mac host. Sunna already has both. |
| **Smoother worst-case frames (p99)** | Repair lost frames by the next screen refresh instead of after 40–90 ms (Sunna's current timeouts at 30 ms ping), invalidate damaged reference frames instead of waiting for a keyframe. |
| **Higher refresh streaming with no monitor** | Sunna's own virtual display sets the capture rate (120/144/240 Hz), which is a latency feature, not only a resolution one. |

Where Sunna can only **match** Parsec: raw latency on a local network. Parsec's own camera test measured
7 ms added on a 2018 LAN setup (one run). Moonlight/Sunshine is in the same range. The realistic target is
"at least as fast on a typical frame, clearly better on the worst frames", proven by measurement, not claimed.

Where nobody wins cleanly: **kernel anti-cheat**. Reports say Valorant's Vanguard ignores mouse input
injected by Sunshine/Apollo; the mechanism is unproven. Sunna will publish a list of games known not to
work and will never try to evade anti-cheat.

---

## 2. What a stock Windows PC has, and what Sunna must ship

| Need | On a stock Windows 10/11 PC? | What Sunna does |
|---|---|---|
| Screen capture | Yes: DXGI Desktop Duplication (DDA), Windows.Graphics.Capture (WGC) | Use DDA first, WGC as fallback and for single windows |
| Hardware video encoder | Comes with the GPU driver (NVENC, AMD AMF, Intel VPL), not with Windows | Detect it, test-encode before using it, fall back to software H.264 at reduced size/rate |
| Keyboard, mouse | Yes: `SendInput` | Scan codes for games, Unicode for text |
| Touch, pen | Yes: `InjectSyntheticPointerInput` (Windows 10 1809+) | Use it; no driver |
| Desktop sound | Yes: WASAPI loopback, if the PC has *some* audio output | Use it |
| Lock screen, login screen, UAC prompts | Reachable only by a SYSTEM process in the user's session | A SYSTEM service that starts Sunna's agent in the console session (Sunshine and Parsec do the same) |
| **Virtual monitor** (no monitor attached, or stream at the client's size/refresh) | **No.** IddCx is the framework; the driver is not included | **Sunna Virtual Display**: our own user-mode (UMDF) IddCx driver |
| **Virtual gamepad** | **No.** GameInput and Windows.Gaming.Input only read controllers | **Sunna Virtual Gamepad**: our own user-mode driver, Xbox 360 first |
| **Virtual speaker / microphone** (PC with no audio device; mic passthrough) | **No** | Audio drivers are kernel-mode only (PortCls/ACX), so this needs Microsoft signing. Deferred (section 6) |

Things on Valkyris that Sunna must **not** rely on: ViGEm Bus (abandoned November 2023, installed there
by hand in August 2025), Parsec's virtual display and USB drivers (proprietary), Voicemeeter, Steam's
streaming speakers. Sunna may *use* ViGEm if a user already has it, until its own gamepad driver ships,
but never installs it.

Also unproven: whether a monitor-less NVIDIA PC still has a capturable screen output. Valkyris reports an
NVIDIA 1024×768 mode, but the GPU says no display is attached or active. Plan on "no monitor = Sunna's
virtual display is required".

---

## 3. How it fits together

```
SYSTEM service (Session 0)                    starts/restarts the agent on login, logout, user switch, wake
   │
   └─ Sunna agent: SYSTEM, in the console session, follows the input desktop (normal / lock / UAC)
        │
        ├─ capture   DDA on the output's own GPU  →  owned GPU texture (no CPU copy)
        ├─ colour    GPU shader: BGRA8 → NV12 (SDR), FP16 scRGB → P010 BT.2020 PQ (HDR), or 4:4:4
        ├─ encode    NVENC (D3D11) │ AMF (D3D11) │ Intel VPL (D3D11) │ OpenH264 (degraded)
        ├─ send      existing QUIC transport: per-frame IDs and timestamps, deadline-aware repair
        ├─ audio     WASAPI loopback → Opus 5–10 ms packets
        └─ input     SendInput / synthetic pointer / Sunna Virtual Gamepad
Games are launched as the logged-in user, never as SYSTEM.
```

Key choices, each taken from the research:

- **Capture.** DDA through `IDXGIOutput5::DuplicateOutput1`, which can hand over FP16 for HDR (Sunshine
  does this). WGC is the fallback and the single-window path; its `MinUpdateInterval` exists only from
  Windows 11 24H2. The DDA device must be on the GPU that drives the output, which matters on laptops with
  two GPUs: prefer that GPU's own encoder. Keep capture and encode on separate D3D11 devices (Sunshine
  documents a D3D11 lock that otherwise stalls the encoder). Keep only the newest frame waiting; never queue.
- **Colour on the GPU.** Today Sunna's NVIDIA path copies each frame to the CPU and converts it there
  (`crates/codec/src/nvenc/mod.rs:441`, `bgra_to_i420`). On Windows that moves to a shader. The same fix is
  needed in the Linux viewer, which copies decoded frames back to the CPU (`crates/codec/src/ffmpeg/shim.c:181`).
- **Encoders.** NVIDIA: P1 ultra-low-latency, no lookahead, no B-frames, one-frame CBR buffer, async
  completion on a worker, reference-frame invalidation on loss. Note NVENC's H.264 4:4:4 is CAVLC-only
  (5–15% less efficient), so HEVC is the better 4:4:4 codec. AMD has **no** 4:4:4 encode on any generation,
  and RX 6400/6500 have no encoder at all. Intel 11th–13th gen iGPUs and Arc do HEVC 4:4:4. The Macs
  (M1/M2) decode H.264 and HEVC in hardware but not AV1, so HEVC is the default for them.
- **Transport.** Keep QUIC. Changes the research found necessary: input gets priority over video (today
  all datagrams share one 1 MiB send buffer that evicts the oldest); a receiver-side bandwidth estimate
  drives the encoder bitrate; repair gives up when it can no longer make the next refresh and asks for a
  reference invalidation instead; detect when Tailscale falls back to a DERP relay (TCP, so loss becomes
  stalls) and switch to a safer profile and tell the user.
- **Gaming input on the clients**, which neither Sunna nor the research had covered before: relative
  mouse with the pointer locked (macOS `CGAssociateMouseAndMouseCursorPosition`, Android
  `requestPointerCapture`, Wayland `zwp_relative_pointer_v1` + `zwp_locked_pointer_v1`), high-rate mice
  coalesced with timestamps, and controller forwarding from each client's gamepad API.

---

## 4. Signing and installing: the part that decides what's possible

| Driver or binary | Runs in | Signature needed on x64 Windows | Who can get it |
|---|---|---|---|
| Sunna app, service, installer | user mode | Any publicly trusted code-signing certificate | An individual in India (IV/OV certificate) |
| Sunna Virtual Display (IddCx) | user mode (UMDF) | Catalog signed with a publicly trusted certificate; Microsoft signature **not** required on x64 | Same certificate |
| Sunna Virtual Gamepad | user mode (UMDF) | Same | Same certificate |
| Virtual speaker/microphone | **kernel** | Microsoft signing (attestation or WHQL) | Needs an EV certificate **and an organisation** registered in Partner Center |
| Anything on Windows on ARM | either | Microsoft signing | Organisation |

Facts behind this table (checked twice):

- Microsoft's PnP rules accept user-mode driver catalogs signed by "a third-party release certificate" on
  x86/x64 (not on ARM64, not in S mode). The open-source Virtual Display Driver ships exactly this way,
  signed by SignPath Foundation, with the DLL signed too.
- **Not yet proven:** that such a driver installs and loads with Secure Boot, Memory Integrity *and*
  Smart App Control all on. This is the single experiment the driver plan depends on (section 5, step 1).
- Azure's cheap signing service is open to individuals only in the US and Canada, so not to the owner.
- Price found for an individual certificate accepting Indian ID: SSL.com IV code signing, **$129/year +
  $379 hardware token** (or a cloud-signing fee instead of the token), about **$508 the first year** before
  shipping and tax. Certificates are now capped at 460 days, so expect yearly reissuing, and expect
  Windows SmartScreen to warn on downloads until the certificate builds reputation.
- Never install a home-made root certificate to make a driver trusted. Apollo does this for SudoVDA, and
  RustDesk received CVE-2024-25140 (disputed) for the same pattern.
- Installer: an MSI (WiX), because MSIX cannot install drivers. Updates come from Sunna's own service,
  verified by Authenticode **and** Sunna's pinned update signature. Uninstall removes only Sunna's own
  devices, drivers and certificates.

Code bases we can use: SudoVDA and the Microsoft IddCx sample (MIT) for the display; HIDMaestro (MIT)
and Microsoft's `vhidmini2` sample for the gamepad; ViGEm's client library (BSD) for the interim fallback.
Not usable: LizardByte's Virtual HID Driver (source-available licence forbids competitive use) and
Parsec's drivers (proprietary). The Xbox 360 (XUSB) interface the gamepad needs is not a documented
Microsoft contract, so it needs testing on every Windows release, with an Xbox One/Series HID profile as
the fallback.

---

## 5. Build order

Each step ends with something measurable. Steps 1 and 2 don't need any purchase.

| Step | What | Done when |
|---|---|---|
| **1. Prove the signing route** (no money) | On a clean Windows 11 24H2/25H2 VM with Secure Boot, Memory Integrity and Smart App Control on, install the SignPath-signed Virtual Display Driver release as a stand-in for "a user-mode driver with a public, non-Microsoft signature". Record every prompt, the setup and code-integrity logs, and whether it works after reboot. | We know whether the OV/IV route is real before paying for it |
| **2. Measurement first** | Per-frame IDs and stage timestamps across capture → encode → send → receive → decode → present, on all hosts. A click-to-flash test: microcontroller that clicks as a USB mouse and reads a photodiode on the client screen (sub-0.1 ms clock), automated for 10,000 clicks. Record Parsec's numbers on Valkyris first. | A Parsec baseline with p50/p95/p99 |
| **3. Windows host, first version** | SYSTEM service + agent, DDA, GPU colour conversion, NVENC through D3D11, SendInput, WASAPI loopback, lock/UAC/login, sleep/wake, user switching. Uses whatever display exists (a monitor or a dummy plug; on Valkyris for development only, Parsec's display). | Plays a game from the Macs; lock screen works; measured against Parsec |
| **4. Sunna Virtual Display** | IddCx driver: monitors created on demand at the client's resolution and refresh, HDR on 23H2+, removed if the agent dies, previous layout restored. Watch for the reported 24H2 mode-change bug. | A monitor-less PC streams at 1440p120 with no Parsec installed |
| **5. Client gaming input** | Pointer lock + raw relative mouse on macOS, Android, Linux; controller forwarding from each client; input priority in transport | Mouse feels the same as Parsec in an FPS, measured |
| **6. Sunna Virtual Gamepad** | Xbox 360 profile first (4 slots, rumble), ViGEm used only if already present until then; DualShock 4 / DualSense later | XInput, SDL and Steam games see the pad on a clean PC |
| **7. Installer and updates** | Signed MSI, service updater, clean uninstall, winget manifest | Clean-VM install with all protections on shows no unexpected warnings |
| **8. AMD, Intel, laptops, no-GPU** | AMF and Intel VPL backends; two-GPU laptops; software fallback | Tested on real AMD and Intel machines (we have none yet) |
| **9. Better than Parsec** | HDR end to end, free 4:4:4, deadline-aware repair and reference invalidation, bandwidth estimator, relay-mode detection; then early sending of frame slices only if measurements show a gain | Measured wins published, losses too |
| **10. Windows app** | D3D11 hardware decode, flip-model swapchain, latency-waitable presentation, raw input | Playing *from* Windows |
| Later | Virtual speaker/microphone, Windows on ARM, zero-prompt driver installs | Needs an organisation + EV + Partner Center |

---

## 6. Decisions for the owner

1. **Priority.** This reverses the 2026-09-25 decision (Mac/Linux first). Does the Windows host now come
   before the Hyprland/headless Linux work?
2. **Certificate.** Buy an individual IV/OV code-signing certificate (~$508 first year) after step 1
   confirms the route?
3. **Organisation.** A company/LLP (or a sole proprietorship that a CA accepts for EV) is needed only for
   the virtual speaker/microphone, Windows on ARM and zero-prompt drivers. Defer, or start now?
4. **First release scope.** Recommended: the virtual display is part of the first public release (most
   streaming PCs and Valkyris have no monitor); the virtual speaker/microphone is not, so a PC with no audio
   device at all streams without sound until then.
5. **Lock, login and UAC control:** on by default like Parsec, or an explicit opt-in (safer)?
6. **Build and test machines.** May Valkyris get Visual Studio Build Tools + the Windows Driver Kit (about
   8–10 GB on C:), or should Windows builds run on GitHub Actions? Where should the clean test VM run
   (Hyper-V on Valkyris, or a paid cloud VM)?
7. **Test hardware.** A photodiode + microcontroller rig (a few dollars of parts); a 120/144 Hz external
   monitor for the client side; access to any AMD or Intel GPU machine.
8. **Anti-cheat policy.** Publish a "known not to work" list and never attempt evasion. Agree?

---

## 7. Biggest risks

- **One test PC.** Valkyris is one NVIDIA card, no monitor, protections off. AMD, Intel, two-GPU laptops
  and clean PCs are untested until we get access to them.
- **The signing route is reasoned, not proven** (step 1 fixes this cheaply).
- **Undocumented Xbox 360 gamepad interface** can break on a Windows update.
- **Valkyris is probably the Arch box dual-booted** (same motherboard, CPU and GPU; never online at the same
  time), so Windows and Linux testing there can't happen at once.
- **Tailscale relay fallback** turns lost packets into stalls; the direct path must be the norm and the
  relay case must be visible to the user.
- **Maintenance.** Two drivers, a SYSTEM service and an updater, on a platform that changed its kernel
  driver policy in 2026 and its virtual-display framework three times in three years.

---

## 8. Verified on Valkyris (2026-10-12)

The first Windows host now runs. Measured on Valkyris (RTX 4060, driver 610.88, Windows 11 Pro 25H2), viewed from
the GCP VM over Tailscale (`sunna-cli connect`, software HEVC/H.264 decode). Valkyris has no monitor, so
every test used a borrowed virtual display (below); its frame rate varied from 5 to 60 fps between runs, so
latency figures are indicative only, not a Parsec comparison.

| Question | Answer |
|---|---|
| Can a PC with no monitor be captured? | **No.** Windows keeps a 1024×768 desktop on the NVIDIA output but never draws it: Desktop Duplication times out forever. A virtual display (Sunna's own, step 4) is required, as planned. |
| Does a sleeping display stop capture? | **Yes**, until it's woken. Broadcasting `WM_SYSCOMMAND/SC_MONITORPOWER(-1)` wakes it with no input injection; `ES_DISPLAY_REQUIRED` on the capture thread then keeps it on. |
| GPU-only path (DDA → video processor NV12 BT.709 → NVENC D3D11, no CPU copy) | Works. HEVC 1080p: capture → encoded 2.9 ms p50, 3.5 ms p95 on the host. |
| End to end at 60 fps | H.264 720p, 59.3 fps, 0 dropped: capture → decoded on the VM 8.8 ms p50, 10.6 ms p99 (RTT 3.7 ms). |
| SYSTEM in the console session | Works: a SYSTEM launcher in session 0 starts the host as SYSTEM in session 1 (`sunnad --console-session`), and as a real service (`--install-service`, tested, then removed). |
| Input | `SendInput` with scan codes, injected from the SYSTEM host into a test window: click position exact, "Sunna 1!" arrived with correct scan codes and Shift. |
| Sound | WASAPI loopback: 25.0 s of audio received in a 25 s session. |
| Pointer shapes, keep-awake, OS details | Work (`powercfg /requests` lists Sunna under DISPLAY and SYSTEM while streaming). |
| Codec/size change mid-session | Failed at first (one duplication per display per process); fixed: a new capture takes over the old one's duplication. |
| RTX 4060 NVENC capabilities | H.264: 4:4:4 yes, 10-bit no. HEVC: 4:4:4 yes, 10-bit yes. AV1: 4:4:4 no, 10-bit yes. All: reference-picture invalidation, long-term references (6–8), sub-frame readback, one encoder engine. So free 4:4:4 (HEVC) and HDR (HEVC/AV1) are possible on this GPU. |

Development-only scaffolding (not product code): `crates/capture/examples/parsec_display.rs` borrows an
installed Parsec Virtual Display driver to add a display (it only refreshes at full rate while Parsec itself
reads it); `test_pattern.rs` draws a moving pattern and logs input it receives; `crates/codec/examples/windows_bench.rs`
times capture + encode and prints NVENC capabilities.

From the owner's Mac (MacBook Air, HEVC, over Tailscale, owner away from home), with the host as SYSTEM in
the console session (`--console-session`):

| Question | Answer |
|---|---|
| Sign in from the Windows sign-in screen | **Works.** The PC had booted with nobody signed in; the owner saw the sign-in screen and typed the password from the Mac. Capture followed Winlogon → the new desktop (three `ACCESS_LOST` restarts in 3 s). |
| Lock screen, unlock, UAC prompt | **Work** (owner-confirmed; the log shows the secure-desktop switches and recovery within ~6 s). |
| Parsec virtual display at sign-in | **Stalled.** As the user's session started, the Parsec driver re-created its D3D11 device twice, and from then on Desktop Duplication got no image at all (even with a window drawing), while DWM kept composing at ~12 Hz. Removing and re-adding the display fixed it. A borrowed-driver quirk, but the host hid it: it repeated the last image for 10 s, so the viewer looked frozen. To do: tell the viewer when the display stops being drawn. |
| Clipboard | Built and verified (text and pictures, both ways, VM viewer ↔ Valkyris). A SYSTEM process sees only a placeholder of what the user copied (one private format, format 49161 here), even when its thread impersonates the user; writing works. So the SYSTEM host starts `sunnad --clipboard-helper` as the signed-in user (`WTSQueryUserToken`, inheritance limited to its two pipes and the log) and asks it over stdio, as Parsec does with `parsecd`. Nobody signed in: the host uses the clipboard directly. OLE apps fill the clipboard in steps, so a change that reads as empty is read again (up to ~2 s). |
| Display stops being drawn | Handled: after 2 s without an image the host duplicates the display again (a drawn display answers at once); if that brings nothing within 1 s it stops sending and logs it, keeps probing every 2 s, and starts over on a replaced display. The viewer says "No picture from <host>: its screen isn't being drawn" after 3 s. Tested by removing and re-adding the virtual display mid-session: no frames for 7 s, then the picture was back 3 s after the display was. |
| `WSAEMSGSIZE` (10040) | Was quinn's MTU probing (first probe 1326 bytes) over Tailscale's 1280-byte path: Linux ignores the failed sends, Windows logged each. Fixed by capping probes at 1252 bytes (1280 less IPv4 and UDP headers); on Valkyris the probes are now 1226 and 1252, both delivered, and the warning is gone. Video datagrams were already ≤1200 bytes. |
| Sunna's own virtual display | Built (`drivers/windows/sunna-display`: IddCx 1.4 / UMDF 2.31 driver, Windows 10 2004+; `crates/capture/src/windows_display.rs`; `sunnad --virtual-display`) and loaded on Valkyris in test-signing mode. sunnad creates a software display adapter (gone when sunnad exits) and, per viewer, a display at the viewer's size and refresh rate. Windows reported exactly what was asked: 1920×1080 @ 120.00 Hz, 2560×1440 @ 144.00 Hz, 3840×2160 @ 60.00 Hz, each ready ~65–70 ms after the request, replaced cleanly when the mode changed, captured with Desktop Duplication. Not yet measured: frames per second under motion (needs a signed-in session), cursor kept out of frames, HDR. |
| Firewall | The dev allow rule is for `D:\sunna-dev\target\release\sunnad.exe` only. A copy elsewhere listening for the first time made Windows add two silent *block* rules for that path (removed). The installer must add its rule for the installed path. |

Still open on Windows: proper 60–144 Hz measurement and a Parsec baseline (needs a display Windows draws at full rate:
a monitor, Parsec connected, or a virtual display driver); HEVC 4:4:4 and HDR end to end; whether M1/M2 Macs
decode HEVC 4:4:4 in hardware.
