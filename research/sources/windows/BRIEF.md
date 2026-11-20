# Brief: Sunna Windows gaming host — deep-dive research

## The goal (the owner's words, paraphrased)
Build a "very, very good" Windows host for Sunna that can beat Parsec for gaming. The owner's explicit rule:
**do not assume anyone's PC is set up like the owner's.** Whatever Sunna needs must either be on a stock
Windows install, or be something Sunna installs, signs and maintains itself, with a clear story for each.

## Facts (verified 2026-10-11)

**Sunna today** (repo: /home/sunny/projects/sunna, Rust workspace):
- Hosts exist for macOS (ScreenCaptureKit + VideoToolbox) and Linux (X11 capture, GNOME-Wayland portal + PipeWire;
  NVENC via libcuda/libnvidia-encode, which converts BGRA→I420 on the CPU before encoding; OpenH264 fallback).
- Viewer apps: macOS, Linux (wgpu + FFmpeg decode), Android. No Windows app.
- Windows host: none. `crates/capture/src/windows.rs` is a 9-line comment ("DXGI Desktop Duplication primary, WGC fallback").
- Transport: QUIC on quinn; media rides QUIC unreliable datagrams; Tailscale is the network layer today.
- Crates: android, audio (Opus), capture, client, clipboard, codec, host, input, portal, proto, telemetry, transport, viewer.
- Existing planning: research/00–10 and research/sources/*. Most relevant: research/01-parsec-internals.md,
  02-competitive-landscape.md, 03-engineering-reference.md, 08-architecture-plan.md (§3.7 encoder configs,
  §3.8 Windows capture/input/services/displays, §4.3 clients, §1 measurement gates vs Parsec).
  Treat those as prior claims to verify or correct, not as truth. Some are from mid-2026 and may be stale.
- Reference source checkouts in /home/sunny/research-repos: Sunshine (2026-09), Apollo (2026-05), SudoVDA (2025-04),
  moonlight-qt, moonlight-common-c, obs-studio, FreeRDP, inputtino, libvirtualhid, hwcodec, gamescope, and others.
  Read the actual code when making claims about how they work.

**The owner's test PC, "Valkyris"** (reachable: `ssh valkyris` from this VM; admin; Windows 11 Pro build 26200;
i3-10105F 4C/8T, 16 GB, RTX 4060 driver 610.88; no monitor attached; C: 103 GB free, D: 710 GB free).
It is NOT representative. It has, because the owner installed them over time:
- Parsec 150-103a (service running) with Parsec's own drivers: "Parsec Virtual Display Adapter" (IddCx, 0.45.0.0) and
  "Parsec Virtual USB Adapter" (0.3.10.0), both signed by "Microsoft Windows Hardware Compatibility Publisher".
  Parsec's virtual display (3420x2148@60) is the active screen; the NVIDIA output sits at 1024x768.
- ViGEm Bus Driver 1.22.0 (Nefarius), installed 2025-08-14 by its standalone installer — NOT a Windows default.
- Seven ghost "Xbox 360 Controller for Windows" devices from past virtual pads.
- Steam, NVIDIA App/ShadowPlay, Tailscale, git. No Rust, no MSVC Build Tools, no ffmpeg.

Rules for Valkyris: **read-only.** You may run read-only queries (PowerShell Get-*, pnputil /enum-*, dxdiag /t to a
temp file you delete afterwards, nvidia-smi -q). Do not install, uninstall, change settings, stop services or reboot.
Don't stop Parsec. The SSH shell prints a harmless OneDrive-profile execution-policy error; pass scripts as
`powershell -NoProfile -NonInteractive -EncodedCommand <base64 of UTF-16LE script>` to avoid quoting problems.

## Standards
- Cite sources (URLs, repo file:line). Mark anything you couldn't verify as UNVERIFIED. Prefer primary sources
  (Microsoft Learn, vendor SDK docs, actual source code) over blog posts. Note dates: things change (e.g. driver
  signing policy, ViGEm's maintenance status, Windows 11 24H2/25H2 changes).
- Distinguish "works on a stock Windows 11 PC" from "works only if the user/Sunna installs X".
- Where vendors differ (NVIDIA / AMD / Intel / no discrete GPU / hybrid laptop GPUs), say so.
- Don't edit the Sunna repo. Write your report to the file named in your task. Plain, direct English; tables welcome.
