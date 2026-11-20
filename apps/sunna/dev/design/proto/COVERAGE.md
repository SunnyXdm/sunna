# Sunna prototype coverage

## Open and test

Open [Desktop](desktop.html) in a normal browser window, or [Phone](phone.html) on the Android phone. Add the phone page to the home screen for full screen. The landing page is [index.html](index.html). Classic scripts and local assets also support `file://`.

All computer pictures, probes, network results, remote input, permissions, and notifications are simulated. No real computer is contacted. Controls retain their values in the tab. The last selected index alone is saved locally; Test → Reset clears it. The existing server at `http://100.108.56.3:8790/` was not changed.

## URL parameters

Parameters apply to either `desktop.html` or `phone.html`. `case` applies first, other parameters override it. Values containing spaces or punctuation must be URL-encoded.

| Parameter | Values / effect |
|---|---|
| `case` | Every numbered inventory state has an ID, e.g. `L01.10`, `V06.4`, `A12.6`. Tables below link each one. |
| `view` | `computer`, `overview`, `add`, `edit`, `find`, `qr`, `settings`, `licenses`, `connecting`, `session`, `menu`. Direct connecting links hold the selected phase; clicking Connect runs the timed flow. |
| `sel` | Zero-based computer index. |
| `computers` | 0–12; Test offers 0, 1, 2, 5, 12. |
| `state.<id>` | IDs `arch`, `mac`, `studio`, `ubuntu`, `gaming`, then `computer-5` … `computer-11`; numeric indexes also work. Values: `loading`, `idle`, `checking`, `ready-no-latency`, `ready`, `busy-anonymous`, `busy`, `key`, `update`, `update-client`, `not-found`, `invalid`, `unreachable`, `offline`, `never`. |
| `state` | State of the selected computer. Same values. |
| `fail` / `ended` | Show a failed/ended reason with Try again / Reconnect at level 1. Keys: `unexpected`, `crashed`, `timeout`, `host`, `replaced`, `key`, `busy`, `busy-anonymous`, `decoder`, `version`, `unreachable`, `unanswered`, `spawn`, `already`, `missing`, `resolution`, `invalid`, `raw`, `cancelled`. Arbitrary text is also accepted for unbounded host/library errors. |
| `nextFail` | `success` or any failure key above; affects the next actual Connect. |
| `phase` | `reaching`, `video`, `connected`, `ended`, `cancel`, `waiting`. `waiting` shows pre-codec menu/stats. |
| `notice` | `help`, `unavailable`, `captured`, `released`, `changing`, `changed`, `stream-error`, `clipboard-on`, `clipboard-off`, `clipboard-empty`, `typing`, `skipped`, `sent`, `sound-on`, `sound-off`, `hidden`, `no-drawing`, `no-video`, `unsupported`, `touch`, `trackpad`, `link`, `copied`, `storage`, `action`, `normal`; or arbitrary text. Defaults to session unless `view` is supplied. |
| `submenu` | `keyboard`, `clipboard`, `video`; use `view=menu`. Phone scrolls its list to that group. |
| `form` | `idle`, `checking`, `ready`, `ready-no-latency`, `busy`, `busy-anonymous`, `key`, `update`, `update-client`, `unreachable`, `not-found`, `invalid`, `invalid-ipv6`, `invalid-tail`, `invalid-port`, `invalid-key`, `empty`, `pending`, `save-error`; use `view=add` or `edit`. |
| `link=1`, `reveal=1` | Link-populated form helper; reveal form/self key. |
| `discovery` | `loading`, `results`, `empty`, `error`, `signed-out`, `output`, `rescan`; use `view=find`. |
| `qr` | `generating`, `ready`, `error`; use `view=qr`. |
| `self` | `loading`, `ready`, `busy`, `key`, `network`, `linux`, `mac`; use `view=settings`. |
| `permission` | `not-requested`, `denied`, `opened`, `allowed`, `reset`, `reset-failed`. |
| `permissionShown=1` | Show the permission row at home, or in phone Settings. Desktop Settings always has its Accessibility row. |
| `context=1`, `remove=1` | Anchored computer menu; inline removal confirmation. |
| `stats`, `sound`, `menu`, `capture`, `clipboard`, `fullscreen`, `keyboardButton`, `suggestions`, `remember`, `previews`, `fast` | Boolean control overrides. Use `true`/`false` (the common UI flags also accept 1/0). |
| `touch` | `Trackpad` or `Touch`. |
| `codec`, `resolution`, `fps`, `bitrate` | The exact choice label, URL-encoded; desktop and phone use their respective inventory labels. |
| `keyboard=1`, `modifier` | Draw the phone keyboard; `armed` or `locked` sets Ctrl on the accessory strip. |
| `statsDetail`, `decoder` | `basic`, `host`, `latency`, `decoder`, `rtt`, `loss`, `sound`, `full`; decoder: `hardware` / `software`. |
| `pointer`, `zoom`, `floatX` | `remote`, `fallback`, `outside`; local picture scale 1–3; movable phone buttons' x position in pixels. |
| `notification` | `starting`, `connected`, `background`, `denied`, `disconnected`. Drawn only in Test. |
| `scroll` | Pixel scroll offset for Settings, session controls, or Overview. Useful for all rows of a long view. |
| `toast`, `toastAction` | Arbitrary capsule text; optional `edit` action. |
| `saveError=true`, `removeError=true`, `streamError=true` | Force storage or stream request errors during real interactions. |
| `licensesEmpty=true`, `idle=true`, `pulse=true` | Empty license list, idle phone buttons, fixed pointer tap pulse. |
| `longNames=1` | Long-name stress fixture; also available as Test → Long names. Defaults: archlinux, MacBook Air, studio, ubuntu, gaming-pc. |
| `test=0`, `testOpen=true` | Completely hide Test, or open it. `test=0` wins. |
| `still=1` | Jump motion to final state, stop preview/spinner motion, hold notices for screenshots. |
| `reduced=true` | 150 ms cross-fades instead of zooms; stop live preview motion. OS `prefers-reduced-motion` is also honoured. |

Example: [wrong key on phone](phone.html?state.arch=key&test=0&still=1); [Mac keyboard submenu](desktop.html?view=menu&sel=1&submenu=keyboard&test=0&still=1); [12 computers](desktop.html?view=overview&computers=12&test=0&still=1).

## Interaction paths

