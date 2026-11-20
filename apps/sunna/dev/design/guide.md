# Sunna interface: design guide

This is the brief for the whole interface, desktop and phone. `inventory.md`
(next to this file) lists everything the apps do today; this guide says how all
of it should look and behave. Where the two disagree about *what exists*, the
inventory wins. Where they disagree about *how it looks or moves*, this guide
wins.

## 1. The idea: Sunna is a zoom

A computer's screen is one object. It is never replaced by a page, a spinner
screen or a dialog. You only ever move closer to it or further from it.

| Level | Name | What you see |
|---|---|---|
| 0 | Overview | All your computers at once, each at its true shape |
| 1 | Computer | One computer, large, with its name, how it is, and what you can do |
| 2 | Inside | The session: that screen fills the window |

- Moving between levels is always the same motion: the screen grows or shrinks
  from where it is to where it's going. Nothing cuts, nothing slides in from
  off-screen, no separate "connecting" page.
- The app opens at level 1 on the computer you used last. Return connects.
- The session menu is half a step out from level 2: the live screen shrinks a
  little and the controls appear around it. Closing it zooms back in.
- A problem is shown on the computer it belongs to, at level 1, in the place
  the Connect button would be, with the fix. No error dialogs, no dead ends.

Everything below follows from this. If a screen can't be explained as "a level,
or something attached to a computer at a level", it's designed wrong.

## 2. What a screen shows

Each computer is drawn as its real screen, in this order of preference:

1. **Live** (ready computers, at levels 0 and 1): a low-rate preview of what
   it shows now. In the prototype the stand-in screens move a little (a caret
   blinks, a clock ticks, meters change, a playhead moves). Subtle: no more
   than one small thing moving per second per screen.
2. **Last picture**: what it showed when you last disconnected. Offline
   computers show this, dimmed and without colour.
3. **Never connected**: a plain dark screen with the computer's name in it and
   its system mark (section 9). No invented wallpapers.

States drawn on the screen itself:

| State | The screen |
|---|---|
| Ready | Full brightness, live |
| In use | Full brightness, a small "In use" tag in its top-right corner |
| Key doesn't match | Blurred and dimmed, a lock in the centre |
| Update needed | Dimmed, an up-arrow mark in the centre |
| Offline | Last picture, no colour, 42% brightness |
| Checking | As it last was, with the status dot pulsing (opacity only) |
| Not found / invalid address | Never-connected screen, dimmed |

(The real apps need two things the prototype fakes: saving the last picture on
disconnect, and the host sending preview pictures. Both get an off switch in
Settings: "Screens".)

## 3. Look

Quiet, native, dark. The screens are the only colour in the interface.

- **Colour.** Window `#0b0b0c`. Text `#f4f4f5`; secondary 58% white; tertiary
  36% white. Hairlines 8% white. Status dots only: ready `#30d158`, in use
  `#ff9f0a`, problem `#ff453a`, offline/unknown `#6e6e73`. There is no accent
  colour and no light theme.
- **Type.** The system interface font everywhere (`-apple-system`, Segoe UI
  Variable, system-ui); a system monospace only for addresses and keys.
  Desktop: computer name 26/650, tracking -0.02em; body 13.5; small 12. Phone:
  name 29/650; body 15; small 13. Numbers are tabular. Sentence case
  everywhere ("Add a computer", not "Add a Computer").
- **Shape.** Screens: 8px corners (10 on phone), a 1px 9%-white edge, one soft
  shadow. Controls: 9px corners, 34px tall on desktop; 14px corners, 50px tall
  on phone. Nothing else has a border or a shadow: no cards, no panels.
- **Buttons.** Primary: light (`#f2f2f3`, dark text), one per view at most.
  Secondary: 10% white fill. Destructive: red text, no fill, and it always asks
  once, inline. Icon buttons: 30×28 desktop, 44×44 phone.
- **Fields.** 7% white fill, 13% white edge, edge goes to 40% white on focus.
  No blue focus rings.
- **Switches and choices.** A switch is neutral: on = light track, dark knob;
  off = 16% white track, light knob. A two- or three-way choice is a segmented
  control in the same neutral style.
- **Capsule.** The one floating element: a small rounded label
  (`rgba(18,18,20,.86)`, 1px 12%-white edge, 12.5/600 text) used for
  connecting progress, session notices and the stats line. Always top-centre in
  a session.

