# Sunna Windows gaming host: everything beyond the video pipeline

Research report, 2026-10-11. Scope: gamepads, virtual displays, driver and app signing, installer/update, service
architecture, keyboard/mouse/touch injection, anti-cheat, and the owner's test PC. Judged by the owner's rule:
**do not assume anyone's PC is set up like the owner's.** Each section says what a stock Windows 10/11 PC gives
you, what Sunna would have to ship, how to ship it, what it costs, and what Parsec, Sunshine/Apollo, Steam and
RustDesk do. Claims are sourced; anything I could not confirm is marked **UNVERIFIED**.

Sources read directly: Microsoft Learn pages (fetched 2026-10-11), the reference checkouts in
`/home/sunny/research-repos` (Sunshine 2026-09-24, Apollo 2026-05-21, libvirtualhid 2026-09-24, SudoVDA 2025-04-15,
RustDesk 2026-09-25), vendor pages, GitHub issues, and read-only PowerShell queries on Valkyris. Parsec's support
site blocks non-browser fetches (Cloudflare), so Parsec statements come from search-engine excerpts of those pages
and are marked accordingly.

---

## 0. The one-paragraph answer

A stock Windows PC gives Sunna everything for keyboard, mouse, touch and pen injection, session/service plumbing,
and capture of the login/lock/UAC desktops (if the agent runs as SYSTEM in the user's session). It gives **nothing**
for virtual gamepads or virtual monitors: both need a driver package, and no user-mode API exists that avoids one
(GameInput and Windows.Gaming.Input are consumers, not producers). The good news is that both can be **user-mode
(UMDF) drivers**, and on x64 Windows a UMDF package only needs a catalog signed by a publicly trusted Authenticode
certificate, not Microsoft's attestation signature. That means an **OV code-signing certificate** (buyable by an
individual in India, roughly $200–450/year plus a hardware token) covers the app, the service, the installer and both
drivers. The EV certificate + Partner Center route (organisation only, $300–600/year, needed for any kernel driver
and for Windows on ARM) can be deferred, possibly forever. ViGEmBus is archived but still loads on 25H2; Sunna should
detect and use it if present but never ship it. Build Sunna's own UMDF virtual HID driver (Xbox 360 XUSB personality
first, DualShock 4 second) and its own IddCx virtual display, both from MIT-licensed bases. Be honest that
kernel anti-cheat (Vanguard above all) may ignore injected input or virtual devices; nobody has solved that cleanly.

---

## 1. Gamepads

### 1.1 What a stock Windows PC provides

