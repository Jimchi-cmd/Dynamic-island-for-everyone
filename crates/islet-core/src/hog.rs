//! The performance alert (opt-in): when one program has been using most of
//! the processor for a while, or a large share of the memory, the island
//! says so and offers to close it.

use std::collections::HashMap;

/// A running program, as the OS lists it.
#[derive(Debug, Clone, PartialEq)]
pub struct Proc {
    pub pid: u32,
    pub name: String,
    /// Processor time used so far, in seconds (Linux, Windows)…
    pub cpu_secs: Option<f64>,
    /// …or the OS's own recent figure, in percent of one core (macOS).
    pub cpu_pct: Option<f32>,
    /// Memory in use, in bytes.
    pub rss: u64,
}

/// What a program is hogging.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    /// Its share of the whole processor (all cores), 0..=1.
    Cpu(f32),
    /// Bytes of memory, and their share of all memory, 0..=1.
    Memory(u64, f32),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Hog {
    pub pid: u32,
    pub name: String,
    pub kind: Kind,
}

/// A program counts once it has more than half the processor…
pub const CPU_SHARE: f32 = 0.5;
/// …for this many readings in a row (a minute, at one every 20 s)…
pub const CPU_STRIKES: u8 = 3;
/// …or this share of the memory, and at least `MEMORY_FLOOR` bytes.
pub const MEMORY_SHARE: f32 = 0.4;
pub const MEMORY_FLOOR: u64 = 1 << 30;

impl Hog {
    /// "is using 62% of the processor", "is using 5.2 GB of memory".
    pub fn describe(&self) -> String {
        match self.kind {
            Kind::Cpu(share) => format!("is using {}% of the processor", (share * 100.0).round() as u32),
            Kind::Memory(bytes, _) => format!("is using {} of memory", gigabytes(bytes)),
        }
    }

    /// How full its meter is, 0..=1.
    pub fn share(&self) -> f32 {
        match self.kind {
            Kind::Cpu(s) | Kind::Memory(_, s) => s.clamp(0.0, 1.0),
        }
    }
}

fn gigabytes(bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / (1u64 << 30) as f64)
}

/// A program's name for people: no folder, no ".exe".
pub fn display_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let base = base.strip_suffix(".exe").or_else(|| base.strip_suffix(".EXE")).unwrap_or(base);
    base.trim().to_string()
}

/// Remembers the last reading, to turn processor time into a share.
#[derive(Debug, Default)]
pub struct Watch {
    before: HashMap<u32, f64>,
    before_at: Option<f64>,
    strikes: HashMap<u32, u8>,
}

impl Watch {
    /// Take a reading at `now` (seconds). Returns the worst offender, if
    /// any: a processor hog comes before a memory hog. `skip` says which
    /// programs never count (Islet itself, the system's own).
    pub fn update(
        &mut self,
        procs: &[Proc],
        now: f64,
        cores: u32,
        mem_total: u64,
        skip: impl Fn(&Proc) -> bool,
    ) -> Option<Hog> {
        let cores = cores.max(1) as f32;
        let dt = self.before_at.map(|b| now - b).filter(|d| *d > 0.5);
        let mut strikes = HashMap::new();
        let mut cpu: Option<Hog> = None;
        let mut memory: Option<Hog> = None;
        for p in procs.iter().filter(|p| !skip(p)) {
            let share = match (p.cpu_pct, p.cpu_secs, dt, self.before.get(&p.pid)) {
                (Some(pct), ..) => Some(pct / 100.0 / cores),
                (None, Some(secs), Some(dt), Some(before)) => Some(((secs - before) / dt).max(0.0) as f32 / cores),
                _ => None,
            };
            if let Some(share) = share.filter(|s| *s >= CPU_SHARE) {
                let n = self.strikes.get(&p.pid).copied().unwrap_or(0).saturating_add(1);
                strikes.insert(p.pid, n);
                if n >= CPU_STRIKES && cpu.as_ref().is_none_or(|h| h.share() < share) {
                    cpu = Some(Hog { pid: p.pid, name: display_name(&p.name), kind: Kind::Cpu(share.min(1.0)) });
                }
            }
            if mem_total > 0 && p.rss >= MEMORY_FLOOR {
                let share = (p.rss as f64 / mem_total as f64) as f32;
                if share >= MEMORY_SHARE && memory.as_ref().is_none_or(|h| h.share() < share) {
                    memory = Some(Hog {
                        pid: p.pid,
                        name: display_name(&p.name),
                        kind: Kind::Memory(p.rss, share.min(1.0)),
                    });
                }
            }
        }
        self.strikes = strikes;
        self.before = procs.iter().filter_map(|p| Some((p.pid, p.cpu_secs?))).collect();
        self.before_at = Some(now);
        cpu.or(memory)
    }
}

