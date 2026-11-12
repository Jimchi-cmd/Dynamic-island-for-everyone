//! The system monitor: CPU, memory and disk, read once a second while the
//! panel is open. When it's closed the thread sleeps on its channel (no
//! wakeups).

use super::Sink;
use islet_core::activity::Report;
use islet_core::sysmon::{CpuTimes, Stats};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::time::Duration;

const EVERY: Duration = Duration::from_secs(1);

pub fn start(sink: Sink) -> Sender<bool> {
    let (tx, rx) = channel::<bool>();
    let _ = std::thread::Builder::new().name("sysmon".into()).spawn(move || {
        while let Ok(on) = rx.recv() {
            if !on {
                continue;
            }
            let mut before = cpu_times();
            let mut net = Net::default();
            // Report memory and disk at once; CPU and traffic need two readings.
            sink.send(Report::System(Some(sample(None, &mut before, &mut net))));
            loop {
                match rx.recv_timeout(EVERY) {
                    Ok(true) => continue,
                    Ok(false) => {
                        sink.send(Report::System(None));
                        break;
                    }
                    Err(RecvTimeoutError::Timeout) => {
                        let prev = before;
                        sink.send(Report::System(Some(sample(prev, &mut before, &mut net))));
                    }
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            }
        }
    });
    tx
}

/// All the memory this computer has, in bytes (0 if it can't be read).
pub fn memory_total() -> u64 {
    memory().map_or(0, |(_, total)| total)
}

/// Network traffic between readings. Where the totals come from a tool
/// (macOS, Windows) they are read every other second, not every second.
#[derive(Default)]
struct Net {
    before: Option<((u64, u64), std::time::Instant)>,
    rate: Option<(u64, u64)>,
    tick: u32,
}

impl Net {
    fn sample(&mut self) -> Option<(u64, u64)> {
        self.tick = self.tick.wrapping_add(1);
        if NET_SLOWLY && self.tick % 2 == 0 {
            return self.rate;
        }
        let now = (net_totals()?, std::time::Instant::now());
        if let Some((totals, at)) = self.before {
            self.rate = islet_core::sysmon::net_rate(totals, now.0, now.1.duration_since(at).as_secs_f64());
        }
        self.before = Some(now);
        self.rate
    }
}

fn sample(prev: Option<CpuTimes>, before: &mut Option<CpuTimes>, net: &mut Net) -> Stats {
    let now = cpu_times();
    let cpu = match (prev, now) {
        (Some(a), Some(b)) => b.load_since(&a),
        _ => 0.0,
    };
    *before = now;
    let (mem_used, mem_total) = memory().unwrap_or_default();
    let (disk_used, disk_total) = disk().unwrap_or_default();
    Stats { cpu, mem_used, mem_total, disk_used, disk_total, net: net.sample() }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn tool_output(program: &str, args: &[&str]) -> Option<String> {
    use std::process::{Command, Stdio};
    let out = Command::new(program).args(args).stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Read how worn the laptop's battery is, on another thread, and report it
/// (None on a computer without a battery, or where it can't be told).
pub fn read_battery_health(sink: Sink) {
    std::thread::spawn(move || sink.send(Report::BatteryHealth(battery_health())));
}

// ---- Linux -----------------------------------------------------------------

#[cfg(target_os = "linux")]
fn cpu_times() -> Option<CpuTimes> {
    islet_core::sysmon::parse_proc_stat(&std::fs::read_to_string("/proc/stat").ok()?)
}

#[cfg(target_os = "linux")]
fn memory() -> Option<(u64, u64)> {
    islet_core::sysmon::parse_meminfo(&std::fs::read_to_string("/proc/meminfo").ok()?)
}

/// Whether traffic is read every other second only (where it takes a tool).
#[cfg(target_os = "linux")]
const NET_SLOWLY: bool = false;

#[cfg(target_os = "linux")]
fn net_totals() -> Option<(u64, u64)> {
    islet_core::sysmon::parse_proc_net_dev(&std::fs::read_to_string("/proc/net/dev").ok()?)
}

#[cfg(target_os = "linux")]
fn battery_health() -> Option<islet_core::sysmon::BatteryHealth> {
    let dir = std::fs::read_dir("/sys/class/power_supply").ok()?;
    dir.filter_map(Result::ok).find_map(|e| {
        let path = e.path();
        let read = |name: &str| std::fs::read_to_string(path.join(name)).ok()?.trim().parse::<u64>().ok();
        if std::fs::read_to_string(path.join("type")).ok()?.trim() != "Battery" {
            return None;
        }
        let (full, design) = match (read("energy_full"), read("energy_full_design")) {
            (Some(f), Some(d)) => (f, d),
            _ => (read("charge_full")?, read("charge_full_design")?),
        };
        islet_core::sysmon::BatteryHealth::from_capacity(full, design, read("cycle_count").map(|c| c as u32))
    })
}

#[cfg(unix)]
fn statvfs(path: &str) -> Option<(u64, u64)> {
    let c = std::ffi::CString::new(path).ok()?;
    // SAFETY: zeroed out-struct filled by statvfs for a valid path.
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c.as_ptr(), &mut st) } != 0 {
        return None;
    }
    let unit = st.f_frsize as u64;
    let total = st.f_blocks as u64 * unit;
    // Used as Files/Finder show it: everything not free for users.
    let used = total.saturating_sub(st.f_bavail as u64 * unit);
    Some((used, total))
}

#[cfg(target_os = "linux")]
fn disk() -> Option<(u64, u64)> {
    statvfs(&std::env::var("HOME").unwrap_or_else(|_| "/".into()))
}

// ---- Windows ---------------------------------------------------------------

#[cfg(target_os = "windows")]
fn cpu_times() -> Option<CpuTimes> {
    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::System::Threading::GetSystemTimes;
    let (mut idle, mut kernel, mut user) = (FILETIME::default(), FILETIME::default(), FILETIME::default());
    // SAFETY: three out-pointers to local FILETIMEs.
    unsafe { GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)) }.ok()?;
    let v = |f: FILETIME| (u64::from(f.dwHighDateTime) << 32) | u64::from(f.dwLowDateTime);
    // Kernel time includes idle time.
    let total = v(kernel) + v(user);
    Some(CpuTimes { busy: total.saturating_sub(v(idle)), total })
}

