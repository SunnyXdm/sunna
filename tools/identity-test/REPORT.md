# Host identity verification

Tested on Linux on 2026-11-19. The deployed side was built from `1c46c9d4585bc893981eefb98c7ce197cb601bb2`. Each network test used a local process, an isolated desktop or a disconnected guest. Credentials were generated for the fixtures.

## Implementation

The daemon saves its certificate and PKCS#8 key together. Unix opens check ownership, file type and permissions. Creation, replacement and first-use pinning use atomic file operations. Linux stores the identity in the user's Sunna configuration or in `/etc/sunna`. The macOS host uses the app's Application Support directory. The Windows service uses the LocalSystem profile. A standalone daemon without a configured key generates a private 128-bit key beside its identity.

The verifier compares the SHA-256 digest of the DER SubjectPublicKeyInfo during TLS and checks the handshake signature. Both peers use Quinn's 32-byte TLS exporter and ring's HMAC-SHA256 with separate client and host transcripts. Nonces must be 32 bytes. Link identities are unpadded base64url. The new authentication messages are appended enum variants. Probe, Hello and UpdateRequest keep their protocol-8 wire layouts.

A record has an optional pin, an upgrade flag and an explicit older-computer setting. A per-record lock and private state file carry the decision between the launcher, its native viewer subprocess and reconnects. A first successful proof commits the pin before sending the requested operation. A stale snapshot cannot re-enable legacy access after the pin is written. Android uses the same Rust admission code with its app-private directory.

## Compatibility

| Viewer and saved record | Host | Probe, session and update |
| --- | --- | --- |
| Deployed | Deployed | Original token-based probe and session work. No identity, proof or remote update operation. |
| New, no pin, default settings | Deployed build | TLS negotiates the old protocol. No control request is sent. The app shows **Needs an update** with local installation guidance. |
| New, no pin, previously upgraded | Deployed build | Proof remains required. Even a stale enabled switch cannot send a token. |
| New, never verified, switch on | Deployed build | Probe and session succeed and show **Not fully verified**. The legacy request sends the key inside TLS. The deployed host has no remote update capability, so the viewer asks for a local install. |
| New, pin present | Deployed build or another identity | A different public key fails the TLS handshake. A matching public key would still need to support proof. |
| New, with or without a pin | New | Both sides prove the key. Probe, session, handoff and update use empty token fields. First proof or adding a link with an identity disables the exception for that record. |
| Deployed | New, legacy clients allowed | Deployed probe and session work. The host counts established older sessions and records them for local status. This deployed binary has no update-request operation. A separate wire test exercises a token-based update request on the new host. |
| Deployed | New, proof required | The host refuses before returning computer details, video or update results. Linux system mode and the Windows service default to requiring proof. |

The default changed for never-upgraded records following the revised section 5. TLS negotiates `sunna/8/auth1` ahead of the deployed `sunna/0`. This distinguishes an old endpoint without opening an application stream on the wire. It also avoids retrying a failed proof as a legacy connection. Quinn establishes a stream on the wire only when its sender writes. The pin parser uses the existing rustls-webpki dependency for its SPKI accessor. No new cryptographic provider or primitive was added.

A negative answer cannot authenticate a first-use peer. A modern endpoint that explicitly rejects the client proof can produce the same “key doesn't match” message as a genuine host with another key. An invalid host proof produces **Identity changed**, and a pinned certificate mismatch does so during TLS. All three paths stop before the requested operation. A silent peer can time out without revealing which software it runs.

## Numbered proofs

1. **Key handling and channel binding.** The initial transport fixtures failed to compile against the old transport because the pin and proof APIs were absent. The deployed message definitions and call sites populate the token directly. The new tests examine the serialized authentication envelope, verify correct proofs on a real QUIC connection and reject both proofs on a second, independent connection. A bad host acknowledgement causes the real client to report the identity refusal and close without sending its probe. The successful control fixtures check empty token fields in Probe, Hello and UpdateRequest. The real-binary matrix checks old-host refusal by default and the explicitly marked legacy session. The transport fixture `sign_in_does_not_pass_through_a_relay` opens two independent loopback QUIC connections. It checks a pinned handshake rejection, rejection of a proof forwarded between those connections, and rejection of a host acknowledgement from another connection. The deployed-host exception deliberately sends the full key to the endpoint. The public binary matrix exercises genuine endpoints and stale viewer snapshots.

   ```sh
   cargo test -p sunna-transport --test identity
   cargo test -p sunna-client --lib
   cargo test -p sunna-host --test live_stream
   python3 tools/identity-test/compat.py "$old/target/debug" target/debug
   ```

