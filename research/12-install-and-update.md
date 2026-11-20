# 12 · Installing and updating Sunna

Status: design and work brief, 2026-11-18. Not built yet.

## The problem

Every part of Sunna is built from source on the machine it runs on. Installing a host means a Rust toolchain, a compiler and a few minutes of building; updating means `git pull` and building again, on every computer, by hand. There are no downloads, no version check, and no way to update a computer you can only reach through Sunna itself.

## What "done" looks like

1. **Install with one command, no toolchain.** `curl -fsSL <url>/install.sh | sh -s -- host` (or `app`) on Linux and Mac; an APK for Android; a PowerShell line for the Windows host.
2. **Update with one click or one command, or by itself.** `sunna-host update`, a button in the app, or automatic.
3. **Update a host from the app.** The computers you share are often headless or far away. The app shows which of your computers are out of date and can tell each one to update itself.
4. **Safe.** Only releases signed with the Sunna release key are installed. Switching versions is atomic. A version that fails to start is rolled back by itself. A host never restarts under someone who is connected.

## Design

### Releases

- One version number for everything: the workspace version in `Cargo.toml`. `scripts/release.sh X.Y.Z` sets it everywhere it appears (Cargo, `tauri.conf.json`, Android `versionName`/`versionCode`) and tags `vX.Y.Z`.
- A tag builds, in CI (`.github/workflows/release.yml`), and the same steps run locally through scripts under `scripts/release/` so they can be tested without GitHub:
  - `sunna-host-linux-<arch>.tar.gz`: `sunnad`, `sunna-host`, the desktop helper scripts.
  - `sunna-app-linux-<arch>.tar.gz`: the app (launcher and viewer), icons, desktop entry.
  - `Sunna-macos-<arch>.zip`: `Sunna.app`; and `sunna-host-macos-<arch>.tar.gz`: `sunnad` for the Terminal host.
  - `sunna-android.apk`, signed with the release key.
  - `sunna-host-windows-x86_64.zip` once the Windows host is committed.
  - `manifest.json`: version, date, notes, minimum protocol, and for each file its URL, size and SHA-256. `manifest.json.sig`: an Ed25519 signature of the manifest.
- **Linux builds must run on other people's distros.** Build against an old glibc (Ubuntu 22.04 or older in a container). The app depends on WebKitGTK 4.1 (leave to the system, name the package when it's missing) and FFmpeg (its library versions differ by distro: bundle LGPL FFmpeg libraries beside the binary, or load them at run time; decide by testing). Prove it in clean containers: Ubuntu 22.04 and 24.04, Debian 13, Fedora, Arch.
- The address compiled into every copy is one we own: the newest manifest is at `https://sunna.sunnydx.dev/releases/latest/manifest.json` (decided 2026-11-19; originally a GitHub Releases URL). Each release's files live under `/releases/<version>/`; `/install.sh` is the newest installer. Where the files physically live can change later without touching installed copies. The base URL can be overridden (`SUNNA_UPDATE_URL`) for self-hosting and tests.
- **The release key.** `scripts/release/keygen.sh` makes an Ed25519 key pair. The public half is committed (`crates/update/release.pub`) and compiled into every binary. The private half never enters the repository: `~/.config/sunna/release.key`, mode 0600, and later a CI secret. Tests use a separate throwaway key behind a cargo feature; a release binary accepts only the real one.

### Where things go (Linux)

```
~/.local/share/sunna/versions/0.3.0/{bin,lib,share}
~/.local/share/sunna/current -> versions/0.3.0        (flipped with an atomic rename)
~/.local/bin/{sunnad,sunna-host,sunna} -> ../share/sunna/current/bin/…
```

Updating unpacks a new version beside the old one and flips `current`. Rolling back flips it back. The two newest versions are kept. Installs made by the old from-source scripts (plain files in `~/.local/bin`) are migrated in place. The from-source scripts stay for developers and install into the same layout. A system package (`.deb`) can't be self-updated: the updater says so and names the package manager.

### The updater (`crates/update`)

