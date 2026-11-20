# Real Linux desktop frames

Captured 17 November 2026, 22:13–22:15 UTC. These are uncomposited screenshots of real desktop sessions and real applications, captured from container-local Xvfb displays with ImageMagick `import -window root`. PNGs retain the native display pixels. No generated imagery, painted window chrome, host display, or screenshot overlays were used. The contact sheet alone combines and labels the captures.

All desktops run **Debian GNU/Linux 13 (trixie)**. GNOME is **48.7, Debian stable's maintained release, not the latest upstream GNOME**. This is the one version deviation from the request. All five requested desktop scenarios rendered successfully.

## Files

Each row has a full-resolution PNG and a 1600px-wide JPEG, encoded by Pillow with `quality=85`, Lanczos downsampling, and preserved aspect ratio (rounded height). `-b` is a genuinely later capture of the same running session.

| PNG | JPEG | Desktop | Native resolution | On screen / change |
| --- | --- | --- | --- | --- |
| [01-plasma-rust.png](01-plasma-rust.png) | [01-plasma-rust.jpg](01-plasma-rust.jpg) | KDE Plasma 6.3.6, Breeze Dark | 2560×1440 | Kate 25.04.3 with the actual `crates/capture/src/linux.rs`; Konsole 25.04.2 running live `htop -u sunny`, side by side. |
| [01-plasma-rust-b.png](01-plasma-rust-b.png) | [01-plasma-rust-b.jpg](01-plasma-rust-b.jpg) | KDE Plasma 6.3.6, Breeze Dark | 2560×1440 | Live CPU/memory/process values and clock advance. |
| [02-gnome-notes.png](02-gnome-notes.png) | [02-gnome-notes.jpg](02-gnome-notes.jpg) | GNOME Shell 48.7, default Adwaita | 1920×1080 | Nautilus 48.3 with populated home directory; GNOME Text Editor 48.3 with capture-session notes. Debian default wallpaper. |
| [02-gnome-notes-b.png](02-gnome-notes-b.png) | [02-gnome-notes-b.jpg](02-gnome-notes-b.jpg) | GNOME Shell 48.7, default Adwaita | 1920×1080 | Pictures folder selected; native selection status and clock advance. |
| [03-plasma-krita.png](03-plasma-krita.png) | [03-plasma-krita.jpg](03-plasma-krita.jpg) | KDE Plasma 6.3.6 | 3440×1440 | Maximized Krita 5.2.9 with real `Earthrise-study.kra` photo project, layers, brush presets and color selector; native Plasma panel remains visible. |
| [03-plasma-krita-b.png](03-plasma-krita-b.png) | [03-plasma-krita-b.jpg](03-plasma-krita-b.jpg) | KDE Plasma 6.3.6 | 3440×1440 | Brush size changes from 40px to 41px and its canvas outline moves. Photograph unchanged. |
| [04-xfce-docs.png](04-xfce-docs.png) | [04-xfce-docs.jpg](04-xfce-docs.jpg) | XFCE 4.20 (session 4.20.2) | 1920×1080 | Firefox ESR 153.7.0 on the live Rust Book ownership chapter; XFCE Terminal showing actual source-file output from `sed`. Native panels and XFCE wallpaper. |
| [04-xfce-docs-b.png](04-xfce-docs-b.png) | [04-xfce-docs-b.jpg](04-xfce-docs-b.jpg) | XFCE 4.20 (session 4.20.2) | 1920×1080 | Terminal runs `wc -l linux.rs`, returning 546; clock advances. |
| [05-mate-planning.png](05-mate-planning.png) | [05-mate-planning.jpg](05-mate-planning.jpg) | MATE 1.26 (session 1.26.1), light theme | 2560×1600 | LibreOffice Calc 25.2.3 with a formatted workspace-planning spreadsheet at 140% zoom; E9 selected. Sample planning data, not benchmark results. |
| [05-mate-planning-b.png](05-mate-planning-b.png) | [05-mate-planning-b.jpg](05-mate-planning-b.jpg) | MATE 1.26 (session 1.26.1), light theme | 2560×1600 | Selection moves to E8, with matching formula-bar text and clock advance. |

[contact.png](contact.png) is a labelled 1920×3250 sheet: first frame on the left, later frame on the right. JPEG dimensions are 1600×900, 1600×900, 1600×670, 1600×900 and 1600×1000 respectively.

## Content and licences

