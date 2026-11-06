//! Alarms: when they are due, ringing, snoozing and stopping.
//!
//! Pure logic driven by the wall clock (`LocalTime`), so it is unit-tested.
//! Time is counted in "minute keys" (minutes since 1970-01-01 local), which
//! makes catching up after the computer slept a simple range check.

use crate::clock::LocalTime;
use serde::{Deserialize, Serialize};

/// Snooze length, as on most phones.
pub const SNOOZE_MINUTES: i64 = 9;
/// A ringing alarm nobody answers stops by itself after this long.
pub const RING_MINUTES: i64 = 10;
/// After sleep, alarms due within this many minutes still ring; older ones
/// are skipped rather than going off hours late.
pub const CATCH_UP_MINUTES: i64 = 10;

/// Days of the week as bits: bit 0 = Sunday … bit 6 = Saturday. 0 = once.
pub const DAILY: u8 = 0b111_1111;
pub const WEEKDAYS: u8 = 0b011_1110;
pub const WEEKENDS: u8 = 0b100_0001;
const DAY_NAMES: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Alarm {
    pub id: u32,
    pub hour: u8,
    pub minute: u8,
    #[serde(default)]
    pub label: String,
    /// Repeat days (see [`DAILY`]); 0 rings once and then turns itself off.
    #[serde(default)]
    pub days: u8,
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn yes() -> bool {
    true
}

impl Alarm {
    pub fn title(&self) -> &str {
        if self.label.trim().is_empty() {
            "Alarm"
        } else {
            &self.label
        }
    }

    fn rings_at(&self, key: i64) -> bool {
        let (weekday, hour, minute) = split_key(key);
        self.enabled
            && self.hour == hour
            && self.minute == minute
            && (self.days == 0 || self.days & (1 << weekday) != 0)
    }
}

/// Days since 1970-01-01 for a civil date (proleptic Gregorian).
pub fn days_from_civil(y: i32, m: u8, d: u8) -> i64 {
    let y = i64::from(y) - i64::from(m <= 2);
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = i64::from(m);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

pub fn minute_key(t: &LocalTime) -> i64 {
    days_from_civil(t.year, t.month, t.day) * 1440 + i64::from(t.hour) * 60 + i64::from(t.minute)
}

/// (weekday 0 = Sunday, hour, minute) of a minute key.
fn split_key(key: i64) -> (u8, u8, u8) {
    let days = key.div_euclid(1440);
    let mins = key.rem_euclid(1440);
    // 1970-01-01 was a Thursday.
    ((days + 4).rem_euclid(7) as u8, (mins / 60) as u8, (mins % 60) as u8)
}

/// "7:30", "07:30", "19:30", "7:30pm", "7 pm", "7am".
pub fn parse_time(s: &str) -> Option<(u8, u8)> {
    let s = s.trim().to_ascii_lowercase().replace(' ', "");
    let (body, pm) = match (s.strip_suffix("am"), s.strip_suffix("pm")) {
        (Some(b), _) => (b.to_string(), Some(false)),
        (_, Some(b)) => (b.to_string(), Some(true)),
        _ => (s.clone(), None),
    };
    let (h, m) = match body.split_once([':', '.', 'h']) {
        Some((h, m)) => (h.parse::<u8>().ok()?, if m.is_empty() { 0 } else { m.parse::<u8>().ok()? }),
        None => (body.parse::<u8>().ok()?, 0),
    };
    if m > 59 {
        return None;
    }
    let h = match pm {
        None if h <= 23 => h,
        Some(pm) if (1..=12).contains(&h) => (h % 12) + if pm { 12 } else { 0 },
        _ => return None,
    };
    Some((h, m))
}

/// "once", "daily", "weekdays", "weekends", or a list such as "mon,wed,fri".
pub fn parse_days(s: &str) -> Option<u8> {
    let s = s.trim().to_ascii_lowercase();
    match s.as_str() {
        "" | "once" | "never" => Some(0),
        "daily" | "everyday" | "every-day" | "all" => Some(DAILY),
        "weekdays" => Some(WEEKDAYS),
        "weekends" => Some(WEEKENDS),
        list => list.split([',', ' ']).filter(|d| !d.is_empty()).try_fold(0u8, |acc, d| {
            let i = DAY_NAMES
                .iter()
                .position(|n| d.len() >= 2 && n.to_ascii_lowercase().starts_with(&d[..d.len().min(3)]))?;
            Some(acc | 1 << i)
        }),
    }
}

/// "Once", "Every day", "Weekdays", "Weekends" or "Mon, Wed, Fri".
pub fn describe_days(days: u8) -> String {
    match days & DAILY {
        0 => "Once".into(),
        DAILY => "Every day".into(),
        WEEKDAYS => "Weekdays".into(),
        WEEKENDS => "Weekends".into(),
        d => (0..7).filter(|i| d & (1 << i) != 0).map(|i| DAY_NAMES[i]).collect::<Vec<_>>().join(", "),
    }
}

/// "07:30" or "7:30 AM".
pub fn format_hm(hour: u8, minute: u8, h24: bool) -> String {
    if h24 {
        format!("{hour:02}:{minute:02}")
    } else {
        let h = if hour % 12 == 0 { 12 } else { hour % 12 };
        format!("{h}:{minute:02} {}", if hour < 12 { "AM" } else { "PM" })
    }
}

/// What changed on a [`AlarmClock::tick`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AlarmEvent {
    /// Start ringing (and show the bell).
    Ring(Alarm),
    /// Ringing ended without an answer.
    TimedOut,
    /// A one-time alarm rang; it should be saved as turned off.
    Finished(u32),
}

#[derive(Debug, Default)]
pub struct AlarmClock {
    alarms: Vec<Alarm>,
    /// The last minute checked.
    last: Option<i64>,
    ringing: Option<(Alarm, i64)>,
    snoozed: Option<(Alarm, i64)>,
}

impl AlarmClock {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_alarms(&mut self, alarms: Vec<Alarm>) {
        self.alarms = alarms;
    }

    pub fn alarms(&self) -> &[Alarm] {
        &self.alarms
    }

    pub fn ringing(&self) -> Option<&Alarm> {
        self.ringing.as_ref().map(|(a, _)| a)
    }

    pub fn snoozed(&self) -> Option<&Alarm> {
        self.snoozed.as_ref().map(|(a, _)| a)
    }

    /// Check the clock. Call often (every loop); it only acts when the
    /// minute changes.
    pub fn tick(&mut self, now: &LocalTime) -> Vec<AlarmEvent> {
        let key = minute_key(now);
        let mut events = Vec::new();
        let last = match self.last {
            // On startup, the current minute still counts.
            None => key - 1,
            // The clock was set back: start over from now.
            Some(l) if l > key => key,
            Some(l) => l,
        };
        self.last = Some(key);
        if key == last {
            return events;
        }
        if let Some((_, since)) = &self.ringing {
            if key - since >= RING_MINUTES {
                self.ringing = None;
                events.push(AlarmEvent::TimedOut);
            }
        }
        for k in (last + 1).max(key - CATCH_UP_MINUTES + 1)..=key {
            let due = match &self.snoozed {
                Some((a, at)) if *at == k => Some((a.clone(), true)),
                _ => self.alarms.iter().find(|a| a.rings_at(k)).cloned().map(|a| (a, false)),
            };
            let Some((alarm, snoozed)) = due else { continue };
            if snoozed {
                self.snoozed = None;
            } else if alarm.days == 0 {
                // A one-time alarm turns itself off once it has rung.
                if let Some(a) = self.alarms.iter_mut().find(|a| a.id == alarm.id) {
                    a.enabled = false;
                }
                events.push(AlarmEvent::Finished(alarm.id));
            }
            if self.ringing.is_none() {
                self.ringing = Some((alarm.clone(), key));
                events.push(AlarmEvent::Ring(alarm));
            }
        }
        // A snooze that was skipped over (long sleep) is dropped.
        if self.snoozed.as_ref().is_some_and(|(_, at)| *at <= key - CATCH_UP_MINUTES) {
            self.snoozed = None;
        }
        events
    }

    /// Stop the ringing alarm (and forget any snooze).
    pub fn stop(&mut self) -> bool {
        let was = self.ringing.is_some() || self.snoozed.is_some();
        self.ringing = None;
        self.snoozed = None;
        was
    }

    /// Ring again in [`SNOOZE_MINUTES`].
    pub fn snooze(&mut self, now: &LocalTime) -> bool {
        match self.ringing.take() {
            Some((a, _)) => {
                self.snoozed = Some((a, minute_key(now) + SNOOZE_MINUTES));
                true
            }
            None => false,
        }
    }

    /// Seconds until the clock needs checking again: at the next minute while
    /// any alarm is on, snoozed or ringing; None when nothing is set (zero
    /// wakeups). Checking every minute, rather than sleeping until the alarm,
    /// keeps alarms on time after the computer wakes from sleep.
    pub fn next_check(&self, now: &LocalTime) -> Option<f64> {
        let active = self.ringing.is_some() || self.snoozed.is_some() || self.alarms.iter().any(|a| a.enabled);
        active.then(|| 60.0 - f64::from(now.second.min(59)) + 0.05)
    }

    /// The next enabled alarm and minutes until it rings (within a week).
    pub fn upcoming(&self, now: &LocalTime) -> Option<(&Alarm, i64)> {
        let key = minute_key(now);
        (1..=7 * 1440).find_map(|d| self.alarms.iter().find(|a| a.rings_at(key + d)).map(|a| (a, d)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Wednesday 4 November 2026.
    fn at(day: u8, hour: u8, minute: u8) -> LocalTime {
        let weekday = ((3 + i32::from(day) - 4).rem_euclid(7)) as u8;
        LocalTime { year: 2026, month: 11, day, weekday, hour, minute, second: 0 }
    }

    fn alarm(id: u32, hour: u8, minute: u8, days: u8) -> Alarm {
        Alarm { id, hour, minute, label: String::new(), days, enabled: true }
    }

    #[test]
    fn minute_keys_know_the_weekday() {
        let t = at(4, 7, 30);
        assert_eq!(split_key(minute_key(&t)), (3, 7, 30), "4 Nov 2026 is a Wednesday");
        assert_eq!(minute_key(&at(5, 0, 0)) - minute_key(&at(4, 23, 59)), 1);
    }

    #[test]
    fn parses_times_and_days() {
        assert_eq!(parse_time("7:30"), Some((7, 30)));
        assert_eq!(parse_time("19:05"), Some((19, 5)));
        assert_eq!(parse_time("7:30 pm"), Some((19, 30)));
        assert_eq!(parse_time("12am"), Some((0, 0)));
        assert_eq!(parse_time("12:15pm"), Some((12, 15)));
        assert_eq!(parse_time("25:00"), None);
        assert_eq!(parse_time("13pm"), None);
        assert_eq!(parse_days("weekdays"), Some(WEEKDAYS));
        assert_eq!(parse_days("mon,wed,fri"), Some(0b010_1010));
        assert_eq!(parse_days("monday tuesday"), Some(0b000_0110));
        assert_eq!(parse_days("someday"), None);
        assert_eq!(describe_days(0b010_1010), "Mon, Wed, Fri");
        assert_eq!(describe_days(WEEKENDS), "Weekends");
        assert_eq!(format_hm(7, 5, false), "7:05 AM");
        assert_eq!(format_hm(0, 0, false), "12:00 AM");
    }

    #[test]
    fn rings_on_the_minute_and_once_alarms_turn_off() {
        let mut c = AlarmClock::new();
        c.set_alarms(vec![alarm(1, 7, 30, 0)]);
        assert!(c.tick(&at(4, 7, 29)).is_empty());
        assert_eq!(c.next_check(&at(4, 7, 29)), Some(60.05));
        let ev = c.tick(&at(4, 7, 30));
        assert_eq!(ev, vec![AlarmEvent::Finished(1), AlarmEvent::Ring(alarm(1, 7, 30, 0))]);
        assert!(c.tick(&at(4, 7, 30)).is_empty(), "once per minute");
        assert!(c.stop());
        assert!(c.tick(&at(5, 7, 30)).is_empty(), "a one-time alarm doesn't ring again");
        assert_eq!(c.next_check(&at(5, 7, 31)), None, "nothing set: no wakeups");
    }

    #[test]
    fn repeats_only_on_its_days() {
        let mut c = AlarmClock::new();
        c.set_alarms(vec![alarm(1, 6, 0, WEEKDAYS)]);
        c.tick(&at(7, 5, 59)); // Saturday
        assert!(c.tick(&at(7, 6, 0)).is_empty());
        c.tick(&at(9, 5, 59)); // Monday
        assert!(matches!(c.tick(&at(9, 6, 0))[..], [AlarmEvent::Ring(_)]));
        assert_eq!(c.upcoming(&at(9, 6, 1)).map(|(a, m)| (a.id, m)), Some((1, 24 * 60 - 1)));
    }

    #[test]
    fn snoozes_and_times_out() {
        let mut c = AlarmClock::new();
        c.set_alarms(vec![alarm(1, 7, 0, DAILY)]);
        c.tick(&at(4, 6, 59));
        c.tick(&at(4, 7, 0));
        assert!(c.snooze(&at(4, 7, 1)));
        assert!(c.ringing().is_none());
        assert!(c.tick(&at(4, 7, 9)).is_empty());
        assert!(matches!(c.tick(&at(4, 7, 10))[..], [AlarmEvent::Ring(_)]), "rings again after 9 minutes");
        assert_eq!(c.tick(&at(4, 7, 20)), vec![AlarmEvent::TimedOut]);
        assert!(c.ringing().is_none());
    }

    #[test]
    fn a_snoozed_one_time_alarm_finishes_only_once() {
        let mut c = AlarmClock::new();
        c.set_alarms(vec![alarm(1, 7, 0, 0)]);
        c.tick(&at(4, 6, 59));
        assert_eq!(c.tick(&at(4, 7, 0)).len(), 2, "Finished + Ring");
        c.snooze(&at(4, 7, 0));
        assert!(matches!(c.tick(&at(4, 7, 9))[..], [AlarmEvent::Ring(_)]), "snooze rings, no second Finished");
    }

    #[test]
    fn catches_up_briefly_after_sleep_but_not_hours_late() {
        let mut c = AlarmClock::new();
        c.set_alarms(vec![alarm(1, 7, 0, DAILY)]);
        c.tick(&at(4, 6, 55));
        assert!(matches!(c.tick(&at(4, 7, 3))[..], [AlarmEvent::Ring(_)]), "slept through 7:00 by 3 minutes");
        c.stop();
        c.tick(&at(5, 1, 0));
        assert!(c.tick(&at(5, 9, 0)).is_empty(), "woke two hours after the alarm: skip it");
    }
}
