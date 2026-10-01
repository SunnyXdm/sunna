# Sunna for Android

The Android app: Kotlin on the platform's own views (no other libraries), and
a Rust library for everything else (`crates/android`): connecting, the
session, decoding with the phone's MediaCodec straight onto the SurfaceView,
and sound through AAudio. Android 10 (API 29) or later.

## Building

You need:

- JDK 17
- the Android SDK with platform 36 and NDK 27.1.12297006 (`sdkmanager "platforms;android-36" "ndk;27.1.12297006"`)
- Rust with the Android targets, and cargo-ndk:

  ```sh
  rustup target add aarch64-linux-android x86_64-linux-android
  cargo install cargo-ndk --locked
  ```

Then, with `ANDROID_HOME` (or `local.properties`) pointing at the SDK:

```sh
./gradlew assembleDebug      # app/build/outputs/apk/debug/app-debug.apk
./gradlew assembleRelease    # app/build/outputs/apk/release/app-release.apk
```

Gradle builds the Rust library first (`buildRust`: always optimized, for
arm64-v8a and x86_64). `-Psunna.rustToolchain=<toolchain>` picks a rustup
toolchain other than the default.

## Signing

Android installs an update only if it's signed with the same key as the app
it replaces, so release builds are signed with a key of your own, kept
outside the repository. Make one once:

```sh
mkdir -p ~/.sunna/android && chmod 700 ~/.sunna/android
keytool -genkeypair -keystore ~/.sunna/android/sunna-release.jks -alias sunna \
  -keyalg RSA -keysize 4096 -validity 10000 -dname "CN=Sunna"
```

and describe it in `~/.sunna/android/signing.properties` (readable only by
you):

```properties
storeFile=/home/you/.sunna/android/sunna-release.jks
storePassword=…
keyAlias=sunna
keyPassword=…
```

`-Psunna.signing=<file>` or `SUNNA_ANDROID_SIGNING=<file>` points at another
properties file. Without one, the release build is signed with the debug key.
Back the key up: an app signed with a different key can't update this one;
it has to be uninstalled first, which deletes its computers.

## Adding computers

Android doesn't let one app ask Tailscale which devices it knows, so the app
can't list the tailnet's computers. Instead, a computer shows a QR code of its
`sunna://` link: `sunna-host link` on Linux, Settings › Show QR Code in the
desktop app, or `scripts/run.sh host` on a Mac. The phone's camera opens the
link, and Sunna's Add a Computer comes up with the address and key filled in
(an intent filter for the `sunna` scheme).

## Trying it in the emulator

An x86_64 system image (`system-images;android-36;google_apis;x86_64`) runs
the app; the emulator's decoders (`c2.goldfish.*`) stand in for a phone's.
A host on the same machine is at `10.0.2.2`. Headless:

```sh
emulator -avd <name> -no-window -no-audio -gpu swangle_indirect
adb install -r app/build/outputs/apk/debug/app-debug.apk
adb logcat -s sunna            # the Rust side's log
```

With `-gpu swiftshader_indirect`, the emulator itself crashed (in its
renderer) while running the app; `swangle_indirect` didn't.
