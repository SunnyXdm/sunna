# Sunna: user-facing design inventory

Research snapshot: 2026-11-17, current working files, not a release or a clean Git revision. Source inspection only; no app, build or test was run. Scope is the desktop launcher, native desktop viewer, and Android app. Paths below link to the inspected files and starting lines; a citation covers the associated paragraph/table row, including its continuation. No design references were needed.

## How to use this reference

A “screen” in the counts is one separately inventoried screen, sheet, popup, overlay, notification or permission surface below, including reusable components that need their own design. A “state” is one numbered item in an A-section checklist, **not** every Cartesian combination of flags, sizes, host metadata and controls. B catalogs count distinct exact strings/templates per surface; repeated wording within that surface counts once. Runtime names, addresses, OS versions, host errors and OS-owned permission/keyboard/menu text have unbounded values: their binding and source are recorded rather than invented. Rust `{}` formatting slots and Kotlin/JS interpolations are preserved. Some composed text is cataloged as its literal fragments plus its composition rule. Android license paragraphs are counted separately because they dwarf application copy.

No dedicated add-by-key screen exists: a key authenticates an address/name. QR scanning on Android belongs to the phone’s camera app; Sunna accepts its resulting link. Neither platform provides an interactive reorder control. See each surface’s flows and limitations. [main.js:514](/home/sunny/projects/sunna/apps/sunna/ui/main.js:514) [MachineSheet.kt:190](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MachineSheet.kt:190) [AndroidManifest.xml:30](/home/sunny/projects/sunna/apps/android/app/src/main/AndroidManifest.xml:30) [machines.rs:214](/home/sunny/projects/sunna/apps/sunna/src/machines.rs:214) [Machines.kt:241](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Machines.kt:241)

| Surface | Inventoried surfaces | Named checklist states | Distinct app strings/templates |
|---|---:|---:|---:|
| Desktop launcher | 10 | 76 | 204 |
| Native desktop viewer | 9 | 66 | 110 |
| Android | 17 | 116 | 249 |

Android additionally displays **2,628 license paragraphs**, reproduced in the appendix. Counts include upstream error fragments with their mapping caveat, but exclude arbitrary runtime values and OS-owned text.


# 1. Desktop launcher (Tauri webview)

## A. Surfaces, controls and state checklists

### L01. Computers home and machine cards

Purpose: choose a saved computer. Entry: app startup or return from a session. Toolbar **“Computers”**, add icon (**“Add a Computer (⌘N)”**, accessibility **“Add a computer”**) and settings icon (**“Settings (⌘,)”**, **“Settings”**). Grid accessibility label **“Your computers”**. Each tile shows name, state, OS/model or device/address, OS-themed miniature desktop and laptop/stand silhouette, optional boot distro name; its hit area is labeled `${machine.name}. ${status}` and tooltip joins OS, model, address with ` · `. **“Connect”** appears on ready hover; **“More”** opens L08. Empty home shows animated **“No Signal”**, explanation, **“Add a Computer…”** and **“Find on Tailscale”**. Populated grid also has **“Add a Computer” / “By address and key”** tile. [index.html:57](/home/sunny/projects/sunna/apps/sunna/ui/index.html:57) [index.html:75](/home/sunny/projects/sunna/apps/sunna/ui/index.html:75) [index.html:118](/home/sunny/projects/sunna/apps/sunna/ui/index.html:118) [index.html:160](/home/sunny/projects/sunna/apps/sunna/ui/index.html:160) [main.js:147](/home/sunny/projects/sunna/apps/sunna/ui/main.js:147)

Ready opens L09; wrong key opens L02 with key focused; busy/update/checking/unavailable show L10, with **“Edit”** action for unavailable. Status polling runs every 4 seconds, stops while hidden/connecting and avoids concurrent polls; failures only log. Grid density changes at 3/6 total tiles including add. Arrow navigation, Enter native button activation, Cmd/Ctrl+1…9 activate corresponding machine, Cmd/Ctrl+R refresh, Cmd/Ctrl+N add, Cmd/Ctrl+, settings, Cmd/Ctrl+E edit, Delete/Backspace remove; ContextMenu/Shift+F10 opens tile menu. [main.js:205](/home/sunny/projects/sunna/apps/sunna/ui/main.js:205) [main.js:258](/home/sunny/projects/sunna/apps/sunna/ui/main.js:258) [main.js:275](/home/sunny/projects/sunna/apps/sunna/ui/main.js:275) [main.js:1162](/home/sunny/projects/sunna/apps/sunna/ui/main.js:1162)

State triggers: idle has no address; checking awaits probe; ready is compatible version and key with no other active viewer (same viewer may take over); busy has another viewer; wrong-key and update-needed come from probe, version takes precedence over key; invalid is parser failure; not-found is resolution failure; unreachable is no probe response. Ready/busy update last_seen; last_seen is used only for unavailable copy. Busy/key/update cards remain lit with dim/blur/gray effects. Backend read/parse failure loading saved machines silently becomes empty. [main.js:96](/home/sunny/projects/sunna/apps/sunna/ui/main.js:96) [main.js:14](/home/sunny/projects/sunna/apps/sunna/ui/main.js:14) [reach.rs:112](/home/sunny/projects/sunna/crates/client/src/reach.rs:112) [machines.rs:72](/home/sunny/projects/sunna/apps/sunna/src/machines.rs:72) [machines.rs:136](/home/sunny/projects/sunna/apps/sunna/src/machines.rs:136)

**State checklist (15):** **1.** initial loading; **2.** empty first run; **3.** populated grid; **4.** idle address preview; **5.** checking; **6.** ready without latency; **7.** ready with latency; **8.** busy without viewer; **9.** busy with viewer; **10.** wrong-key; **11.** update-needed; **12.** not-found; **13.** invalid; **14.** unreachable never seen; **15.** unreachable previously seen. [main.js:96](/home/sunny/projects/sunna/apps/sunna/ui/main.js:96) [main.js:236](/home/sunny/projects/sunna/apps/sunna/ui/main.js:236)

### L02. Add / edit computer sheet

Entry: home add, Cmd/Ctrl+N, tile Edit…, wrong-key tile, or an error’s Edit action. Add title **“Add a Computer”**; edit title is machine name. Live miniature preview, **“Address”** with placeholder, password **“Key”**, reveal title **“Show” / “Hide”**, aria **“Show key”** (does not change), **“Name”** with optional/discovered placeholder, live check line. New form seeds key from Settings. **“Find on Tailscale”** exists only on add; **“Remove”** only on edit. **“Cancel”**, close icon **“Close”**, scrim and Escape dismiss; **“Add Computer” / “Save”** submit. Tab is trapped. [index.html:181](/home/sunny/projects/sunna/apps/sunna/ui/index.html:181) [main.js:438](/home/sunny/projects/sunna/apps/sunna/ui/main.js:438) [main.js:568](/home/sunny/projects/sunna/apps/sunna/ui/main.js:568)

Address/key edits debounce checks 420ms, links parse immediately; name changes preview. Empty address submit shakes/focuses field without a new textual error. Invalid syntax or line breaks fail backend validation; empty key is allowed. Unavailable/wrong-key/busy/update results do not prohibit saving a syntactically valid address. Pending submit spins; invoke rejection stays in form/check line. Add flies preview into grid; edit closes. No duplicate-address warning is implemented. [main.js:604](/home/sunny/projects/sunna/apps/sunna/ui/main.js:604) [main.js:650](/home/sunny/projects/sunna/apps/sunna/ui/main.js:650) [machines.rs:128](/home/sunny/projects/sunna/apps/sunna/src/machines.rs:128) [machines.rs:155](/home/sunny/projects/sunna/apps/sunna/src/machines.rs:155)

**State checklist (14):** **1.** new blank; **2.** editing saved; **3.** link pasted; **4.** checking; **5.** ready; **6.** busy; **7.** wrong-key; **8.** update-needed; **9.** unreachable; **10.** not-found; **11.** invalid; **12.** key revealed; **13.** submit pending; **14.** save failure. [main.js:568](/home/sunny/projects/sunna/apps/sunna/ui/main.js:568) [main.js:604](/home/sunny/projects/sunna/apps/sunna/ui/main.js:604) [main.js:650](/home/sunny/projects/sunna/apps/sunna/ui/main.js:650)

### L03. Find on Tailscale page

Entry: empty home’s Find on Tailscale, or L02’s find button. Replaces page within same sheet. **“Back”**/Escape returns to form; **“On Your Tailnet”** heading, explanation, **“Look again”** rescans. Result rows have name, OS/IP, state pill **“Sharing” / “In use” / “Other key” / “Update”**; selecting fills IP and name if blank, returns and focuses key/checks. Already saved IP becomes **“Added”**, disabled. Nonsharing/offline peers are disabled under **“Not sharing · ${others.length}”**; empty explanation is separate from errors. Scan failures are printed into list. Runs `tailscale status --json`, probes online IPv4 peers; no install/sign-in action button. [index.html:219](/home/sunny/projects/sunna/apps/sunna/ui/index.html:219) [main.js:699](/home/sunny/projects/sunna/apps/sunna/ui/main.js:699) [main.js:753](/home/sunny/projects/sunna/apps/sunna/ui/main.js:753) [discovery.rs:78](/home/sunny/projects/sunna/apps/sunna/src/discovery.rs:78)

**State checklist (8):** **1.** loading; **2.** sharing results; **3.** no sharing computers; **4.** scan error; **5.** rescan; **6.** already added; **7.** not sharing; **8.** offline. [main.js:699](/home/sunny/projects/sunna/apps/sunna/ui/main.js:699) [main.js:753](/home/sunny/projects/sunna/apps/sunna/ui/main.js:753) [discovery.rs:78](/home/sunny/projects/sunna/apps/sunna/src/discovery.rs:78)

### L04. Settings sheet

Entry: toolbar/Cmd+comma. Header **“Settings”**, **“Close”**. Self status, editable **“This computer's key”** with **“Show”**, **“Copy”** icons; **“Link to this computer”**, **“Copy”**, **“Show QR Code”** when address+key exist (even if not sharing). Key text autosaves after 500ms, trimmed; rejection is toast. Link displays masked key; copies real key. Footer version/protocol, shortcuts reference and Tux attribution. [index.html:230](/home/sunny/projects/sunna/apps/sunna/ui/index.html:230) [main.js:907](/home/sunny/projects/sunna/apps/sunna/ui/main.js:907) [main.js:976](/home/sunny/projects/sunna/apps/sunna/ui/main.js:976)

Every session setting/default: **“System shortcuts”** macOS only, **“Allowed”** or **“Allow…”**, **“Reset”** appears after asking while still denied; **“Open sessions”** = **“Full Screen”** default / **“Window”**; **“Stats bar”** on; **“Menu button”** on; **“Sound”** on. All switches save immediately. Key default is configured token or empty. Settings change future viewer environment. No video/clipboard default controls here. Access state polls 1500ms while sheet open, checks again 800ms after permission request, and treats probe errors as allowed. [index.html:270](/home/sunny/projects/sunna/apps/sunna/ui/index.html:270) [settings.rs:30](/home/sunny/projects/sunna/apps/sunna/src/settings.rs:30) [main.js:874](/home/sunny/projects/sunna/apps/sunna/ui/main.js:874) [main.js:907](/home/sunny/projects/sunna/apps/sunna/ui/main.js:907)

**State checklist (13):** **1.** loading self status; **2.** sharing ready; **3.** sharing busy; **4.** sharing different key; **5.** no Tailscale address; **6.** not sharing Linux; **7.** not sharing Mac; **8.** key masked; **9.** key revealed; **10.** permission denied; **11.** permission allowed; **12.** permission reset offered; **13.** settings save error. [main.js:874](/home/sunny/projects/sunna/apps/sunna/ui/main.js:874) [main.js:976](/home/sunny/projects/sunna/apps/sunna/ui/main.js:976) [settings.rs:30](/home/sunny/projects/sunna/apps/sunna/src/settings.rs:30)

### L05. Computer-link QR sheet

Entry: saved tile’s **“Show QR Code”**, or Settings **“Show QR Code”**. Title **“Add “${machine.name}” on a Phone”**, **“Close”**, image aria **“Computer link QR code”** (busy while generating), scanning helper and key privacy helper, **“Done”**. Backend encodes actual address/key link into SVG; failure replaces image with **“Couldn't make the QR code.”** No scanner or save-image button. [index.html:333](/home/sunny/projects/sunna/apps/sunna/ui/index.html:333) [main.js:1004](/home/sunny/projects/sunna/apps/sunna/ui/main.js:1004) [main.rs:170](/home/sunny/projects/sunna/apps/sunna/src/main.rs:170)

**State checklist (3):** **1.** generating; **2.** code ready; **3.** generation failed. [main.js:1004](/home/sunny/projects/sunna/apps/sunna/ui/main.js:1004)

### L06. Remove confirmation

Entry: edit Remove, menu Remove… or Delete/Backspace. Alertdialog heading **“Remove ${machine.name}?”**, explanation, default-focused **“Cancel”**, destructive **“Remove”**. Confirm invokes removal; backend failure toasts; success animates tile out/reflows and toasts **“Removed ${machine.name}.”** No undo. Scrim dismissal closes UI but does not resolve the confirmation promise; Escape uses cancel-sheet event. [index.html:350](/home/sunny/projects/sunna/apps/sunna/ui/index.html:350) [main.js:829](/home/sunny/projects/sunna/apps/sunna/ui/main.js:829)

**State checklist (4):** **1.** awaiting decision; **2.** cancelled; **3.** confirmed; **4.** delete failed. [main.js:829](/home/sunny/projects/sunna/apps/sunna/ui/main.js:829) [main.js:854](/home/sunny/projects/sunna/apps/sunna/ui/main.js:854)

### L07. macOS Accessibility handoff

Startup may show L10’s shortcut warning with **“Allow…”**. Settings Allow requests trust and opens System Settings Accessibility. Reset runs `tccutil reset Accessibility` for app then opens the list; failure has explicit remove-entry instructions. The system dialog/list wording is macOS-owned, absent from these sources. The native viewer needs reconnect after grant. Linux returns true and has no row. [main.js:1275](/home/sunny/projects/sunna/apps/sunna/ui/main.js:1275) [main.rs:194](/home/sunny/projects/sunna/apps/sunna/src/main.rs:194) [main.rs:204](/home/sunny/projects/sunna/apps/sunna/src/main.rs:204) [main.rs:214](/home/sunny/projects/sunna/apps/sunna/src/main.rs:214) [viewer.rs:87](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:87)

**State checklist (5):** **1.** permission missing on startup; **2.** system settings opened; **3.** permission granted; **4.** old entry reset; **5.** reset failed. [main.rs:194](/home/sunny/projects/sunna/apps/sunna/src/main.rs:194) [main.rs:204](/home/sunny/projects/sunna/apps/sunna/src/main.rs:204) [main.rs:214](/home/sunny/projects/sunna/apps/sunna/src/main.rs:214)

### L08. Machine context menu

Entry: right-click, More, keyboard context menu. Items exactly **“Connect”** (↵; disabled unless ready), **“Edit…”** (⌘E), **“Copy Address”**, **“Show QR Code”**, **“Remove…”** (⌫). Arrows cycle; Escape/outside/blur/resize close; selections dispatch normal flows. Shortcut glyphs remain Mac-style on Linux. [main.js:1050](/home/sunny/projects/sunna/apps/sunna/ui/main.js:1050) [main.js:1162](/home/sunny/projects/sunna/apps/sunna/ui/main.js:1162)

**State checklist (3):** **1.** open ready; **2.** open not ready; **3.** dismissed. [main.js:1050](/home/sunny/projects/sunna/apps/sunna/ui/main.js:1050)

### L09. Connecting transition

Entry: ready tile/menu/number shortcut. Enlarged tile screen, OS mark, machine name, progress track, changing phase, **“Cancel”** while reaching/video. Reaching starts before resolve/spawn; video comes from viewer’s session-established log; open comes from viewer-window log and launcher hides. End reverses into source tile; no source uses fallback. Error waits for opening plus 250ms; exit waits 500ms if opened, 150ms otherwise, then toast and poll after 300ms. Escape requests cancellation even where button is hidden. Cancel kills an existing child, but cannot cancel pending resolution while pid=0. [index.html:93](/home/sunny/projects/sunna/apps/sunna/ui/index.html:93) [main.js:301](/home/sunny/projects/sunna/apps/sunna/ui/main.js:301) [main.js:347](/home/sunny/projects/sunna/apps/sunna/ui/main.js:347) [main.js:396](/home/sunny/projects/sunna/apps/sunna/ui/main.js:396) [main.rs:241](/home/sunny/projects/sunna/apps/sunna/src/main.rs:241) [main.rs:319](/home/sunny/projects/sunna/apps/sunna/src/main.rs:319) [main.rs:397](/home/sunny/projects/sunna/apps/sunna/src/main.rs:397)

**State checklist (6):** **1.** reaching; **2.** starting video; **3.** connected; **4.** ended; **5.** cancel requested; **6.** startup failure. [main.js:301](/home/sunny/projects/sunna/apps/sunna/ui/main.js:301) [main.js:396](/home/sunny/projects/sunna/apps/sunna/ui/main.js:396)

### L10. Toast / copy feedback

