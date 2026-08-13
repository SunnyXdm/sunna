# Trackpad Gestures Over Remote Sessions: Why They Die and How to Fix Them

> Research compiled 2026-08-05. Part 5 of the "build something better than Parsec" research series.
> Sources: own analysis + a deep-dive consultation with OpenAI Codex (its claims cross-checked; verification status noted inline). This is a genuine differentiator gap — no mainstream game-streaming product handles trackpad gestures; only Citrix (2026) does it commercially.

## 1. The problem: a gesture dies three times

Using a remote Mac from a local Mac, three/four-finger swipes (Spaces, Mission Control) never reach the host — worse, they trigger the *local* machine's Spaces switch and yank you out of the stream window.

1. **Client capture:** macOS consumes system gestures at the WindowServer/Dock level before the frontmost app sees them. Apps only receive the domesticated leftovers: two-finger scroll as precise scroll-wheel events (with `phase`/`momentumPhase`), and magnify/rotate/swipe NSEvents while focused. Raw multitouch needs `NSTouch` (focused window, `allowedTouchTypes = [.indirect]`) or the private `MultitouchSupport.framework` (`MTDeviceCreateDefault`, `MTRegisterContactFrameCallback`, `MTDeviceStart` — BetterTouchTool's approach; [example gist](https://gist.github.com/KSroido/f03202a40e57fe4eff37f38cf96f9b56)). Observation ≠ suppression: even if you see the contacts, the local system action still fires.
2. **Wire protocol:** Parsec's SDK has only keyboard/mouse-button/wheel/motion/gamepad messages — no touch or gesture vocabulary. Moonlight/Sunshine do have native pen/touch packets now, but they carry absolute *touchscreen* contacts, not an indirect trackpad contact stream ([Sunshine docs](https://docs.lizardbyte.dev/projects/sunshine/v0.22.1/about/advanced_usage.html)).
3. **Host injection:** macOS has no public API to inject multitouch or gesture events. `CGEventPost` does keyboard, mouse, and scroll only — scroll phases + momentum are public fields, which is exactly why two-finger scrolling survives remote sessions while everything else dies.

## 2. macOS host-side injection: the full map (per Codex survey)

| Path | What it can realistically do | Verdict |
|---|---|---|
| `CGEventPost` mouse/keyboard/scroll | Pointer, clicks, pixel scroll with phases/momentum, shortcuts | Production-ready |
| Private forged gesture CGEvents | Application magnify/rotate/swipe/smart-zoom | Experimental |
| CoreHID `HIDVirtualDevice` / HIDDriverKit | Create virtual HID reports/services | Public API, but native trackpad recognition **unproven — one careful attempt failed** |
| `VZMacTrackpadConfiguration` | Real multitouch gestures in a macOS VM guest | Supported, **VM-only** |
| VoodooInput-style HID service | Native Magic-Trackpad-like behavior | Proven on Hackintosh kexts; architecture unsupported on modern macOS |
| Exact USB/BLE Magic Trackpad 2 emulation | Might bind Apple's own trackpad driver | Plausible hardware R&D; no proven public project |
| Private `IOHIDEventSystemClient` digitizer dispatch | Low-level event injection | Private/entitlement-gated; research only |

Details worth keeping:

- **Private gesture CGEvents are real, not folklore.** Hammerspoon ships `hs.eventtap.event.newGesture("beginMagnify"/"beginSwipeLeft"/...)`, built on Calf Trail's reverse-engineered `tl_CGEventCreateFromGesture` ([Hammerspoon API](https://www.hammerspoon.org/docs/hs.eventtap.event.html), [TouchEvents.h](https://github.com/Hammerspoon/hammerspoon/blob/master/extensions/eventtap/TouchEvents.h), [author's explanation](https://stackoverflow.com/questions/21082362/how-to-send-raw-multitouch-trackpad-data-under-mac-os-x)). Caveats: it posts an *already-classified* gesture event rather than finger contacts, so it likely works for app-level magnify/rotate/swipe but there's no convincing proof it drives the interactive Mission Control/Spaces recognizer; private structure layouts change across macOS releases; the Calf Trail code is GPL; needs Accessibility permission.
- **CoreHID negative result:** Apple's public [`HIDVirtualDevice`](https://developer.apple.com/documentation/corehid/hidvirtualdevice) can create virtual HID devices, and HIDDriverKit has [`dispatchDigitizerTouchEvent`](https://developer.apple.com/documentation/hiddriverkit/iouserhideventservice/dispatchdigitizertouchevent) — but a documented experiment with a HID-standard clickpad (works as full Precision Touchpad on Windows) produced **no pointer motion or gestures on macOS 14.7** even though reports reached the event system ([Apple dev forums thread](https://developer.apple.com/forums/thread/768586)). A standards-compliant "Touch Pad" descriptor does not enter Apple's native trackpad pipeline; that pipeline appears reserved for Apple's own devices/drivers.
- **`VZMacTrackpadConfiguration`** ([docs](https://developer.apple.com/documentation/virtualization/vzmactrackpadconfiguration)) is Apple's supported virtual Mac trackpad — but only inside Virtualization.framework guests (macOS 13+), fed via `VZVirtualMachineView`. If the host is a macOS VM you control (cloud Mac, VM-based multi-seat), full native gestures are achievable *today*. This is a compelling special case for Sunna's architecture.
- **[VoodooInput](https://github.com/acidanthera/VoodooInput)** (Hackintosh ecosystem) proves arbitrary contact data can become native macOS gestures via Magic Trackpad 2 software emulation — but as a kext, an architecture Apple stopped supporting (all IOHIDFamily KPIs unsupported since Big Sur; [kext migration](https://developer.apple.com/support/kernel-extensions/)). Great reverse-engineering reference, not a shippable component.
- **Magic Trackpad 2 wire protocol** is substantially documented in Linux's [`hid-magicmouse.c`](https://kernel.googlesource.com/pub/scm/linux/kernel/git/hid/hid/+/refs/heads/for-4.20/apple/drivers/hid/hid-magicmouse.c) (BT report `0x31`, USB `0x02`, 9-byte per-touch records, feature-report handshake) — an external USB/BLE gadget emulating Apple's VID/PID + descriptors *might* bind Apple's private driver. Unproven end-to-end; hardware R&D territory.

**Bottom line for macOS hosts:** no supported path injects real multi-finger contacts into the native gesture recognizer on an arbitrary physical Mac. Semantic replay is the shippable backend; forged gesture events are a worthwhile experiment; VM hosts get the real thing via Virtualization.framework.

## 3. Client-side capture and the UX policy

There's no zero-tradeoff capture. The strongest suppression is exclusive HID access — `IOHIDDeviceOpen(device, kIOHIDOptionsTypeSeizeDevice)` ([docs](https://developer.apple.com/documentation/iokit/1588670-iohiddeviceopen)) — which prevents the system from seeing the device at all, but is only sane for a *dedicated external* Magic Trackpad (seizing the built-in top-case risks locking the local machine). A `CGEventTap` is not a reliable suppressor (system actions fire upstream; swipe events are inconsistently visible at that layer).

The Citrix-validated UX pattern (worth copying):

- Default to local system gestures; offer an explicit **"send trackpad gestures to remote" capture mode**, active only while the stream window is focused/fullscreen, with an unmistakable on-screen indicator.
- **Hold `fn` to route a gesture locally** while captured (Citrix's exact mechanism).
- Always keep a keyboard escape chord; release capture on focus loss, disconnect, or watchdog timeout.
- Optional exclusive-device mode for an external trackpad; otherwise tell users to reassign conflicting local gestures — never silently edit their settings.

## 4. The semantic gesture channel (recommended core design)

Send more than a gesture name — carry enough state for native-feeling replay:

```text
sequence_id, device_id, timestamp_us
kind, finger_count, phase (begin/update/end/cancel)
centroid_x/y, translation_dx/dy, velocity_x/y
scale_delta_log, rotation_delta, modifiers, committed
# optional raw contacts:
contact_id, x, y, major, minor, pressure, state
```

Negotiate per-gesture host capability: `native_contacts | native_gesture | semantic_action | shortcut_only | unsupported`. **Keep raw contacts in the protocol even if the macOS MVP ignores them** — Windows and Linux hosts can consume them natively (below), and a future macOS backend might.

Mapping quality tiers on a macOS host:

- **High fidelity:** two-finger scroll (scroll CGEvents with begin/change/end + momentum phases — macOS already models it as scroll, not gesture); tap/click/secondary click; three-finger drag (down → moves → up).
- **Works but visibly non-native:** Mission Control / App Exposé / Spaces via configured shortcuts (Ctrl+↑/↓/←/→) — commits the action but loses the interactive scrub/reversal/velocity; pinch → Cmd+/− or Ctrl+scroll (app-dependent, loses focal point); page back/forward (no universal mapping); forged magnify/rotate events where they work.
- **Fails:** arbitrary continuous rotation; smart zoom; Force Click/Look Up; native interactive Spaces scrubbing; gestures inside games that remap the fallback shortcut; secure/login-window contexts.

## 5. Windows and Linux hosts: actually solvable

**Windows — new first-class API (verified 2026-08-05 on MS Learn):** Windows 11 now documents [`CreateSyntheticPointerDevice2`](https://learn.microsoft.com/en-us/windows/win32/input-precisiontouchpad/createsyntheticpointerdevice2) with `pointerType = PT_TOUCHPAD` + [`InjectSyntheticPointerInput`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-injectsyntheticpointerinput): up to 5 contacts, physical dimensions in himetric required (`SDCO_PHYSICAL_SIZE`), and without `SDCO_TOUCHPAD_GESTURE_ONLY` "the system will treat the injected touchpad input identically to physical touchpad input" — i.e., real three/four-finger shell gestures. There's also [`InjectTouchpadAction`](https://learn.microsoft.com/en-us/windows/win32/input-precisiontouchpad/injecttouchpadaction) for discrete 3/4/5-finger tap/press actions. Caveats: docs carry the **pre-release banner**; Windows 11 only, no Server; test on the exact build before relying on it. Fallback for older Windows: a signed KMDF driver on the [Virtual HID Framework](https://learn.microsoft.com/en-us/windows-hardware/drivers/hid/virtual-hid-framework--vhf-) exposing a [Precision Touchpad descriptor](https://learn.microsoft.com/en-us/windows-hardware/design/component-guidelines/touchpad-protocol-implementation) — same driver-signing cost bucket as the virtual gamepad/display drivers already planned.

**Linux — solvable with uinput, classification is the trap:** create an **indirect multitouch touchpad** (not a touchscreen) via `/dev/uinput` using type-B slots (`ABS_MT_SLOT`, `ABS_MT_TRACKING_ID`, `ABS_MT_POSITION_X/Y`, tracking ID −1 on lift; [kernel MT protocol](https://kernel.org/doc/html/latest/input/multi-touch-protocol.html), prefer [libevdev-uinput](https://kernel.org/doc/html/latest/input/uinput.html)). Expose `INPUT_PROP_POINTER` (+`INPUT_PROP_BUTTONPAD` for a clickpad), `BTN_TOOL_FINGER`…`BTN_TOOL_QUINTTAP`, accurate ranges/resolution, and ensure udev classifies it `ID_INPUT_TOUCHPAD=1` — if it lands as touchscreen, libinput will never produce touchpad gestures ([libinput udev config](https://wayland.freedesktop.org/libinput/doc/latest/device-configuration-via-udev.html), [gesture events](https://wayland.freedesktop.org/libinput/doc/latest/api/group__event__gesture.html)). Then GNOME/KDE workspace swipes come from the compositor's normal libinput gesture handling. Caveats: per-compositor differences; `/dev/uinput` is a system-wide input capability — use a narrow privileged helper; portal/libei has no synthetic-touchpad vocabulary yet. Prior art: [Weylus](https://github.com/H-M-H/Weylus) (uinput multitouch, though touchscreen-style).

## 6. Prior art scorecard

- **Citrix Workspace/VDA for macOS 2603 (verified 2026-08-05):** the commercial proof it's possible Mac-to-Mac — native-like trackpad gestures with a local/remote ownership setting and `fn`-hold override; limitations: two-finger tap/swipe remain mouse ops, three-finger tap unsupported, only Apple trackpads validated ([docs](https://docs.citrix.com/en-us/mac-vda/configure/keyboard/apple-trackpad-support.html)). Mechanism undisclosed; black-boxing their VDA (watch IORegistry/HID services + event taps during gestures) would reveal whether they use a virtual HID service or private gesture events.
- **Apple Universal Control:** most gestures work across Macs/iPads — first-party WindowServer/Continuity integration, no public API ([guide](https://support.apple.com/en-am/102459)). Lesson: ownership arbitration before local gesture handling.
- **Apple Sidecar:** fixed semantic set (two-finger scroll, three-finger copy/paste gestures); macOS 27 beta adds app-facing gesture recognizers for Sidecar touch ([TN3212](https://developer.apple.com/documentation/technotes/tn3212-adopting-gesture-recognizers-for-sidecar-touch-support)) — still not a general injection API.
- **Screens:** pure semantic translation — three-finger swipes → Ctrl+arrows ([docs](https://support.edovia.com/en/screens-4/features/cursor-control-modes-and-other-gestures)). **Jump Desktop:** gestures are client-viewer controls only. **HP Anyware/PCoIP:** explicitly reserves Mission Control gestures for the local Mac, tells users to use keyboard commands remotely ([docs](https://anyware.hp.com/web-help/pcoip_client/mac/22.07/in-session/using-mission-control/)). **Parsec/Moonlight:** nothing for trackpads.

## 7. Recommended strategy for Sunna

1. **Ship the semantic gesture channel** as the supported backend everywhere: phase/momentum scroll done perfectly, host-configurable shortcut replay for system actions, Citrix-style capture mode + `fn` escape on the Mac client.
2. **Windows and Linux hosts get real gestures** from the raw-contacts protocol path: `PT_TOUCHPAD` injection on Windows 11 (VHF driver fallback), uinput indirect touchpad on Linux. This alone beats every incumbent — nobody forwards trackpad gestures to Windows/Linux hosts today.
3. **macOS host, two opt-in experimental layers:** (a) private MultitouchSupport capture client-side; (b) Hammerspoon/Calf-Trail-style forged gesture events for app-level magnify/rotate/swipe, falling back to shortcuts.
4. **Bounded experiments:** compare `HIDVirtualDevice` output vs a physical Magic Trackpad in `ioreg` (standards descriptor, then Apple-exact descriptor/reports); black-box Citrix 2603's VDA.
5. **VM special case:** for macOS-VM hosts, use `VZMacTrackpadConfiguration` for fully native gestures.
6. **Don't gate launch** on native physical-Mac Spaces scrubbing — it's the one piece with no supported mechanism; everything else above is buildable.

## Verification notes

- Verified directly (2026-08-05): `CreateSyntheticPointerDevice2`/`PT_TOUCHPAD`/`SDCO_TOUCHPAD_GESTURE_ONLY` semantics and pre-release status on MS Learn; Citrix mac-vda 2603 trackpad support, `fn` override, and limitations.
- High confidence, consistent with prior knowledge: CGEvent scroll phases, MultitouchSupport, NSTouch behavior, uinput/libinput mechanics, VoodooInput, Hammerspoon `newGesture`, kext deprecation.
- Reported by Codex, not independently verified: the Apple forums HIDVirtualDevice negative result (thread 768586), hid-magicmouse report-format specifics, macOS 27 Sidecar TN3212, Sidecar/Universal Control gesture inventories. Check before building on them.

## Sources

- https://learn.microsoft.com/en-us/windows/win32/input-precisiontouchpad/createsyntheticpointerdevice2 (verified)
- https://learn.microsoft.com/en-us/windows/win32/input-precisiontouchpad/injecttouchpadaction
- https://learn.microsoft.com/en-us/windows-hardware/drivers/hid/virtual-hid-framework--vhf-
- https://learn.microsoft.com/en-us/windows-hardware/design/component-guidelines/touchpad-protocol-implementation
- https://docs.citrix.com/en-us/mac-vda/configure/keyboard/apple-trackpad-support.html (verified)
- https://www.hammerspoon.org/docs/hs.eventtap.event.html and https://github.com/Hammerspoon/hammerspoon/blob/master/extensions/eventtap/TouchEvents.h
- https://stackoverflow.com/questions/21082362/how-to-send-raw-multitouch-trackpad-data-under-mac-os-x
- https://developer.apple.com/documentation/corehid/hidvirtualdevice
- https://developer.apple.com/documentation/hiddriverkit/iouserhideventservice/dispatchdigitizertouchevent
- https://developer.apple.com/forums/thread/768586 (HID touchpad negative result on macOS)
- https://developer.apple.com/documentation/virtualization/vzmactrackpadconfiguration
- https://github.com/acidanthera/VoodooInput
- https://kernel.googlesource.com/pub/scm/linux/kernel/git/hid/hid/+/refs/heads/for-4.20/apple/drivers/hid/hid-magicmouse.c
- https://developer.apple.com/support/kernel-extensions/
- https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/EventOverview/HandlingTouchEvents/HandlingTouchEvents.html
- https://developer.apple.com/documentation/iokit/1588670-iohiddeviceopen
- https://gist.github.com/KSroido/f03202a40e57fe4eff37f38cf96f9b56 (MultitouchSupport example)
- https://kernel.org/doc/html/latest/input/multi-touch-protocol.html and https://kernel.org/doc/html/latest/input/uinput.html
- https://wayland.freedesktop.org/libinput/doc/latest/device-configuration-via-udev.html and https://wayland.freedesktop.org/libinput/doc/latest/api/group__event__gesture.html
- https://github.com/H-M-H/Weylus
- https://support.apple.com/en-am/102459 (Universal Control) and https://support.apple.com/en-gb/102597 (Sidecar)
- https://developer.apple.com/documentation/technotes/tn3212-adopting-gesture-recognizers-for-sidecar-touch-support
- https://support.edovia.com/en/screens-4/features/cursor-control-modes-and-other-gestures
- https://support.jumpdesktop.com/hc/en-us/articles/216423503-Getting-Started-Jump-Desktop-Controls-and-Gesture-Reference
- https://anyware.hp.com/web-help/pcoip_client/mac/22.07/in-session/using-mission-control/
- https://docs.lizardbyte.dev/projects/sunshine/v0.22.1/about/advanced_usage.html
