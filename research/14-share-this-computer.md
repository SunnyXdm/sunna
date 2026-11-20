# 14 · "Share this computer" in the app

Status: design and work brief, 2026-11-18.

## Why

Sharing a computer still needs a terminal: `sunna-host setup` on Linux, a Terminal window running `scripts/run.sh host` on a Mac. The app's Settings only says "Not sharing" and names a command. Parsec's model is the right one: one app, and sharing is a switch.

## What the person sees

In Settings, where "This computer" is today:

- **Share this computer** (a switch). Off by default.
- Turning it on:
  1. If the host isn't installed on this machine, the app installs it (the signed host component, through the updater that already exists) and says so while it does.
  2. It sets the host up to share **the desktop the person is sitting at**, to start by itself, and to keep running when the app is closed.
  3. Anything the system must ask (GNOME's "allow remote desktop" dialog on Wayland; on a Mac, Screen Recording and Accessibility) is explained in one sentence *before* it appears, and the switch shows "Waiting for permission…" until it's given.
  4. Then: "Sharing", how it can be reached ("on your Tailscale network" or "on this Wi-Fi/network", with the address), who is connected if anyone, and the existing **Copy link** and **Show QR code**.
- Turning it off stops the host and stops it starting by itself. Someone connected is told first ("Priya's MacBook is connected. Stop sharing anyway?").
- Closing the app does not stop sharing. Quitting the app quits the app only.
- A host that was set up from the command line is the same host: the switch shows it as on and can turn it off. The switch never creates a second one.
- When sharing can't work, the switch says why and the one thing to do, in plain words: the background-desktop mode needs system packages (show the exact command for this distribution, with a Copy button; the app never runs `sudo`); a Wayland desktop that isn't GNOME or KDE; the port is taken; the installer couldn't be reached.

Tailscale is not required. With Tailscale running the host listens there; otherwise on the local network, and the app says which (the existing `SUNNA_LISTEN` logic in `scripts/sunna-host`). The security note about untrusted networks in `docs/security.md` stays true and is linked from the switch.

## The contract between the UI and each platform

Tauri commands, in `apps/sunna/src/sharing.rs`, with one module per platform:

```
sharing_status() -> Sharing
sharing_set(on: bool) -> Sharing        // long-running; emits "sharing-changed" events with a Sharing as it progresses
sharing_open_permission(which: String)   // macOS only: opens the right Settings pane

Sharing {
  supported: bool,            // false: this build or desktop can't share; `reason` says why
  state: "off" | "installing" | "starting" | "needs-permission" | "on" | "error",
  managed_by: "app" | "cli" | "system" | null,   // who set it up
  mode: "desktop" | "virtual" | null,
  reach: "tailscale" | "lan" | null,
  address: string | null,
  link: string | null,        // sunna://… ; never logged
  viewer: string | null,      // who is connected now
  permissions: [{ id: "screen" | "input" | "portal", granted: bool, label: string }],
  reason: string | null,      // one plain sentence when state is error or supported is false
  fix: { label: string, command: string | null } | null,   // e.g. a command to copy
  starts_by_itself: bool,
  shows_login_screen: bool    // see research/15
}
```

The Linux module is built in this work. The macOS module is built separately (research/16) against exactly this struct; here, leave `sharing/macos.rs` as a stub that returns `supported: false` with a reason, and do not write Mac host code.

## Linux, concretely

- The host and the app are separate components. "Installed" means the managed host exists (`~/.local/share/sunna/host/current`) or a from-source or package host is on `PATH`. Install through the updater's own install path for the `host` component; never by downloading a script and running it.
- Set up with the equivalent of `sunna-host setup --desktop` (it already chooses X11 or the GNOME/KDE portal, the listen address and the systemd user unit). Read state from the host itself: add a machine-readable `sunna-host status --json` (the only change this work makes to `scripts/sunna-host`; another agent is working in that file, so re-read it right before editing and add only that).
- A source checkout or a `.deb` host is `managed_by: "cli"`: show its state, allow off and on, don't reinstall it.
- A system-wide host that also shares the login screen (research/15) is `managed_by: "system"`: show it, explain that turning it off needs an administrator, and give the command.

## A computer that is running but can't share

Another agent is making the host stay up when it has nothing to share and say why in its probe reply (research/15). In the app, a probe result with `state: "not-sharing"` and a `detail` sentence must show, in every view, as "Not sharing" with the detail where failure reasons already go, and must not be drawn as offline. Build that against mock data; the real field arrives with the other work.

## Constraints

- Match the app's current look exactly (see `apps/sunna/ui`, `apps/sunna/dev/design/shots-app/`). No new colours, banners or badges. Space is a light view: check the Settings sheet there too. Words plain and true; read the code before writing a label.
- The page security forbids inline styles and scripts. The app runs in WebKit.
- Files: `apps/sunna/**` and the single `status --json` addition in `scripts/sunna-host`, plus `docs/using.md`, `docs/app-linux.md`, `docs/host-linux.md` (the sharing-from-the-app parts only). Not `crates/host`, `crates/proto`, `crates/update`, `apps/sunnad`, `apps/android`, or anything macOS.
- The tree holds a lot of uncommitted work by others: no commit, stash, reset, checkout or reformatting of files you aren't changing. Nothing is pushed or published.

## Proving it

1. In clean containers with a desktop (X11 with Xfce; GNOME on Wayland through `tools/gnome-test`): from a machine with only the app, turn the switch on, connect from a second viewer, close the app, connect again, reboot the container's session, connect again, turn it off, confirm nothing is left running or enabled. Use a local release feed and the throwaway test key for the host install (commands in `docs/update-verification.md`).
2. The same switch against a host set up by the command line, and against a from-source host.
3. Each failure case above produces its sentence and its fix.
4. Mock-driven screenshots of every state of the switch in a dark view and in Space, at 980×660 and 680×480, and of "Not sharing" with a reason in all five views; then the real app under Xvfb (WebKitGTK). Contact sheets in `apps/sunna/dev/design/shots-app/sharing-*.png`; look at them at full size and fix what looks wrong.
5. `apps/sunna/dev` verification scripts still pass, with new ones for the switch.
