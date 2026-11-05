# Changelog

All notable changes are listed here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow [Semantic Versioning](https://semver.org/).

## [0.1.0] - unreleased

The first public release.

### Island
- Native, webview-free island for macOS, Windows and Linux (X11, and XWayland on Wayland): about 5 MB of RAM, 0 % CPU and zero wakeups when idle.
- iPhone-style shapes and motion: continuous-corner squircles, spring animations, peek, compact, the split pill with a metaball neck for two activities, the expanded card and a large view. Wraps the notch on MacBooks.
- Inter bundled for text. San Francisco is used on macOS, from the system.

### Activities
- **Now Playing** for any app: artwork, title, artist, a live waveform, progress with scrubbing, and controls. Linux uses MPRIS, Windows the system media session, and macOS a MediaRemote helper with a Spotify/Music fallback.
- **Battery pop-ups** when headphones, earbuds, mice, keyboards or controllers connect, plus low-battery and charger alerts.
- **Navigation ETA** with live traffic via the Google Routes API (your own key, kept in the OS keychain), adaptive refresh, and hand-off to Google Maps or Waze.
- **Alarms**: one-time or repeating. When one rings, the island opens with a swinging bell and plays an original chime until you snooze or stop it.
- **Wi-Fi and Bluetooth** pickers from the clock card: join networks and connect paired devices.
- **Volume, brightness and caps-lock pop-ups** on the island (each can be switched off).
- **Timer and focus sessions:** a countdown ring on the island, Pomodoro work/break rounds with a chime at each change, and Repeat when it ends.

### Apps and tools
- Islet Settings: a first-run welcome with permissions, start at login, Wi-Fi & Bluetooth, alarms, navigation key and trip, updates (opt-in checks) and about.
- `islet` command line: `nav`, `key`, `alarm`, `wifi`, `bt`, `autostart`, `status`, `update check`, `quit`.
- Installers: .deb, .rpm and AppImage for Linux; .msi, setup .exe and a portable zip for Windows; a universal .dmg for macOS. SHA-256 checksums, and signed in-app updates once a signing key is configured.
