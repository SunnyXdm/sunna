# Sunna landing page · round 4

> **Changed after round 4 (2026-11-19, by the reviewer).** Where this report and the lines below disagree, these lines are right.
> - The sample data is served with `platform: "linux"` (`capture/fixture.py`), so every capture shows the app as it runs on Linux: no space reserved for Mac window buttons, no Mac-only Settings rows.
> - `archlinux` shows its whole desktop capture again, not the editor-only crop. At the sizes shipped, the process list in it is not legible.
> - "Share with a switch" is one continuous crop of the real Settings sheet (500 × 612 at 1x) and a film of the same crop. The earlier poster joined two separate crops and hid the field between them; that is gone.
> - "After a restart" shows the two SDDM frames whole, at their own 640 × 400, not an enlarged crop.
> - "From your phone" is removed. The only Android capture is of an earlier build with operating-system marks that the app no longer shows; `android.webp` is a clean-up candidate.
> - `capture/look.mjs` takes plain screenshots of Settings for checking.


The 15.5-second film opens ubuntu from Gallery, runs five protocol tests through a native Sunna viewer, and returns to Gallery. The launcher, its launch and return springs, the XFCE desktop, test output, and viewer menu are captures of running software. The launcher uses the project’s development fixture. The host and viewer make a real loopback connection.

## Pictures and commands

Run the commands from the repository root. The capture workspace is `~/ext/work/site/r4`. The full-size proof images are in its sibling `shots` directory.

| Picture | Source and command |
|---|---|
| Session and ubuntu tile | `python3 site/capture/session.py ~/ext/work/site/r3/target/debug` uses the previously built host and CLI. Display 60 runs xfce4-session, xfwm4, xfdesktop with the stock teal wallpaper, the panel, Thunar and XFCE Terminal. The file manager shows the staged Project folder under `~/ext/work/site/demo`. Display 61 runs the native viewer. FFmpeg grabs this display and its pointer at 1280 × 862, 24 fps. Frames at 0.08 s and 5.00 s supply the opening and return tile pictures. |
| Test output | The script prepares a standalone Cargo manifest from the workspace and uses the repository’s protocol crate sources and lockfile. Its private target directory is inside the demo shell’s project. It warms the test binary offline, then types `cargo test --lib roundtrip` through the viewer. The captured run reports five passed, zero failed, fourteen filtered out. Terminal text uses DejaVu Sans Mono at 13 points. |
| Hero, poster and README | `node site/capture/app.mjs` drives the actual 980 × 660 launcher at double pixel density. The cursor follows the dispatched input. `bash site/capture/encode.sh` joins the lossless launcher frames with the live viewer recording, including the opening spring and the return after Disconnect. The viewer segment uses only the grabbed pointer. The encoder writes both MP4/WebM pairs, the hover poster, the native README PNG and animated WebP. |
| Phone close-up | The encoder crops the same viewer recording to the terminal’s full width, including Cargo’s final result. The still is the real viewer showing the completed run. |
| Gallery, Devices, Wall, Space and Type | `node site/capture/app.mjs features` selects each view using the app’s View menu. Each capture is 1960 × 1320. The other tiles use the existing desktop captures from the project. The archlinux picture uses the editor region of the Kate/htop capture to exclude the login printed by htop. |
| Share with a switch | `node site/capture/sharing.mjs` operates the real Settings switch. The film shows Off, Installing, Starting and Sharing. The switch and link are separated by the default-key field in the product’s DOM. The poster joins two native-pixel crops from the On state after scrolling each into view. The link and its Copy and Show QR Code buttons are intact. The film reserves space for the link until sharing starts. |
| Update a computer | The features command captures a 688 × 352 region at the top right of a 720 × 400, two-computer window at 2×. The same coordinates apply in all three states. The menu opens through the Updates button. Progress and completion use the project’s sample host-update responses. |
| After a restart | The encoder crops the SDDM VM’s `machine-greeter.png` and `desktop.png` at 320 × 210 starting at (305, 102). Both came from the Linux login test evidence. SDDM’s account selector and the mountain desktop give the pair distinct visual states. The account labels belong to the disposable test VM. [Login-test evidence](../tools/login-test/evidence/REPORT.md) |
| From your phone | The encoder converts the entire archived 1080 × 2400 Android capture at `~/ext/shots/home.png` to WebP. The status bar and lower computers are part of the original image. The exact acquisition command and build number are unavailable. The caption identifies the earlier Android app. Desktop presentation is 300 px wide with a 1 px border and a 28 px corner radius. |
| Brand and typing figure | The mast and favicon use the existing Sunna icon. The typing chart retains its HTML/CSS bars, measurement values and method. The benchmark was last measured in an earlier round. |