- **Earthrise:** NASA / Bill Anders, Apollo 8, 24 December 1968, AS08-14-2383. Public domain as a NASA work. [Source and rights record](https://commons.wikimedia.org/wiki/File:NASA-Apollo8-Dec24-Earthrise.jpg); [original JPEG](https://upload.wikimedia.org/wikipedia/commons/a/a8/NASA-Apollo8-Dec24-Earthrise.jpg). Original resolution 2400×2400. SHA-256 `24fd2a4f833534ca6741ed96d03b028ac1d8fda2cd129ded4d208535f541d4da`. Opened in Krita and saved as a native project without modifying the photograph. Reusable project: [source/Earthrise-study.kra](source/Earthrise-study.kra).
- **Rust source:** copied only from this repository's `crates/capture/src/linux.rs`, unchanged. SHA-256 at capture: `377af75b9c7e4c28f71193cf03c7a0df916a185179a420d756d947b370452e38`. Repository licence applies.
- **Browser:** [The Rust Programming Language — What is Ownership?](https://doc.rust-lang.org/book/ch04-01-what-is-ownership.html), loaded directly in Firefox. The Rust Book is distributed under MIT/Apache-2.0; see its [repository](https://github.com/rust-lang/book).
- **Notes and spreadsheet:** original sample content prepared for these sessions; no personal files or production data. Notes are embedded in `setup.sh`; spreadsheet generation is in `content.py`, with the actual opened document retained as [source/Workspace-study.fods](source/Workspace-study.fods).
- Desktop wallpapers, icons, and application chrome are the installed distributions' assets. Their respective upstream licences apply; these screenshots do not relicense third-party assets.

## Reproduction

The supporting scripts are intentionally small building blocks, not a promise of pixel-identical replay: clocks, process output, package repositories and website content change. [packages.txt](packages.txt) records every installed package/version. No Docker image was committed; packages were installed into the disposable container's writable layer.

Start from the repository root. Commands below create only the named container and bind only this directory. No ports, host X sockets, host home directories or credentials are mounted.

```sh
frame_dir="$PWD/apps/sunna/dev/design/frames"
docker pull debian:trixie-slim
# Capture used digest:
# sha256:a72b4433de1cf7d6bae0c4cee74d0547f58601a643b3375f070c51600d63b6b2
docker run -d --name sunna-frames-desktops --hostname debian --shm-size=1g \
  -v "$frame_dir:/frames" debian:trixie-slim sleep infinity
docker exec sunna-frames-desktops bash /frames/install.sh
docker cp crates/capture/src/linux.rs sunna-frames-desktops:/tmp/linux.rs
curl -L --fail -o /tmp/sunna-earthrise.jpg \
  https://upload.wikimedia.org/wikipedia/commons/a/a8/NASA-Apollo8-Dec24-Earthrise.jpg
docker cp /tmp/sunna-earthrise.jpg sunna-frames-desktops:/tmp/earthrise.jpg
docker exec sunna-frames-desktops bash /frames/setup.sh
docker exec sunna-frames-desktops python3 /frames/content.py
docker exec -d sunna-frames-desktops /usr/lib/systemd/systemd-logind
docker exec -d sunna-frames-desktops bash /frames/session.sh sunny :1 2560x1440 kde
docker exec -d sunna-frames-desktops bash /frames/session.sh gnome :2 1920x1080 gnome
docker exec -d sunna-frames-desktops bash /frames/session.sh xfce :3 1920x1080 xfce
docker exec -d sunna-frames-desktops bash /frames/session.sh mate :4 2560x1600 mate
```

`session.sh` creates independent 24-bit Xvfb displays and user/session buses, with software rendering (`LIBGL_ALWAYS_SOFTWARE=1`). All X servers live inside this container. `control.py USER 'COMMAND'` runs commands on the corresponding container display and session bus. Wait for the session before opening applications, and after every move/maximize operation before capture.

### Plasma development

Apply `plasma-apply-colorscheme BreezeDark`. Launch Kate and Konsole with `XDG_CURRENT_DESKTOP=KDE QT_QPA_PLATFORMTHEME=kde` so Qt uses the installed Plasma integration. Open `/home/sunny/Projects/linux.rs` in Kate and use Ctrl++ three times to reach the captured 133% editor zoom. In Konsole use DejaVu Sans Mono 12pt and run `htop -u sunny`; press Shift+H to hide user threads. These are real process readings, not fabricated build results.

Arrange with `wmctrl -r 'linux.rs' -e 0,30,60,1500,1240` and `wmctrl -r 'htop' -e 0,1560,60,965,1240`. Native window decorations can adjust the final outer geometry. Set panel launchers to Konsole, Kate, Dolphin and Firefox through Plasma's panel configuration. Capture display `:1`; wait for htop to refresh and capture `-b`.

### GNOME notes

Start the container's `systemd-logind` before GNOME Shell. Shell runs with `gnome-shell --x11`; GNOME's default overview is dismissed with Escape. Launch both apps with **`GSK_RENDERER=cairo`**: the default GTK4 renderer failed to attach X11 shared-memory buffers in this setup, yielding invisible application surfaces. Cairo rendered correctly.

Open Nautilus at `/home/gnome`, and `gnome-text-editor /home/gnome/Documents/Capture-notes.txt`. Copy the existing Earthrise JPEG and notes into the home directory as `Earthrise.jpg` and `Session-notes.txt`. Use `wmctrl` positions `0,80,160,950,700` for Home and `0,1080,170,740,730` for Capture-notes. Mutter adds the client-side decoration offsets seen in the final image. Capture `:2`, then single-click Pictures and capture again. The standard GNOME top panel is present; its dock is normally hidden outside the overview.

### Ultrawide Krita

Create an additional user `creative`, its writable `/run/user/<uid>` (mode 700), and copy the KDE user's `kdeglobals` plus the source photo/project to that home. Launch `session.sh creative :5 3440x1440 kde`. Run Krita with `XDG_CURRENT_DESKTOP=KDE QT_QPA_PLATFORMTHEME=kde`, open the retained `.kra`, and maximize with `wmctrl -r Krita -b add,maximized_vert,maximized_horz`. The captured canvas is fitted at 51.1%, with one Background layer and standard default dockers. Panel launchers: Krita, Dolphin, Firefox. Move the brush outline into the gray canvas margin at `(2800,1100)`, capture `:5`, press `]` once and move to `(2750,1040)`, then capture `-b`. No photograph edit is needed.

### XFCE documentation

Launch Firefox with a dedicated profile; disable `browser.aboutwelcome.enabled` and `browser.shell.checkDefaultBrowser`, and set `browser.startup.homepage_override.mstone` to `ignore` in its `user.js`. Open the Rust Book URL above. Open XFCE Terminal in `/home/xfce/Projects`; `setup.sh` supplies the normal `sunny@debian` prompt. Run `sed -n '12,36p' linux.rs`.

Window geometry: Firefox `0,45,85,1180,850`; Terminal `0,1250,235,620,650`. Capture `:3`. Focus the terminal, wait for focus to settle, run `wc -l linux.rs`, and capture `-b`. Do not type immediately after an asynchronous window activation: keystrokes can otherwise reach the browser.

### MATE planning

Open the retained FODS file in `SAL_USE_VCLPLUGIN=gtk3 libreoffice --norestore --calc`. Close Tip of the Day and any first-run information bar. Unmaximize the window, then position it using `0,90,100,2320,1300`. Use Calc's zoom control to set 140%. Select E9 through the name box, capture `:4`, then select E8 and capture again. The GTK integration package is necessary for the captured appearance. Home icon label was set to `Home` with `gsettings set org.mate.caja.desktop home-icon-name Home`.

### Capture and export

Run each capture as container root so the bind-mounted folder's ownership does not prevent writing:

```sh
docker exec -e DISPLAY=:1 sunna-frames-desktops \
  import -window root /frames/01-plasma-rust.png
# Repeat for the display/file pairs above, including later frames.
docker exec sunna-frames-desktops python3 /frames/export.py
```

`export.py` leaves PNGs untouched, creates the quality-85 JPEGs, and builds the labelled contact sheet. Ordinary X hardware cursors are not included by `import`; Krita's application-drawn brush outline is included. Each pair therefore also has visible application-state or clock changes.

## Verification and resolved failures

All ten native PNGs were opened and visually inspected, including later frames. Dimensions, JPEG widths, and nonidentical pixels within each pair were checked programmatically. No blank screens, welcome dialogs, recovery dialogs or half-painted windows remain in the delivered frames. No tokens, keys, machine IP addresses, host files or environment dumps are visible. Only the selected Rust source was copied from the repository. The terminal process list is scoped to the container user; aggregate htop resource counters are real Linux readings.

Initial GNOME startup failed because `org.freedesktop.login1` was missing; starting the real login service fixed it. GTK4 app surfaces needed Cairo as described above. Plasma applications initially lacked theme integration; installing `plasma-integration` and launching with the KDE environment fixed that. LibreOffice's initial generic backend and first-run/recovery UI were rejected; GTK3 and normal app state were captured instead. Preliminary diagnostic screenshots were deleted. No desktop scenario had to be abandoned or replaced with a fake.

## Cleanup

The capture container was stopped and removed after export, along with the only image pulled for this task:

- Container: `sunna-frames-desktops` (all five virtual desktop sessions lived here).
- Image: `debian:trixie-slim`, digest `sha256:a72b4433de1cf7d6bae0c4cee74d0547f58601a643b3375f070c51600d63b6b2`.
- No custom image, named volume, network, published port or host service was created.
- Existing `sunna-shot` container and `sunna-bench:latest` image were left untouched.

The small scripts, package manifest and two source documents remain here for reproducibility. Nothing was committed; repository writes were confined to this directory.