2. **Persistence, migration and rotation.** Previously the daemon generated a certificate on every launch. The storage fixture now checks restart, deliberate replacement and refusal to overwrite a corrupt file. The real-binary test restarts the daemon, changes its identity, checks rejection and re-adds its new link. A disconnected Ubuntu 24.04 LightDM guest served a real greeter, updated from signed A to B and rolled back a deliberately broken signed candidate. Its public identity matched before and after, and the same saved viewer decoded another 640×400 picture. The successful post-update and post-rollback frames took 3.8 and 3.5 seconds. An initial cold guest run timed out on its first probe. Subsequent probes and frame reads succeeded. In a separate migration, the viewer decoded a 640×360 per-user frame, the previous owner ran the signed system installer, and the same saved viewer decoded the LightDM greeter in 4.1 seconds. The fixture checked both fields of the system link, root ownership and mode 0600, and removal of the previous owner’s identity file. The first migration attempt used a guest that still had the earlier rollback fixture’s version floor. The installer refused the older package. The successful run preserved that fixture separately and started with no system installation.

   ```sh
   tools/login-test/test-releases.sh "$feed"
   SUNNA_LOGIN_PREFIX=sunna-id SUNNA_LOGIN_IMAGE=sunna-id-guest-base \
     SUNNA_LOGIN_FEED="$feed" SUNNA_LOGIN_CLIENT="$client" tools/login-test/boot-vm.sh lightdm 24.04
   docker exec "$vm" guest /usr/local/bin/install-guest
   docker exec "$vm" guest /usr/local/bin/check-guest /tmp/confirmed.y4m
   docker exec "$vm" guest /bin/bash /root/prove-guest.sh update
   docker exec "$vm" guest /bin/bash /root/migrate-guest.sh
   ```

3. **Compatibility.** Both sides of the table above used the actual QUIC clients and hosts. The old viewer exercised the deployed client's library with its Probe, Hello and H.264 decoder. The new viewer decoded frames and used its actual update entry point. The four combinations of viewer version and host version were checked on loopback. The guest also exercised the older viewer with the system setting explicitly relaxed, including the login manager going away. The release proof separately checked authenticated remote update and deferral while a session is active. The deployed commit cannot originate a remote update. A raw legacy UpdateRequest fixture on the new host distinguishes an invalid key from an authenticated operation.

   ```sh
   git archive HEAD | tar -xf - -C "$old"
   mkdir -p "$old/crates/client/examples"
   cp tools/login-test/old-viewer.rs "$old/crates/client/examples/"
   CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0 cargo build -j4 \
     --manifest-path "$old/Cargo.toml" -p sunnad -p sunna-client --bin sunnad --example old-viewer
   cargo build -p sunna -p sunnad -p sunna-client --features custom-protocol --bins --examples
   python3 tools/identity-test/compat.py "$old/target/debug" target/debug
   docker exec "$vm" guest /bin/bash /root/compatibility.sh
   ```

4. **Rejection and backoff.** The deployed protocol had no proof or pin to check. The transport tests now reject a wrong pin, wrong key, weak key, replay on a separate connection, and nonces of 0, 31, 33 and 256 bytes. The host's three-strike source limit is exercised with wrong or malformed proofs, prior-connection proofs and TLS certificate alerts. The refusal, retry budget and shared deadline use the existing admission gate. Proof equality uses `ring::hmac::verify`.

   ```sh
   cargo test -p sunna-transport --test identity
   cargo test -p sunna-host --test live_stream sign_in_budget
   cargo test -p sunna-host --test live_stream a_proof_from_another_connection
   ```

