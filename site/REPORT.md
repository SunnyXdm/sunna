# Landing-page media, round 5

The launcher opens `ubuntu`. Its XFCE desktop, terminal, five passing protocol tests and Disconnect menu come from the native loopback session recorded in round 4. Round 5 recorded the launcher opening and returning, the five Computers views, the update menu and statuses, and the guide stills in the running UI.

| Computer | What appears |
|---|---|
| debian | Debian 13, MATE and a LibreOffice planning sheet. Ready. |
| thinkpad | Debian 13, GNOME Files and the capture notes. Ready. |
| studio | Debian 13, KDE Plasma and Krita with the Earthrise study. In use from MacBook Pro. |
| ubuntu | The live Ubuntu XFCE terminal and Thunar. Ready. |
| macbook | macOS laptop, offline, last seen three hours ago. The app draws its fallback. |
| windows | Windows 11 desktop with a mismatched key. The app draws its lock. |

The three Debian sessions are documented in the [desktop frame inventory](../apps/sunna/dev/design/frames/README.md). A 3:2 Windows display lets Wall fit the six computers into three rows at 980 × 660. The guide update screenshot uses the product’s ready state for one computer without a saved picture.

## Reproduction

Run from the repository root after checking the four loopback endpoints are free. The capture driver keeps its browser profiles and lossless frames in the private workspace.

```sh
w="$HOME/ext/work/site/r5"
export TMPDIR="$w/tmp" SUNNA_SITE_OUT="$w/build"
mkdir -p "$TMPDIR" "$HOME/ext/work/site/shots"
cp "$HOME/ext/work/site/r4/"{before.png,after.png,result.png,session.mkv} "$w/"
python3 site/capture/fixture.py > "$w/fixture.log" 2>&1 &
fixture=$!
node site/capture/app.mjs features && node site/capture/app.mjs
kill "$fixture"
bash site/capture/encode.sh
magick "$HOME/ext/shots/add.png" -quality 82 docs/images/recordings/android.webp
python3 site/capture/proof.py
node scripts/site/build.mjs
python3 apps/sunna/dev/serve.py 8797 127.0.0.1 "$SUNNA_SITE_OUT" > "$w/page.log" 2>&1 &
page=$!
node site/capture/verify.mjs
kill "$page"
```

The regenerated terminal close-ups match the originals byte for byte. The sharing switch, sharing film and SDDM stills retain their round 4 captures. The unused, ignored Android file now shows the empty Add a Computer form from an archived native capture. Its original build number is unknown.

The round 4 site verifier stopped on a removed radio ID. The updated verifier exercises the current view and update radio groups and scroll-triggered sharing playback. The six page proofs cover JavaScript on, off and reduced motion at 1440 and 390 pixels. The hero contact sheet samples the delivered double-resolution MP4. Native-size proofs are under `~/ext/work/site/shots/r5-*`. Tesseract was unavailable, so the privacy check used visual inspection.

All four hero videos are 16.08 seconds, half a second longer than before. Each is smaller. The animated README WebP is 14,338 bytes larger. Every checked image and video retained its original dimensions. The six private-build checks reported zero layout shift, horizontal overflow and outside requests.

The full update matrix produced 96 screenshots both before and after. Both runs then stopped at the known toolbar-order assertion, as did both flow-only runs. All seven edge checks passed before and after. Test copies under `r5/matrix` redirect the existing scripts’ server, CDP ports and screenshot output into the private setup. The proof path list is `~/ext/work/site/shots/r5-proof-files.txt` and measurements are in `~/ext/work/site/r5/media-measurements.txt`.
