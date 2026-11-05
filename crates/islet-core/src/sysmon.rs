//! The system monitor panel's numbers: CPU load, memory and disk use.
//! Sampled once a second, and only while the panel is open.

/// One reading.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Stats {
    /// Share of all cores busy since the previous reading, 0..=1.
    pub cpu: f32,
    pub mem_used: u64,
    pub mem_total: u64,
    pub disk_used: u64,
    pub disk_total: u64,
}

impl Stats {
    pub fn mem_fraction(&self) -> f32 {
        fraction(self.mem_used, self.mem_total)
    }

    pub fn disk_fraction(&self) -> f32 {
        fraction(self.disk_used, self.disk_total)
    }
}

fn fraction(used: u64, total: u64) -> f32 {
    if total == 0 {
        0.0
    } else {
        (used as f64 / total as f64).clamp(0.0, 1.0) as f32
    }
}

/// Cumulative CPU time: (busy, total), in any consistent unit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CpuTimes {
    pub busy: u64,
    pub total: u64,
}

impl CpuTimes {
    /// Busy share between two readings.
    pub fn load_since(&self, before: &CpuTimes) -> f32 {
        let total = self.total.saturating_sub(before.total);
        if total == 0 {
            return 0.0;
        }
        (self.busy.saturating_sub(before.busy) as f64 / total as f64).clamp(0.0, 1.0) as f32
    }
}

/// The aggregate `cpu` line of Linux's `/proc/stat`.
pub fn parse_proc_stat(s: &str) -> Option<CpuTimes> {
    let line = s.lines().find(|l| l.starts_with("cpu "))?;
    let v: Vec<u64> = line.split_whitespace().skip(1).filter_map(|n| n.parse().ok()).collect();
    if v.len() < 4 {
        return None;
    }
    // user nice system idle iowait irq softirq steal (guest time is already
    // counted in user/nice).
    let total: u64 = v.iter().take(8).sum();
    let idle = v[3] + v.get(4).copied().unwrap_or(0);
    Some(CpuTimes { busy: total - idle, total })
}

/// (used, total) bytes from Linux's `/proc/meminfo`: used is what apps
/// hold, i.e. total minus `MemAvailable`.
pub fn parse_meminfo(s: &str) -> Option<(u64, u64)> {
    let field = |name: &str| {
        s.lines()
            .find_map(|l| l.strip_prefix(name)?.strip_prefix(':'))
            .and_then(|v| v.split_whitespace().next()?.parse::<u64>().ok())
            .map(|kb| kb * 1024)
    };
    let total = field("MemTotal")?;
    let avail = field("MemAvailable").or_else(|| field("MemFree"))?;
    Some((total.saturating_sub(avail), total))
}

/// "5.2 of 16 GB"
pub fn format_bytes_of(used: u64, total: u64) -> String {
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;
    let (u, t) = (used as f64 / GB, total as f64 / GB);
    if t >= 100.0 {
        format!("{u:.0} of {t:.0} GB")
    } else {
        format!("{u:.1} of {t:.0} GB")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_load_between_readings() {
        let a = parse_proc_stat("cpu  100 0 50 800 50 0 0 0 0 0\ncpu0 1 2 3 4\n").unwrap();
        assert_eq!(a, CpuTimes { busy: 150, total: 1000 });
        let b = parse_proc_stat("cpu  250 0 100 1000 50 0 0 0 0 0\n").unwrap();
        assert!((b.load_since(&a) - 200.0 / 400.0).abs() < 1e-6);
        assert_eq!(a.load_since(&a), 0.0);
        assert_eq!(parse_proc_stat("intr 1 2 3"), None);
    }

    #[test]
    fn memory_in_use() {
        let m = "MemTotal:       16000000 kB\nMemFree:         1000000 kB\nMemAvailable:    6000000 kB\n";
        assert_eq!(parse_meminfo(m), Some((10_000_000 * 1024, 16_000_000 * 1024)));
        assert_eq!(parse_meminfo("MemTotal: 100 kB\nMemFree: 40 kB\n"), Some((60 * 1024, 100 * 1024)));
        assert_eq!(parse_meminfo("nothing"), None);
    }

    #[test]
    fn readable_sizes() {
        let gb = 1024 * 1024 * 1024;
        assert_eq!(format_bytes_of(5 * gb + gb / 5, 16 * gb), "5.2 of 16 GB");
        assert_eq!(format_bytes_of(120 * gb, 512 * gb), "120 of 512 GB");
        let s = Stats { mem_used: gb, mem_total: 4 * gb, ..Default::default() };
        assert_eq!(s.mem_fraction(), 0.25);
        assert_eq!(s.disk_fraction(), 0.0);
    }
}