5. **Links and QR codes.** Previous links only carried a key. The Linux host's real terminal QR now decodes to the exact printed link including address, key and identity. Rust tests round-trip the shared Mac link formatter, IPv6 and escaped keys, accept old links and reject malformed or duplicate identities. Android JVM tests exercise link parsing, the exception's eligibility and both status labels. The native Mac app and a physical Android camera were not exercised.

   ```sh
   python3 tools/identity-test/qr.py
   cargo test -p sunna-client links_preserve_the_pin_or_fail_closed
   bash apps/sunna/dev/verify-sharing-mac.sh
   (cd apps/android && ./gradlew --offline :app:testDebugUnitTest :app:assembleDebug)
   ```

6. **UI.** Real Tauri/WebKitGTK used the packaged page and its normal security policy on a private Xvfb display. Three local records produced **Identity changed**, **Not fully verified**, and **Needs an update**. The full-size evidence includes all five launcher layouts and the Edit sheet in dark Devices and light Space. The changed-identity computer has no picture. Edit shows a short identity for that record and a switch for the never-verified one. The first layout put the Save button partly below the sheet. The fields now scroll independently and the footer stays visible. The Android APK builds for arm64 and x86-64. A software Android emulator reported boot completion but repeatedly lost the package service or timed out installing the APK. Android visual evidence is outstanding.

   ```sh
   DISPLAY=:68 python3 tools/identity-test/views.py "$old/target/debug/sunnad" target/debug "$out"
   node apps/sunna/dev/verify-states.mjs
   SUNNA_NATIVE_ADDRESS=127.0.0.1:49853 SUNNA_NATIVE_DISPLAY=:69 \
     SUNNA_NATIVE_OUTPUT="$out" python3 apps/sunna/dev/verify-native.py
   ```

   [Devices](evidence/identity-devices-changed.png) · [Wall](evidence/identity-wall.png) · [Space](evidence/identity-space.png) · [Type](evidence/identity-type.png) · [Gallery](evidence/identity-gallery.png) · [Edit in Devices](evidence/identity-devices-edit-pin.png) · [Edit in Space](evidence/identity-space-edit-legacy.png)

## Verification from the preceding identity review

- `cargo test --workspace` passed 191 tests. Nine pre-existing hardware, X11 and timing tests are marked ignored by the suite.
- All seven desktop `verify-*.mjs` scripts passed against the development fixture on private ports. The states check captured 59 states. `verify-native.py` exercised all five layouts, opening and leaving the native viewer, the toolbar and all three saved-picture settings. Its fixture host and display were private to this run.
- `verify-sharing-linux.sh` passed the XFCE and GNOME signed install and reconnect flows. `SUNNA_SHARING_REVIEW_ONLY=1` passed the additional WebKit checks for the helper, external services and system receipts. The test package's FFmpeg libraries accompanied the test executables.
- `python3 scripts/release/prove-update.py "$work" "$work/releases/A/.proof-client"` passed all 16 cases using signed test releases and the previous commit's binaries. The standalone proof drivers need the package's FFmpeg libraries. The runner now takes those libraries from A's test archive. `python3 scripts/release/test-publication.py` passed five tests.
- `CC_aarch64_apple_darwin="$HOME/.local/bin/zcc-aarch64-macos" CARGO_INCREMENTAL=0 cargo check --target aarch64-apple-darwin -p sunna -p sunnad -p sunna-host -p sunna-capture` passed. The Mac sharing script also ran 12 platform-independent tests on Linux. `cargo check --target x86_64-pc-windows-gnu -p sunnad` passed with temporary Zig C/C++ wrappers targeting Windows GNU. Neither platform was run natively.

## Part 2 final review, 2026-11-19

The red baselines used the uncommitted pre-review application. The protocol-8 compatibility baseline used a separate build of commit `1c46c9d4585bc893981eefb98c7ce197cb601bb2`.