## Layout and playback

The 1120 px column reserves 56 px between copy and picture. The picture takes 62% of the remaining width. Adjacent features are 112 px apart on desktop and 64 px apart on phones. At 390 px the picture appears first at the full 342 px content width. Section headings use 34 px, weight 600 and −0.02 em tracking. Body text is 17 px. Captions are 13 px with a 10 px gap.

The setup steps distinguish pasting a link in the app from scanning a code on Android. The wording follows the [getting-started guide](../docs/get-started.md). The hero follows the system motion preference. The sharing row opens on the On poster in all modes. Its playback link plays the loop inline with JavaScript and opens the MP4 directly without JavaScript. This keeps native video controls clear of the status and link before playback.

## Reproduction

```sh
export TMPDIR="$HOME/ext/work/site/r4/tmp"
export SUNNA_SITE_OUT="$HOME/ext/work/site/r4/build"
mkdir -p "$TMPDIR" "$HOME/ext/work/site/shots"
python3 site/capture/session.py ~/ext/work/site/r3/target/debug
python3 site/capture/fixture.py &
fixture_pid=$!
node site/capture/app.mjs features
node site/capture/app.mjs
node site/capture/sharing.mjs
kill "$fixture_pid"
bash site/capture/encode.sh
python3 site/capture/proof.py
node scripts/site/build.mjs
python3 apps/sunna/dev/serve.py 8797 127.0.0.1 "$SUNNA_SITE_OUT" &
page_pid=$!
node site/capture/verify.mjs
kill "$page_pid"
```

For a fresh host and viewer build, `bash site/capture/build-binaries.sh` builds offline into round 4’s private target. Then `session.py` can use its default binary path. New recordings have slightly different timings. Check the contact sheet and the selected desktop frames after recording.

## Review and privacy

- `r4-hero-sheet.png` contains sixteen frames decoded from the delivered MP4. The sheet samples the changing area of the teal desktop during both springs.
- `r4-real-vs-video.png` pairs the fresh plain launcher screenshot with its first lossless screencast frame. The 1960 × 1320 sources differ in 895 pixels within the cursor’s 36 × 49 rectangle. The delivered video uses lossy compression.
- `r4-page-{1440,390}-{on,off,reduced}.png` are the six full-page captures. Chrome’s network and layout-shift events were collected independently of page JavaScript. The checks scroll through lazy media, exercise the keyboards of all three radio groups, and check inline sharing playback.
- The review caught a clipped timestamp at the edge of the greeter, a cut-off Cargo line in the phone close-up, native controls covering the masked link, and a wrapped timing value. The final captures include those corrections. Feature proofs use the delivered images’ native pixel dimensions. The typing proof uses the desktop page’s actual chart dimensions.
- The private capture home, configuration, cache and session bus keep the staged desktop separate. The host uses a fresh throwaway token, loopback UDP 48860, disabled audio and disabled clipboard. The terminal shows the neutral prompt and relative target path. The fixture supplies neutral device labels and a documentation-range address for sharing. The UI masks the key. The complete Android capture shows device names and its status bar. The delivered pictures were reviewed for personal names, home paths, email, real network addresses and credentials.

## Claims and sources

