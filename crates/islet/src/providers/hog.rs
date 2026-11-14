//! The performance alert's eyes: every 20 seconds while the alert is on,
//! list the running programs and report one that hogs the processor or the
//! memory (see `islet_core::hog`). Off by default; nothing runs until then.

use super::{debug_log, Sink};
use islet_core::activity::Report;
use islet_core::hog::{Proc, Watch};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

const EVERY: Duration = Duration::from_secs(20);

/// Send `true` to start watching, `false` to stop.
pub fn start(sink: Sink) -> Sender<bool> {
    let (tx, rx) = channel::<bool>();
    let _ = std::thread::Builder::new().name("islet-hog".into()).spawn(move || {
        let me = std::process::id();
        let cores = std::thread::available_parallelism().map_or(1, |n| n.get() as u32);
        while let Ok(on) = rx.recv() {
            if !on {
                continue;
            }
            let (mut watch, mut shown, began) = (Watch::default(), None, Instant::now());
            loop {
                let procs = processes();
                if procs.is_empty() {
                    debug_log("performance alert: can't list programs here");
                }
                let hog = watch.update(&procs, began.elapsed().as_secs_f64(), cores, memory_total(), |p| {
                    p.pid == me || p.pid <= 4 || is_system(&p.name)
                });
                // Say so when a hog appears, changes or goes.
                let key = hog.as_ref().map(|h| (h.pid, (h.share() * 20.0) as u32));
                if key != shown {
                    shown = key;
                    sink.send(Report::Hog(hog));
                }
                match rx.recv_timeout(EVERY) {
                    Ok(true) | Err(RecvTimeoutError::Timeout) => {}
                    Ok(false) => {
                        if shown.is_some() {
                            sink.send(Report::Hog(None));
                        }
                        break;
                    }
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            }
        }
    });
    tx
}

/// The system's own programs: closing them would do harm, not good.
fn is_system(name: &str) -> bool {
    let n = islet_core::hog::display_name(name).to_lowercase();
    const SYSTEM: [&str; 16] = [
        "system",
        "system idle process",
        "idle",
        "registry",
        "memory compression",
        "kernel_task",
        "launchd",
        "windowserver",
        "systemd",
        "init",
        "dwm",
        "csrss",
        "wininit",
        "services",
        "lsass",
        "smss",
    ];
    let raw = name.to_lowercase();
    SYSTEM.contains(&n.as_str()) || raw.starts_with("kworker") || raw.starts_with("ksoftirqd")
}

fn memory_total() -> u64 {
    super::sysmon::memory_total()
}

/// Ask a program to close: the polite way first (it can save its work).
pub fn close(pid: u32) {
    #[cfg(unix)]
    {
        // SAFETY: kill() with a pid and a valid signal number.
        let ok = i32::try_from(pid).is_ok_and(|p| p > 1 && unsafe { libc::kill(p, libc::SIGTERM) } == 0);
        if !ok {
            debug_log(&format!("could not close program {pid}"));
        }
    }
    #[cfg(windows)]
    std::thread::spawn(move || {
        use std::process::Stdio;
        // taskkill without /F sends the window a close request.
        let status = crate::hidden::command("taskkill")
            .args(["/PID", &pid.to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        if !status.is_ok_and(|s| s.success()) {
            debug_log(&format!("could not close program {pid}"));
        }
    });
}

// ---- Linux -----------------------------------------------------------------

#[cfg(target_os = "linux")]
pub fn processes() -> Vec<Proc> {
    // SAFETY: sysconf() with valid names has no preconditions.
    let (ticks, page) = unsafe { (libc::sysconf(libc::_SC_CLK_TCK), libc::sysconf(libc::_SC_PAGESIZE)) };
    let (ticks, page) = (ticks.max(1) as f64, page.max(1) as u64);
    let Ok(dir) = std::fs::read_dir("/proc") else { return Vec::new() };
    dir.filter_map(|e| {
        let pid: u32 = e.ok()?.file_name().to_str()?.parse().ok()?;
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        let (name, cpu_ticks, pages) = islet_core::hog::parse_proc_pid_stat(&stat)?;
        // Kernel threads have no memory of their own.
        (pages > 0).then(|| Proc {
            pid,
            name,
            cpu_secs: Some(cpu_ticks as f64 / ticks),
            cpu_pct: None,
            rss: pages * page,
        })
    })
    .collect()
}

// ---- macOS -----------------------------------------------------------------

#[cfg(target_os = "macos")]
pub fn processes() -> Vec<Proc> {
    use std::process::Stdio;
    let out = crate::hidden::command("/bin/ps")
        .args(["-Ao", "pid=,pcpu=,rss=,comm="])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output();
    let Ok(out) = out else { return Vec::new() };
    String::from_utf8_lossy(&out.stdout).lines().filter_map(islet_core::hog::parse_ps_line).collect()
}

// ---- Windows ---------------------------------------------------------------

#[cfg(target_os = "windows")]
pub fn processes() -> Vec<Proc> {
    use windows::Win32::Foundation::{CloseHandle, FILETIME};
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows::Win32::System::Threading::{GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    let mut out = Vec::new();
    // SAFETY: a snapshot handle we own and close; every struct passed is a
    // local of the size the call is told about.
    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return out };
        let mut entry = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut more = Process32FirstW(snapshot, &mut entry).is_ok();
        while more {
            let pid = entry.th32ProcessID;
            let len = entry.szExeFile.iter().position(|c| *c == 0).unwrap_or(entry.szExeFile.len());
            let name = String::from_utf16_lossy(&entry.szExeFile[..len]);
            // Programs we may not look into (other users', protected ones) are skipped.
            if let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
                let (mut created, mut exited, mut kernel, mut user) =
                    (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
                let timed = GetProcessTimes(process, &mut created, &mut exited, &mut kernel, &mut user).is_ok();
                let mut mem = PROCESS_MEMORY_COUNTERS::default();
                let size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
                let sized = K32GetProcessMemoryInfo(process, &mut mem, size).as_bool();
                let _ = CloseHandle(process);
                if timed && sized {
                    let hundred_ns = |f: FILETIME| (u64::from(f.dwHighDateTime) << 32) | u64::from(f.dwLowDateTime);
                    out.push(Proc {
                        pid,
                        name,
                        cpu_secs: Some((hundred_ns(kernel) + hundred_ns(user)) as f64 / 1e7),
                        cpu_pct: None,
                        rss: mem.WorkingSetSize as u64,
                    });
                }
            }
            more = Process32NextW(snapshot, &mut entry).is_ok();
        }
        let _ = CloseHandle(snapshot);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs on every system in CI: the list must hold this very test.
    #[test]
    fn lists_running_programs_including_this_one() {
        let procs = processes();
        let me = procs.iter().find(|p| p.pid == std::process::id()).expect("this process is listed");
        assert!(me.rss > 1 << 20, "it uses more than a megabyte: {me:?}");
        assert!(me.cpu_secs.is_some() || me.cpu_pct.is_some());
        assert!(!me.name.is_empty());
        assert!(memory_total() > 0);
    }

    #[test]
    fn the_systems_own_programs_are_left_alone() {
        for name in ["System", "kernel_task", "C:\\Windows\\System32\\dwm.exe", "kworker/0:1", "systemd"] {
            assert!(is_system(name), "{name}");
        }
        for name in ["firefox", "Code.exe", "/Applications/Safari.app/Contents/MacOS/Safari"] {
            assert!(!is_system(name), "{name}");
        }
    }
}