- Home is Computer, at the last selected computer. All computers / Escape opens Overview. Click a screen there to choose it; double-click or Return connects. Arrow keys navigate. The final empty slot and + open Add.
- Desktop: ⌘N adds, ⌘, opens Settings, ⌘E edits, ⌘⌫ / Delete removes inline, ⌘1–9 selects/connects, ⌘R simulates refresh. Ctrl also works for launcher shortcuts. ContextMenu / Shift+F10 opens the computer menu; its arrow keys cycle the items.
- Desktop session: ⌃⌥M opens/closes menu, ⌃⌥G captures/releases shortcut routing, ⌃⌥F changes window/full-screen choice, ⌃⌥S toggles stats, ⌃⌥Q disconnects. Escape closes submenus/menu before leaving the session.
- Phone: swipe the stage, tap Connect or the screen, long-press the name, pinch inward for Overview. Session pointer and local pinch have visual feedback. Drag floating buttons sideways. Keyboard uses a drawn panel; modifier tap arms, double tap locks, third tap unlocks. Back/Escape closes menu, hides keyboard, or opens the session menu before leaving via Disconnect.
- Add: type any plausible address; it answers after 600 ms. `bad…` does not answer; `missing…` is not found; `wrong…` keys fail; `old…` / `newer…` need an update; `busy…` is in use. Paste a `sunna://address?key=value` link into Address to fill both. Edit checks identically. The desktop Find list fills an address. Phone Paste reads the browser clipboard when allowed.
- Test includes every numbered checklist state, every computer state, every mapped failure/end reason and session notice, counts, permission rows, reduced motion and reset.

## Decisions made

1. Guide sentence case supersedes original title-case controls. Chord notation, proper nouns, status sentences and error wording remain intact. The startup help says “top right” because the guide moves ••• there.
2. Copy address remains in the computer menu; Copy link is added because the explicit task requests it. Desktop Show QR code remains; phone does not gain QR output. Move left/right is omitted because inventory explicitly says no reorder UI and says to add no menu items beyond its scope.
3. Phone Scan QR code is an explanatory camera handoff, not a new in-app scanner or a camera permission request. The drawn QR is deliberately not scannable, as permitted.
4. Add/save is enabled only for a successful check, following the explicit brief and guide §7, although today's apps allow saving unavailable or wrong-key computers. The checklist's blocked and storage-error states remain reachable.
5. The guide's views replace legacy sheets, alert dialogs, No Signal animation, progress tracks and bottom notices. Remove uses inline Keep/Remove. Notice and stats capsules are top-centre in sessions. No new reconnecting state is invented because the inventory says none exists.
6. Screen dimensions, host metadata, preview contents, timing, addresses and metrics are deterministic stand-ins. Desktop initial stream uses HEVC, native size, 60 fps, 20 Mbps and fast lane on; inventory says those are negotiated rather than universal defaults. Phone defaults exactly follow its preference table. About uses repository version 0.0.1 and protocol 8.
7. Desktop stats wrap between complete fields when narrow, preserving every field. Long menus and Settings scroll. The desktop menu's screen shrinks further on short windows to keep controls and other-computer names usable. Twelve-computer navigation scrolls horizontally; Overview preserves justified rows.
8. In phone landscape the real app fills the viewport even when its width exceeds 600 px. The decorative 390×844 frame is used only when both viewport dimensions exceed 600 px. This resolves the width-only framing rule against the requested 844×390 landscape test.
9. The drawn system keyboard uses standard QWERTY rows (10/9/7), with Shift/Backspace and 123/space/return. Its picture keys may be narrower than 44px; the Sunna accessory strip retains 44px targets. System input fields still invoke the browser keyboard when tapped.
10. Linux mark geometry is copied from the repository SVG; its yellow details become white to satisfy the single-colour rule. Credit is retained in About. Android and Windows marks are simple monochrome outlines.
11. Permission actions stay inside the prototype: Allow/Reset update the handoff row and explain the simulated system destination. Test supplies grant/denial outcomes. No real OS dialog is requested by a static file.
12. Only the last selected index is persisted; all machine edits and preferences are tab-local. The title's long-name truncation and carousel neighbour peeks are intentional. One preview change per second at most: caret blinks every second; clocks update each minute. The guide's explicit 300 ms first-video reveal and ring spinner are the only exceptions to its two springs / 200 ms fade rule.

## Inventory surfaces and every numbered checklist state

A link opens a deterministic visual state. Ended/cancelled/dismissed states show their resulting destination. Native behaviour that a static browser cannot provide is explicitly marked below; its representative visual is still available where useful.


### L01. Computers home and machine cards

| State | Where / how to reach it |
|---|---|
| L01.1 — initial loading | [L01.1](desktop.html?case=L01.1&still=1&test=0) — computer |
| L01.2 — empty first run | [L01.2](desktop.html?case=L01.2&still=1&test=0) — add |
| L01.3 — populated grid | [L01.3](desktop.html?case=L01.3&still=1&test=0) — overview |
| L01.4 — idle address preview | [L01.4](desktop.html?case=L01.4&still=1&test=0) — computer |
| L01.5 — checking | [L01.5](desktop.html?case=L01.5&still=1&test=0) — computer |
| L01.6 — ready without latency | [L01.6](desktop.html?case=L01.6&still=1&test=0) — computer |
| L01.7 — ready with latency | [L01.7](desktop.html?case=L01.7&still=1&test=0) — computer |
| L01.8 — busy without viewer | [L01.8](desktop.html?case=L01.8&still=1&test=0) — computer |
| L01.9 — busy with viewer | [L01.9](desktop.html?case=L01.9&still=1&test=0) — computer |
| L01.10 — wrong-key | [L01.10](desktop.html?case=L01.10&still=1&test=0) — computer |
| L01.11 — update-needed | [L01.11](desktop.html?case=L01.11&still=1&test=0) — computer |
| L01.12 — not-found | [L01.12](desktop.html?case=L01.12&still=1&test=0) — computer |
| L01.13 — invalid | [L01.13](desktop.html?case=L01.13&still=1&test=0) — computer |
| L01.14 — unreachable never seen | [L01.14](desktop.html?case=L01.14&still=1&test=0) — computer |
| L01.15 — unreachable previously seen | [L01.15](desktop.html?case=L01.15&still=1&test=0) — computer |

### L02. Add / edit computer sheet