#[cfg(target_os = "windows")]
fn memory() -> Option<(u64, u64)> {
    use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let mut m = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
    // SAFETY: a sized MEMORYSTATUSEX.
    unsafe { GlobalMemoryStatusEx(&mut m) }.ok()?;
    Some((m.ullTotalPhys.saturating_sub(m.ullAvailPhys), m.ullTotalPhys))
}

#[cfg(target_os = "windows")]
fn disk() -> Option<(u64, u64)> {
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into()) + "\\";
    let w: Vec<u16> = drive.encode_utf16().chain(std::iter::once(0)).collect();
    let (mut avail, mut total) = (0u64, 0u64);
    // SAFETY: NUL-terminated path and out-pointers.
    unsafe { GetDiskFreeSpaceExW(PCWSTR(w.as_ptr()), Some(&mut avail), Some(&mut total), None) }.ok()?;
    Some((total.saturating_sub(avail), total))
}

// ---- macOS -----------------------------------------------------------------

#[cfg(target_os = "windows")]
const NET_SLOWLY: bool = true;

#[cfg(target_os = "windows")]
fn net_totals() -> Option<(u64, u64)> {
    islet_core::sysmon::parse_netstat_e(&tool_output("netstat", &["-e"])?)
}

#[cfg(target_os = "windows")]
fn battery_health() -> Option<islet_core::sysmon::BatteryHealth> {
    // Full-charge and design capacity, in mWh, on one line.
    let script = "$f=(Get-CimInstance -Namespace root/WMI -ClassName BatteryFullChargedCapacity -ErrorAction Stop | \
                  Select-Object -First 1).FullChargedCapacity; \
                  $d=(Get-CimInstance -Namespace root/WMI -ClassName BatteryStaticData -ErrorAction Stop | \
                  Select-Object -First 1).DesignedCapacity; \"$f $d\"";
    let out = tool_output("powershell", &["-NoProfile", "-NonInteractive", "-Command", script])?;
    let mut n = out.split_whitespace().filter_map(|v| v.parse::<u64>().ok());
    islet_core::sysmon::BatteryHealth::from_capacity(n.next()?, n.next()?, None)
}

#[cfg(target_os = "macos")]
const NET_SLOWLY: bool = true;

#[cfg(target_os = "macos")]
fn net_totals() -> Option<(u64, u64)> {
    islet_core::sysmon::parse_netstat_ib(&tool_output("/usr/sbin/netstat", &["-ib"])?)
}

