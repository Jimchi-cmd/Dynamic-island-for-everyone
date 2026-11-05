# Roadmap

Each phase ships with tests and green CI on Windows, macOS and Linux before the next one starts.

| Phase | Scope | Status |
|---|---|---|
| **0** | Scaffold, perf harness, CI matrix, stack decision (native island) | ✅ done |
| **1** | Island shell: full state machine, squircle morph, metaball split, input (hover, click, hold, scroll, Esc), notch wrap on Macs, Wayland layer-shell, macOS click-through, text rendering (Inter / San Francisco), clock activity, AccessKit, golden-image tests | in progress: ✅ state machine, morph, metaball split, hover/click/hold/Esc/auto-collapse, scroll to switch, text (Inter; San Francisco on macOS), clock, notch wrapping and click-through on macOS (needs testing on a Mac), golden-image tests · ⬜ native Wayland layer-shell (runs via XWayland today), AccessKit screen-reader support |
| **2** | Activity framework and the **music controller** (Windows SMTC, Linux MPRIS, macOS media adapter): art, title, progress and scrubbing, controls, waveform. **Device battery**: connect pop-ups for headphones, mice, keyboards and controllers, low-battery alerts, laptop charging | in progress: ✅ views (compact, two-at-once, player card, device list), activity logic, Linux MPRIS + UPower (tested end to end), Windows SMTC + Bluetooth battery + laptop (compiles; needs testing on a PC), macOS every app with album art via the MediaRemote helper, falling back to Spotify/Music, + IOKit battery (compiles; needs testing on a Mac) · ⬜ AirPods per-earbud levels, settings, exclusions list |
| **3** | Navigation ETA (Google Routes, bring your own key) and Google Maps / Waze hand-off; optional Spotify Web API | in progress: ✅ live ETA with traffic, arrival time, distance and first instruction from the Routes API (key in the OS keychain, adaptive refresh, no caching, Google attribution), Google Maps / Waze hand-off buttons, `islet nav` / `islet key` commands. See [navigation.md](navigation.md) · ⬜ destination entry in Settings (Phase 4), Places autocomplete, optional Spotify Web API |
| **3b** | **Alarms**: one-time or repeating alarms; when one rings, the island opens with a swinging bell (the bell stays at the right of the pill when collapsed) and an original chime plays until Snooze (9 min) or Stop | ✅ done: alarm logic (catches up after sleep, never rings hours late), island views, chime synthesised by Islet and played with the OS player, `islet alarm` commands, Alarms page in Settings |
| **3c** | **Wi-Fi & Bluetooth**: pick a Wi-Fi network or a paired Bluetooth device from the island (clock card → Wi-Fi / Bluetooth) or Settings | ✅ Linux via NetworkManager + BlueZ (tested end to end against fake services); Windows Wi-Fi via WiFiAdapter and macOS via CoreWLAN + IOBluetooth (compile; need testing on real machines). See the notes below |
| **4** | Release pipeline and v0.1.0: installers for every OS, auto-update, Settings app, first-run permissions screen | in progress: ✅ Settings app with first-run welcome and permissions, installers (deb, rpm, AppImage, msi, setup exe, portable zip, universal dmg), tag-triggered draft releases with checksums, signed in-app updates (once a key is set up), README, CONTRIBUTING, CODE_OF_CONDUCT, SECURITY, CHANGELOG, issue and PR templates, third-party licenses, [install](install.md), [signing](signing.md) and [publishing](publishing-checklist.md) guides · ⬜ the v0.1.0 tag (maintainer), smoke tests on real Windows and Mac machines |
| **5** | Notifications, volume/brightness/caps-lock HUD, calendar, file shelf, focus timer, weather | in progress: ✅ timer and focus sessions (countdown ring on the island, Pause/Resume/Stop, chime and quick look at each phase change, Repeat when done; keeps time across sleep); volume, brightness and caps-lock pop-ups (see below); weather on the clock card (Open-Meteo, off until a place is chosen, hourly) · ⬜ calendar, notifications, file shelf |
| **6** | Camera/mic indicators, system monitor, clipboard history, quick toggles | |
| **7** | Themes, size and position, per-monitor settings, hotkeys UI, plugin API for live activities, GNOME Shell extension | |
| **8** | Android companion for live turn-by-turn from Google Maps / Waze (separate plan) | |

## Device battery: how each OS reports it