| State | Where / how to reach it |
|---|---|
| L02.1 — new blank | [L02.1](desktop.html?case=L02.1&still=1&test=0) — add; live check: idle |
| L02.2 — editing saved | [L02.2](desktop.html?case=L02.2&still=1&test=0) — edit; live check: ready |
| L02.3 — link pasted | [L02.3](desktop.html?case=L02.3&still=1&test=0) — add; live check: ready |
| L02.4 — checking | [L02.4](desktop.html?case=L02.4&still=1&test=0) — add; live check: checking |
| L02.5 — ready | [L02.5](desktop.html?case=L02.5&still=1&test=0) — add; live check: ready |
| L02.6 — busy | [L02.6](desktop.html?case=L02.6&still=1&test=0) — add; live check: busy |
| L02.7 — wrong-key | [L02.7](desktop.html?case=L02.7&still=1&test=0) — add; live check: key |
| L02.8 — update-needed | [L02.8](desktop.html?case=L02.8&still=1&test=0) — add; live check: update |
| L02.9 — unreachable | [L02.9](desktop.html?case=L02.9&still=1&test=0) — add; live check: unreachable |
| L02.10 — not-found | [L02.10](desktop.html?case=L02.10&still=1&test=0) — add; live check: not-found |
| L02.11 — invalid | [L02.11](desktop.html?case=L02.11&still=1&test=0) — add; live check: invalid |
| L02.12 — key revealed | [L02.12](desktop.html?case=L02.12&still=1&test=0) — add; live check: ready |
| L02.13 — submit pending | [L02.13](desktop.html?case=L02.13&still=1&test=0) — add; live check: pending |
| L02.14 — save failure | [L02.14](desktop.html?case=L02.14&still=1&test=0) — add; live check: save-error |

### L03. Find on Tailscale page

| State | Where / how to reach it |
|---|---|
| L03.1 — loading | [L03.1](desktop.html?case=L03.1&still=1&test=0) — find |
| L03.2 — sharing results | [L03.2](desktop.html?case=L03.2&still=1&test=0) — find |
| L03.3 — no sharing computers | [L03.3](desktop.html?case=L03.3&still=1&test=0) — find |
| L03.4 — scan error | [L03.4](desktop.html?case=L03.4&still=1&test=0) — find |
| L03.5 — rescan | [L03.5](desktop.html?case=L03.5&still=1&test=0) — find |
| L03.6 — already added | [L03.6](desktop.html?case=L03.6&still=1&test=0) — find |
| L03.7 — not sharing | [L03.7](desktop.html?case=L03.7&still=1&test=0) — find |
| L03.8 — offline | [L03.8](desktop.html?case=L03.8&still=1&test=0) — find |

### L04. Settings sheet

| State | Where / how to reach it |
|---|---|
| L04.1 — loading self status | [L04.1](desktop.html?case=L04.1&still=1&test=0) — settings |
| L04.2 — sharing ready | [L04.2](desktop.html?case=L04.2&still=1&test=0) — settings |
| L04.3 — sharing busy | [L04.3](desktop.html?case=L04.3&still=1&test=0) — settings |
| L04.4 — sharing different key | [L04.4](desktop.html?case=L04.4&still=1&test=0) — settings |
| L04.5 — no Tailscale address | [L04.5](desktop.html?case=L04.5&still=1&test=0) — settings |
| L04.6 — not sharing Linux | [L04.6](desktop.html?case=L04.6&still=1&test=0) — settings |
| L04.7 — not sharing Mac | [L04.7](desktop.html?case=L04.7&still=1&test=0) — settings |
| L04.8 — key masked | [L04.8](desktop.html?case=L04.8&still=1&test=0) — settings |
| L04.9 — key revealed | [L04.9](desktop.html?case=L04.9&still=1&test=0) — settings |
| L04.10 — permission denied | [L04.10](desktop.html?case=L04.10&still=1&test=0) — settings |
| L04.11 — permission allowed | [L04.11](desktop.html?case=L04.11&still=1&test=0) — settings |
| L04.12 — permission reset offered | [L04.12](desktop.html?case=L04.12&still=1&test=0) — settings |
| L04.13 — settings save error | [L04.13](desktop.html?case=L04.13&still=1&test=0) — settings |

### L05. Computer-link QR sheet

| State | Where / how to reach it |
|---|---|
| L05.1 — generating | [L05.1](desktop.html?case=L05.1&still=1&test=0) — qr |
| L05.2 — code ready | [L05.2](desktop.html?case=L05.2&still=1&test=0) — qr |
| L05.3 — generation failed | [L05.3](desktop.html?case=L05.3&still=1&test=0) — qr |

### L06. Remove confirmation

| State | Where / how to reach it |
|---|---|
| L06.1 — awaiting decision | [L06.1](desktop.html?case=L06.1&still=1&test=0) — computer |
| L06.2 — cancelled | [L06.2](desktop.html?case=L06.2&still=1&test=0) — computer |
| L06.3 — confirmed | [L06.3](desktop.html?case=L06.3&still=1&test=0) — computer |
| L06.4 — delete failed | [L06.4](desktop.html?case=L06.4&still=1&test=0) — computer |

### L07. macOS Accessibility handoff

| State | Where / how to reach it |
|---|---|
| L07.1 — permission missing on startup | [L07.1](desktop.html?case=L07.1&still=1&test=0) — computer |
| L07.2 — system settings opened | [L07.2](desktop.html?case=L07.2&still=1&test=0) — settings |
| L07.3 — permission granted | [L07.3](desktop.html?case=L07.3&still=1&test=0) — settings |
| L07.4 — old entry reset | [L07.4](desktop.html?case=L07.4&still=1&test=0) — settings |
| L07.5 — reset failed | [L07.5](desktop.html?case=L07.5&still=1&test=0) — settings |

### L08. Machine context menu

| State | Where / how to reach it |
|---|---|
| L08.1 — open ready | [L08.1](desktop.html?case=L08.1&still=1&test=0) — computer |
| L08.2 — open not ready | [L08.2](desktop.html?case=L08.2&still=1&test=0) — computer |
| L08.3 — dismissed | [L08.3](desktop.html?case=L08.3&still=1&test=0) — computer |

### L09. Connecting transition

| State | Where / how to reach it |
|---|---|
| L09.1 — reaching | [L09.1](desktop.html?case=L09.1&still=1&test=0) — connecting |
| L09.2 — starting video | [L09.2](desktop.html?case=L09.2&still=1&test=0) — connecting |
| L09.3 — connected | [L09.3](desktop.html?case=L09.3&still=1&test=0) — session |
| L09.4 — ended | [L09.4](desktop.html?case=L09.4&still=1&test=0) — computer |
| L09.5 — cancel requested | [L09.5](desktop.html?case=L09.5&still=1&test=0) — computer |
| L09.6 — startup failure | [L09.6](desktop.html?case=L09.6&still=1&test=0) — computer |

### L10. Toast / copy feedback

| State | Where / how to reach it |
|---|---|
| L10.1 — informational | [L10.1](desktop.html?case=L10.1&still=1&test=0) — computer |
| L10.2 — error | [L10.2](desktop.html?case=L10.2&still=1&test=0) — computer |
| L10.3 — success | [L10.3](desktop.html?case=L10.3&still=1&test=0) — computer |
| L10.4 — action available | [L10.4](desktop.html?case=L10.4&still=1&test=0) — computer |
| L10.5 — dismissed or replaced | [L10.5](desktop.html?case=L10.5&still=1&test=0) — computer |

