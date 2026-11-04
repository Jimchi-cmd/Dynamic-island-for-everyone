//! islet-perf: launch Islet, measure it, compare with the budgets.
//!
//! Usage:
//!   islet-perf run --app <path> [--idle-secs 60] [--active-secs 20] [--warmup-secs 5]
//!                  [--json out.json] [--markdown out.md] [--enforce]
//!   islet-perf size <installer>... [--markdown out.md] [--enforce]
//!
//! RAM is summed over the app and every helper process its webview starts.

mod budget;
mod mem;

use budget::{cpu_pct, markdown, Budgets, Check};
use serde::Serialize;
use std::collections::HashSet;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

#[derive(Default)]
struct Args {
    mode: String,
    app: Option<String>,
    files: Vec<String>,
    idle_secs: u64,
    active_secs: u64,
    warmup_secs: u64,
    json: Option<String>,
    markdown: Option<String>,
    enforce: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut it = std::env::args().skip(1);
    let mut a = Args { idle_secs: 60, active_secs: 20, warmup_secs: 5, ..Default::default() };
    a.mode = it.next().ok_or("expected a mode: run | size")?;
    while let Some(arg) = it.next() {
        let mut val = |name: &str| it.next().ok_or(format!("{name} needs a value"));
        match arg.as_str() {
            "--app" => a.app = Some(val("--app")?),
            "--idle-secs" => a.idle_secs = val("--idle-secs")?.parse().map_err(|e| format!("{e}"))?,
            "--active-secs" => a.active_secs = val("--active-secs")?.parse().map_err(|e| format!("{e}"))?,
            "--warmup-secs" => a.warmup_secs = val("--warmup-secs")?.parse().map_err(|e| format!("{e}"))?,
            "--json" => a.json = Some(val("--json")?),
            "--markdown" => a.markdown = Some(val("--markdown")?),
            "--enforce" => a.enforce = true,
            s if s.starts_with("--") => return Err(format!("unknown flag {s}")),
            file => a.files.push(file.to_string()),
        }
    }
    Ok(a)
}

/// Processes that belong to the app: the root, its descendants and, on
/// macOS, WebKit XPC services (launched by launchd, not by us) that did not
/// exist before the app started.
fn app_tree(sys: &System, root: Pid, baseline: &HashSet<Pid>) -> Vec<Pid> {
    let mut out = Vec::new();
    for (pid, p) in sys.processes() {
        // On Linux sysinfo also lists threads; count each process once.
        if p.thread_kind().is_some() {
            continue;
        }
        let mut cur = Some(*pid);
        let mut is_desc = false;
        let mut hops = 0;
        while let Some(c) = cur {
            if c == root {
                is_desc = true;
                break;
            }
            hops += 1;
            if hops > 64 {
                break;
            }
            cur = sys.process(c).and_then(|q| q.parent());
        }
        let webkit_xpc = cfg!(target_os = "macos")
            && !baseline.contains(pid)
            && p.name().to_string_lossy().contains("com.apple.WebKit")
            // Launched by launchd on behalf of some app: count it only if
            // macOS says our app is the responsible process (when known).
            && mem::responsible_pid(pid.as_u32()).is_none_or(|r| r == root.as_u32());
        if is_desc || webkit_xpc {
            out.push(*pid);
        }
    }
    out
}

struct Snapshot {
    ram_bytes: u64,
    cpu_ms: u64,
    ctx_switches: Option<u64>,
    procs: usize,
}

fn snapshot(sys: &mut System, root: Pid, baseline: &HashSet<Pid>) -> Snapshot {
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_cpu().with_memory().with_exe(UpdateKind::OnlyIfNotSet),
    );
    let pids = app_tree(sys, root, baseline);
    let mut s = Snapshot { ram_bytes: 0, cpu_ms: 0, ctx_switches: Some(0), procs: pids.len() };
    for pid in pids {
        if let Some(p) = sys.process(pid) {
            s.ram_bytes += mem::attributed_bytes(pid, p);
            s.cpu_ms += p.accumulated_cpu_time();
            s.ctx_switches = match (s.ctx_switches, mem::context_switches(pid.as_u32())) {
                (Some(a), Some(b)) => Some(a + b),
                _ => None,
            };
        }
    }
    s
}

