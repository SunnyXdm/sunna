# Sharing a GNOME desktop on Wayland

Status (2026-10-09): built (`crates/portal`, `sunnad --authorize`, `sunna-host setup --desktop` on Wayland) and working end to end on GNOME 46, 49 and 50 in the test bed (below). Not yet run on a real laptop, nor on KDE.

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

**The portal itself**, on GNOME 51.0 (mutter 51.0, xdg-desktop-portal 1.22.1, xdg-desktop-portal-gnome 51.0, PipeWire 1.6.9), with `tools/gnome-test/gnome-poc.py`:

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

**Sunna's implementation**, with `sunnad` in the test bed and `sunna-cli connect --click X,Y --type TEXT` as the viewer (it clicks and types like the apps do), on GNOME 46.0, 49.0 and 50.1 (Ubuntu 24.04, 25.10 and 26.04's own packages):

| | |
|---|---|
| `sunnad --authorize` | the dialog came up; after Share it said whether keyboard and pointer were granted (one run where the switch was missed said "not granted", as it should), and saved the token, mode 600 |
| A session | started silently from the token: `portal capture (PipeWire)`, keyboard and pointer, cursor metadata, 1920×1080; frames at the viewer about 40 ms after capture with software H.264 |
| Input | Sunna's click on Activities opened the overview and its typing searched for "files", on all three |
| Stop in GNOME's indicator | the session ended with "Sharing was stopped on the computer", sent to the viewer as the reason; the next connection started silently, so Stop doesn't revoke the permission |

Two bugs that only a real GNOME showed, both fixed: negotiated format values come wrapped in a choice of one, and mutter now and then gives a frame's stride in pixels rather than bytes (during the overview's animation); the chunk's size is right, so it's used then.

Not yet checked: a real laptop (display scaling, several screens, the lock screen), KDE with this code, and how the pointer's shapes look in a viewer. On a real desktop the clipboard goes through Xwayland (`DISPLAY`), which GNOME keeps in step with Wayland's; the test bed has no Xwayland. The dialog says "An app wants to share your screen" even though Sunna registers its name: to look into.

## GNOME versions

| Ubuntu | GNOME | Xorg session | Through the portal |
|---|---|---|---|
| 22.04 | 42 | yes | not tested |
| 24.04 | 46 | yes (Wayland is the default) | works; its portal (1.18) has no Registry, and the permission is remembered anyway |
| 25.10 | 49 | no | works |
| 26.04 | 50 | no | works |
| (Arch) | 51 | no | the portal works (gnome-poc.py) |

All of them remember the permission, so no fallback (a portal session for as long as the host runs) is needed for these.

## The test bed

`tools/gnome-test/` runs GNOME Shell headless in a Docker container, isolated from the machine's own sessions, with PipeWire, the GNOME portal and a mock logind (`python-dbusmock`, as GNOME's own tests do; GNOME 51 won't start without logind). `up.sh` uses Arch's newest GNOME, or `UBUNTU=24.04`, `25.10` or `26.04` for Ubuntu's own. GNOME runs in unsafe mode so the helpers can take screenshots (`shot.sh`) and click or type (`click.sh`, `key.sh`) through virtual input devices, which is how the dialog gets approved: from the overview, click the dialog's preview, then the switch, then Share.

The helpers stamp every event with the current time: an event stamped ahead makes mutter refuse focus changes that come before it, and it asserts (`meta_window_unmanage: focus_window != window`) when the dialog then closes.

## Not covered

- **KDE Plasma**: the same portal works (`tools/wayland-poc.py`); KDE can skip the dialog for pre-authorized apps.
- **Hyprland and other wlroots desktops**: their portal has screen cast but no remote desktop, so input would need wlroots' own virtual pointer and keyboard protocols.
- **The login screen**: GNOME's Remote Login (RDP from GDM) is a different, privileged system.

## Next

1. On a real laptop: display scaling (the portal's coordinates against the frame's pixels), several screens, the lock screen, the clipboard.
2. KDE with this code.
3. Later: the fast lane from damage metadata, DMA-BUF to NVENC, KDE without the dialog.