### V01. Remote desktop window

| State | Where / how to reach it |
|---|---|
| V01.1 — waiting for first picture | [V01.1](desktop.html?case=V01.1&still=1&test=0) — connecting |
| V01.2 — fullscreen | [V01.2](desktop.html?case=V01.2&still=1&test=0) — session |
| V01.3 — windowed | [V01.3](desktop.html?case=V01.3&still=1&test=0) — session |
| V01.4 — live picture | [V01.4](desktop.html?case=V01.4&still=1&test=0) — session |
| V01.5 — focused | [V01.5](desktop.html?case=V01.5&still=1&test=0) — session |
| V01.6 — unfocused | [V01.6](desktop.html?case=V01.6&still=1&test=0) — session |
| V01.7 — native renderer unavailable | [V01.7](desktop.html?case=V01.7&still=1&test=0) — session; **not in the prototype as native behaviour:** Native CPU fallback renderer is not implemented; representative screen with overlays hidden only. This browser draws HTML stand-ins. |
| V01.8 — session ending | [V01.8](desktop.html?case=V01.8&still=1&test=0) — computer |

### V02. Live stats bar

| State | Where / how to reach it |
|---|---|
| V02.1 — hidden | [V02.1](desktop.html?case=V02.1&still=1&test=0) — session |
| V02.2 — connecting | [V02.2](desktop.html?case=V02.2&still=1&test=0) — session |
| V02.3 — streaming basic | [V02.3](desktop.html?case=V02.3&still=1&test=0) — session |
| V02.4 — host metrics available | [V02.4](desktop.html?case=V02.4&still=1&test=0) — session |
| V02.5 — latency available | [V02.5](desktop.html?case=V02.5&still=1&test=0) — session |
| V02.6 — decoder metrics available | [V02.6](desktop.html?case=V02.6&still=1&test=0) — session |
| V02.7 — RTT available | [V02.7](desktop.html?case=V02.7&still=1&test=0) — session |
| V02.8 — loss present | [V02.8](desktop.html?case=V02.8&still=1&test=0) — session |
| V02.9 — sound buffer available | [V02.9](desktop.html?case=V02.9&still=1&test=0) — session |
| V02.10 — fast-lane tiles present | [V02.10](desktop.html?case=V02.10&still=1&test=0) — session |

### V03. Notice overlay and missing-picture warning

| State | Where / how to reach it |
|---|---|
| V03.1 — none | [V03.1](desktop.html?case=V03.1&still=1&test=0) — session |
| V03.2 — startup keyboard help | [V03.2](desktop.html?case=V03.2&still=1&test=0) — session |
| V03.3 — capture unavailable | [V03.3](desktop.html?case=V03.3&still=1&test=0) — session |
| V03.4 — capture enabled | [V03.4](desktop.html?case=V03.4&still=1&test=0) — session |
| V03.5 — capture released | [V03.5](desktop.html?case=V03.5&still=1&test=0) — session |
| V03.6 — stream changing | [V03.6](desktop.html?case=V03.6&still=1&test=0) — session |
| V03.7 — stream changed | [V03.7](desktop.html?case=V03.7&still=1&test=0) — session |
| V03.8 — stream change failed | [V03.8](desktop.html?case=V03.8&still=1&test=0) — session |
| V03.9 — clipboard enabled | [V03.9](desktop.html?case=V03.9&still=1&test=0) — session |
| V03.10 — clipboard paused | [V03.10](desktop.html?case=V03.10&still=1&test=0) — session |
| V03.11 — clipboard empty | [V03.11](desktop.html?case=V03.11&still=1&test=0) — session |
| V03.12 — clipboard typing | [V03.12](desktop.html?case=V03.12&still=1&test=0) — session |
| V03.13 — shortcut sent | [V03.13](desktop.html?case=V03.13&still=1&test=0) — session |
| V03.14 — sound on | [V03.14](desktop.html?case=V03.14&still=1&test=0) — session |
| V03.15 — sound off | [V03.15](desktop.html?case=V03.15&still=1&test=0) — session |
| V03.16 — menu button hidden | [V03.16](desktop.html?case=V03.16&still=1&test=0) — session |
| V03.17 — host not drawing | [V03.17](desktop.html?case=V03.17&still=1&test=0) — session |
| V03.18 — video not arriving | [V03.18](desktop.html?case=V03.18&still=1&test=0) — session |

### V04. ••• menu button

| State | Where / how to reach it |
|---|---|
| V04.1 — visible idle | [V04.1](desktop.html?case=V04.1&still=1&test=0) — session |
| V04.2 — hovered | [V04.2](desktop.html?case=V04.2&still=1&test=0) — session |
| V04.3 — menu open | [V04.3](desktop.html?case=V04.3&still=1&test=0) — menu |
| V04.4 — hidden | [V04.4](desktop.html?case=V04.4&still=1&test=0) — session |

### V05. Root session menu

| State | Where / how to reach it |
|---|---|
| V05.1 — closed | [V05.1](desktop.html?case=V05.1&still=1&test=0) — session |
| V05.2 — open connecting | [V05.2](desktop.html?case=V05.2&still=1&test=0) — menu |
| V05.3 — open streaming | [V05.3](desktop.html?case=V05.3&still=1&test=0) — menu |
| V05.4 — fullscreen checked | [V05.4](desktop.html?case=V05.4&still=1&test=0) — menu |
| V05.5 — stats checked | [V05.5](desktop.html?case=V05.5&still=1&test=0) — menu |
| V05.6 — sound checked | [V05.6](desktop.html?case=V05.6&still=1&test=0) — menu |

### V06. Keyboard submenu

| State | Where / how to reach it |
|---|---|
| V06.1 — capture available and on | [V06.1](desktop.html?case=V06.1&still=1&test=0) — menu → keyboard |
| V06.2 — capture available and off | [V06.2](desktop.html?case=V06.2&still=1&test=0) — menu → keyboard |
| V06.3 — capture unavailable | [V06.3](desktop.html?case=V06.3&still=1&test=0) — menu → keyboard |
| V06.4 — Mac host shortcuts | [V06.4](desktop.html?case=V06.4&still=1&test=0) — menu → keyboard |
| V06.5 — non-Mac host shortcuts | [V06.5](desktop.html?case=V06.5&still=1&test=0) — menu → keyboard |

### V07. Clipboard submenu

