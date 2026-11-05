# Contributing to Islet

Thanks for helping. Islet aims to feel like the iPhone's Dynamic Island while staying tiny: a few MB of RAM, zero CPU and zero wakeups when idle. Every change is judged against that.

## Ground rules

- **Budgets are tests.** CI fails if idle RAM (60 MB), idle CPU (0.5 %), active CPU (5 %), cold start (1.5 s) or installer size (20 MB) go over. See [`docs/perf.md`](docs/perf.md).
- **No polling.** Block on OS events, channels or a deadline. A new timer needs a reason and must stop when nothing is on screen.
- **Original work only.** Don't copy code from other projects, especially GPL ones (NotchNook clones, Boring Notch, NexNotch…). Ideas are fine; code isn't.
- **No Apple assets.** Don't bundle or imitate San Francisco font files, SF Symbols, Apple icons or sounds. Islet's icons are drawn in code (`islet-render/src/elements.rs`) and its alarm chime is synthesised (`crates/islet/src/sound.rs`).
- **Privacy.** No telemetry. Network calls only for features the user turned on. Secrets live only in the OS keychain (`crates/islet/src/secrets.rs`). Never commit keys, tokens or certificates.
- **Terms of service.** Use only documented APIs (Google Routes, Maps URLs, Waze deep links). No scraping, no private endpoints.

## Getting started

```bash
cargo test --workspace --exclude islet-settings   # core, renderer, island, perf harness
cargo run -p islet                                # the island
ISLET_DEMO=media cargo run -p islet               # scripted music + battery demo
```

The Settings app additionally needs Node 22 and the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/):

```bash
npm ci && npm run tauri dev
```

Before you push: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `npm run check && npm run lint && npm test`.

## Layout

| Path | What it is |
|---|---|
| `crates/islet-core` | Platform-free logic: springs, island state machine, activities, alarms, navigation, Wi-Fi/Bluetooth model. Unit-tested. |
| `crates/islet-render` | tiny-skia renderer: squircles, metaball split, text, icons. Golden-image tests. |
| `crates/islet` | The island: event loop, one window backend per OS (`platform/`), OS readers (`providers/`), CLI. |
| `crates/islet-mediaremote` | macOS Now Playing helper loaded into `/usr/bin/perl`. |
| `src-tauri`, `ui` | Islet Settings (Tauri + Svelte), a thin UI over allow-listed `islet` commands. |
| `perf` | `islet-perf`: launches the app and enforces the budgets. |

More in [`docs/architecture.md`](docs/architecture.md).

## Adding an activity

An activity is anything that appears on the island (music, a battery pop-up, a trip, a ringing alarm). To add one:

1. **Model it in `islet-core`.** Add the data type and a `Report` variant in `activity.rs` (what a reader observed), store it in `Activities`, and decide its place in `Activities::live()`: the order is the priority. Add unit tests for when it appears and goes away. If it expires by itself, return the moment from `next_change()` so the island wakes exactly then and never polls.
2. **Read it from the OS** in `crates/islet/src/providers/`. Run on its own thread, block on OS events, and send `Report`s through the `Sink` (which wakes the loop). Give every OS a module, even if one only reports "not available". Linux readers get integration tests against fake D-Bus services (`linux_tests.rs`).
3. **Draw it** in `crates/islet/src/content.rs`: a compact view (`compact`), the split-pill views (`leading` and the bubble in `minimal`), and an expanded card. Add its height in `App::target_dims` and any buttons in `App::handle`, with hit areas defined next to the drawing code.
4. **Animate only while visible.** If it moves on its own (a waveform, a swinging bell), return a frame interval from `App::frame_interval` only while it is on screen.
5. **Expose it** if users configure it: an `islet …` command in `cli.rs`, then a page in Settings that calls that command (add it to `allowed()` in `src-tauri/src/lib.rs`).
6. **Check it**: unit tests, a render in the contact sheet (`ISLET_CONTACT_SHEET=…`), and the perf harness (`cargo run -p islet-perf -- run --app target/release/islet`).

## Pull requests

- One topic per PR, with tests. Conventional commit titles (`feat(alarms): …`, `fix(macos): …`).
- Say which OSes you tested on. Windows and macOS code that only compiles in CI should be labelled as such.
- UI changes: include a screenshot or contact-sheet crop.

By contributing you agree that your work is licensed under the [MIT License](LICENSE).
