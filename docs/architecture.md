# Architecture

## Decision: a native island, with webviews only for Settings

**Context.** The brief sets hard budgets for low-end laptops: under 60 MB idle RAM across all processes, under 0.5 % idle CPU with zero wakeups, and a 1.5 s cold start. Phase 0 measured a bare Tauri 2 pill against them. On Linux, the WebKitGTK webview alone used 169–256 MB across three processes, and about 52 MB of that is non-shared memory that no tuning can remove. A webview cannot meet the budget on every platform, so it cannot deliver "for everyone".

**Decision.** The always-on island is a small native Rust process with no webview. It draws every frame on the CPU with [tiny-skia](https://github.com/RazrFalcon/tiny-skia) and shows the pixels through a thin window layer per OS. The Tauri/Svelte webview is still used for the parts that benefit from HTML forms and built-in accessibility: Settings, first-run permissions and onboarding. That app runs as a **separate process that starts on demand and exits when closed**.

**Consequences.**
- Measured on Linux, the island uses 1.7 MB RAM, 0 % idle CPU, 0 wakeups and a 21 ms cold start ([`perf.md`](perf.md)).
- We own the window code for each OS, about 300–400 lines per backend.
- Rendering is deterministic, so visual tests compare against golden PNGs on every OS in CI.
- The island has no free accessibility tree. It needs AccessKit (UI Automation on Windows, NSAccessibility on macOS, AT-SPI on Linux), planned for Phase 1/2.
- Text needs a shaping stack (planned: cosmic-text, loaded lazily so cold start stays fast).

## Layout

```
crates/
  islet-core/     platform-independent: geometry, springs, island state machine,
                  activities, settings (unit-tested everywhere)
  islet-render/   tiny-skia renderer: continuous-corner squircles, morphing,
                  metaball split, text and images (golden-image tests)
  islet/          the island binary: portable event loop + one backend per OS
    src/platform/
      x11.rs      X11 / XWayland (x11rb): ARGB visual, dock window, XShape input region
      windows.rs  Win32 layered window (UpdateLayeredWindow); alpha hit-testing
      macos.rs    non-activating NSPanel above the menu bar; CALayer contents
      (Phase 1)   wayland.rs: wlr-layer-shell via smithay-client-toolkit (KDE, sway, Hyprland)
src-tauri/        Settings / onboarding app (Tauri 2 + Svelte in ui/), started on demand
perf/             islet-perf: launches the app and checks it against the budgets
```

## The event loop

The loop in `crates/islet/src/main.rs` is portable. Each frame it:
1. computes a deadline: the next animation frame while a spring is moving, or the next timer, or **none** when idle;
2. blocks in the backend's `wait(deadline)` (`poll(2)` on the X socket, `MsgWaitForMultipleObjectsEx`, `nextEventMatchingMask:untilDate:`), so an idle island makes no wakeups at all;
3. feeds the events to `App`, draws a frame if anything changed, and presents it.

Animations are analytic damped springs (`islet-core::spring`), solved from the time since the animation started. That makes them frame-rate independent: a missed frame never slows an animation down, and an interrupted animation keeps its velocity.

## Platform backends

| | Linux X11 / XWayland | Windows | macOS |
|---|---|---|---|
| Window | ARGB visual, `_NET_WM_WINDOW_TYPE_DOCK`, above, sticky | `WS_EX_LAYERED \| TOPMOST \| TOOLWINDOW \| NOACTIVATE` popup | borderless non-activating `NSPanel`, `NSStatusWindowLevel` |
| Pixels | `PutImage` (premultiplied BGRA) | `UpdateLayeredWindow` from a 32-bit DIB | `CGImage` as the view layer's `contents` |
| Click-through | XShape input region | alpha hit-testing (built into layered windows) | Phase 1: `ignoresMouseEvents` + global mouse monitor |
| Focus | `WM_HINTS.input = false` | `WM_MOUSEACTIVATE → MA_NOACTIVATE` | non-activating panel |
| Idle wait | `poll(2)` | `MsgWaitForMultipleObjectsEx` | `nextEventMatchingMask:untilDate:` |

Wayland compositors with `wlr-layer-shell` (KDE Plasma, sway, Hyprland and others) get a native layer-shell backend in Phase 1. GNOME does not implement layer-shell, so on GNOME Islet runs through XWayland.

## Process model

- `islet`: the island, always running, under 60 MB.
- `islet-settings`: the Tauri webview app, started from the island's menu or the app launcher, and it exits when closed. It edits `settings.json`; the island watches that file and applies changes without a restart.
- Secrets (API keys, OAuth tokens) live only in the OS keychain (`crates/islet/src/secrets.rs`: the `keyring` crate's native stores on macOS and Windows, `secret-tool` on Linux, which avoids pulling an async D-Bus stack with background wakeups into the island).
- Command-line commands (`islet nav …`) reach the running island over a local control channel (`control.rs`): TCP on 127.0.0.1 only, a random port, and a random token in `control.json`, which only the user can read. Each request is one line, and a request without the token is refused.
- Providers are threads that block on OS events or channels and post `Report`s to the loop through the backend's waker. The navigation provider (`providers/navigation.rs`) sleeps in `recv_timeout` until its next refresh and has no deadline at all when no trip is active, so idle wakeups stay at zero.

## Alarms and Wi-Fi/Bluetooth

- **Alarms** (`islet-core::alarm`) run off the wall clock the loop already reads. With no alarm switched on there is no deadline at all. With one on, the loop wakes once a minute, which keeps alarms on time after sleep (the monotonic clock pauses during suspend). The chime plays on a separate thread only while ringing.
- **Wi-Fi and Bluetooth** (`providers/connectivity.rs`) run on a worker thread that blocks on a channel. The island sends it `ConnCommand`s only when the clock card or a list opens, or when the user taps an entry, and it answers with `ConnUpdate` reports. Each OS has a `Backend`: NetworkManager + BlueZ over D-Bus, WinRT `WiFiAdapter` plus device enumeration, or CoreWLAN + IOBluetooth. The same backends serve `islet wifi …` and `islet bt …`, which is how Settings joins networks with a password.
