# Build brief: the new Computers screen in the desktop app

You are building the real thing in `apps/sunna` (Tauri: Rust in `src/`, web UI in `ui/`). The look has been designed and approved as a prototype; your job is to make it the app, with every existing state still working.

## What exists

- **The prototype of the look:** `apps/sunna/dev/design/proto/looks.html` (one file; served at http://100.108.56.3:8790/looks.html). Five views of the Computers screen, switchable: **Devices, Wall, Space, Type, Gallery**. Read it fully. Its sizes, colours, type, springs, layout rules and the connect animation are deliberate: port them, don't reinterpret them. It also has phone forms; ignore those (Android comes later).
- **Every screen and state the app has today:** `apps/sunna/dev/design/inventory.md`, launcher sections L01–L10 (76 states, 204 strings, with file:line). Nothing in it may be lost.
- **The current UI:** `apps/sunna/ui/{index.html,main.js,motion.js,style.css}`, backed by `apps/sunna/src/{main.rs,machines.rs,settings.rs,discovery.rs}`.
- **Tools for looking at it:** `apps/sunna/dev/mock.js` (serves the UI with fake computers) and `apps/sunna/dev/cdp.mjs` (headless Chrome: screenshots and recordings; `google-chrome` on this box is a headless-shell wrapper). Real Linux desktop screenshots for mock data: `apps/sunna/dev/design/frames/`.
- `apps/sunna/dev/design/guide.md` is an older brief whose *look* was rejected. Don't follow its look. Its rules on words (section 9) still hold.

## What to build

### 1. Five views of the Computers screen, and a setting to choose

- The home screen renders in one of five views, exactly as in the prototype: Devices, Wall, Space, Type, Gallery.
- **Settings → a "View" choice** with the five options, each with a small drawn preview, applied at once. Persist it in the app's settings (`settings.rs`), default **Devices**.
- Also a quick switch on the home screen itself (a small view menu next to + and Settings) and Cmd/Ctrl+1…5.
- Every view must handle 0, 1, 2, 6 and 20 computers, long names, and the window's minimum size (680×480) up to full screen. With no computers, each view shows the add-a-computer invitation in its own style. Views that can overflow scroll.
- Every per-computer state in the inventory must show correctly in every view: ready with latency, in use (with who, when known), offline with last seen, key doesn't match, never connected, checking, and so on. Every per-computer action that exists today (connect, edit, remove, copy link, fix key, whatever the inventory lists) must be reachable in every view, by mouse and by keyboard.

### 2. Real pictures on the screens

Today the tiles are drawn stand-ins. The new look needs each computer's real screen.

- **Last picture.** The viewer saves a small picture of the session (JPEG, at most 1280 px wide) when a session ends and about every 30 s while it runs, in the app's data folder, keyed by the computer. The launcher shows it. Find the cleanest place in the viewer (`crates/viewer`) to read a frame back; it must work in the Linux viewer and be written for the Mac viewer too. macOS can only be type-checked on this box: `export PATH=$HOME/.cargo/bin:$PATH; CC_aarch64_apple_darwin=$HOME/.local/bin/zcc-aarch64-macos CARGO_INCREMENTAL=0 cargo check -p <crate> --target aarch64-apple-darwin`. Say plainly what you could not run.
- **No picture yet** (never connected): show the existing drawn stand-in for that OS from today's UI, so first run isn't a wall of blanks.
- **Offline:** the last picture, dimmed and desaturated, as in the prototype. Removing a computer deletes its picture.
- Pictures are private: they stay on this device, in the app's data folder, file mode 0600.
- The prototype flips between two frames to look live. The real app doesn't; it shows the one saved picture.

### 3. What kind of computer it is

- The host already reports `device`: laptop, desktop, vm, server or empty (`apps/sunnad/src/about.rs`, `HostAbout` in `crates/proto`). Add **mini** (SMBIOS chassis types 35 and 36 on Linux and Windows; a Mac whose model is "Mac mini" or "Mac Studio"). It is only a new string value: don't change the wire format, and make sure an older app that receives "mini" still shows something sensible.
- In the **Devices** view draw each kind differently: laptop (as in the prototype), desktop (monitor on a stand), mini (monitor with a small box beside the stand), server and vm (no stand: these have no physical screen; a floating panel over a slim rack unit), unknown (plain monitor). The screen's shape always comes from the reported width and height, so an ultrawide looks ultrawide.
- **Edit a computer → Kind:** Automatic, Laptop, Desktop, Mini PC, Server, Virtual machine. Stored per computer on this device; overrides what the host says.

### 4. Connecting, and everything else

- Connecting keeps today's behaviour and states (see inventory L-sections for connecting, cancel, and each failure). The transition is the prototype's: the chosen screen grows to fill the window with the spring, the status sits over it, and the native viewer window takes over when the session is up. On failure or cancel it shrinks back to where it came from and the reason shows on that computer.
- Add, Edit, Find on Tailscale, Show QR code, Settings, Licences, toasts, menus and every dialog: keep their content and behaviour, restyle them in the new visual language (the prototype's type, colours, buttons, glass pills) so nothing looks left over from the old app. Space is a light view: sheets and menus opened from it must still look right.
- Bundle Inter (`proto/fonts/InterVariable.woff2`, SIL OFL) in the app and add it to the licences list. No network fonts.
- Dragging the window by its top area, the traffic-light area on Mac, and text selection rules must keep working. Try dragging; don't assume.

## Constraints

- **The app's page security forbids inline styles and inline scripts** (`tauri.conf.json`: `style-src 'self'; script-src 'self'`). The prototype builds HTML strings with `style="…"`; in the app set styles through the DOM instead. Don't loosen the policy.
- **It runs in WebKit, not Chrome** (WebKitGTK on Linux, WKWebView on Mac). Check `linear()` easings, `backdrop-filter`, masks, `aspect-ratio`, and 3D (`preserve-3d` has misplaced layers in WebKit before). Give a fallback where a feature is missing. The prototype's Devices reflection is a mirrored copy of the device, not `-webkit-box-reflect`; keep it that way.
- No vendor OS logos (Apple, Windows, distro marks). The existing own marks (⌘, the penguin, a plain window) stay.
- Motion: springs as in the prototype, no glow sweeps, streaks or flashes. Respect reduced motion.
- Words: plain, verified, sentence case as in today's app. Don't invent features in labels.
- Another agent is working in this same checkout at the same time on installing and updating (`crates/update`, `scripts/`, `.github/`, `apps/sunnad` update code, `apps/android`, docs for install). **Stay out of those.** Your files: `apps/sunna/**`, `crates/viewer/**`, the device-kind lines in `apps/sunnad/src/about.rs`, `docs/using.md` and `docs/images/`. If you must touch a shared file (`Cargo.toml`, `Cargo.lock`, `apps/sunna/src/main.rs`), re-read it immediately before editing and change only your lines. Leave a clear place in Settings for an "Updates" section but don't build it.
- The tree has a lot of uncommitted work that isn't yours. Don't commit, stash, reset, checkout or reformat files you aren't changing. No workspace-wide `cargo fmt`.
- This box: displays `:1`, `:12`–`:27` and `:40` belong to others; use a fresh one (e.g. `:60`). Don't use `pkill -f`. Don't touch other containers or services, including `sunna-host.service`.

## Proving it

1. With the mock and `cdp.mjs`: a screenshot of every view × {0, 1, 6, 20 computers} × {default size, minimum size}, every per-computer state in every view, and every other screen. Recordings of connect, cancel, failure and leave in each view, and of switching views. Put contact sheets in `apps/sunna/dev/design/shots-app/` and look at them yourself before reporting; fix what looks wrong.
2. Run the real Tauri app on this Linux box under Xvfb (WebKitGTK) and screenshot each view and a connect to the local test host (the stopped container `sunna-shot` is a Sunna test host you may start: `docker start sunna-shot`, it listens on 100.108.56.3:48850; its key is in the container's environment, don't print it in your report; stop it again when done). WebKit differences are the point of this step.
3. `scripts/check.sh` (or the relevant cargo checks and tests) passes.
4. Report: what you verified and how, with file paths to the sheets; what you could not verify (the Mac app, for one); anything in the inventory you changed or could not keep; open questions.
