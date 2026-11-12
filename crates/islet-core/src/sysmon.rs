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
    /// Network traffic since the previous reading: (down, up) in bytes a
    /// second. None on the first reading, or where it can't be read.
    pub net: Option<(u64, u64)>,
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

/// Bytes received and sent so far over every real network link, from
/// Linux's `/proc/net/dev` (the loopback doesn't count).
pub fn parse_proc_net_dev(s: &str) -> Option<(u64, u64)> {
    let mut total: Option<(u64, u64)> = None;
    for line in s.lines() {
        let Some((name, rest)) = line.split_once(':') else { continue };
        if name.trim() == "lo" {
            continue;
        }
        let v: Vec<u64> = rest.split_whitespace().filter_map(|n| n.parse().ok()).collect();
        if let (Some(rx), Some(tx)) = (v.first(), v.get(8)) {
            let t = total.get_or_insert((0, 0));
            *t = (t.0 + rx, t.1 + tx);
        }
    }
    total
}

/// The same from macOS's `netstat -ib`: the link-level row of each
/// interface (the others repeat it per address), without the loopback.
pub fn parse_netstat_ib(s: &str) -> Option<(u64, u64)> {
    let mut lines = s.lines();
    let header: Vec<&str> = lines.next()?.split_whitespace().collect();
    let (ibytes, obytes) = (header.iter().position(|h| *h == "Ibytes")?, header.iter().position(|h| *h == "Obytes")?);
    let mut total: Option<(u64, u64)> = None;
    for line in lines {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() < 3 || f[0].starts_with("lo") || !f[2].starts_with("<Link#") {
            continue;
        }
        // A link row without an address has one column fewer.
        let shift = header.len().saturating_sub(f.len());
        let read = |i: usize| f.get(i.checked_sub(shift)?)?.parse::<u64>().ok();
        if let (Some(rx), Some(tx)) = (read(ibytes), read(obytes)) {
            let t = total.get_or_insert((0, 0));
            *t = (t.0 + rx, t.1 + tx);
        }
    }
    total
}

/// The same from Windows' `netstat -e`: the first row of two numbers is
/// the bytes received and sent (its label is translated, so it isn't read).
pub fn parse_netstat_e(s: &str) -> Option<(u64, u64)> {
    s.lines().find_map(|line| {
        let f: Vec<&str> = line.split_whitespace().collect();
        let n = f.len();
        if n < 3 {
            return None;
        }
        Some((f[n - 2].parse().ok()?, f[n - 1].parse().ok()?))
    })
}

/// Traffic between two readings `secs` apart, in bytes a second.
pub fn net_rate(before: (u64, u64), now: (u64, u64), secs: f64) -> Option<(u64, u64)> {
    if secs <= 0.0 || now.0 < before.0 || now.1 < before.1 {
        return None; // a counter started again (a link went away)
    }
    Some((((now.0 - before.0) as f64 / secs) as u64, ((now.1 - before.1) as f64 / secs) as u64))
}

/// "0 KB/s", "740 KB/s", "1.2 MB/s"
pub fn format_rate(bytes_per_sec: u64) -> String {
    let kb = bytes_per_sec as f64 / 1024.0;
    if kb < 1000.0 {
        format!("{kb:.0} KB/s")
    } else {
        format!("{:.1} MB/s", kb / 1024.0)
    }
}

/// How worn the laptop's battery is: what it holds now when full, as a
/// share of what it held when new.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatteryHealth {
    /// 0..=100
    pub percent: u8,
    /// Charge cycles so far, where the battery counts them.
    pub cycles: Option<u32>,
}

impl BatteryHealth {
    /// From the capacity when full now and when new (any unit, the same).
    pub fn from_capacity(full: u64, design: u64, cycles: Option<u32>) -> Option<Self> {
        if design == 0 || full == 0 {
            return None;
        }
        let percent = ((full as f64 / design as f64) * 100.0).round().clamp(1.0, 100.0) as u8;
        Some(Self { percent, cycles: cycles.filter(|c| *c > 0) })
    }