Never: serif or display fonts, greetings, slogans, gradients on interface
elements, glows, blur/glass panels, grain, emoji, vendor logos, coloured
buttons, bordered cards in a grid, drop shadows on text, anything that
bounces more than a hair.

## 4. Motion

- Two springs only (both are in `directions.html` as `spring(k, c)`):
  **move** `spring(190, 24)` for things changing place or size inside a level;
  **zoom** `spring(150, 22)` for changing level. Fades are 200 ms linear.
- A zoom animates the screen object itself, drawn at its final size and
  transformed from its starting box, so it stays sharp.
- While a screen zooms, everything that isn't the screen fades (200 ms) and
  scales 0.97. It never moves sideways.
- Interruptible: starting a new motion from mid-flight continues from where
  things are.
- `prefers-reduced-motion`: zooms become 150 ms cross-fades; live previews
  stop moving.
- Only `transform` and `opacity` animate in the hot paths.

## 5. Level 1: Computer (the home view)

Layout, top to bottom, centred:

1. Toolbar (52px desktop): left, an "All computers" button (grid icon + text)
   that zooms out to level 0; right, Add (+) and Settings. Nothing else.
2. The screen. Height about 58% of the space under the toolbar (min 240, max
   560 on desktop), width from its real shape. The computers either side peek
   in at the edges, smaller (0.9) and at 26% opacity.
3. Name (with a ••• button that appears beside it on hover; always shown on
   phone).
4. One status line: dot, state, then in tertiary text the system and size
   ("Ready · 12 ms  ·  Arch Linux · Desktop · 2560 × 1440").
5. The action row, one of:
   - Ready: **Connect** (primary, with a ↵ key hint on desktop).
   - In use: text, "You can connect when they leave."
   - Key doesn't match: a key field and **Update key**, right there.
   - Update needed: text saying which side needs the update (from inventory).
   - Offline: text, "Shown as it was 3 h ago. It lights up when it's back."
   - After a failed or ended session: the reason in plain words, and
     **Try again** / **Reconnect** (section 8).
   - Removing: "Remove archlinux?" with **Remove** (red text) and **Keep**.
6. A row of names with status dots along the bottom; the current one is marked
   with a short line that slides between them. On phone this is dots only, the
   current one a pill with its name.

Moving: ← → keys, click a neighbour, drag sideways, swipe on phone. The last
position is an empty slot: a dashed screen outline, "Add a computer" (section 7).

••• menu (small, attached to the name; not a sheet): Edit, Copy link, Show QR
code, Move left, Move right, Remove. Use the items the inventory lists; add
none.

Keys (desktop): ← → move; ↵ connect; Esc zoom out to overview; ⌘N add;
⌘, settings; ⌘⌫ remove (asks inline).

