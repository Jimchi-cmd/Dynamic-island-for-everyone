# Changelog

All notable changes are listed here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.11.3] - 2026-11-28

### Changed
- Phone: **Stay clear of the status bar** is now on by default (it was "Grow downward", for pop-ups only). The small island stays narrow round the camera, an icon each side, with its words (the song, a timer, a call's time, a download's percentage) and the lyrics in a row under the status bar, so Samsung's and Android's own icons no longer sit on top of it. The clock beside the camera is left to the status bar's own. Switch it off in What it shows to get the wide island back.

### Fixed
- **Album art shows up with the song**, on the computer and the phone. Players often name a new song before its cover is ready, or hand over the last song's cover for a moment; on Windows and Linux that first answer was kept for the whole song, so the cover came late, stayed wrong or never appeared. The cover is now read again until the real one arrives. On the phone a cover the player keeps as a file is read at the size shown instead of full size (often 3000 px), and the media notification's cover is matched to the song even when the player words the title a little differently.
- Phone: at the **Small** island size, Control Centre (and the other cards) lost their last row: cards were made 10% shorter while what's in them stayed the same size. The size setting now changes the pill round the camera; cards keep the height their contents need at every size.
- Synced lyrics are found for many more songs, on the phone and the computer: titles lose what players and video sites add ("(Official Video)", "[Lyrics]", "| Official Audio", " - Remastered 2011"), artists lose "- Topic", "VEVO" and featured names, a video named "Artist - Song" is looked up as that song, and a free-text search is the last try. The phone used to try only the exact title and artist. Lyrics still come from LRCLIB, the open lyrics database: the other sources of synced lyrics are paid or private.
- Phone: calls from apps that use Android's call style (Snapchat, and others) show **Incoming call** with a red decline and a green answer button, instead of "Calling…" and two grey phone buttons: Islet reads Android's own word on whether the call is ringing and its own answer, decline and hang-up actions. Buttons whose labels it doesn't know follow Android's order (decline, then answer).
- Phone: an open card's "‹ Control Centre" button sits under the camera's pill, clear of the status bar's icons, which were drawn over it.

## [0.11.2] - 2026-11-28

### Added
- **A download website** at https://jimchi-cmd.github.io/Dynamic-island-for-everyone/: what Islet does, the four new videos, and one button that picks the right download for the visitor's device (Windows, Mac, Linux or Android). It is rebuilt with each release's files, sets no cookies and loads nothing from other sites ([docs/website.md](docs/website.md)).