| State | Where / how to reach it |
|---|---|
| V07.1 — sharing on | [V07.1](desktop.html?case=V07.1&still=1&test=0) — menu → clipboard |
| V07.2 — sharing paused | [V07.2](desktop.html?case=V07.2&still=1&test=0) — menu → clipboard |
| V07.3 — type requested with no text | [V07.3](desktop.html?case=V07.3&still=1&test=0) — session |
| V07.4 — type requested with supported text | [V07.4](desktop.html?case=V07.4&still=1&test=0) — session |
| V07.5 — type requested with skipped characters | [V07.5](desktop.html?case=V07.5&still=1&test=0) — session |

### V08. Video submenu

| State | Where / how to reach it |
|---|---|
| V08.1 — unavailable before codec | [V08.1](desktop.html?case=V08.1&still=1&test=0) — menu → video |
| V08.2 — available | [V08.2](desktop.html?case=V08.2&still=1&test=0) — menu → video |
| V08.3 — change pending | [V08.3](desktop.html?case=V08.3&still=1&test=0) — session |
| V08.4 — change succeeded | [V08.4](desktop.html?case=V08.4&still=1&test=0) — session |
| V08.5 — change rejected | [V08.5](desktop.html?case=V08.5&still=1&test=0) — session |

### V09. Remote pointer and capture feedback

| State | Where / how to reach it |
|---|---|
| V09.1 — remote bitmap visible | [V09.1](desktop.html?case=V09.1&still=1&test=0) — session; **not in the prototype as native behaviour:** Arbitrary remote RGBA cursor transport is not implemented; a drawn arrow represents the pointer. |
| V09.2 — pointer outside | [V09.2](desktop.html?case=V09.2&still=1&test=0) — session |
| V09.3 — menu pointer | [V09.3](desktop.html?case=V09.3&still=1&test=0) — menu |
| V09.4 — fallback cursor | [V09.4](desktop.html?case=V09.4&still=1&test=0) — session; **not in the prototype as native behaviour:** Native cursor fallback is represented by the same drawn arrow; no native cursor API. |
| V09.5 — scaled bitmap | [V09.5](desktop.html?case=V09.5&still=1&test=0) — session; **not in the prototype as native behaviour:** Native cursor bitmap scaling is not implemented; local picture zoom represents the visual state. |

### A01. Computers home and cards

| State | Where / how to reach it |
|---|---|
| A01.1 — initial loading | [A01.1](phone.html?case=A01.1&still=1&test=0) — computer |
| A01.2 — empty | [A01.2](phone.html?case=A01.2&still=1&test=0) — add |
| A01.3 — populated | [A01.3](phone.html?case=A01.3&still=1&test=0) — overview |
| A01.4 — checking | [A01.4](phone.html?case=A01.4&still=1&test=0) — computer |
| A01.5 — ready without latency | [A01.5](phone.html?case=A01.5&still=1&test=0) — computer |
| A01.6 — ready with latency | [A01.6](phone.html?case=A01.6&still=1&test=0) — computer |
| A01.7 — busy without viewer | [A01.7](phone.html?case=A01.7&still=1&test=0) — computer |
| A01.8 — busy with viewer | [A01.8](phone.html?case=A01.8&still=1&test=0) — computer |
| A01.9 — wrong-key | [A01.9](phone.html?case=A01.9&still=1&test=0) — computer |
| A01.10 — update-needed | [A01.10](phone.html?case=A01.10&still=1&test=0) — computer |
| A01.11 — not-found | [A01.11](phone.html?case=A01.11&still=1&test=0) — computer |
| A01.12 — invalid | [A01.12](phone.html?case=A01.12&still=1&test=0) — computer |
| A01.13 — offline never seen | [A01.13](phone.html?case=A01.13&still=1&test=0) — computer |
| A01.14 — offline previously seen | [A01.14](phone.html?case=A01.14&still=1&test=0) — computer |

### A02. Add / edit machine bottom sheet

| State | Where / how to reach it |
|---|---|
| A02.1 — blank add | [A02.1](phone.html?case=A02.1&still=1&test=0) — add; live check: idle |
| A02.2 — editing | [A02.2](phone.html?case=A02.2&still=1&test=0) — edit; live check: ready |
| A02.3 — deep link populated | [A02.3](phone.html?case=A02.3&still=1&test=0) — add; live check: ready |
| A02.4 — clipboard pasted | [A02.4](phone.html?case=A02.4&still=1&test=0) — add; live check: ready |
| A02.5 — idle preview | [A02.5](phone.html?case=A02.5&still=1&test=0) — add; live check: idle |
| A02.6 — checking | [A02.6](phone.html?case=A02.6&still=1&test=0) — add; live check: checking |
| A02.7 — ready | [A02.7](phone.html?case=A02.7&still=1&test=0) — add; live check: ready |
| A02.8 — busy | [A02.8](phone.html?case=A02.8&still=1&test=0) — add; live check: busy |
| A02.9 — wrong-key | [A02.9](phone.html?case=A02.9&still=1&test=0) — add; live check: key |
| A02.10 — update-needed | [A02.10](phone.html?case=A02.10&still=1&test=0) — add; live check: update |
| A02.11 — not-found | [A02.11](phone.html?case=A02.11&still=1&test=0) — add; live check: not-found |
| A02.12 — invalid | [A02.12](phone.html?case=A02.12&still=1&test=0) — add; live check: invalid |
| A02.13 — unreachable | [A02.13](phone.html?case=A02.13&still=1&test=0) — add; live check: unreachable |
| A02.14 — key revealed | [A02.14](phone.html?case=A02.14&still=1&test=0) — add; live check: ready |
| A02.15 — validation failure | [A02.15](phone.html?case=A02.15&still=1&test=0) — add; live check: invalid-key |
| A02.16 — storage failure | [A02.16](phone.html?case=A02.16&still=1&test=0) — add; live check: save-error |

### A03. Machine popup menu

| State | Where / how to reach it |
|---|---|
| A03.1 — ready | [A03.1](phone.html?case=A03.1&still=1&test=0) — computer |
| A03.2 — not ready | [A03.2](phone.html?case=A03.2&still=1&test=0) — computer |
| A03.3 — dismissed | [A03.3](phone.html?case=A03.3&still=1&test=0) — computer |

### A04. Remove confirmation

| State | Where / how to reach it |
|---|---|
| A04.1 — awaiting choice | [A04.1](phone.html?case=A04.1&still=1&test=0) — computer |
| A04.2 — cancelled | [A04.2](phone.html?case=A04.2&still=1&test=0) — computer |
| A04.3 — confirmed | [A04.3](phone.html?case=A04.3&still=1&test=0) — computer |
| A04.4 — storage failure | [A04.4](phone.html?case=A04.4&still=1&test=0) — computer |

### A05. Settings screen

