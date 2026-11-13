# Your Android phone on the island

With the Islet app on an Android phone, the island on your computer shows:

- the phone's **notifications**, as they arrive;
- **what is playing** on the phone, with working play, next and previous buttons;
- the phone's **battery level**, when it connects and when it runs low.

The computer side is part of Islet Pro. The phone app is free and open source, in the [`android`](../android) folder.

iPhones can't do this: iOS doesn't let an app read other apps' notifications.

## What you need

- Islet 0.3.0 or later on the computer, with Islet Pro active.
- A phone with Android 8 or later.
- Both on the same Wi-Fi (or other local network).

## Install the app

The app isn't on the Play Store. You install the APK file yourself:

1. On the phone, open Islet's [releases page](https://github.com/Jimchi-cmd/Dynamic-island-for-everyone/releases) and download the `.apk` file.
2. Open the downloaded file. Android asks whether your browser may install apps: allow it, then tap **Install**.

Builds made before a release are on the repository's **Actions** tab: open the latest **Android** run and download `islet-android-debug`. You need to be signed in to GitHub to download these.

## Pair

1. On the computer: Islet Settings → **Islet Pro** → **Your Android phone** → **Pair a phone**. A QR code appears.
2. On the phone: open Islet and tap **Scan the code**. Allow the camera, and point it at the code.
3. The app asks for **notification access**. Tap **Open settings** and turn on Islet.

On Android 13 and later the switch may be greyed out, because the app was not installed from a store:

1. Open the phone's Settings → Apps → Islet.
2. Tap the three dots at the top right and choose **Allow restricted settings**.
3. Go back to the Islet app and tap **Allow notification access** again.

The app then shows **Connected to** and your computer's name, and the island shows the phone's battery.

If the computer's firewall asks whether Islet may accept connections, allow it on **private** networks.

## Check that it works

- Play a song on the phone: it appears on the island. The island's play, next and previous buttons control the phone.
- Send yourself a message: it pops up on the island.
- Plug the phone in: the island shows it charging.

## If it doesn't connect

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
