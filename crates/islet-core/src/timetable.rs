//! A weekly class timetable, entered once: the island then shows the time
//! to the next lesson and the free gaps between them. Kept in
//! timetable.json on this computer only.

use crate::calendar::CalEvent;
use serde::{Deserialize, Serialize};

/// The most lessons a timetable holds.
pub const MAX: usize = 80;
/// The longest lesson or room name, in characters.
pub const MAX_LEN: usize = 40;

pub const DAYS: [&str; 7] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];
pub const DAY_NAMES: [&str; 7] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lesson {
    /// 0 = Monday … 6 = Sunday.
    pub day: u8,
    /// Minutes after midnight.
    pub start: u16,
    pub end: u16,
    pub name: String,
    #[serde(default)]
    pub room: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Timetable {
    pub lessons: Vec<Lesson>,
}

/// "mon", "Monday", "MON" → 0.
pub fn parse_day(s: &str) -> Option<u8> {
    let s = s.trim().to_lowercase();
    DAYS.iter()
        .position(|d| s.len() >= 3 && s.starts_with(d) && DAY_NAMES.iter().any(|n| n.to_lowercase().starts_with(&s)))
        .map(|i| i as u8)
}

/// The weekday (0 = Monday) of a day counted from 1970-01-01, a Thursday.
pub fn weekday_of_day(day: i64) -> u8 {
    (day + 3).rem_euclid(7) as u8
}

fn tidy(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(MAX_LEN).collect()
}

/// What the timetable says about this moment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status<'a> {
    /// A lesson is on, with this many minutes left.
    Now { lesson: &'a Lesson, left: u16 },
    /// The next lesson today starts in this many minutes. `free` when an
    /// earlier lesson today has ended: this is a gap, not the morning.
    Next { lesson: &'a Lesson, starts_in: u16, free: bool },
}

impl Timetable {
    /// Add a lesson, keeping the week in order.
    pub fn add(&mut self, day: u8, start: u16, end: u16, name: &str, room: &str) -> Result<(), String> {
        let name = tidy(name);
        if name.is_empty() {
            return Err("Give the lesson a name.".into());
        }
        if day > 6 || start >= 24 * 60 || end > 24 * 60 {
            return Err("That isn't a time in the week.".into());
        }
        if end <= start {
            return Err("A lesson ends after it starts.".into());
        }
        if self.lessons.len() >= MAX {
            return Err(format!("A timetable holds up to {MAX} lessons. Remove one first."));
        }
        let lesson = Lesson { day, start, end, name, room: tidy(room) };
        if self.lessons.contains(&lesson) {
            return Err("That lesson is already there.".into());
        }
        self.lessons.push(lesson);
        self.lessons.sort_by_key(|l| (l.day, l.start, l.end));
        Ok(())
    }

    /// Remove the lesson at `index` (in week order). False if there is none.
    pub fn remove(&mut self, index: usize) -> bool {
        if index < self.lessons.len() {
            self.lessons.remove(index);
            true
        } else {
            false
        }
    }

    /// Where the day stands at `minute` after midnight on `weekday`.
    /// None once the day's lessons are over (or there are none).
    pub fn status(&self, weekday: u8, minute: u16) -> Option<Status<'_>> {
        let today = || self.lessons.iter().filter(move |l| l.day == weekday);
        if let Some(lesson) = today().find(|l| l.start <= minute && minute < l.end) {
            return Some(Status::Now { lesson, left: lesson.end - minute });
        }
        let lesson = today().filter(|l| l.start > minute).min_by_key(|l| l.start)?;
        let free = today().any(|l| l.end <= minute);
        Some(Status::Next { lesson, starts_in: lesson.start - minute, free })
    }

    /// One line for the clock card: "Maths · 20 min left",
    /// "Free 45 min · then Physics", "Maths in 25 min", "Maths at 14:00".
    pub fn line(&self, weekday: u8, minute: u16, h24: bool) -> Option<String> {
        Some(match self.status(weekday, minute)? {
            Status::Now { lesson, left } => format!("{} · {} left", lesson.name, mins(left)),
            Status::Next { lesson, starts_in, free: true } => {
                format!("Free {} · then {}", mins(starts_in), lesson.name)
            }
            Status::Next { lesson, starts_in, .. } if starts_in <= 90 => {
                format!("{} in {}", lesson.name, mins(starts_in))
            }
            Status::Next { lesson, .. } => {
                let at = crate::alarm::format_hm((lesson.start / 60) as u8, (lesson.start % 60) as u8, h24);
                format!("{} at {at}", lesson.name)
            }
        })
    }

    /// The lessons from `first_day` (days since 1970, local) for `days`
    /// days, as calendar events, so "up next" and the agenda show them.
    pub fn events(&self, first_day: i64, days: u32, utc_offset: i64) -> Vec<CalEvent> {
        let mut out = Vec::new();
        for day in first_day..first_day + i64::from(days) {
            let weekday = weekday_of_day(day);
            for l in self.lessons.iter().filter(|l| l.day == weekday) {
                let at = |m: u16| day * 86_400 + i64::from(m) * 60 - utc_offset;
                out.push(CalEvent {
                    summary: l.name.clone(),
                    start: at(l.start),
                    end: at(l.end),
                    all_day: false,
                    location: Some(l.room.clone()).filter(|r| !r.is_empty()),
                    link: None,
                });
            }
        }
        out.sort_by_key(|e| (e.start, e.end));
        out
    }
}

/// "45 min", "1 h", "1 h 30".
fn mins(m: u16) -> String {
    match (m / 60, m % 60) {
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m:02}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn week() -> Timetable {
        let mut t = Timetable::default();
        t.add(0, 11 * 60, 12 * 60, "Physics", "Lab 2").unwrap();
        t.add(0, 9 * 60, 10 * 60, "  Maths ", "Room 4").unwrap();
        t.add(0, 14 * 60, 15 * 60 + 30, "History", "").unwrap();
        t.add(2, 8 * 60, 9 * 60, "Art", "").unwrap();
        t
    }

    #[test]
    fn keeps_the_week_in_order_and_checks_what_it_is_given() {
        let mut t = week();
        let names: Vec<&str> = t.lessons.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["Maths", "Physics", "History", "Art"]);
        assert!(t.add(0, 600, 540, "Backwards", "").is_err());
        assert!(t.add(7, 600, 660, "No such day", "").is_err());
        assert!(t.add(0, 600, 660, "   ", "").is_err());
        assert!(t.add(0, 9 * 60, 10 * 60, "Maths", "Room 4").is_err(), "twice is once too many");
        assert!(t.remove(3) && !t.remove(3));
        assert_eq!(t.lessons.len(), 3);
    }

    #[test]
    fn says_where_the_day_stands() {
        let t = week();
        // Monday 08:35: Maths in 25 min.
        assert_eq!(t.line(0, 8 * 60 + 35, true).as_deref(), Some("Maths in 25 min"));
        // Early morning: too far for a countdown, so the time.
        assert_eq!(t.line(0, 6 * 60, true).as_deref(), Some("Maths at 09:00"));
        assert_eq!(t.line(0, 6 * 60, false).as_deref(), Some("Maths at 9:00 AM"));
        // In Maths.
        assert_eq!(t.line(0, 9 * 60 + 40, true).as_deref(), Some("Maths · 20 min left"));
        // The gap between Maths and Physics, and the long one before History.
        assert_eq!(t.line(0, 10 * 60 + 15, true).as_deref(), Some("Free 45 min · then Physics"));
        assert_eq!(t.line(0, 12 * 60 + 30, true).as_deref(), Some("Free 1 h 30 · then History"));
        assert_eq!(t.line(0, 13 * 60, true).as_deref(), Some("Free 1 h · then History"));
        // After the last lesson, and on a day without any.
        assert_eq!(t.line(0, 16 * 60, true), None);
        assert_eq!(t.line(1, 9 * 60, true), None);
        assert!(matches!(t.status(2, 8 * 60 + 59), Some(Status::Now { left: 1, .. })));
        assert_eq!(t.status(2, 9 * 60), None, "a lesson is over at its end time");
    }

    #[test]
    fn days_and_weekdays() {
        assert_eq!(parse_day("mon"), Some(0));
        assert_eq!(parse_day("Wednesday"), Some(2));
        assert_eq!(parse_day("SUN"), Some(6));
        assert_eq!(parse_day("thurs"), Some(3));
        assert_eq!(parse_day("mo"), None);
        assert_eq!(parse_day("monkey"), None);
        assert_eq!(weekday_of_day(0), 3, "1 January 1970 was a Thursday");
        assert_eq!(weekday_of_day(4), 0);
        assert_eq!(weekday_of_day(-1), 2);
    }

    #[test]
    fn lessons_become_events_on_the_local_clock() {
        let t = week();
        // Day 4 (5 January 1970) is a Monday; at UTC+3, 09:00 local is 06:00 UTC.
        let ev = t.events(4, 7, 3 * 3600);
        assert_eq!(ev.len(), 4);
        assert_eq!(ev[0].summary, "Maths");
        assert_eq!(ev[0].start, 4 * 86_400 + 6 * 3600);
        assert_eq!(ev[0].end - ev[0].start, 3600);
        assert_eq!(ev[0].location.as_deref(), Some("Room 4"));
        assert_eq!(ev[2].location, None, "no room: no place line");
        assert_eq!(ev[3].summary, "Art");
        assert_eq!(ev[3].start, 6 * 86_400 + 5 * 3600);
        // Two weeks hold everything twice.
        assert_eq!(t.events(4, 14, 0).len(), 8);
    }
}
