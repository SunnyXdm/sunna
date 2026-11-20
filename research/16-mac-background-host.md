# 16 · Mac: a host that lives in the menu bar

Status: design and work brief, 2026-11-18. None of this can be run on the Linux box this is written on; see "Proving it".

## Why

Sharing a Mac today means a Terminal window running `scripts/run.sh host`, built from source, listening only on Tailscale, gone when the window closes or the Mac restarts. It should be: open Sunna, turn on **Share this Mac**, close the window. A small icon in the menu bar shows it's sharing. Nothing heavy stays in memory.

## Decisions

1. **One thing to install on a Mac: Sunna.app.** No separate host package. A Mac you only want to reach has sharing turned on and the window closed. (`sunna host …` stays available from a terminal for scripts.)
2. **The host is the app's own executable in another mode**, started as `Sunna.app/Contents/MacOS/sunna host` (the way the viewer already runs as `sunna viewer`). One code identity (`dev.sunna.app`), so macOS shows one "Sunna" under Screen Recording and under Accessibility, and the permission the app already has for sending shortcuts is the same one the host uses for input. No second binary with its own permission rows.
3. **The menu bar icon belongs to the host process, not the window.** The host runs as an accessory (no Dock icon) and owns an `NSStatusItem`. Closing the app's window quits the app process completely: no web view left in memory. A Mac that isn't sharing has nothing of Sunna running when the window is closed.
4. **launchd keeps it running.** A per-user LaunchAgent (`~/Library/LaunchAgents/dev.sunna.host.plist`): starts at login, restarted if it dies, limited to the graphical session. Installed and removed by the switch. It points at the app's current location; if the app is moved or updated the agent is rewritten and the host restarted when nobody is connected.
5. **Tailscale is optional.** Listen on the Tailscale address when there is one, otherwise on the local network, the same rule as the Linux host; the app and the menu say which.
6. **Idle means idle.** With nobody connected: no capture, no encoder, no audio tap, no polling timers; the process waits on its socket and on status-item events. State this as a measured number (below).

## The menu

```
● Sharing this Mac                  (or: ○ Not sharing — <reason>)
  on your Tailscale network · 100.x.y.z
  Priya's MacBook is connected      (or: Nobody is connected)
  ─────────────
  Open Sunna
  Copy Link
  Stop Sharing                      (asks first if someone is connected)
  ─────────────
  Quit Sunna Host                   (stops sharing until next login or until the switch is turned on again: say so)
```

A template image that follows the menu bar's light and dark appearance, with a filled variant while someone is connected. No vendor marks.

## Permissions

- Screen Recording (the picture and system sound) and Accessibility (mouse and keyboard). The switch in the app explains each in a sentence, opens the right Settings pane, and watches for the grant; the host starts sharing as soon as both are there, without a restart where macOS allows it.
- The app is signed on each Mac with that Mac's own "Sunna Local Code Signing" certificate (`scripts/install-mac-app.sh`, `scripts/release/sign-mac.sh`), which is what keeps grants across updates. Everything here must keep that working: same identifier, same designated requirement before and after an update. Read the notes in `scripts/install-mac-app.sh` about stale grants and build the same checks into the switch ("macOS lists Sunna as allowed but it isn't: remove it from the list and add it again", with the pane opened).
- **Unknown, to be found out on a real Mac, not assumed:** whether current macOS re-asks periodically for screen recording by an app without Apple's persistent-capture entitlement, and what happens to an unattended host when it does. Write the host so that losing the permission is a state it reports ("Waiting for permission on that Mac", through the probe's not-sharing reason from research/15 when that lands, and in the menu), never a crash or a silent black picture.
- Before anyone logs in after a restart, a LaunchAgent does not run, and with FileVault nothing does. Out of scope here; say it in the docs.

## The app side

The switch, its states and the Tauri contract are defined in `research/14-share-this-computer.md` and are being built for Linux by another agent at the same time. Implement the macOS half of exactly that contract in `apps/sunna/src/sharing/macos.rs` (the other agent leaves a stub there; coordinate through the file, re-reading it before every edit, and do not change the shared struct or the UI). `managed_by` is "app" for the LaunchAgent, "cli" for a host someone started in a terminal.

Updating: the app's updater replaces the bundle; afterwards the host is restarted onto the new executable when nobody is connected, with the same start-count and health rules the update work established (research/13 A1–A5). If a session is active the restart waits.

Sleep: while a viewer is connected, hold a power assertion so the Mac doesn't idle-sleep mid-session; release it on disconnect. Do not keep the Mac awake merely because sharing is on.

## Files

The macOS parts of `apps/sunnad` and of the host, capture, input and audio crates; `apps/sunna/src` (a `host` mode entry, `sharing/macos.rs`); `apps/sunna/tauri.conf.json` and bundle resources if needed; `scripts/install-mac-app.sh`, `scripts/run.sh`, `scripts/release/build-macos.sh`, `scripts/release/sign-mac.sh`; `docs/host-mac.md`, `docs/app-mac.md`, `docs/troubleshooting.md`. Not the UI (`apps/sunna/ui`), not Linux, Windows or Android code, not `crates/update` beyond what restarting the host after an app update needs.

The tree holds a lot of uncommitted work by others: no commit, stash, reset, checkout or reformatting of files you aren't changing. Nothing is pushed or published. Never print keys.

## Proving it

On this Linux box only this is possible, and all of it is required:

- `export PATH=$HOME/.cargo/bin:$PATH; CC_aarch64_apple_darwin=$HOME/.local/bin/zcc-aarch64-macos CARGO_INCREMENTAL=0 cargo check --target aarch64-apple-darwin` for every crate touched (say which could not be cross-checked and why), and the Linux build and tests still pass.
- Unit tests for everything that isn't an AppKit call: the LaunchAgent plist (golden file), the state machine of the switch, listen-address choice, menu contents for each state, restart-after-update decisions.
- A reading of Apple's current documentation for each API and behaviour relied on (status items from an accessory process, `launchctl bootstrap`/`bootout` in the GUI domain, screen-capture permission checks and prompts, power assertions), with links, and a list of every assumption that only a real Mac can confirm.

And a **test script for a person at a Mac**, `docs/dev/mac-host-test.md`: numbered steps with what should be seen at each, starting from `scripts/install-mac-app.sh`, covering first-time permissions, close the window (Activity Monitor: which processes remain, memory and CPU with nobody connected; write down the numbers), connect from another device, disconnect (numbers return to idle), restart the Mac and log in, update the app, turn sharing off (nothing left running, no LaunchAgent), and the "listed as allowed but isn't" recovery. Each step says what to send back if it goes wrong (`log show` predicate, the host's log path). Say plainly in the final report that none of the Mac behaviour has been run.