| State | Where / how to reach it |
|---|---|
| A05.1 — open | [A05.1](phone.html?case=A05.1&still=1&test=0) — settings |
| A05.2 — phone name editing | [A05.2](phone.html?case=A05.2&still=1&test=0) — settings |
| A05.3 — preferences changed | [A05.3](phone.html?case=A05.3&still=1&test=0) — settings |
| A05.4 — closing | [A05.4](phone.html?case=A05.4&still=1&test=0) — computer |

### A06. Open Source Software sheet

| State | Where / how to reach it |
|---|---|
| A06.1 — loaded | [A06.1](phone.html?case=A06.1&still=1&test=0) — licenses |
| A06.2 — asset missing or unreadable | [A06.2](phone.html?case=A06.2&still=1&test=0) — licenses |
| A06.3 — dismissed | [A06.3](phone.html?case=A06.3&still=1&test=0) — settings |

### A07. Connecting screen / return transition

| State | Where / how to reach it |
|---|---|
| A07.1 — reaching | [A07.1](phone.html?case=A07.1&still=1&test=0) — connecting |
| A07.2 — starting video | [A07.2](phone.html?case=A07.2&still=1&test=0) — connecting |
| A07.3 — first frame revealing | [A07.3](phone.html?case=A07.3&still=1&test=0) — session |
| A07.4 — cancel requested | [A07.4](phone.html?case=A07.4&still=1&test=0) — computer |
| A07.5 — failure before stream | [A07.5](phone.html?case=A07.5&still=1&test=0) — computer |
| A07.6 — disconnected after stream | [A07.6](phone.html?case=A07.6&still=1&test=0) — computer |

### A08. Remote session picture / gestures

| State | Where / how to reach it |
|---|---|
| A08.1 — live foreground | [A08.1](phone.html?case=A08.1&still=1&test=0) — session |
| A08.2 — background retained | [A08.2](phone.html?case=A08.2&still=1&test=0) — session; **not in the prototype as native behaviour:** No native background service in a static browser prototype; notification example is available in Test. |
| A08.3 — trackpad mode | [A08.3](phone.html?case=A08.3&still=1&test=0) — session |
| A08.4 — direct touch mode | [A08.4](phone.html?case=A08.4&still=1&test=0) — session |
| A08.5 — zoomed | [A08.5](phone.html?case=A08.5&still=1&test=0) — session |
| A08.6 — keyboard visible | [A08.6](phone.html?case=A08.6&still=1&test=0) — session |
| A08.7 — menu visible | [A08.7](phone.html?case=A08.7&still=1&test=0) — menu |
| A08.8 — ending | [A08.8](phone.html?case=A08.8&still=1&test=0) — computer |

### A09. Movable session toolbar

| State | Where / how to reach it |
|---|---|
| A09.1 — keyboard button shown | [A09.1](phone.html?case=A09.1&still=1&test=0) — session |
| A09.2 — keyboard button hidden | [A09.2](phone.html?case=A09.2&still=1&test=0) — session |
| A09.3 — keyboard up | [A09.3](phone.html?case=A09.3&still=1&test=0) — session |
| A09.4 — menu open | [A09.4](phone.html?case=A09.4&still=1&test=0) — menu |
| A09.5 — idle dim | [A09.5](phone.html?case=A09.5&still=1&test=0) — session |
| A09.6 — dragging position | [A09.6](phone.html?case=A09.6&still=1&test=0) — session |

### A10. Session stats

| State | Where / how to reach it |
|---|---|
| A10.1 — off | [A10.1](phone.html?case=A10.1&still=1&test=0) — session |
| A10.2 — connecting | [A10.2](phone.html?case=A10.2&still=1&test=0) — session |
| A10.3 — basic stream | [A10.3](phone.html?case=A10.3&still=1&test=0) — session |
| A10.4 — optional host and transport metrics | [A10.4](phone.html?case=A10.4&still=1&test=0) — session |
| A10.5 — hardware decoder | [A10.5](phone.html?case=A10.5&still=1&test=0) — session |
| A10.6 — software decoder | [A10.6](phone.html?case=A10.6&still=1&test=0) — session |

### A11. Session menu sheet

| State | Where / how to reach it |
|---|---|
| A11.1 — open | [A11.1](phone.html?case=A11.1&still=1&test=0) — menu |
| A11.2 — touch selected | [A11.2](phone.html?case=A11.2&still=1&test=0) — menu |
| A11.3 — trackpad selected | [A11.3](phone.html?case=A11.3&still=1&test=0) — menu |
| A11.4 — sound on | [A11.4](phone.html?case=A11.4&still=1&test=0) — menu |
| A11.5 — sound off | [A11.5](phone.html?case=A11.5&still=1&test=0) — menu |
| A11.6 — stats on | [A11.6](phone.html?case=A11.6&still=1&test=0) — menu |
| A11.7 — stats off | [A11.7](phone.html?case=A11.7&still=1&test=0) — menu |
| A11.8 — clipboard on | [A11.8](phone.html?case=A11.8&still=1&test=0) — menu → clipboard |
| A11.9 — clipboard off | [A11.9](phone.html?case=A11.9&still=1&test=0) — menu → clipboard |
| A11.10 — video selection changed | [A11.10](phone.html?case=A11.10&still=1&test=0) — menu → video |
| A11.11 — stream change rejected | [A11.11](phone.html?case=A11.11&still=1&test=0) — menu → video |

### A12. IME and accessory key strip

| State | Where / how to reach it |
|---|---|
| A12.1 — hidden | [A12.1](phone.html?case=A12.1&still=1&test=0) — session |
| A12.2 — suggestions off | [A12.2](phone.html?case=A12.2&still=1&test=0) — session |
| A12.3 — suggestions on | [A12.3](phone.html?case=A12.3&still=1&test=0) — session |
| A12.4 — modifier off | [A12.4](phone.html?case=A12.4&still=1&test=0) — session |
| A12.5 — modifier next key | [A12.5](phone.html?case=A12.5&still=1&test=0) — session |
| A12.6 — modifier locked | [A12.6](phone.html?case=A12.6&still=1&test=0) — session |
| A12.7 — unsupported character warning | [A12.7](phone.html?case=A12.7&still=1&test=0) — session |

### A13. Pointer and gesture feedback

| State | Where / how to reach it |
|---|---|
| A13.1 — fallback arrow | [A13.1](phone.html?case=A13.1&still=1&test=0) — session |
| A13.2 — remote bitmap | [A13.2](phone.html?case=A13.2&still=1&test=0) — session; **not in the prototype as native behaviour:** Arbitrary host cursor bitmap decoding is not implemented; drawn arrow represents it. |
| A13.3 — tap pulse | [A13.3](phone.html?case=A13.3&still=1&test=0) — session |
| A13.4 — zoomed pointer | [A13.4](phone.html?case=A13.4&still=1&test=0) — session |
| A13.5 — shape unavailable | [A13.5](phone.html?case=A13.5&still=1&test=0) — session; **not in the prototype as native behaviour:** Invalid host bitmap handling is not implemented; fallback arrow represents it. |