/// `utime + stime` (in clock ticks) and the resident pages from a Linux
/// `/proc/<pid>/stat` line. The name is in brackets and may hold spaces.
pub fn parse_proc_pid_stat(stat: &str) -> Option<(String, u64, u64)> {
    let open = stat.find('(')?;
    let close = stat.rfind(')')?;
    let name = stat.get(open + 1..close)?.to_string();
    let rest: Vec<&str> = stat.get(close + 1..)?.split_whitespace().collect();
    // After the name: state is field 3, so utime (14) is index 11 here.
    let ticks = rest.get(11)?.parse::<u64>().ok()? + rest.get(12)?.parse::<u64>().ok()?;
    let pages = rest.get(21)?.parse::<u64>().ok()?;
    Some((name, ticks, pages))
}

/// One line of `ps -Ao pid=,pcpu=,rss=,comm=` (macOS): rss is in KiB.
pub fn parse_ps_line(line: &str) -> Option<Proc> {
    let mut it = line.split_whitespace();
    let pid = it.next()?.parse().ok()?;
    let pct: f32 = it.next()?.replace(',', ".").parse().ok()?;
    let rss_kb: u64 = it.next()?.parse().ok()?;
    let name = it.collect::<Vec<_>>().join(" ");
    (!name.is_empty()).then(|| Proc { pid, name, cpu_secs: None, cpu_pct: Some(pct), rss: rss_kb * 1024 })
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1 << 30;

    fn p(pid: u32, name: &str, secs: f64, rss: u64) -> Proc {
        Proc { pid, name: name.into(), cpu_secs: Some(secs), cpu_pct: None, rss }
    }

    #[test]
    fn a_program_must_hog_the_processor_for_a_while() {
        let mut w = Watch::default();
        let none = |_: &Proc| false;
        // Two cores. "render" burns 1.4 cores (70% of the machine); "idle" nothing.
        let at = |i: f64| [p(10, "/usr/bin/render", i * 28.0, GB / 4), p(11, "idle", 1.0, GB / 8)];
        assert_eq!(w.update(&at(0.0), 0.0, 2, 8 * GB, none), None, "the first reading has nothing to compare");
        assert_eq!(w.update(&at(1.0), 20.0, 2, 8 * GB, none), None, "one busy reading");
        assert_eq!(w.update(&at(2.0), 40.0, 2, 8 * GB, none), None, "two");
        let hog = w.update(&at(3.0), 60.0, 2, 8 * GB, none).expect("a minute of it");
        assert_eq!((hog.pid, hog.name.as_str()), (10, "render"));
        assert_eq!(hog.describe(), "is using 70% of the processor");
        // It calms down: the count starts again.
        let calm = [p(10, "/usr/bin/render", 3.0 * 28.0 + 1.0, GB / 4)];
        assert_eq!(w.update(&calm, 80.0, 2, 8 * GB, none), None);
        let busy = [p(10, "/usr/bin/render", 3.0 * 28.0 + 29.0, GB / 4)];
        assert_eq!(w.update(&busy, 100.0, 2, 8 * GB, none), None, "busy again, but only just");
    }

    #[test]
    fn a_big_share_of_memory_counts_at_once_and_some_programs_never_do() {
        let mut w = Watch::default();
        let procs = [p(20, "C:\\Apps\\Browser.exe", 5.0, 5 * GB), p(21, "small", 5.0, GB / 2)];
        let hog = w.update(&procs, 0.0, 4, 8 * GB, |_| false).expect("5 of 8 GB");
        assert_eq!(hog.name, "Browser");
        assert_eq!(hog.describe(), "is using 5.0 GB of memory");
        assert!((hog.share() - 0.625).abs() < 0.001);
        // On a big machine the same 5 GB is nothing special.
        assert_eq!(Watch::default().update(&procs, 0.0, 4, 64 * GB, |_| false), None);
        // Skipped programs (Islet itself, the system's) are never reported.
        assert_eq!(Watch::default().update(&procs, 0.0, 4, 8 * GB, |p| p.pid == 20), None);
    }

    #[test]
    fn the_systems_own_percentage_is_used_where_given() {
        let mut w = Watch::default();
        // macOS: 310% of one core on a 4-core machine is 77.5% of it.
        let procs =
            [parse_ps_line("  812 310,0 204800 /Applications/Video Tool.app/Contents/MacOS/Video Tool").unwrap()];
        assert_eq!(procs[0].rss, 204_800 * 1024);
        for t in [0.0, 20.0] {
            assert_eq!(w.update(&procs, t, 4, 16 * GB, |_| false), None);
        }
        let hog = w.update(&procs, 40.0, 4, 16 * GB, |_| false).unwrap();
        assert_eq!(hog.name, "Video Tool");
        assert_eq!(hog.describe(), "is using 78% of the processor");
        assert!(parse_ps_line("garbage").is_none());
    }

    #[test]
    fn reads_linux_stat_lines_with_awkward_names() {
        let line = "4242 (Web Content (x)) S 1 4242 4242 0 -1 4194560 100 0 0 0 1500 250 0 0 20 0 30 0 1000 2000000 5120 18446744073709551615 0 0";
        let (name, ticks, pages) = parse_proc_pid_stat(line).unwrap();
        assert_eq!((name.as_str(), ticks, pages), ("Web Content (x)", 1750, 5120));
        assert!(parse_proc_pid_stat("nonsense").is_none());
    }
}