| Item | Change and regression evidence |
| --- | --- |
| 2.1 | Desktop `ui/main.js` and Android `MachineSheet.kt` invalidate the pending check before starting the debounce. Saved and imported identities are separate from the latest check. Changing addresses clears an imported link identity. `verify-draft.mjs` originally saved A's identity after the address changed to B. It now submits no pin or metadata from A after an address or key edit. The Android JVM fixture checks both field changes, including an already accepted answer and a late reply. Android background learning also checks the original record inside the store lock. |
| 2.2 | Desktop `machines.rs` and Android `Machines.kt` use the same Rust record transaction in `client/src/auth.rs`. The failed-Save fixture initially left the old record with the replacement pin. The original pin now survives the failure. Recovery tests interrupt both sides of the record rename. Restoring the old list with a conflicting newer sidecar refuses admission. A restored old list without that sidecar uses its own pin. A complete older backup restores the earlier decision, including any legacy exception. |
| 2.3 | The desktop Edit sheet and Android switch warn that even background checks send the full key to whoever answers as an older computer. `reach.rs` marks only ready and busy legacy probes as unverified. The JVM status test originally showed the warning over a wrong-key failure. The deployed-host wrong-key check now returns `wrong-key` with the warning flag clear. Native viewer GPU, layer and CPU presenters and the Android session keep a short label when statistics are off. The Linux capture shows the label against live video. |
| 2.4 | The shared desktop preference writer originally replaced a key generated after Settings opened. `settings::view_tests::preference_save_preserves_generated_key` failed, then passed after key edits moved to the dedicated Tauri `set_key` command. Both Linux and Mac launchers used this writer. Linux host management also has separate host settings. Preference commands now omit the sheet's key, and explicit edits use an owner-private staged file. |
| 2.5 | `mac_host/model.rs` formerly skipped checks indefinitely after one success. `expired_success_requires_a_new_frame` failed at the next scheduled check. A success now expires at 60 seconds. Active polling can start a refresh at 50 seconds while the previous observation is valid. The second model test covers a stalled refresh. Failures retain the existing 10-second cooldown. The Mac procedure covers display loss and recovery with unchanged grants. |
| 2.6 | The original clipboard and audio branches lacked a close check. Both use the same delivery gate after dequeue. A real QUIC datagram is eligible while connected and discarded after close. The clipboard worker also checks immediately before the platform write. Its test queues a copy behind a paused fake backend and verifies both the open and closed outcomes. This is the client half of the seat-change finding. |
| 2.7 | Android originally ended after one client run. `crates/android/src/lib.rs` now reconnects on the desktop viewer's handoff signal using saved trust. A Rust loopback test requires two successful proofs and two token-free Hello messages, then ends the replacement normally. Each attempt owns its input route, decoder and player. Handoff invalidates queued Java clipboard work, discards pending remote clipboard data, resets held input, and covers the old surface while waiting for the next picture. |
| 2.8 | The Windows service previously stored forwarded arguments and the console launcher logged the reconstructed command. The platform-neutral validator tests reject both spellings of the credential option, including an empty value. The service child uses its account's host-key store even if an environment key is present. The launcher log records the log path only. The wiring test failed on the old source. Microsoft documents the service executable and arguments in the persisted ImagePath. [CreateServiceW](https://learn.microsoft.com/en-us/windows/win32/api/winsvc/nf-winsvc-createservicew) |
| 2.9 | Added an OpenSSL-derived known fingerprint, same-key certificate reissue, changed-key and invalid-signature checks, concurrent first creation, Unix symlink and mode refusal, host nonce and tag lengths, role substitution, and five invalid acknowledgements through the public client. Each invalid acknowledgement leaves the record unpinned and sends no operation. Separate viewer processes exercise the first proof and disabling the exception while another viewer holds a stale snapshot. The transaction tests cover failure, crash boundaries and backup combinations. Several underlying checks were already correct and gained coverage without an observed pre-fix failure. The existing transport relay passes. The public-binary matrix covers genuine hosts, substituted endpoints and all five saved-policy rows. A TLS-terminating relay matrix through both public entry points, including legacy credential interception, remains untested. |
| 2.10 | The former idle-capture description failed the disclosure check. `docs/security.md` now covers the short local Linux and Mac test frames without a viewer, first proof, pinning and background legacy checks. Using, troubleshooting and the Mac test procedure cover the user-facing changes. Apple distinguishes complete, idle, blank and stopped display-stream updates. [CGDisplayStreamFrameStatus](https://developer.apple.com/documentation/coregraphics/cgdisplaystreamframestatus) |

### Shared interfaces

The wire format and ALPN names are unchanged. The desktop adds `set_key`. The Android store calls `Native.saveRecords` in place of separate pin and exception writes. `Trust::save_records` holds the same per-record lock used by proof and legacy admission. It stages a private replacement list and journals the intended policy. Renaming the list commits the edit. Recovery under that lock checks whether the stage still exists and finishes or discards the pending decision before admitting an operation. Android also exposes the session generation to clipboard callbacks. The client uses the new `ClipboardSession::start_guarded` entry point so the worker can check the connection at the actual write. Existing clipboard callers keep their previous entry points. Windows service startup adds a private child flag to select the protected key store.

### Verification commands

The final workspace run passed with nine ignored tests. All eight Android JVM tests passed and the debug APK assembled. The desktop harnesses used isolated homes, synthetic hosts, separate Chromium profiles and private Xvfb displays. The browser scripts ran from copies with a private screenshot directory. The sharing script copy used `sunna-fix5-` image and container names. The release fixtures used generated signing keys.

```sh
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --workspace
python3 tools/identity-test/review_checks.py
cargo build -p sunna -p sunnad -p sunna-client --features custom-protocol --bins --example update-proof
git archive HEAD | tar -xf - -C "$old"
mkdir -p "$old/crates/client/examples"
cp tools/login-test/old-viewer.rs "$old/crates/client/examples/"
CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0 cargo build --manifest-path "$old/Cargo.toml" -p sunnad -p sunna-client --bin sunnad --example old-viewer
python3 tools/identity-test/compat.py "$old/target/debug" target/debug
(cd apps/android && ./gradlew --offline :app:testDebugUnitTest :app:assembleDebug)
bash apps/sunna/dev/verify-sharing-mac.sh
CC_x86_64_pc_windows_gnu="$wincc" CXX_x86_64_pc_windows_gnu="$wincxx" AR_x86_64_pc_windows_gnu="$winar" cargo check --target x86_64-pc-windows-gnu -p sunnad
```

The Mac script ran the model and capture-stop tests on Linux, then checked the app, daemon, host and capture crates for `aarch64-apple-darwin`. The Windows check used Zig targeting Windows GNU. The Android build compiled arm64 and x86-64 native libraries. Native macOS, native Windows and an Android device were unavailable.

The desktop runs covered the following entry points.

```sh
node verify-draft.mjs
node verify-actions.mjs
node verify-computers.mjs
node verify-states.mjs
node verify-sharing.mjs
node verify-updates.mjs
node verify-updates-edges.mjs
node verify-updates-policy.mjs
python3 verify-native.py
python3 verify-picture-removal.py
python3 verify-updates-native.py
python3 verify-updates-remote.py "$update_fixture"
python3 tools/identity-test/views.py "$old/target/debug/sunnad" target/debug "$captures"
bash verify-sharing-linux.sh --build-fixture "$sharing_fixture"
bash verify-sharing-linux.sh xfce "$app" "$feed" "$viewer"
bash verify-sharing-linux.sh gnome "$app" "$feed" "$viewer"
SUNNA_SHARING_REVIEW_ONLY=1 bash verify-sharing-linux.sh xfce "$app" "$feed" "$viewer"
```

Real WebKit rendered all five layouts, persisted each selection, opened and closed a native viewer, and verified sharp, blurred and disabled picture storage. The signed remote update changed the test host from 0.1.0 to healthy 0.2.0. XFCE and GNOME reconnected after the launcher closed and after the guest restarted. The first GNOME run reported a busy previous viewer. The release proof client now supplies its stable device ID and lets Quinn flush the close before exiting. The repeat passed. The browser matrix captured 172 computer layouts, 59 additional states and 100 update screenshots. The final Edit captures were inspected at their native 980 × 660 size. [Dark Devices](evidence/review-devices-edit-legacy.png) · [Space](evidence/review-space-edit-legacy.png) · [Legacy session with stats hidden](evidence/review-legacy-stats-off.png)