| OS | Source | Notes |
|---|---|---|
| Windows | Device property `DEVPKEY_Bluetooth_Battery` (what Settings → Bluetooth & devices shows) + a `DeviceWatcher` for connect/disconnect events | Bluetooth Classic (hands-free battery reports) and Bluetooth LE devices |
| macOS | IOBluetooth connect notifications; HID `BatteryPercent` from IOKit (mice, keyboards, trackpads); per-earbud and case levels where macOS exposes them | asks once for Bluetooth permission |
| Linux | UPower over D-Bus (`DeviceAdded`, `PropertiesChanged`), which aggregates BlueZ `Battery1` and kernel power supplies | also covers many wireless mice with USB receivers (for example Logitech) |
| Laptop battery | `GetSystemPowerStatus` + power notifications / IOKit power sources / UPower `DisplayDevice` | charger plug and unplug events |

Islet can only show a level the device reports to the OS. If your OS's own settings show a percentage, Islet can too.

## Music: what each OS gives today

| OS | Apps | Album art | Notes |
|---|---|---|---|
| Linux | any MPRIS player (Spotify, browsers, VLC, mpv, Rhythmbox…) | from local `file://` art only | web art (e.g. Spotify's `https://` covers) is not downloaded; an opt-in setting comes with Settings |
| Windows | any app with a system media session (Spotify, browsers, Apple Music, Media Player…) | yes (the session's thumbnail) | |
| macOS | any app (Safari, Chrome, Spotify, Music, Podcasts…) via Islet's MediaRemote helper in `/usr/bin/perl` | yes | if the helper can't run, falls back to Spotify and Apple Music only (no art; controls ask once for Automation permission). `islet --mediaremote-probe` checks a Mac |

## Trying it without a media player

`ISLET_DEMO=media` runs a scripted session (music, earbuds connecting, a low battery, a mouse) through the real pipeline. `ISLET_DEBUG=1` writes what each reader sees to `islet-debug.log` in your temp folder, which helps when testing on real hardware.

## Alarms: how they work

- Alarms live in `alarms.json` in Islet's config folder. Add, list, switch and delete them in Settings → Alarms or with `islet alarm …`.
- With no alarm switched on, the island never wakes up for them. With one on, it checks the clock once a minute. That costs nothing measurable and keeps alarms on time after the computer wakes from sleep. An alarm missed by more than 10 minutes (the laptop was asleep) is skipped rather than going off late.
- A ringing alarm stops by itself after 10 minutes. A one-time alarm switches itself off after it rings.
- The chime is generated by Islet (two short notes, no bundled or system sound files) and played with `afplay` on macOS, `pw-play`/`paplay`/`aplay` on Linux and PlaySound on Windows.
- Alarms ring only while Islet is running and the computer is awake. Waking a sleeping computer needs OS scheduling (Phase 5).

## Wi-Fi & Bluetooth: what each OS allows

| | Wi-Fi | Bluetooth |
|---|---|---|
| Linux | NetworkManager: list, join saved networks, join new ones (password in Settings) | BlueZ: connect and disconnect paired devices |
| Windows | WiFiAdapter: list and join. Windows 11 asks for **Location** permission before apps can list networks | Paired devices are listed. Windows gives apps no public way to connect them, so Islet opens Bluetooth settings |
| macOS | CoreWLAN: list and join. Since macOS 14, network names need **Location** permission (Islet asks once) | IOBluetooth: connect and disconnect paired devices |

- Lists load only when you open them (no background scanning), so idle wakeups stay at zero.
- The island never takes keyboard focus, so passwords for new networks are typed in Settings. The island's "Enter the password in Settings" button opens that page with the network chosen.
- Pairing new devices stays in the OS's own Bluetooth settings.

## Pop-ups: what each OS reports

| | Volume | Brightness | Caps Lock |
|---|---|---|---|
| Linux | PulseAudio / PipeWire (`pactl subscribe`) | GNOME (settings-daemon) and KDE Plasma signals | XKB lock-state events (X11 / XWayland) |
| Windows | Core Audio change callback | WMI `WmiMonitorBrightnessEvent` (built-in displays) | A keyboard hook that checks only the Caps Lock key, installed only while this pop-up is on |
| macOS | CoreAudio property listener | Not available (no public API) | Modifier-flag changes (may need Accessibility permission) |

All of them are event-driven: nothing polls, and each pop-up can be switched off in Settings → General (or `islet set volume-hud off`).