#[derive(Serialize)]
struct Phase {
    name: &'static str,
    cold_start_ms: f64,
    ram_mb_mean: f64,
    ram_mb_max: f64,
    cpu_pct: f64,
    wakeups_per_sec: Option<f64>,
    processes: usize,
}

fn kill_tree(sys: &mut System, child: &mut Child, baseline: &HashSet<Pid>) {
    let root = Pid::from_u32(child.id());
    sys.refresh_processes(ProcessesToUpdate::All, true);
    let pids = app_tree(sys, root, baseline);
    let _ = child.kill();
    for pid in pids {
        if let Some(p) = sys.process(pid) {
            p.kill();
        }
    }
    let _ = child.wait();
}

fn measure(app: &str, name: &'static str, demo: bool, warmup: u64, secs: u64) -> Result<Phase, String> {
    let mut sys = System::new().map_err(|e| format!("sysinfo: {e}"))?;
    sys.refresh_processes(ProcessesToUpdate::All, true);
    let baseline: HashSet<Pid> = sys.processes().keys().copied().collect();

    let mut cmd = Command::new(app);
    cmd.stdout(Stdio::piped()).stderr(Stdio::inherit());
    if demo {
        cmd.env("ISLET_PERF_DEMO", "1");
    }
    let spawned = Instant::now();
    let mut child = cmd.spawn().map_err(|e| format!("failed to launch {app}: {e}"))?;
    let root = Pid::from_u32(child.id());

    let stdout = child.stdout.take().ok_or("no stdout")?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if line.starts_with("ISLET_VISIBLE") {
                let _ = tx.send(());
            }
        }
    });
    let cold_start_ms = match rx.recv_timeout(Duration::from_secs(30)) {
        Ok(()) => spawned.elapsed().as_secs_f64() * 1000.0,
        Err(_) => {
            kill_tree(&mut sys, &mut child, &baseline);
            return Err("app never reported ISLET_VISIBLE within 30 s".into());
        }
    };

    std::thread::sleep(Duration::from_secs(warmup));
    let start = snapshot(&mut sys, root, &baseline);
    let t0 = Instant::now();
    let mut ram: Vec<u64> = vec![start.ram_bytes];
    let mut procs = start.procs;
    let step = Duration::from_secs(5.min(secs.max(1)));
    let mut last = snapshot(&mut sys, root, &baseline);
    while t0.elapsed() < Duration::from_secs(secs) {
        std::thread::sleep(step);
        last = snapshot(&mut sys, root, &baseline);
        ram.push(last.ram_bytes);
        procs = procs.max(last.procs);
    }
    let wall_ms = t0.elapsed().as_millis() as u64;
    kill_tree(&mut sys, &mut child, &baseline);

    let mb = |b: u64| b as f64 / (1024.0 * 1024.0);
    let wakeups_per_sec = match (start.ctx_switches, last.ctx_switches) {
        (Some(a), Some(b)) if wall_ms > 0 => Some(b.saturating_sub(a) as f64 / (wall_ms as f64 / 1000.0)),
        _ => None,
    };
    Ok(Phase {
        name,
        cold_start_ms,
        ram_mb_mean: mb(ram.iter().sum::<u64>() / ram.len() as u64),
        ram_mb_max: mb(ram.iter().copied().max().unwrap_or(0)),
        cpu_pct: cpu_pct(last.cpu_ms.saturating_sub(start.cpu_ms), wall_ms),
        wakeups_per_sec,
        processes: procs,
    })
}

#[derive(Serialize)]
struct Report {
    os: &'static str,
    budgets: Budgets,
    phases: Vec<Phase>,
    checks: Vec<Check>,
}