Single bottom status region: text, success/check or alert icon, optional action **“Edit”**/**“Allow…”**. Default 4500ms; error 8000ms. Copy succeeds with **“Copied.”** and temporary check icon 1300ms; failure gives ⌘C instruction. Dynamic backend/I/O errors can be passed through verbatim. No error-details dialog. [index.html:111](/home/sunny/projects/sunna/apps/sunna/ui/index.html:111) [main.js:1095](/home/sunny/projects/sunna/apps/sunna/ui/main.js:1095) [main.js:1137](/home/sunny/projects/sunna/apps/sunna/ui/main.js:1137)

**State checklist (5):** **1.** informational; **2.** error; **3.** success; **4.** action available; **5.** dismissed or replaced. [main.js:1095](/home/sunny/projects/sunna/apps/sunna/ui/main.js:1095) [main.js:1137](/home/sunny/projects/sunna/apps/sunna/ui/main.js:1137)

### B. Home, cards and summary — exact copy

| String or template (verbatim) | Source |
|---|---|
| `Sunna` | [index.html:6](/home/sunny/projects/sunna/apps/sunna/ui/index.html:6) |
| `Computers` | [index.html:57](/home/sunny/projects/sunna/apps/sunna/ui/index.html:57) |
| `Add a Computer (⌘N)` | [index.html:61](/home/sunny/projects/sunna/apps/sunna/ui/index.html:61) |
| `Add a computer` | [index.html:61](/home/sunny/projects/sunna/apps/sunna/ui/index.html:61) |
| `Settings (⌘,)` | [index.html:64](/home/sunny/projects/sunna/apps/sunna/ui/index.html:64) |
| `Settings` | [index.html:64](/home/sunny/projects/sunna/apps/sunna/ui/index.html:64) |
| `Your computers` | [index.html:70](/home/sunny/projects/sunna/apps/sunna/ui/index.html:70) |
| `No Signal` | [index.html:75](/home/sunny/projects/sunna/apps/sunna/ui/index.html:75) |
| `No computers yet` | [index.html:80](/home/sunny/projects/sunna/apps/sunna/ui/index.html:80) |
| `Add a computer that's sharing with Sunna. Setting up sharing prints a link (on Linux, sunna-host link shows it again); paste it here.` | [index.html:81](/home/sunny/projects/sunna/apps/sunna/ui/index.html:81) |
| `Add a Computer…` | [index.html:83](/home/sunny/projects/sunna/apps/sunna/ui/index.html:83) |
| `Find on Tailscale` | [index.html:85](/home/sunny/projects/sunna/apps/sunna/ui/index.html:85) |
| `Connect` | [index.html:146](/home/sunny/projects/sunna/apps/sunna/ui/index.html:146) |
| `More` | [index.html:154](/home/sunny/projects/sunna/apps/sunna/ui/index.html:154) |
| `Add a Computer` | [index.html:173](/home/sunny/projects/sunna/apps/sunna/ui/index.html:173) |
| `By address and key` | [index.html:174](/home/sunny/projects/sunna/apps/sunna/ui/index.html:174) |
| `Laptop` | [main.js:55](/home/sunny/projects/sunna/apps/sunna/ui/main.js:55) |
| `Desktop` | [main.js:57](/home/sunny/projects/sunna/apps/sunna/ui/main.js:57) |
| `VM` | [main.js:55](/home/sunny/projects/sunna/apps/sunna/ui/main.js:55) |
| `Server` | [main.js:55](/home/sunny/projects/sunna/apps/sunna/ui/main.js:55) |
| `<1 ms` | [main.js:84](/home/sunny/projects/sunna/apps/sunna/ui/main.js:84) |
| `${Math.round(value)} ms` | [main.js:84](/home/sunny/projects/sunna/apps/sunna/ui/main.js:84) |
| `just now` | [main.js:89](/home/sunny/projects/sunna/apps/sunna/ui/main.js:89) |
| `${Math.round(diff / 60)} min ago` | [main.js:90](/home/sunny/projects/sunna/apps/sunna/ui/main.js:90) |
| `${Math.round(diff / 3600)} h ago` | [main.js:91](/home/sunny/projects/sunna/apps/sunna/ui/main.js:91) |
| `yesterday` | [main.js:93](/home/sunny/projects/sunna/apps/sunna/ui/main.js:93) |
| `${days} days ago` | [main.js:93](/home/sunny/projects/sunna/apps/sunna/ui/main.js:93) |
| `Enter its address` | [main.js:99](/home/sunny/projects/sunna/apps/sunna/ui/main.js:99) |
| `Looking…` | [main.js:101](/home/sunny/projects/sunna/apps/sunna/ui/main.js:101) |
| `Ready · ${ms(check.rtt_ms)}` | [main.js:103](/home/sunny/projects/sunna/apps/sunna/ui/main.js:103) |
| `Ready` | [main.js:103](/home/sunny/projects/sunna/apps/sunna/ui/main.js:103) |
| `In use · ${check.viewer}` | [main.js:105](/home/sunny/projects/sunna/apps/sunna/ui/main.js:105) |
| `In use` | [main.js:105](/home/sunny/projects/sunna/apps/sunna/ui/main.js:105) |
| `Key doesn't match` | [main.js:107](/home/sunny/projects/sunna/apps/sunna/ui/main.js:107) |
| `Needs an update` | [main.js:109](/home/sunny/projects/sunna/apps/sunna/ui/main.js:109) |
| `Address not found` | [main.js:111](/home/sunny/projects/sunna/apps/sunna/ui/main.js:111) |
| `Address isn't valid` | [main.js:113](/home/sunny/projects/sunna/apps/sunna/ui/main.js:113) |
| `Offline · seen ${ago(machine.last_seen)}` | [main.js:115](/home/sunny/projects/sunna/apps/sunna/ui/main.js:115) |
| `Not responding` | [main.js:115](/home/sunny/projects/sunna/apps/sunna/ui/main.js:115) |
| `${machine.name}. ${status}` | [main.js:168](/home/sunny/projects/sunna/apps/sunna/ui/main.js:168) |
| `None added yet` | [main.js:239](/home/sunny/projects/sunna/apps/sunna/ui/main.js:239) |
| `Looking for your computers…` | [main.js:240](/home/sunny/projects/sunna/apps/sunna/ui/main.js:240) |
| `${machine.name} is ready` | [main.js:244](/home/sunny/projects/sunna/apps/sunna/ui/main.js:244) |
| `${machine.name} isn't available` | [main.js:244](/home/sunny/projects/sunna/apps/sunna/ui/main.js:244) |
| `All ${machines.length} ready` | [main.js:246](/home/sunny/projects/sunna/apps/sunna/ui/main.js:246) |
| `None of ${machines.length} available` | [main.js:247](/home/sunny/projects/sunna/apps/sunna/ui/main.js:247) |
| `${ready.length} of ${machines.length} ready` | [main.js:248](/home/sunny/projects/sunna/apps/sunna/ui/main.js:248) |
| `${check.viewer \|\| "Someone"} is connected to ${machine.name} right now.` | [main.js:285](/home/sunny/projects/sunna/apps/sunna/ui/main.js:285) |
| `${machine.name} runs a different version of Sunna.` | [main.js:287](/home/sunny/projects/sunna/apps/sunna/ui/main.js:287) |
| `Still looking for ${machine.name}…` | [main.js:290](/home/sunny/projects/sunna/apps/sunna/ui/main.js:290) |
| `${machine.name} isn't responding.` | [main.js:292](/home/sunny/projects/sunna/apps/sunna/ui/main.js:292) |
| `Edit` | [main.js:293](/home/sunny/projects/sunna/apps/sunna/ui/main.js:293) |

### B. Add/edit and Tailscale — exact copy

| String or template (verbatim) | Source |
|---|---|
| `Close` | [index.html:187](/home/sunny/projects/sunna/apps/sunna/ui/index.html:187) |
| `Address` | [index.html:192](/home/sunny/projects/sunna/apps/sunna/ui/index.html:192) |
| `100.101.102.103, a name, or a sunna:// link` | [index.html:193](/home/sunny/projects/sunna/apps/sunna/ui/index.html:193) |
| `Key` | [index.html:196](/home/sunny/projects/sunna/apps/sunna/ui/index.html:196) |
| `Show` | [index.html:199](/home/sunny/projects/sunna/apps/sunna/ui/index.html:199) |
| `Show key` | [index.html:199](/home/sunny/projects/sunna/apps/sunna/ui/index.html:199) |
| `Name` | [index.html:203](/home/sunny/projects/sunna/apps/sunna/ui/index.html:203) |
| `Optional` | [index.html:204](/home/sunny/projects/sunna/apps/sunna/ui/index.html:204) |
| `Remove` | [index.html:210](/home/sunny/projects/sunna/apps/sunna/ui/index.html:210) |
| `Cancel` | [index.html:212](/home/sunny/projects/sunna/apps/sunna/ui/index.html:212) |
| `Add Computer` | [index.html:213](/home/sunny/projects/sunna/apps/sunna/ui/index.html:213) |
| `Back` | [index.html:219](/home/sunny/projects/sunna/apps/sunna/ui/index.html:219) |
| `On Your Tailnet` | [index.html:220](/home/sunny/projects/sunna/apps/sunna/ui/index.html:220) |
| `Look again` | [index.html:221](/home/sunny/projects/sunna/apps/sunna/ui/index.html:221) |
| `Computers on your Tailscale network that are sharing with Sunna. Choose one to fill in its address.` | [index.html:223](/home/sunny/projects/sunna/apps/sunna/ui/index.html:223) |
| `Looking for Sunna at ${hostOf(address)}…` | [main.js:553](/home/sunny/projects/sunna/apps/sunna/ui/main.js:553) |
| `Found ` | [main.js:555](/home/sunny/projects/sunna/apps/sunna/ui/main.js:555) |
| `Found ${check.name \|\| hostOf(address)}. ${check.viewer \|\| "Someone"} is connected to it right now.` | [main.js:559](/home/sunny/projects/sunna/apps/sunna/ui/main.js:559) |
| `${check.detail} You can add it anyway.` | [main.js:561](/home/sunny/projects/sunna/apps/sunna/ui/main.js:561) |
| `Save` | [main.js:576](/home/sunny/projects/sunna/apps/sunna/ui/main.js:576) |
| `New computer` | [main.js:592](/home/sunny/projects/sunna/apps/sunna/ui/main.js:592) |
| `When a computer starts sharing, it shows its address and key.` | [main.js:609](/home/sunny/projects/sunna/apps/sunna/ui/main.js:609) |
| `Hide` | [main.js:650](/home/sunny/projects/sunna/apps/sunna/ui/main.js:650) |
| `Looking on your tailnet…` | [main.js:701](/home/sunny/projects/sunna/apps/sunna/ui/main.js:701) |
| `Sharing` | [main.js:754](/home/sunny/projects/sunna/apps/sunna/ui/main.js:754) |
| `In use` | [main.js:755](/home/sunny/projects/sunna/apps/sunna/ui/main.js:755) |
| `Other key` | [main.js:756](/home/sunny/projects/sunna/apps/sunna/ui/main.js:756) |
| `Update` | [main.js:757](/home/sunny/projects/sunna/apps/sunna/ui/main.js:757) |
| `Added` | [main.js:778](/home/sunny/projects/sunna/apps/sunna/ui/main.js:778) |
| `Offline` | [main.js:786](/home/sunny/projects/sunna/apps/sunna/ui/main.js:786) |
| `Not sharing` | [main.js:786](/home/sunny/projects/sunna/apps/sunna/ui/main.js:786) |
| `No computers on your tailnet are sharing with Sunna yet.` | [main.js:793](/home/sunny/projects/sunna/apps/sunna/ui/main.js:793) |
| `Not sharing · ${others.length}` | [main.js:797](/home/sunny/projects/sunna/apps/sunna/ui/main.js:797) |
| `Tailscale isn't installed on this computer.` | [discovery.rs:82](/home/sunny/projects/sunna/apps/sunna/src/discovery.rs:82) |
| `Tailscale isn't connected. Open Tailscale and sign in.` | [discovery.rs:84](/home/sunny/projects/sunna/apps/sunna/src/discovery.rs:84) |
| `Unexpected Tailscale output: {error}` | [discovery.rs:87](/home/sunny/projects/sunna/apps/sunna/src/discovery.rs:87) |
| `The key can't contain line breaks.` | [machines.rs:131](/home/sunny/projects/sunna/apps/sunna/src/machines.rs:131) |
| `That computer is no longer in your list.` | [main.rs:253](/home/sunny/projects/sunna/apps/sunna/src/main.rs:253) |

### B. Address/probe validation (form, discovery, error toast) — exact copy

| String or template (verbatim) | Source |
|---|---|
| `Enter the computer's address.` | [reach.rs:55](/home/sunny/projects/sunna/crates/client/src/reach.rs:55) |
| `That IPv6 address is missing its closing ']'.` | [reach.rs:60](/home/sunny/projects/sunna/crates/client/src/reach.rs:60) |
| `Unexpected text after the address.` | [reach.rs:64](/home/sunny/projects/sunna/crates/client/src/reach.rs:64) |
| `That doesn't look like an address.` | [reach.rs:75](/home/sunny/projects/sunna/crates/client/src/reach.rs:75) |
| `The port must be a number from 1 to 65535.` | [reach.rs:83](/home/sunny/projects/sunna/crates/client/src/reach.rs:83) |
| `Couldn't find a computer called “{host}”.` | [reach.rs:98](/home/sunny/projects/sunna/crates/client/src/reach.rs:98) |
| `Couldn't find “{host}”.` | [reach.rs:108](/home/sunny/projects/sunna/crates/client/src/reach.rs:108) |
| `Nothing answered at {resolved}. Is Sunna sharing on that computer?` | [reach.rs:122](/home/sunny/projects/sunna/crates/client/src/reach.rs:122) |
| `That computer runs a newer Sunna. Update this one.` | [reach.rs:145](/home/sunny/projects/sunna/crates/client/src/reach.rs:145) |
| `That computer runs an older Sunna. Update it.` | [reach.rs:147](/home/sunny/projects/sunna/crates/client/src/reach.rs:147) |
| `Sunna is there, but the key doesn't match.` | [reach.rs:148](/home/sunny/projects/sunna/crates/client/src/reach.rs:148) |
| `{viewer} is connected to it right now.` | [reach.rs:149](/home/sunny/projects/sunna/crates/client/src/reach.rs:149) |
| `Someone is connected to it right now.` | [reach.rs:150](/home/sunny/projects/sunna/crates/client/src/reach.rs:150) |

### B. Settings and Accessibility — exact copy

| String or template (verbatim) | Source |
|---|---|
| `Checking…` | [index.html:241](/home/sunny/projects/sunna/apps/sunna/ui/index.html:241) |
| `This computer's key` | [index.html:247](/home/sunny/projects/sunna/apps/sunna/ui/index.html:247) |
| `Other computers need it to connect to this one. New computers you add start with it.` | [index.html:248](/home/sunny/projects/sunna/apps/sunna/ui/index.html:248) |
| `Copy` | [index.html:255](/home/sunny/projects/sunna/apps/sunna/ui/index.html:255) |
| `Copy key` | [index.html:255](/home/sunny/projects/sunna/apps/sunna/ui/index.html:255) |
| `Link to this computer` | [index.html:260](/home/sunny/projects/sunna/apps/sunna/ui/index.html:260) |
| `Show QR Code` | [index.html:265](/home/sunny/projects/sunna/apps/sunna/ui/index.html:265) |
| `Sessions` | [index.html:270](/home/sunny/projects/sunna/apps/sunna/ui/index.html:270) |
| `System shortcuts` | [index.html:274](/home/sunny/projects/sunna/apps/sunna/ui/index.html:274) |
| `⌘Tab, ⌘Space and the like go to the other computer. macOS asks you to allow this once, under Accessibility.` | [index.html:275](/home/sunny/projects/sunna/apps/sunna/ui/index.html:275) |
| `Allowed` | [index.html:277](/home/sunny/projects/sunna/apps/sunna/ui/index.html:277) |
| `Allow…` | [index.html:278](/home/sunny/projects/sunna/apps/sunna/ui/index.html:278) |
| `Sunna is already on in that list, but this still says no? That entry is from an earlier build: reset it, then allow Sunna again.` | [index.html:282](/home/sunny/projects/sunna/apps/sunna/ui/index.html:282) |
| `Reset` | [index.html:284](/home/sunny/projects/sunna/apps/sunna/ui/index.html:284) |
| `Open sessions` | [index.html:288](/home/sunny/projects/sunna/apps/sunna/ui/index.html:288) |
| `⌃⌥F switches during a session.` | [index.html:289](/home/sunny/projects/sunna/apps/sunna/ui/index.html:289) |
| `Full Screen` | [index.html:292](/home/sunny/projects/sunna/apps/sunna/ui/index.html:292) |
| `Window` | [index.html:293](/home/sunny/projects/sunna/apps/sunna/ui/index.html:293) |
| `Stats bar` | [index.html:298](/home/sunny/projects/sunna/apps/sunna/ui/index.html:298) |
| `Latency, bitrate and codec along the top. ⌃⌥S toggles it.` | [index.html:299](/home/sunny/projects/sunna/apps/sunna/ui/index.html:299) |
| `Menu button` | [index.html:305](/home/sunny/projects/sunna/apps/sunna/ui/index.html:305) |
| `The ••• in the corner. ⌃⌥M opens the menu either way.` | [index.html:306](/home/sunny/projects/sunna/apps/sunna/ui/index.html:306) |
| `Sound` | [index.html:312](/home/sunny/projects/sunna/apps/sunna/ui/index.html:312) |
| `Play the other computer's sound. Also in the session menu.` | [index.html:313](/home/sunny/projects/sunna/apps/sunna/ui/index.html:313) |
| `During a session` | [index.html:319](/home/sunny/projects/sunna/apps/sunna/ui/index.html:319) |
| `Session menu` | [index.html:321](/home/sunny/projects/sunna/apps/sunna/ui/index.html:321) |
| `⌃⌥M` | [index.html:306](/home/sunny/projects/sunna/apps/sunna/ui/index.html:306) |
| `Give ⌘Tab and other shortcuts back to this Mac` | [index.html:322](/home/sunny/projects/sunna/apps/sunna/ui/index.html:322) |
| `⌃⌥G` | [index.html:322](/home/sunny/projects/sunna/apps/sunna/ui/index.html:322) |
| `Full screen or window` | [index.html:323](/home/sunny/projects/sunna/apps/sunna/ui/index.html:323) |
| `⌃⌥F` | [index.html:289](/home/sunny/projects/sunna/apps/sunna/ui/index.html:289) |
| `⌃⌥S` | [index.html:299](/home/sunny/projects/sunna/apps/sunna/ui/index.html:299) |
| `Disconnect` | [index.html:325](/home/sunny/projects/sunna/apps/sunna/ui/index.html:325) |
| `⌃⌥Q` | [index.html:325](/home/sunny/projects/sunna/apps/sunna/ui/index.html:325) |
| `The Linux penguin is drawn after Tux by Larry Ewing (lewing@isc.tamu.edu), made with The GIMP.` | [index.html:328](/home/sunny/projects/sunna/apps/sunna/ui/index.html:328) |
| `Sunna ${info.version ?? ""} · protocol ${info.protocol ?? ""}` | [main.js:976](/home/sunny/projects/sunna/apps/sunna/ui/main.js:976) |
| `This computer` | [main.js:977](/home/sunny/projects/sunna/apps/sunna/ui/main.js:977) |
| `Sharing at ${me.address}` | [main.js:984](/home/sunny/projects/sunna/apps/sunna/ui/main.js:984) |
| `Sharing at ${me.address} · in a session` | [main.js:985](/home/sunny/projects/sunna/apps/sunna/ui/main.js:985) |
| `Sharing, but with a different key than the one below` | [main.js:986](/home/sunny/projects/sunna/apps/sunna/ui/main.js:986) |
| `Not on a Tailscale network` | [main.js:987](/home/sunny/projects/sunna/apps/sunna/ui/main.js:987) |
| `Not sharing. Run sunna-host setup to share it.` | [main.js:990](/home/sunny/projects/sunna/apps/sunna/ui/main.js:990) |
| `Not sharing. To share this Mac, run scripts/run.sh host.` | [main.js:991](/home/sunny/projects/sunna/apps/sunna/ui/main.js:991) |
| `sunna://${me.address}?key=••••` | [main.js:996](/home/sunny/projects/sunna/apps/sunna/ui/main.js:996) |
| `⌘Tab, ⌘Space and other shortcuts will stay on this Mac.` | [main.js:1275](/home/sunny/projects/sunna/apps/sunna/ui/main.js:1275) |
| `Couldn't reset it. In the list that opened, select Sunna, remove it with −, then allow Sunna again.` | [main.rs:223](/home/sunny/projects/sunna/apps/sunna/src/main.rs:223) |
| `The key can't contain line breaks or '='.` | [settings.rs:119](/home/sunny/projects/sunna/apps/sunna/src/settings.rs:119) |

### B. QR, confirmation, context menu and toast — exact copy

| String or template (verbatim) | Source |
|---|---|
| `Add “${machine.name}” on a Phone` | [main.js:1004](/home/sunny/projects/sunna/apps/sunna/ui/main.js:1004) |
| `Computer link QR code` | [index.html:340](/home/sunny/projects/sunna/apps/sunna/ui/index.html:340) |
| `Scan it with your phone's camera to add this computer to Sunna on the phone.` | [index.html:341](/home/sunny/projects/sunna/apps/sunna/ui/index.html:341) |
| `It contains the computer's key: show it only to your own devices.` | [index.html:342](/home/sunny/projects/sunna/apps/sunna/ui/index.html:342) |
| `Done` | [index.html:345](/home/sunny/projects/sunna/apps/sunna/ui/index.html:345) |
| `Couldn't make the QR code.` | [main.js:1018](/home/sunny/projects/sunna/apps/sunna/ui/main.js:1018) |
| `Remove ${machine.name}?` | [main.js:831](/home/sunny/projects/sunna/apps/sunna/ui/main.js:831) |
| `It leaves this list. You can add it again any time with its address and key.` | [main.js:833](/home/sunny/projects/sunna/apps/sunna/ui/main.js:833) |
| `Removed ${machine.name}.` | [main.js:867](/home/sunny/projects/sunna/apps/sunna/ui/main.js:867) |
| `↵` | [main.js:1050](/home/sunny/projects/sunna/apps/sunna/ui/main.js:1050) |
| `Edit…` | [main.js:1051](/home/sunny/projects/sunna/apps/sunna/ui/main.js:1051) |
| `⌘E` | [main.js:1051](/home/sunny/projects/sunna/apps/sunna/ui/main.js:1051) |
| `Copy Address` | [main.js:1052](/home/sunny/projects/sunna/apps/sunna/ui/main.js:1052) |
| `Remove…` | [main.js:1055](/home/sunny/projects/sunna/apps/sunna/ui/main.js:1055) |
| `⌫` | [main.js:1055](/home/sunny/projects/sunna/apps/sunna/ui/main.js:1055) |
| `Couldn't copy. Select the text and press ⌘C.` | [main.js:1137](/home/sunny/projects/sunna/apps/sunna/ui/main.js:1137) |
| `Copied.` | [main.js:1141](/home/sunny/projects/sunna/apps/sunna/ui/main.js:1141) |

### B. Connection and session-ended feedback — exact copy

| String or template (verbatim) | Source |
|---|---|
| `Reaching ${name}…` | [main.js:302](/home/sunny/projects/sunna/apps/sunna/ui/main.js:302) |
| `Starting video…` | [main.js:303](/home/sunny/projects/sunna/apps/sunna/ui/main.js:303) |
| `Connected` | [main.js:304](/home/sunny/projects/sunna/apps/sunna/ui/main.js:304) |
| `Disconnected` | [main.js:305](/home/sunny/projects/sunna/apps/sunna/ui/main.js:305) |
| `A session is already open.` | [main.rs:245](/home/sunny/projects/sunna/apps/sunna/src/main.rs:245) |
| `Couldn't start the viewer: {error}` | [main.rs:296](/home/sunny/projects/sunna/apps/sunna/src/main.rs:296) |
| `The session ended unexpectedly.` | [main.rs:337](/home/sunny/projects/sunna/apps/sunna/src/main.rs:337) |
| `The viewer crashed. Its log has the details.` | [main.rs:409](/home/sunny/projects/sunna/apps/sunna/src/main.rs:409) |
| `Lost the connection: nothing came back from that computer for 10 seconds. Its network dropped, or it went to sleep.` | [main.rs:414](/home/sunny/projects/sunna/apps/sunna/src/main.rs:414) |
| `That computer ended the session: it restarted, or stopped sharing.` | [main.rs:416](/home/sunny/projects/sunna/apps/sunna/src/main.rs:416) |
| `This session moved to a newer one from this Mac.` | [main.rs:418](/home/sunny/projects/sunna/apps/sunna/src/main.rs:418) |
| `The key doesn't match. Edit the computer and paste its key.` | [main.rs:420](/home/sunny/projects/sunna/apps/sunna/src/main.rs:420) |
| `{who} is connected to that computer right now.` | [main.rs:422](/home/sunny/projects/sunna/apps/sunna/src/main.rs:422) |
| `Someone else is connected to that computer.` | [main.rs:424](/home/sunny/projects/sunna/apps/sunna/src/main.rs:424) |
| `This computer can't play that computer's video ({message}).` | [main.rs:426](/home/sunny/projects/sunna/apps/sunna/src/main.rs:426) |
| `That computer runs a different version of Sunna. Update both.` | [main.rs:428](/home/sunny/projects/sunna/apps/sunna/src/main.rs:428) |
| `Couldn't reach that computer ({message}).` | [main.rs:430](/home/sunny/projects/sunna/apps/sunna/src/main.rs:430) |

### B. Dynamic composition and non-finite error copy

- `Found ` is followed by `check.name || hostOf(address)`, then optional OS and latency, joined with ` · `. System subtitles join OS and model/device with the same separator, otherwise address. Names, distro boot words (uppercased and version stripped), model, viewer, IP and OS are host/user data; they are not fixed translation strings. Initial summary is a nonbreaking space. [main.js:59](/home/sunny/projects/sunna/apps/sunna/ui/main.js:59) [main.js:67](/home/sunny/projects/sunna/apps/sunna/ui/main.js:67) [main.js:236](/home/sunny/projects/sunna/apps/sunna/ui/main.js:236) [main.js:550](/home/sunny/projects/sunna/apps/sunna/ui/main.js:550)
- Save/list/settings/clipboard/discovery command rejection uses the error text or detail verbatim. Persistence errors come from filesystem/JSON; these are not a closed wording set. Unknown viewer failures return the raw last error line. Known client errors include `no answer from the host for 10 seconds`, `protocol version mismatch: host {version}, client {}`, `host refused the session: {reason}`, `expected HelloAck, got {other:?}` and decoder/transport errors; mappings above take precedence where matched. [machines.rs:79](/home/sunny/projects/sunna/apps/sunna/src/machines.rs:79) [main.js:650](/home/sunny/projects/sunna/apps/sunna/ui/main.js:650) [main.js:907](/home/sunny/projects/sunna/apps/sunna/ui/main.js:907) [main.rs:407](/home/sunny/projects/sunna/apps/sunna/src/main.rs:407) [lib.rs:509](/home/sunny/projects/sunna/crates/client/src/lib.rs:509)
- Actual copied link is `sunna://${address}?key=${key}` (values are not encoded here); masked display differs. QR encodes the same real link. Names in confirmation/QR titles are literal user data. [main.js:996](/home/sunny/projects/sunna/apps/sunna/ui/main.js:996) [main.js:1004](/home/sunny/projects/sunna/apps/sunna/ui/main.js:1004)


### B. Additional error fragments and dynamically composed copy

| String/template | Source |
|---|---|
| `tls setup: {0}` | [lib.rs:34](/home/sunny/projects/sunna/crates/transport/src/lib.rs:34) |
| `certificate generation: {0}` | [lib.rs:36](/home/sunny/projects/sunna/crates/transport/src/lib.rs:36) |
| `quic crypto config: {0}` | [lib.rs:38](/home/sunny/projects/sunna/crates/transport/src/lib.rs:38) |
| `io: {0}` | [lib.rs:40](/home/sunny/projects/sunna/crates/transport/src/lib.rs:40) |
| `connect: {0}` | [lib.rs:42](/home/sunny/projects/sunna/crates/transport/src/lib.rs:42) |
| `connection: {0}` | [lib.rs:44](/home/sunny/projects/sunna/crates/transport/src/lib.rs:44) |
| `stream write: {0}` | [lib.rs:46](/home/sunny/projects/sunna/crates/transport/src/lib.rs:46) |
| `stream read: {0}` | [lib.rs:48](/home/sunny/projects/sunna/crates/transport/src/lib.rs:48) |
| `codec: {0}` | [lib.rs:50](/home/sunny/projects/sunna/crates/transport/src/lib.rs:50) |
| `control message too large ({0} bytes)` | [lib.rs:52](/home/sunny/projects/sunna/crates/transport/src/lib.rs:52) |
| `stream closed: {0}` | [lib.rs:56](/home/sunny/projects/sunna/crates/transport/src/lib.rs:56) |
| `host pipeline failed: {error}` | [lib.rs:592](/home/sunny/projects/sunna/crates/host/src/lib.rs:592) |
| `no answer from the host for 10 seconds` | [lib.rs:523](/home/sunny/projects/sunna/crates/client/src/lib.rs:523) |
| `protocol version mismatch: host {version}, client {}` | [lib.rs:541](/home/sunny/projects/sunna/crates/client/src/lib.rs:541) |
| `host refused the session: {reason}` | [lib.rs:554](/home/sunny/projects/sunna/crates/client/src/lib.rs:554) |
| `expected HelloAck, got {other:?}` | [lib.rs:555](/home/sunny/projects/sunna/crates/client/src/lib.rs:555) |
| `decode thread exited during reconfigure` | [lib.rs:856](/home/sunny/projects/sunna/crates/client/src/lib.rs:856) |
| `decoder reconfigure failed: {error}` | [lib.rs:1054](/home/sunny/projects/sunna/crates/client/src/lib.rs:1054) |
| `no decoder for codec {other:?} on this platform` | [lib.rs:304](/home/sunny/projects/sunna/crates/codec/src/lib.rs:304) |
| `no {other} decoder here: this viewer was built without FFmpeg` | [lib.rs:319](/home/sunny/projects/sunna/crates/codec/src/lib.rs:319) |
| `The key it shows when it starts sharing` | [index.html:198](/home/sunny/projects/sunna/apps/sunna/ui/index.html:198) |

These are upstream fragments, not additional screens. The platform’s friendly mapping may replace or wrap them; unknown embedded library/OS/host error text remains unbounded. Frame-level decode failures may only log/request recovery rather than end the session; the constructor/reconfiguration failures above are the relevant presentation path. [lib.rs:569](/home/sunny/projects/sunna/crates/client/src/lib.rs:569) [lib.rs:1054](/home/sunny/projects/sunna/crates/client/src/lib.rs:1054) [main.rs:407](/home/sunny/projects/sunna/apps/sunna/src/main.rs:407) [lib.rs:208](/home/sunny/projects/sunna/crates/android/src/lib.rs:208)

## C. Complete launcher flows

1. **First launch:** request app info, settings and machine list; show loading summary; render empty or saved list; poll; possibly show macOS shortcut permission toast. Missing/malformed saved file looks like first run. Empty Add opens blank sheet seeded with default key; Find opens its second page. [main.js:1257](/home/sunny/projects/sunna/apps/sunna/ui/main.js:1257) [machines.rs:72](/home/sunny/projects/sunna/apps/sunna/src/machines.rs:72)
2. **Address/name + key:** enter IPv4, IPv6, host name, optional port (default 48800); name resolves with 2s timeout, probe waits 1500ms. Key alone cannot identify a computer. Optional custom name wins, then discovered name, then host portion. Save validates syntax/newlines, not reachability or key match. Every parser/probe outcome is listed in B. [reach.rs:12](/home/sunny/projects/sunna/crates/client/src/reach.rs:12) [reach.rs:48](/home/sunny/projects/sunna/crates/client/src/reach.rs:48) [reach.rs:90](/home/sunny/projects/sunna/crates/client/src/reach.rs:90) [machines.rs:155](/home/sunny/projects/sunna/apps/sunna/src/machines.rs:155)
3. **Link:** paste `sunna://…?key=…` in address; JS handles scheme case-insensitively, decodes host/query, populates key; save uses same path. There is no desktop external deep-link registration/receiver in the inspected launcher code. Malformed decode behavior is not a dedicated friendly error state. **QR:** generate from existing tile or self Settings; camera on phone opens Android link. No desktop scan/import-by-image control. [main.js:514](/home/sunny/projects/sunna/apps/sunna/ui/main.js:514) [main.js:604](/home/sunny/projects/sunna/apps/sunna/ui/main.js:604) [main.js:1004](/home/sunny/projects/sunna/apps/sunna/ui/main.js:1004) [main.rs:436](/home/sunny/projects/sunna/apps/sunna/src/main.rs:436)
4. **Tailscale:** open Find → loading → CLI unavailable/not connected/invalid JSON error, or sorted sharing/busy/wrong-key/update rows plus disabled nonsharing/offline rows → select → fill IP/name → key focus → probe → save. Look again reruns; Back/Escape restores form. [discovery.rs:78](/home/sunny/projects/sunna/apps/sunna/src/discovery.rs:78) [main.js:699](/home/sunny/projects/sunna/apps/sunna/ui/main.js:699)
5. **Connect:** only ready card invokes connect → reaching animation → resolve address → spawn viewer → handshake log changes to Starting video… → first-frame wait up to 10s, with fallback 1280×720 if none → viewer window log changes Connected, hides launcher. Only one session; missing machine/spawn/resolve/client failures show error after reverse transition. QUIC inactivity timeout is 10s; keepalive every 2s; handshake waits 10s. **Cancel** or Escape kills spawned viewer; the pid=0 resolution window is not cancellable by this command. [main.js:396](/home/sunny/projects/sunna/apps/sunna/ui/main.js:396) [main.rs:241](/home/sunny/projects/sunna/apps/sunna/src/main.rs:241) [main.rs:319](/home/sunny/projects/sunna/apps/sunna/src/main.rs:319) [main.rs:397](/home/sunny/projects/sunna/apps/sunna/src/main.rs:397) [session.rs:107](/home/sunny/projects/sunna/crates/viewer/src/session.rs:107) [session.rs:199](/home/sunny/projects/sunna/crates/viewer/src/session.rs:199) [lib.rs:520](/home/sunny/projects/sunna/crates/client/src/lib.rs:520)
6. **In-session:** native viewer takes over (section 2). Session close success/cancel has no reason toast; unexpected exit produces mapped reason or generic unexpected-session message. Missing log/error detail may become generic; panic becomes crashed-log message. Replaced, host stop, timeout, bad key, busy named/unnamed, incompatible protocol, decoder failure, connect failure and raw fallback are all distinct B branches. [main.rs:319](/home/sunny/projects/sunna/apps/sunna/src/main.rs:319) [main.rs:407](/home/sunny/projects/sunna/apps/sunna/src/main.rs:407)
7. **Edit/rename/remove:** menu/shortcut/wrong key → same fields → Save; address/key changes clear learned OS/device/model/size but retain last_seen. Remove → confirm → save store → remove animation/toast. No undo or reorder UI; backend reorder command alone does not provide a flow. [machines.rs:172](/home/sunny/projects/sunna/apps/sunna/src/machines.rs:172) [machines.rs:214](/home/sunny/projects/sunna/apps/sunna/src/machines.rs:214) [main.js:829](/home/sunny/projects/sunna/apps/sunna/ui/main.js:829)
8. **Settings and permission:** open → async self status + permission polling → edit key/toggles autosave → optional copy/QR; Allow opens macOS Accessibility; if stale grant still denied, Reset and reallow; reconnect native session to capture shortcuts. Linux hides access controls, but much keyboard copy is still Mac-specific. [main.js:874](/home/sunny/projects/sunna/apps/sunna/ui/main.js:874) [main.js:907](/home/sunny/projects/sunna/apps/sunna/ui/main.js:907) [main.rs:194](/home/sunny/projects/sunna/apps/sunna/src/main.rs:194) [viewer.rs:87](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:87)

## D. Existing launcher motion

| Motion | Exact behavior / timing | Source |
|---|---|---|
| Spring system | Euler integration at 1/240s, settle when position error <.0015 and speed <.02, max 3s; snappy stiffness/damping 430/38, bouncy 320/21, gentle 150/23, zoom 125/21, mass 1. Up to 64 CSS linear samples; unsupported linear uses cubic(.2,.9,.3,1) and .85 duration. | [motion.js:9](/home/sunny/projects/sunna/apps/sunna/ui/motion.js:9) |
| Reduced motion | CSS animations/transitions 1ms, delays 0, one iteration; JS helper 1ms unless an explicit options override; pointer tilt disabled. | [motion.js:53](/home/sunny/projects/sunna/apps/sunna/ui/motion.js:53) [motion.js:105](/home/sunny/projects/sunna/apps/sunna/ui/motion.js:105) [style.css:1464](/home/sunny/projects/sunna/apps/sunna/ui/style.css:1464) |
| Home / grid entrance | app opacity .35s, launch .45s; tile-in .7s out-expo from y12, delay index×50+50ms; settled class after1600ms; welcome fade .6s. | [style.css:100](/home/sunny/projects/sunna/apps/sunna/ui/style.css:100) [style.css:167](/home/sunny/projects/sunna/apps/sunna/ui/style.css:167) [style.css:822](/home/sunny/projects/sunna/apps/sunna/ui/style.css:822) [main.js:1257](/home/sunny/projects/sunna/apps/sunna/ui/main.js:1257) |
| Hover / pointer | float y−5 gentle, press y−1 scale.985 .12s; tilt tracks pointer with .15 lerp per frame, max6° horizontal /8.4° vertical; pointer-following glass opacity .3s. | [style.css:192](/home/sunny/projects/sunna/apps/sunna/ui/style.css:192) [motion.js:86](/home/sunny/projects/sunna/apps/sunna/ui/motion.js:86) [style.css:430](/home/sunny/projects/sunna/apps/sunna/ui/style.css:430) |
| Lit wallpaper | drift30s ease-in-out alternate, paused unless lit; x−3%→3%,y−2%→2.5%,scale1.02→1.1; opacity/filter .6s; glow opacity .9s, scale/position gentle; desk opacity .6s delay .1s. | [style.css:251](/home/sunny/projects/sunna/apps/sunna/ui/style.css:251) [style.css:283](/home/sunny/projects/sunna/apps/sunna/ui/style.css:283) [style.css:716](/home/sunny/projects/sunna/apps/sunna/ui/style.css:716) |
| Boot / wake / sleep | boot1.35s linear, opacity at0/8/72/100%; boot progress .85s cubic(.35,.6,.35,1) after.2s; wake.9s out-expo from scale1.06 blur6; desk .6s delay.2; bloom1.4s from scale.6/y10%; boot delays .95/1.15/.9; sleep.7s to scale.97 blur5 brightness.4. Initial wake stagger120+110i ms; wake classes cleared2300+delay, sleep900. | [style.css:327](/home/sunny/projects/sunna/apps/sunna/ui/style.css:327) [main.js:129](/home/sunny/projects/sunna/apps/sunna/ui/main.js:129) |
| Status | checking sweep1.9s ease-in-out; dot pulse1.2s, busy pulse2s to opacity.4; glyph fade.5s; status text/Connect hover opacity.18s and y5 transform.25s; name color.4s, status dot background.4s, label opacity.5s. | [style.css:368](/home/sunny/projects/sunna/apps/sunna/ui/style.css:368) [style.css:396](/home/sunny/projects/sunna/apps/sunna/ui/style.css:396) [style.css:412](/home/sunny/projects/sunna/apps/sunna/ui/style.css:412) [style.css:482](/home/sunny/projects/sunna/apps/sunna/ui/style.css:482) |
| More / add | More opacity/background .15s, transform bouncy; add border/background/color .25s, plus rotates90° bouncy. | [style.css:583](/home/sunny/projects/sunna/apps/sunna/ui/style.css:583) [style.css:601](/home/sunny/projects/sunna/apps/sunna/ui/style.css:601) |
| Remove / reorder layout | leave.45s ease-in to scale.88 blur6 fade, JS waits380ms then removes; FLIP gentle translations for survivors. | [style.css:590](/home/sunny/projects/sunna/apps/sunna/ui/style.css:590) [main.js:854](/home/sunny/projects/sunna/apps/sunna/ui/main.js:854) [motion.js:73](/home/sunny/projects/sunna/apps/sunna/ui/motion.js:73) |
| Add flight | preview clone flies from sheet to tile using gentle; arriving tile hides screen/label until landing. | [main.js:803](/home/sunny/projects/sunna/apps/sunna/ui/main.js:803) [style.css:173](/home/sunny/projects/sunna/apps/sunna/ui/style.css:173) |
| Controls | buttons scale.96 snappy; background/color/filter .15s, opacity.2s; input ring.15s; switch background.2s, thumb16px bouncy; segmented background/color.15s. | [style.css:896](/home/sunny/projects/sunna/apps/sunna/ui/style.css:896) [style.css:960](/home/sunny/projects/sunna/apps/sunna/ui/style.css:960) [style.css:999](/home/sunny/projects/sunna/apps/sunna/ui/style.css:999) |
| Validation / loading | input shake .42s offsets−6,+5,−3,+2; submit spinner.7s; rescan.9s; check spinner.8s; check text phase-in.35s. | [style.css:929](/home/sunny/projects/sunna/apps/sunna/ui/style.css:929) [style.css:946](/home/sunny/projects/sunna/apps/sunna/ui/style.css:946) [style.css:966](/home/sunny/projects/sunna/apps/sunna/ui/style.css:966) [style.css:1214](/home/sunny/projects/sunna/apps/sunna/ui/style.css:1214) |
| Connecting | tile-screen rect/radius zoom spring; expanded after160ms; desk fade.4s, shade.55s to.86, content opacity.3s/transform gentle delay.12s; reaching progress travels−100→290% in1.3s; video75%,open/end100% width .6s; phase swap.45s y5; return zoom with late fade. | [main.js:347](/home/sunny/projects/sunna/apps/sunna/ui/main.js:347) [style.css:1051](/home/sunny/projects/sunna/apps/sunna/ui/style.css:1051) [style.css:1097](/home/sunny/projects/sunna/apps/sunna/ui/style.css:1097) |
| Sheets | scale.86/opacity0 bouncy from opener pivot; close170ms cubic(.4,0,1,1),scale.94; scrim.25s and removed after320ms; page height snappy and x±28/fade. | [main.js:438](/home/sunny/projects/sunna/apps/sunna/ui/main.js:438) [style.css:1140](/home/sunny/projects/sunna/apps/sunna/ui/style.css:1140) |
| Find | rows .5s out-expo,35ms stagger; background hover.12s; rescan icon clears after400ms. | [style.css:1239](/home/sunny/projects/sunna/apps/sunna/ui/style.css:1239) [main.js:699](/home/sunny/projects/sunna/apps/sunna/ui/main.js:699) |
| Menu / toast / copy | menu scale.9 bouncy; toast y14/scale.96 bouncy in,160ms y8 fade out; copy check scale.8 bouncy, clears1300ms. | [main.js:1050](/home/sunny/projects/sunna/apps/sunna/ui/main.js:1050) [main.js:1095](/home/sunny/projects/sunna/apps/sunna/ui/main.js:1095) [main.js:1137](/home/sunny/projects/sunna/apps/sunna/ui/main.js:1137) |
| Empty illustration | horizontal6.1s and vertical4.3s linear alternate bounces. | [style.css:844](/home/sunny/projects/sunna/apps/sunna/ui/style.css:844) |
| Minor shell controls | toolbar border/background.2s, toolbar/icon buttons background/color.15s, launch Cancel background/color.15s. | [style.css:112](/home/sunny/projects/sunna/apps/sunna/ui/style.css:112) [style.css:130](/home/sunny/projects/sunna/apps/sunna/ui/style.css:130) [style.css:940](/home/sunny/projects/sunna/apps/sunna/ui/style.css:940) [style.css:1126](/home/sunny/projects/sunna/apps/sunna/ui/style.css:1126) |

The CSS easing aliases are `--out-expo: cubic-bezier(0.16, 1, 0.3, 1)` and `--in: cubic-bezier(0.5, 0, 0.75, 0)`. Pre-JS fallback snappy/bouncy/gentle durations are320/520/640ms; motion.js replaces them. [style.css:25](/home/sunny/projects/sunna/apps/sunna/ui/style.css:25)

## E. Launcher platform differences

Desktop has self key/sharing/link/QR, Tailscale discovery, keyboard navigation, session-window defaults and macOS Accessibility onboarding. Android has none of those self-host/discovery controls, but adds external deep-link handling, Paste, touch/keyboard/video defaults and licenses. Desktop launcher runs on macOS/Linux with Linux toolbar padding and sharing setup text; its shortcut labels largely stay Mac-specific. [index.html:230](/home/sunny/projects/sunna/apps/sunna/ui/index.html:230) [main.js:874](/home/sunny/projects/sunna/apps/sunna/ui/main.js:874) [main.js:976](/home/sunny/projects/sunna/apps/sunna/ui/main.js:976) [style.css:118](/home/sunny/projects/sunna/apps/sunna/ui/style.css:118) [Settings.kt:53](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Settings.kt:53) [AndroidManifest.xml:30](/home/sunny/projects/sunna/apps/android/app/src/main/AndroidManifest.xml:30)

## F. Launcher unfinished / inconsistent / nonproduction paths

- Backend reorder exists, no UI or drag handler; mock reorder is a no-op. Cancel during resolution has no effect (pid zero), whereas Android cancels its asynchronous stages. [machines.rs:214](/home/sunny/projects/sunna/apps/sunna/src/machines.rs:214) [mock.js:140](/home/sunny/projects/sunna/apps/sunna/dev/mock.js:140) [main.rs:397](/home/sunny/projects/sunna/apps/sunna/src/main.rs:397) [lib.rs:279](/home/sunny/projects/sunna/crates/android/src/lib.rs:279)
- Form copies `latest` check metadata on save without Android’s matching address/key guard; edits can preserve previous-address last_seen. Android clears it. [main.js:650](/home/sunny/projects/sunna/apps/sunna/ui/main.js:650) [machines.rs:172](/home/sunny/projects/sunna/apps/sunna/src/machines.rs:172) [MachineSheet.kt:296](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MachineSheet.kt:296) [Machines.kt:231](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Machines.kt:231)
- Key reveal aria stays “Show key” after revealing; Settings title also remains Show. macOS glyphs appear on Linux tooltips/menu/settings/copy failure. [main.js:647](/home/sunny/projects/sunna/apps/sunna/ui/main.js:647) [main.js:907](/home/sunny/projects/sunna/apps/sunna/ui/main.js:907) [index.html:274](/home/sunny/projects/sunna/apps/sunna/ui/index.html:274) [main.js:1050](/home/sunny/projects/sunna/apps/sunna/ui/main.js:1050) [main.js:1137](/home/sunny/projects/sunna/apps/sunna/ui/main.js:1137)
- Confirmation scrim close leaves unresolved promise; access-check failure is treated as allowed; startup permission reminder is described as once but has no persisted once flag here. [main.js:438](/home/sunny/projects/sunna/apps/sunna/ui/main.js:438) [main.js:829](/home/sunny/projects/sunna/apps/sunna/ui/main.js:829) [main.js:874](/home/sunny/projects/sunna/apps/sunna/ui/main.js:874) [main.js:1275](/home/sunny/projects/sunna/apps/sunna/ui/main.js:1275)
- Mock is browser-only, not product backend: fixed hosts/key; ready,busy,wrong-key,offline samples; `scene=empty`, `fail`, `access=no`; comments advertise `time=19:30` but no use of `time` exists; fake QR is not scannable; cancel/access/reset/reorder no-ops; video700ms/open1400ms/end3600ms (failure1500ms). `update-needed` exists in renderer but default mock does not produce it. Mock errors and host names are demo data, not extra product screens. [mock.js:1](/home/sunny/projects/sunna/apps/sunna/dev/mock.js:1) [mock.js:19](/home/sunny/projects/sunna/apps/sunna/dev/mock.js:19) [mock.js:45](/home/sunny/projects/sunna/apps/sunna/dev/mock.js:45) [mock.js:79](/home/sunny/projects/sunna/apps/sunna/dev/mock.js:79) [mock.js:98](/home/sunny/projects/sunna/apps/sunna/dev/mock.js:98) [mock.js:140](/home/sunny/projects/sunna/apps/sunna/dev/mock.js:140) [mock.js:162](/home/sunny/projects/sunna/apps/sunna/dev/mock.js:162)

# 2. Desktop native in-session viewer

## A. Surfaces, controls and state checklists

### V01. Remote desktop window

Entry: launcher ready connection; also native viewer entry point. Title **“Sunna — {host_name}”**. Remote pixels are the content; OS chrome/window-close and menu/hotkeys are controls. Fullscreen default comes from launcher; windowed size budget uses 90% screen width/80% height. Fit preserves aspect; if frame fits and covers at least85% of either axis it uses 1:1 instead. Native macOS presentation or Linux GPU normally supplies overlays; CPU fallback displays video but does not implement those overlays/menu. Remote clicks, mouse buttons, wheel, keyboard and pinch are forwarded; pointer leaves/focus loss release appropriate local/remote state. [session.rs:29](/home/sunny/projects/sunna/crates/viewer/src/session.rs:29) [session.rs:199](/home/sunny/projects/sunna/crates/viewer/src/session.rs:199) [viewer.rs:46](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:46) [viewer.rs:937](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:937) [viewer.rs:1144](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:1144)

Window close, Disconnect or Ctrl+Alt+Q ends session, launcher returns. No reconnect screen/automatic reconnection flow is rendered. Packet/frame recovery runs internally without a reconnecting label; connection death becomes launcher reason toast. [session.rs:107](/home/sunny/projects/sunna/crates/viewer/src/session.rs:107) [viewer.rs:344](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:344) [lib.rs:509](/home/sunny/projects/sunna/crates/client/src/lib.rs:509)

**State checklist (8):** **1.** waiting for first picture; **2.** fullscreen; **3.** windowed; **4.** live picture; **5.** focused; **6.** unfocused; **7.** native renderer unavailable; **8.** session ending. [viewer.rs:937](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:937) [session.rs:199](/home/sunny/projects/sunna/crates/viewer/src/session.rs:199)

### V02. Live stats bar

Entry: default on, toggle Stats Bar or Ctrl+Alt+S. Top single-line monospace overlay; no buttons except separate menu. Shows uppercase codec + width×height, actual fps, actual Mbps / target Mbps when host report exists (actual only otherwise), p50/p95 latency, encode/decode milliseconds, RTT, lost count only>0, sound buffered ms only when sound on and player data exists, tile batches/s only>0. Joins fields using two spaces on both sides of ·; appends platform hide shortcut. Update is throttled to500ms. macOS Menlo12, translucent rounded background; long lines do not have a bespoke responsive breakdown. [lib.rs:186](/home/sunny/projects/sunna/crates/client/src/lib.rs:186) [viewer.rs:90](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:90) [viewer.rs:253](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:253) [layer_presenter.rs:129](/home/sunny/projects/sunna/crates/viewer/src/layer_presenter.rs:129)

**State checklist (10):** **1.** hidden; **2.** connecting; **3.** streaming basic; **4.** host metrics available; **5.** latency available; **6.** decoder metrics available; **7.** RTT available; **8.** loss present; **9.** sound buffer available; **10.** fast-lane tiles present. [lib.rs:186](/home/sunny/projects/sunna/crates/client/src/lib.rs:186) [viewer.rs:253](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:253)

### V03. Notice overlay and missing-picture warning

Single transient bottom notice, normally6s; newer notices replace older ones. In-session actions produce exact messages cataloged below. Stream change notice lasts6s, success4s, error8s; keyboard-access warning12s. No-picture logic after3s without video distinguishes host fps0 (“its screen isn't being drawn”) from incoming video failure, refreshes with2500ms expiry and clears on picture return; it can replace other notices. `host` is title host or **“the other computer”**. No dismiss button. Clipboard typing is bounded to4000 characters and paced8ms between groups; unsupported US-layout characters are skipped and counted. [viewer.rs:76](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:76) [viewer.rs:295](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:295) [viewer.rs:463](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:463) [viewer.rs:615](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:615) [viewer.rs:697](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:697)

**State checklist (18):** **1.** none; **2.** startup keyboard help; **3.** capture unavailable; **4.** capture enabled; **5.** capture released; **6.** stream changing; **7.** stream changed; **8.** stream change failed; **9.** clipboard enabled; **10.** clipboard paused; **11.** clipboard empty; **12.** clipboard typing; **13.** shortcut sent; **14.** sound on; **15.** sound off; **16.** menu button hidden; **17.** host not drawing; **18.** video not arriving. [viewer.rs:295](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:295) [viewer.rs:478](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:478) [viewer.rs:544](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:544) [viewer.rs:667](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:667)

### V04. ••• menu button

Top-left36×24 at14,14, shows **“•••”**. Click opens V05. Idle opacity.35, hover/menu .95. **“Hide Menu Button”** removes it; Ctrl+Alt+M still opens menu and **“Show Menu Button”** restores it. Default visible; setting can start hidden. Button hover participates in pointer/capture handling; not a draggable Android-style pill. [layer_presenter.rs:129](/home/sunny/projects/sunna/crates/viewer/src/layer_presenter.rs:129) [gpu.rs:687](/home/sunny/projects/sunna/crates/viewer/src/gpu.rs:687) [viewer.rs:152](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:152) [viewer.rs:368](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:368)

**State checklist (4):** **1.** visible idle; **2.** hovered; **3.** menu open; **4.** hidden. [layer_presenter.rs:129](/home/sunny/projects/sunna/crates/viewer/src/layer_presenter.rs:129) [gpu.rs:687](/home/sunny/projects/sunna/crates/viewer/src/gpu.rs:687)

### V05. Root session menu

Entry: ••• or Ctrl+Alt+M. Header host and disabled stream detail (**“Connecting…”** or codec/size/fps/latency); no edit connection control. Items: **“Windowed”** when fullscreen / **“Full Screen”** otherwise (F), **“Stats Bar”** (S), **“Play Sound”**, **“Keyboard”**, **“Clipboard”**, **“Video”**, **“Hide Menu Button” / “Show Menu Button”**, **“Disconnect”** (Q). Parentheses here describe shortcut key equivalents, not label text. Linux orders submenus before window/stats/sound; macOS after. Menu temporarily releases keyboard capture and held keys; close restores capture unless explicitly changed. macOS uses native dark NSMenu; Linux uses egui popup, checked ✓ and submenu ⏵, mouse hover/click, Escape/outside dismiss. [menu.rs:385](/home/sunny/projects/sunna/crates/viewer/src/menu.rs:385) [gpu.rs:701](/home/sunny/projects/sunna/crates/viewer/src/gpu.rs:701) [viewer.rs:368](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:368)

**State checklist (6):** **1.** closed; **2.** open connecting; **3.** open streaming; **4.** fullscreen checked; **5.** stats checked; **6.** sound checked. [menu.rs:385](/home/sunny/projects/sunna/crates/viewer/src/menu.rs:385) [gpu.rs:701](/home/sunny/projects/sunna/crates/viewer/src/gpu.rs:701) [viewer.rs:368](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:368)

### V06. Keyboard submenu

**“Send ⌘ Shortcuts to Remote”** on Mac viewer / **“Send System Shortcuts to Remote”** on Linux, check state and G key; disabled if capture unavailable. **“Send keys”** heading, then six host-specific shortcut items: Mac Switch Apps, Spotlight, Mission Control, Force Quit…, Lock Screen, Escape; otherwise Switch Windows, App Launcher, Terminal, Lock Screen, Log Out…, Escape. Full labels/chords in B. Selecting synthesizes key chord and shows Sent notice. Unknown host uses Mac list; any known non-Mac host (including Windows) uses Linux list. [menu.rs:295](/home/sunny/projects/sunna/crates/viewer/src/menu.rs:295) [gpu.rs:762](/home/sunny/projects/sunna/crates/viewer/src/gpu.rs:762) [keys.rs:31](/home/sunny/projects/sunna/crates/client/src/keys.rs:31) [keys.rs:93](/home/sunny/projects/sunna/crates/client/src/keys.rs:93) [viewer.rs:544](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:544)

**State checklist (5):** **1.** capture available and on; **2.** capture available and off; **3.** capture unavailable; **4.** Mac host shortcuts; **5.** non-Mac host shortcuts. [menu.rs:295](/home/sunny/projects/sunna/crates/viewer/src/menu.rs:295) [keys.rs:93](/home/sunny/projects/sunna/crates/client/src/keys.rs:93)

### V07. Clipboard submenu

**“Share Clipboard”** checked toggle controls synchronization; **“Type Clipboard Text”** sends local text as US-layout key events, useful for login/password fields. Empty clipboard only notices; typable text count/skipped count notices. Local clipboard sharing is independent of typing. No preview/editor/permission dialog. [menu.rs:324](/home/sunny/projects/sunna/crates/viewer/src/menu.rs:324) [viewer.rs:554](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:554) [viewer.rs:615](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:615) [keys.rs:100](/home/sunny/projects/sunna/crates/client/src/keys.rs:100)

**State checklist (5):** **1.** sharing on; **2.** sharing paused; **3.** type requested with no text; **4.** type requested with supported text; **5.** type requested with skipped characters. [menu.rs:324](/home/sunny/projects/sunna/crates/viewer/src/menu.rs:324) [viewer.rs:615](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:615)

### V08. Video submenu

**“Codec”**: **“HEVC (sharper per bit)”**, **“H.264 (most compatible)”**. **“Resolution”**: **“Native”** (selected100% becomes **“Native ({}×{})”**), **“75% (less to send, softer)”**, **“50% (less to send, softer)”**. Percent uses viewer stream-size budget, not necessarily full host-native resolution. **“Frame rate”**: **“60 fps”**, **“30 fps (half the work for slow computers)”**. **“Bitrate limit”**: **“10 Mbps”**, **“20 Mbps”**, **“40 Mbps”**, **“80 Mbps”**. **“Fast Lane (lossless text while typing)”** toggle. Checkmarks indicate actual/requested values according to menu state; video controls disabled before nonempty codec. No Auto choice for codec/bitrate. Changing sends desired whole StreamSettings; no modal confirmation; host errors surface V03. Default scale100, stream values host/default negotiated, requested bitrate initially None. [menu_model.rs:39](/home/sunny/projects/sunna/crates/viewer/src/menu_model.rs:39) [menu.rs:207](/home/sunny/projects/sunna/crates/viewer/src/menu.rs:207) [viewer.rs:418](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:418) [viewer.rs:463](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:463) [viewer.rs:497](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:497)

**State checklist (5):** **1.** unavailable before codec; **2.** available; **3.** change pending; **4.** change succeeded; **5.** change rejected. [menu.rs:207](/home/sunny/projects/sunna/crates/viewer/src/menu.rs:207) [viewer.rs:463](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:463) [viewer.rs:478](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:478)

### V09. Remote pointer and capture feedback

Host supplies arbitrary RGBA cursor bitmap, dimensions and hotspot: arrow, I-beam, resize/hand etc are image data, not a fixed named option list. macOS NSCursor applies host shape; Linux caches/scales custom cursors (scale clamp.25…4). Cursor only shown as remote while inside picture and menu closed; OS arrow/default outside/menu, CPU fallback does not use Linux GPU cursor presentation. There is no local pointer mode selector. Linux viewer remaps Ctrl/Super for Mac or unknown hosts unless `SUNNA_SWAP_CTRL_CMD=0`. [remote_cursor.rs:1](/home/sunny/projects/sunna/crates/viewer/src/remote_cursor.rs:1) [cursor.rs:1](/home/sunny/projects/sunna/crates/viewer/src/cursor.rs:1) [viewer.rs:725](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:725) [viewer.rs:773](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:773)

**State checklist (5):** **1.** remote bitmap visible; **2.** pointer outside; **3.** menu pointer; **4.** fallback cursor; **5.** scaled bitmap. [viewer.rs:773](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:773) [cursor.rs:9](/home/sunny/projects/sunna/crates/viewer/src/cursor.rs:9) [remote_cursor.rs:50](/home/sunny/projects/sunna/crates/viewer/src/remote_cursor.rs:50)

### B. Window, menus and controls — exact copy

| String or template (verbatim) | Source |
|---|---|
| `Sunna — {host_name}` | [session.rs:212](/home/sunny/projects/sunna/crates/viewer/src/session.rs:212) |
| `Connecting…` | [viewer.rs:419](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:419) |
| `{} {}×{}` | [viewer.rs:422](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:422) |
| `{} fps` | [viewer.rs:428](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:428) |
| `{p50:.0} ms` | [viewer.rs:431](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:431) |
| `•••` | [layer_presenter.rs:284](/home/sunny/projects/sunna/crates/viewer/src/layer_presenter.rs:284) |
| `Windowed` | [menu.rs:396](/home/sunny/projects/sunna/crates/viewer/src/menu.rs:396) |
| `Full Screen` | [menu.rs:398](/home/sunny/projects/sunna/crates/viewer/src/menu.rs:398) |
| `Stats Bar` | [menu.rs:412](/home/sunny/projects/sunna/crates/viewer/src/menu.rs:412) |
| `Play Sound` | [menu.rs:421](/home/sunny/projects/sunna/crates/viewer/src/menu.rs:421) |
| `Keyboard` | [menu.rs:429](/home/sunny/projects/sunna/crates/viewer/src/menu.rs:429) |
| `Clipboard` | [menu.rs:431](/home/sunny/projects/sunna/crates/viewer/src/menu.rs:431) |
| `Video` | [menu.rs:433](/home/sunny/projects/sunna/crates/viewer/src/menu.rs:433) |
| `Hide Menu Button` | [menu.rs:436](/home/sunny/projects/sunna/crates/viewer/src/menu.rs:436) |
| `Show Menu Button` | [menu.rs:438](/home/sunny/projects/sunna/crates/viewer/src/menu.rs:438) |
| `Disconnect` | [menu.rs:453](/home/sunny/projects/sunna/crates/viewer/src/menu.rs:453) |
| `Send ⌘ Shortcuts to Remote` | [menu.rs:302](/home/sunny/projects/sunna/crates/viewer/src/menu.rs:302) |
| `Send System Shortcuts to Remote` | [gpu.rs:762](/home/sunny/projects/sunna/crates/viewer/src/gpu.rs:762) |
| `Send keys` | [menu.rs:309](/home/sunny/projects/sunna/crates/viewer/src/menu.rs:309) |
| `Share Clipboard` | [menu.rs:331](/home/sunny/projects/sunna/crates/viewer/src/menu.rs:331) |
| `Type Clipboard Text` | [menu.rs:340](/home/sunny/projects/sunna/crates/viewer/src/menu.rs:340) |
| `Codec` | [menu.rs:212](/home/sunny/projects/sunna/crates/viewer/src/menu.rs:212) |
| `HEVC (sharper per bit)` | [menu.rs:215](/home/sunny/projects/sunna/crates/viewer/src/menu.rs:215) |
| `H.264 (most compatible)` | [menu.rs:216](/home/sunny/projects/sunna/crates/viewer/src/menu.rs:216) |
| `Resolution` | [menu.rs:230](/home/sunny/projects/sunna/crates/viewer/src/menu.rs:230) |
| `Native ({}×{})` | [menu.rs:233](/home/sunny/projects/sunna/crates/viewer/src/menu.rs:233) |
| `Native` | [menu.rs:235](/home/sunny/projects/sunna/crates/viewer/src/menu.rs:235) |
| `{percent}% (less to send, softer)` | [menu.rs:237](/home/sunny/projects/sunna/crates/viewer/src/menu.rs:237) |
| `Frame rate` | [menu.rs:251](/home/sunny/projects/sunna/crates/viewer/src/menu.rs:251) |
| `30 fps (half the work for slow computers)` | [menu.rs:254](/home/sunny/projects/sunna/crates/viewer/src/menu.rs:254) |
| `{fps} fps` | [menu.rs:256](/home/sunny/projects/sunna/crates/viewer/src/menu.rs:256) |
| `Bitrate limit` | [menu.rs:269](/home/sunny/projects/sunna/crates/viewer/src/menu.rs:269) |
| `{mbps} Mbps` | [menu.rs:275](/home/sunny/projects/sunna/crates/viewer/src/menu.rs:275) |
| `Fast Lane (lossless text while typing)` | [menu.rs:286](/home/sunny/projects/sunna/crates/viewer/src/menu.rs:286) |
| `⏵` | [gpu.rs:709](/home/sunny/projects/sunna/crates/viewer/src/gpu.rs:709) |
| `✓` | [gpu.rs:853](/home/sunny/projects/sunna/crates/viewer/src/gpu.rs:853) |

### B. Send keys items — exact copy

| String or template (verbatim) | Source |
|---|---|
| `Switch Apps (⌘Tab)` | [keys.rs:33](/home/sunny/projects/sunna/crates/client/src/keys.rs:33) |
| `Spotlight (⌘Space)` | [keys.rs:37](/home/sunny/projects/sunna/crates/client/src/keys.rs:37) |
| `Mission Control (⌃↑)` | [keys.rs:41](/home/sunny/projects/sunna/crates/client/src/keys.rs:41) |
| `Force Quit… (⌥⌘Esc)` | [keys.rs:45](/home/sunny/projects/sunna/crates/client/src/keys.rs:45) |
| `Lock Screen (⌃⌘Q)` | [keys.rs:49](/home/sunny/projects/sunna/crates/client/src/keys.rs:49) |
| `Escape` | [keys.rs:53](/home/sunny/projects/sunna/crates/client/src/keys.rs:53) |
| `Switch Windows (Alt+Tab)` | [keys.rs:60](/home/sunny/projects/sunna/crates/client/src/keys.rs:60) |
| `App Launcher (Super)` | [keys.rs:64](/home/sunny/projects/sunna/crates/client/src/keys.rs:64) |
| `Terminal (Ctrl+Alt+T)` | [keys.rs:68](/home/sunny/projects/sunna/crates/client/src/keys.rs:68) |
| `Lock Screen (Super+L)` | [keys.rs:72](/home/sunny/projects/sunna/crates/client/src/keys.rs:72) |
| `Log Out… (Ctrl+Alt+Delete)` | [keys.rs:76](/home/sunny/projects/sunna/crates/client/src/keys.rs:76) |

### B. Stats bar — exact copy

| String or template (verbatim) | Source |
|---|---|
| `connecting…` | [lib.rs:188](/home/sunny/projects/sunna/crates/client/src/lib.rs:188) |
| `{} {}×{}` | [lib.rs:191](/home/sunny/projects/sunna/crates/client/src/lib.rs:191) |
| `{} fps` | [lib.rs:192](/home/sunny/projects/sunna/crates/client/src/lib.rs:192) |
| `{:.1} / {:.0} Mbps` | [lib.rs:195](/home/sunny/projects/sunna/crates/client/src/lib.rs:195) |
| `{:.1} Mbps` | [lib.rs:196](/home/sunny/projects/sunna/crates/client/src/lib.rs:196) |
| `latency {p50:.0} ms (p95 {p95:.0})` | [lib.rs:199](/home/sunny/projects/sunna/crates/client/src/lib.rs:199) |
| `encode {:.1} ms` | [lib.rs:202](/home/sunny/projects/sunna/crates/client/src/lib.rs:202) |
| `decode {decode:.1} ms` | [lib.rs:205](/home/sunny/projects/sunna/crates/client/src/lib.rs:205) |
| `rtt {rtt:.0} ms` | [lib.rs:208](/home/sunny/projects/sunna/crates/client/src/lib.rs:208) |
| `lost {}` | [lib.rs:211](/home/sunny/projects/sunna/crates/client/src/lib.rs:211) |
| `sound {} ms` | [lib.rs:214](/home/sunny/projects/sunna/crates/client/src/lib.rs:214) |
| `tiles {}/s` | [lib.rs:217](/home/sunny/projects/sunna/crates/client/src/lib.rs:217) |
| `⌃⌥S hide` | [viewer.rs:94](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:94) |
| `Ctrl+Alt+S hide` | [viewer.rs:94](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:94) |

### B. Notices and keyboard help — exact copy

| String or template (verbatim) | Source |
|---|---|
| `••• (top left) or ⌃⌥M: menu  ·  ⌃⌥G release keyboard  ·  ⌃⌥F full screen  ·  ⌃⌥Q disconnect` | [viewer.rs:79](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:79) |
| `••• (top left) or Ctrl+Alt+M: menu  ·  Ctrl+Alt+G release keyboard  ·  Ctrl+Alt+F full screen  ·  Ctrl+Alt+Q disconnect` | [viewer.rs:82](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:82) |
| `This desktop doesn't let apps take its shortcuts: Alt+Tab and Super stay here  ·  Ctrl+Alt+Q disconnect` | [viewer.rs:85](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:85) |
| `⌘Tab, ⌘Space and other shortcuts stay on this Mac: allow them in Sunna's Settings → System shortcuts, then reconnect  ·  ⌃⌥Q disconnect` | [viewer.rs:87](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:87) |
| `the other computer` | [viewer.rs:311](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:311) |
| `No picture from {host}: its screen isn't being drawn` | [viewer.rs:314](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:314) |
| `No picture: the video isn't getting through` | [viewer.rs:316](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:316) |
| `Changing the stream…` | [viewer.rs:463](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:463) |
| `Now streaming {} {}×{}  ·  {lane}` | [viewer.rs:478](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:478) |
| `fast lane on` | [viewer.rs:472](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:472) |
| `fast lane off` | [viewer.rs:474](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:474) |
| `Couldn't change the stream: {error}` | [viewer.rs:489](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:489) |
| `Sent {}` | [viewer.rs:544](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:544) |
| `Clipboard sharing paused: copies stay on each computer` | [viewer.rs:554](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:554) |
| `Clipboard sharing on` | [viewer.rs:556](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:556) |
| `Sound on` | [viewer.rs:565](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:565) |
| `Sound off` | [viewer.rs:565](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:565) |
| `Menu button hidden  ·  ⌃⌥M opens the menu` | [viewer.rs:582](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:582) |
| `The clipboard has no text to type` | [viewer.rs:615](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:615) |
| `Typing {typed} characters  ·  {skipped} skipped (not on a US keyboard)` | [viewer.rs:634](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:634) |
| `Typing {typed} characters` | [viewer.rs:636](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:636) |
| `Keyboard captured: ⌘Tab, ⌘Space and other shortcuts go to the remote  ·  ⌃⌥G gives them back to this Mac` | [viewer.rs:667](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:667) |
| `Keyboard released: shortcuts stay on this Mac  ·  ⌃⌥G to capture again` | [viewer.rs:672](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:672) |
| `Keyboard captured: Alt+Tab, Super and other shortcuts go to the remote  ·  Ctrl+Alt+G gives them back` | [viewer.rs:683](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:683) |
| `Keyboard released: shortcuts stay on this computer  ·  Ctrl+Alt+G to capture again` | [viewer.rs:688](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:688) |

### B. Session termination and upstream error templates — exact copy

| String or template (verbatim) | Source |
|---|---|
| `lost the connection: nothing came back for 10 seconds` | [session.rs:75](/home/sunny/projects/sunna/crates/viewer/src/session.rs:75) |
| `replaced: a newer session from this Mac took over` | [session.rs:77](/home/sunny/projects/sunna/crates/viewer/src/session.rs:77) |
| `the host ended the session` | [session.rs:82](/home/sunny/projects/sunna/crates/viewer/src/session.rs:82) |
| `no answer from the host for 10 seconds` | [lib.rs:523](/home/sunny/projects/sunna/crates/client/src/lib.rs:523) |
| `left before the session started` | [lib.rs:524](/home/sunny/projects/sunna/crates/client/src/lib.rs:524) |
| `protocol version mismatch: host {version}, client {}` | [lib.rs:541](/home/sunny/projects/sunna/crates/client/src/lib.rs:541) |
| `host refused the session: {reason}` | [lib.rs:554](/home/sunny/projects/sunna/crates/client/src/lib.rs:554) |
| `expected HelloAck, got {other:?}` | [lib.rs:555](/home/sunny/projects/sunna/crates/client/src/lib.rs:555) |
| `decode thread exited during reconfigure` | [lib.rs:856](/home/sunny/projects/sunna/crates/client/src/lib.rs:856) |
| `decoder reconfigure failed: {error}` | [lib.rs:1054](/home/sunny/projects/sunna/crates/client/src/lib.rs:1054) |

### B. Copy bindings and ownership

The native window does not have a session-ended dialog: termination strings above travel to launcher’s failure mapping (L B); they are included here to preserve the boundary. Arbitrary nonempty host application-close reasons are passed through, and unknown decoder/transport failures can also pass through. `Sent {}` inserts the full shortcut title. Stats `{}` slots bind codec uppercase, width,height; fps; actual/target Mbps; encode milliseconds; lost frame count; sound buffer ms; tile batches per second. Native menu key equivalents F/S/G/Q use Ctrl+Alt modifiers; UI-library/OS menu chrome strings are outside this repository. [session.rs:72](/home/sunny/projects/sunna/crates/viewer/src/session.rs:72) [main.rs:407](/home/sunny/projects/sunna/apps/sunna/src/main.rs:407) [viewer.rs:544](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:544) [lib.rs:186](/home/sunny/projects/sunna/crates/client/src/lib.rs:186) [menu.rs:396](/home/sunny/projects/sunna/crates/viewer/src/menu.rs:396)


### B. Additional error fragments and dynamically composed copy

| String/template | Source |
|---|---|
| `tls setup: {0}` | [lib.rs:34](/home/sunny/projects/sunna/crates/transport/src/lib.rs:34) |
| `certificate generation: {0}` | [lib.rs:36](/home/sunny/projects/sunna/crates/transport/src/lib.rs:36) |
| `quic crypto config: {0}` | [lib.rs:38](/home/sunny/projects/sunna/crates/transport/src/lib.rs:38) |
| `io: {0}` | [lib.rs:40](/home/sunny/projects/sunna/crates/transport/src/lib.rs:40) |
| `connect: {0}` | [lib.rs:42](/home/sunny/projects/sunna/crates/transport/src/lib.rs:42) |
| `connection: {0}` | [lib.rs:44](/home/sunny/projects/sunna/crates/transport/src/lib.rs:44) |
| `stream write: {0}` | [lib.rs:46](/home/sunny/projects/sunna/crates/transport/src/lib.rs:46) |
| `stream read: {0}` | [lib.rs:48](/home/sunny/projects/sunna/crates/transport/src/lib.rs:48) |
| `codec: {0}` | [lib.rs:50](/home/sunny/projects/sunna/crates/transport/src/lib.rs:50) |
| `control message too large ({0} bytes)` | [lib.rs:52](/home/sunny/projects/sunna/crates/transport/src/lib.rs:52) |
| `stream closed: {0}` | [lib.rs:56](/home/sunny/projects/sunna/crates/transport/src/lib.rs:56) |
| `host pipeline failed: {error}` | [lib.rs:592](/home/sunny/projects/sunna/crates/host/src/lib.rs:592) |
| `no decoder for codec {other:?} on this platform` | [lib.rs:304](/home/sunny/projects/sunna/crates/codec/src/lib.rs:304) |
| `no {other} decoder here: this viewer was built without FFmpeg` | [lib.rs:319](/home/sunny/projects/sunna/crates/codec/src/lib.rs:319) |
| `Sunna viewer` | [session.rs:131](/home/sunny/projects/sunna/crates/viewer/src/session.rs:131) |
| `{}  ·  {hide}` | [viewer.rs:95](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:95) |

These are upstream fragments, not additional screens. The platform’s friendly mapping may replace or wrap them; unknown embedded library/OS/host error text remains unbounded. Frame-level decode failures may only log/request recovery rather than end the session; the constructor/reconfiguration failures above are the relevant presentation path. [lib.rs:569](/home/sunny/projects/sunna/crates/client/src/lib.rs:569) [lib.rs:1054](/home/sunny/projects/sunna/crates/client/src/lib.rs:1054) [main.rs:407](/home/sunny/projects/sunna/apps/sunna/src/main.rs:407) [lib.rs:208](/home/sunny/projects/sunna/crates/android/src/lib.rs:208)

## C. Complete viewer flows

1. **Launch/connection:** transport connect → control hello (10s) → token/protocol acceptance → decoder created → session-established logged → wait first frame at most10s → create native window even if no frame (1280×720 fallback) → launcher hides. Every early failure is returned to launcher: wrong token, busy, version mismatch, no decoder, arbitrary refusal, transport/hello/read/write errors. Client cancellation’s “left before…” is an internal reason normally suppressed by intentional exit. [session.rs:107](/home/sunny/projects/sunna/crates/viewer/src/session.rs:107) [session.rs:199](/home/sunny/projects/sunna/crates/viewer/src/session.rs:199) [lib.rs:509](/home/sunny/projects/sunna/crates/client/src/lib.rs:509)
2. **Initial defaults:** fullscreen unless launcher/env windowed; stats/menu button on unless disabled; sound from launcher; clipboard shared; capture attempted where supported; scale100, codec/fps/bitrate use initial negotiated/default stream. Native menu settings are session-local; no persistent preferences write here. [viewer.rs:59](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:59) [viewer.rs:129](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:129) [session.rs:107](/home/sunny/projects/sunna/crates/viewer/src/session.rs:107) [lib.rs:509](/home/sunny/projects/sunna/crates/client/src/lib.rs:509)
3. **Input:** click/move/scroll/mouse buttons forwarded; Linux physical keyboard maps Ctrl/Super for Mac/unknown host unless overridden; Mac global shortcuts captured only with Accessibility. Ctrl+Alt+M menu, G capture/release, F fullscreen/window, S stats, Q disconnect. Shift does not disqualify these matches; Super does. Opening menu temporarily releases held keys/capture, restores on close unless toggle changed. Focus loss releases held keys. [viewer.rs:344](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:344) [viewer.rs:368](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:368) [viewer.rs:725](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:725) [viewer.rs:1144](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:1144) [viewer.rs:1296](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:1296)
4. **Pinch and host shortcuts:** desktop pinch becomes remote zoom (Ctrl-scroll for Linux, ⌘± for Mac); three/four-finger system gestures remain local. Keyboard submenu sends its host-specific chords and Sent notice; unknown host assumes Mac, other known OS uses Linux set. [viewer.rs:1273](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:1273) [keys.rs:31](/home/sunny/projects/sunna/crates/client/src/keys.rs:31) [keys.rs:93](/home/sunny/projects/sunna/crates/client/src/keys.rs:93)
5. **Change video:** submenu → choose codec,size,fps,bitrate,Fast Lane → sends aggregate desired settings → Changing… → actual stream epoch update yields Now streaming… or host/client error yields Couldn't change… and current stream remains. Video menu is enabled by nonempty codec, not an explicit negotiated feature flag. [viewer.rs:418](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:418) [viewer.rs:463](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:463) [viewer.rs:478](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:478) [viewer.rs:497](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:497)
6. **Clipboard and sound:** toggle sharing → on/paused notice; Type Clipboard Text → empty notice or up to4000 characters converted to US keys, skipped count, paced events. Toggle Play Sound → on/off notice; audio initialization failure elsewhere only logs, so checked Sound is not proof playback works. [viewer.rs:554](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:554) [viewer.rs:565](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:565) [viewer.rs:615](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:615) [lib.rs:661](/home/sunny/projects/sunna/crates/client/src/lib.rs:661)
7. **Missing picture:** frame gap≥3s → host-report fps0 distinguishes remote screen not drawn from video not arriving; notice renews until frame returns. This is not reconnecting and has no Retry button. [viewer.rs:295](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:295)
8. **Leave:** menu Disconnect, Ctrl+Alt+Q or window close cancels client; launcher restores without error. Transport idle timeout10s, host stopped/restarted, replacement from same client, raw application-close reason, process panic and other unexpected termination all reach launcher mappings. Endpoint cleanup waits500ms; client stop completion bounded1s. [session.rs:72](/home/sunny/projects/sunna/crates/viewer/src/session.rs:72) [session.rs:107](/home/sunny/projects/sunna/crates/viewer/src/session.rs:107) [main.rs:319](/home/sunny/projects/sunna/apps/sunna/src/main.rs:319) [main.rs:407](/home/sunny/projects/sunna/apps/sunna/src/main.rs:407)

## D. Existing native viewer motion

| Element | Behavior / timing | Source |
|---|---|---|
| Video / overlay layout | Frame presentation follows incoming frames. macOS Core Animation transactions disable implicit animations for frame/layout/text changes; no app-defined crossfade for stats/notices. | [layer_presenter.rs:383](/home/sunny/projects/sunna/crates/viewer/src/layer_presenter.rs:383) [layer_presenter.rs:424](/home/sunny/projects/sunna/crates/viewer/src/layer_presenter.rs:424) |
| ••• | Opacity switches .35↔.95 with hover/open. Native layer opacity mutation may use platform implicit behavior; no explicit animation duration declared. Linux paints immediate value. | [layer_presenter.rs:129](/home/sunny/projects/sunna/crates/viewer/src/layer_presenter.rs:129) [gpu.rs:687](/home/sunny/projects/sunna/crates/viewer/src/gpu.rs:687) |
| Menus / fullscreen | NSMenu and OS fullscreen transitions are platform-managed, no explicit duration here. Linux egui popup has no custom duration/spring in app code. | [menu.rs:385](/home/sunny/projects/sunna/crates/viewer/src/menu.rs:385) [gpu.rs:701](/home/sunny/projects/sunna/crates/viewer/src/gpu.rs:701) [viewer.rs:497](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:497) |
| Notices | Expire rather than custom fade: default6s, stream success4s,error8s,access12s; missing-picture refreshed2500ms after threshold3s. | [viewer.rs:76](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:76) [viewer.rs:295](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:295) [viewer.rs:478](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:478) [viewer.rs:697](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:697) |
| Remote pointer | Bitmap/hotspot/scale changes track remote state; no interpolation/shape morph specified. | [viewer.rs:773](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:773) [remote_cursor.rs:50](/home/sunny/projects/sunna/crates/viewer/src/remote_cursor.rs:50) |
| Stats | Replaced at≤2Hz, no count-up/interpolation. | [viewer.rs:253](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:253) |

## E. Viewer differences

- macOS native menu/layers versus Linux egui/GPU; root order, capture label, keyboard help and hide shortcut differ. Capture requires macOS Accessibility or Linux compositor support; Linux Wayland may leave Alt+Tab/Super local. [menu.rs:295](/home/sunny/projects/sunna/crates/viewer/src/menu.rs:295) [gpu.rs:762](/home/sunny/projects/sunna/crates/viewer/src/gpu.rs:762) [viewer.rs:79](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:79) [viewer.rs:697](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:697)
- Desktop has fullscreen/window, shortcut capture, menu-button visibility, three submenus, Fast Lane and no-picture warnings. Android has local pinch zoom, trackpad/direct touch, IME accessory strip, draggable top toolbar, video defaults including Auto, notification/background service. [menu.rs:207](/home/sunny/projects/sunna/crates/viewer/src/menu.rs:207) [viewer.rs:295](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:295) [Session.kt:505](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:505) [Touch.kt:14](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Touch.kt:14) [Rows.kt:126](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Rows.kt:126)

Viewer startup requests a platform-preferred decoder codec when one is known, maximum size and audio. Bitrate, FPS and Fast Lane requests are None initially, so the host decides; they have no universal client-side numeric/on-off default. `SUNNA_VIEWER_NAME` fallback is `Sunna viewer`, visible on the remote host’s busy status. [session.rs:128](/home/sunny/projects/sunna/crates/viewer/src/session.rs:128) [messages.rs:7](/home/sunny/projects/sunna/crates/proto/src/messages.rs:7)

## F. Viewer gaps and inconsistencies

- “Menu button hidden  ·  ⌃⌥M opens the menu” is used on Linux too. Linux capture startup path can show generic keyboard help even if capture unavailable; non-Mac/Windows host shortcut list uses Linux actions. [viewer.rs:582](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:582) [viewer.rs:697](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:697) [keys.rs:93](/home/sunny/projects/sunna/crates/client/src/keys.rs:93)
- Codec stats format H264, menu says H.264; “Native” size is based on viewer budget and may not equal host native. No Auto bitrate/codec in desktop menu though host/default can be unrepresented. [lib.rs:191](/home/sunny/projects/sunna/crates/client/src/lib.rs:191) [menu.rs:215](/home/sunny/projects/sunna/crates/viewer/src/menu.rs:215) [menu.rs:233](/home/sunny/projects/sunna/crates/viewer/src/menu.rs:233) [session.rs:29](/home/sunny/projects/sunna/crates/viewer/src/session.rs:29)
- CPU fallback lacks overlay/menu/cursor implementations; renderer failure is logged rather than a dedicated user-facing degraded-mode message. Audio initialization failures also log without an actionable notice. [viewer.rs:937](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:937) [viewer.rs:253](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:253) [viewer.rs:368](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:368) [lib.rs:661](/home/sunny/projects/sunna/crates/client/src/lib.rs:661)
- No reconnecting screen or retry action; no-picture notices can replace other notices. Stream-error copy differs from Android. [viewer.rs:295](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:295) [viewer.rs:489](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:489) [Session.kt:357](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:357)

# 3. Android (Kotlin plain views + native client)

## A. Surfaces, controls and state checklists

### A01. Computers home and cards

Entry: app launch and return from session. **“Computers”**, summary, add icon aria **“Add a computer”**, **“Settings”**. Empty illustration **“No Signal”**, **“No computers yet”**, QR/link explanation, **“Add a Computer…”**. Normal add tile **“Add a Computer” / “By address and key”**; single-column compact version **“By address and key, or its sunna:// link”**. Cards show name/state/OS-model-device, miniature desktop, **“More”**. Tap ready connects; wrong key opens edit; other states give busy/update/checking/detail notice, with **“Edit”** action for unavailable. Long press opens same popup as More. Card accessibility `${machine.name}. $words`. [Home.kt:54](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Home.kt:54) [Home.kt:214](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Home.kt:214) [Home.kt:413](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Home.kt:413) [Home.kt:525](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Home.kt:525)

Polling every4s while resumed, results from edited address/key discarded; metadata saved after checks. Same state meanings as desktop, same last-seen rules/format. Missing/malformed store reads as empty. Grid uses available width: minimum290dp for≤3 items,190dp≤6,150dp beyond, max440dp; one-column add becomes slim. No drag reorder. [Home.kt:488](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Home.kt:488) [Home.kt:544](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Home.kt:544) [Machines.kt:135](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Machines.kt:135) [Machines.kt:169](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Machines.kt:169)

**State checklist (14):** **1.** initial loading; **2.** empty; **3.** populated; **4.** checking; **5.** ready without latency; **6.** ready with latency; **7.** busy without viewer; **8.** busy with viewer; **9.** wrong-key; **10.** update-needed; **11.** not-found; **12.** invalid; **13.** offline never seen; **14.** offline previously seen. [Home.kt:475](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Home.kt:475) [Machines.kt:135](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Machines.kt:135)

### A02. Add / edit machine bottom sheet

Entry: add/More Edit…/wrong-key tap/link intent. Title **“Add a Computer”** or **“Edit “${machine.name}””**; close icon **“Close”**. Preview screen/name/status/dot. **“Address”** placeholder, **“Paste”** icon only for add; password **“Key”** placeholder, **“Show key” / “Hide key”**; **“Name”**, placeholder **“Name (optional)”** or **“$it (from the computer)”**. **“Remove”** only edit, **“Cancel”**, **“Add” / “Save”**. IME Done saves. Key defaults empty, no phone-host key. Form autofocus shows keyboard after320ms; sheet moves for IME. [MachineSheet.kt:40](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MachineSheet.kt:40) [MachineSheet.kt:57](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MachineSheet.kt:57) [MachineSheet.kt:90](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MachineSheet.kt:90) [MachineSheet.kt:145](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MachineSheet.kt:145)

Check debounce450ms; known check kept on immediate unchanged request; latest result sequence/open guard. Errors show colored helper and shake relevant field on save. Save requires native address parse and rejects LF in key, but permits blank/wrong key and unreachable host. Learned metadata is applied only for matching checked address/key. Store error distinguishes missing saved record from storage failure. Paste empty has its own text; pasted/deep-linked key gets helper **“The key came with the link.”** Existing non-error helper may remain after ready. [MachineSheet.kt:190](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MachineSheet.kt:190) [MachineSheet.kt:239](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MachineSheet.kt:239) [MachineSheet.kt:296](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MachineSheet.kt:296)

**State checklist (16):** **1.** blank add; **2.** editing; **3.** deep link populated; **4.** clipboard pasted; **5.** idle preview; **6.** checking; **7.** ready; **8.** busy; **9.** wrong-key; **10.** update-needed; **11.** not-found; **12.** invalid; **13.** unreachable; **14.** key revealed; **15.** validation failure; **16.** storage failure. [MachineSheet.kt:190](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MachineSheet.kt:190) [MachineSheet.kt:239](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MachineSheet.kt:239) [MachineSheet.kt:296](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MachineSheet.kt:296)

### A03. Machine popup menu

Entry: More or long press. **“Connect”** only when ready (absent otherwise); **“Edit…”**, **“Copy Address”**, **“Remove…”**. Copy uses clipboard label **“Address”**, notice **“Copied ${machine.address}”**. No Show QR Code. Anchored popup with fade/scale; outside dismisses. [MainActivity.kt:167](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MainActivity.kt:167) [Sheet.kt:295](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Sheet.kt:295)

**State checklist (3):** **1.** ready; **2.** not ready; **3.** dismissed. [MainActivity.kt:167](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MainActivity.kt:167) [Sheet.kt:295](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Sheet.kt:295)

### A04. Remove confirmation

Entry: popup Remove… or edit Remove. Bottom sheet **“Remove “${machine.name}”?”**, **“You can add it again with its address and key.”**, **“Cancel”**, **“Remove”**, header **“Close”**. Success rerenders home, no removal-success notice or undo. Failure appears in form or notice depending entry. [MainActivity.kt:177](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MainActivity.kt:177) [MachineSheet.kt:332](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MachineSheet.kt:332) [Sheet.kt:273](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Sheet.kt:273)

**State checklist (4):** **1.** awaiting choice; **2.** cancelled; **3.** confirmed; **4.** storage failure. [MainActivity.kt:177](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MainActivity.kt:177) [MachineSheet.kt:332](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MachineSheet.kt:332)

### A05. Settings screen

Entry: home Settings. Fullscreen scrolling page, **“Back”**, **“Settings”**. **“This phone”** field uses device/system name fallback and saves changed custom name; helper embeds initial ownName. **“Sessions”**: Touch Trackpad/Touch, Sound, Share the clipboard, Keyboard button, Keyboard suggestions, Stats bar. **“Video”**: Sharpness, Frame rate, Bitrate, Codec. **“About”**: Open source software link and version/protocol. No save/apply button, changes persist immediately. Defaults/options in C below. [Settings.kt:42](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Settings.kt:42) [Settings.kt:53](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Settings.kt:53) [Settings.kt:72](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Settings.kt:72) [Settings.kt:89](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Settings.kt:89) [Machines.kt:263](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Machines.kt:263)

**State checklist (4):** **1.** open; **2.** phone name editing; **3.** preferences changed; **4.** closing. [Settings.kt:53](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Settings.kt:53) [Settings.kt:119](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Settings.kt:119)

### A06. Open Source Software sheet

Entry: Settings **“Open source software”**. Header **“Open Source Software”**, **“Close”**, long monospace scrolling ListView split by blank paragraphs. Source asset is fully reproduced in the final appendix, with per-paragraph citations. Missing/read failure silently produces empty list. [Settings.kt:139](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Settings.kt:139) [licenses.txt:1](/home/sunny/projects/sunna/apps/android/app/src/main/assets/licenses.txt:1)

**State checklist (3):** **1.** loaded; **2.** asset missing or unreadable; **3.** dismissed. [Settings.kt:139](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Settings.kt:139)

### A07. Connecting screen / return transition

Entry: ready tile. Enlarged miniature screen, machine name/OS mark, shimmer/progress, **“Cancel”**. **“Reaching ${machine.name}…”** during resolve/transport, **“Starting video…”** before client handshake; streaming starts on first decoded frame and reveals remote view. There is no visible Connected phase. Cancel/Back ends silently; ending reason before picture is shown in launch phase, after picture phase says **“Disconnected”**, then home notice receives reason. Streaming termination waits500ms before reverse; Activity destroy leaves immediately without animation. [Launch.kt:45](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Launch.kt:45) [Session.kt:207](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:207) [Session.kt:252](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:252) [Session.kt:279](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:279) [lib.rs:279](/home/sunny/projects/sunna/crates/android/src/lib.rs:279)

**State checklist (6):** **1.** reaching; **2.** starting video; **3.** first frame revealing; **4.** cancel requested; **5.** failure before stream; **6.** disconnected after stream. [Session.kt:207](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:207) [Session.kt:279](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:279) [Session.kt:364](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:364)

### A08. Remote session picture / gestures

Fullscreen remote image with keep-screen-on and transient system bars; viewport fits safe area, cutout and keyboard. No fullscreen/window toggle. Touch defaults phone trackpad/tablet direct. Trackpad: one-finger relative movement with speed gain1.15…2.85, tap left click, hold-drag; direct: tap absolute left click, move drag, hold right click. Two-finger tap right click; two-finger drag scroll with momentum; pinch locally zooms/pans; three-finger tap toggles keyboard. Keyboard/menu buttons also available. Physical keyboard forwards supported keys, excludes Back/Home/volume/power/app switch; focus/pause releases keys/buttons. Background retains session; destruction disconnects. [Session.kt:381](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:381) [Session.kt:505](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:505) [Session.kt:660](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:660) [Session.kt:792](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:792) [Touch.kt:14](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Touch.kt:14) [Pointer.kt:1](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Pointer.kt:1)

**State checklist (8):** **1.** live foreground; **2.** background retained; **3.** trackpad mode; **4.** direct touch mode; **5.** zoomed; **6.** keyboard visible; **7.** menu visible; **8.** ending. [Session.kt:381](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:381) [Session.kt:792](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:792) [Touch.kt:14](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Touch.kt:14)

### A09. Movable session toolbar

Top pill: keyboard icon aria **“Keyboard” / “Hide the keyboard”**, more icon aria **“Session menu”**. Keyboard icon optional (default on); menu icon remains. Drag horizontally to reposition; persisted menuButtonAt −1…1,default0(center), respects cutout. Idle dims after3s; interaction restores. Stats move to opposite side, min120dp space. Menu click hides keyboard/opens A11; keyboard click toggles IME. [Session.kt:137](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:137) [Session.kt:316](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:316) [Session.kt:417](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:417) [Session.kt:505](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:505) [Machines.kt:263](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Machines.kt:263)

**State checklist (6):** **1.** keyboard button shown; **2.** keyboard button hidden; **3.** keyboard up; **4.** menu open; **5.** idle dim; **6.** dragging position. [Session.kt:316](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:316) [Session.kt:417](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:417) [Session.kt:505](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:505)

### A10. Session stats

Off by default; Settings Stats bar or menu Stats toggles, persisted. Same shared line as desktop, no hide shortcut appended; JNI adds **“  ·  hardware decoder”** or **“  ·  software decoder”** depending MediaCodec name (c2.android./OMX.google. software). Android decode/presentation delay contributes metrics; session menu fps can use shownFps, distinct from shared stats fps. The shared no-picture counter is not exposed in Android status or presented as a warning here. [Session.kt:374](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:374) [lib.rs:131](/home/sunny/projects/sunna/crates/android/src/lib.rs:131) [lib.rs:186](/home/sunny/projects/sunna/crates/client/src/lib.rs:186)

**State checklist (6):** **1.** off; **2.** connecting; **3.** basic stream; **4.** optional host and transport metrics; **5.** hardware decoder; **6.** software decoder. [Session.kt:374](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:374) [lib.rs:131](/home/sunny/projects/sunna/crates/android/src/lib.rs:131)

### A11. Session menu sheet

Entry: ••• or Back after stream is shown (Back first hides keyboard if up). Bottom scrolling sheet; header machine name, OS badge, codec·size·fps, **“Close”**. Quick actions **“Keyboard”** (closes menu, opens IME), **“Touch” / “Trackpad”** (toggles mode+notice), **“Sound On” / “Sound Off”** (toggle/persist/request), **“Stats”** (toggle/persist). **“Shortcuts”** includes six shared host-specific chord buttons. **“Keyboard”** group: Keyboard button and Keyboard suggestions switches. **“Clipboard”**: Share the clipboard switch; Type clipboard text link. **“Video”**: same four choice rows as Settings. **“Disconnect”** destructive. All groups in one sheet, no desktop-like submenus. Toggle/choice labels are accessibility labels; choice accessibility is `$title: $label`. [SessionMenu.kt:42](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/SessionMenu.kt:42) [SessionMenu.kt:68](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/SessionMenu.kt:68) [SessionMenu.kt:87](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/SessionMenu.kt:87) [Rows.kt:106](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Rows.kt:106)

Preferences here persist, including video. Request failure shows **“The computer kept its video as it was: $streamError”**; no desktop-style Changing/Now streaming success notice. Requested preferences can diverge from actual header. Shared shortcuts choose Mac for unknown OS, Linux for every known non-Mac OS. [Rows.kt:126](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Rows.kt:126) [Session.kt:357](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:357) [keys.rs:93](/home/sunny/projects/sunna/crates/client/src/keys.rs:93)

**State checklist (11):** **1.** open; **2.** touch selected; **3.** trackpad selected; **4.** sound on; **5.** sound off; **6.** stats on; **7.** stats off; **8.** clipboard on; **9.** clipboard off; **10.** video selection changed; **11.** stream change rejected. [SessionMenu.kt:68](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/SessionMenu.kt:68) [Rows.kt:126](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Rows.kt:126) [Session.kt:357](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:357)

### A12. IME and accessory key strip

Entry: keyboard toolbar, menu Keyboard, three-finger tap. System IME is OS/user-selected (its keys, suggestions, permissions are not fixed app strings). Invisible1px input host mediates text; suggestions off uses visible-password/no-suggestions behavior; on uses editable mirror and retypes corrections/swipe edits. App horizontal strip: host Mac modifiers **“⌃” “⌥” “⌘” “⇧”**, otherwise **“Ctrl” “Alt” “Super” “Shift”**; **“esc”**, **“tab”**, **“←” “↑” “↓” “→”**, **“⌦” / “Del”**, **“home” “end” “pg↑” “pg↓”**, F1…F12; hide icon aria **“Hide keyboard”**. [Keys.kt:90](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Keys.kt:90) [Keys.kt:351](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Keys.kt:351)

Modifier tap arms next sent key; double tap within400ms locks, tapping again unlocks; modifiers are synthesized around each event rather than held remotely. Armed outlined/locked filled. Keyboard text/clipboard typing is US-layout limited; nonrepresentable characters produce warning once/session. No alternate keyboard-layout selector. [Keys.kt:351](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Keys.kt:351) [Session.kt:696](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:696)

**State checklist (7):** **1.** hidden; **2.** suggestions off; **3.** suggestions on; **4.** modifier off; **5.** modifier next key; **6.** modifier locked; **7.** unsupported character warning. [Keys.kt:90](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Keys.kt:90) [Keys.kt:351](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Keys.kt:351) [Session.kt:696](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:696)

### A13. Pointer and gesture feedback

Host cursor image and hotspot override fallback arrow; shape data is arbitrary (not a fixed named list of cursors). Invalid shape leaves previous; fallback white arrow black border. Local pointer rendered in viewport; trackpad moves with acceleration and keeps56dp margin when zoomed; direct touches position pointer. Click pulse ring6→24dp fade over380ms. Pinch zoom1…max(2,3/fit), resets if<1.06; pan clamps. No Android pointer-mode “menu” beyond Touch/Trackpad. [Pointer.kt:1](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Pointer.kt:1) [Pointer.kt:190](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Pointer.kt:190) [Session.kt:505](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:505)

**State checklist (5):** **1.** fallback arrow; **2.** remote bitmap; **3.** tap pulse; **4.** zoomed pointer; **5.** shape unavailable. [Pointer.kt:1](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Pointer.kt:1) [Pointer.kt:190](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Pointer.kt:190) [Session.kt:505](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:505)

### A14. Transient notice

Reusable bottom pill, text and optional **“Edit”** action. Default4500ms even errors; first-session help6500ms once via stored hinted flag. New notice replaces old. Receives home gating, copy, link-during-session, storage failure, disconnect reason, video/typing feedback. No long error-details screen, no clipboard-sync on/off notice, no sound-toggle notice beyond quick-button label. [Ui.kt:279](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Ui.kt:279) [Home.kt:525](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Home.kt:525) [Session.kt:309](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:309) [Session.kt:696](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:696)

**State checklist (7):** **1.** normal; **2.** action Edit; **3.** first-session gesture help; **4.** connection failure; **5.** clipboard result; **6.** typing unsupported; **7.** video change failure. [Ui.kt:279](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Ui.kt:279) [Session.kt:309](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:309)

### A15. Foreground service notification

Service is started when connection begins, before successful handshake. Low-importance ongoing notification title **“Connected to $name”** (fallback **“a computer”**), text **“Tap to go back to it”**, **“Disconnect”** action; tap returns Activity, action ends session. Channel **“Sessions”**, description **“While you're connected to a computer”**. No badge. Stops when session ends. START_NOT_STICKY; Activity owns session, this is not guaranteed process-death recovery. Service-start failure is caught silently. [MainActivity.kt:130](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MainActivity.kt:130) [SessionService.kt:29](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/SessionService.kt:29)

**State checklist (5):** **1.** starting connection; **2.** connected foreground; **3.** connected background; **4.** notifications denied; **5.** disconnected. [MainActivity.kt:130](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MainActivity.kt:130) [SessionService.kt:29](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/SessionService.kt:29)

### A16. Android notification permission handoff

Android13/API33+ POST_NOTIFICATIONS requested once after first picture reveals; asked flag set before request. Grant starts/refreshes service; denial has no custom explanation/settings CTA or automatic re-prompt. System title/body/buttons depend on OS/version and app label **“Sunna”**, not hardcoded here. Internet permission is normal/no runtime dialog. No camera permission: external camera scans QR. Manifest service description also available to OS. [MainActivity.kt:110](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MainActivity.kt:110) [Session.kt:296](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:296) [AndroidManifest.xml:4](/home/sunny/projects/sunna/apps/android/app/src/main/AndroidManifest.xml:4) [AndroidManifest.xml:9](/home/sunny/projects/sunna/apps/android/app/src/main/AndroidManifest.xml:9) [strings.xml:3](/home/sunny/projects/sunna/apps/android/app/src/main/res/values/strings.xml:3)

**State checklist (4):** **1.** not yet requested; **2.** system prompt; **3.** granted; **4.** denied. [MainActivity.kt:110](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MainActivity.kt:110) [AndroidManifest.xml:9](/home/sunny/projects/sunna/apps/android/app/src/main/AndroidManifest.xml:9)

### A17. Shared sheet interaction / modal shell

All sheets: scrim outside tap/Back/Close dismiss; narrow full width, ≥600dp width560dp. Drag downward only when touched child cannot scroll further up; dismiss if distance>min28% card height,160dp or velocity>900dp/s, otherwise snap back. IME inset animation moves sheet; confirm uses same shell. Session menu can leave upper14% clear. This reusable shell needs its own touch, safe-area, focus and scroll design even though copy belongs to child screens. [Sheet.kt:43](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Sheet.kt:43) [Sheet.kt:143](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Sheet.kt:143) [Sheet.kt:171](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Sheet.kt:171) [SessionMenu.kt:42](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/SessionMenu.kt:42)

**State checklist (7):** **1.** opening; **2.** open narrow; **3.** open tablet; **4.** IME raised; **5.** dragging; **6.** drag cancelled; **7.** dismissed. [Sheet.kt:43](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Sheet.kt:43) [Sheet.kt:171](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Sheet.kt:171)

### B. Home, machine cards and copy actions — exact copy

| String or template (verbatim) | Source |
|---|---|
| `Sunna` | [strings.xml:3](/home/sunny/projects/sunna/apps/android/app/src/main/res/values/strings.xml:3) |
| `More` | [Home.kt:73](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Home.kt:73) |
| `${machine.name}. $words` | [Home.kt:149](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Home.kt:149) |
| `Add a Computer` | [Home.kt:214](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Home.kt:214) |
| `By address and key` | [Home.kt:216](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Home.kt:216) |
| `By address and key, or its sunna:// link` | [Home.kt:233](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Home.kt:233) |
| `Add a computer` | [Home.kt:247](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Home.kt:247) |
| `No Signal` | [Home.kt:343](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Home.kt:343) |
| `Computers` | [Home.kt:413](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Home.kt:413) |
| `Settings` | [Home.kt:429](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Home.kt:429) |
| `No computers yet` | [Home.kt:444](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Home.kt:444) |
| `Add a computer that's sharing with Sunna: scan its QR code with this phone's camera, or paste its link here. Both appear when sharing starts; Settings in Sunna on that computer (or sunna-host link on Linux) shows them again.` | [Home.kt:449](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Home.kt:449) |
| `Add a Computer…` | [Home.kt:454](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Home.kt:454) |
| `None added yet` | [Home.kt:478](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Home.kt:478) |
| `Looking for your computers…` | [Home.kt:479](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Home.kt:479) |
| `${machines[0].name} is ready` | [Home.kt:481](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Home.kt:481) |
| `${machines[0].name} isn't available` | [Home.kt:481](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Home.kt:481) |
| `All ${machines.size} ready` | [Home.kt:482](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Home.kt:482) |
| `None of ${machines.size} available` | [Home.kt:483](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Home.kt:483) |
| `$ready of ${machines.size} ready` | [Home.kt:484](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Home.kt:484) |
| `${check.viewer.ifEmpty { "Someone" }} is connected to ${machine.name} right now.` | [Home.kt:532](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Home.kt:532) |
| `${machine.name} runs a different version of Sunna.` | [Home.kt:533](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Home.kt:533) |
| `Still looking for ${machine.name}…` | [Home.kt:534](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Home.kt:534) |
| `${machine.name} isn't responding.` | [Home.kt:535](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Home.kt:535) |
| `Edit` | [Home.kt:535](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Home.kt:535) |
| `Laptop` | [Machines.kt:110](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Machines.kt:110) |
| `Desktop` | [Machines.kt:110](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Machines.kt:110) |
| `VM` | [Machines.kt:110](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Machines.kt:110) |
| `Server` | [Machines.kt:110](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Machines.kt:110) |
| `<1 ms` | [Machines.kt:122](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Machines.kt:122) |
| `${value.roundToInt()} ms` | [Machines.kt:122](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Machines.kt:122) |
| `just now` | [Machines.kt:127](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Machines.kt:127) |
| `${(diff / 60.0).roundToInt()} min ago` | [Machines.kt:128](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Machines.kt:128) |
| `${(diff / 3600.0).roundToInt()} h ago` | [Machines.kt:129](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Machines.kt:129) |
| `yesterday` | [Machines.kt:130](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Machines.kt:130) |
| `$it days ago` | [Machines.kt:130](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Machines.kt:130) |
| `Enter its address` | [Machines.kt:136](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Machines.kt:136) |
| `Looking…` | [Machines.kt:137](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Machines.kt:137) |
| `Ready · ${ms(it)}` | [Machines.kt:138](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Machines.kt:138) |
| `Ready` | [Machines.kt:138](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Machines.kt:138) |
| `In use · $it` | [Machines.kt:139](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Machines.kt:139) |
| `In use` | [Machines.kt:139](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Machines.kt:139) |
| `Key doesn't match` | [Machines.kt:140](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Machines.kt:140) |
| `Needs an update` | [Machines.kt:141](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Machines.kt:141) |
| `Address not found` | [Machines.kt:142](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Machines.kt:142) |
| `Address isn't valid` | [Machines.kt:143](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Machines.kt:143) |
| `Offline · seen ${ago(it)}` | [Machines.kt:144](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Machines.kt:144) |
| `Not responding` | [Machines.kt:144](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Machines.kt:144) |
| `Connect` | [MainActivity.kt:170](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MainActivity.kt:170) |
| `Edit…` | [MainActivity.kt:172](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MainActivity.kt:172) |
| `Copy Address` | [MainActivity.kt:173](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MainActivity.kt:173) |
| `Address` | [MainActivity.kt:174](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MainActivity.kt:174) |
| `Copied ${machine.address}` | [MainActivity.kt:175](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MainActivity.kt:175) |
| `Remove…` | [MainActivity.kt:177](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MainActivity.kt:177) |

### B. Add/edit/remove and link handling — exact copy

| String or template (verbatim) | Source |
|---|---|
| `100.101.102.103, a name, or a sunna:// link` | [MachineSheet.kt:40](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MachineSheet.kt:40) |
| `The key it shows when it starts sharing` | [MachineSheet.kt:41](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MachineSheet.kt:41) |
| `Name (optional)` | [MachineSheet.kt:42](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MachineSheet.kt:42) |
| `Edit “${machine.name}”` | [MachineSheet.kt:57](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MachineSheet.kt:57) |
| `Paste` | [MachineSheet.kt:98](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MachineSheet.kt:98) |
| `Key` | [MachineSheet.kt:103](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MachineSheet.kt:103) |
| `Show key` | [MachineSheet.kt:109](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MachineSheet.kt:109) |
| `Hide key` | [MachineSheet.kt:118](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MachineSheet.kt:118) |
| `Name` | [MachineSheet.kt:123](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MachineSheet.kt:123) |
| `Remove` | [MachineSheet.kt:145](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MachineSheet.kt:145) |
| `Cancel` | [MachineSheet.kt:151](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MachineSheet.kt:151) |
| `Add` | [MachineSheet.kt:152](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MachineSheet.kt:152) |
| `Save` | [MachineSheet.kt:152](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MachineSheet.kt:152) |
| `The key came with the link.` | [MachineSheet.kt:214](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MachineSheet.kt:214) |
| `The clipboard is empty.` | [MachineSheet.kt:221](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MachineSheet.kt:221) |
| `$it (from the computer)` | [MachineSheet.kt:282](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MachineSheet.kt:282) |
| `New Computer` | [MachineSheet.kt:283](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MachineSheet.kt:283) |
| `The key can't contain line breaks.` | [MachineSheet.kt:310](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MachineSheet.kt:310) |
| `That computer is no longer in your list.` | [MachineSheet.kt:317](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MachineSheet.kt:317) |
| `Couldn't save it: the phone's storage may be full.` | [MachineSheet.kt:317](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MachineSheet.kt:317) |
| `Remove “${machine.name}”?` | [MachineSheet.kt:332](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MachineSheet.kt:332) |
| `You can add it again with its address and key.` | [MachineSheet.kt:332](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MachineSheet.kt:332) |
| `Couldn't remove it: the phone's storage may be full.` | [MachineSheet.kt:334](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MachineSheet.kt:334) |
| `Close` | [Sheet.kt:266](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Sheet.kt:266) |
| `Leave the session to add a computer.` | [MainActivity.kt:102](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MainActivity.kt:102) |
| `Enter the computer's address.` | [reach.rs:55](/home/sunny/projects/sunna/crates/client/src/reach.rs:55) |
| `That IPv6 address is missing its closing ']'.` | [reach.rs:60](/home/sunny/projects/sunna/crates/client/src/reach.rs:60) |
| `Unexpected text after the address.` | [reach.rs:64](/home/sunny/projects/sunna/crates/client/src/reach.rs:64) |
| `That doesn't look like an address.` | [reach.rs:75](/home/sunny/projects/sunna/crates/client/src/reach.rs:75) |
| `The port must be a number from 1 to 65535.` | [reach.rs:83](/home/sunny/projects/sunna/crates/client/src/reach.rs:83) |
| `Couldn't find a computer called “{host}”.` | [reach.rs:98](/home/sunny/projects/sunna/crates/client/src/reach.rs:98) |
| `Couldn't find “{host}”.` | [reach.rs:108](/home/sunny/projects/sunna/crates/client/src/reach.rs:108) |
| `Nothing answered at {resolved}. Is Sunna sharing on that computer?` | [reach.rs:122](/home/sunny/projects/sunna/crates/client/src/reach.rs:122) |
| `That computer runs a newer Sunna. Update this one.` | [reach.rs:145](/home/sunny/projects/sunna/crates/client/src/reach.rs:145) |
| `That computer runs an older Sunna. Update it.` | [reach.rs:147](/home/sunny/projects/sunna/crates/client/src/reach.rs:147) |
| `Sunna is there, but the key doesn't match.` | [reach.rs:148](/home/sunny/projects/sunna/crates/client/src/reach.rs:148) |
| `{viewer} is connected to it right now.` | [reach.rs:149](/home/sunny/projects/sunna/crates/client/src/reach.rs:149) |
| `Someone is connected to it right now.` | [reach.rs:150](/home/sunny/projects/sunna/crates/client/src/reach.rs:150) |

### B. Settings, choice controls and licenses — exact copy

| String or template (verbatim) | Source |
|---|---|
| `Back` | [Settings.kt:42](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Settings.kt:42) |
| `This phone` | [Settings.kt:53](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Settings.kt:53) |
| `What computers call this phone while it's connected: “$ownName is connected to it.”` | [Settings.kt:69](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Settings.kt:69) |
| `Sessions` | [Settings.kt:72](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Settings.kt:72) |
| `Touch` | [Settings.kt:77](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Settings.kt:77) |
| `Trackpad` | [Settings.kt:77](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Settings.kt:77) |
| `Trackpad: drag to move the pointer, tap to click. Touch: tap where you want to click.` | [Settings.kt:79](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Settings.kt:79) |
| `Sound` | [Settings.kt:81](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Settings.kt:81) |
| `Play the computer's sound on this phone` | [Settings.kt:81](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Settings.kt:81) |
| `Share the clipboard` | [Settings.kt:82](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Settings.kt:82) |
| `Copy on one, paste on the other, both ways` | [Settings.kt:82](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Settings.kt:82) |
| `Keyboard button` | [Settings.kt:83](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Settings.kt:83) |
| `Next to ••• at the top of sessions, for the keyboard in one tap` | [Settings.kt:83](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Settings.kt:83) |
| `Keyboard suggestions` | [Settings.kt:84](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Settings.kt:84) |
| `Corrections and swipe typing from your keyboard; Sunna retypes what changes` | [Settings.kt:84](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Settings.kt:84) |
| `Stats bar` | [Settings.kt:85](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Settings.kt:85) |
| `Latency, frame rate and bitrate during sessions` | [Settings.kt:85](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Settings.kt:85) |
| `Video` | [Settings.kt:89](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Settings.kt:89) |
| `How sessions start; the session menu changes them for the session you're in.` | [Settings.kt:96](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Settings.kt:96) |
| `About` | [Settings.kt:99](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Settings.kt:99) |
| `Open source software` | [Settings.kt:102](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Settings.kt:102) |
| `The software Sunna is made with, and its licenses` | [Settings.kt:102](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Settings.kt:102) |
| `Sunna $version · protocol ${Native.protocol()}` | [Settings.kt:104](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Settings.kt:104) |
| `Open Source Software` | [Settings.kt:160](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Settings.kt:160) |
| `$title: $label` | [Rows.kt:106](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Rows.kt:106) |
| `Sharpness` | [Rows.kt:128](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Rows.kt:128) |
| `Full` | [Rows.kt:128](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Rows.kt:128) |
| `75%` | [Rows.kt:128](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Rows.kt:128) |
| `50%` | [Rows.kt:128](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Rows.kt:128) |
| `Frame rate` | [Rows.kt:132](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Rows.kt:132) |
| `60` | [Rows.kt:132](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Rows.kt:132) |
| `30` | [Rows.kt:132](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Rows.kt:132) |
| `Bitrate` | [Rows.kt:136](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Rows.kt:136) |
| `Auto` | [Rows.kt:136](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Rows.kt:136) |
| `10` | [Rows.kt:136](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Rows.kt:136) |
| `20` | [Rows.kt:136](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Rows.kt:136) |
| `40` | [Rows.kt:136](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Rows.kt:136) |
| `80` | [Rows.kt:136](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Rows.kt:136) |
| `Codec` | [Rows.kt:140](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Rows.kt:140) |
| `HEVC` | [Rows.kt:140](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Rows.kt:140) |
| `H.264` | [Rows.kt:140](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Rows.kt:140) |

### B. Connecting, session toolbar, menu and notices — exact copy

| String or template (verbatim) | Source |
|---|---|
| `Keyboard` | [Session.kt:137](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:137) |
| `Session menu` | [Session.kt:144](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:144) |
| `Reaching ${machine.name}…` | [Session.kt:207](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:207) |
| `Disconnected` | [Session.kt:279](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:279) |
| `The screen is a trackpad: tap to click, two fingers to scroll.` | [Session.kt:309](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:309) |
| `Tap to click, two fingers to scroll.` | [Session.kt:309](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:309) |
| `The buttons at the top: the keyboard, and ••• for the menu.` | [Session.kt:310](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:310) |
| `Tap ••• for the keyboard and menu.` | [Session.kt:310](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:310) |
| `$how $buttons` | [Session.kt:311](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:311) |
| `The computer kept its video as it was: $streamError` | [Session.kt:357](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:357) |
| `Starting video…` | [Session.kt:366](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:366) |
| `connecting…` | [Session.kt:379](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:379) |
| `Hide the keyboard` | [Session.kt:505](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:505) |
| `Some characters can't be typed: Sunna types on a US keyboard layout.` | [Session.kt:696](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:696) |
| `The clipboard has no text to type.` | [Session.kt:822](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:822) |
| `Typing $typed characters · $skipped skipped (not on a US keyboard)` | [Session.kt:828](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:828) |
| `Typing $typed characters` | [Session.kt:828](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:828) |
| `—` | [Session.kt:884](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:884) |
| `Touch: tap where you want to click. Touch and hold for a right click.` | [SessionMenu.kt:75](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/SessionMenu.kt:75) |
| `Trackpad: drag to move the pointer, tap to click, two fingers to scroll.` | [SessionMenu.kt:75](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/SessionMenu.kt:75) |
| `Sound On` | [SessionMenu.kt:77](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/SessionMenu.kt:77) |
| `Sound Off` | [SessionMenu.kt:77](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/SessionMenu.kt:77) |
| `Stats` | [SessionMenu.kt:81](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/SessionMenu.kt:81) |
| `Shortcuts` | [SessionMenu.kt:87](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/SessionMenu.kt:87) |
| `Next to ••• at the top, for the keyboard in one tap` | [SessionMenu.kt:99](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/SessionMenu.kt:99) |
| `Corrections and swipe typing; Sunna retypes what changes` | [SessionMenu.kt:100](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/SessionMenu.kt:100) |
| `Clipboard` | [SessionMenu.kt:104](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/SessionMenu.kt:104) |
| `Type clipboard text` | [SessionMenu.kt:107](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/SessionMenu.kt:107) |
| `Key by key, for login screens and password prompts` | [SessionMenu.kt:107](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/SessionMenu.kt:107) |
| `Disconnect` | [SessionMenu.kt:122](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/SessionMenu.kt:122) |
| `${codecName(status.optString("codec"))}  ·  $w×$h  ·  $fps fps` | [SessionMenu.kt:141](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/SessionMenu.kt:141) |
| `Switch Apps (⌘Tab)` | [keys.rs:33](/home/sunny/projects/sunna/crates/client/src/keys.rs:33) |
| `Spotlight (⌘Space)` | [keys.rs:37](/home/sunny/projects/sunna/crates/client/src/keys.rs:37) |
| `Mission Control (⌃↑)` | [keys.rs:41](/home/sunny/projects/sunna/crates/client/src/keys.rs:41) |
| `Force Quit… (⌥⌘Esc)` | [keys.rs:45](/home/sunny/projects/sunna/crates/client/src/keys.rs:45) |
| `Lock Screen (⌃⌘Q)` | [keys.rs:49](/home/sunny/projects/sunna/crates/client/src/keys.rs:49) |
| `Escape` | [keys.rs:53](/home/sunny/projects/sunna/crates/client/src/keys.rs:53) |
| `Switch Windows (Alt+Tab)` | [keys.rs:60](/home/sunny/projects/sunna/crates/client/src/keys.rs:60) |
| `App Launcher (Super)` | [keys.rs:64](/home/sunny/projects/sunna/crates/client/src/keys.rs:64) |
| `Terminal (Ctrl+Alt+T)` | [keys.rs:68](/home/sunny/projects/sunna/crates/client/src/keys.rs:68) |
| `Lock Screen (Super+L)` | [keys.rs:72](/home/sunny/projects/sunna/crates/client/src/keys.rs:72) |
| `Log Out… (Ctrl+Alt+Delete)` | [keys.rs:76](/home/sunny/projects/sunna/crates/client/src/keys.rs:76) |

### B. Accessory keyboard and clipboard metadata — exact copy

| String or template (verbatim) | Source |
|---|---|
| `⌃` | [Keys.kt:362](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Keys.kt:362) |
| `⌥` | [Keys.kt:362](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Keys.kt:362) |
| `⌘` | [Keys.kt:362](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Keys.kt:362) |
| `⇧` | [Keys.kt:362](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Keys.kt:362) |
| `Ctrl` | [Keys.kt:364](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Keys.kt:364) |
| `Alt` | [Keys.kt:364](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Keys.kt:364) |
| `Super` | [Keys.kt:364](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Keys.kt:364) |
| `Shift` | [Keys.kt:364](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Keys.kt:364) |
| `esc` | [Keys.kt:376](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Keys.kt:376) |
| `tab` | [Keys.kt:377](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Keys.kt:377) |
| `←` | [Keys.kt:382](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Keys.kt:382) |
| `↑` | [Keys.kt:383](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Keys.kt:383) |
| `↓` | [Keys.kt:384](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Keys.kt:384) |
| `→` | [Keys.kt:385](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Keys.kt:385) |
| `⌦` | [Keys.kt:386](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Keys.kt:386) |
| `Del` | [Keys.kt:386](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Keys.kt:386) |
| `home` | [Keys.kt:387](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Keys.kt:387) |
| `end` | [Keys.kt:388](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Keys.kt:388) |
| `pg↑` | [Keys.kt:389](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Keys.kt:389) |
| `pg↓` | [Keys.kt:390](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Keys.kt:390) |
| `F${i + 1}` | [Keys.kt:391](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Keys.kt:391) |
| `Hide keyboard` | [Keys.kt:401](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Keys.kt:401) |
| `Image` | [Clipboard.kt:92](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Clipboard.kt:92) |
| `clip-${System.currentTimeMillis()}.png` | [Clipboard.kt:159](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Clipboard.kt:159) |

### B. Live stats and decoder suffix — exact copy

| String or template (verbatim) | Source |
|---|---|
| `connecting…` | [lib.rs:188](/home/sunny/projects/sunna/crates/client/src/lib.rs:188) |
| `{} {}×{}` | [lib.rs:191](/home/sunny/projects/sunna/crates/client/src/lib.rs:191) |
| `{} fps` | [lib.rs:192](/home/sunny/projects/sunna/crates/client/src/lib.rs:192) |
| `{:.1} / {:.0} Mbps` | [lib.rs:195](/home/sunny/projects/sunna/crates/client/src/lib.rs:195) |
| `{:.1} Mbps` | [lib.rs:196](/home/sunny/projects/sunna/crates/client/src/lib.rs:196) |
| `latency {p50:.0} ms (p95 {p95:.0})` | [lib.rs:199](/home/sunny/projects/sunna/crates/client/src/lib.rs:199) |
| `encode {:.1} ms` | [lib.rs:202](/home/sunny/projects/sunna/crates/client/src/lib.rs:202) |
| `decode {decode:.1} ms` | [lib.rs:205](/home/sunny/projects/sunna/crates/client/src/lib.rs:205) |
| `rtt {rtt:.0} ms` | [lib.rs:208](/home/sunny/projects/sunna/crates/client/src/lib.rs:208) |
| `lost {}` | [lib.rs:211](/home/sunny/projects/sunna/crates/client/src/lib.rs:211) |
| `sound {} ms` | [lib.rs:214](/home/sunny/projects/sunna/crates/client/src/lib.rs:214) |
| `tiles {}/s` | [lib.rs:217](/home/sunny/projects/sunna/crates/client/src/lib.rs:217) |
| `  ·  software decoder` | [lib.rs:154](/home/sunny/projects/sunna/crates/android/src/lib.rs:154) |
| `  ·  hardware decoder` | [lib.rs:154](/home/sunny/projects/sunna/crates/android/src/lib.rs:154) |

### B. Connection failures, session-ended reasons and notification — exact copy

| String or template (verbatim) | Source |
|---|---|
| `Lost the connection: nothing came back from that computer for 10 seconds. Its network dropped, or it went to sleep.` | [lib.rs:211](/home/sunny/projects/sunna/crates/android/src/lib.rs:211) |
| `That computer ended the session: it restarted, or stopped sharing.` | [lib.rs:213](/home/sunny/projects/sunna/crates/android/src/lib.rs:213) |
| `This session moved to a newer one from this phone.` | [lib.rs:215](/home/sunny/projects/sunna/crates/android/src/lib.rs:215) |
| `The key doesn't match. Edit the computer and paste its key.` | [lib.rs:217](/home/sunny/projects/sunna/crates/android/src/lib.rs:217) |
| `{who} is connected to that computer right now.` | [lib.rs:219](/home/sunny/projects/sunna/crates/android/src/lib.rs:219) |
| `Someone else is connected to that computer.` | [lib.rs:221](/home/sunny/projects/sunna/crates/android/src/lib.rs:221) |
| `That computer runs a different version of Sunna. Update both.` | [lib.rs:223](/home/sunny/projects/sunna/crates/android/src/lib.rs:223) |
| `This phone can't play that computer's video ({message}).` | [lib.rs:225](/home/sunny/projects/sunna/crates/android/src/lib.rs:225) |
| `Couldn't reach {addr}. Is Sunna sharing on that computer?` | [lib.rs:297](/home/sunny/projects/sunna/crates/android/src/lib.rs:297) |
| `Nothing answered at {addr} for 10 seconds. Is Sunna sharing on that computer?` | [lib.rs:299](/home/sunny/projects/sunna/crates/android/src/lib.rs:299) |
| `no answer from the host for 10 seconds` | [lib.rs:523](/home/sunny/projects/sunna/crates/client/src/lib.rs:523) |
| `protocol version mismatch: host {version}, client {}` | [lib.rs:541](/home/sunny/projects/sunna/crates/client/src/lib.rs:541) |
| `host refused the session: {reason}` | [lib.rs:554](/home/sunny/projects/sunna/crates/client/src/lib.rs:554) |
| `expected HelloAck, got {other:?}` | [lib.rs:555](/home/sunny/projects/sunna/crates/client/src/lib.rs:555) |
| `decode thread exited during reconfigure` | [lib.rs:856](/home/sunny/projects/sunna/crates/client/src/lib.rs:856) |
| `decoder reconfigure failed: {error}` | [lib.rs:1054](/home/sunny/projects/sunna/crates/client/src/lib.rs:1054) |
| `a computer` | [SessionService.kt:29](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/SessionService.kt:29) |
| `Sessions` | [SessionService.kt:40](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/SessionService.kt:40) |
| `While you're connected to a computer` | [SessionService.kt:41](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/SessionService.kt:41) |
| `Connected to $name` | [SessionService.kt:48](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/SessionService.kt:48) |
| `Tap to go back to it` | [SessionService.kt:49](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/SessionService.kt:49) |
| `Keeps a remote desktop session connected while the app is in the background` | [AndroidManifest.xml:48](/home/sunny/projects/sunna/apps/android/app/src/main/AndroidManifest.xml:48) |

### B. Dynamic text, resources and system-owned surfaces

- Names, OS/model/device, IP/address, initial phone name, version/protocol and received host error reasons are runtime data. Home system subtitle joins values with ` · `; menu/stats use `  ·  `. Keyboard modifiers use host OS; all key labels are also accessibility labels. Clipboard text clip label is `Sunna`, image `Image`, copied machine address `Address`; actual copied remote text is arbitrary user content. [Machines.kt:110](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Machines.kt:110) [Settings.kt:54](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Settings.kt:54) [SessionMenu.kt:141](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/SessionMenu.kt:141) [Keys.kt:351](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Keys.kt:351) [Clipboard.kt:83](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Clipboard.kt:83) [MainActivity.kt:174](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MainActivity.kt:174)
- Native unknown errors are trimmed and passed through. Initial client `host refused the session: ` prefix is stripped before friendly mapping; later nonempty ApplicationClosed reason passes directly without friendly remapping except “replaced”. A generic error therefore may be raw host text rather than one of B’s fixed sentences. [lib.rs:208](/home/sunny/projects/sunna/crates/android/src/lib.rs:208) [lib.rs:232](/home/sunny/projects/sunna/crates/android/src/lib.rs:232) [lib.rs:326](/home/sunny/projects/sunna/crates/android/src/lib.rs:326)
- `res/values/strings.xml` contains only `Sunna`; app copy is hardcoded in Kotlin/Rust. Vector XML assets are path/color drawings, not additional textual labels; semantic labels come from the consuming views. Android system keyboard, clipboard overlay, notification permission and OS app/task surfaces supply their own locale/version-dependent words. [strings.xml:1](/home/sunny/projects/sunna/apps/android/app/src/main/res/values/strings.xml:1) [Ui.kt:168](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Ui.kt:168) [Keys.kt:90](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Keys.kt:90) [MainActivity.kt:110](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MainActivity.kt:110)
- The complete license asset is user-visible, including dependency versions, legal headings, URLs and copyright notices. Its exact paragraphs appear in the final appendix; no attempt is made to rewrite or summarize that legal copy. [Settings.kt:139](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Settings.kt:139) [licenses.txt:1](/home/sunny/projects/sunna/apps/android/app/src/main/assets/licenses.txt:1)


### B. Additional error fragments and dynamically composed copy

| String/template | Source |
|---|---|
| `tls setup: {0}` | [lib.rs:34](/home/sunny/projects/sunna/crates/transport/src/lib.rs:34) |
| `certificate generation: {0}` | [lib.rs:36](/home/sunny/projects/sunna/crates/transport/src/lib.rs:36) |
| `quic crypto config: {0}` | [lib.rs:38](/home/sunny/projects/sunna/crates/transport/src/lib.rs:38) |
| `io: {0}` | [lib.rs:40](/home/sunny/projects/sunna/crates/transport/src/lib.rs:40) |
| `connect: {0}` | [lib.rs:42](/home/sunny/projects/sunna/crates/transport/src/lib.rs:42) |
| `connection: {0}` | [lib.rs:44](/home/sunny/projects/sunna/crates/transport/src/lib.rs:44) |
| `stream write: {0}` | [lib.rs:46](/home/sunny/projects/sunna/crates/transport/src/lib.rs:46) |
| `stream read: {0}` | [lib.rs:48](/home/sunny/projects/sunna/crates/transport/src/lib.rs:48) |
| `codec: {0}` | [lib.rs:50](/home/sunny/projects/sunna/crates/transport/src/lib.rs:50) |
| `control message too large ({0} bytes)` | [lib.rs:52](/home/sunny/projects/sunna/crates/transport/src/lib.rs:52) |
| `stream closed: {0}` | [lib.rs:56](/home/sunny/projects/sunna/crates/transport/src/lib.rs:56) |
| `host pipeline failed: {error}` | [lib.rs:592](/home/sunny/projects/sunna/crates/host/src/lib.rs:592) |
| `this phone has no {mime} decoder` | [android.rs:231](/home/sunny/projects/sunna/crates/codec/src/android.rs:231) |
| `couldn't start the {mime} decoder (error {last_error})` | [android.rs:281](/home/sunny/projects/sunna/crates/codec/src/android.rs:281) |
| `no Android decoder for {other:?}` | [android.rs:509](/home/sunny/projects/sunna/crates/codec/src/android.rs:509) |
| `${Build.MANUFACTURER.replaceFirstChar { it.uppercase() }} ${Build.MODEL}` | [Session.kt:249](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:249) |

These are upstream fragments, not additional screens. The platform’s friendly mapping may replace or wrap them; unknown embedded library/OS/host error text remains unbounded. Frame-level decode failures may only log/request recovery rather than end the session; the constructor/reconfiguration failures above are the relevant presentation path. [lib.rs:569](/home/sunny/projects/sunna/crates/client/src/lib.rs:569) [lib.rs:1054](/home/sunny/projects/sunna/crates/client/src/lib.rs:1054) [main.rs:407](/home/sunny/projects/sunna/apps/sunna/src/main.rs:407) [lib.rs:208](/home/sunny/projects/sunna/crates/android/src/lib.rs:208)

## C. Complete Android flows

### First run, adding, edits and removal

1. Launch Activity → restore store/preferences → loading summary → first poll → empty No computers yet, or cards. Corrupt/missing store silently becomes empty. Add opens bottom sheet and shows IME; no default key. [MainActivity.kt:55](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MainActivity.kt:55) [Home.kt:475](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Home.kt:475) [Machines.kt:169](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Machines.kt:169)
2. **Address/name:** enter host/IP (optional port), key, optional display name; shared native parse accepts IPv4/name/bracketed or bare IPv6/default48800. Check after450ms (resolve2s, probe1500ms). Preview uses named states; error helper raw detail. Save rejects parse failure/LF key; blank keys, busy/wrong-key/offline/update may be saved. Name precedence custom→host reported→address host. Metadata only copied when check matches form values. Storage errors stay in form. [MachineSheet.kt:239](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MachineSheet.kt:239) [MachineSheet.kt:296](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MachineSheet.kt:296) [Machines.kt:203](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Machines.kt:203) [reach.rs:12](/home/sunny/projects/sunna/crates/client/src/reach.rs:12) [reach.rs:48](/home/sunny/projects/sunna/crates/client/src/reach.rs:48)
3. **By key:** no separate lookup/import; key must accompany address or be supplied in a link. **Paste:** clipboard icon reads/coerces text; empty gives The clipboard is empty.; ordinary text fills address; recognized `sunna://` fills address/key and gives key helper. Name is not encoded by this link parser. [MachineSheet.kt:190](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MachineSheet.kt:190) [MachineSheet.kt:217](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MachineSheet.kt:217) [Machines.kt:102](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Machines.kt:102)
4. **External link/QR:** phone camera/browser recognizes `sunna://` → ACTION_VIEW → Activity opens add sheet prefilled. No in-app camera/scanner and no CAMERA permission. While session exists, link is consumed with Leave the session… notice; it is not queued. Kotlin link recognition requires lowercase prefix; desktop JS is case-insensitive. Malformed links fall back to address/native validation where possible, not a dedicated invalid-link screen. Tailscale discovery is absent; a resolvable Tailscale IP/name can be typed. [AndroidManifest.xml:30](/home/sunny/projects/sunna/apps/android/app/src/main/AndroidManifest.xml:30) [MainActivity.kt:98](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MainActivity.kt:98) [Machines.kt:102](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Machines.kt:102)
5. **Edit/rename:** More/long-press→Edit…, or wrong-key tap→same sheet. Address/key change resets learned metadata and lastSeen; name edit preserves connection values. Save rerenders. **Remove:** menu or edit→confirmation→delete; failure notice/helper, success no undo. No reorder flow; Store.put exists but is not a drag-order UI. [Machines.kt:231](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Machines.kt:231) [Machines.kt:241](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Machines.kt:241) [MainActivity.kt:167](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MainActivity.kt:167) [MachineSheet.kt:332](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MachineSheet.kt:332)

### Connect, cancel, failures and end reasons

1. Ready tap→home pauses/fades→Session + foreground service start→Reaching…; native resolution cancellable; transport has explicit10s timeout. Failure yields Couldn't reach… or Nothing answered…10seconds; resolution errors use shared messages. [MainActivity.kt:130](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MainActivity.kt:130) [Session.kt:207](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:207) [lib.rs:279](/home/sunny/projects/sunna/crates/android/src/lib.rs:279)
2. Transport connected→Starting video… **before** hello/decoder; client hello has10s timeout, rejects token/busy/version/unexpected response, may lack decoder. Auto codec prefers hardware HEVC else H264. Stream size adjusts to decoder support by.9 steps down toward640; unknown host dimension assumes3840×2400. No separate fallback-quality notice. [lib.rs:307](/home/sunny/projects/sunna/crates/android/src/lib.rs:307) [lib.rs:520](/home/sunny/projects/sunna/crates/client/src/lib.rs:520) [Session.kt:207](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:207)
3. First decoded frame→streaming→record lastConnected→reveal picture→show once-only gesture help→request notification permission on API33+. Unlike desktop, no10s first-frame fallback window rule; may stay Starting video… while connection remains up without frames. No Android no-picture/reconnecting notice. [lib.rs:307](/home/sunny/projects/sunna/crates/android/src/lib.rs:307) [Session.kt:296](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:296) [Session.kt:364](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:364)
4. Cancel button/Back before picture→cancel/end silently→reverse tile animation/home; cancellation works through native resolve, transport, starting stages. Late stale callbacks are ignored by session handle/lifecycle. [Session.kt:252](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:252) [Session.kt:279](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:279) [lib.rs:279](/home/sunny/projects/sunna/crates/android/src/lib.rs:279)
5. Established session ends by Disconnect/menu, notification action, Activity destruction, or native reason. User leave is silent; unsolicited end changes phase Disconnected, waits500ms, returns home and shows mapped reason. Timeout10s, host stopped, replacement, named/unnamed busy, bad key, protocol, decoder failures are B; arbitrary host close reason retained. No retry button. [Session.kt:252](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:252) [Session.kt:279](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:279) [SessionService.kt:54](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/SessionService.kt:54) [lib.rs:208](/home/sunny/projects/sunna/crates/android/src/lib.rs:208) [lib.rs:232](/home/sunny/projects/sunna/crates/android/src/lib.rs:232)

### In-session interaction, keyboard, clipboard and audio

| Interaction | Exact behavior | Source |
|---|---|---|
| Trackpad | One finger relative pointer; tap left; hold after min(system long-press timeout,450ms) begins drag; release ends drag; speed-dependent gain1.15…2.85. | [Touch.kt:14](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Touch.kt:14) [Session.kt:505](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:505) |
| Direct Touch | Absolute positioning, tap left, one-finger drag; hold right click. | [Touch.kt:14](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Touch.kt:14) |
| Two fingers | Tap within300ms right-click; movement scroll; momentum if threshold reached, exp(−dt/.33) decay,16ms ticks, stop<40 speed; Mac scroll factor2. | [Touch.kt:14](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Touch.kt:14) [Session.kt:505](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:505) |
| Pinch | Local zoom/pan, trigger spread22dp and >.6 of travel; not forwarded remote application zoom; clamp1…max(2,3/fit), snap reset<1.06. | [Touch.kt:14](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Touch.kt:14) [Pointer.kt:1](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Pointer.kt:1) |
| Three fingers | Tap within300ms toggles keyboard. | [Touch.kt:14](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Touch.kt:14) |
| Keyboard | Toolbar/menu/three-finger shows IME and key strip; typed text synthesized US layout; suggestion edits retype changed suffix. Unsupported character notice once. Physical keys supported, system keys remain local. Modifier tap next / doubletap<400ms lock. | [Keys.kt:90](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Keys.kt:90) [Keys.kt:351](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Keys.kt:351) [Session.kt:660](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:660) |
| Toolbar | Drag horizontally; persists location; keyboard-button switch only hides keyboard shortcut, menu remains. Back hides keyboard first, then opens menu when streaming. | [Session.kt:417](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:417) [Session.kt:737](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:737) |
| Shortcuts | Shared six-item Mac/Linux chord list from B; sends remote key events. Unlike desktop there is no global system-shortcut capture switch. | [SessionMenu.kt:87](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/SessionMenu.kt:87) [keys.rs:31](/home/sunny/projects/sunna/crates/client/src/keys.rs:31) |
| Clipboard sync | Enabled default, opt-out persists; Android only accesses foreground clipboard. New copies since session start, text and images; remote clip changes applied without echo. PNG conversion/downsample4096, max16MiB, unsupported/failed transfers silently skipped. Returning focus sends pending copy. | [Clipboard.kt:1](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Clipboard.kt:1) [Session.kt:792](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:792) |
| Type clipboard | Explicit Type clipboard text reads up to4000 Kotlin chars; empty with period notice, typed/skipped counts; US mapping; no text editor. | [Session.kt:820](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:820) |
| Sound | Default on; menu Sound On/Off persists and updates stream request. No volume control, audio device chooser or permission dialog. Audio initialization failures shared client logs only. | [SessionMenu.kt:77](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/SessionMenu.kt:77) [lib.rs:661](/home/sunny/projects/sunna/crates/client/src/lib.rs:661) |
| Video | Four shared choice rows persist immediately and request stream changes; header updates actual codec/size/fps, rejection emits kept-video notice. No Fast Lane option; native Android callback discards tile updates. | [Rows.kt:126](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Rows.kt:126) [Session.kt:357](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:357) [lib.rs:325](/home/sunny/projects/sunna/crates/android/src/lib.rs:325) |

### Settings: every row, options and defaults

| Row/control | Options/default; effect | Source |
|---|---|---|
| This phone name | Custom text default empty → OS device name/manufacturer+model fallback; used as remote viewer name. Hint is initial ownName, not live updated helper. | [Settings.kt:53](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Settings.kt:53) [Machines.kt:277](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Machines.kt:277) [Session.kt:246](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:246) |
| Touch | Trackpad on phones; Touch if smallestScreenWidthDp≥600; persists. | [Settings.kt:77](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Settings.kt:77) [Machines.kt:263](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Machines.kt:263) |
| Sound | On; remote sound playback. | [Settings.kt:81](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Settings.kt:81) [Machines.kt:263](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Machines.kt:263) |
| Share the clipboard | On; two-way text/image clipboard when Android permits access. | [Settings.kt:82](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Settings.kt:82) [Machines.kt:263](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Machines.kt:263) |
| Keyboard button | On; toolbar keyboard shortcut. | [Settings.kt:83](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Settings.kt:83) [Machines.kt:263](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Machines.kt:263) |
| Keyboard suggestions | Off; IME corrections/swipe mirror. | [Settings.kt:84](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Settings.kt:84) [Machines.kt:263](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Machines.kt:263) |
| Stats bar | Off (desktop default on). | [Settings.kt:85](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Settings.kt:85) [Machines.kt:263](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Machines.kt:263) |
| Sharpness | Full100(default),75%,50%; requested stream dimensions. | [Rows.kt:128](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Rows.kt:128) [Machines.kt:263](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Machines.kt:263) |
| Frame rate | 60(default),30; labels omit fps. | [Rows.kt:132](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Rows.kt:132) [Machines.kt:263](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Machines.kt:263) |
| Bitrate | Auto0(default),10,20,40,80 Mbps; labels omit units. | [Rows.kt:136](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Rows.kt:136) [Machines.kt:263](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Machines.kt:263) |
| Codec | Auto empty(default),HEVC,H.264; Auto hardware preference described above. | [Rows.kt:140](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Rows.kt:140) [Machines.kt:263](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Machines.kt:263) |
| Open source software | Opens A06, no setting. | [Settings.kt:102](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Settings.kt:102) |
| Version | Read-only Sunna/version/protocol. | [Settings.kt:104](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Settings.kt:104) |
| Toolbar position (not Settings row) | Persisted normalized−1…1,default0; changed by dragging. | [Machines.kt:263](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Machines.kt:263) [Session.kt:417](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:417) |

### Permissions, background and notifications

Service starts on connect attempt; notification already says Connected to… before reaching succeeds. On first visible picture Android13+ asks POST_NOTIFICATIONS once. Denial does not disconnect or show explanation; success starts/refreshes service. Background/lock pauses UI input, keeps native session/service; tapping notification returns; Disconnect action leaves. Notification is ongoing low-importance, no badge; not a reconnect/persistence guarantee if process/Activity destroyed. System camera opens links externally; no runtime camera, microphone, screen-capture or Accessibility prompt exists in this Android client. [MainActivity.kt:110](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MainActivity.kt:110) [MainActivity.kt:130](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MainActivity.kt:130) [SessionService.kt:29](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/SessionService.kt:29) [Session.kt:792](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:792) [AndroidManifest.xml:4](/home/sunny/projects/sunna/apps/android/app/src/main/AndroidManifest.xml:4)

Back dispatch order at Activity level is top sheet → Settings → Session. Within Session: open menu closes, then keyboard hides, then unrevealed connection cancels, otherwise opens menu. Menu header initially uses host OS for non-Mac and blank detail for Mac until a nonzero stream width arrives. [MainActivity.kt:205](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MainActivity.kt:205) [Session.kt:753](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:753) [SessionMenu.kt:133](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/SessionMenu.kt:133)

## D. Existing Android motion

| Element | Motion and timing | Source |
|---|---|---|
| Curves | SpringInterpolator damping ratio1−bounce; snappy/gentle bounce0, bouncy.28, zoom.06. Named durations snappy360,bouncy560,gentle640,zoom640ms. outExpo(.16,1,.3,1), easeIn(.5,0,.75,0), easeInOut(.45,0,.55,1). | [Ui.kt:75](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Ui.kt:75) |
| Press | Generic controls scale.96 down120ms snappy/up560bouncy; machine card scale.975/y−2 down140/up560. | [Ui.kt:135](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Ui.kt:135) [Home.kt:120](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Home.kt:120) |
| Home cards/add | New tile alpha0/y14→visible700outExpo; boot stagger120+110i ms; plus rotates90°560bouncy then resets. | [Home.kt:250](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Home.kt:250) [Home.kt:501](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Home.kt:501) |
| Empty No Signal | Two-axis linear mirrored bounces6.1s/4.3s. | [Home.kt:309](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Home.kt:309) |
| Miniature boot |1350ms boot alpha phases0/8/72/100%; progress850 after200ms cubic(.35,.6,.35,1); wallpaper wake900 after950ms, desk600 after1150,bloom1400 after900; state ends2350. | [ScreenView.kt:165](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/ScreenView.kt:165) |
| Miniature wake/sleep | Plain wake900,desk600 after200,bloom1400,end1450; sleep700easeIn toscale.97 ends720; lift640gentle. | [ScreenView.kt:165](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/ScreenView.kt:165) |
| Miniature ambient | Wallpaper30s alternate cosine transform1.02→1.10,x−3→3%,y−2→2.5%; busy cosine pulse2s; checking sweep1.9s; pauses when view detached/home paused. | [ScreenView.kt:99](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/ScreenView.kt:99) [Looks.kt:173](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Looks.kt:173) |
| Status dot | Alpha1↔.35 each600easeInOut. | [Ui.kt:333](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Ui.kt:333) |
| Form invalid | Shake offsets−6,+5,−3,+2,0 at70ms each,total350ms. | [Ui.kt:235](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Ui.kt:235) |
| Sheet | Open bottom translate640zoom,scrim260; close260easeIn toheight+40dp,scrimfade260; drag follows finger; cancelled drag snap560bouncy,scrim200; IME tracks platform insets animation. | [Sheet.kt:143](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Sheet.kt:143) [Sheet.kt:171](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Sheet.kt:171) |
| Machine popup | Alpha0/scale.9→1,560bouncy, anchored pivot. | [Sheet.kt:335](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Sheet.kt:335) |
| Settings | Slide from right640snappy; close right260easeIn. | [Settings.kt:119](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Settings.kt:119) |
| Launch | Rect/radius/bezel interpolate640zoom from tile tofull screen; desk/shade400 after160,content380 after220; shimmer1400easeInOut reverse,36%segment; phase y6/fade320outExpo; reveal320easeInOut. | [Launch.kt:45](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Launch.kt:45) [Launch.kt:134](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Launch.kt:134) [Launch.kt:159](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Launch.kt:159) |
| Return | Content160/shade300/overlay180; reverse tile640zoom,fade last.35 starting.55; missing tile fallback260fade; home entering session450fade. | [Launch.kt:180](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Launch.kt:180) [MainActivity.kt:130](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MainActivity.kt:130) |
| Chrome | First frame fade300; idle after3s opacity.55 over600; wake150; drag follows finger. | [Session.kt:296](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:296) [Session.kt:316](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:316) |
| Keyboard | Keybar alpha200 when shown; hide gone; safe viewport tracks IME insets (system timing). | [Session.kt:171](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:171) [Session.kt:660](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:660) |
| Pointer tap |380ms outExpo ring radius6→24,alpha.55→0. | [Pointer.kt:190](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Pointer.kt:190) |
| Scroll | Inertial16ms tick,exp(−dt/.33) decay; scroll threshold and termination in gesture implementation. | [Touch.kt:14](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Touch.kt:14) |
| Notice | Appear y24/alpha0→1,560bouncy; lifetime4500(default)/6500firsthelp; exit220easeIn to y16 fade. | [Ui.kt:279](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Ui.kt:279) [Session.kt:311](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:311) |
| Reduced motion | No app-specific reduced-motion branch found in custom Motion/ScreenView/Touch drawing loops; platform animator duration scaling may affect ViewPropertyAnimator, not a documented app equivalent of web prefers-reduced-motion. | [Ui.kt:75](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Ui.kt:75) [ScreenView.kt:99](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/ScreenView.kt:99) [Touch.kt:14](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Touch.kt:14) |

## E. Android differences

Android owns session and home in one Activity, uses bottom sheets/plain views, mobile gestures/local pinch, IME key strip, persistent video/touch/keyboard prefs, foreground service and notification. Desktop separates launcher/native viewer and provides window/fullscreen, global shortcut capture, Fast Lane, no-picture notices, Tailscale find, QR output/self-host key. Android QR input is external link handling. Tablet touch default differs from phone trackpad; Android12+ dynamic accent versus fallback accent, otherwise dark palette. [MainActivity.kt:130](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MainActivity.kt:130) [Session.kt:505](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:505) [Settings.kt:72](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Settings.kt:72) [SessionService.kt:29](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/SessionService.kt:29) [viewer.rs:295](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:295) [main.js:699](/home/sunny/projects/sunna/apps/sunna/ui/main.js:699) [Ui.kt:31](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Ui.kt:31)

## F. Android unfinished / inconsistent / silent paths

- Settings claims session-menu changes are for the current session, but menu writes the same persisted Prefs (video, touch, sound, stats, keyboard, clipboard). Actual header may differ after host rejects requested preferences. [Settings.kt:96](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Settings.kt:96) [Rows.kt:126](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Rows.kt:126) [SessionMenu.kt:68](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/SessionMenu.kt:68) [Session.kt:357](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:357)
- Notification says Connected before handshake. Permission is only asked after first frame and never automatically asked again after denial. Background service start failures silently caught. [MainActivity.kt:110](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MainActivity.kt:110) [MainActivity.kt:130](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MainActivity.kt:130) [SessionService.kt:29](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/SessionService.kt:29)
- No missing-picture notice/reconnecting UI, even though the shared client tracks no-picture duration (Android status does not expose that field). Starting video may remain while connection is alive without a frame. Fast Lane tile callback is discarded; no UI toggle. [Session.kt:364](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:364) [lib.rs:131](/home/sunny/projects/sunna/crates/android/src/lib.rs:131) [lib.rs:325](/home/sunny/projects/sunna/crates/android/src/lib.rs:325)
- Same state words mostly match desktop, but edit title includes Edit and curly quotes; Add vs Add Computer; New Computer vs New computer; Remove quote/prompt difference; Copy notice includes address; clipboard empty notice includes final period; typing separator has single-spaced ·; Sharpness/Full vs Resolution/Native; bitrate/frame rate labels omit units. [MachineSheet.kt:57](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MachineSheet.kt:57) [MachineSheet.kt:152](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MachineSheet.kt:152) [MachineSheet.kt:283](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MachineSheet.kt:283) [MainActivity.kt:175](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MainActivity.kt:175) [Session.kt:822](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:822) [Rows.kt:126](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Rows.kt:126) [main.js:568](/home/sunny/projects/sunna/apps/sunna/ui/main.js:568) [viewer.rs:615](/home/sunny/projects/sunna/crates/viewer/src/viewer.rs:615)
- Lowercase-only link parser versus case-insensitive desktop; no Tailscale or Show QR; key validation only LF versus desktop LF+CR. Address changes correctly clear lastSeen unlike desktop. [Machines.kt:102](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Machines.kt:102) [MachineSheet.kt:296](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MachineSheet.kt:296) [Machines.kt:231](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Machines.kt:231) [main.js:514](/home/sunny/projects/sunna/apps/sunna/ui/main.js:514)
- Phone-name helper embeds initial name, not updated typed custom name; neutral form helper can survive ready state; malformed/missing store and missing license asset silently look empty. [Settings.kt:69](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Settings.kt:69) [MachineSheet.kt:289](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MachineSheet.kt:289) [Machines.kt:169](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Machines.kt:169) [Settings.kt:142](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Settings.kt:142)
- Most custom drawing/gesture views use suppressed accessibility lint; text/content descriptions exist on controls, but remote image is not a semantic accessibility tree. Keyboard is US-layout only; unknown/non-Mac shortcut assumptions match desktop. [Home.kt:54](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Home.kt:54) [Session.kt:67](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:67) [Keys.kt:90](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Keys.kt:90) [keys.rs:93](/home/sunny/projects/sunna/crates/client/src/keys.rs:93)

# Cross-surface visual assets and copy ownership

Launcher controls and OS marks are inline SVG symbols; miniature OS desktops are CSS illustrations, not live previews. Android draws equivalents with Looks/ScreenView plus vector badges. Ready wallpaper depends on OS family; desktop/laptop/VM/server affects silhouette/subtitle. OS families include macOS, Windows, Arch, Ubuntu, Fedora, Debian, Mint/Manjaro, NixOS, Pop, generic Linux, unknown. [index.html:10](/home/sunny/projects/sunna/apps/sunna/ui/index.html:10) [main.js:34](/home/sunny/projects/sunna/apps/sunna/ui/main.js:34) [style.css:623](/home/sunny/projects/sunna/apps/sunna/ui/style.css:623) [Looks.kt:17](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Looks.kt:17)

Android vector assets below contain no additional text. The icon name identifies its visual role; use the cited vector and its Kotlin consumer for exact geometry/tint. Unreferenced assets are listed explicitly to preserve redesign coverage.

| Asset | Kotlin/XML references in app (not a separate screen) | Source |
|---|---|---|
| `ic_alert.xml` | [Ui.kt:253](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Ui.kt:253) | [ic_alert.xml:1](/home/sunny/projects/sunna/apps/android/app/src/main/res/drawable/ic_alert.xml:1) |
| `ic_arrow.xml` | [Rows.kt:52](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Rows.kt:52); [MainActivity.kt:170](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MainActivity.kt:170) | [ic_arrow.xml:1](/home/sunny/projects/sunna/apps/android/app/src/main/res/drawable/ic_arrow.xml:1) |
| `ic_back.xml` | [Settings.kt:41](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Settings.kt:41) | [ic_back.xml:1](/home/sunny/projects/sunna/apps/android/app/src/main/res/drawable/ic_back.xml:1) |
| `ic_check.xml` | No direct Kotlin/adaptive-icon reference found; potentially unused | [ic_check.xml:1](/home/sunny/projects/sunna/apps/android/app/src/main/res/drawable/ic_check.xml:1) |
| `ic_close.xml` | [Sheet.kt:265](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Sheet.kt:265); [SessionMenu.kt:55](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/SessionMenu.kt:55) | [ic_close.xml:1](/home/sunny/projects/sunna/apps/android/app/src/main/res/drawable/ic_close.xml:1) |
| `ic_copy.xml` | [MainActivity.kt:173](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MainActivity.kt:173) | [ic_copy.xml:1](/home/sunny/projects/sunna/apps/android/app/src/main/res/drawable/ic_copy.xml:1) |
| `ic_edit.xml` | [MainActivity.kt:172](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MainActivity.kt:172) | [ic_edit.xml:1](/home/sunny/projects/sunna/apps/android/app/src/main/res/drawable/ic_edit.xml:1) |
| `ic_eye.xml` | [MachineSheet.kt:105](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MachineSheet.kt:105); [MachineSheet.kt:117](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MachineSheet.kt:117) | [ic_eye.xml:1](/home/sunny/projects/sunna/apps/android/app/src/main/res/drawable/ic_eye.xml:1) |
| `ic_eye_off.xml` | [MachineSheet.kt:117](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MachineSheet.kt:117) | [ic_eye_off.xml:1](/home/sunny/projects/sunna/apps/android/app/src/main/res/drawable/ic_eye_off.xml:1) |
| `ic_keyboard.xml` | [SessionMenu.kt:68](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/SessionMenu.kt:68); [Session.kt:503](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:503); [Keys.kt:400](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Keys.kt:400) | [ic_keyboard.xml:1](/home/sunny/projects/sunna/apps/android/app/src/main/res/drawable/ic_keyboard.xml:1) |
| `ic_keyboard_hide.xml` | [Session.kt:503](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:503); [Keys.kt:400](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Keys.kt:400) | [ic_keyboard_hide.xml:1](/home/sunny/projects/sunna/apps/android/app/src/main/res/drawable/ic_keyboard_hide.xml:1) |
| `ic_launcher_background.xml` | [ic_launcher.xml:3](/home/sunny/projects/sunna/apps/android/app/src/main/res/mipmap-anydpi-v26/ic_launcher.xml:3) | [ic_launcher_background.xml:1](/home/sunny/projects/sunna/apps/android/app/src/main/res/drawable/ic_launcher_background.xml:1) |
| `ic_launcher_foreground.xml` | [ic_launcher.xml:3](/home/sunny/projects/sunna/apps/android/app/src/main/res/mipmap-anydpi-v26/ic_launcher.xml:3) | [ic_launcher_foreground.xml:1](/home/sunny/projects/sunna/apps/android/app/src/main/res/drawable/ic_launcher_foreground.xml:1) |
| `ic_launcher_monochrome.xml` | [ic_launcher.xml:3](/home/sunny/projects/sunna/apps/android/app/src/main/res/mipmap-anydpi-v26/ic_launcher.xml:3) | [ic_launcher_monochrome.xml:1](/home/sunny/projects/sunna/apps/android/app/src/main/res/drawable/ic_launcher_monochrome.xml:1) |
| `ic_link.xml` | No direct Kotlin/adaptive-icon reference found; potentially unused | [ic_link.xml:1](/home/sunny/projects/sunna/apps/android/app/src/main/res/drawable/ic_link.xml:1) |
| `ic_lock.xml` | [ScreenView.kt:142](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/ScreenView.kt:142) | [ic_lock.xml:1](/home/sunny/projects/sunna/apps/android/app/src/main/res/drawable/ic_lock.xml:1) |
| `ic_more.xml` | [Home.kt:72](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Home.kt:72); [Session.kt:143](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Session.kt:143) | [ic_more.xml:1](/home/sunny/projects/sunna/apps/android/app/src/main/res/drawable/ic_more.xml:1) |
| `ic_notification.xml` | [SessionService.kt:47](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/SessionService.kt:47) | [ic_notification.xml:1](/home/sunny/projects/sunna/apps/android/app/src/main/res/drawable/ic_notification.xml:1) |
| `ic_paste.xml` | [MachineSheet.kt:94](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MachineSheet.kt:94) | [ic_paste.xml:1](/home/sunny/projects/sunna/apps/android/app/src/main/res/drawable/ic_paste.xml:1) |
| `ic_plus.xml` | [Home.kt:189](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Home.kt:189); [Home.kt:421](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Home.kt:421) | [ic_plus.xml:1](/home/sunny/projects/sunna/apps/android/app/src/main/res/drawable/ic_plus.xml:1) |
| `ic_power.xml` | [SessionMenu.kt:122](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/SessionMenu.kt:122); [SessionService.kt:54](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/SessionService.kt:54) | [ic_power.xml:1](/home/sunny/projects/sunna/apps/android/app/src/main/res/drawable/ic_power.xml:1) |
| `ic_settings.xml` | [Home.kt:428](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Home.kt:428) | [ic_settings.xml:1](/home/sunny/projects/sunna/apps/android/app/src/main/res/drawable/ic_settings.xml:1) |
| `ic_shortcut.xml` | No direct Kotlin/adaptive-icon reference found; potentially unused | [ic_shortcut.xml:1](/home/sunny/projects/sunna/apps/android/app/src/main/res/drawable/ic_shortcut.xml:1) |
| `ic_sound.xml` | [SessionMenu.kt:77](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/SessionMenu.kt:77) | [ic_sound.xml:1](/home/sunny/projects/sunna/apps/android/app/src/main/res/drawable/ic_sound.xml:1) |
| `ic_sound_off.xml` | [SessionMenu.kt:77](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/SessionMenu.kt:77) | [ic_sound_off.xml:1](/home/sunny/projects/sunna/apps/android/app/src/main/res/drawable/ic_sound_off.xml:1) |
| `ic_stats.xml` | [SessionMenu.kt:81](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/SessionMenu.kt:81) | [ic_stats.xml:1](/home/sunny/projects/sunna/apps/android/app/src/main/res/drawable/ic_stats.xml:1) |
| `ic_touch.xml` | [SessionMenu.kt:72](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/SessionMenu.kt:72) | [ic_touch.xml:1](/home/sunny/projects/sunna/apps/android/app/src/main/res/drawable/ic_touch.xml:1) |
| `ic_trackpad.xml` | [SessionMenu.kt:72](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/SessionMenu.kt:72) | [ic_trackpad.xml:1](/home/sunny/projects/sunna/apps/android/app/src/main/res/drawable/ic_trackpad.xml:1) |
| `ic_trash.xml` | [MachineSheet.kt:145](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MachineSheet.kt:145); [MainActivity.kt:177](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/MainActivity.kt:177) | [ic_trash.xml:1](/home/sunny/projects/sunna/apps/android/app/src/main/res/drawable/ic_trash.xml:1) |
| `ic_up.xml` | [ScreenView.kt:143](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/ScreenView.kt:143) | [ic_up.xml:1](/home/sunny/projects/sunna/apps/android/app/src/main/res/drawable/ic_up.xml:1) |
| `mark_command.xml` | [Looks.kt:25](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Looks.kt:25) | [mark_command.xml:1](/home/sunny/projects/sunna/apps/android/app/src/main/res/drawable/mark_command.xml:1) |
| `mark_screen.xml` | [Looks.kt:27](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Looks.kt:27) | [mark_screen.xml:1](/home/sunny/projects/sunna/apps/android/app/src/main/res/drawable/mark_screen.xml:1) |
| `mark_tux.xml` | [ScreenView.kt:417](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/ScreenView.kt:417); [Looks.kt:28](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Looks.kt:28); [Ui.kt:360](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Ui.kt:360); [Ui.kt:373](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Ui.kt:373); [Launch.kt:74](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Launch.kt:74) | [mark_tux.xml:1](/home/sunny/projects/sunna/apps/android/app/src/main/res/drawable/mark_tux.xml:1) |
| `mark_window.xml` | [Looks.kt:26](/home/sunny/projects/sunna/apps/android/app/src/main/java/dev/sunna/app/Looks.kt:26) | [mark_window.xml:1](/home/sunny/projects/sunna/apps/android/app/src/main/res/drawable/mark_window.xml:1) |

Android OS launcher/task representation uses app label `Sunna` and adaptive launcher foreground/background/monochrome. App theme is DeviceDefault.NoActionBar, dark background, transparent system bars with light icons, short-edge cutout support; no custom named splash/intro screen is defined. [AndroidManifest.xml:12](/home/sunny/projects/sunna/apps/android/app/src/main/AndroidManifest.xml:12) [themes.xml:4](/home/sunny/projects/sunna/apps/android/app/src/main/res/values/themes.xml:4) [ic_launcher.xml:2](/home/sunny/projects/sunna/apps/android/app/src/main/res/mipmap-anydpi-v26/ic_launcher.xml:2)


# Appendix

The Android "Open Source Software" sheet shows 2,628 licence paragraphs generated by `apps/android/licenses.py`. They are not reproduced here.
