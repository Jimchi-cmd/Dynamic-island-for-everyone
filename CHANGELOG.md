# Changelog

All notable changes are listed here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow [Semantic Versioning](https://semver.org/).

## [0.3.0] - Unreleased

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
- **Per-app volume** on the Controls panel: a slider for each app playing sound (up to four), on Linux (pactl) and Windows (Core Audio sessions).

### Fixed
- Labels cut short on the clock card and in the Wi-Fi, Bluetooth and shelf lists.
- CI no longer fails when Ubuntu's package servers are unreachable.

### Not included
- Dragging files out of the shelf: it needs per-system code that could not be verified on Windows and macOS.
- A per-app volume mixer on macOS: there is no public API for one app's volume.

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