fn write_outputs(
    a: &Args,
    title: &str,
    checks: &[Check],
    notes: &[String],
    json: Option<String>,
) -> Result<(), String> {
    let md = markdown(title, checks, notes);
    print!("{md}");
    if let Some(path) = &a.markdown {
        std::fs::write(path, &md).map_err(|e| format!("{path}: {e}"))?;
    }
    if let (Some(path), Some(j)) = (&a.json, json) {
        std::fs::write(path, j).map_err(|e| format!("{path}: {e}"))?;
    }
    Ok(())
}

fn run_mode(a: &Args) -> Result<bool, String> {
    let app = a.app.as_deref().ok_or("--app is required")?;
    let b = Budgets::default();
    let idle = measure(app, "idle", false, a.warmup_secs, a.idle_secs)?;
    let active = measure(app, "active (expand/collapse loop)", true, a.warmup_secs, a.active_secs)?;
    let checks = vec![
        Check::new("Cold start to visible", idle.cold_start_ms, b.cold_start_ms, "ms"),
        Check::new("Idle RAM (mean, all processes)", idle.ram_mb_mean, b.idle_ram_mb, "MB"),
        Check::new("Idle CPU", idle.cpu_pct, b.idle_cpu_pct, "%"),
        Check::new("Active CPU", active.cpu_pct, b.active_cpu_pct, "%"),
    ];
    let mut notes = vec![
        format!(
            "OS: {}; idle {} s, active {} s after {} s warm-up",
            std::env::consts::OS,
            a.idle_secs,
            a.active_secs,
            a.warmup_secs
        ),
        format!("Processes counted: idle {}, active {}", idle.processes, active.processes),
        format!("Idle RAM max {:.1} MB; active RAM mean {:.1} MB", idle.ram_mb_max, active.ram_mb_mean),
        "CPU % is a share of a 2-core reference machine; CI runners are faster than the target laptops.".into(),
    ];
    if let Some(w) = idle.wakeups_per_sec {
        notes.push(format!("Idle context switches: {w:.1}/s (Linux only; informational)"));
    }
    let pass = checks.iter().all(|c| c.pass);
    let report = Report { os: std::env::consts::OS, budgets: b, phases: vec![idle, active], checks: checks.clone() };
    let json = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
    write_outputs(a, &format!("Islet perf ({})", std::env::consts::OS), &checks, &notes, Some(json))?;
    Ok(pass)
}

fn size_mode(a: &Args) -> Result<bool, String> {
    if a.files.is_empty() {
        return Err("size: pass one or more installer files".into());
    }
    let b = Budgets::default();
    let mut checks = Vec::new();
    let mut notes = Vec::new();
    for f in &a.files {
        let len = std::fs::metadata(f).map_err(|e| format!("{f}: {e}"))?.len();
        let name = std::path::Path::new(f).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let mb = len as f64 / (1024.0 * 1024.0);
        if name.ends_with(".AppImage") {
            // Bundles WebKitGTK; documented exception to the install budget.
            notes.push(format!("{name}: {mb:.1} MB (AppImage bundles WebKitGTK; exempt)"));
            continue;
        }
        let metric: &'static str = Box::leak(name.into_boxed_str());
        checks.push(Check::new(metric, mb, b.installer_mb, "MB"));
    }
    let pass = checks.iter().all(|c| c.pass);
    write_outputs(a, "Islet installer sizes", &checks, &notes, None)?;
    Ok(pass)
}

fn main() {
    let a = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("islet-perf: {e}");
            std::process::exit(2);
        }
    };
    let result = match a.mode.as_str() {
        "run" => run_mode(&a),
        "size" => size_mode(&a),
        m => Err(format!("unknown mode {m}")),
    };
    match result {
        Ok(true) => {}
        Ok(false) if a.enforce => {
            eprintln!("islet-perf: budget exceeded");
            std::process::exit(1);
        }
        Ok(false) => eprintln!("islet-perf: budget exceeded (not enforced)"),
        Err(e) => {
            eprintln!("islet-perf: {e}");
            std::process::exit(2);
        }
    }
}
