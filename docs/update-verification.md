# Install and update verification

Linux x86_64, 18 November 2026. Two locally signed releases, A (0.1.0) and B (0.2.0), exercised installation, remote updates, interruption, restart and rollback. On five distributions both the host and desktop app started through their installed launchers. A real systemd user manager recovered a reachable daemon after each of 38 process kills across 19 switch boundaries. The public release workflow has not run.

## Host availability

| Finding | Implementation | Before and after |
|---|---|---|
| A1. Exit or hang before readiness | `crates/update/src/bin/sunna-update.rs`, `crates/update/src/{lib,host}.rs`, `scripts/sunna-host`. A probationary child gets a unique readiness path. Its PID and selected version must match. The supervisor allows 60 seconds, kills and waits for a timed-out child, and counts early success as a failed start. The user unit uses `Restart=always`. | On the baseline helper, `test-recovery.py` saw the nonzero case recover, the exit-zero case leave B selected, and the hang exceed the fixture timeout. The revised helper restored A in all three cases and ignored a confirmation from another PID. |
| A2. Interrupted selection or unreadable state | `crates/update/src/lib.rs`. Sync the staged files and directories before renaming the tree and recording the transaction. The versioned state reader accepts the existing format. Recovery sets invalid records aside, checks a journal target’s executable and version, and consults a separately persisted known-good version. Migration also syncs the imported tree. If state repair cannot write, the supervisor can serve a validated known-good host with updates disabled for that run. | Corrupt JSON and a missing staged version prevented the baseline helper from running A. A newer state format was also mishandled. All three fixtures now run A. During this review an additional read-only/corrupt-state fixture failed with EACCES. It now starts the known-good host without changing the pointer. `test_switch_boundaries` materializes 19 states from a partial download to journal removal, including temporary files, renames and directory syncs. `prove-systemd.py` tests them again with each of the two processes killed. |
| A3. Commit failure | `crates/update/src/lib.rs`, host worker and desktop restart command. Record both sides, write the proposed state, then replace the pointer. Restore the prior state on flip failure. Reconcile before prepare, activation and rollback. A surviving worker can identify its side of a transaction. | `failed_state_commit_keeps_running_pointer` initially found B selected after state replacement failed. It now finds A. In `prove-full-disk.py` a 64-inode tmpfs runs out of inodes between the journal and state write. The pointer and state agree after the kernel returns ENOSPC. Freeing space allows a retry. |
| A4. App restart path | `crates/update/src/lib.rs`, helper, `apps/sunna/src/update.rs`. Preflight the real program, packaged helper and app launcher. The running supervisor handles restart code 75. | The earlier implementation only checked the real program. Signed B fixtures individually omit the helper and the launcher. Each fails preparation and leaves app A selected. The Linux package starts through the actual launcher in the distro proof. |
| A5. Health and lock contention | `apps/sunnad/src/main.rs`, host worker, desktop startup and supervisor. A probationary host binds, opens its configured capture backend, gets a frame and opens the input backend for a screen source. A transient no-picture result waits within the supervisor deadline. The desktop signals from Tauri setup. The supervisor retries writing health while the confirmed child lives. | The retained baseline helper failed the health-write fixture. With the new helper, a child holds the update lock past the test startup deadline and is still confirmed after unlock. The live migration proof uses real X11 capture/input on a private Xvfb display. With XTEST disabled on a second private Xvfb display, the last-commit daemon still answered a real probe. B exited on the unsupported input extension before signaling readiness. Synthetic-source proofs exercise the listener and encoder. |
| A6. Request durability | `crates/update/src/{lib,host}.rs`. Store the action, peer constraint, request number and result in state. Commit consumes the request. An older result cannot clear a newer request. Recognize legacy request markers. | The baseline rollback test left its marker behind. The revised test passes. The functional proof holds an HTTP artifact response, terminates the requesting host and supervisor, then starts them again. B installs once and the request clears. |
| A7. Running legacy installation | `scripts/release/install.sh.in`, `scripts/sunna-host`, helper. The generated installer detects an active user unit. It prints the migration command or runs it when given `--restart`. The command rewrites/reloads the unit and checks the supervisor, child executable and health. | The previous proof only migrated files. `prove-migration.sh` starts a real last-commit daemon under systemd. Installation without restart preserves its PID and prints the command. Installation with restart replaces it, and authenticated status reports a healthy managed A. |
| A8. Background results and app policy | Host worker, desktop supervisor, `apps/sunna/ui/{index.html,updates.js}`. Authenticated status carries the background error. Automatically activates a staged release on the next launch. Ask first waits for an explicit restart. Never disables scheduled checks. | The baseline helper failed the automatic-launch assertion. All three launch-policy fixtures now pass. Bad signature, hash and truncated downloads each leave A reachable and return an error in host status. UI tests exercise all three saved choices. |

