//! Per-process memory that best matches what users see in their OS tools,
//! and avoids double-counting shared pages across the webview's processes.
//! Linux: PSS. Windows: private bytes. macOS: physical footprint.

use sysinfo::{Pid, Process};

/// Bytes attributed to `pid`. Falls back to resident memory when the
/// precise counter is unavailable.
pub fn attributed_bytes(pid: Pid, proc_: &Process) -> u64 {
    precise(pid.as_u32()).unwrap_or_else(|| proc_.memory())
}

#[cfg(target_os = "linux")]
fn precise(pid: u32) -> Option<u64> {
    let text = std::fs::read_to_string(format!("/proc/{pid}/smaps_rollup")).ok()?;
    parse_pss_kb(&text).map(|kb| kb * 1024)
}

#[cfg(any(target_os = "linux", test))]
pub fn parse_pss_kb(smaps_rollup: &str) -> Option<u64> {
    smaps_rollup
        .lines()
        .find_map(|l| l.strip_prefix("Pss:"))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|n| n.parse().ok())
}

#[cfg(target_os = "windows")]
fn precise(pid: u32) -> Option<u64> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
    };
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    // SAFETY: plain Win32 calls with a correctly sized, zeroed out-struct.
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return None;
        }
        let mut c: PROCESS_MEMORY_COUNTERS_EX = std::mem::zeroed();
        c.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
        let ok = GetProcessMemoryInfo(h, &mut c as *mut _ as *mut PROCESS_MEMORY_COUNTERS, c.cb);
        CloseHandle(h);
        (ok != 0).then_some(c.PrivateUsage as u64)
    }
}

#[cfg(target_os = "macos")]
fn precise(pid: u32) -> Option<u64> {
    // SAFETY: proc_pid_rusage fills a correctly sized rusage_info_v2.
    unsafe {
        let mut info: libc::rusage_info_v2 = std::mem::zeroed();
        let rc = libc::proc_pid_rusage(
            pid as libc::c_int,
            libc::RUSAGE_INFO_V2,
            &mut info as *mut _ as *mut libc::rusage_info_t,
        );
        (rc == 0).then_some(info.ri_phys_footprint)
    }
}

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
fn precise(_pid: u32) -> Option<u64> {
    None
}

/// The process macOS holds responsible for `pid` (an XPC service's client
/// app). Looked up at runtime because the libsystem symbol is not in the
/// public headers; `None` if it is unavailable.
#[cfg(target_os = "macos")]
pub fn responsible_pid(pid: u32) -> Option<u32> {
    type Fn = unsafe extern "C" fn(libc::pid_t) -> libc::pid_t;
    // SAFETY: dlsym with a NUL-terminated name; the symbol, if present, has
    // the signature `pid_t responsibility_get_pid_responsible_for_pid(pid_t)`.
    unsafe {
        let sym = libc::dlsym(libc::RTLD_DEFAULT, c"responsibility_get_pid_responsible_for_pid".as_ptr());
        if sym.is_null() {
            return None;
        }
        let f: Fn = std::mem::transmute::<*mut libc::c_void, Fn>(sym);
        let r = f(pid as libc::pid_t);
        (r > 0).then_some(r as u32)
    }
}

#[cfg(not(target_os = "macos"))]
pub fn responsible_pid(_pid: u32) -> Option<u32> {
    None
}

/// Context switches across all threads of `pid` (Linux only). Used as a
/// proxy for wakeups.
#[cfg(target_os = "linux")]
pub fn context_switches(pid: u32) -> Option<u64> {
    let mut total = 0u64;
    for task in std::fs::read_dir(format!("/proc/{pid}/task")).ok()? {
        let path = task.ok()?.path().join("status");
        let text = std::fs::read_to_string(path).unwrap_or_default();
        for line in text.lines() {
            if let Some(v) = line
                .strip_prefix("voluntary_ctxt_switches:")
                .or_else(|| line.strip_prefix("nonvoluntary_ctxt_switches:"))
            {
                total += v.trim().parse::<u64>().unwrap_or(0);
            }
        }
    }
    Some(total)
}

#[cfg(not(target_os = "linux"))]
pub fn context_switches(_pid: u32) -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pss() {
        let s = "00400000-7ffd [rollup]\nRss:               51200 kB\nPss:               20480 kB\nPss_Anon: 1 kB\n";
        assert_eq!(parse_pss_kb(s), Some(20480));
        assert_eq!(parse_pss_kb("Rss: 1 kB"), None);
    }
}
