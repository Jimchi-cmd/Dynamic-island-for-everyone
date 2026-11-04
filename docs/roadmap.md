# Roadmap

Each phase ships with tests and green CI on Windows, macOS and Linux before the next one starts.

| Phase | Scope | Status |
|---|---|---|
| **0** | Scaffold, perf harness, CI matrix, stack decision (native island) | ✅ done |
| **1** | Island shell: full state machine, squircle morph, metaball split, input (hover, click, hold, scroll, Esc), notch wrap on Macs, Wayland layer-shell, macOS click-through, text rendering (Inter / San Francisco), clock activity, AccessKit, golden-image tests | in progress |
| **2** | Activity framework and the **music controller** (Windows SMTC, Linux MPRIS, macOS media adapter): art, title, progress and scrubbing, controls, waveform. **Device battery**: connect pop-ups for headphones, mice, keyboards and controllers, low-battery alerts, laptop charging | next |
| **3** | Navigation ETA (Google Routes, bring your own key) and Google Maps / Waze hand-off; optional Spotify Web API | |
| **4** | Release pipeline and v0.1.0: installers for every OS, auto-update, Settings app, first-run permissions screen | |
| **5** | Notifications, volume/brightness/caps-lock HUD, calendar, file shelf, focus timer, weather | |
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