### A14. Transient notice

| State | Where / how to reach it |
|---|---|
| A14.1 — normal | [A14.1](phone.html?case=A14.1&still=1&test=0) — computer |
| A14.2 — action Edit | [A14.2](phone.html?case=A14.2&still=1&test=0) — computer |
| A14.3 — first-session gesture help | [A14.3](phone.html?case=A14.3&still=1&test=0) — session |
| A14.4 — connection failure | [A14.4](phone.html?case=A14.4&still=1&test=0) — computer |
| A14.5 — clipboard result | [A14.5](phone.html?case=A14.5&still=1&test=0) — session |
| A14.6 — typing unsupported | [A14.6](phone.html?case=A14.6&still=1&test=0) — session |
| A14.7 — video change failure | [A14.7](phone.html?case=A14.7&still=1&test=0) — session |

### A15. Foreground service notification

| State | Where / how to reach it |
|---|---|
| A15.1 — starting connection | [A15.1](phone.html?case=A15.1&still=1) — connecting |
| A15.2 — connected foreground | [A15.2](phone.html?case=A15.2&still=1) — session |
| A15.3 — connected background | [A15.3](phone.html?case=A15.3&still=1) — session |
| A15.4 — notifications denied | [A15.4](phone.html?case=A15.4&still=1) — session |
| A15.5 — disconnected | [A15.5](phone.html?case=A15.5&still=1) — computer |

### A16. Android notification permission handoff

| State | Where / how to reach it |
|---|---|
| A16.1 — not yet requested | [A16.1](phone.html?case=A16.1&still=1&test=0) — settings |
| A16.2 — system prompt | [A16.2](phone.html?case=A16.2&still=1&test=0) — settings |
| A16.3 — granted | [A16.3](phone.html?case=A16.3&still=1&test=0) — settings |
| A16.4 — denied | [A16.4](phone.html?case=A16.4&still=1&test=0) — settings |

### A17. Shared sheet interaction / modal shell

| State | Where / how to reach it |
|---|---|
| A17.1 — opening | [A17.1](phone.html?case=A17.1&still=1&test=0) — add; live check: idle |
| A17.2 — open narrow | [A17.2](phone.html?case=A17.2&still=1&test=0) — add; live check: ready |
| A17.3 — open tablet | [A17.3](phone.html?case=A17.3&still=1&test=0) — add; live check: ready |
| A17.4 — IME raised | [A17.4](phone.html?case=A17.4&still=1&test=0) — session; **not in the prototype as native behaviour:** The real system IME is not raised. A drawn keyboard is used in sessions as requested; form inputs use the browser keyboard when tapped. |
| A17.5 — dragging | [A17.5](phone.html?case=A17.5&still=1&test=0) — menu; **not in the prototype as native behaviour:** Native sheet drag-to-dismiss was replaced by the guide’s in-place views and half-step-out menu; Back/Close is available. |
| A17.6 — drag cancelled | [A17.6](phone.html?case=A17.6&still=1&test=0) — menu; **not in the prototype as native behaviour:** Native cancelled sheet drag is not in the prototype because guide views replace that sheet shell. |
| A17.7 — dismissed | [A17.7](phone.html?case=A17.7&still=1&test=0) — computer |

## Deliberate prototype boundaries

- No transport, real screen capture/video/audio, Tailscale CLI, remote process, storage failure, native rendering, native cursor transport, global shortcut capture, clipboard sync/images, or OS permission/service is implemented. Their UI states and choices are simulated. Clipboard Copy/Paste/Type attempts the browser clipboard; fallback copy uses local selection. Browser security can deny clipboard reads over the supplied HTTP address or file URLs.
- V01.7, V09.1/4/5, A08.2 and A13.2/5 have representative pictures rather than native renderer/cursor/background implementations, as marked individually.
- A17.5/6 sheet drag-to-dismiss/cancel behaviour is intentionally absent: the guide replaces that shell with in-place views and the half-step-out session menu. The Test bottom sheet closes by Close/outside tap. A17.4 uses the drawn session keyboard instead of a real IME for session controls.
- The 2,628 Android license paragraphs are replaced by the explicitly permitted short placeholder list. System-owned keyboard/dialog wording and unbounded host/library messages are not exhaustively enumerated. Raw messages can be displayed through `fail`, `ended`, `notice` and `toast`.
- No reorder feature, Android Tailscale discovery, Android QR output, Android fast lane, or invented reconnecting state was added.

## Verification — second review

All checks use installed Chrome against the owner's existing 8790 server, which was not changed. `tools/cdp.mjs` is a copy of the supplied driver with 60 fps MP4 output and temporary files confined to `shots/raw/`. Only the requested `npx -y prettier@3` package command was used.

- **546 screenshot loads:** zero console errors, zero document overflows, zero native select elements. All contact sheets regenerated. [verification.json](shots/verification.json)
- **27 interaction assertions:** add/edit, key fix, remove, connect/cancel/failure, session switches, every desktop submenu and phone menu, Settings and Tailscale. [interactions.json](shots/interactions.json)
- **34 supplemental assertions:** file URLs, Test controls/reset, reduced motion, notifications, touch gestures, accessory targets and floating buttons. Drawn system keyboard keys are excluded from the app's 44px target rule. [supplemental.json](shots/supplemental.json)
- **90 review assertions:** anchored lists/context menus, uniform 16px action gaps and stable action positions, 12-computer scrolling, same-box form discovery, QWERTY geometry, safe-area geometry, and Escape/browser Back on both platforms. Includes nested Settings → QR and Add → Find return paths. [round2.json](shots/round2.json)
- **Nine submenu bounds assertions:** 820×560, 900×600 and 1280×800. [edges.json](shots/edges.json)
- Eight regenerated **60 fps MP4s**, with motion sheets; both menu recordings also have first-six-frame sheets. First 100ms controls are hidden; subsequent nonzero opacity begins with the screen already moving. Closing fades controls before zooming. Real-time samples: [desktop](shots/desktop-menu-timing.json), [phone](shots/phone-menu-timing.json).
- Prettier 3, two-space indentation, print width 110: all four reference files pass `--check`. All three JavaScript files pass `node --check`.
- Long names can be toggled in Test, or loaded with `longNames=1`; toggling restores MacBook Air when off. The other default names are archlinux, studio, ubuntu and gaming-pc.