A normal healthy child does not undergo probation on every launch. Automatic rollback covers a newly selected release with a previous installation. A fresh installation has no previous release to restore. The process-kill tests use materialized filesystem states and real processes. They do not measure drive firmware behavior during physical power loss.

## Signed releases and publication

| Finding | Implementation | Verification |
|---|---|---|
| B1. Files after verification | `crates/update/src/lib.rs` and `check-root.ps1`. Check ancestor ownership/access, use private staging and exclusive temporary-file creation, refuse symlinked partial files, hash and rewind the same file handle used for extraction, and check the selected install tree before launch. | The shared-root test failed before the change. It now refuses the lock. The partial-file test refuses a symlink without changing its referent. Normal cached, truncated and resumed transfers pass. The Windows caller cross-compiles. The ACL script still needs a Windows runtime test. |
| B2. HTTP and redirects | The Rust fetcher follows a bounded number of redirects itself and checks each destination before requesting it. Production requires HTTPS. Test builds allow numeric loopback HTTP. Both bootstrap templates constrain curl’s initial and redirect protocols. | The baseline attempted a plain HTTP metadata fetch. The new metadata and redirect fixtures reject an untrusted HTTP destination before fetching it. The local feeds use the test feature and real signatures. |
| B3. Replay | The helper records the highest accepted version independently of selection. It seeds the floor from installed history or the bootstrap version and probes a migrated program for its version. | A signed 0.1.5 is refused after accepting 0.2.0 and explicitly rolling back to 0.1.0. A further regression test corrupted state after an explicit rollback. Recovery initially selected the release the operator had just rolled back. Updating the known-good record in the transaction and its recovery path made the test keep the operator’s selection and the higher replay floor. The migration test retains the previous binary under a legacy version label. |
| B4. Extraction bounds | The updater caps entries at 16,384, bounds decompressed tar input including headers, iterates raw tar entries and refuses extension records. The ZIP writer counts actual copied bytes and compares the result to the declaration. | A tar of repeated empty directories exceeds the entry budget and is rejected. Archive-link and real release extraction tests also pass. The tar fixtures do not need billions of bytes or inodes. |
| B5. APK installation | `apps/android/app/src/main/AndroidManifest.xml`, `Updates.kt` and `InstallResult.kt`. Use a PackageInstaller session with an explicit private result activity. The session requests user action where the API supports it. The old APK content provider is no longer registered. | The debug APK builds, and the JVM tests check the approved session byte stream and refusal of changed bytes. The Android results below describe the device test. |
| B6. Android consent | `crates/android/src/lib.rs`, `Native.kt`, `Updates.kt`. Carry the offered version and digest through the download, compare them to signed metadata, use a digest-specific cache file, check package identity/version/signers, and hash the stream written to the installer. Save permission-flow consent only when opening Settings and consume it on return. | The off-device test rejects changed bytes before commit. See the Android results below for the changed-feed and cancel cases. |
| B7. Release keys | `crates/update/build.rs`, `manifest.py`, `check-updaters.py`, `check-apk-key.py`, Linux/macOS/Android packaging scripts. Public packaging forbids the test feature. Signing compares the Ed25519 public key derived from the supplied secret with the expected public file. | A locally generated wrong signing key is refused. The production Linux helper, daemon and app reject a throwaway-signed manifest with the expected embedded key. The script requires all bundled helper copies to match. A test-key release fails that publication check. The APK check reads the actual verifier-key symbol from the loadable ELF segments for both ABIs. This is an key check for Android. Executing the signature challenge on both mobile ABIs remains unverified. |
| C1. Signing inputs | `.github/workflows/release.yml`, `inventory.py`, `manifest.py`. Pin actions by full revision. Keep the downloads separated by job until filenames, counts and regular-file checks pass. Generate bootstrap cases from allow-listed targets. Read-only contents permission applies to builds. Only publish receives write permission. Android signing and publication select the releases environment. | The baseline generator emitted a manifest for an unlisted helper. The new generator rejects it. The inventory test rejects an APK delivered by a Mac job even though the global list of names is correct. The signing wrapper refused a mismatched secret even with a test-key environment override. In an isolated source fixture with the matching throwaway public key, the wrapper produced a verifiable signature. Actionlint passes. Required reviewers must be configured on GitHub. |
| C2. Version agreement | `version.py --check` verifies the tag version, Cargo workspace, Tauri version and Android name/code before all build jobs. Linux restamping is reserved for test snapshots. | The baseline lacked this validation command. The mismatch fixture fails with the expected diagnosis. The current 0.0.1 checkout passes. |
| C3. Minimum peer protocol | The manifest field is the lowest peer protocol the release supports. Host state records the offered and staged requirements. The client checks the advertised minimum. The host checks a request and checks newly fetched metadata during preparation. The versioned action is used only when advertised. | With a signed minimum of 9, the protocol-8 client returns “update the app first” and the host request counter stays at zero. A direct host request below the minimum also fails. An older updating client’s plain update action implies protocol 8. |

