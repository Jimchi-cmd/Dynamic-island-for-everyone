# Performance

Islet has hard budgets because it targets low-end laptops (4 GB RAM, 2 cores, integrated GPU).

The budgets are for the free app, which is what CI measures. Islet Pro turns the low-end limits off (see [pro.md](pro.md)), so it may use more memory and more CPU while music plays. Its Power saver switch brings the limits back.

| Metric | Budget |
|---|---|
| Idle RAM (collapsed), all processes | < 60 MB |
| Idle CPU (collapsed) | < 0.5 % |
| CPU while animating/expanded | < 5 % |
| Installer size (msi, exe, dmg, deb, rpm) | < 20 MB (AppImage exempt: it bundles WebKitGTK) |
| Cold start to visible | < 1.5 s |

## The harness: `islet-perf`

```bash
npx tauri build --no-bundle
cargo build --release -p islet-perf
# Linux headless:
xvfb-run -a dbus-run-session -- target/release/islet-perf run --app target/release/islet --enforce
# Installer sizes:
target/release/islet-perf size target/release/bundle/deb/*.deb --enforce
```

The harness makes two runs:
1. **idle**: launch, wait for the `ISLET_VISIBLE` marker (cold start), wait for warm-up, then sample for `--idle-secs`.
2. **active**: the same with `ISLET_PERF_DEMO=1`, which makes the UI expand and collapse every 1.2 s. This phase runs three times and the median is judged against the budget, because shared CI runners are noisy (one macOS build measured 3.4 % and 7.5 % on back-to-back runs). All three values are printed.

How it measures:
- **RAM** is summed over the app and every helper process: WebKitGTK `WebKitWebProcess`/`WebKitNetworkProcess`, WebView2 `msedgewebview2.exe`, and the macOS `com.apple.WebKit.*` XPC services that appear after launch. The counter per OS:
  - Linux: **PSS**, where shared pages are split between the processes that map them.
  - Windows: **private bytes**.
  - macOS: **phys_footprint**, the "Memory" column in Activity Monitor.
- **CPU %** is CPU time divided by wall time, expressed as a share of a **2-core reference machine** (the Task Manager convention). CI runners are faster than the target laptops, so CI numbers are optimistic.
- **Wakeups**: context switches per second on Linux, reported for information only.

## Phase 0 baseline (bare Tauri 2 pill)

Measured locally on Linux (Xvfb with software GL, nothing else running):

| Process | PSS | Anonymous |
|---|---|---|
| `islet` (GTK + Rust core) | 63–84 MB | 21–36 MB |
| `WebKitWebProcess` | 85 MB (167 MB with GL compositing under llvmpipe) | 22 MB (86 MB) |
| `WebKitNetworkProcess` | 20 MB | 7 MB |
| **Total** | **169–256 MB** | **~52–130 MB** |

Cold start was 450 ms (passes). Idle CPU was 0.03–0.08 % (passes). Active CPU was 4.5–7.7 % (borderline).

How to read this: file-backed pages (the shared WebKitGTK, GTK and Mesa libraries) make up most of the PSS. On a real desktop, other GTK apps share some of those pages, so the number drops. The anonymous memory alone is already about 52 MB, though, so **WebKitGTK cannot meet a 60 MB total on Linux**. Disabling GL compositing saves about 80 MB under software rendering.

Windows (WebView2) and macOS (WKWebView) numbers come from the CI `build` job summary.

## Gate decision: native island (no webview)

The same pill drawn natively: Rust, tiny-skia, and a thin window layer per OS (see [`architecture.md`](architecture.md)). Measured with the same harness, same machine and same settings:

| Metric | Tauri webview pill | Native island | Budget |
|---|---|---|---|
| Idle RAM, all processes | 169–256 MB (3 processes) | **1.7 MB** (1 process) | < 60 MB |
| Idle CPU | 0.03–0.08 % | **0.00 %** | < 0.5 % |
| Idle wakeups (context switches/s) | 2.3–39 | **0** | zero when idle |
| Active CPU (expand/collapse every 1.2 s) | 4.5–7.7 % | **1.5 %** | < 5 % |
| Cold start to visible | 450–1176 ms | **21 ms** | < 1.5 s |
| Binary size | 3.5 MB (+ system webview) | **0.85 MB** | < 20 MB installer |

The island process therefore leaves more than 55 MB of the budget for features (album art, text, media sessions). The webview is kept only for the **Settings** window, which runs as a separate process started on demand and exits when closed, so it never counts towards idle RAM.

### Confirmed on all three OSes (GitHub Actions, run 41843053746)

| | Native island: RAM / idle CPU / active CPU / cold start | Tauri webview pill: RAM / cold start |
|---|---|---|
| **Linux** (ubuntu-latest, Xvfb) | **1.6 MB** / 0.00 % / 0.77 % / **20 ms** | 282 MB / 2.5 s |
| **Windows** (windows-latest) | **1.9 MB** / 0.00 % / 1.25 % / **41 ms** | 100 MB / 6.5 s |
| **macOS** (macos-latest, Apple silicon) | **≤ 17 MB*** / 0.01 % / 0.99 % / **191 ms** | 58 MB / 2.2 s |

\* That macOS run also counted unrelated WebKit processes started on the runner during the test (6 processes for a single-process app). The harness now attributes them by macOS's "responsible process", so the next run reports the island alone.

Installers stay far below the 20 MB budget: msi 1.6 MB, setup.exe 1.1 MB, dmg 1.4 MB, deb/rpm 1.4 MB, and the island binary is 0.5–0.8 MB.

## Throttled low-end profile (manual)

To approximate the target hardware, run the harness in a VM:

1. Create a VM with **2 vCPUs, 4 GB RAM** and no GPU passthrough. On a fast host, also cap CPU: VirtualBox *Execution Cap* 50 %, or QEMU `-smp 2 -m 4096` with `cpulimit`/cgroup `cpu.max = "100000 100000"`.
2. Install the release build from CI artifacts.
3. Run `islet-perf run --app <path> --idle-secs 60 --active-secs 30`.
4. Paste the markdown output into the PR or issue.

Do this for Windows 10/11, Ubuntu (GNOME, X11 and Wayland) and, where possible, an Intel Mac.
