//! The stopwatch (Islet Pro): start, pause, laps, reset. Times are on the
//! app clock (seconds); like the timer, it keeps wall-clock time across
//! sleep because the app clock is corrected for sleep.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StopwatchCommand {
    Start,
    Pause,
    Resume,
    Lap,
    Reset,
}

/// At most this many laps are kept (the newest).
pub const MAX_LAPS: usize = 99;

#[derive(Debug, Clone, PartialEq)]
pub struct Stopwatch {
    /// Time counted before the current run.
    banked: f64,
    /// When the current run started (None while paused).
    since: Option<f64>,
    /// Lap times: each lap's own length, oldest first.
    laps: Vec<f64>,
    /// Total time at the last lap mark.
    lap_mark: f64,
    /// How many laps there have been (older ones may have been dropped).
    lap_count: usize,
}

impl Stopwatch {
    pub fn started(t: f64) -> Self {
        Stopwatch { banked: 0.0, since: Some(t), laps: Vec::new(), lap_mark: 0.0, lap_count: 0 }
    }

    pub fn elapsed(&self, t: f64) -> f64 {
        self.banked + self.since.map_or(0.0, |s| (t - s).max(0.0))
    }

    pub fn running(&self) -> bool {
        self.since.is_some()
    }

    pub fn pause(&mut self, t: f64) {
        if let Some(s) = self.since.take() {
            self.banked += (t - s).max(0.0);
        }
    }

    pub fn resume(&mut self, t: f64) {
        if self.since.is_none() {
            self.since = Some(t);
        }
    }

    /// Mark a lap (only while running).
    pub fn lap(&mut self, t: f64) {
        if !self.running() {
            return;
        }
        let now = self.elapsed(t);
        self.laps.push(now - self.lap_mark);
        self.lap_mark = now;
        self.lap_count += 1;
        if self.laps.len() > MAX_LAPS {
            self.laps.remove(0);
        }
    }

    /// The current, unfinished lap.
    pub fn current_lap(&self, t: f64) -> f64 {
        self.elapsed(t) - self.lap_mark
    }

    /// Finished laps, newest first, with their numbers.
    pub fn laps(&self) -> impl Iterator<Item = (usize, f64)> + '_ {
        let first = self.lap_count - self.laps.len() + 1;
        self.laps.iter().enumerate().rev().map(move |(i, &d)| (first + i, d))
    }

    pub fn lap_count(&self) -> usize {
        self.lap_count
    }

    /// The fastest and slowest finished laps (numbers), once there are two.
    pub fn extremes(&self) -> Option<(usize, usize)> {
        if self.laps.len() < 2 {
            return None;
        }
        let first = self.lap_count - self.laps.len() + 1;
        let by = |better: fn(f64, f64) -> bool| {
            let mut best = 0;
            for (i, &d) in self.laps.iter().enumerate() {
                if better(d, self.laps[best]) {
                    best = i;
                }
            }
            first + best
        };
        Some((by(|a, b| a < b), by(|a, b| a > b)))
    }

    /// When the shown time next changes: whole seconds (`tenths` false) or
    /// tenths. None while paused.
    pub fn next_tick(&self, t: f64, tenths: bool) -> Option<f64> {
        self.since?;
        let step = if tenths { 0.1 } else { 1.0 };
        let e = self.elapsed(t);
        Some(t + (step - e % step).max(0.001))
    }
}

/// `0:07`, `12:34`, `1:02:03`; with `tenths`, `0:07.4`.
pub fn format(secs: f64, tenths: bool) -> String {
    let secs = secs.max(0.0);
    let whole = secs.floor() as u64;
    let (h, m, s) = (whole / 3600, whole / 60 % 60, whole % 60);
    let base = if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m}:{s:02}") };
    if tenths {
        format!("{base}.{}", ((secs - secs.floor()) * 10.0).floor() as u64 % 10)
    } else {
        base
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_runs_and_pauses() {
        let mut s = Stopwatch::started(10.0);
        assert_eq!(s.elapsed(15.0), 5.0);
        s.pause(15.0);
        assert_eq!(s.elapsed(100.0), 5.0, "paused time doesn't count");
        assert_eq!(s.next_tick(100.0, false), None);
        s.resume(100.0);
        s.resume(101.0); // a second resume changes nothing
        assert_eq!(s.elapsed(102.5), 7.5);
    }

    #[test]
    fn keeps_laps_newest_first_and_finds_extremes() {
        let mut s = Stopwatch::started(0.0);
        s.lap(10.0);
        s.lap(14.0);
        s.lap(30.0);
        assert_eq!(s.laps().collect::<Vec<_>>(), vec![(3, 16.0), (2, 4.0), (1, 10.0)]);
        assert_eq!(s.current_lap(31.0), 1.0);
        assert_eq!(s.extremes(), Some((2, 3)));
        s.pause(31.0);
        s.lap(40.0);
        assert_eq!(s.lap_count(), 3, "no laps while paused");
    }

    #[test]
    fn drops_the_oldest_laps() {
        let mut s = Stopwatch::started(0.0);
        for i in 1..=(MAX_LAPS + 5) {
            s.lap(i as f64);
        }
        let laps: Vec<_> = s.laps().collect();
        assert_eq!(laps.len(), MAX_LAPS);
        assert_eq!(laps[0].0, MAX_LAPS + 5);
        assert_eq!(laps.last().unwrap().0, 6);
    }

    #[test]
    fn ticks_on_the_shown_unit() {
        let s = Stopwatch::started(0.0);
        assert!((s.next_tick(2.3, false).unwrap() - 3.0).abs() < 1e-9);
        assert!((s.next_tick(2.34, true).unwrap() - 2.4).abs() < 1e-9);
    }

    #[test]
    fn formats() {
        assert_eq!(format(7.46, false), "0:07");
        assert_eq!(format(7.46, true), "0:07.4");
        assert_eq!(format(754.0, false), "12:34");
        assert_eq!(format(3723.95, true), "1:02:03.9");
        assert_eq!(format(-1.0, false), "0:00");
    }

    #[test]
    fn commands_read_from_json() {
        assert_eq!(serde_json::from_str::<StopwatchCommand>("\"lap\"").unwrap(), StopwatchCommand::Lap);
    }
}
