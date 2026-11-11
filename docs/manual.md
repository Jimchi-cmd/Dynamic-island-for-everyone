# Islet user manual

A step-by-step guide to installing Islet, setting it up, and using every feature. Short on time? Do **1. Download**, **2. Install** and **3. First start**; the rest you can look up when you need it.

**Contents**

1. [Download](#1-download)
2. [Install](#2-install) (Windows, macOS, Linux)
3. [First start](#3-first-start)
4. [Using the island](#4-using-the-island)
5. [Music and lyrics](#5-music-and-lyrics)
6. [Control Centre: Wi-Fi, Bluetooth, shelf, clipboard, system](#6-control-centre)
7. [Pop-ups: battery, charging, volume, brightness, Caps Lock](#7-pop-ups)
8. [Timer, focus sessions and alarms](#8-timer-focus-sessions-and-alarms)
9. [Calendar, weather and notifications](#9-calendar-weather-and-notifications)
10. [Navigation ETA](#10-navigation-eta)
11. [Make it yours: look, hover-only mode, shortcut](#11-make-it-yours)
12. [Privacy settings](#12-privacy-settings)
13. [Command line (optional)](#13-command-line-optional)
14. [Updating and uninstalling](#14-updating-and-uninstalling)
15. [Troubleshooting](#15-troubleshooting)

---

## 1. Download

1. Open the **[latest release](https://github.com/Jimchi-cmd/Dynamic-island-for-everyone/releases/latest)**.
2. Scroll to **Assets** and download the file for your computer:

| Your computer | Download | Notes |
|---|---|---|
| Windows 10 or 11 | `Islet_<version>_x64-setup.exe` | The easiest. |
| Windows, with notifications on the island | `Islet_<version>_x64.msix` **and** `Islet-test-certificate.cer` | Windows only lets packaged apps read notifications. |
| Windows, no install | `Islet_<version>_x64_portable.zip` | Unzip anywhere. |
| Mac (Apple silicon or Intel) | `Islet_<version>_universal.dmg` | |
| Ubuntu, Debian, Mint, Pop!_OS | `Islet_<version>_amd64.deb` | |
| Fedora, openSUSE | `Islet-<version>-1.x86_64.rpm` | |
| Any other Linux | `Islet_<version>_amd64.AppImage` | |
| GNOME desktop (extra) | `islet@jimchi-cmd.github.io.shell-extension.zip` | Adds a shortcut and the Caps Lock pop-up on GNOME. |

> **Browser warning?** Firefox, Edge or Chrome may say the file "may be harmful" or "contains malware". Islet isn't signed with a paid certificate yet and few people have downloaded it, which is what triggers that warning. In Firefox: open the downloads panel (arrow, top right) → right-click the download → **Allow download**. In Edge or Chrome: **⋯ → Keep → Show more → Keep anyway**. You can check any file at [virustotal.com](https://www.virustotal.com) first.

Optional: every release has a `SHA256SUMS` file to check that a download is intact (`sha256sum -c SHA256SUMS --ignore-missing`).

---

## 2. Install

### Windows: the installer (`.exe` or `.msi`)

1. Double-click `Islet_<version>_x64-setup.exe`.
2. If Windows says **"Windows protected your PC"**, click **More info** → **Run anyway**.
3. Follow the installer. Islet Settings opens when it finishes; go to [3. First start](#3-first-start).

### Windows: the MSIX package (for notifications)

Do this once per download, because each build comes with its own test certificate:

1. Put `Islet_<version>_x64.msix` and `Islet-test-certificate.cer` in the same folder (unzip first if they came in a zip).
2. **Trust the certificate.** Double-click `Islet-test-certificate.cer` → **Install Certificate**.
3. Choose **Local Machine** → **Next** (say **Yes** to the administrator prompt).
4. Choose **Place all certificates in the following store** → **Browse** → **Trusted People** → **OK** → **Next** → **Finish**. You should see "The import was successful".
5. **Install Islet.** Double-click `Islet_<version>_x64.msix` → **Install**.
6. Open **Islet Settings** from the Start menu.

If the installer shows an error code, see [Troubleshooting](#15-troubleshooting) (0x800B010A means step 2–4 was skipped; 0x80073CFB means an older Islet is still installed).

### Windows: portable

Unzip `Islet_<version>_x64_portable.zip` into a folder you'll keep, then double-click `islet-settings.exe`.

### Mac

1. Open `Islet_<version>_universal.dmg` and drag **Islet** onto **Applications**.
2. Open Islet from Applications. macOS says it can't check the developer: click **Done**.
3. Open **System Settings → Privacy & Security**, scroll down, and click **Open Anyway** next to the message about Islet.
4. Open Islet again and click **Open**.

If macOS says Islet **"is damaged and can't be opened"**, open Terminal and run:

```bash
xattr -dr com.apple.quarantine /Applications/Islet.app
```

### Linux

| File | Install it with |
|---|---|
| `.deb` | `sudo apt install ./Islet_<version>_amd64.deb` |
| `.rpm` | `sudo dnf install ./Islet-<version>-1.x86_64.rpm` |
| `.AppImage` | `chmod +x Islet_<version>_amd64.AppImage`, then double-click it |

Then open **Islet** from your applications menu.

**GNOME users (optional extension):**

```bash
gnome-extensions install --force islet@jimchi-cmd.github.io.shell-extension.zip
```

Log out and back in, then run `gnome-extensions enable islet@jimchi-cmd.github.io`. It adds **Ctrl+Alt+I** to open and close the island, the Caps Lock pop-up, and an option to hide the top bar's clock.

---

## 3. First start

1. Islet Settings opens with a short welcome. Read the **Private by default** screen and click **Continue**.
2. The next screen lists the permissions Islet may ask for and why. Click **Continue**.
3. Choose **Start now and at login** (recommended) or **Start, just this once**.
4. A small black pill appears at the **top centre** of your screen. That's the island.

The first time you use some features, your system asks for permission:

- **Location** (Windows 11, macOS) when you open the Wi-Fi list. The system requires it before any app can see network names. Islet never reads your location.
- **Bluetooth** (macOS) for battery levels and connecting devices.
- **Notifications** (Windows MSIX) when you turn on notifications.

You can close the Settings window; the island keeps running. Open **Islet Settings** from your apps whenever you want to change something.

---

## 4. Using the island

| To… | Do this |
|---|---|
| Peek (it grows slightly) | Move the pointer over the island |
| Open it | Click it |
| Close it | Click it again, press **Esc**, click anywhere else, or move away (it closes by itself after 5 seconds) |
| Open the biggest view | Press and hold |
| Switch between two things at once (music and a timer, say) | Scroll over the island |
| Open the menu (Settings, hide for an hour, pause pop-ups, lyrics, quit) | Right-click the island |
| Quit Islet | Right-click the island → **Quit Islet** |

What it shows on its own:

- **Nothing happening:** the small pill with the current time in it (or nothing at all in [hover-only mode](#11-make-it-yours)). The time follows your computer's clock and its 12- or 24-hour setting.
- **One thing going on** (music, a timer, navigation): the pill widens, with a picture on the left and a live indicator on the right.
- **Two things:** the pill plus a small bubble beside it. Scroll to swap them.

**Fullscreen videos and games:** when something goes fullscreen (a YouTube video, a game, a slideshow), the island hides so it's not in the way, even with music playing. Move the pointer to the top centre of the screen to bring it back; it hides again when you move away. A ringing alarm or timer still shows. (On Linux with native Wayland the island simply stays behind the fullscreen window.)

When you open the island with nothing going on, you get **Control Centre**: the time, the date, the weather (if set), and buttons for Wi-Fi, Bluetooth and more.

---

## 5. Music and lyrics

Islet shows what's playing in any app: Spotify, a browser, Apple Music, VLC and so on. Nothing to set up.

**In the small pill:** album art on the left, a moving waveform on the right.

**Open the island** for the full player:

1. **Play / pause / previous / next:** the buttons on the left of the bottom row.
2. **Volume:** the slider beside them (your computer's volume). Drag it or tap where you want it.
3. **Jump in the song:** press on the progress bar and drag.
4. **Control Centre:** the **‹ Control Centre** button at the top left. Control Centre then shows **Now Playing ›** to go back.

### Synced lyrics

1. Open **Islet Settings → General → Now Playing**.
2. Turn on **Show synced lyrics**.
3. Play a song. The pill gets wider and the lyrics slide through it as the song plays: the line being sung is bright, and the lines before and after it are dimmed. Open the island to see two lines under the player too.

**Your own lyric files come first.** Islet looks for an `.lrc` file before going online:

- next to the song file, with the same name (`Song.flac` → `Song.lrc`), when the player tells Islet which file it's playing (most Linux players do);
- in a `lyrics` folder inside Islet's settings folder (`%APPDATA%\Islet\lyrics` on Windows, `~/Library/Application Support/Islet/lyrics` on a Mac, `~/.config/islet/lyrics` on Linux), named `Artist - Title.lrc` or `Title.lrc`.

Otherwise lyrics come from [LRCLIB](https://lrclib.net), a free, open lyrics database. When this is on, Islet sends LRCLIB only the title, artist, album and length of the song that's playing, once per song. Songs without timed lyrics there simply show none. (On a MacBook with a notch, lyrics show only in the open player, because the notch would hide the middle of the line.)

**Lyrics a little early or late?** Right-click the island → **Lyrics timing** → tap **−** to show them sooner or **+** to show them later (half a second per tap, for that song). The same menu turns lyrics on or off.

### More music options (Settings → General → Now Playing)

- **Hide an app's music** (e.g. a browser playing videos): type its name under "Hide music and videos from these apps" and click **Hide**.
- **Download web artwork** (Linux): shows covers that players only publish as web links, such as Spotify's.
- **Spotify on your other devices:** see the card **Spotify on your other devices**. You register your own free Spotify app and paste its Client ID; then music playing on your phone or a speaker shows on the island too.

---

## 6. Control Centre

Open the island when nothing is playing, or tap **‹ Control Centre** on any open card.

| Button | What it does |
|---|---|
| **Wi-Fi** (shows your network) | Opens the list of networks |
| **Bluetooth** (shows a connected device) | Opens your paired devices |
| **Shelf · N** | Files you dropped on the island (appears once there are some) |
| Clipboard icon | Your recent copies (when clipboard history is on) |
| Gauge icon | CPU, memory and disk use |

In every list, the round **‹** button at the top left goes back to Control Centre. If a list is longer than the card, **scroll** over it to see the rest (a thin bar on the right shows there's more).

**Wi-Fi**

1. Tap **Wi-Fi**. Islet looks for networks.
2. Tap a network to join it. Saved networks join straight away.
3. A new network with a password: the island tells you, and the button underneath opens Settings → **Wi-Fi & Bluetooth**, where you type the password and click **Join**.
4. The switch at the top right turns Wi-Fi on or off.

**Bluetooth**

1. Tap **Bluetooth** to see your paired devices and whether each is connected.
2. Tap a device to connect or disconnect it. On Windows, apps aren't allowed to do that, so the button underneath opens Windows' Bluetooth settings instead.
3. The switch at the top right turns Bluetooth on or off.

**Shelf**

1. Drag files or folders from your desktop or file manager onto the island.
2. Open Control Centre → **Shelf** to see them. Tap one to open it; tap **×** to take it off the shelf. The shelf keeps shortcuts only: your files stay where they are.

**Clipboard history** (off until you turn it on in Settings → General → Privacy) keeps your last 20 copied texts in memory (500 with Islet Pro). Tap one to copy it again. Copies from password managers are never kept.

**System monitor:** CPU, memory and disk, read only while that page is open.

---

## 7. Pop-ups

These appear for a moment by themselves:

- **Headphones, earbuds, mice, keyboards or controllers connect:** the device and its battery level.
- **Low battery** on a connected device.
- **Charging:** plug in your laptop and a green bolt shows "Charging" with the level; unplug and it says "On battery"; at 100 % it says "Fully charged". Earbuds or a mouse starting to charge get their own pop-up.
- **Volume, screen brightness and Caps Lock:** turn each on or off in **Settings → General → Pop-ups**.
- **Camera and microphone in use:** a green dot (camera) or orange dot (microphone) at the right end of the pill. Turn it off in **Settings → General → Privacy**.

---

**Calls:** when Zoom, Teams, a browser call (Google Meet, for example) or any other app uses your microphone for more than a few seconds, the island shows a microphone, the app's name and how long the call has been going. Open the island and tap the big microphone button to **mute** your microphone (it turns red); tap again to unmute. It disappears when the call ends. Don't want it? Turn off **Settings → General → Privacy → Calls**. (On a Mac the island can't tell which app it is, so it says "Microphone on".)

**Downloads** (off until you turn it on in **Settings → General → Privacy → Downloads in progress**): while your browser downloads a file, the island shows its name, how much has arrived and the speed. When it's done it says **Downloaded**; open the island and tap the card to open the file. It works with Chrome, Edge, Brave, Firefox and Safari saving to your Downloads folder.

## 8. Timer, focus sessions and alarms

**Timer** (Settings → **Timer & focus**)

1. Enter the minutes, a name if you like (e.g. "Tea"), and click **Start**.
2. The pill shows a countdown ring. Open the island to **Pause** or **Stop**.
3. When it ends, the island opens and chimes; **Repeat** starts it again.

**Focus session:** set **Work** and **Break** minutes and the number of **Rounds**, then start it. The island chimes and pops up at each change between work and break.

**Focus streak:** every work round you see through to its end counts. Do at least one a day and Islet counts the days in a row: the focus card shows "3 days in a row" from the second day, and Settings → Timer & focus shows the streak, your best, and whether today is done. Missing a whole day starts it again. The count is kept in `focus.json` on this computer only. From a terminal:

```bash
islet focus streak
```

**Alarms** (Settings → **Alarms**)

1. Under **New alarm**, choose the **Time**, a name if you like, and under **Repeat** the days it should ring (none = once). Click **Add alarm**.
2. When an alarm rings, the island opens with a swinging bell and a chime. Click **Snooze** (9 minutes) or **Stop**.
3. Switch an alarm off with its switch, or click **Delete**.

---

## 9. Calendar, weather and notifications

**Weather** (Settings → General → **Weather**)

1. Type your city and click **Search**, then click **Use** next to the right place.
2. The temperature and today's high and low appear on Control Centre. Turn on **Fahrenheit** if you prefer it. Data comes from Open-Meteo once an hour; no account needed.

**Calendar** (Settings → General → **Calendar**)

1. Copy your calendar's **private iCal / ICS link**: Google Calendar → Settings → your calendar → *Secret address in iCal format*; Outlook → Calendar settings → *Shared calendars → Publish*; iCloud → share the calendar as a *Public Calendar*.
2. Paste it in the box and click **Save link**. It's stored in your system's keychain.
3. Turn on **Show upcoming events**. Ten minutes before an event, the island shows what's next; open it for today's agenda.

When an event that is about to start has a video-call link (Zoom, Google Meet, Teams, Webex, Whereby, Jitsi, GoTo, BlueJeans, Chime, Skype or Discord), its row gets a green **Join** button that opens the call in your browser or the call's app. Other links in an invitation are never opened.

**Notifications** (Linux, and Windows with the MSIX)

1. Settings → General → **Notifications** → turn on **Show notifications on the island**. Windows asks once for permission.
2. New notifications pop up on the island; open it to see the latest few.

---

## 10. Navigation ETA

Shows your arrival time with live traffic, and hands off to Google Maps or Waze. It needs your own free Google key: the full setup is in **[navigation.md](navigation.md)**. In short:

1. Create a Google Cloud project with the **Routes API** (and **Places API (New)** for suggestions) and make an API key.
2. Settings → **Navigation** → paste the key → **Save key**.
3. Under **Trip**, type a destination (pick from the suggestions), choose how you're travelling, and click **Start trip**.
4. The pill shows the minutes left; open the island for the arrival time and the first turn, and tap **Google Maps** or **Waze** to navigate there. **Stop** (or **Stop trip** in Settings) ends it.

---

## 11. Make it yours

**Settings → General → Look**

- **Show the island only when I point at it** (hover-only mode): the empty island disappears until you move the pointer to the top centre of the screen. Music, timers, pop-ups and anything else going on always show.
- **Show the time when nothing is playing:** the time in the empty pill (on by default).
- **Accent colour:** for buttons and switches on the island.
- **Size:** small, medium or large.
- **Display:** which screen the island is on (with more than one), or **All displays** for an island on each. The extra islands show music, calls, batteries, downloads and Control Centre; alarms, timers and the shortcut stay with the main island. Plug in a new screen? Choose the setting again (or restart Islet) to give it an island.
- **Gap above the island:** move it down from the top edge (0–40 px).

Islet restarts itself to apply a new size, gap or display.

**Settings → General → Shortcut:** click the box, press the keys you want (for example **Ctrl+Alt+I**) and click **Save**; they then open and close the island from the keyboard. It needs Ctrl, Alt or the Windows/Command key. On native Wayland desktops, set a shortcut in your desktop's keyboard settings that runs `islet toggle` instead.

**Settings → General → Start at login:** start Islet when you log in, or not.

---

**The right-click menu** also has:

- **Hide for an hour:** the island disappears (point at the top of the screen to see it anyway). Choose **Show the island again** to undo.
- **Pause pop-ups for an hour:** no volume, battery, notification or calendar pop-ups; music, timers and alarms carry on. **Resume pop-ups** undoes it.

## 12. Privacy settings

Islet has no accounts, no analytics and no telemetry. It uses the internet only for features you turn on:

| Feature | Sends | To |
|---|---|---|
| Weather | the place you chose, hourly | Open-Meteo |
| Lyrics | title, artist, album and length of the song playing | LRCLIB |
| Calendar | fetches your link every 15 minutes | your calendar provider |
| Navigation | origin, destination, travel mode | Google, with your key |
| Spotify | playback state | Spotify, with your app |
| Web artwork (Linux) | the cover's web address | wherever the player points |
| Update checks (off by default) | a version check | GitHub |

Keys and links are kept in your system's keychain, never in a file. Clipboard history and lyrics live in memory only.

---

## 13. Command line (optional)

Everything in Settings can also be done from a terminal with the `islet` command. A few examples:

```bash
islet status                       # is Islet running, and how it's set up
islet toggle                       # open or close the island
islet set lyrics on                # synced lyrics
islet set auto-hide on             # hover-only mode
islet set idle-clock off           # no time on the empty island
islet timer start 25m --label Tea
islet alarm add 7:30 --days weekdays --label "Wake up"
islet look accent purple
islet activity start build --title "Build" --progress 40   # your own live activity
islet --help                       # everything else
```

---

## 14. Updating and uninstalling

**Update:** Settings → **Updates** → **Check for updates now**. If a new version is out, Settings opens its download page; install it the same way as the first time.

- **Windows MSIX:** trust the new certificate that comes with it, then open the new `.msix`. If Windows refuses because Islet is already installed, uninstall the old one first.
- **`.deb` / `.rpm`:** install the new package over the old one.

**Uninstall:**

- **Windows:** Settings → Apps → Installed apps → **Islet** → **⋯** → **Uninstall**. (To remove the test certificate too: run `certlm.msc` → Trusted People → Certificates → delete "Islet Test".)
- **Mac:** right-click the island → **Quit Islet**, then drag Islet from Applications to the Bin.
- **Linux:** `sudo apt remove islet`, `sudo dnf remove islet`, or delete the AppImage.

Your settings stay in `%APPDATA%\Islet` (Windows), `~/Library/Application Support/Islet` (Mac) or `~/.config/islet` (Linux); delete that folder to remove them too.

---

## 15. Troubleshooting

| Problem | Fix |
|---|---|
| Browser says the download contains malware | A false alarm for unsigned new apps. See [1. Download](#1-download). |
| "Windows protected your PC" | **More info** → **Run anyway**. |
| MSIX error **0x800B010A** ("publisher certificate could not be verified") | Trust `Islet-test-certificate.cer` into **Local Machine → Trusted People** ([steps](#windows-the-msix-package-for-notifications)), then open the `.msix` again. |
| MSIX error **0x80073CFB** ("same identity… contents are different") | An older Islet is installed. Uninstall it (Settings → Apps), or in PowerShell run `Get-AppxPackage *Islet* \| Remove-AppxPackage`, then install again. |
| New features (lyrics, hover-only…) are missing | You're still running an older version: uninstall every Islet in Settings → Apps and install the new one. |
| I can't see the island | Hover-only mode may be on: point at the top centre of the screen. Otherwise open Islet Settings and check **Island** says it's running, or run `islet status`. |
| Wi-Fi list is empty or says to allow Location | Windows: Settings → Privacy & security → **Location** → let apps (and Islet) use it. Mac: System Settings → Privacy & Security → Location Services → Islet. |
| Bluetooth devices can't be connected from the island (Windows) | Windows doesn't let apps do that; the button opens Windows' Bluetooth settings. |
| No lyrics for a song | LRCLIB has no timed lyrics for it. Check that **Show synced lyrics** is on. |
| No music shown | Check the app isn't hidden (Settings → Now Playing). On Linux the player must support MPRIS (most do). |
| Mac: "Islet is damaged" | `xattr -dr com.apple.quarantine /Applications/Islet.app` |
| GNOME: shortcut or Caps Lock pop-up doesn't work | Install the GNOME Shell extension ([Linux](#linux)) and log out and back in. |

Still stuck? [Open an issue](https://github.com/Jimchi-cmd/Dynamic-island-for-everyone/issues) with what you did, what you expected and a screenshot.
