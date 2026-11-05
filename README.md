# Islet: a Dynamic Island-style overlay for everyone

A Dynamic Island-style overlay for macOS, Windows and Linux. Music controls, navigation ETA, notifications and more in a smooth pill at the top of your screen. Lightweight enough for low-end laptops.

> **Status: early development (Phase 0).** Nothing to download yet.

The island is a tiny native program with no web browser inside. It idles at **a few MB of RAM, 0 % CPU and zero wakeups**, so it runs fine on 4 GB laptops.

**Works today (early):** the island's shapes and motion (peek, expand, hold for the large view, the gooey split for two activities), a clock, **Now Playing** with album art, waveform, progress, scrubbing and controls, and **battery pop-ups** when headphones, earbuds, mice, keyboards or controllers connect, plus low-battery and charger alerts, **alarms** that open the island with a swinging bell and a chime until you snooze or stop them, and a **navigation ETA** with live traffic and one-tap hand-off to Google Maps or Waze, using your own Google API key ([how](docs/navigation.md)). See [`docs/roadmap.md`](docs/roadmap.md) for what each OS supports.

- Roadmap: [`docs/roadmap.md`](docs/roadmap.md)
- Design spec (look and motion): [`docs/design-spec.md`](docs/design-spec.md)
- Architecture: [`docs/architecture.md`](docs/architecture.md)
- Navigation ETA setup, costs and privacy: [`docs/navigation.md`](docs/navigation.md)
- Performance budgets and measurements: [`docs/perf.md`](docs/perf.md)

## Build from source

Requirements: Rust (stable).

```bash
cargo run --release -p islet                      # the island
ISLET_DEMO=media cargo run --release -p islet     # a scripted music + battery demo
```

Right-click the island to quit (a proper menu comes later). `islet --help` lists the commands:

```bash
islet key routes                                   # store your Google Routes API key in the OS keychain
islet nav start "Kampala Road" --from "Ntinda" --label Work
islet nav stop
islet alarm add 6:30am --days weekdays --label "Wake up"
islet alarm list
```

The Settings app additionally needs Node 22 and the [Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/) for your OS:

```bash
npm ci
npm run tauri dev
```

## Legal

Islet is not affiliated with, endorsed by or sponsored by Apple Inc. "Dynamic Island" is a trademark of Apple Inc. Google Maps is a trademark of Google LLC; Waze is a trademark of Waze Mobile Ltd. All other product names are trademarks of their respective owners.

Licensed under the [MIT License](LICENSE).