#[cfg(target_os = "macos")]
fn battery_health() -> Option<islet_core::sysmon::BatteryHealth> {
    islet_core::sysmon::parse_ioreg_battery(&tool_output("/usr/sbin/ioreg", &["-rn", "AppleSmartBattery"])?)
}

#[cfg(target_os = "macos")]
mod mach {
    pub const HOST_CPU_LOAD_INFO: i32 = 3;
    pub const HOST_VM_INFO64: i32 = 4;

    #[repr(C)]
    #[derive(Default)]
    pub struct VmStatistics64 {
        pub free_count: u32,
        pub active_count: u32,
        pub inactive_count: u32,
        pub wire_count: u32,
        pub zero_fill_count: u64,
        pub reactivations: u64,
        pub pageins: u64,
        pub pageouts: u64,
        pub faults: u64,
        pub cow_faults: u64,
        pub lookups: u64,
        pub hits: u64,
        pub purges: u64,
        pub purgeable_count: u32,
        pub speculative_count: u32,
        pub decompressions: u64,
        pub compressions: u64,
        pub swapins: u64,
        pub swapouts: u64,
        pub compressor_page_count: u32,
        pub throttled_count: u32,
        pub external_page_count: u32,
        pub internal_page_count: u32,
        pub total_uncompressed_pages_in_compressor: u64,
    }

    extern "C" {
        pub fn mach_host_self() -> u32;
        pub fn host_statistics(host: u32, flavor: i32, info: *mut i32, count: *mut u32) -> i32;
        pub fn host_statistics64(host: u32, flavor: i32, info: *mut i32, count: *mut u32) -> i32;
    }
}

#[cfg(target_os = "macos")]
fn cpu_times() -> Option<CpuTimes> {
    // user, system, idle, nice ticks.
    let mut ticks = [0u32; 4];
    let mut count = 4u32;
    // SAFETY: a 4-entry buffer for HOST_CPU_LOAD_INFO.
    let r = unsafe {
        mach::host_statistics(mach::mach_host_self(), mach::HOST_CPU_LOAD_INFO, ticks.as_mut_ptr().cast(), &mut count)
    };
    if r != 0 {
        return None;
    }
    let total: u64 = ticks.iter().map(|&t| u64::from(t)).sum();
    Some(CpuTimes { busy: total - u64::from(ticks[2]), total })
}

#[cfg(target_os = "macos")]
fn memory() -> Option<(u64, u64)> {
    let mut total = 0u64;
    let mut size = std::mem::size_of::<u64>();
    // SAFETY: a u64 out-buffer for hw.memsize.
    let r = unsafe {
        libc::sysctlbyname(c"hw.memsize".as_ptr(), (&mut total as *mut u64).cast(), &mut size, std::ptr::null_mut(), 0)
    };
    if r != 0 {
        return None;
    }
    let mut vm = mach::VmStatistics64::default();
    let mut count = (std::mem::size_of::<mach::VmStatistics64>() / 4) as u32;
    // SAFETY: a VmStatistics64-sized buffer for HOST_VM_INFO64.
    let r = unsafe {
        mach::host_statistics64(
            mach::mach_host_self(),
            mach::HOST_VM_INFO64,
            (&mut vm as *mut mach::VmStatistics64).cast(),
            &mut count,
        )
    };
    if r != 0 {
        return Some((0, total));
    }
    // SAFETY: sysconf has no preconditions.
    let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) }.max(4096) as u64;
    // As Activity Monitor counts "Memory Used": app + wired + compressed.
    let pages = u64::from(vm.internal_page_count.saturating_sub(vm.purgeable_count))
        + u64::from(vm.wire_count)
        + u64::from(vm.compressor_page_count);
    Some(((pages * page).min(total), total))
}

#[cfg(target_os = "macos")]
fn disk() -> Option<(u64, u64)> {
    // The writable data volume (the system volume is sealed and small).
    statvfs("/System/Volumes/Data").or_else(|| statvfs("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs on every system in CI: each one must be able to count traffic.
    #[test]
    fn reads_the_network_counters_and_memory() {
        let totals = net_totals();
        assert!(totals.is_some(), "no network counters on this system");
        assert!(memory_total() > 0);
        // No battery on a CI machine is fine; the call just mustn't hang or panic.
        let _ = battery_health();
    }
}
