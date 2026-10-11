# The Sunna app on Android

Your computers on a phone or tablet: the same Computers screen as on the Mac, and sessions full screen, with touch, the phone's keyboard, the clipboard and sound. Android 10 or later.

[← Back to the README](../README.md)

## Install

There's no store listing yet, so the app is built from source on a computer, then installed on the phone. Building needs JDK 17, the Android SDK and NDK, and Rust with the Android targets: [apps/android/README.md](../apps/android/README.md) lists the exact versions and setup.

```sh
cd apps/android
./gradlew assembleRelease
adb install app/build/outputs/apk/release/app-release.apk
```

Without `adb`, copy `app-release.apk` to the phone and open it; Android asks you to allow installing apps from that source. To update, install the new APK over the old one: your computers are kept, as long as it was signed with the same key.

The phone must be on the same network as your computers, or on Tailscale with them.

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
