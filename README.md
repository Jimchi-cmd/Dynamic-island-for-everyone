# Islet

**A Dynamic Island-style overlay for Windows, macOS and Linux.** Music, battery pop-ups, alarms, Wi-Fi and Bluetooth, and your navigation ETA, in a smooth pill at the top of your screen.

[![CI](https://github.com/Jimchi-cmd/Dynamic-island-for-everyone/actions/workflows/ci.yml/badge.svg)](https://github.com/Jimchi-cmd/Dynamic-island-for-everyone/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

The island is a tiny native program with no web browser inside. It idles at **about 5 MB of RAM, 0 % CPU and zero wakeups**, starts in well under a second, and its installers are 3–5 MB, so it runs happily on a 4 GB laptop. CI enforces these numbers on every change ([perf.md](docs/perf.md)).

> **Status:** v0.1.0 is being prepared. Everything below works and is tested on Linux. The Windows and macOS code builds and passes CI, but needs testing on real machines. See [what each OS supports](docs/roadmap.md).

## What it does

- **The island itself:** iPhone-style continuous corners and spring motion. Hover to peek, click to expand, press and hold for more, scroll to switch between activities. Two activities split into a pill and a bubble joined by a liquid neck. On a notched MacBook it wraps the notch.
- **Now Playing** from any app: artwork, a live waveform, progress you can drag, and play/pause/skip.
- **Battery pop-ups** when headphones, earbuds, mice, keyboards or controllers connect, plus low-battery and charger alerts.
- **Volume, brightness and caps-lock pop-ups**, like the iPhone's.
- **Timer and focus sessions:** a countdown ring on the island, or Pomodoro rounds of work and breaks.
- **Alarms:** one-time or repeating. When one rings, the island opens with a swinging bell and a chime until you snooze or stop it.
- **Wi-Fi and Bluetooth:** open the island, tap Wi-Fi or Bluetooth on the clock card, and pick a network or a device.
- **Navigation ETA** with live traffic, arrival time and the first turn, then one tap hands off to Google Maps or Waze. Uses your own free-tier Google key ([setup](docs/navigation.md)).
- **Settings** for all of the above, with a first-run welcome that explains every permission.

## Install

Get the latest version from [Releases](https://github.com/Jimchi-cmd/Dynamic-island-for-everyone/releases/latest):

| | Download |
|---|---|
| Windows 10/11 | `…_x64-setup.exe`, `.msi`, or the portable `.zip` |
| macOS 11+ (Apple silicon and Intel) | `…_universal.dmg` |
| Linux | `.deb`, `.rpm` or `.AppImage` |

Islet isn't code-signed yet, so Windows SmartScreen and macOS Gatekeeper will warn the first time. **[docs/install.md](docs/install.md)** walks through getting past each warning safely, plus permissions, updating and uninstalling.

## Privacy

- No accounts, no analytics, no telemetry.
- Islet uses the network only for features you turn on: the navigation ETA (straight to Google, with your key) and update checks (off by default).
- Your Google key is kept in the OS keychain, never in a file. Route results are never stored.
- Wi-Fi and Bluetooth lists load only when you open them.

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
```

## Build from source

Requirements: Rust (stable). The Settings app also needs Node 22 and the [Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/).

```bash
cargo run --release -p islet                       # the island
ISLET_DEMO=media cargo run --release -p islet      # a scripted music + battery demo
npm ci && npm run tauri dev                        # Settings
```

Right-click the island to quit.

## Known limitations

- **Windows:** apps can't connect Bluetooth devices, so Islet lists them and opens Bluetooth settings. Listing Wi-Fi networks needs Location permission on Windows 11.
- **macOS:** Now Playing for every app uses a small helper that runs through the system's Perl, because Apple restricted the MediaRemote API in macOS 15.4. If a macOS update breaks it, Islet falls back to Spotify and Apple Music. Wi-Fi names need Location permission.
- **Linux:** on GNOME Wayland the island runs through XWayland. Web artwork (Spotify covers, for example) isn't downloaded yet.
- Alarms ring only while the computer is awake and Islet is running.
- No live turn-by-turn from your phone yet. An Android companion is planned.

## Contributing

Bug reports and pull requests are welcome. Read [CONTRIBUTING.md](CONTRIBUTING.md), including how to add a new activity, and the [code of conduct](CODE_OF_CONDUCT.md). Report security problems privately ([SECURITY.md](SECURITY.md)).

More docs: [roadmap](docs/roadmap.md) · [design spec](docs/design-spec.md) · [architecture](docs/architecture.md) · [performance](docs/perf.md) · [navigation](docs/navigation.md) · [signing](docs/signing.md) · [publishing checklist](docs/publishing-checklist.md) · [changelog](CHANGELOG.md)

## Legal

Islet is not affiliated with, endorsed by or sponsored by Apple Inc. "Dynamic Island", "iPhone", "macOS" and "San Francisco" are trademarks of Apple Inc. Islet doesn't include Apple's fonts, icons, symbols or sounds: it draws its own icons, synthesises its own alarm chime, and on macOS uses the system font that's already on your Mac. Google Maps is a trademark of Google LLC; Waze is a trademark of Waze Mobile Ltd. Windows is a trademark of Microsoft Corporation. All other product names are trademarks of their respective owners.

Islet is licensed under the [MIT License](LICENSE). Third-party components are listed in [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md).