One library used by the host, the app and the command line: fetch the manifest, verify the signature (Ed25519 through `ring`, already in the tree), download with resume, check the SHA-256, unpack, pre-flight the new version (`<new>/bin/sunnad --version`, so a binary that can't even start is caught before switching), flip, and remember state in a small file (last check, what's available, what's pending, the previous version, versions that failed).

### The host

- `sunnad update` and `sunna-host update`: `--check`, `--yes`, `--rollback`.
- While running as a service it checks once a day. Policy in `sunna.env`, `SUNNA_UPDATE=auto|notify|off`, default **notify**; `install.sh --auto-update` and the app can turn on auto.
- It applies an update only when nobody is connected. If a viewer asks while connected, it updates as soon as that session ends.
- After the flip it restarts itself (the systemd user service; re-exec when not under systemd). The new version counts its starts; once it has been listening for a short while it marks itself healthy. If it starts more than twice without getting there, it flips back to the previous version and records the failed one so it isn't tried again.
- **From the app:** an authenticated viewer can ask a host for its version and update state, and tell it to update. The host only ever installs what the signed manifest names: the viewer supplies no URL and no file. The host's version is included where the app already learns about a host without a session, so the Computers screen can show "Update available" on a computer.
- **Compatibility.** Hosts already installed run older builds that know none of this. New messages must not break talking to them (`PROTOCOL_VERSION` in `crates/proto` is 8; check how unknown messages and new fields behave before choosing how to add these). When the two sides are too far apart to talk, each says which side needs updating.

### The app (Mac and Linux)

- Checks at launch and once a day, downloads in the background, then offers **Restart to update**. Linux: the same versions folder and flip, then relaunch.
- Mac: there is no paid Apple developer account, so the app is signed on each Mac with a certificate that Mac made for itself ("Sunna Local Code Signing"; see `scripts/install-mac-app.sh`), which is what keeps Accessibility and Screen Recording permissions across updates. Installing and updating from a download keep that: unpack, sign locally with the same identity (creating it the first time, as the script does), swap `/Applications/Sunna.app`, relaunch. Files fetched by `curl` or by the app itself carry no quarantine flag, so Gatekeeper doesn't block them.
- The app's update commands are exposed to the UI (Tauri commands in `apps/sunna/src/update.rs`) but **the screens for it are a later step**: the UI is being rebuilt by another agent right now. Document the commands and their results at the top of that file.

### Android

Settings → **Check for updates**, and a check at launch once a day: fetch and verify the manifest (verification in the Rust library through JNI), download the APK, check its hash, hand it to the system installer, which asks the person to confirm. Needs the "install unknown apps" permission flow. An update only installs over an app signed with the same key, so say clearly when it can't (a debug build can't be updated by a release build).

### Windows

Same model: `install.ps1`, a versions folder, the scheduled task pointing at `current`, self-replace by rename. The Windows host is still uncommitted work, and its display driver needs separate care. Write the installer and the updater's Windows paths so they compile and are reviewed, but treat Windows as untested and say so.

## Work brief (for the agent building this)

Build everything above. Notes:

- **Read first:** `README.md`, `docs/*.md`, `scripts/*.sh`, `scripts/sunna-host`, `apps/sunnad/src/main.rs`, `crates/proto/src/`, `crates/host/src/lib.rs`, `crates/client/src/reach.rs`, `apps/android/README.md`, `apps/android/app/src/main/java/dev/sunna/app/Settings.kt`.
- **Another agent is rebuilding the desktop app's UI in this same checkout right now.** Its files: `apps/sunna/ui/**`, most of `apps/sunna/src/**`, `crates/viewer/**`, `apps/sunna/dev/**`, `docs/using.md`. Don't edit those, except: add `apps/sunna/src/update.rs` and register its commands in `main.rs` with the smallest possible edit (re-read the file right before editing). Same care for `Cargo.toml` and `Cargo.lock`. It will touch a few device-kind lines in `apps/sunnad/src/about.rs`; leave that file alone.
- The tree has a lot of uncommitted work that isn't yours (the Windows host among it). Don't commit, stash, reset, checkout, or reformat files you aren't changing. No workspace-wide `cargo fmt`.
- **Nothing leaves this machine.** Don't push, tag, create releases, or upload anything. Write the CI workflow; it can't be run here, so keep its steps thin wrappers over the `scripts/release/` scripts that you *have* run.
- **The real release key:** run `keygen.sh` once, leave the private key at `~/.config/sunna/release.key` (0600), commit only the public key file, and never print or copy the private key anywhere, including your report.
- **This box:** don't touch `sunna-host.service`, `sunna-desktop.service`, displays `:1`, `:12`–`:27`, `:40`, or other people's containers. No `pkill -f`. Test in your own Docker containers and remove them when done. Don't reach out to the user's other machines.
- **Prove it, end to end, with a local release server and the test key:**
  1. Build release A and release B (different versions) with the release scripts.
  2. In clean containers (Ubuntu 22.04, 24.04, Debian 13, Fedora, Arch): `install.sh host` installs A with no toolchain present and the host starts (a virtual display is fine); `install.sh app` installs and the app binary starts under Xvfb.
  3. `sunna-host update` takes A to B. So does the automatic path. So does a request from a client over the wire.
  4. A manifest with a bad signature, a file with a bad hash, and a truncated download are each refused, and A keeps running.
  5. A deliberately broken B (fails at start) is rolled back to A by itself and isn't retried.
  6. An update requested during a session waits for it to end.
  7. An install made by the old `scripts/install-host-linux.sh` is migrated by the installer and can then update.
  8. A new host talks to an old (current `main`) client and the other way round.
  9. The Android app builds with the update code; unit-test what can be tested off-device.
- **Docs:** rewrite the install and update parts of `README.md` and `docs/{host-linux,host-mac,app-linux,app-mac,app-android,troubleshooting,building}.md` in the repository's plain style. State only what you verified; mark Mac and Windows paths as not yet run on a real machine.
- **Report:** what you built, what each test above showed (with the commands), what you could not test, every decision you made where this document left a choice, and what the owner must do before the first public release (key into CI secrets, Android key, first tag).