## Compatibility and installation details

| Finding | Change and proof |
|---|---|
| D1. Probe race | `crates/host/src/lib.rs` uses one viewer snapshot. `crates/client/src/lib.rs` dispatches a bounded optional tail by variant and abandons the stream after timeout. A QUIC fixture sends busy, host info and update info without a viewer record. The client retains the update. No positional postcard field, enum index or protocol number changed. Real binaries from 1c46c9d4585bc893981eefb98c7ce197cb601bb2 probed and opened sessions in both directions. The old host returned the expected unsupported-update result through the new client. |
| D2. Bootstrap filesystem and PATH | `scripts/release/install.sh.in` uses a private directory under a checked home or SUNNA_INSTALL_TMPDIR. It diagnoses an unexecutable helper. The helper prints the absolute installed command. All ten distro runs used a noexec temporary mount for the installer. |
| D3. Release pinning | `scripts/release/install.sh.in` and `install.ps1.in` pass their release-specific feed to the helper for installation. A functional fixture serves latest B while invoking A’s installer and verifies A is installed. The Windows template has not been run. |
| D4. Clocks and first-install trust | Host, app and Android treat future persisted times as due and use elapsed time after an in-process check. The future-clock Rust test passes. README automation downloads, checks for nonempty content, and runs with success chaining. Its real shell example passed the complete-download control and refused both an empty success and an interrupted transfer containing executable text. PowerShell documents bootstrap trust. The Android guide identifies the first sideload’s signer. |
| D5. Documentation | `docs/app-mac.md` and `docs/app-linux.md` describe Settings → Updates. `docs/host-linux.md` covers migration, and this report distinguishes observed rollback, transport and platform behavior. |

**Baseline evidence.** Actual red runs on the original uncommitted implementation covered early success, hang, missing stage, corrupt state, failed state commit, rollback-marker retention, unsafe root, HTTP feed, unlisted helper and the missing version check. The original helper was retained and later failed the automatic-launch, newer-state and health-write fixtures. Nonzero rollback and several interrupted-switch states already passed. For the other cases the “before” evidence is the independent review or source inspection. Consequently, the requested red-before-edit sequence is incomplete for those items.

## Where to find the regression tests

Rust updater fixtures are in `crates/update/src/tests.rs`. Python and shell fixtures below live in `scripts/release/`.