**Verification limits:** Chrome's installed CDP version does not expose safe-area emulation, so notch checks inject 44px top/left inset fixtures through the same CSS variables that normally resolve to `env(safe-area-inset-*)`. Physical Android/iOS notch behavior, system keyboards, native app integrations, clipboard permissions and OS chrome were not tested. The 60 fps files resample timestamped Chrome screencast frames; they do not prove 60 fps rendering on physical hardware.

## Contact sheets

Raw screenshots and Chrome temporary profiles are deleted after sheet generation and ignored by `.gitignore`. [manifest.json](shots/manifest.json) retains each source URL, dimensions and original capture name so the sheets can be reproduced by `node tools/verify.mjs`. Keep the numbered contact sheets, motion sheets and MP4s.

Review landmarks: desktop-06 (Add), desktop-21 (context menu), desktop-45 (Video), desktop-49 (12 computers), desktop-59 (short/tall forms), desktop-60/61 (long names and bounded submenus); phone-07 (keyboard), phone-13 (12 computers), phone-19 (long names).

Desktop:

[desktop-01.png](shots/desktop-01.png) · [desktop-02.png](shots/desktop-02.png) · [desktop-03.png](shots/desktop-03.png) · [desktop-04.png](shots/desktop-04.png) · [desktop-05.png](shots/desktop-05.png) · [desktop-06.png](shots/desktop-06.png) · [desktop-07.png](shots/desktop-07.png) · [desktop-08.png](shots/desktop-08.png) · [desktop-09.png](shots/desktop-09.png) · [desktop-10.png](shots/desktop-10.png) · [desktop-11.png](shots/desktop-11.png) · [desktop-12.png](shots/desktop-12.png) · [desktop-13.png](shots/desktop-13.png) · [desktop-14.png](shots/desktop-14.png) · [desktop-15.png](shots/desktop-15.png) · [desktop-16.png](shots/desktop-16.png) · [desktop-17.png](shots/desktop-17.png) · [desktop-18.png](shots/desktop-18.png) · [desktop-19.png](shots/desktop-19.png) · [desktop-20.png](shots/desktop-20.png) · [desktop-21.png](shots/desktop-21.png) · [desktop-22.png](shots/desktop-22.png) · [desktop-23.png](shots/desktop-23.png) · [desktop-24.png](shots/desktop-24.png) · [desktop-25.png](shots/desktop-25.png) · [desktop-26.png](shots/desktop-26.png) · [desktop-27.png](shots/desktop-27.png) · [desktop-28.png](shots/desktop-28.png) · [desktop-29.png](shots/desktop-29.png) · [desktop-30.png](shots/desktop-30.png) · [desktop-31.png](shots/desktop-31.png) · [desktop-32.png](shots/desktop-32.png) · [desktop-33.png](shots/desktop-33.png) · [desktop-34.png](shots/desktop-34.png) · [desktop-35.png](shots/desktop-35.png) · [desktop-36.png](shots/desktop-36.png) · [desktop-37.png](shots/desktop-37.png) · [desktop-38.png](shots/desktop-38.png) · [desktop-39.png](shots/desktop-39.png) · [desktop-40.png](shots/desktop-40.png) · [desktop-41.png](shots/desktop-41.png) · [desktop-42.png](shots/desktop-42.png) · [desktop-43.png](shots/desktop-43.png) · [desktop-44.png](shots/desktop-44.png) · [desktop-45.png](shots/desktop-45.png) · [desktop-46.png](shots/desktop-46.png) · [desktop-47.png](shots/desktop-47.png) · [desktop-48.png](shots/desktop-48.png) · [desktop-49.png](shots/desktop-49.png) · [desktop-50.png](shots/desktop-50.png) · [desktop-51.png](shots/desktop-51.png) · [desktop-52.png](shots/desktop-52.png) · [desktop-53.png](shots/desktop-53.png) · [desktop-54.png](shots/desktop-54.png) · [desktop-55.png](shots/desktop-55.png) · [desktop-56.png](shots/desktop-56.png) · [desktop-57.png](shots/desktop-57.png) · [desktop-58.png](shots/desktop-58.png) · [desktop-59.png](shots/desktop-59.png) · [desktop-60.png](shots/desktop-60.png) · [desktop-61.png](shots/desktop-61.png)

Phone:

[phone-01.png](shots/phone-01.png) · [phone-02.png](shots/phone-02.png) · [phone-03.png](shots/phone-03.png) · [phone-04.png](shots/phone-04.png) · [phone-05.png](shots/phone-05.png) · [phone-06.png](shots/phone-06.png) · [phone-07.png](shots/phone-07.png) · [phone-08.png](shots/phone-08.png) · [phone-09.png](shots/phone-09.png) · [phone-10.png](shots/phone-10.png) · [phone-11.png](shots/phone-11.png) · [phone-12.png](shots/phone-12.png) · [phone-13.png](shots/phone-13.png) · [phone-14.png](shots/phone-14.png) · [phone-15.png](shots/phone-15.png) · [phone-16.png](shots/phone-16.png) · [phone-17.png](shots/phone-17.png) · [phone-18.png](shots/phone-18.png) · [phone-19.png](shots/phone-19.png)

Motion sheets:

[01-overview-to-computer-frames.png](shots/motions/01-overview-to-computer-frames.png) · [02-connect-frames.png](shots/motions/02-connect-frames.png) · [03-menu-open-close-frames.png](shots/motions/03-menu-open-close-frames.png) · [04-switch-computers-frames.png](shots/motions/04-switch-computers-frames.png) · [05-connect-failure-return-frames.png](shots/motions/05-connect-failure-return-frames.png) · [06-phone-swipe-frames.png](shots/motions/06-phone-swipe-frames.png) · [round2-desktop-menu-first-six.png](shots/motions/round2-desktop-menu-first-six.png) · [round2-desktop-menu-frames.png](shots/motions/round2-desktop-menu-frames.png) · [round2-phone-menu-first-six.png](shots/motions/round2-phone-menu-first-six.png) · [round2-phone-menu-frames.png](shots/motions/round2-phone-menu-frames.png)

Recordings:

[01-overview-to-computer.mp4](shots/motions/01-overview-to-computer.mp4) · [02-connect.mp4](shots/motions/02-connect.mp4) · [03-menu-open-close.mp4](shots/motions/03-menu-open-close.mp4) · [04-switch-computers.mp4](shots/motions/04-switch-computers.mp4) · [05-connect-failure-return.mp4](shots/motions/05-connect-failure-return.mp4) · [06-phone-swipe.mp4](shots/motions/06-phone-swipe.mp4) · [round2-desktop-menu.mp4](shots/motions/round2-desktop-menu.mp4) · [round2-phone-menu.mp4](shots/motions/round2-phone-menu.mp4)
