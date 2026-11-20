# The Sunna app on Android

Your computers on a phone or tablet: the same Computers screen as on the Mac, and sessions full screen, with touch, the phone's keyboard, the clipboard and sound. Android 10 or later.

[← Back to the README](../README.md)

## Install

On the phone, open [sunna.sunnydx.dev/download/sunna.apk](https://sunna.sunnydx.dev/download/sunna.apk) and then the downloaded file. Android asks once to let your browser install apps, and then to confirm the install. Sunna isn't on Google Play.

To build it yourself, follow the [Android build guide](../apps/android/README.md). From `apps/android`, run `./gradlew assembleDebug`. With USB debugging enabled and the phone connected, install it with:

```sh
adb install -r app/build/outputs/apk/debug/app-debug.apk
```

A copy you built yourself is signed with your own key, so the downloaded APK can't replace it: uninstall one before installing the other.

Connect the phone to the host's trusted network or tailnet, then add the computer below.

## Signed installations (in testing)

In Sunna, Settings → **Check for updates** verifies the signed manifest and
downloads the APK. Sunna also checks once a day when it launches. Before handing
the APK to Android, it verifies the hash, package name, version and signing key.
Android's **Allow from this source** setting is required for Sunna, and the
system installer asks you to confirm each update.

The first sideload establishes the Android signing identity your device trusts for future updates.
A debug installation or an app signed with another key cannot be updated by the
release APK. Sunna explains this instead of asking Android to replace it. Keep
your computers' links before uninstalling an incompatible build: uninstalling
deletes its stored computers. Devices must be on the same network as the hosts,
or on Tailscale with them.

For source builds and SDK setup, see [apps/android/README.md](../apps/android/README.md).
See [verification](update-verification.md) for the build and off-device test
results; the permission and package-installer flow still needs a device test.

## Add a computer

When a computer starts sharing, it shows a **QR code**. Scan it with the phone's camera: Sunna opens with the address and key filled in. To see the code again: `sunna-host link` on a Linux host, or Settings → **Show QR Code** in the Sunna app on a Mac. Or copy the computer's `sunna://` link to the phone and tap the paste button in **Add a Computer**.

(Android doesn't let apps ask Tailscale which devices it knows, so there's no **Find on Tailscale** here.)

## In a session

**Touch works two ways**, switched in the session menu:

| | Trackpad (the default on phones) | Touch (the default on tablets) |
|---|---|---|
| Move the pointer | drag | — |
| Click | tap | tap where you want |
| Drag | touch and hold, then drag | drag |
| Right click | two-finger tap | touch and hold, or two-finger tap |
| Scroll | two-finger drag | two-finger drag |
| Zoom the picture | pinch | pinch |

**The buttons at the top:** the keyboard button brings up the keyboard (a three-finger tap does too), and **•••** opens the session menu. Drag them along the top edge to move them out of the way. The keyboard button can be hidden in Settings.

**The keyboard** has an extra row with the keys phones lack: esc, tab, Ctrl/Alt/Super (⌃ ⌥ ⌘ for a Mac), arrows, delete, home, end, page up and down, F1–F12. A modifier holds for the next key; tap it twice to keep it down. Text is typed on a US layout.

**Keyboard suggestions** are off at first, so each key goes over as you press it. Turn them on in Settings to get autocorrect and swipe typing; Sunna retypes on the computer whatever the keyboard changes.

**The session menu** (•••, or Back) has the keyboard, Trackpad or Touch, sound, the stats bar, the computer's own shortcuts (Alt+Tab, lock screen…), the clipboard, video settings, and **Disconnect**. Close it with ×, Back, a drag down, or a tap above it.

**The clipboard** is shared both ways, text and images. Android only lets the app on screen read the clipboard, so something you copy in another app goes over when you come back to Sunna. **Type clipboard text** in the menu types it key by key, for password prompts that won't accept a paste.

**A Bluetooth keyboard or mouse** works as on a computer. On a Mac host, Ctrl acts as ⌘.

**Switching apps** doesn't end the session: a notification takes you back or disconnects. Android asks once whether Sunna may show it.

## Settings

The gear on the Computers screen: this phone's name as computers see it, and how sessions start (Trackpad or Touch, sound, clipboard, the keyboard button, keyboard suggestions, the stats bar, video).