| Findings | Tests and entry points |
|---|---|
| A1, A2, A5 | `test-recovery.py` includes nonzero, early-success, hang, wrong child, missing stage, unreadable state, unknown format and health-lock contention. `switch_states.py` supplies the boundaries for both the helper and real systemd fixtures. `prove-migration.sh` exercises X11 capture and input. `prove-input.sh` removes XTEST while keeping the screen available. |
| A3, A6 | `failed_state_commit_keeps_running_pointer`, `rollback_request_is_consumed_with_result`, `prove-full-disk.py` and the interrupted-request case in `prove-update.py`. |
| A4, A7, A8 | `prove-update.py` checks incomplete app archives and bad-release status. `prove-migration.sh` checks the running old user service. `test_app_launch_policy` checks all three policies through the supervisor. `apps/sunna/dev/verify-updates*.mjs` check the desktop controls. |
| B1, B2 | `shared_install_root_refused_before_lock_creation`, `partial_file_must_be_regular`, `truncated_and_bad_hash_downloads_fail_and_a_partial_download_resumes`, `public_http_metadata_refused_before_network_access` and `redirect_policy_is_applied_before_following_location`. |
| B3, B4 | `rollback_preserves_high_water_mark`, `corrupt_state_after_explicit_rollback_preserves_operator_selection`, `archive_entry_limit_includes_empty_directories` and `archives_cannot_make_links_outside_the_version`. The signed stale-release case uses the real host. |
| B5, B6 | `apps/android/app/src/test/java/dev/sunna/app/UpdatesTest.kt` tests the session byte stream. `prove-android-session.py` contains the system confirmation, changed-feed and cancellation cases. Its device-run limitation is recorded below. |
| B7, C1, C2 | `test-publication.py`, `check-updaters.py`, `check-apk-key.py`, `sign-ci.sh`, `version.py --check` and actionlint. |
| C3, D1 | `remote_request_checks_peer_floor_and_stale_results`, `crates/host/tests/live_stream.rs::probe_dispatches_update_after_busy_viewer_disappears`, `crates/proto/tests/update_compat.rs` and real old/new sessions in `prove-update.py`. The enforcement points are in `crates/update/src/lib.rs`, `crates/client/src/lib.rs` and `crates/host/src/lib.rs`. |
| D2, D3, D4, D5 | `runtime-smoke.sh` runs the installed launchers under noexec temporary storage. `prove-update.py` tests the pinned feed. `future_timestamp_is_due` covers the Rust clock rule. Android uses `SystemClock.elapsedRealtime()` in `Updates.kt`. First-install and settings text was checked against the shipped controls and bootstrap templates. |

## Commands and results

Run the pair proof from the repository root, with Docker access and a private directory below a trusted home. The script builds both releases, builds the old binaries via git archive, signs local feeds with a temporary key and deletes the temporary signing key on exit.

```sh
export PATH="$HOME/.cargo/bin:$PATH"
work=$(mktemp -d "$HOME/sunna-fix-proof.XXXXXXXX")
scripts/release/prove-release-pair.sh "$work"
```

The wrapper invokes the first four commands below while its fixture key and loopback feeds exist. The remaining checks run independently.

```sh
python3 scripts/release/prove-update.py "$work" "$work/releases/A/.proof-client"
scripts/release/prove-systemd.sh "$work"
# With the runtime feed served on loopback port 48991:
scripts/release/prove-linux.sh "$work/runtime/A"
scripts/release/prove-linux.sh "$work/runtime/B"

cargo build -p sunna-update
python3 scripts/release/test-recovery.py
cargo test -p sunna-update -p sunna-proto
cargo test -p sunna-host --test live_stream probe_
scripts/release/test-layout.sh
python3 scripts/release/test-publication.py
python3 scripts/release/version.py 0.0.1 --check
actionlint .github/workflows/release.yml
```

