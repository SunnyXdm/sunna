# 15 · Linux: reach the machine, not just a session

Status: design and work brief, 2026-11-18.

## Why

Two failures on real machines:

1. **A rebooted desktop can't be reached.** In desktop mode the host is a systemd *user* service that starts with the graphical session. After a reboot the computer sits at its login screen, no session exists, and nothing is listening. Only the background-desktop mode comes back by itself, and that shares a separate desktop, not the machine's real one.
2. **A host that can't capture dies instead of saying so.** On the project's own VM, after a reboot the desktop packages were missing; `sunnad` exited with "connecting to the X server … Connection refused" and systemd restarted it 813 times over 70 minutes. From the app it was simply offline, with no reason.

## What "done" looks like

- A Linux computer set up for whole-machine sharing is reachable from boot: the app shows its **login screen**, the person types their password into it, and lands on their desktop, in one session if at all possible, otherwise with an automatic reconnect within a few seconds. Logging out returns to the login screen. Switching users follows the screen that is actually showing. Locking shows the lock screen.
- A host that is running but has nothing it can share stays up, keeps retrying quietly, and answers a probe with why: "Its desktop isn't running", "Nobody is logged in and the login screen can't be shared on this system", "Waiting for permission on that computer". The app shows that instead of "offline" (the app side is research/14).

## Design

### One listener for the machine, workers for sessions

A **system** service (`sunna-host`, system unit, starts at boot, `Restart=always`) owns the port, the key and the connection to viewers for as long as the machine is up. It never touches a display itself. It asks logind (D-Bus `org.freedesktop.login1`) which session is active on the seat: its class (`greeter` or `user`), type (`x11`, `wayland`), user and display. For that session it runs a **session worker** as that session's user, inside that session's environment, which does capture, input, sound and clipboard and talks to the system service over a local socket. When the active session changes, the worker is swapped. A connected viewer stays connected across the swap when the pipeline allows it (a short gap, then the new screen, re-announcing size); if it can't, the service ends the stream with a "session changed" reason that the viewer treats as "reconnect now", and the viewer reconnects by itself.

Read `crates/host`, `apps/sunnad` and the Windows host's console-session launcher (uncommitted, `apps/sunnad/src/console_session.rs`, `service.rs`: the same problem solved for Windows lock, login and UAC screens) before deciding how much of this can be shared. If a simpler structure meets the "done" list, build that and say why.

### Finding and entering a session

- **X11** (user sessions, and the LightDM, SDDM-X11 and GDM-X11 greeters): the display and its authority file come from the session's X server command line (`-auth …`) or the session leader's environment, read as root. The worker runs as the session's user (the greeter's own user for a greeter) with `DISPLAY` and `XAUTHORITY` set.
- **Wayland user sessions** (GNOME, KDE): the worker runs as the user inside the session (`XDG_RUNTIME_DIR`, the session bus) and uses the existing portal path with its stored approval (`research/10-gnome-wayland.md`). The one-time approval still happens at the computer.
- **Wayland greeters** (GDM by default on Ubuntu and Fedora): there is no portal. Investigate driving Mutter's own `org.gnome.Mutter.RemoteDesktop` and `ScreenCast` D-Bus interfaces from a worker running as the greeter user on the greeter's session bus, which is how GNOME's own remote login reaches it. Prove or disprove it on GNOME 46, 49 and 50 with the existing test bed (`tools/gnome-test`). If it can't be done safely, say so plainly, make the host answer "the login screen can't be shared on this system" there, and document the choices the owner has (a different login manager, or automatic login). Do not change the machine's login manager configuration from the installer.

### Security (this runs as root; be strict)

- The system service executes only root-owned code from a root-owned, non-user-writable install (`/opt/sunna`), with its key and settings in `/etc/sunna` (0600 root). The updater already refuses roots another user can write; use it for this install with the same supervisor, health check and rollback, and prove that.
- Workers drop to the session user before touching anything of that session, with a clean environment built by the service, never one inherited from a user process. A user must not be able to make the service run their code as root or as another user: treat every path, socket and environment value that a session controls as hostile.
- The login screen is the system's own; Sunna never sees or stores the password beyond relaying keystrokes like any other.
- Sharing the login screen means anyone with the key can try passwords at it. Say so in setup and in the docs.

### Setup

- `sunna-host setup --system` (asks for `sudo` once, says exactly what it will install and where) and the installer's `host --system`. `sunna-host status`, `link`, `logs`, `update`, `uninstall` understand the system install. An existing per-user desktop-mode host on the same machine is migrated (same key, so computers already added keep working) and its user unit removed.
- The per-user modes stay as they are for people without administrator rights.
- `sunna-host status --json` is being added by another agent in the same file for the app; re-read the file right before each edit and keep that subcommand working, extended with `managed_by: "system"` and `shows_login_screen`.

### A host with nothing to share

In every mode, `sunnad` stays up when capture or input can't start or goes away. It retries with backoff, and answers probes with `not-sharing` and a short reason (an optional addition to the probe reply that older apps ignore; protocol 8 and positional postcard encoding must keep working both ways, as the update work did). `crates/client`'s check result gains `state: "not-sharing"` with `detail`. The update health check must still refuse to call a release healthy when its backends are broken *because of the release* (see research/13 A5): distinguish "this build can't capture" from "there is no desktop right now".

## Files

`crates/host`, `crates/proto`, `crates/client` (the probe addition only), `crates/capture`, `crates/input`, `crates/audio`, `crates/clipboard` (Linux parts), `apps/sunnad`, `scripts/sunna-host`, `scripts/linux-desktop.sh`, `scripts/wayland-desktop.sh`, the Linux installer and updater pieces under `scripts/release` and `crates/update` that a system install needs, a new test bed under `tools/`, and `docs/host-linux.md`, `docs/security.md`, `docs/troubleshooting.md`. Not `apps/sunna` (another agent is building the app's sharing switch there against research/14), not `apps/android`, not macOS or Windows code beyond reading it.

The tree holds a lot of uncommitted work by others: no commit, stash, reset, checkout or reformatting of files you aren't changing. Nothing is pushed or published. On this machine: don't touch the running `sunna-host.service`, `sunna-desktop.service`, the `newt` and `sunna-web` containers, displays `:1`, `:12`–`:27`, `:40`, or other people's containers; no `pkill -f`; never print keys.

## Proving it

A reusable test bed (`tools/login-test`), in privileged systemd containers or small VMs, each with a real login manager on a virtual display: LightDM (Xfce), SDDM on X11 (Plasma), GDM on X11, GDM on Wayland (GNOME 46 and the newest available). In each, driven by a real Sunna client that saves frames:

1. Boot, no login: connect, the frame is the login screen.
2. Type the password through Sunna: the desktop appears, in the same session or after an automatic reconnect; measure the gap.
3. Lock, unlock, log out (login screen again), log in as a second user, switch users.
4. Reboot the machine with nobody touching it: reachable again; measure time from boot to first frame.
5. Kill the session worker, the system service, and the display server in turn: reachable again without help, and while it isn't, the probe says why.
6. Remove the desktop packages (the VM's real failure): the host stays up, the probe says "Its desktop isn't running", and it recovers by itself when they return.
7. A local user without administrator rights cannot read the key, replace anything the service runs, or get code run as root through any path, socket or environment value they control: write the attempts down and show they fail.
8. Update the system install A → B and a broken B that rolls back, with the existing proof scripts adapted.
9. Old app → new host and new app → old host still probe and connect.

Keep the saved frames and a contact sheet per login manager. Report what works on which login manager, with evidence, and exactly what doesn't.
