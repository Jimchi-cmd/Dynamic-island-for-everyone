# Islet on your Android phone

The Islet app for Android does two things, and you can use either or both:

1. **An island on the phone itself:** a black pill round the front camera that grows to show what is going on, like the one on your computer. See [The island on your phone](#the-island-on-your-phone).
2. **Your phone on your computer's island:** the phone's notifications, what is playing (with working play, next and previous buttons) and its battery level show on the computer. See [Pair](#pair).

Everything is free: no Islet Pro needed. The app is open source, in the [`android`](../android) folder.

iPhones can't do this: iOS doesn't let an app read other apps' notifications or draw over them.

## The island on your phone

### Set it up

Install the app first (see [Install the app](#install-the-app)); Android 8 or later. Then open Islet. Under **The island on this phone**:

1. **Notification access** → **Allow**, then turn on Islet. The island needs it for notifications, music, calls, directions and downloads.
2. **Round the camera (recommended)** → **Open Accessibility** → **Islet island** → turn it on. This lets the island sit over the status bar, round the front camera, and gives the control centre its screenshot and lock buttons. Islet asks for no accessibility events and reads nothing on your screen.
   - **Or**, if you'd rather not use accessibility: **Display over other apps** → **Allow**. The island then shows just below the status bar.

On Android 13 and later, a switch may be greyed out because the app wasn't installed from a store. Open the phone's **Settings → Apps → Islet**, tap **⋮** at the top right, choose **Allow restricted settings**, and try again.

Optional permissions, each asked for with its own button:

- **Calendar:** count down to meetings, with a **Join** button for Google Meet, Zoom, Teams, Webex, Whereby and Jitsi links.
- **Photos:** show each new screenshot.
- **Nearby devices:** name the Bluetooth device that connected.
- **Modify system settings:** the brightness slider.

### Use it

- **Tap** the island to open it. Tap again, swipe up, or tap anywhere else to close it.
- **Long-press** for the control centre.
- **Swipe sideways** when two things are going on (a call and music, say) to switch which one leads. The second one sits in a small bubble beside the island; tap the bubble to open it.
- Tap a notification pop-up to open it, or a screenshot pop-up to see the picture.

### What it shows

| | On the island | Opened |
|---|---|---|
| **Music** (any player) | The cover and moving bars | Cover, title, artist, a progress bar you can drag, previous / play / next. Tap the cover to open the player. Synced lyrics if you switch them on. |
| **Calls** | A green phone and the call's length | The caller, with the phone app's own buttons (answer, decline, hang up, mute, speaker) |
| **Directions** (Google Maps, Waze, Organic Maps, OsmAnd, HERE, Sygic, Yandex) | The turn arrow and the distance | The next turn and the road |
| **Timers** | Time left | Pause, add a minute, stop. Focus rounds go focus → break, with a longer break after every fourth. A buzz and a sound when it ends. |
| **Stopwatch** | Time so far | Laps, pause, reset |
| **Another app's countdown** (the Clock app's timer) | Time left | Open the app |
| **Meetings** (from 15 minutes before to 5 after) | Minutes to go | Join, or open the calendar |
| **Downloads and uploads** | A ring filling up | The file, the app and the percentage |
| **Nothing going on** | The empty pill; a plane or moon in airplane mode or Do Not Disturb | The clock card: time, date, weather, world clocks, your next meeting and alarm, your first to-do, and 1, 5, 10 and 25-minute timer buttons |

Pop-ups, each with its own switch: new notifications, charging and low battery (20, 10 and 5 %), ring / vibrate / silent, airplane mode, Do Not Disturb, Wi-Fi and Bluetooth connecting, new screenshots, and timers ending. Notifications don't pop up during Do Not Disturb, and **Apps that never pop up** keeps chosen apps quiet.

### Control centre

Long-press the island: flashlight, Do Not Disturb, ring or vibrate, volume, brightness, screenshot, lock, and up to four shortcut apps. Android doesn't let apps switch Wi-Fi, Bluetooth or airplane mode, so those buttons open the phone's own panel.

### Looks

Size (small, medium, large); nudges left, right, up, down and wider for phones where the pill doesn't sit exactly on the camera; font (default, light, condensed, serif, mono); the empty pill on or off; a border in the album's colour while music plays; the moving bars; °C or °F; the plane and moon; green and orange dots while the camera or microphone is on; and hiding the island in landscape (videos and games).

### Weather, clocks, to-do and notes

- **Weather:** choose a place and the clock card shows the temperature, the sky, and today's high and low. It comes from Open-Meteo, free and without an account; Islet sends it only your place's coordinates, at most every half hour.
- **World clocks:** up to three cities.
- **To-do:** add, tick and delete. The first unfinished one shows on the clock card.
- **Quick notes:** one line at a time, with the date and time. **Share all** sends them to any app.

### Also in the app

- **Speed test:** about 35 MB with Cloudflare's public speed-test servers, only when you press the button.

### What the laptop island does that the phone can't

Android doesn't let apps do these, so they stay on the computer:

- **Clipboard history:** since Android 10 only the keyboard and the app in front may read the clipboard.
- **Per-app volume** and the **app switcher:** Android has no way for an app to set other apps' volume or list their windows.
- **System monitor** (CPU and memory use): Android 8 and later hide other apps' use.
- **Plugins** (scripts on the island) and the **file shelf**: Android apps can't run your scripts or hold files for dragging into other apps.
- **Switching Wi-Fi, Bluetooth and airplane mode:** the buttons open the phone's own panel instead.
- **Replacing the volume slider:** Android's own one still shows when you press the volume buttons.

### Privacy

The island runs on the phone. It goes online only for what you switch on: weather for the place you chose, lyrics (LRCLIB gets the song's title, artist and length), and the speed test. Notifications, music and everything else never leave the phone unless you pair it with your computer, and then only over your own Wi-Fi, encrypted.

## Your phone on your computer's island

### What you need

- Islet 0.3.3 or later on the computer (0.3.0 to 0.3.2 needed Islet Pro for this).
- A phone with Android 8 or later.
- Both on the same Wi-Fi (or other local network).

### Install the app

The app isn't on the Play Store. You install the APK file yourself:

1. On the phone, open Islet's [releases page](https://github.com/Jimchi-cmd/Dynamic-island-for-everyone/releases) and download the `.apk` file.
2. Open the downloaded file. Android asks whether your browser may install apps: allow it, then tap **Install**.

Builds made before a release are on the repository's **Actions** tab: open the latest **Android** run and download `islet-android-debug`. You need to be signed in to GitHub to download these.

### Pair

1. On the computer: Islet Settings → **Wi-Fi, Bluetooth & phone** → **Your Android phone** → **Pair a phone**. A QR code appears.
2. On the phone: open Islet and tap **Scan the code**. Allow the camera, and point it at the code.
3. The app asks for **notification access**. Tap **Open settings** and turn on Islet.

On Android 13 and later the switch may be greyed out, because the app was not installed from a store:

1. Open the phone's Settings → Apps → Islet.
2. Tap the three dots at the top right and choose **Allow restricted settings**.
3. Go back to the Islet app and tap **Allow notification access** again.

The app then shows **Connected to** and your computer's name, and the island shows the phone's battery.

If the computer's firewall asks whether Islet may accept connections, allow it on **private** networks.

### Check that it works

- Play a song on the phone: it appears on the island. The island's play, next and previous buttons control the phone.
- Send yourself a message: it pops up on the island.
- Plug the phone in: the island shows it charging.

### If it doesn't connect

- **"Looking for…" doesn't change:** check that both devices are on the same Wi-Fi, that Islet is running on the computer, and that the firewall allows it. Guest and public Wi-Fi often stop devices from reaching each other.
- **The computer got a new address:** pair again. The code holds the computer's address at the time.
- **It worked, then stopped overnight:** some phones stop background apps to save battery. In the phone's Settings → Apps → Islet → Battery, choose **Unrestricted**.
- **After installing a newer build, Android says the app can't be installed:** builds made before a signing key was set up are each signed differently. Uninstall the old one first, then install and pair again.

## What is sent, and how it is protected

- The phone sends notifications' app name, title and text, the playing song's title, artist and position, and the battery level. Ongoing notifications (music players, downloads), ones marked secret, and group summaries are not sent. The app has a switch each for notifications and music.
- The phone and the computer talk directly over your network. Nothing goes through the internet, and there is no account or server.
- Pairing: the computer makes a random 256-bit key and shows it in the QR code with its address. The key is kept in the computer's keychain and in the phone app's private storage, and nowhere else. Treat the QR code like a password.
- Each connection derives its own key from the pairing key and fresh random numbers from both sides (HMAC-SHA-256). Every message is sealed with AES-256-GCM and carries a counter that only goes up, so a recorded message can't be replayed.
- A device without the key is disconnected before anything it sends is used.
- **New code** in Settings makes a fresh key (the old code stops working), and **Unpair** removes it.

From a terminal:

```bash
islet phone pair
```

```bash
islet phone status
```

```bash
islet phone unpair
```

## For the maintainer: a signing key for releases

Without a signing key, each build is signed with a throwaway debug key, so a new build can't be installed over an old one. To sign releases with one key:

1. Make a keystore once, on your own computer, and keep it and its passwords safe (losing it means users must reinstall):

```bash
keytool -genkeypair -v -keystore islet-release.jks -keyalg RSA -keysize 4096 -validity 10000 -alias islet
```

2. Add four repository **secrets** (Settings → Secrets and variables → Actions):
   - `ANDROID_KEYSTORE_BASE64`: the keystore file, base64-encoded;
   - `ANDROID_KEYSTORE_PASSWORD`;
   - `ANDROID_KEY_ALIAS` (`islet` above);
   - `ANDROID_KEY_PASSWORD`.

The release workflow then builds a signed release APK. Never commit the keystore.
