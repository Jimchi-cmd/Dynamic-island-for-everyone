# Islet: a Dynamic Island-style overlay for everyone

A Dynamic Island-style overlay for macOS, Windows and Linux. Music controls, navigation ETA, notifications and more in a smooth pill at the top of your screen. Lightweight enough for low-end laptops.

> **Status: early development (Phase 0).** Nothing to download yet.

The island is a tiny native program with no web browser inside. On Linux it currently idles at **under 2 MB of RAM and 0 % CPU**, so it runs fine on 4 GB laptops.

- Architecture: [`docs/architecture.md`](docs/architecture.md)
- Performance budgets and measurements: [`docs/perf.md`](docs/perf.md)

## Build from source

Requirements: Rust (stable).

```bash
cargo run --release -p islet          # the island
```

The Settings app additionally needs Node 22 and the [Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/) for your OS:

```bash
npm ci
npm run tauri dev
```

## Legal

Islet is not affiliated with, endorsed by or sponsored by Apple Inc. "Dynamic Island" is a trademark of Apple Inc. All other product names are trademarks of their respective owners.

Licensed under the [MIT License](LICENSE).