| Thing | Stock? | Notes |
|---|---|---|
| XInput, DirectInput, Windows.Gaming.Input, GameInput (consumer APIs) | Yes | GameInput is inbox on Windows 11 25H2 (`GameInputSvc` service, `GameInput.dll` in System32 on Valkyris) and also installed as a redistributable by games. It **reads** devices; its docs describe it as "a unified interface that exposes input" and no device-creation API exists ([GameInput overview](https://learn.microsoft.com/en-us/gaming/gdk/docs/features/common/input/overviews/input-overview?view=gdk-2604)). |
| Virtual HID Framework (`vhf.sys`) | Yes, inbox | Present and Microsoft-signed on Valkyris (`C:\WINDOWS\system32\drivers\vhf.sys`, "Stopped/Manual" = ready to load). It is a **kernel framework that a driver uses**; Microsoft's page says "VHF supports a HID source driver only in kernel mode" ([VHF](https://learn.microsoft.com/en-us/windows-hardware/drivers/hid/virtual-hid-framework--vhf-)). Yet libvirtualhid ships a UMDF2 driver that "relies only on Microsoft's inbox UMDF reflector and VHF lower filter in kernel mode" (`libvirtualhid/docs/windows-driver.md`), and it works, so the doc note is stale or narrower than practice. |
| UMDF HID minidriver host (`mshidumdf.sys`) | Yes, inbox | The documented way to write a HID minidriver in user mode. HIDMaestro uses this ([repo](https://github.com/hifihedgehog/HIDMaestro)); Microsoft's `hid/vhidmini2` sample is the reference ([Windows-driver-samples](https://github.com/microsoft/Windows-driver-samples/tree/main/hid/vhidmini2)). |
| A user-mode "create virtual Xbox controller" API | **No** | I found no such API on Windows 11, including GameInput and `Windows.Gaming.Input.Custom` (which is for *custom controller providers* layered over real HID devices). Conclusion: a driver package is unavoidable. |
| ViGEmBus, Parsec Virtual USB, HidHide | No | Third-party installs. Valkyris has ViGEmBus 1.22.0 and Parsec's drivers because the owner installed them. |

### 1.2 ViGEmBus: status, licence, risks

- **Maintainer / status.** Nefarius Software Solutions. Retired and archived on **2 November 2023** after a
  trademark conflict with ViGEM GmbH; "will no longer receive any updates" ([End of Life statement](https://docs.nefarius.at/projects/ViGEm/End-of-Life/), [repo](https://github.com/nefarius/ViGEmBus)).
  The 2022 "ViGEmBus 2" successor announcement ([issue #139](https://github.com/nefarius/ViGEmBus/issues/139)) has
  no later update; search results refer to a successor called "VirtualPad" as "in development" with **no public release found** (UNVERIFIED beyond search snippets).
- **Licence.** BSD-3-Clause (`LICENSE` in the repo, verified via raw GitHub). Redistribution of the signed
  installer is legally permitted by BSD-3 if the notice is kept; the trademark is the sensitive part (Nefarius agreed to retire the *name*, and the installer's own auto-updater points at a domain they no longer own, which the EOL page calls a security risk). Sunshine 2026-09 does **not** bundle it; its docs link users to the Nefarius releases page and call ViGEmBus "a limited alternative ... reached end of life" (`Sunshine/docs/getting_started.md` lines 572–600).
- **Still works.** Valkyris runs `ViGEmBus.sys` 1.21.442 (package 1.22.0) on Windows 11 25H2 build 26200, signed by "Microsoft Windows Hardware Compatibility Publisher" (issuer "Microsoft Windows Third Party Component CA 2014"), i.e. attestation-signed, so it loads under Secure Boot and HVCI.
- **Conflicts.** It is a kernel driver nobody maintains: if a vulnerability is ever found it can land on the Microsoft vulnerable driver blocklist (on by default since Windows 11 22H2, updated quarterly, [block rules](https://learn.microsoft.com/en-us/windows/security/application-security/application-control/app-control-for-business/design/microsoft-recommended-driver-block-rules)); that is a risk statement, not a current fact. HidHide (also Nefarius, also archived) is a filter that hides physical pads from games; it coexists with ViGEm by design but can hide Sunna's own virtual pad if the user's whitelist is wrong. Leftovers: Valkyris has **7 ghost "Xbox 360 Controller for Windows" devices** and ~25 ghost HID game-controller nodes from past virtual pads (ViGEm and Parsec both leave them); an uninstaller should clean these.
- **Anti-cheat.** Community guides warn that Vanguard "may flag third-party input software"; no official Riot statement names ViGEm (see §7).
- **Advisable for Sunna?** Use it opportunistically (detect `\\.\ViGEmBus` via the BSD-licensed client library, which Sunshine vendors in `third-party/ViGEmClient`), never install it, never make it the only path.

### 1.3 What the others do

| Product | Mechanism | Signing | Notes |
|---|---|---|---|
| **Parsec** | Own **kernel-mode** "Parsec Virtual USB Adapter" (`parsecvusba.sys`, class USB, device `ROOT\USB\0000`, v0.3.10.0, 2024-08-23) plus `parsecvirtualds.sys` (class USBDevice). It emulates a USB host controller and "plugs in" Xbox 360 (default), DualShock 4 (touchpad/motion) or DualSense (driver ≥ 0.2.5.0) devices, "the same as if an ordinary xbox controller was plugged on the host" (Parsec support excerpt, UNVERIFIED wording). | Attestation-signed by Microsoft (signer on Valkyris: "Microsoft Windows Hardware Compatibility Publisher"). | Needs the service running as SYSTEM ("running as administrator is not enough", Parsec support excerpt). Parsec's installer ships `vdd` and `vusb` folders (`C:\Program Files\Parsec\`). Proprietary; not redistributable. |
| **Steam** | No OS-level virtual device on Windows. Valve's docs: "On Windows, the Steam Overlay will hook traditional gamepad input APIs such as XInput, DirectInput, RawInput, and Windows.Gaming.Input and inject an emulated Xbox controller device. On macOS and Linux, emulated controller input is provided by a driver" ([Steam Input gamepad emulation best practices](https://partner.steamgames.com/doc/features/steam_controller/steam_input_gamepad_emulation_bestpractices)). Remote Play Together guests therefore exist only inside the game process via Steam Input. | n/a | Not reusable by Sunna; a game not launched through Steam sees nothing. Steam does install signed virtual **audio** devices ("Steam Streaming Speakers/Microphone") on first use (community reports; not present on Valkyris). |
| **Sunshine** (2026-09) | `libvirtualhid` (MIT library) → **Virtual HID Driver**: a UMDF2 package with a broker service; Xbox 360 via an XUSB "software device" (`SwDeviceCreate`) plus a VHF HID child; Xbox One/Series, DS4, DualSense, Switch Pro, Generic via VHF; also a driver-backed Raw-Input keyboard and relative mouse (`libvirtualhid/docs/windows-driver.md`, `docs/platform-support.md`). ViGEmBus kept as free fallback (`Sunshine/src/platform/windows/input.cpp` lines 537–610). | Release catalogs signed with **Azure Trusted Signing**, not Microsoft attestation ("Release builds must use Azure Trusted Signing for the catalog and generated MSI", `windows-driver.md` §Signing). AMD64 only: "Windows ARM64 release packages require a Microsoft dashboard signing path". | **Paid**: $14.99/year or $49.99 lifetime, 5 machines, online licence check when a gamepad is created ([LizardByte announcement 2026-08-16](https://app.lizardbyte.dev/2026-08-16-introducing-libvirtualhid-and-virtual-hid-driver/)). Driver, broker and MSI are under the **LizardByte Source-Available License 1.0**, which forbids distribution, commercial use and "any Competitive Purpose" (`LICENSES/LicenseRef-LizardByte-SAL-1.0.md` §3). **Not usable by Sunna.** |
| **Apollo** | Same ViGEm path as Sunshine (Apollo is older than the libvirtualhid work; `Apollo/third-party/ViGEmClient`). | n/a | Maintainer refuses driver-based input ("a more dangerous way for software to control the computer", [Apollo #1471](https://github.com/ClassicOldSong/Apollo/issues/1471)). |
| **RustDesk** | No virtual gamepad. | n/a | Remote-desktop product, not a game streamer. |
| **HIDMaestro** (MIT, active 2026) | UMDF2 HID minidriver hosted by `mshidumdf.sys`, XUSB companion for XInput, 228 controller profiles, HID PID force feedback; "Pure user mode: no kernel driver, no EV cert, no reboots" ([repo](https://github.com/hifihedgehog/HIDMaestro)). | **Self-signed certificate added to the machine's Root and TrustedPublisher stores** by its own tool. | Honest about limits: kernel anti-cheat can detect virtual devices; XInput capped at 4 slots. Best MIT base for a Sunna-owned driver if re-signed with Sunna's own certificate instead of a self-signed root (see §3.4). |

### 1.4 How a Sunna-owned driver would work

- **Model.** UMDF2 driver package (one DLL in the Driver Store, no `.sys`), root-enumerated control device, VHF
  children for HID-faithful pads (DS4/DualSense/Xbox One descriptors) and a **separate XUSB personality for Xbox 360** so
  XInput games see a real XInput slot. Both libvirtualhid and HIDMaestro say the XUSB interface GUID/IOCTLs are "not a
  supported public Microsoft driver contract" and need regression testing on every Windows release
  (`libvirtualhid/docs/windows-driver.md`). Rust on the app side; the driver itself is C++ built with the WDK and MSVC
  (Valkyris has neither; a Windows build box or CI runner is needed).
- **Profiles.** Xbox 360/XInput is the universal default (works in every XInput game, 4 slots, rumble only).
  DualShock 4 gives touchpad, gyro and lightbar for games that read the Sony HID report (and for Steam Input).
  DualSense adds adaptive triggers and haptics only for games that speak the DualSense output report. Switch Pro is
  niche. Xbox One/Series HID profiles get Impulse Triggers. The libvirtualhid compatibility matrix
  (`docs/end-user-gamepad-guide.md`) is a good map of which client/host combinations actually work end to end.
- **Multiple pads.** Up to 4 XInput slots; HID-only pads are unlimited but XInput games ignore them.
- **Brokering.** A SYSTEM service should own device creation; the per-session agent submits reports over a handle.
  Both LizardByte and HIDMaestro arrived at this split; it keeps licence/ownership checks off the hot path and lets an
  unelevated app use the device.
- **Install / update / uninstall.** `pnputil /add-driver x.inf /install` plus creating the root device
  (`SwDeviceCreate` or a devcon-style helper; Apollo uses Nefarius' `nefconc.exe`). Update = stage the new INF
  version and re-enumerate; uninstall = remove device nodes (including ghosts), `pnputil /delete-driver`, remove the
  publisher certificate from TrustedPublisher if you added one. Driver packages need **admin**; the device must be
  gone before the package can be deleted.
- **Signing.** See §3. Short version: catalog signed with Sunna's OV certificate works on x64 Windows 10/11
  (PnP accepts a "third-party release certificate"); ARM64 and S mode require WHQL/Microsoft signing.

---

## 2. Virtual displays

### 2.1 What a stock PC provides, and what happens headless

- **IddCx** (Indirect Display Driver Class Extension) is inbox since Windows 10 1607 and is the sanctioned way to add a
  monitor that does not exist. The driver is UMDF; the kernel side (`IndirectKmd.sys`) is Microsoft's, present and
  signed on Valkyris. Versions ([Microsoft IddCx versions page, dated 2026-04-01](https://learn.microsoft.com/en-us/windows-hardware/drivers/display/iddcx-versions)):

| Windows | IddCx | What you get |
|---|---|---|
| Windows 10 20H1–22H2 | 1.5 | SDR only, system-memory swapchains. **No HDR.** |
| Windows 11 21H2 | 1.8 | precise present regions |
| Windows 11 22H2 | 1.9 | `IddCxSetRealtimeGPUPriority`, no UMDF process pooling |
| Windows 11 23H2 | 1.10 (0x1A00) | **HDR10 + SDR wide gamut** |
| Windows 11 24H2 ("Windows 2024") | 1.10 (0x1A80) | HDR + runtime power management |
| Newer Windows 11 (26H1?) | 1.11 (0x1B00) | D3D12, DisplayID-only, atomic I2C (page says prerelease) |

  A single binary built against 1.10 runs on 1803+ with runtime checks; SudoVDA does exactly this (`IDD_IS_FUNCTION_AVAILABLE(IddCxSwapChainReleaseAndAcquireBuffer2)`, `SudoVDA/Driver.cpp` lines 673, 795; project targets UMDF 2.25 / IddCx 1.10, `SudoVDA.vcxproj` lines 62–78). SudoVDA's README: "HDR is only fully supported on Windows 11 24H2 ... HDR is not supported on Windows 10." High refresh is just a mode in the EDID/mode list (SudoVDA lists up to 500 Hz; VDD advertises floating-point refresh rates); the real limit is the encoder and the client.
- **No monitor, no driver.** Desktop Duplication needs a DXGI output. Parsec's error article: Parsec "requires the host's graphics card to believe there is a physical monitor connected" (support excerpt for error 14003, UNVERIFIED wording). Vendor behaviour differs:
  - **NVIDIA** keeps a phantom output alive: on Valkyris (no monitor) the RTX 4060 still reports an active 1024x768@60 output next to Parsec's 3420x2148 virtual display. That is what you would capture with no driver: a 1024x768 desktop.
  - **AMD / Intel / hybrid laptops**: no phantom output is reported in my sources; expect "no outputs" and capture failure (**UNVERIFIED**, needs a test on real AMD/Intel hardware). Laptops always have the internal panel, but lid-closed behaviour depends on the power policy.
  - Parsec's free tier has "Fallback To Virtual Display" that adds one virtual display when none is present; it also warns that USB display adapters and some docks do not work (support excerpt).
- **24H2/25H2 regression to know about.** VDD issue [#471](https://github.com/VirtualDrivers/Virtual-Display-Driver/issues/471) (Feb 2026): on 24H2 a newly set mode on an IddCx display is accepted by the driver but `ChangeDisplaySettingsEx` leaves the "Desktop mode" at the old value until the user touches the refresh-rate dropdown; not seen on 23H2 or Windows 10; open, no root cause. Whether Parsec VDD/SudoVDA hit this is unknown. Budget test time for mode-switch logic on 24H2+.

### 2.2 Candidates

| Driver | Licence | Status | How it is signed/installed | Verdict |
|---|---|---|---|---|
| **SudoVDA** (`/home/sunny/research-repos/SudoVDA`) | "MIT and CC0 or Public Domain" for the author's changes; the rest is Microsoft's IddCx sample (MIT) ([README](https://github.com/SudoMaker/SudoVDA)) | Last commit 2025-04-15; **no GitHub releases** (releases page is empty). Binary ships inside Apollo. | Apollo's `install.bat` runs `certutil -addstore -f root sudovda.cer` and `-addstore -f TrustedPublisher`, then `nefconc.exe --create-device-node ... root\sudomaker\sudovda` and `--install-driver` (`Apollo/src_assets/windows/drivers/sudovda/install.bat`). The certificate is **self-signed** (`CN = sudovda@su.mk`, valid 2025-07-13 to 2030-07-13) and the catalog is signed by it (checked with openssl). | Good code base (IOCTL API in `Apollo/third-party/sudovda/sudovda-ioctl.h`, watchdog, per-monitor GUIDs, HDR bits), but the shipping trust model (a private self-signed root CA installed on users' PCs) is the same pattern that got RustDesk [CVE-2024-25140](https://nvd.nist.gov/vuln/detail/CVE-2024-25140). Reuse the source, sign it ourselves. |
| **Virtual Display Driver** (VirtualDrivers / MikeTheTech) | MIT | Active (maintainers MikeTheTech, Jocke); IddCx 1.10, HDR on 23H2+, 8/10/12-bit, custom EDID ([repo](https://github.com/VirtualDrivers/Virtual-Display-Driver)) | Catalog signed by **SignPath Foundation** (a free OSS code-signing service with a publicly trusted certificate). Install shows a "trust this publisher?" prompt; when it does not, users hit `0xE0000242 Publisher trust ... has not been established` ([issue #446](https://github.com/VirtualDrivers/Virtual-Display-Driver/issues/446)). README: ARM64 on 24H2 "may require test signing". | Proof that an OSS IddCx driver ships without Microsoft signing on x64. Alternative base to SudoVDA; more features, more churn. |
| **Parsec VDD** | Proprietary | Shipped with Parsec; `mm.inf` Display class 0.45.0.0 (2024-01-25) on Valkyris; up to 3 displays | Signed "Microsoft Windows Hardware Compatibility Publisher" (attestation). The `nomi-san/parsec-vdd` project drives it standalone; redistribution by others is not permitted (Parsec EULA, UNVERIFIED). | Shows what "feels native" looks like: no trust prompt, no root-CA hack. |
| **RustDesk** | AGPL app | Historically its own `RustDeskIddDriver`, test-signed with the WDK test cert installed into the root store (CVE above, disputed by vendor "because it has no EV certificate"); reports say it later moved to Amyuni's Microsoft-signed `usbmmidd_v2` (UNVERIFIED); `src/platform/windows_delete_test_cert.cc` still exists to clean the old cert. | | Cautionary tale. |
| **Microsoft IddCx sample** | MIT | Reference | Unsigned | The common ancestor of all of the above. |

### 2.3 Sunna's virtual display

- Build from SudoVDA (cleanest IOCTL surface) or the Microsoft sample; keep MIT headers. Features for v1: N monitors
  on demand, mode list from the client (resolution, refresh, optionally HDR on 23H2+), stable per-client EDID serial,
  watchdog that removes the monitor if the agent dies, "restore previous topology" on disconnect.
- Capture it with the same DXGI Desktop Duplication path as a physical monitor (simplest; what Parsec does). A direct
  IddCx-swapchain-to-encoder path would skip one copy but couples the driver to the encoder; defer.
- Package it as an **optional component** of the installer (Apollo marks SudoVDA "required"; Parsec bundles it
  silently). On AMD/Intel headless machines it is the only way to stream at all.

---

## 3. Driver signing reality in 2026

### 3.1 The rules (Microsoft Learn, fetched 2026-10-11)

- **Kernel drivers**: "Starting with Windows 10, version 1607, Windows will not load any new kernel-mode drivers which
  are not signed by the Dev Portal ... an EV code signing certificate is required to establish a dashboard account"
  ([Driver Signing Policy](https://learn.microsoft.com/en-us/windows-hardware/drivers/install/kernel-mode-code-signing-policy--windows-vista-and-later-)).
  Exceptions existed for Secure-Boot-off PCs and pre-2015 cross-signed certs; Microsoft's **April 2026** update
  removed trust for cross-signed kernel drivers on Windows 11 24H2/25H2/26H1 and Server 2025, first in an audit
  "evaluation mode" then enforced ([Neowin](https://www.neowin.net/news/microsoft-is-changing-a-windows-kernel-policy-thats-been-around-for-decades/), [Igor's Lab](https://www.igorslab.de/en/windows-11-is-phasing-out-support-for-legacy-kernel-drivers-microsoft-will-permanently-end-cross-signing-in-april/)). Net: **any kernel driver = Partner Center, full stop.**
- **Attestation signing** (no HLK): requires a Hardware Developer Program account, which requires an EV certificate
  and an organisation (Entra global admin, legal contact, company record or D-U-N-S, questionnaire)
  ([Register](https://learn.microsoft.com/en-us/windows-hardware/drivers/dashboard/hardware-program-register)).
  Microsoft now files it under "for testing scenarios" and says attestation-signed drivers "can't be published to
  Windows Update for retail audiences", but the requirements table still says they load on Windows 10/11 retail
  ("Windows 10: Yes"), they support "kernel mode and user mode drivers", and they do **not** load on Windows Server
  2016+ ([Driver signing options, dated 2026-03-23](https://learn.microsoft.com/en-us/windows-hardware/drivers/dashboard/driver-signing-offerings)).
  ViGEmBus, Parsec's drivers and VB-Audio on Valkyris are all attestation-signed, so this is the normal path for
  software vendors. Turnaround is typically under an hour to a day (OSR community, anecdotal). ARM64 is supported if
  every folder in the CAB ships the same architectures ([Attestation how-to](https://learn.microsoft.com/en-us/windows-hardware/drivers/dashboard/code-signing-attestation)).
- **User-mode (UMDF) driver packages**: kernel-mode code signing policy does not apply to the DLL. What applies is the
  PnP install rule: the catalog must be "signed by WHQL or signed by a third-party release certificate"; but
  "Windows 10 in S mode and editions for architectures other than x86 and amd64 require catalog files ... to have been
  signed by WHQL" ([PnP signing requirements](https://learn.microsoft.com/en-us/windows-hardware/drivers/install/pnp-device-installation-signing-requirements--windows-vista-and-later-)).
  This is why libvirtualhid ships an Azure-Trusted-Signing-signed UMDF package for x64 only, VDD ships a
  SignPath-signed one, and both say ARM64 needs the Microsoft path. **This is the hinge of the whole plan.**
- **HVCI / Memory Integrity**: affects kernel drivers only (non-compliant memory usage blocks the load). Microsoft
  says "HVCI is on by-default for most new Windows 11 devices" ([block rules page](https://learn.microsoft.com/en-us/windows/security/application-security/application-control/app-control-for-business/design/microsoft-recommended-driver-block-rules)). UMDF drivers are unaffected.
- **Vulnerable driver blocklist**: on by default since Windows 11 22H2, enforced also under HVCI/Smart App
  Control/S mode, updated quarterly and via monthly updates (same page). Kernel drivers only.
- **Secure Boot**: with it on, the kernel rules above are strict; Valkyris has it **off** (not typical; Windows 11
  setup requires it and Vanguard refuses to run without it on Windows 11, [Riot VAN:RESTRICTION 4](https://support.riotgames.com/riot/penalties/error-van-restriction-4)).
- **Smart App Control**: blocks apps that are unsigned or have an invalid signature when the cloud cannot judge them;
  "an app with a valid signature runs even if the service can't make a confident prediction"; it is on for new
  installs and turns itself off on upgraded, enterprise, developer-mode or no-diagnostic-data machines
  ([Microsoft support](https://support.microsoft.com/en-us/windows/what-is-smart-app-control-285ea03d-fa88-4d56-882e-6698afdb7003)).
  Whether SAC accepts a UMDF DLL whose chain ends in a **locally added self-signed root** is **UNVERIFIED**; a
  publicly trusted OV/EV chain should pass. Test on a clean SAC-on VM.
- **Windows on ARM**: UMDF or kernel, the catalog must be WHQL/Microsoft-signed (PnP rule above). Sunna on ARM
  needs Partner Center. Defer.

### 3.2 Certificates: who can buy what, and for how much (prices are USD list/reseller, 2026, UNVERIFIED for India)

| Option | Who | Signs what | Cost / lead time | Notes |
|---|---|---|---|---|
| **OV code signing, individual validation** (Sectigo/Certum/SSL.com via resellers) | Individuals, including India | App, service, installer, **UMDF catalogs** (x64) | ~$200–450/year; private key must live on a FIPS token or cloud HSM since 2023 (token shipped by courier, 1–2 weeks); validation days to a week ([sslinsights](https://sslinsights.com/digicert-vs-sectigo-code-signing/), [ssldragon](https://www.ssldragon.com/ssl-certificates/code-signing/)) | The practical choice now. |
| **EV code signing** | Registered organisations; SSL.com advertises a "Sole Proprietor EV" tier with conflicting eligibility text ([SSL.com](https://www.ssl.com/products/software-integrity/code-signing/ev-sole-proprietor/)); Indian sole proprietorship with GST/Udyam proof **UNVERIFIED** | Everything OV does, plus registering for Partner Center (kernel drivers, ARM64) | ~$290–620/year + token/HSM; 1–4 weeks; Partner Center approval adds days to weeks | Since 2024 EV no longer buys instant SmartScreen reputation ([DigiCert](https://knowledge.digicert.com/alerts/ev-signed-application-showing-microsoft-defender-smartscreen-warnings), [ToDesktop](https://www.todesktop.com/blog/posts/windows-apps-psa-ev-certs-do-not-grant-immediate-reputation-anymore)). |
| **Azure Artifact Signing** (ex Trusted Signing) | Organisations in US/CA/EU/UK/AU/NZ/JP/KR/SG/CH/NO/IL; individuals **only US/Canada** ([quickstart, dated 2026-05-21](https://learn.microsoft.com/en-us/azure/artifact-signing/quickstart)) | App + UMDF catalogs (libvirtualhid's path); **not** kernel drivers, not Partner Center | ~$10/month Basic (UNVERIFIED current price) | **Not available to the owner in India** today. Microsoft said it is "actively working" on more countries, no ETA. |
| **Self-signed root installed by the installer** | Anyone | UMDF catalogs | Free | SudoVDA/Apollo, HIDMaestro, old RustDesk. Works on x64, but installs a private CA on the user's machine. Reject for production. |
| **Test signing mode** | Dev only | | | Watermark, Secure Boot interaction; never for users. |

### 3.3 What an individual open-source developer can realistically get

- Immediately: an OV certificate. That covers a signed app, signed service, signed MSI, and signed **UMDF**
  gamepad and display drivers on x64 Windows 10/11, with a one-time "Would you like to install this device software
  from Sunna?" prompt the first time (or none if the installer, running elevated, adds Sunna's leaf certificate to
  TrustedPublisher, which is what VDD's silent installer does and is not the same as adding a root CA).
- With a company/LLP (or an accepted sole-proprietor EV): Partner Center → attestation-signed kernel drivers and
  ARM64 packages. Needed only if Sunna ever wants a kernel-level virtual USB controller like Parsec's, or ARM.
- SmartScreen reputation accrues per certificate by download volume; renewals have reset it for some vendors
  ([Microsoft Q&A](https://learn.microsoft.com/en-ie/answers/questions/5900208/smartscreen-reputation-reset-following-ev-certific)). Plan for the "Windows protected your PC" prompt in the first weeks of each new certificate and for 3-year certificates to reduce resets.

### 3.4 The trust-model decision

Three ways to get a UMDF driver onto a stock x64 PC, from worst to best:

1. **Self-signed root** (SudoVDA/Apollo, HIDMaestro): no money, but you are installing a CA whose private key sits on
   a developer laptop. It contradicts "feels natively installed" and is the pattern that got RustDesk a CVE.
2. **Publicly trusted OV cert on the catalog** (VDD via SignPath, libvirtualhid via Azure): no Microsoft involvement,
   one publisher prompt or a TrustedPublisher pre-add. Works with Secure Boot, HVCI, blocklist. SAC: likely fine, test.
3. **Microsoft attestation** (Parsec, ViGEm): zero prompts, ARM64 possible, but organisation + EV + Partner Center.

Recommendation: (2) now, (3) when there is a company.

---

## 4. App signing and first-run trust

- **Authenticode on everything** (exe, service, DLLs, MSI, driver catalogs) with the OV certificate; timestamp every
  signature. Parsec's binaries are signed "Unity Technologies SF" (DigiCert), Steam "Valve Corp.", Tailscale with an
  EV cert (its subject carries the EV "Private Organization" OID) — all observed on Valkyris with
  `Get-AuthenticodeSignature`.
- **Installer format.** MSIX cannot install drivers ("MSIX doesn't support Windows drivers"), cannot write HKLM,
  cannot run elevated, and supports only session-0 services under LocalSystem/LocalService/NetworkService
  ([MSIX prepare](https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-prepare)). A viewer-only app
  could be MSIX; the host cannot. Use **MSI (WiX)** per-machine: it is what libvirtualhid uses for its driver package,
  what enterprises can deploy, and winget supports it. NSIS/Inno exe installers (Sunshine, Apollo) work but are harder
  to make transactional and to repair.
- **winget.** Submit a manifest to the community repo (Sunshine and Parsec both have one). winget is a distribution
  channel, not an updater: it only upgrades what it installed, and background auto-update needs a third-party tool
  ([Windows Central on Winget-AutoUpdate](https://www.windowscentral.com/tag/windows-11?page=2)). Microsoft's
  announced third-party "Windows Update orchestration platform" status in 2026 is **UNVERIFIED**.
- **Auto-update.** Ship Sunna's own updater: the service (already SYSTEM) downloads the new MSI, verifies the
  Authenticode chain **and** Sunna's pinned Ed25519 manifest signature (matches D-30 in `research/08-architecture-plan.md`),
  then runs `msiexec /i /qn` without a UAC prompt. Parsec self-updates silently via its service; Sunshine notifies
  but does not auto-install (UNVERIFIED for the latest version).
- **First run.** Expect SmartScreen's "unrecognized app" prompt on a brand-new certificate; mitigate with a steady
  download stream and never rotating the certificate early. Uninstall must remove: service, firewall rules, both
  driver packages and their device nodes (incl. ghosts), TrustedPublisher entry, scheduled tasks, per-user data only on request.

---

## 5. Service architecture

### 5.1 The problem

Since Vista, services live in Session 0 and cannot see or touch the interactive desktop. The interactive session's
desktops (`Default`, `Winlogon` for login/lock/UAC secure desktop, `Screen-saver`) are only accessible to processes
in that session, and the `Winlogon` desktop only to SYSTEM. Capture (Desktop Duplication) and `SendInput` both
require being in the right session **and** on the right desktop, and `SendInput` additionally requires an integrity
level at or above the target window ("subject to UIPI", and UIPI failures are silent:
[SendInput](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput)).

### 5.2 How the others solve it (from source)

- **Sunshine** (`Sunshine/tools/sunshinesvc.cpp`, "Handles launching Sunshine.exe into user sessions as SYSTEM"):
  the service duplicates its own LocalSystem token, `SetTokenInformation(TokenSessionId, consoleSessionId)` (lines
  98–120), and `CreateProcessAsUserW(console_token, L"Sunshine.exe", ...)` with `lpDesktop = "winsta0\\default"`
  inside a job object (lines 251–273). It accepts `SERVICE_ACCEPT_SESSIONCHANGE` and restarts the agent when the
  console session changes (`WTS_CONSOLE_CONNECT`, lines 31–34, 285). The agent, now SYSTEM in session 1, calls
  `OpenInputDesktop`/`SetThreadDesktop` before `DuplicateOutput` so it can follow the user onto the secure desktop
  (`misc.cpp` 225–233, `display_base.cpp` 405–425, where `E_ACCESSDENIED` means "no permission for this desktop").
  Games are launched **as the user** via `WTSGetActiveConsoleSessionId` → `WTSQueryUserToken` →
  `CreateProcessAsUserW` with the user's environment block (`misc.cpp` 303–311, 1035–1059).
- **RustDesk** (`rustdesk/src/platform/windows.cc` 115–270): finds `winlogon.exe` (for SYSTEM) or `explorer.exe`
  (for user) in the target session, duplicates that process's token, launches on `winsta0\default`; a helper switches
  the thread desktop to the input desktop (`selectInputDesktop`, lines 328–355).
- **Parsec** (observed on Valkyris): service "Parsec" = `C:\Program Files\Parsec\pservice.exe`, LocalSystem,
  auto-start, `WIN32_OWN_PROCESS (interactive)`; two `parsecd.exe` processes in session 1. Broadcom's KB on
  Parsec + UAC: the service "should be installed, configured for Automatic startup, and actively running as SYSTEM";
  the portable/per-user mode "will never be able to click on the notifier" (UAC prompt)
  ([Broadcom KB 449607](https://knowledge.broadcom.com/external/article/449607/unable-to-interact-with-notifier-prompt.html)).
- **Windows Graphics Capture** cannot capture the secure desktop at all; only DDA from SYSTEM can (consistent with
  Sunshine's design; direct Microsoft statement **UNVERIFIED**).

### 5.3 Sunna design

| Concern | Stock Windows gives | Sunna does |
|---|---|---|
| Lock screen, login screen, UAC secure desktop | Reachable only by SYSTEM in the console session on the `Winlogon` desktop | SYSTEM service → SYSTEM agent in console session (Sunshine model). Agent re-opens the input desktop on `DXGI_ERROR_ACCESS_LOST`. Injection on the secure desktop is a **privilege**: make "control UAC/login screen" an explicit host setting (D-22 in the plan already says so). |
| Fast user switching | `WTS_CONSOLE_CONNECT/DISCONNECT`, console session id changes | Service kills/respawns agent per session; named pipes and shared textures are per-session. |
| Sleep/wake | `PBT_APMRESUME…` power broadcasts; DDA returns `DXGI_ERROR_ACCESS_LOST`; GPU devices may be lost | Re-create D3D device + duplication; virtual displays survive sleep (UMDF power-managed queues, libvirtualhid docs) but SudoVDA's 3-second watchdog needs feeding across resume. |
| Wake-on-LAN | NIC + BIOS; Valkyris: Ethernet `WakeOnMagicPacket Enabled`, S3 + hibernate available, no Modern Standby | Magic packets do not cross Tailscale; send from another Sunna device on the same LAN or from the Android client over local Wi-Fi. Moonlight sends magic packets (well-known feature; not located in this checkout, UNVERIFIED-in-code). Parsec has no WoL (UNVERIFIED). |
| Reboot before login | Service auto-starts; login screen is on the `Winlogon` desktop | Works with the SYSTEM agent; BitLocker pre-boot is unreachable (same as FileVault). Do not push auto-logon; document it as a user choice. |
| Security | SYSTEM agent = full machine | Pipe ACLs with explicit SIDs, no remote-supplied commands, per-session tokens, pairing + revocation already in the plan. A compromised viewer session can click through UAC: say so in the docs. |

---

## 6. Input beyond gamepads

- **`SendInput`** (stock): UIPI-limited (solved by running as SYSTEM), serial insertion, does not reset key state
  ([SendInput](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput)). Injected events
  are visible to Raw Input consumers (a window receives `WM_INPUT` for them; community report
  [here](https://windows-hexerror.linestarve.com/q/so2174512-whmousell-hook-doesnt-get-called-for-injected-events-mouseevent-sendinput)),
  which is why Parsec and Sunshine work in Raw-Input FPS games. Low-level hooks and kernel anti-cheat can see the
  `LLMHF_INJECTED` flag; that is the lever Vanguard appears to use (§7).
- **Relative mouse for FPS.** Send `MOUSEEVENTF_MOVE` deltas with no `ABSOLUTE` flag; never let the client
  accelerate and the host accelerate again (Moonlight/Parsec both do raw capture on the client). Windows applies
  "Enhance pointer precision" to cursor movement from injected deltas, while Raw-Input games read the deltas; the
  exact behaviour for injected input is **UNVERIFIED** and should be measured. Sunshine's answer in 2026 was a
  **driver-backed HID mouse** so "applications using Raw Input can receive them" as a real device
  (`Sunshine/docs/getting_started.md` 582–590); keep SendInput as the v1 path and add a HID mouse/keyboard to the
  same UMDF driver later if measurements or anti-cheat demand it.
- **Keyboard layouts.** Send scan codes (`KEYEVENTF_SCANCODE`, extended-key flag) for games and shortcuts, derived on
  the host from the client's VK with the **host's** layout; send `KEYEVENTF_UNICODE` for text and IME composition.
  Both Parsec and Sunshine carry VK + modifiers in the protocol; libvirtualhid has a `keylayout.hpp` for the HID
  keyboard case. IME: there is no public API to drive a host IME remotely; Unicode text injection is the honest answer.
- **Touch and pen.** Stock, no driver: `CreateSyntheticPointerDevice` + `InjectSyntheticPointerInput` (Windows 10
  1809+, `PT_TOUCH` up to `MAX_TOUCH_COUNT`, `PT_PEN` one contact, coordinates in virtual-screen pixels)
  ([InjectSyntheticPointerInput](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-injectsyntheticpointerinput)).
  Sunshine creates "per-client raw input devices for touch and pen events" with it (`input.cpp` line 643). Parsec
  Warp sells pen support on the same API (UNVERIFIED).
- **Is a virtual mouse/keyboard driver needed?** Not for v1. Needed only for: games/anti-cheat that discard
  injected input (Vanguard), true multi-seat keyboards/mice (two players, two cursors: Windows has one cursor anyway),
  and marginal Raw-Input fidelity. Sunshine gates this behind the paid driver; Parsec's VUSB may also expose
  keyboard/mouse (UNVERIFIED).

---

## 7. Anti-cheat and what is honest to promise

- **No vendor (Riot, Epic/EAC, BattlEye, FACEIT) publishes a remote-play policy** that I could find.
- **Riot Vanguard** (Valorant, League, TFT): multiple reports that mouse input injected by Sunshine/Apollo has no
  effect once `vgk.sys` is loaded while keyboard works; the reporter's diagnosis is that Vanguard drops
  `SendInput` from the streaming process. Apollo's maintainer closed it "not planned" rather than add a driver
  ([Apollo #1471](https://github.com/ClassicOldSong/Apollo/issues/1471), [#1202](https://github.com/ClassicOldSong/Apollo/issues/1202), [LizardByte discussion #203](https://github.com/orgs/LizardByte/discussions/203)).
  Reporters claim Parsec's driver-backed input works; **UNVERIFIED**. Vanguard also requires Secure Boot + TPM on
  Windows 11, so a Valorant player's PC will reject any non-Microsoft-signed kernel driver.
- **EAC / BattlEye**: remote play with Parsec/Sunshine is common and I found no ban policy; anecdotes of HWID bans
  from unrelated RGB software show these systems can be aggressive ([Igor's Lab](https://www.igorslab.de/en/battleye-and-easy-anti-cheat-rgb-control-software-causes-anew-bans-and-accounts-blocks-that-make-games-worthless/)). Some EAC games are invisible to Parsec's overlay-style features (old Steam thread). Treat as "usually works".
- **Virtual gamepads**: every virtual pad (ViGEm, Parsec VUSB, HIDMaestro, libvirtualhid) is detectable by kernel
  anti-cheat; HIDMaestro says so openly. Games served through Steam Input generally accept them.
- **Capture** is not an anti-cheat problem in practice (DDA is an OS feature; Parsec/Sunshine/OBS all use it).
- **What to promise**: "Works with most games. Games with kernel anti-cheat may ignore streamed mouse input or
  virtual controllers (Valorant is the known case). Sunna does not and will not try to hide from anti-cheat." Keep
  a public per-game compatibility table fed by users, as libvirtualhid does for controllers.

---

## 8. The owner's machine (Valkyris), read-only checks, 2026-10-11

| Check | Valkyris | Typical gamer PC in 2026 | Why it matters |
|---|---|---|---|
| Edition / build | Windows 11 Pro 25H2, 10.0.26200.9457, UEFI, Gigabyte H410M H V3, TPM 2.0 enabled | Home or Pro, 24H2/25H2 | IddCx 1.10, GameInput inbox, April-2026 kernel policy all apply |
| Secure Boot | **Off** (`Confirm-SecureBootUEFI` = False) | **On** (Windows 11 requirement, Vanguard requirement) | Non-Microsoft kernel drivers that happen to work here will not work there |
| HVCI / Memory Integrity | **Off** (`HypervisorEnforcedCodeIntegrity\Enabled` = 0; VBS running, no services) | **On** by default on new Windows 11 devices | Kernel drivers must be HVCI-compatible; UMDF unaffected |
| Smart App Control | Off (`VerifiedAndReputablePolicyState` = 0) | On for clean installs, off after upgrades | Unsigned binaries blocked on SAC machines |
| Vulnerable driver blocklist | On (= default) | On | Kernel-only |
| Test signing / nointegritychecks | Not set | Not set | Nobody runs test mode |
| Defender | Real-time protection on, tamper protection on | Same | New-certificate SmartScreen prompts |
| Session layout | Session 0 services; console session 1 user `sunny` active; AutoAdminLogon = 0; password required | Same shape, often PIN/Hello | Service + per-session agent model |
| Power | S3 standby + hibernate; **no Modern Standby**; Ethernet WoL magic packet enabled | Desktops: S3; laptops: Modern Standby (S0ix) | WoL and resume testing differ |
| GPU/display | RTX 4060 driver 32.0.16.1088 with a phantom 1024x768 output; Parsec VDD 3420x2148@60 is primary; ghost LG monitors | One or two physical monitors; NVIDIA/AMD/Intel | Headless behaviour is NVIDIA-specific |
| Third-party drivers | ViGEmBus 1.22.0, Parsec VUSB 0.3.10.0 + VDD 0.45.0.0 (all attestation-signed), VB-Audio Voicemeeter, Realtek, Intel ME, Apple USB | None of these except what games/Steam install | **Do not depend on any of them** |
| Inbox pieces Sunna will use | `vhf.sys`, `WUDFRd.sys`, `IndirectKmd.sys`, `GameInputSvc`, winget 1.29 | Same | All present on stock Windows 11 |
| Leftovers | 7 ghost "Xbox 360 Controller for Windows" devices, ~25 ghost HID controller nodes, no SudoVDA registry key | Clean | Uninstaller hygiene |
| Toolchain | No Rust, no MSVC, no WDK | n/a | Driver builds need a Windows build box/CI |

Note: Parsec's service here runs as LocalSystem and Parsec's gamepad and display both work with Secure Boot off; the
same binaries work with it on because they are attestation-signed. Nothing about Valkyris's weaker posture should
be assumed for users.

---

## 9. Recommended plan

### 9.1 What to build, borrow, skip

| Area | v1 (first public Windows host) | v1.x | Skip / defer |
|---|---|---|---|
| Service + agent | Build: SYSTEM service, SYSTEM per-session agent (Sunshine model), games launched as the user, session-change and power handling, pipe ACLs | WoL helper from LAN peers | Per-user "portable" mode as the default (cannot do lock screen/UAC) |
| Capture (other agent's scope) | DDA incl. secure desktop; physical monitor or dummy plug | | |
| Keyboard/mouse/touch/pen | Build on `SendInput` (scancode + Unicode) and `InjectSyntheticPointerInput` | HID keyboard/mouse personality in Sunna's UMDF driver if measurements or anti-cheat justify it | |
| Gamepad | Build: **Sunna Virtual HID** UMDF driver, Xbox 360 XUSB personality + VHF child, from HIDMaestro (MIT) or `vhidmini2`; OV-signed catalog; optional installer component. Detect and use an existing ViGEmBus (BSD client lib) as a zero-install fallback; never ship ViGEm | DS4 then DualSense profiles (touchpad, gyro, lightbar, adaptive triggers), Xbox Series Impulse Triggers | libvirtualhid's Virtual HID Driver (SAL licence, paid, competitive-use ban); Parsec-style kernel VUSB; Steam Input |
| Virtual display | Ship v1 with physical monitor/dummy plug and an explicit "no display found" diagnostic | Build: **Sunna Virtual Display** IddCx driver from SudoVDA/MS sample (MIT), OV-signed, follow-viewer modes, HDR on 23H2+ | Direct IddCx-swapchain encode path; Windows 10 HDR (impossible) |
| Signing | Buy an **OV code-signing certificate (individual validation) + hardware token now** | 3-year renewal to protect SmartScreen reputation | EV + Partner Center until a company exists or ARM/kernel is needed; Azure Artifact Signing (not available in India) |
| Installer/updates | WiX MSI per-machine with service, drivers as optional features, clean uninstall; winget manifest; service-driven updater with Authenticode + pinned Ed25519 manifest | MSIX for the viewer only | MSIX host, Store |
| ARM64 | Viewer only | | Host drivers on ARM (needs Microsoft signing) |

### 9.2 Costs and lead times (UNVERIFIED exact figures; order of magnitude)

| Item | Cost | Lead time |
|---|---|---|
| OV code-signing cert, individual, 1 yr | $200–450 + token (~$50–100 or included) | 1–3 weeks incl. token courier to India |
| EV cert (needs org) | $290–620/yr + FIPS token/HSM | 1–4 weeks |
| Partner Center hardware program | free, needs EV + Entra + legal contact + questionnaire | days–weeks |
| Attestation submission | free per submission | under an hour to a day each |
| Windows build box with VS + WDK | $0 if Valkyris is used (install MSVC + WDK; ask owner) or a GitHub Actions `windows-latest` runner | days |
| Test matrix: clean Win11 24H2 VM with Secure Boot + HVCI + SAC on; Win10 22H2 VM; one AMD or Intel machine | VM time; AMD/Intel hardware is a gap | ongoing |

### 9.3 Order

1. Certificate purchase and a signed-build pipeline (blocks everything user-facing).
2. Service/agent skeleton + capture/input on a physical display; verify lock screen, UAC, FUS, sleep, reboot-before-login on Valkyris and on a clean VM with Secure Boot/HVCI/SAC on.
3. MSI installer, uninstaller hygiene, winget manifest, updater.
4. Sunna Virtual HID driver (Xbox 360 first); test with XInput, SDL, Steam Input, browser Gamepad API, Game Bar; test the TrustedPublisher flow on a clean VM; ViGEm detection fallback.
5. Sunna Virtual Display driver; test the 24H2 mode-change regression, HDR on 23H2+, NVIDIA/AMD/Intel headless.
6. DS4/DualSense profiles, HID mouse/keyboard personality, WoL helper.
7. Company formation → EV → Partner Center → ARM64 and zero-prompt driver installs (optional, later).

### 9.4 Biggest risks

- **The XUSB Xbox 360 personality rests on an undocumented contract**; a Windows update can break XInput for virtual pads overnight. Mitigation: HID-only Xbox One/Series profile as fallback, release-by-release regression tests.
- **Trusted Publisher / SAC behaviour on clean machines** has only been reasoned about, not tested; VDD's issue #446 shows real users failing on exactly this step.
- **Vanguard**: no clean solution exists; expect "mouse does nothing in Valorant" reports.
- **24H2+ IddCx mode-apply regression** (#471) could make "follow the viewer's resolution" flaky.
- **Certificate logistics in India** (token shipping, CA validation of an individual) and SmartScreen's cold start.
- **Maintenance load**: two drivers, a service, an updater and a Windows release cadence that changed kernel trust policy in April 2026 and IddCx three times in three years. Budget the 25–30% recurring capacity the architecture plan already reserves.
- **Toolchain**: drivers are C++/WDK, outside the Rust workspace; nobody on the team has built a WDK driver yet (assumption).

### 9.5 Open questions for the owner

1. Is forming a company/LLP (or an Indian sole proprietorship with GST/Udyam registration) acceptable, and when? It gates EV, Partner Center, kernel drivers and ARM64.
2. Budget approval for the OV certificate and token now (~$300 first year).
3. Should v1 use an already-installed ViGEmBus as a fallback, or ship gamepad support only with Sunna's own driver (slower but cleaner)? Prior decision D-36 said "no production archived-driver dependency"; detection-only is the middle ground.
4. Should "control the login/lock/UAC desktops" be on by default (Parsec-like) or opt-in (safer)?
5. May Valkyris get Visual Studio Build Tools + WDK installed, or should driver builds go to GitHub Actions?
6. How important is HDR in the first Windows release (it forces Windows 11 23H2+ and the virtual display)?
7. Is any AMD or Intel GPU machine available for the headless and encoder tests? Nothing in this report about AMD/Intel headless behaviour is verified.
8. Policy on anti-cheat: publish a "known not to work" list openly, and never attempt evasion. Agree?

---

## Appendix A. Commands used on Valkyris (all read-only)

`Get-CimInstance Win32_OperatingSystem`, `Confirm-SecureBootUEFI`, `Get-CimInstance Win32_DeviceGuard`, registry reads
under `HKLM:\SYSTEM\CurrentControlSet\Control\DeviceGuard`, `...\CI\Policy`, `...\CI\Config`, `bcdedit /enum {current}`,
`Get-ComputerInfo`, `pnputil /enum-drivers`, `Get-CimInstance Win32_SystemDriver`, `Get-Service`, uninstall-registry
reads, `Get-CimInstance Win32_VideoController`, `Get-PnpDevice`, `query session`, `query user`, `powercfg /a`,
`Get-NetAdapter`, `Get-NetAdapterPowerManagement`, `Get-AuthenticodeSignature` on Parsec/Steam/Tailscale/driver
binaries, `sc.exe qc Parsec`, `Get-Process parsecd`, `Get-LocalUser`, `Get-MpComputerStatus`, `winget --version`.
Nothing was installed, changed, stopped or rebooted. The dxdiag dump was not needed.

## Appendix B. Source index

- Microsoft Learn: [IddCx versions](https://learn.microsoft.com/en-us/windows-hardware/drivers/display/iddcx-versions), [IddCx 1.10 updates](https://learn.microsoft.com/en-sg/WINDOWS-HARDWARE/DRIVERS/display/iddcx1.10-updates), [VHF](https://learn.microsoft.com/en-us/windows-hardware/drivers/hid/virtual-hid-framework--vhf-), [Driver signing policy](https://learn.microsoft.com/en-us/windows-hardware/drivers/install/kernel-mode-code-signing-policy--windows-vista-and-later-), [PnP signing requirements](https://learn.microsoft.com/en-us/windows-hardware/drivers/install/pnp-device-installation-signing-requirements--windows-vista-and-later-), [Driver signing options](https://learn.microsoft.com/en-us/windows-hardware/drivers/dashboard/driver-signing-offerings), [Code signing requirements](https://learn.microsoft.com/en-us/windows-hardware/drivers/dashboard/code-signing-reqs), [Attestation how-to](https://learn.microsoft.com/en-us/windows-hardware/drivers/dashboard/code-signing-attestation), [Hardware program registration](https://learn.microsoft.com/en-us/windows-hardware/drivers/dashboard/hardware-program-register), [Driver block rules](https://learn.microsoft.com/en-us/windows/security/application-security/application-control/app-control-for-business/design/microsoft-recommended-driver-block-rules), [Smart App Control](https://support.microsoft.com/en-us/windows/what-is-smart-app-control-285ea03d-fa88-4d56-882e-6698afdb7003), [Artifact Signing quickstart](https://learn.microsoft.com/en-us/azure/artifact-signing/quickstart), [SendInput](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput), [InjectSyntheticPointerInput](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-injectsyntheticpointerinput), [Desktop Duplication](https://learn.microsoft.com/en-us/windows/win32/direct3ddxgi/desktop-dup-api), [MSIX prepare](https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-prepare), [GameInput overview](https://learn.microsoft.com/en-us/gaming/gdk/docs/features/common/input/overviews/input-overview?view=gdk-2604).
- Nefarius: [ViGEm End of Life](https://docs.nefarius.at/projects/ViGEm/End-of-Life/), [ViGEmBus repo](https://github.com/nefarius/ViGEmBus), [issue #139](https://github.com/nefarius/ViGEmBus/issues/139).
- LizardByte: [libvirtualhid announcement](https://app.lizardbyte.dev/2026-08-16-introducing-libvirtualhid-and-virtual-hid-driver/), [store](https://app.lizardbyte.dev/store/), local checkout `libvirtualhid/docs/*.md`, `LICENSES/*`, `src/platform/windows/driver/libvirtualhid.inf.in`; `Sunshine/tools/sunshinesvc.cpp`, `src/platform/windows/{misc,input,display_base}.cpp`, `docs/getting_started.md`.
- Apollo: `src_assets/windows/drivers/sudovda/{install,uninstall}.bat`, `sudovda.cer`, `cmake/packaging/windows_nsis.cmake`, `src/platform/windows/virtual_display.cpp`, [issue #1471](https://github.com/ClassicOldSong/Apollo/issues/1471), [#1202](https://github.com/ClassicOldSong/Apollo/issues/1202).
- SudoVDA: local checkout README, `SudoVDA.vcxproj`, `Driver.cpp`; [GitHub releases (empty)](https://github.com/SudoMaker/SudoVDA/releases).
- VirtualDrivers VDD: [repo](https://github.com/VirtualDrivers/Virtual-Display-Driver), [#446](https://github.com/VirtualDrivers/Virtual-Display-Driver/issues/446), [#471](https://github.com/VirtualDrivers/Virtual-Display-Driver/issues/471).
- HIDMaestro: [repo](https://github.com/hifihedgehog/HIDMaestro). Microsoft sample: [vhidmini2](https://github.com/microsoft/Windows-driver-samples/tree/main/hid/vhidmini2).
- RustDesk: `src/platform/windows.cc`, `src/platform/windows_delete_test_cert.cc`, `libs/virtual_display/dylib/README.md`, [CVE-2024-25140](https://nvd.nist.gov/vuln/detail/CVE-2024-25140).
- Parsec (search excerpts; site blocks fetches): gamepad article 4407439919629 / 32381705301908, error 14003 article 360002165172, [Broadcom KB 449607](https://knowledge.broadcom.com/external/article/449607/unable-to-interact-with-notifier-prompt.html).
- Steam: [Steam Input gamepad emulation best practices](https://partner.steamgames.com/doc/features/steam_controller/steam_input_gamepad_emulation_bestpractices).
- Signing/market: [SSL.com sole proprietor EV](https://www.ssl.com/products/software-integrity/code-signing/ev-sole-proprietor/), [sslinsights pricing](https://sslinsights.com/digicert-vs-sectigo-code-signing/), [DigiCert SmartScreen note](https://knowledge.digicert.com/alerts/ev-signed-application-showing-microsoft-defender-smartscreen-warnings), [ToDesktop PSA](https://www.todesktop.com/blog/posts/windows-apps-psa-ev-certs-do-not-grant-immediate-reputation-anymore), [Neowin on April 2026 kernel policy](https://www.neowin.net/news/microsoft-is-changing-a-windows-kernel-policy-thats-been-around-for-decades/), [Riot VAN:RESTRICTION 4](https://support.riotgames.com/riot/penalties/error-van-restriction-4).