**No computers yet:** level 1 shows only the empty slot, already open as the
add form, with one sentence above it saying what to do (take the wording from
the inventory's first-run text).

## 6. Level 0: Overview

- Rows of screens, all the same height within a row, filling the width,
  computers you can use first (`directions.html` → `V.wall.layout`). On phone:
  one full-width row per ready computer, the rest two to a row.
- Under each: name, status line. No buttons on the tiles.
- Click or tap a computer: zoom to it at level 1. Double-click, or ↵ on the
  selected one: zoom straight through into the session.
- The last tile is the dashed "Add a computer" slot.
- Arrow keys move a selection ring (1.5px, 60% white, outside the screen).
- Getting here: the toolbar button, Esc at level 1, pinch in on phone (and the
  same button, top-left, on phone).

## 7. Adding and editing a computer

Adding is level 1 for a computer that doesn't exist yet. Editing is the same
view, filled in.

- The stage shows the dashed empty screen. As soon as the address answers, it
  fills in as that computer (never-connected style, with its name and system)
  with the **move** spring.
- Below, a single 420px column (full width on phone):
  1. Address: placeholder `100.101.102.103, a name, or a sunna:// link`.
     Pasting a `sunna://` link fills address and key together.
  2. Key: hidden, with show/hide.
  3. Name: optional; placeholder is the name the computer gave.
- One live line under the fields says what's happening, using the inventory's
  exact strings (looking, found with system and latency, nothing answers, key
  doesn't match, update needed).
- **Add computer** (primary; enabled when address and key check out) and
  **Cancel** (Esc).
- Other ways in, as quiet text buttons under the form, only where they exist
  today: **Find on Tailscale** (desktop: the form's place is taken by the list
  of tailnet devices; choosing one fills the address) and **Scan QR code**
  (phone), **Paste** (phone).
- "Show QR code" for an existing computer / this computer: the QR is drawn
  where the screen is, at the screen's size, white on the dark window, with
  the link under it and **Copy link**.

## 8. Connecting, the session, and leaving

**Connecting.** Connect starts the zoom at once: the screen grows to fill the
window showing its last picture at 55% brightness. A capsule (top-centre)
shows a 12px ring spinner and the current stage in the inventory's words, and
on desktop "esc to cancel". When video arrives the picture brightens over
300 ms, the capsule says "Connected · 12 ms" and fades after 1.6 s. Cancel, or
any failure, zooms back out to level 1, where the action row gives the reason
and **Try again**. Every failure reason in the inventory gets its own plain
sentence there.

**Session, desktop.** The screen and nothing else. Optional (Settings): the
stats capsule, top-centre, tertiary text, tabular numbers, showing the fields
the inventory lists; and a ••• button, top-right. Notices (no picture,
reconnecting, and the rest in the inventory) use the same capsule, replacing
the stats while they show.

**Session menu, desktop** (••• or its shortcut): half a step out. The live
screen scales to about 76% and moves up; the window behind is `#0b0b0c`. Under
the screen:
- a row of controls, each an icon over a label, 72px wide, no boxes: every
  item the session menu has today (inventory), in this order: Disconnect last
  and in red text; toggles show their state in the label ("Sound on");
  submenus open as a small list above their control.
- under that, "Other computers": the rest of your computers as small screens
  (96px tall) with names. Choosing one switches: this screen shrinks back into
  the strip while that one grows into the session.
Esc, the shortcut again, or clicking the screen closes the menu (zooms in).

**Session, phone.** Portrait and landscape. The picture fits the width; the
existing floating buttons (inventory: keyboard, •••) stay, as 44px neutral
circles. The menu is the same half step out: the screen shrinks to the top,
and below it a list of 56px rows (icon, label, state on the right), Disconnect
last in red text, then the other computers as a sideways strip. Back goes one
step out, always: menu → session is "close menu"; session → leaves as it does
today (inventory).

**Leaving.** The screen shrinks back to its place at level 1. If the session
ended by itself, the action row says why, in the inventory's reasons, with
**Reconnect**.

## 9. Settings and small things

**Settings** is a view, not a dialog: the level-1 content cross-fades (200 ms)
to a single centred column, 560px max (full width on phone), with "‹ Computers"
top-left. Groups are a small tertiary heading and rows separated by hairlines;
a row is label + one line of help on the left, the control on the right. No
boxes. Groups: This computer (name, whether it's being shared, its key with
show/copy, Show QR code); Sessions; Screens (new: "Remember each computer's
last screen", "Live previews"; both on); Shortcuts (desktop: each with its
keycaps); About. Every row in the inventory's settings must appear; add only
the two Screens rows.

**System marks.** Our own, single-colour, 16px: ⌘ for macOS, a penguin for
Linux (credit line stays in About, as today), a plain window for Windows, a
phone for Android. Never vendor logos.

**Permissions and one-off prompts** (macOS Accessibility, Android
notifications, anything else in the inventory): a row in the level-1 action
area or in Settings, saying what it's for and offering the one button. Never a
custom full-screen prompt.

**Words.** Plain and short. Keep the inventory's strings unless this guide
changes them. Sentence case. No exclamation marks. Say what happened and what
to do: "Couldn't reach archlinux. It may be asleep or off the network."

## 10. Phone specifics

- The app fills the screen edge to edge (`viewport-fit=cover`, safe-area
  insets respected). Header 52px: overview button left, Add and Settings right.
- Level 1: the screen sits in the upper half; name, status and the system line
  under it, centred; the dots row, then the action (a 50px full-width button
  or the inline fix) pinned above the home indicator, in thumb reach.
- Nothing depends on hover. Every target is at least 44×44.
- Swipe sideways to move between computers; pinch in (or the button) for the
  overview; tap the screen or the button to connect; long-press the name for
  the ••• menu.
- Add/edit: fields stay above the keyboard; the dashed screen shrinks to make
  room.

## 11. What must be covered

Every screen, state, string and flow in `inventory.md` sections A–C has a
place in this design. Nothing is dropped silently: if something has no place,
that's written down with the reason.