### Fixed
- Phone: a notification with a progress bar that isn't a running download (Netflix's “continue watching”, a paused download) no longer stays on the island as a ring with the app's icon for as long as it sits in the notification shade. Only downloads and uploads that are running get the ring, and a ring whose bar hasn't moved for five minutes goes too (an app's job that is stuck or paused).

## [0.11.1] - 2026-11-25

### Changed
- **Signed releases**: from this version the phone app is signed with Islet's own release key (`Islet-0.11.1-android.apk`), so later versions install over it and keep your settings, and Islet on a computer can update itself from Settings. Coming from an earlier phone version (signed with a test key), uninstall it once before installing this one.

### Fixed
- `scripts/setup-signing.ps1` and `setup-signing.sh` save each key's password as soon as the key is made, so a run that stops halfway (for example when antivirus HTTPS scanning blocks npm) can simply be run again; the npm error now says how to get past it.

## [0.11.0] - 2026-11-24

### Changed
- Phone: notification pop-ups go sooner (3 seconds instead of 5, a little longer with buttons), so they get out of the way of the phone's own notification banner. **What it shows → How long notifications show** picks Brief (2 s), Normal or Long (5 s, as before).

## [0.10.0] - 2026-11-23

### Added
- **Phone and computer together**: answer the phone's messages from the computer's island with quick replies (your own four, or type one in Settings or `islet phone reply`); **Ring my phone** (Settings, the island's menu, `islet phone ring`) rings it loudly even on silent; **Send to your phone** puts text or a link on the phone's clipboard (`islet phone send`); and the phone's **Share → Send to computer** puts text on the computer's clipboard.
- **Release keys in one step**: `scripts/setup-signing.ps1` (Windows) or `scripts/setup-signing.sh` makes the phone app's signing key and the update key on the maintainer's own computer and adds them to the repository's secrets (or says what to paste). With them, each phone release installs over the last one and Islet on a computer can update itself in place. A [code signing policy](CODE_SIGNING_POLICY.md) and a plain checklist ([docs/signing.md](docs/signing.md)) for the free Windows signing from SignPath Foundation.

## [0.9.0] - 2026-11-23

### Changed
- **Opening and closing the island** (computer and phone): the pill stretches sideways first, then drops into a card (closing, the other way round), and the card's content unfolds row by row as the outline grows past it, a little smaller until it is open, instead of an empty black shape followed by everything popping in cut off at the edge.
- More of Islet speaks your language: the command line's messages (and so Settings' error messages), the world clocks' cities, the calendar's weekday letters and month titles, and the alarm days in Settings. The phone's calendar and its world clock cities now follow the language picked in Islet instead of the phone's. `islet --help` stays in English.

## [0.8.0] - 2026-11-23

### Added
- **Control Centre layouts** (Settings → General → Look): Classic (as before), **Modules** (a block of round buttons with the timer beside it), **Sliders** (volume and brightness as tall sliders beside the buttons) and **Dock** (a big clock with every button in one labelled row). `islet look control-centre <layout>`.
- **Your own button order** for Control Centre, used by every layout (`islet card order …`), and a **background photo** behind it, darkened so the buttons stay readable (`islet look photo <file>|none`). The photo stays on the computer.
- **Tint colour**: the Tinted and Tinted glass styles can have a colour of their own instead of the accent colour (`islet look tint <#rrggbb>|accent`).
- **Phone app**: the same four Control Centre layouts, a background photo, the button order (Look → Control Centre, Button order) and a tint colour of your own for the Tinted style.

### Fixed
- Windows (MSIX package): turning on **Start at login** in Settings could fail with "The parameter is incorrect". Settings now has its own startup task in the package, Islet finds whichever one Windows lets it see, and if none is found it says to use Task Manager → Startup apps.

## [0.7.0] - 2026-11-23

### Added
- **Ten languages**: English, Spanish, French, German, Portuguese, Russian, Chinese (Simplified), Japanese, Arabic and Hindi. Automatic follows the computer's language; `islet language <code>` picks one. The island's text, dates, menus and screen-reader labels are translated; for Chinese, Japanese, Arabic and Hindi the island uses the computer's own fonts for those letters. Settings → General → Language and the phone app's Look → Language pick it too; Settings and the phone app are translated as well. All translations live in one table, `locales/strings.tsv`, shared by the island, Settings and the phone app.

## [0.6.0] - 2026-11-23

### Changed
- Phone: the **music card** is redesigned from a Figma mock-up to match the computer's: the player's name in a tag at the top right, a progress bar in the cover's colour, a big white play/pause button between previous and next, round sleep-timer and lyrics buttons, the volume under them, and the lyrics at the bottom. With lyrics off, the card is shorter instead of leaving a gap.
- Computer: the **Now Playing card** is redesigned from a Figma mock-up: a bigger cover with rounder corners, a larger song title, a thicker progress bar, bigger play, previous and next buttons, and more room between the rows.

## [0.5.1] - 2026-11-22

### Fixed
- Phone: a finished download no longer leaves its circle on the island. Apps such as Samsung's downloader and Chrome keep a full bar in their "Download complete" notification; Islet now reads that as done, takes the ring away and says "done" once.
- Computer: a download that stopped (paused, failed, or the browser closed) no longer shows "Downloading" for ever: a partial file untouched for two minutes is left out.

## [0.5.0] - 2026-11-22

### Changed
- **Islet is completely free.** Islet Pro is gone: no licence key, no store, no Gumroad check. Everything it had is in the free app for everyone: the tinted and glass styles, the notch shape, the clock faces, the album glow and moving bars, the screenshot preview, the stopwatch, world clocks, quick notes, the to-do list, your own shortcut buttons, early builds, and no low-end limits. The Pro page in Settings is now **Extras**.
- **Performance** (Settings → Extras, or `islet performance`): Automatic, Full, Light or Power saver. Automatic is Full, or Light on a computer with 4 GB of memory or less or two cores; an old Power saver setting carries over.
- The clock card's **To-do list** and **Stopwatch** buttons are switched on in Settings → General → Clock card buttons, so Wi-Fi and Bluetooth keep their names.

### Added
- Computer: a **12-hour or 24-hour clock** (Settings → General → Clock, or `islet look hours 12|24|auto`). Automatic follows the system's language.
- Computer: the **Notch** shape (Settings → General → Shape) is free: square top corners that join the island to the top of the screen.
- Phone: **Pop-ups → Grow downward**. Pop-ups drop down from the camera in a narrow pill instead of spreading sideways, and notifications open just under the status bar, so its clock and battery (on Samsung phones, for one) stay in view.

### Removed
- `islet pro …` commands, the stored Pro key, and the release workflow's store variables (`ISLET_GUMROAD_PRODUCT`, `ISLET_BUY_URL` and the rest).

## [0.4.9] - 2026-11-22

### Changed
- Phone: the control centre looks like the computer's: the big time and date, the weather with its high and low, Wi-Fi and Bluetooth with the network's name, round to-do, speed and toggles buttons, and search, calendar, screenshot and lock under them. Opened from a card, **Now Playing ›** (or Timer ›, Directions ›…) goes back to it. The toggles button opens the flashlight, Do Not Disturb, volume and brightness, with **‹ Back** to the control centre. The clock card has the same look.
- Phone: the island's font is Inter, the computer's, by default (bundled under the SIL Open Font License); the phone's own font is still under Look → Font.

## [0.4.8] - 2026-11-22

### Added
- Islet Pro can be sold through **Gumroad** (set `ISLET_GUMROAD_PRODUCT`), as well as Polar and Lemon Squeezy. Keys are checked with Gumroad's public license API; a key works on up to five computers.
- The launch reels can be cut to any song: `scripts/promo/beats.py` finds its tempo, beats and bars, and `make-reels.py --song` starts each reel two bars before a drop, puts every cut on a bar line, times the island's moments to the beat, and drives the punches, streaks and glow from the song's kicks, snares and loudness.
- Islet Pro cover and thumbnail for the store page (`scripts/promo/make-cover.py`).
- Phone: a bigger music card, like a phone's own player: a large cover, the song in bigger type, the lyrics line being sung and the next one, a full-width progress bar, a volume slider and big buttons.
- Phone: the laptop's extras, ported:
  - with lyrics on, the line being sung slides along a row under the small music pill;
  - a **‹ Control Centre** button on every open card;
  - volume and brightness pop-ups (opt-in), earbuds' and headphones' battery in the Bluetooth pop-up, and "Fully charged";
  - the weather in the strip, what's left of today's agenda on the calendar card, and memory and storage on the Battery page;
  - a Tinted style, an accent colour for the rim, a Notch shape, the day or seconds on the empty island's clock, and a liquid neck between the pill and its bubble;
  - activities from automation apps such as Tasker (opt-in): a title and a progress ring, by broadcast.
- Phone: a **‹ Control Centre** button on the music card, and a **‹ Back** button in the control centre that returns to the card it was opened from (the music card, the clock card or the calendar).

### Fixed
- Phone: the island no longer twitches sideways as it grows or shrinks (opening and closing a card, or a pop-up such as Bluetooth on or off). Its window's top left corner now stays put and only its size changes: on many phones a window that moved showed up a frame apart from what was drawn in it.
- Phone: a download's ring glides to each new percentage instead of jumping in steps, and a download that doesn't say how far it is spins smoothly (it moved once a second). A download's notification is no longer decoded on every update.
- Phone: video players such as YouTube show their thumbnail on the island (their notification wasn't recognised as a player's), wide pictures are cropped to the middle instead of squashed, and a song or video with no picture shows its app's icon instead of a plain note.
- Phone: the battery on the empty island shows its percentage inside it.
- Phone: the island no longer jumps when the screen's insets briefly leave out the camera hole (for instance while the quick settings are open): the camera's place now comes from the screen itself, and is kept once found.

## [0.4.7] - 2026-11-19

### Fixed
- Phone: the lock and screenshot buttons explain themselves when they can't work. Android lets only an accessibility service press them; if Islet island is off, or on but stopped by the phone (some phones stop apps from outside the Play Store), the button says so and opens Accessibility settings. Set up and the home page now show when the switch is on but not running, with the fix (off and on again, and battery use set to Unrestricted).
- Phone: the island changes shape without a twitch. Its old and new contents now cross-fade (they used to fade out to nothing and back, a blink half way), the rim and the album border blend instead of switching, the second activity's bubble fades instead of popping, and the shape settles without bouncing back. A frame-by-frame test checks that it shrinks steadily, stays centred and never goes blank.

## [0.4.6] - 2026-11-17

### Fixed
- Phone: tapping the island several times no longer undoes itself. Taps closer together than a fifth of a second count once, and tapping an open card no longer closes it (its buttons still work); swipe up, tap outside or press back to close it.
- Phone: the island no longer twitches when it grows or shrinks. With the accessibility switch on, it now draws in a window that never moves, and an invisible window that follows the pill takes its taps (on many phones a moving window lands a frame late, which was the twitch).
- Phone: taps on the upper part of the small pill, and just under it, reach it more reliably: the touch area is wider and reaches further below.
- Phone: covers that players hand over as GPU-only images (common with Spotify) no longer go missing; that's why the cover sometimes didn't show after switching songs.
- Phone: when the island runs without the accessibility switch (where the status bar takes its taps), it says once how to fix that.

## [0.4.5] - 2026-11-16

### Added
- A download table at the top of the README, and promo videos made from the island's own drawing code (docs/promo.md, scripts/promo/make-videos.py).

### Fixed
- Phone: the cover beside a new song shows straight away and is never the last song's. Covers and lyrics now have their own worker, so they no longer wait behind the weather, the calendar or a speed test, and a player's notification picture is only used when it's for the song playing.
- Computer: on the open music card, a lyric line leaving at the top no longer slides over the play buttons; it rises half a row and is gone.
- Phone: the bars that follow the sound stop reading when Islet's service stops.
- Phone: the big time on the clock card shrinks to fit instead of being cut to "10:08…" on narrow phones that show AM/PM.
- Phone: lyrics keep time better: each line shows 0.15 s early, as on the computer, and podcasts or audiobooks played faster keep their lines in step.

## [0.4.4] - 2026-11-16

### Fixed
- Phone: taps on the small pill open it reliably. A quick thumb tap that slid a little was taken for a sideways swipe and did nothing; now only a real swipe (about a third of an inch) counts as one, and the area just under the pill takes taps too. A light buzz confirms each tap.
- Phone: changing a setting that changes the island's size (Time on the empty island, the size, the position nudges) left it cut off as a square block until something else happened. It now moves to the new size straight away.

## [0.4.3] - 2026-11-15

### Added
- The phone's empty island shows the **time** beside the camera and the **battery** on the other side, as the computer's does (Look → Time on the empty island).
- A **lyrics button** on the phone's music card: one tap switches synced lyrics on (or off). A song without synced lyrics says so.

### Fixed
- Phone: the album cover (and the island's border in its colour) was missing for players that add the cover a moment after the song, or only put it in their notification, such as Spotify. The island now takes it from either.
- Phone: the island opens and closes faster (about a third of a second, and what's inside shows sooner), and stays smooth while music plays: the glowing rim, the clock and the control centre no longer ask the system for slow readings on every frame.
- Phone: the island no longer twitches sideways when it grows or shrinks (a Bluetooth or Wi-Fi pop-up, for example). Android moves the island's window a frame after it's asked to, and the island was drawn for the new place a frame too early.
- Phone: paused music no longer stays on the island for good. As on the computer, it leaves 15 seconds after you pause, and the time comes back.

## [0.4.2] - 2026-11-15

### Added
- The rest of the computer's extras on the phone island:
  - a **class timetable**, counted down on the island from 15 minutes before each lesson, then the time left in it, with optional Do Not Disturb during lessons;
  - **deadlines** from your calendar (exam, essay, assignment, due…) on the calendar card, counted down;
  - **alarms** set from Islet, kept by the phone's Clock app, with the next one on the calendar card;
  - the **focus streak**, on the timer and in Timer & focus;
  - a **sleep timer** for music (the moon on the music card);
  - a **quick launcher** from the island's card, for apps and your own links;
  - a **Battery** page with health, temperature, voltage and charge cycles (Android 14 and later);
  - **network speed**, live in the Speed test page and, if you choose, in the island's strip;
  - optional **bars that follow the sound** of the music.

## [0.4.1] - 2026-11-15

### Changed
- **The phone app looks like Islet's Settings on the computer:** a list of pages with coloured tiles, and grouped cards with switches, light and dark.
- **The phone island looks like the computer's:** a tap on the empty island shows the strip (battery, time, memory use); a second tap, the card with the time, Wi-Fi and Bluetooth buttons and round buttons for notes, the speed test, controls, the calendar, a screenshot and lock. The calendar button shows the month with timer buttons.
- The phone island sits round the camera with or without the accessibility switch, a little thicker, with a soft glowing rim (a switch in Look).
- Notification pop-ups show the sender's picture or initial, a count, and the notification's own buttons; **Reply** opens a box to answer in. Ongoing calls show green bars; Silent and Do Not Disturb read like the computer's.

### Added
- **Paid** pop-ups for wallet payments, with the amount.
- A frosted **Glass** style for the phone island.

### Fixed
- The phone app's screen no longer sits under the status bar on Android 15.

## [0.4.0] - 2026-11-15

### Added
- **An island on your Android phone:** the Islet app now draws its own island round the phone's front camera (placed over the status bar through an accessibility switch that reads nothing, or below it with "display over other apps"). It shows:
  - music from any player, with the cover, moving bars, a draggable progress bar, controls, synced lyrics and a border in the album's colour;
  - calls with the phone app's own answer and hang-up buttons, directions from maps apps, other apps' countdowns, downloads with a progress ring, and meetings with a Join button;
  - its own focus timer (with focus rounds) and stopwatch with laps;
  - pop-ups for notifications, charging and low battery, ring / vibrate / silent, airplane mode, Do Not Disturb, Wi-Fi and Bluetooth, screenshots and finished timers;
  - a clock card with weather (Open-Meteo), world clocks, the next meeting and alarm, and your first to-do;
  - a control centre on a long press: flashlight, Do Not Disturb, ring or vibrate, volume, brightness, screenshot, lock and shortcut apps.
  
  The app also gains looks settings (size, position nudges, fonts, °C/°F), to-dos, quick notes and a speed test. See docs/phone.md, including what Android doesn't allow (clipboard history, per-app volume, the system monitor, plugins).

### Changed
- **Pairing your Android phone is free:** it no longer needs Islet Pro. The pairing code has moved to Settings → **Wi-Fi, Bluetooth & phone** → Your Android phone.

## [0.3.2] - 2026-11-14

### Fixed
- Windows: a PowerShell or console window no longer flashes up when the island opens the Controls or system panels. The island reads brightness, battery health and network use with Windows' own tools, and each one used to open a visible window while it ran; now none of them do.

## [0.3.1] - 2026-11-14

### Added
- **Fonts:** the island's text in any font installed on your computer (Settings → Look → Font, or `islet fonts` and `islet look font <name>`); nothing is bundled or copied.
- **Celsius or Fahrenheit** as a clear °C / °F choice (Settings → Look and the Weather page).
- **A border in the album's colour** round the island while a song plays (free; Settings → Look).
- **Airplane mode and Do Not Disturb:** a pop-up when either turns on or off, and a small plane or moon beside the time on the empty island while it's on. Linux: airplane mode from the radios' switches (rfkill), Do Not Disturb from GNOME or KDE. Windows: airplane mode, and Do Not Disturb through Windows 11's Focus sessions. Not on a Mac (no airplane mode; macOS shares Focus only with App Store apps). Switch: Settings → General → Pop-ups, or `islet set mode-indicators off`.

### Changed
- Settings' dropdowns are redrawn to match the rest of the app (the system's plain white lists were hard to read), with search for long lists, and the island's size is three buttons.

### Fixed
- The empty island no longer pops open whenever the pointer passes over the top of the screen: it grows (and shows its status strip) once the pointer rests on it for a third of a second.
- The clock card's Wi-Fi and Bluetooth buttons are blue while Wi-Fi or Bluetooth is switched on (connected or not) and grey when it's off. Wi-Fi no longer stays blue after being switched off, and the island reads the state again once a radio has settled.

## [0.3.0] - 2026-11-14

### Added
- **Islet Pro** (optional, one payment): a licence key checked with the store, kept in the system keychain, good on three computers; being offline never turns it off. Everything in 0.2.0 stays free. See [docs/pro.md](docs/pro.md).
  - No low-end limits (60 fps waveform, sharper album art, 500 clipboard copies, 100 shelf files), and a **Power saver** switch that brings the light limits back and holds the waveform still.
  - Stopwatch with laps, world clocks, a to-do list on the island, quick notes saved to a text file, shortcut buttons of your own (also in the right-click menu).
  - Looks: tinted and glass styles, a notch shape, a glow in the album's colour, the song's progress as moving bars, clock faces with the day or with seconds.
  - Screenshot preview: each new screenshot shows on the island; tap its card to open it.
  - Early builds: `islet update check --early`.
  - **Your Android phone on the island:** a companion app sends the phone's notifications, music and battery to the island over your Wi-Fi, encrypted with a key shown once in a QR code; the island's music buttons control the phone. See [docs/phone.md](docs/phone.md).
- **Clock card:** Wi-Fi and Bluetooth chips, round tool buttons you can show or hide, a quick timer, a month calendar, screenshot and lock buttons, and twelve accent colours.
- **Controls panel:** volume and brightness sliders and a dark-mode switch.
- **Status strip:** point at the empty island for the weather, battery and processor load beside the time.
- **Calendar:** a Join button for events with a video-call link; deadlines and exams count down ("Due in 6 days"); the feed is read six weeks ahead.
- **Class timetable:** enter your week once; lessons show ten minutes before, and the clock card counts down to the next one and shows free gaps.
- **Focus:** a streak of days with a finished round, and "do not disturb while working".
- **Performance alert** (opt-in): names a program hogging the processor or memory, with Close it and Ignore.
- **System panel:** network speed and battery health.
- **Navigation:** an arrow for the first turn.
- **Wi-Fi pop-up:** "Connected" and the network's name when Wi-Fi joins a network.
- **Clipboard history** keeps pictures too (with thumbnails), and never keeps copies from the apps you list (password managers are listed already).
- **Per-app rules:** the island steps aside while apps you list are in front (a game, Steam, slides).
- **Position and size per display:** top centre, top left, top right or bottom; and its own size on each display.
- **More shortcuts:** keys of your own for the launcher and for a quick note, beside the one that opens the island.
- **Sleep timer** in the right-click menu; an "update available" pill; a quick **launcher** in Settings (`islet launcher`) for apps, web searches, sums and unit conversions.
- **Settings:** a new look, with a page for Islet Pro, the timetable and the launcher.
- **App switcher:** the launcher finds open windows by title or app and brings the one you choose to the front (X11, sway, Hyprland, Windows; apps on macOS). `islet windows` and `islet windows focus <id>` do the same from a terminal.
- **Speed test** in Settings (Wi-Fi & Bluetooth) and `islet speedtest`: latency, download and upload, only when asked, with Cloudflare's public speed-test servers (about 36 MB).
- **Plugin widgets** (off by default): your own scripts in the plugins folder, run on their own schedule, shown as live activities (docs/plugins.md, `islet plugins`).
- **Drag files out of the shelf** into Finder, Explorer, a file manager or any app (Windows, macOS, X11 and Wayland).
- **Per-app volume** on the Controls panel: a slider for each app playing sound (up to four), on Linux (pactl) and Windows (Core Audio sessions).

### Fixed
- Labels cut short on the clock card and in the Wi-Fi, Bluetooth and shelf lists.
- CI no longer fails when Ubuntu's package servers are unreachable.
- macOS: sliders (volume, brightness, the song position) follow the pointer while the button is held.

### Not included
- A per-app volume mixer on macOS: there is no public API for one app's volume.
- A brightness slider on macOS: setting the screen's brightness there needs private APIs, which Islet doesn't use.

## [0.2.0] - 2026-11-06

The first published release: everything in 0.1.0 (below), plus:

### Added
- **Out of the way over fullscreen:** when a video, game or presentation goes fullscreen on its display, the island hides (music and all) and shows again while you point at the top of the screen. Alarms and timers still ring through. Windows and X11 follow the fullscreen window; macOS checks when the Space or app changes; on Wayland the island sits on the layer fullscreen windows cover.
- **Right-click menu** on the island: Islet Settings, hide for an hour, pause pop-ups for an hour, lyrics on or off and their timing for the song, and Quit (right-click used to quit straight away).
- **Calls:** once an app has had the microphone for a few seconds, the island shows the call (the app, how long it's run) with a mute button for the microphone (pactl on Linux, Core Audio on Windows, CoreAudio on macOS). Switch: Settings → Privacy, or `islet set calls off`.
- **Volume slider** on the Now Playing card (the computer's output volume; no pop-up for its own changes).
- **Your own .lrc lyric files** are used first: beside the song file (Linux players say which file), or in the `lyrics` folder of Islet's settings folder, before asking LRCLIB.
- **Downloads** (opt-in): the Downloads folder is watched for Chrome/Edge/Brave `.crdownload`, Firefox `.part` and Safari `.download` files; the newest shows with its size and speed, then "Downloaded" opens the file when tapped. No polling while nothing downloads. `islet activity start … --open <file or link>` gives script activities the same tap-to-open.
- **An island on every display** (Settings → Look → Display → All displays, or `islet look display all`): the main island starts a copy on each other display that shows music, calls, batteries, downloads and Control Centre, follows the settings, and quits with it. Alarms, timers, the shortcut and network features stay with the main island.
- Windows installers are code-signed through SignPath once the project's keys are set (docs/signing.md).

### Changed
- Lighter on macOS: frames go to Core Animation through IOSurfaces, which it shows without copying; handing over a frame takes about a sixth of the time it did, and animating uses about 40 % less CPU (falls back to the old way where IOSurfaces aren't available). CI checks the frames show right side up.
- Dependencies: zune-jpeg 0.5, sha2 0.11; GitHub Actions updated.

## [0.1.0] - 2026-11-06

Built but never published (its release step tripped over a stray folder); its contents ship in 0.2.0.

### Island
- Native, webview-free island for macOS, Windows and Linux (native Wayland on KDE Plasma, sway, Hyprland and other wlroots compositors; X11 and XWayland elsewhere): about 5 MB of RAM, 0 % CPU and zero wakeups when idle.
- iPhone-style shapes and motion: continuous-corner squircles, spring animations, peek, compact, the split pill with a metaball neck for two activities, the expanded card and a large view. Wraps the notch on MacBooks.
- **The time on the empty island:** when nothing is playing, the pill shows the current time (12- or 24-hour, as your system is set); switch it off in Settings → Look. It wakes once a minute to change the time, and not at all when hidden.
- **Hover-only mode** (Settings → Look): the empty island stays hidden until you point at it; music and other live activities always show.
- **Control Centre from any card:** a "‹ Control Centre" button on every activity's card, and a back button on the Wi-Fi, Bluetooth, shelf, clipboard and system lists.
- Smooth 60 fps on Windows: frames are timed with a high-resolution waitable timer instead of a millisecond timeout that Windows rounded up to its 15.6 ms tick.
- Buttons respond while a card is still settling, so a quick tap on "Control Centre" no longer closes the island; the Wi-Fi and Bluetooth switches flip as soon as you tap them, follow the radio's real state on Windows, and have a bigger tap area; lists show four rows above a footer button (five without) and scroll for more, so they're never cut off. The waveform and other upright bars are drawn thin instead of as blobs.
- Inter bundled for text. San Francisco is used on macOS, from the system.

### Activities
- **Now Playing** for any app: artwork, title, artist, a live waveform, progress with scrubbing, and controls. Linux uses MPRIS, Windows the system media session, and macOS a MediaRemote helper with a Spotify/Music fallback.
- **Battery pop-ups** when headphones, earbuds, mice, keyboards or controllers connect, plus low-battery alerts.
- **Charging notification:** plug in and the island shows a green bolt, "Charging", the level and a filling battery; unplugging shows "On battery", reaching 100 % shows "Fully charged", and earbuds or a mouse that start charging get their own pop-up.
- **Navigation ETA** with live traffic via the Google Routes API (your own key, kept in the OS keychain), adaptive refresh, and hand-off to Google Maps or Waze.
- **Alarms**: one-time or repeating. When one rings, the island opens with a swinging bell and plays an original chime until you snooze or stop it.
- **Wi-Fi and Bluetooth** pickers from the clock card: join networks and connect paired devices.
- **Spotify on your other devices** (optional): playback on your phone or a speaker shows on the island, with controls, using your own Spotify app.
- **Synced lyrics** (opt-in, from LRCLIB): the lyrics slide through the compact island as the song plays, with the line being sung highlighted and its neighbours dimmed, and two rolling lines on the Now Playing card.
- **Hide Now Playing** for chosen apps; opt-in **web artwork** on Linux.
- **File shelf:** drag files onto the island to keep shortcuts to them; tap to open.
- **Notifications** mirrored on the island (Linux, opt-in), with the latest few one click away.
- **Calendar:** "up next" on the island ten minutes before an event, and today's agenda, from your calendar's private iCal link (Google, Outlook, iCloud).
- **Weather** on the clock card from Open-Meteo (no key; off until you choose a place).
- **Volume, brightness and caps-lock pop-ups** on the island (each can be switched off).
- **Timer and focus sessions:** a countdown ring on the island, Pomodoro work/break rounds with a chime at each change, and Repeat when it ends.

- **Screen readers:** the island describes itself and exposes its controls to Orca, Narrator/NVDA and VoiceOver.

- **GNOME Shell extension** (GNOME 45–50): a shortcut that opens and closes the island, the Caps Lock pop-up on GNOME Wayland, and an option to hide the clock the island covers.
- **Windows MSIX package**: gives Islet a package identity, so it can show other apps' notifications on the island (opt-in), and starts at login through Windows' startup list. Signed with a test certificate until a real one is configured.
- **System monitor** on the clock card (CPU, memory, disk), sampled only while it's open.
- **Clipboard history** (opt-in, memory only, password-manager copies skipped).
- **Wi-Fi and Bluetooth switches** in their panels, and `islet wifi|bt on|off`.
- **Global shortcut** to open and close the island (Settings → Shortcut, `islet hotkey`), and `islet toggle` for Wayland desktops.
- **Look:** accent colour, island size, gap from the top and the display to use (Settings → Look, `islet look`).
- **Destination suggestions** from Google Places while you type a trip (your key, Places API (New)).
- **Camera and microphone dots:** a green dot (camera) or orange dot (microphone) at the right end of the closed island while any app uses them, with the app's name for screen readers (Linux and Windows name the app; macOS doesn't say which).

- **Live activities for scripts and apps:** `islet activity start|update|end` shows your own activity (a build, a download, a delivery) with a progress ring and bar.

### Apps and tools
- Islet Settings: a first-run welcome with permissions, start at login, Wi-Fi & Bluetooth, alarms, navigation key and trip, updates (opt-in checks) and about.
- `islet` command line: `nav`, `key`, `alarm`, `wifi`, `bt`, `autostart`, `status`, `update check`, `quit`.
- Installers: .deb, .rpm and AppImage for Linux; .msi, setup .exe and a portable zip for Windows; a universal .dmg for macOS. SHA-256 checksums, and signed in-app updates once a signing key is configured.
