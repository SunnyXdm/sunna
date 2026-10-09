# Sharing a GNOME desktop on Wayland

Status (2026-10-09): designed, and the whole path proven by hand on GNOME 51 (below); not in `sunnad` yet. This is how it will work, what was verified and how, and what's still open.

## Why it needs its own path

On X11, any program can read the screen and type into other programs, which is what Sunna's Linux host does: MIT-SHM `GetImage` for the picture, XTEST for input, XFixes for the pointer's shape. Wayland forbids all three on purpose: a program sees only its own windows. The sanctioned way in is the **desktop portal** (`org.freedesktop.portal.RemoteDesktop` with `ScreenCast`): the program asks, the desktop asks the user, and then hands over the screen as a PipeWire video stream and takes pointer and keyboard input through the same session.

GNOME is the target because Ubuntu's default desktop is GNOME on Wayland, and from Ubuntu 25.10 there's no Xorg session to fall back to ([announcement](https://discourse.ubuntu.com/t/ubuntu-25-10-drops-support-for-gnome-on-xorg/62538)).

## What the user will see

1. **Setup, once, at the computer.** `sunna-host setup --desktop` in a terminal on the GNOME desktop. GNOME shows its **Remote Desktop** dialog:
   - **Allow Remote Interaction**: off by default. It must be switched on, or Sunna gets the picture without mouse and keyboard. The terminal says so before the dialog appears.
   - **Remember This Selection**: on by default (Sunna asks for it). This is what lets later connections start without asking.
   - Which screen to share, then **Share**.
2. **After that, connecting just works.** Each session starts silently with the remembered permission; nobody needs to be at the computer.
3. **While someone is connected**, GNOME shows its orange sharing indicator in the top bar. Choosing **Stop** there ends the session; the viewer is told it was stopped on the computer.
4. **If the permission is gone** (forgotten by GNOME, or the screens changed so it no longer applies), the next connection shows the dialog on the computer again and the viewer is told to approve it there. `sunna-host setup --desktop` asks again on purpose.
5. Like X11 desktop mode, it shares the desktop of whoever is logged in, so someone has to be logged in. It doesn't share the login screen (that would need GNOME's own Remote Login, a different system).

## How it works

### One portal session per Sunna session

When a viewer connects, the host starts a portal session and closes it when the viewer leaves, so GNOME's indicator shows exactly while someone is connected, and nothing is captured otherwise.

1. `org.freedesktop.host.portal.Registry.Register("dev.sunna.Host")`: the portal's name for Sunna. It needs a `dev.sunna.Host.desktop` file whose program exists (the package and the installer ship one).
2. `RemoteDesktop.CreateSession`.
3. `RemoteDesktop.SelectDevices`: keyboard and pointer, `persist_mode = 2` (until revoked), and the stored `restore_token` if there is one.
4. `ScreenCast.SelectSources`: one monitor, `cursor_mode = metadata` (the pointer comes separately, not drawn into the picture: the viewer draws its own pointer).
5. `RemoteDesktop.Start`. With a valid token it returns at once; otherwise GNOME shows the dialog. The result has the stream (a PipeWire node id, its size and position), the devices granted, and a **new** restore token, which replaces the stored one (tokens are single use). The token lives in `~/.config/sunna/portal-token`, readable only by the user.
6. `ScreenCast.OpenPipeWireRemote` gives a file descriptor for PipeWire.

The source and the input injector are made separately for each session (`SourceFactory`, `InjectorFactory`), so they share the portal session through a small registry: whichever comes first starts it, and it closes when both are done. If GNOME closes it (Stop in the indicator), the source's next frame fails with a reason the host passes on to the viewer.

### Frames

A PipeWire stream on that file descriptor, asking for raw video in BGRx/BGRA (also RGBx/RGBA), shared-memory buffers, and the cursor and damage metadata. The stream's properties must include `media.type=Video` (with `media.category=Capture`, `media.role=Screen`), or WirePlumber won't link it.

GNOME delivers BGRx, the same byte layout as the X11 capture, so frames go to the existing encoders unchanged (`PixelFormat::Bgra8`, CPU bytes). When the viewer asks for a smaller stream, they're downscaled the way X11 frames are. Frames come when the screen changes; like X11, the newest is resent every 100 ms when nothing changes.

`libpipewire-0.3.so.0` is loaded when needed (as NVENC and PulseAudio are), so `sunnad` still runs, and builds, where PipeWire isn't installed. Its C interface is stable and versioned; the small part used (main loop, context, stream, a few format objects) is declared by hand.

Later: the damage metadata can feed the fast lane instead of comparing frames, and DMA-BUF buffers could go to NVENC without a copy.

### Input

Through the same portal session (`Notify*` methods):

| Sunna event | Portal call |
|---|---|
| pointer position | `NotifyPointerMotionAbsolute(stream, x, y)`, in the stream's pixels |
| buttons | `NotifyPointerButton(BTN_LEFT / RIGHT / MIDDLE / SIDE / EXTRA, pressed)` |
| scroll | `NotifyPointerAxis(dx, dy)` with `finish` at the end of a gesture; `NotifyPointerAxisDiscrete` for wheel clicks |
| keys | `NotifyKeyboardKeycode(evdev code, pressed)`: the Mac keycodes on the wire map to evdev exactly as for X11 (`evdev_from_mac`) |
| pinch | Ctrl + wheel, as on X11 |

As on X11, the injector remembers what it holds down and lets go of it when the session ends.

### The pointer's shape

With `cursor_mode = metadata`, each frame can carry the pointer's position and, when it changes, its image. Those become the same `Cursor` messages the X11 host sends (from XFixes), so the viewer draws the right pointer. XFixes itself is no use on Wayland: through Xwayland it only knows X11 windows' pointers.

### Choosing the path

`sunnad --source screen` uses the portal when `XDG_SESSION_TYPE=wayland` (GNOME sets `DISPLAY` for Xwayland too, so that can't decide it), else X11. `SUNNA_CAPTURE=x11` or `=portal` overrides. `sunna-host setup --desktop` stops refusing Wayland; it brings up the dialog through `sunnad` with instructions, and the service gets the session's `WAYLAND_DISPLAY`, `XDG_SESSION_TYPE` and D-Bus address, as it gets `DISPLAY` today.

## Verified

On GNOME 51.0 (mutter 51.0, xdg-desktop-portal 1.22.1, xdg-desktop-portal-gnome 51.0, PipeWire 1.6.9), in the test bed below, with `tools/gnome-test/gnome-poc.py`:

| | |
|---|---|
| Portals | RemoteDesktop v2 (keyboard, pointer, touchscreen); ScreenCast v5 (monitor, window, virtual; cursor hidden, embedded, metadata) |
| First start | the Remote Desktop dialog: Remember This Selection (on), Allow Remote Interaction (off), the screens, Share |
| After Share | the session started with keyboard and pointer (`devices = 3`), a 1920×1080 stream, and a restore token |
| Next start, with the token | started in under 0.1 s, no dialog, keyboard and pointer, and a new token; the permission sits in the permission store's `remote-desktop` table |
| Frames | first frame 30 ms after the stream started, BGRx 1920×1080, up to 60 fps |
| Pointer | a click at the top bar's clock opened the calendar |
| Keyboard | Super opened the overview; typing "files" searched for it |
| While shared | GNOME's orange indicator in the top bar |

Not yet checked: the lock screen (does the stream continue, do keys reach the password field), Stop in the indicator (the `Closed` signal, and whether the token survives it), several screens, and the cursor and damage metadata on real frames. The dialog said "An app wants to share your screen" even though Sunna had registered its name; that's to look into.

## GNOME versions

| Ubuntu | GNOME | Xorg session | Through the portal |
|---|---|---|---|
| 22.04 | 42 | yes | not tested |
| 24.04 | 46 | yes (Wayland is the default) | not tested |
| 25.10 | 49 | no | not tested |
| 26.04 | 50 | no | not tested |
| (Arch, now) | 51 | no | verified |

Remembered remote-desktop permissions arrived in GNOME's portal after screen-cast ones (GNOME 42); which release first had them decides whether older GNOMEs ask on every connection. The test bed will run Ubuntu's own GNOME for each row before this ships. If a version can't remember, the fallback is one portal session for as long as the host runs, approved once per login.

## The test bed

`tools/gnome-test/` runs GNOME Shell headless in a Docker container, isolated from the machine's own sessions, with PipeWire, the GNOME portal and a mock logind (`python-dbusmock`, as GNOME's own tests do; GNOME 51 won't start without logind). GNOME runs in unsafe mode so the helpers can take screenshots (`shot.sh`) and click or type (`click.sh`, `key.sh`) through virtual input devices, which is how the dialog gets approved.

Known quirk: in the container, GNOME Shell crashes when the dialog closes after Share (`meta_window_unmanage: assertion failed: (window->display->focus_window != window)`), but only after the session has started and the token is stored. Restarting GNOME Shell and the portal, then starting with the token, works. Real sessions, with a real keyboard and logind, don't do this.

## Not covered

- **KDE Plasma**: the same portal works (`tools/wayland-poc.py`); KDE can skip the dialog for pre-authorized apps.
- **Hyprland and other wlroots desktops**: their portal has screen cast but no remote desktop, so input would need wlroots' own virtual pointer and keyboard protocols.
- **The login screen**: GNOME's Remote Login (RDP from GDM) is a different, privileged system.

## Plan

1. `sunna-capture`: the portal session (with `zbus`, pure Rust, so building needs no new system packages), the PipeWire source, the cursor metadata. `sunna-input`: the portal injector. `sunnad`: choosing the path, sharing the portal session, the token file, `--authorize`.
2. `sunna-host`, the `.desktop` file, the package and the installer; README.
3. Test in the test bed on GNOME 51, then Ubuntu 26.04, 25.10 and 24.04 images; then on a real laptop.
4. Later: the fast lane from damage metadata, DMA-BUF to NVENC, KDE without the dialog.
