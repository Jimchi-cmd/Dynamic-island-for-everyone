//! The focus streak: how many days in a row you finished at least one focus
//! round. Kept in focus.json on this computer only.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Streak {
    /// The last day a round was finished, as days since 1970 in local time.
    pub last_day: i64,
    /// Days in a row up to `last_day` (0: never).
    pub days: u32,
    /// The longest run so far.
    pub best: u32,
    /// Focus rounds finished, ever, and on `last_day`.
    pub rounds: u32,
    pub today: u32,
}

/// The local calendar day of a Unix time.
pub fn day_of(epoch: i64, utc_offset: i64) -> i64 {
    (epoch + utc_offset).div_euclid(86_400)
}

impl Streak {
    /// A focus round was finished on `day`.
    pub fn record(&mut self, day: i64) {
        if self.days == 0 || day > self.last_day + 1 {
            self.days = 1;
            self.today = 0;
        } else if day == self.last_day + 1 {
            self.days += 1;
            self.today = 0;
        } else if day < self.last_day {
            // The clock went backwards (travel, a wrong date): count the
            // round, leave the run alone.
            self.rounds += 1;
            return;
        }
        self.last_day = day;
        self.today += 1;
        self.rounds += 1;
        self.best = self.best.max(self.days);
    }

    /// Days in a row as of `day`: the run survives until a whole day is missed.
    pub fn current(&self, day: i64) -> u32 {
        if self.days > 0 && day <= self.last_day + 1 {
            self.days
        } else {
            0
        }
    }

    /// Rounds finished on `day`.
    pub fn done_on(&self, day: i64) -> u32 {
        if self.days > 0 && day == self.last_day {
            self.today
        } else {
            0
        }
    }

    /// "3 days in a row", or None before the second day.
    pub fn label(&self, day: i64) -> Option<String> {
        match self.current(day) {
            0 | 1 => None,
            n => Some(format!("{n} days in a row")),
        }
    }

    /// A sentence for `islet focus streak`.
    pub fn describe(&self, day: i64) -> String {
        let rounds = |n: u32| if n == 1 { "1 round".to_string() } else { format!("{n} rounds") };
        let days = |n: u32| if n == 1 { "1 day".to_string() } else { format!("{n} days") };
        match self.current(day) {
            0 if self.rounds == 0 => "No focus rounds yet. Start one with: islet focus".into(),
            0 => format!("No streak right now. Best: {}, {} in all.", days(self.best), rounds(self.rounds)),
            1 => format!("1 day so far ({} today). Best: {}.", rounds(self.done_on(day)), days(self.best)),
            n => format!("{n} days in a row ({} today). Best: {}.", rounds(self.done_on(day)), days(self.best)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_days_in_a_row() {
        let mut s = Streak::default();
        assert_eq!((s.current(100), s.label(100)), (0, None));
        s.record(100);
        s.record(100);
        assert_eq!((s.current(100), s.done_on(100), s.rounds), (1, 2, 2));
        assert_eq!(s.label(100), None, "one day isn't a streak yet");
        s.record(101);
        s.record(102);
        assert_eq!(s.current(102), 3);
        assert_eq!(s.label(102).as_deref(), Some("3 days in a row"));
        // Nothing done yet today: yesterday's run is still alive.
        assert_eq!((s.current(103), s.done_on(103)), (3, 0));
        // A whole day missed ends it.
        assert_eq!(s.current(104), 0);
        s.record(105);
        assert_eq!((s.current(105), s.best, s.rounds), (1, 3, 5));
    }

    #[test]
    fn a_clock_going_backwards_keeps_the_run() {
        let mut s = Streak::default();
        s.record(200);
        s.record(201);
        s.record(199);
        assert_eq!((s.last_day, s.days, s.rounds), (201, 2, 3));
    }

    #[test]
    fn days_follow_the_local_clock() {
        // 23:30 UTC on day 0 is already tomorrow at UTC+3, still today at UTC-5.
        let late = 23 * 3600 + 1800;
        assert_eq!(day_of(late, 3 * 3600), 1);
        assert_eq!(day_of(late, -5 * 3600), 0);
        assert_eq!(day_of(3600, -5 * 3600), -1);
    }

    #[test]
    fn describes_itself() {
        let mut s = Streak::default();
        assert!(s.describe(10).starts_with("No focus rounds yet"));
        s.record(10);
        assert_eq!(s.describe(10), "1 day so far (1 round today). Best: 1 day.");
        s.record(11);
        s.record(11);
        assert_eq!(s.describe(11), "2 days in a row (2 rounds today). Best: 2 days.");
        assert_eq!(s.describe(20), "No streak right now. Best: 2 days, 3 rounds in all.");
    }
}
