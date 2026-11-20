from pathlib import Path
import json
r=Path(__file__).resolve().parents[1]
cases=json.loads((r/'tools/cases.json').read_text())
s='''# Sunna prototype coverage

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
9. The drawn system keyboard uses normal QWERTY rows (10/9/7), Shift and Backspace, then 123/space/return. Its picture keys may be narrower than 44px; Sunna accessory targets remain 44px.
10. Linux mark geometry is copied from the repository SVG; its yellow details become white to satisfy the single-colour rule. Credit is retained in About. Android and Windows marks are simple monochrome outlines.
11. Permission actions stay inside the prototype: Allow/Reset update the handoff row and explain the simulated system destination. Test supplies grant/denial outcomes. No real OS dialog is requested by a static file.
12. Only the last selected index is persisted; all machine edits and preferences are tab-local. The title's long-name truncation and carousel neighbour peeks are intentional. One preview change per second at most: caret blinks every second; clocks update each minute. The guide's explicit 300 ms first-video reveal and ring spinner are the only exceptions to its two springs / 200 ms fade rule.

## Inventory surfaces and every numbered checklist state

A link opens a deterministic visual state. Ended/cancelled/dismissed states show their resulting destination. Native behaviour that a static browser cannot provide is explicitly marked below; its representative visual is still available where useful.

'''
last=''
for case in cases:
 code=case['id'].split('.')[0]
 if code!=last:
  s+=f"\n### {code}. {case['surface']}\n\n| State | Where / how to reach it |\n|---|---|\n";last=code
 surface='phone' if code.startswith('A') else 'desktop'
 query=f"case={case['id']}&still=1"+('' if code=='A15' else '&test=0')
 link=f"[{case['id']}]({surface}.html?{query})"
 config=case['config']; loc=config.get('view','computer')
 note=case['limitation']
 desc=f"{link} — {loc}"
 if config.get('submenu'):desc+=' → '+config['submenu']
 if config.get('form'):desc+='; live check: '+config['form']
 if note:desc+='; **not in the prototype as native behaviour:** '+note
 s+=f"| {case['id']} — {case['label'].replace('|','/')} | {desc} |\n"
s+='''
## Deliberate prototype boundaries

- No transport, real screen capture/video/audio, Tailscale CLI, remote process, storage failure, native rendering, native cursor transport, global shortcut capture, clipboard sync/images, or OS permission/service is implemented. Their UI states and choices are simulated. Clipboard Copy/Paste/Type attempts the browser clipboard; fallback copy uses local selection. Browser security can deny clipboard reads over the supplied HTTP address or file URLs.
- V01.7, V09.1/4/5, A08.2 and A13.2/5 have representative pictures rather than native renderer/cursor/background implementations, as marked individually.
- A17.5/6 sheet drag-to-dismiss/cancel behaviour is intentionally absent: the guide replaces that shell with in-place views and the half-step-out session menu. The Test bottom sheet closes by Close/outside tap. A17.4 uses the drawn session keyboard instead of a real IME for session controls.
- The 2,628 Android license paragraphs are replaced by the explicitly permitted short placeholder list. System-owned keyboard/dialog wording and unbounded host/library messages are not exhaustively enumerated. Raw messages can be displayed through `fail`, `ended`, `notice` and `toast`.
- No reorder feature, Android Tailscale discovery, Android QR output, Android fast lane, or invented reconnecting state was added.

## Verification

Verification uses the supplied `dev/cdp.mjs`, installed Chrome, Node 24 and ffmpeg. No packages were installed. Scripts are in `tools/`. Checks use the existing 8790 server without changing it.

'''
ver=json.loads((r/'shots/verification.json').read_text()) if (r/'shots/verification.json').exists() else {}
inter=json.loads((r/'shots/interactions.json').read_text()) if (r/'shots/interactions.json').exists() else {}
supp=json.loads((r/'shots/supplemental.json').read_text()) if (r/'shots/supplemental.json').exists() else {}
s+=f"- Screenshot sweep: **{ver.get('loads','pending')} page loads**, {len(ver.get('errors',[]))} console errors, {len(ver.get('issues',[]))} document overflow findings. Desktop checklist cases at both 1280×800 and 900×600; phone at 390×844, scale 2. Additional 820×560, 1920×1200, 844×390 landscape, wide phone frame, all counts, failures, ended reasons and scrolled Settings/menu captures.\n"
s+=f"- Interaction run: **{sum(bool(x.get('ok')) for x in inter.get('results',[]))} assertions passed**. Actual mouse clicks, keyboard input and CDP touch events drive add/link parsing, edit, wrong-key/fix, Keep/Remove, connect/cancel/failure/leave, all desktop root/submenu controls, both Settings pages, Tailscale choice, phone swipe, accessory modifiers and phone session controls. Logs: [interactions.json](shots/interactions.json).\n"
s+=f"- Supplemental run: **{sum(bool(x.get('ok')) for x in supp.get('checks',[]))} assertions passed** covering file URLs, defaults, URL overrides, disabled controls, Test/reset, reduced motion, notification/permission visibility, phone target sizes, pinch, three-finger keyboard, direct pointer feedback and movable buttons. Log: [supplemental.json](shots/supplemental.json).\n"
s+='''- Targeted refresh after visual review captures bounded desktop submenus and phone Settings. Nine submenu bounds checks pass at 820×560, 900×600 and 1280×800. Logs: [refresh.json](shots/refresh.json), [edges.json](shots/edges.json).
- Six MP4 recordings and timestamped sampled-frame sheets cover Overview → Computer, connect, menu open/close, session switching, failure return and phone swipe. Sampled frames were visually reviewed for jumps and screen clarity.
- All contact sheets were visually reviewed; fixes included short-window controls, menu label spacing, phone capsule/button collisions, duplicate screenshot filenames and balanced 12-computer rows. Raw screenshots and exact source URLs are indexed in [manifest.json](shots/manifest.json). Console/overflow findings: [verification.json](shots/verification.json).
- **Not verified on physical hardware:** Mac Safari/Chrome, Android Chrome/finger input, install-to-home-screen OS chrome, real safe-area hardware insets, OS keyboard behaviour, or browser clipboard permissions on the owner’s device. Chrome emulation is the verification environment, not proof of those platform behaviours.

## Contact sheets

Each sheet is labelled; desktop sheets are 1800 px wide with six tiles, phone sheets 1500 px wide with ten tiles (last sheets may have fewer). Raw screenshots are temporary; retain the contact sheets and motion sheets/MP4s.

'''
for surface in ['desktop','phone']:
 s+='\n'+surface.capitalize()+':\n\n'
 s+=' · '.join(f'[{p.name}](shots/{p.name})' for p in sorted((r/'shots').glob(surface+'-[0-9][0-9].png')))+'\n'
s+='\nMotion frame sheets:\n\n'+' · '.join(f'[{p.name}](shots/motions/{p.name})' for p in sorted((r/'shots/motions').glob('*-frames.png')))+'\n'
(r/'COVERAGE.md').write_text(s)
print('Coverage:',len(cases),'states')