| Landing-page claim | Source |
|---|---|
| Remote desktop for your own computers. A laptop or Android phone connects directly over your network. | [README introduction and platform matrix](../README.md), [getting started](../docs/get-started.md) |
| Share a computer, add its link or QR code, then open it. | [Getting started](../docs/get-started.md), [using Sunna](../docs/using.md) |
| The hero shows sample computers and a real Linux session. | [Mock computers](../apps/sunna/dev/mock.js), [session capture](capture/session.py), [viewer entry point](../apps/sunna-cli/src/main.rs) |
| Five views let you see multiple computers or focus on one. | [Computers UI](../apps/sunna/ui/computers.js), [view selection and launch](../apps/sunna/ui/main.js) |
| The Linux sharing switch reveals a link for another device. | [Linux host guide](../docs/host-linux.md), [sharing state and link row](../apps/sunna/ui/sharing.js) |
| Updates lets you choose a host. The host verifies a signed release and waits for an idle session before restarting. | [Updates UI](../apps/sunna/ui/updates.js), [update verification](../docs/update-verification.md), [release status](../README.md#updating) |
| The experimental whole-machine Linux host can show the greeter after reboot. LightDM and SDDM were tested in VMs. | [Linux host guide](../docs/host-linux.md), [test matrix and viewer captures](../tools/login-test/evidence/REPORT.md) |
| On Android, scan the code and tap a computer to connect. | [Android guide](../docs/app-android.md), [Android Computers screen](../apps/android/app/src/main/java/dev/sunna/app/Home.kt) |
| Small changes go ahead of video as exact tiles. The figure shows 2 ms and 35–37 ms, on the original six-core CPU-only loopback setup. | [Fast lane](../docs/using.md#the-fast-lane), original landing-page benchmark and method sentence retained in [index.html](../docs/index.html) |
| Mac and Linux support both roles. Android connects as a client. Windows client is planned and Windows host experimental. | [README platform matrix and verification caveats](../README.md#what-runs-where), [Mac host](../docs/host-mac.md), [Linux host](../docs/host-linux.md), [Mac app](../docs/app-mac.md), [Linux app](../docs/app-linux.md), [Android app](../docs/app-android.md) |
| Install from source today. Signed downloads and a one-line installer have been built and tested, pending the first public release. | [Getting started](../docs/get-started.md), [installer and signing tests](../docs/update-verification.md), [README release status](../README.md#updating) |

## Final measurements

All six runs recorded zero outside HTTP requests, zero cumulative layout shift and zero horizontal overflow. All local links and media references resolved across the twelve built HTML pages. The 1440 px captures are 5507 px tall and the 390 px captures are 6076 px tall. Normal motion played the hero. JavaScript-off and reduced-motion runs showed its paused poster with native controls.

The MP4 has one H.264 video stream at 24 fps in yuv420p. The four 15.5-second hero encodes are each smaller than their corresponding round 3 file. The sixteen proof frame indices are 0, 90, 98, 101, 104, 109, 139, 174, 209, 239, 302, 327, 330, 332, 344 and 371.

The homepage is 11,820 bytes of HTML plus 8,519 bytes of shared CSS. The inline JavaScript is 444 bytes gzipped. The local Inter font is 352,240 bytes. The built tree including guides and alternate encodes is 3,760,205 bytes. The browser selects a single hero source for playback.

| Media file | Bytes |
|---|---:|
| devices.webp | 25,244 |
| gallery.webp | 82,964 |
| hero.png | 402,144 |
| hero.webp | 260,678 |
| icon.png | 23,611 |
| recordings/android.webp | 52,532 |
| recordings/closeup.mp4 | 59,739 |
| recordings/closeup.webp | 50,200 |
| recordings/devices.webp | 105,734 |
| recordings/finished.webp | 72,390 |
| recordings/gallery.webp | 121,408 |
| recordings/hero-2x.mp4 | 645,734 |
| recordings/hero-2x.webm | 568,993 |
| recordings/hero.mp4 | 369,964 |
| recordings/hero.webm | 334,027 |
| recordings/machine-greeter.webp | 8,932 |
| recordings/poster.webp | 98,622 |
| recordings/progress.webp | 70,598 |
| recordings/same-viewer-desktop.webp | 6,042 |
| recordings/sharing-poster.webp | 21,102 |
| recordings/sharing.mp4 | 30,667 |
| recordings/space.webp | 117,040 |
| recordings/switch-off.webp | 3,166 |
| recordings/switch-on.webp | 11,442 |
| recordings/type.webp | 137,172 |
| recordings/updates-menu.webp | 73,982 |
| recordings/wall.webp | 136,176 |
| wall.webp | 69,096 |

## Process shutdown and retained work

The capture-owned host, viewer, X servers, desktop session, private D-Bus, terminal, shell, window managers, browsers, encoders, HTTP servers and both private SSH agents have stopped. The SSH agents from the two staging runs were identified by their capture-home sockets and stopped by PID. The reproduction script disables XFCE’s credential-agent startup. XFCE also contacted the system-managed GPG socket during staging. Its service remains under the user service manager’s control.

Final inspection found both capture display sockets absent and no listeners on TCP 8795–8798 or UDP 48860–48869. The static build used the external workspace. No package installation, commit, push or publication was performed.

Clean-up candidates are the private capture home and test target, browser profiles, session logs, raw recordings, lossless frames, rehearsal previews and temporary static build under `~/ext/work/site/r4`, plus the staged project under `~/ext/work/site/demo`. The proof PNGs are under `~/ext/work/site/shots/r4-*.png`. Round 3’s build tools and previous capture workspace remain available. The retired scene code, demo posters, and two switch evidence stills remain local.