    /// "87%", "87% · 312 cycles"
    pub fn describe(&self) -> String {
        match self.cycles {
            Some(1) => format!("{}% · 1 cycle", self.percent),
            Some(c) => format!("{}% · {c} cycles", self.percent),
            None => format!("{}%", self.percent),
        }
    }
}

/// Battery health from macOS's `ioreg -rn AppleSmartBattery`.
pub fn parse_ioreg_battery(s: &str) -> Option<BatteryHealth> {
    let field = |name: &str| {
        s.lines().find_map(|l| {
            let (key, value) = l.split_once('=')?;
            (key.trim().trim_matches('"') == name).then(|| value.trim().parse::<u64>().ok()).flatten()
        })
    };
    // Apple-silicon Macs report "MaxCapacity" as a percentage; the raw
    // figure is the one to compare with the design capacity.
    let full = field("AppleRawMaxCapacity").or_else(|| field("MaxCapacity"))?;
    BatteryHealth::from_capacity(full, field("DesignCapacity")?, field("CycleCount").map(|c| c as u32))
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
    fn network_traffic_from_each_system() {
        let linux = "Inter-|   Receive                                                |  Transmit\n face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed\n    lo: 5000 10 0 0 0 0 0 0 5000 10 0 0 0 0 0 0\n  eth0: 1000 5 0 0 0 0 0 0 300 4 0 0 0 0 0 0\n wlan0: 2000 9 0 0 0 0 0 0 700 8 0 0 0 0 0 0\n";
        assert_eq!(parse_proc_net_dev(linux), Some((3000, 1000)));
        let mac = "Name  Mtu   Network       Address            Ipkts Ierrs     Ibytes    Opkts Oerrs     Obytes  Coll\nlo0   16384 <Link#1>                          100     0      90000      100     0      90000     0\nen0   1500  <Link#4>      a4:83:e7:00:00:01   5000     0    4000000     3000     0     600000     0\nen0   1500  192.168.1     192.168.1.20       5000     -    4000000     3000     -     600000     -\n";
        assert_eq!(parse_netstat_ib(mac), Some((4_000_000, 600_000)));
        let win = "Interface Statistics\r\n\r\n                           Received            Sent\r\n\r\nBytes                    1234567890       987654321\r\nUnicast packets             1000000          900000\r\n";
        assert_eq!(parse_netstat_e(win), Some((1_234_567_890, 987_654_321)));
        assert_eq!(parse_netstat_e("nothing here"), None);
        assert_eq!(net_rate((1000, 500), (3048, 1524), 2.0), Some((1024, 512)));
        assert_eq!(net_rate((1000, 500), (10, 5), 1.0), None, "the counters started again");
        assert_eq!(format_rate(0), "0 KB/s");
        assert_eq!(format_rate(740 * 1024), "740 KB/s");
        assert_eq!(format_rate(1_258_291), "1.2 MB/s");
    }

    #[test]
    fn battery_health() {
        let h = BatteryHealth::from_capacity(43_500_000, 50_000_000, Some(312)).unwrap();
        assert_eq!((h.percent, h.describe().as_str()), (87, "87% · 312 cycles"));
        assert_eq!(BatteryHealth::from_capacity(52, 50, None).unwrap().describe(), "100%", "never above new");
        assert_eq!(BatteryHealth::from_capacity(50, 0, None), None);
        let ioreg = "    | {\n      \"CycleCount\" = 201\n      \"DesignCapacity\" = 4563\n      \"MaxCapacity\" = 100\n      \"AppleRawMaxCapacity\" = 4106\n    }\n";
        assert_eq!(parse_ioreg_battery(ioreg), Some(BatteryHealth { percent: 90, cycles: Some(201) }));
        assert_eq!(parse_ioreg_battery("\"MaxCapacity\" = 5000\n\"DesignCapacity\" = 6250\n").unwrap().percent, 80);
        assert_eq!(parse_ioreg_battery("no battery"), None);
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