- The functional suite passed 16 checks. They include CLI, automatic and authenticated updates, active-session deferral, resuming a request across process death, three bad-release cases, rollback/quarantine, replay refusal, peer minimum, pinned bootstrap, two missing app launcher cases, migration and both old/new directions.
- The final release pair passed 66 end-to-end cases. A separate rerun of `prove-systemd.sh` against the same binaries added the disabled-XTEST check and passed all 41 cases in that suite, including live migration, disabled input, 19 daemon kills, 19 supervisor kills and kernel ENOSPC/retry. After every kill a new supervisor had to serve a successful probe from the expected version and record health. Its service manager and Xvfb run inside the fixture container. The layout check inspected unit paths across pruning.
- Ubuntu 22.04, Ubuntu 24.04, Debian 13, Fedora 45 and Arch each passed A and B with no Rust or C toolchain. Both the listening host and the supervised Xvfb desktop app had to record health. The smoke test now fails explicitly if it finds cargo, rustc or cc. ELF inspection of the Linux app found a maximum GLIBC requirement of 2.35, a runpath of `$ORIGIN/../lib` and the packaged FFmpeg libraries `libavcodec.so.60` and `libavutil.so.58`.
- The recovery script passed eleven tests including subcases. The Rust run passed 16 updater tests, 19 protocol tests and four compatibility tests. Two real-transport probe tests passed. All five Python publication and installer-documentation tests passed. Actionlint 1.7.12 passed.
- The desktop checks ran through a dedicated loopback mock server. Disposable copies of the scripts redirected ports and screenshot output away from the shared design artifacts. `node verify-updates.mjs`, `node verify-updates-edges.mjs` and `node verify-updates-policy.mjs` passed, including all three app choices, 100 matrix screenshots, six flow checks, six edge checks and the host policy form at two sizes.
- `scripts/release/build-linux.sh 0.0.1 "$work/production"` built a production-key package without using a signing secret. `check-updaters.py` exercised its actual programs. The Windows helper passed `cargo check --locked -p sunna-update --target x86_64-pc-windows-gnu -j2` in a MinGW container.

## Android results

The debug build included arm64-v8a and x86_64. Its three JVM tests passed for numeric versions, copying the approved bytes into the session, and rejecting changed bytes before commit. The CI packaging wrapper also built a release APK using a disposable Android signing keystore. Both APKs passed the verifier-key check for both ABIs. A mixed artifact with the production arm64 library and the real test-key x86_64 library failed the same check, identifying x86_64. Inspection of the compiled DEX found the calls to open the installer session stream, request user action, and commit the session.

```sh
(cd apps/android && ANDROID_HOME="$HOME/Android/Sdk" ./gradlew --no-daemon testDebugUnitTest assembleDebug)
python3 scripts/release/check-apk-key.py apps/android/app/build/outputs/apk/debug/app-debug.apk
# With disposable Android keystore credentials in the environment, as in the workflow:
scripts/release/android-ci.sh "$work/android-ci/dist"
python3 scripts/release/check-apk-key.py "$work/android-ci/dist/sunna-android.apk"
```

The following device fixture built APKs A and B with the same disposable Android signer. Its loopback server supplies one signed feed at consent time and a second signed feed with a different digest at download time. The fixture also checks cancellation and a system-confirmed installation. The API 30 emulator ran without KVM on this machine. Its Android framework failed to stay responsive, and the system-server watchdog terminated the blocked process. No end-to-end Android installation, changed-feed dialog, or cancellation was observed. The fixture uses KVM when the device is available.

```sh
mobile=$(mktemp -d "$HOME/sunna-fix-mobile.XXXXXXXX")
(
  export SUNNA_RELEASE_KEY="$mobile/test.key" SUNNA_TEST_PUBLIC_KEY="$mobile/test.pub"
  trap 'rm -f "$SUNNA_RELEASE_KEY"' EXIT
  scripts/release/keygen.sh "$SUNNA_TEST_PUBLIC_KEY"
  scripts/release/build-android-proof.sh "$mobile" &&
    scripts/release/prove-android-session.sh "$mobile"
)
```


## Release prerequisites and limits

The first installer is itself a trust decision. Production HTTP is refused at the initial URL and at redirects. Test-key builds intentionally permit loopback HTTP. The first installer and the account that owns the existing installation remain trust prerequisites.

Before publishing, configure required reviewers and restrict tags on the GitHub **releases** environment. Store the matching release signing key and the permanent Android keystore credentials as protected secrets. Review the exact artifacts, including bundled FFmpeg source and license, and retain the signing identities across upgrades. No private release key is needed for any proof here.

The GitHub jobs, publication, Linux ARM64 runtime, macOS signing/TCC/relaunch, Windows filesystem ACLs, service startup and driver behavior still require platform verification. The Windows host is excluded from the release inventory. Host certificate pinning remains a separate security prerequisite for recommending public downloads to remote-only users.

The disposable key files, private proof tree, Docker containers, images and build volumes were removed. The production release private key was never accessed. The working tree remains uncommitted. No tag, push, publication or upload was performed. The owner’s services and displays were left alone.
