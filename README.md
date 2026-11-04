# Islet: a Dynamic Island-style overlay for everyone

A Dynamic Island-style overlay for macOS, Windows and Linux. Music controls, navigation ETA, notifications and more in a smooth pill at the top of your screen. Lightweight enough for low-end laptops.

> **Status: early development (Phase 0).** Nothing to download yet.

- Plan and architecture: [`docs/`](docs/)
- Performance budgets and measurements: [`docs/perf.md`](docs/perf.md)

## Build from source

Requirements: Rust (stable), Node 22, and the [Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/) for your OS (on Linux, also `libgtk-layer-shell-dev`).

```bash
npm ci
npm run tauri dev
```

## Legal

Islet is not affiliated with, endorsed by or sponsored by Apple Inc. "Dynamic Island" is a trademark of Apple Inc. All other product names are trademarks of their respective owners.

Licensed under the [MIT License](LICENSE).
