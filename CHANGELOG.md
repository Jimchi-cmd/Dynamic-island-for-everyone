# Changelog

All notable changes are listed here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow [Semantic Versioning](https://semver.org/).

## [0.1.0] - unreleased

The first public release.

### Island
- Native, webview-free island for macOS, Windows and Linux (native Wayland on KDE Plasma, sway, Hyprland and other wlroots compositors; X11 and XWayland elsewhere): about 5 MB of RAM, 0 % CPU and zero wakeups when idle.
- iPhone-style shapes and motion: continuous-corner squircles, spring animations, peek, compact, the split pill with a metaball neck for two activities, the expanded card and a large view. Wraps the notch on MacBooks.
- Inter bundled for text. San Francisco is used on macOS, from the system.

### Activities
- **Now Playing** for any app: artwork, title, artist, a live waveform, progress with scrubbing, and controls. Linux uses MPRIS, Windows the system media session, and macOS a MediaRemote helper with a Spotify/Music fallback.
- **Battery pop-ups** when headphones, earbuds, mice, keyboards or controllers connect, plus low-battery alerts.
- **Charging notification:** plug in and the island shows a green bolt, "Charging", the level and a filling battery; unplugging shows "On battery", reaching 100 % shows "Fully charged", and earbuds or a mouse that start charging get their own pop-up.
- **Navigation ETA** with live traffic via the Google Routes API (your own key, kept in the OS keychain), adaptive refresh, and hand-off to Google Maps or Waze.
- **Alarms**: one-time or repeating. When one rings, the island opens with a swinging bell and plays an original chime until you snooze or stop it.
- **Wi-Fi and Bluetooth** pickers from the clock card: join networks and connect paired devices.
- **Spotify on your other devices** (optional): playback on your phone or a speaker shows on the island, with controls, using your own Spotify app.
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
