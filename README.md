# Islet

**A Dynamic Island-style overlay for Windows, macOS and Linux.** Music, battery pop-ups, alarms, Wi-Fi and Bluetooth, and your navigation ETA, in a smooth pill at the top of your screen.

[![CI](https://github.com/Jimchi-cmd/Dynamic-island-for-everyone/actions/workflows/ci.yml/badge.svg)](https://github.com/Jimchi-cmd/Dynamic-island-for-everyone/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

The island is a tiny native program with no web browser inside. It idles at **about 5 MB of RAM, 0 % CPU and zero wakeups**, starts in well under a second, and its installers are 3–5 MB, so it runs happily on a 4 GB laptop. CI enforces these numbers on every change ([perf.md](docs/perf.md)).

> **Status:** v0.1.0 is being prepared. Everything below works and is tested on Linux. The Windows and macOS code builds and passes CI, but needs testing on real machines. See [what each OS supports](docs/roadmap.md).

## What it does

- **The island itself:** iPhone-style continuous corners and spring motion. When nothing is playing it shows the time. Over a fullscreen video, game or presentation it steps aside, and comes back when you point at the top of the screen. Hover to peek, click to expand, press and hold for more, scroll to switch between activities. Two activities split into a pill and a bubble joined by a liquid neck. On a notched MacBook it wraps the notch.
- **Now Playing** from any app: artwork, a live waveform, progress you can drag, play/pause/skip and a volume slider. Optionally, Spotify playback on your phone or speakers too.
- **Synced lyrics** (opt-in): the lyrics slide through the island as the song plays, with the line being sung lit up and the ones around it dimmed. Two lines on the open card too. From [LRCLIB](https://lrclib.net), a free, open lyrics database.
- **Battery pop-ups** when headphones, earbuds, mice, keyboards or controllers connect, plus low-battery alerts.
- **Charging notification** when you plug in (and "Fully charged" at 100 %), like the iPhone's.
- **Volume, brightness and caps-lock pop-ups**, like the iPhone's.
- **Weather** on the clock card, from Open-Meteo (no account or key).
- **File shelf:** drag files onto the island to keep them one tap away (it keeps shortcuts, never copies).
- **Notifications** on the island (Linux, opt-in).
- **Calendar:** what's next, ten minutes before it starts, and today's agenda (Google, Outlook or iCloud via their private iCal link).
- **Timer and focus sessions:** a countdown ring on the island, or Pomodoro rounds of work and breaks.
- **Alarms:** one-time or repeating. When one rings, the island opens with a swinging bell and a chime until you snooze or stop it.
- **Wi-Fi and Bluetooth:** open the island, tap Wi-Fi or Bluetooth on the clock card, and pick a network or a device, or switch the radio on or off.
- **Control Centre from anywhere:** every open card (music, a timer, navigation…) has a "‹ Control Centre" button at the top that goes to the clock card with Wi-Fi, Bluetooth, the shelf and the system monitor, and lists have a back button.
- **Downloads** (opt-in): what your browser is downloading, with its size and speed, then "Downloaded": tap to open it.
- **Calls:** while Zoom, Teams, a browser call or any app has the microphone, the island shows how long the call has run, with a button that mutes your microphone.
- **Camera and microphone dots:** a green dot while an app uses the camera, an orange one for the microphone (your screen reader hears which app).
- **System monitor:** CPU, memory and disk on the clock card, read only while you look at it.
- **Clipboard history** (opt-in): your last 20 copied texts (500 with Islet Pro), one tap to copy again. Kept in memory only; password-manager copies are skipped.
- **Live activities** for your scripts and apps: `islet activity start build --title Build --progress 40` puts a progress ring on the island.
- **Make it yours:** accent colour, island size, gap from the top, which display (or every display), and a global shortcut to open it. Or hide the empty island until you point at it: anything live (music, timers, pop-ups) still shows.
- **Navigation ETA** with live traffic, arrival time and the first turn, then one tap hands off to Google Maps or Waze. Destination suggestions as you type. Uses your own free-tier Google key ([setup](docs/navigation.md)).
- **Settings** for all of the above, with a first-run welcome that explains every permission.

## Install

Get the latest version from [Releases](https://github.com/Jimchi-cmd/Dynamic-island-for-everyone/releases/latest):

| | Download |
|---|---|
| Windows 10/11 | `…_x64-setup.exe`, `.msi`, the portable `.zip`, or the `.msix` (for notifications) |
| macOS 11+ (Apple silicon and Intel) | `…_universal.dmg` |
| Linux | `.deb`, `.rpm` or `.AppImage` |

**New to Islet? Read the [step-by-step manual](docs/manual.md)**: downloading, installing on each system, first start, every feature, and fixes for common problems.

Islet isn't code-signed yet, so browsers, Windows SmartScreen and macOS Gatekeeper may warn the first time. The manual and **[docs/install.md](docs/install.md)** walk through getting past each warning safely, plus permissions, updating and uninstalling.

## Privacy

- No accounts, no analytics, no telemetry.
- Islet uses the network only for features you turn on: Spotify Connect (if you link it), your calendar link (every 15 minutes), weather (Open-Meteo, hourly, only the place you chose), lyrics (LRCLIB, once per song: its title, artist, album and length), the navigation ETA (straight to Google, with your key) and update checks (off by default).
- Your Google key is kept in the OS keychain, never in a file. Route results are never stored.
- Wi-Fi and Bluetooth lists load only when you open them.
- Destination suggestions (Google Places, your key) are requested only while you type in Settings.
- Lyrics are kept in memory for the session only.
- Clipboard history is off until you turn it on, lives in memory only, and is forgotten when you turn it off or quit.

## Command line

`islet --help` lists everything. A few examples:

```bash
islet alarm add 6:30am --days weekdays --label "Wake up"
islet timer start 10m --label Tea
islet focus --work 25m --break 5m --rounds 4
islet wifi list
islet bt list
islet key routes                                    # paste your Google Routes key (stored in the keychain)
islet nav start "Kampala Road" --from "Ntinda" --label Work
islet autostart on
islet activity start build --title "Build" --progress 40   # your own live activity
islet activity end build
islet hotkey Ctrl+Alt+I                             # a shortcut that opens and closes the island
islet look accent green                             # also: look size large, look offset 12, look display HDMI-1
islet set clipboard on                              # opt-in clipboard history
islet set lyrics on                                 # synced lyrics from LRCLIB
islet set auto-hide on                              # show the empty island only on hover
```

## Build from source

Requirements: Rust (stable). The Settings app also needs Node 22 and the [Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/).

```bash
cargo run --release -p islet                       # the island
ISLET_DEMO=media cargo run --release -p islet      # a scripted music + battery demo
npm ci && npm run tauri dev                        # Settings
```

Right-click the island for its menu: Islet Settings, hide it for an hour, pause pop-ups for an hour, lyrics on or off and their timing, and Quit.

## Known limitations

- **Windows:** apps can't connect Bluetooth devices, so Islet lists them and opens Bluetooth settings. Listing Wi-Fi networks needs Location permission on Windows 11.
- **macOS:** Now Playing for every app uses a small helper that runs through the system's Perl, because Apple restricted the MediaRemote API in macOS 15.4. If a macOS update breaks it, Islet falls back to Spotify and Apple Music. Wi-Fi names need Location permission.
- **Linux:** native Wayland on KDE Plasma, sway, Hyprland and other wlroots compositors; on GNOME it runs through XWayland. Dropping files on the shelf works there too. The caps-lock pop-up and Islet's own global shortcut need X11 or XWayland, because Wayland only tells the focused app about the keyboard and Islet never takes focus; on native Wayland, bind a key to `islet toggle` in your desktop's settings. Clipboard history reads the clipboard through XWayland, which every desktop above runs. The microphone dot needs `pactl` (PulseAudio or PipeWire's pulse server). Web artwork (Spotify covers, for example) is downloaded only if you turn it on in Settings. Over fullscreen windows on native Wayland the island stays behind them (Wayland doesn't tell other apps what's fullscreen), so pointing at the top doesn't bring it back there.
- Notifications on the island work on Linux and with the Windows **MSIX** package (Windows allows it only for packaged apps); macOS doesn't allow it at all. The MSIX is signed with a test certificate for now ([how to install it](docs/install.md#installing-the-msix)).
- On GNOME, the shortcut and the Caps Lock pop-up need the small [GNOME Shell extension](docs/install.md#gnome-shell-extension-gnome-4550).
- Alarms ring only while the computer is awake and Islet is running.
- macOS doesn't say which app uses the camera or microphone, so the dot there has no name. It has no public Bluetooth power switch either, so the Bluetooth toggle points to System Settings.
- No live turn-by-turn from your phone yet. An Android companion is planned.

## Contributing

Bug reports and pull requests are welcome. Read [CONTRIBUTING.md](CONTRIBUTING.md), including how to add a new activity, and the [code of conduct](CODE_OF_CONDUCT.md). Report security problems privately ([SECURITY.md](SECURITY.md)).

More docs: [roadmap](docs/roadmap.md) · [design spec](docs/design-spec.md) · [architecture](docs/architecture.md) · [performance](docs/perf.md) · [navigation](docs/navigation.md) · [signing](docs/signing.md) · [publishing checklist](docs/publishing-checklist.md) · [changelog](CHANGELOG.md)

## Legal

Islet is not affiliated with, endorsed by or sponsored by Apple Inc. "Dynamic Island", "iPhone", "macOS" and "San Francisco" are trademarks of Apple Inc. Islet doesn't include Apple's fonts, icons, symbols or sounds: it draws its own icons, synthesises its own alarm chime, and on macOS uses the system font that's already on your Mac. Weather data by [Open-Meteo.com](https://open-meteo.com/) (CC BY 4.0). Google Maps is a trademark of Google LLC; Waze is a trademark of Waze Mobile Ltd. Windows is a trademark of Microsoft Corporation. All other product names are trademarks of their respective owners.

Islet is licensed under the [MIT License](LICENSE). Third-party components are listed in [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md).
