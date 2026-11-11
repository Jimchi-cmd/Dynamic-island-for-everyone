//! Wall-clock formatting for the clock activity. The OS supplies the local
//! time (crates/islet/src/clock.rs); everything here is pure and tested.
//! Strings are English for now and move to the i18n catalogue in phase 2.

/// Broken-down local time, as reported by the OS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalTime {
    pub year: i32,
    /// 1..=12
    pub month: u8,
    /// 1..=31
    pub day: u8,
    /// 0 = Sunday .. 6 = Saturday
    pub weekday: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

const WEEKDAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// "14:05" (24-hour) or "2:05" + "PM" (12-hour, returned separately so the
/// suffix can be drawn smaller, as iOS does).
pub fn format_time(t: &LocalTime, h24: bool) -> (String, Option<&'static str>) {
    if h24 {
        (format!("{:02}:{:02}", t.hour, t.minute), None)
    } else {
        let h = match t.hour % 12 {
            0 => 12,
            h => h,
        };
        (format!("{}:{:02}", h, t.minute), Some(if t.hour < 12 { "AM" } else { "PM" }))
    }
}

/// "November" for 11 ("" out of range).
pub fn month_name(month: u8) -> &'static str {
    month.checked_sub(1).and_then(|i| MONTHS.get(i as usize)).copied().unwrap_or("")
}

/// "Wed 4 Nov": the date when the line has to share its space.
pub fn format_date_short(t: &LocalTime) -> String {
    let wd = WEEKDAYS.get(t.weekday as usize).map_or("", |w| &w[..3]);
    let mo = month_name(t.month);
    format!("{wd} {} {}", t.day, &mo[..mo.len().min(3)])
}

/// "Wednesday, 4 November"
pub fn format_date(t: &LocalTime) -> String {
    let wd = WEEKDAYS.get(t.weekday as usize).copied().unwrap_or("");
    let mo = t.month.checked_sub(1).and_then(|i| MONTHS.get(i as usize)).copied().unwrap_or("");
    format!("{wd}, {} {mo}", t.day)
}

/// Default clock style for a BCP 47 locale such as "en-US": 12-hour where
/// that is the everyday convention, 24-hour elsewhere. A setting overrides it.
pub fn locale_prefers_24h(locale: &str) -> bool {
    const TWELVE_HOUR: [&str; 12] =
        ["en-US", "en-CA", "en-AU", "en-NZ", "en-PH", "en-IN", "en-UG", "en-KE", "en-GH", "en-NG", "es-MX", "ar-EG"];
    let norm = locale.replace('_', "-");
    let base = norm.split('.').next().unwrap_or("");
    !TWELVE_HOUR.iter().any(|l| base.eq_ignore_ascii_case(l))
}

/// Seconds until the minute changes (so the clock redraws exactly then and
/// sleeps in between).
pub fn secs_to_next_minute(t: &LocalTime) -> f64 {
    (60 - t.second.min(59) as u32) as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: LocalTime = LocalTime { year: 2026, month: 11, day: 4, weekday: 3, hour: 14, minute: 5, second: 42 };

    #[test]
    fn formats_24_and_12_hour() {
        assert_eq!(format_time(&T, true), ("14:05".to_string(), None));
        assert_eq!(format_time(&T, false), ("2:05".to_string(), Some("PM")));
        let midnight = LocalTime { hour: 0, minute: 7, ..T };
        assert_eq!(format_time(&midnight, false), ("12:07".to_string(), Some("AM")));
        let noon = LocalTime { hour: 12, minute: 0, ..T };
        assert_eq!(format_time(&noon, false), ("12:00".to_string(), Some("PM")));
    }

    #[test]
    fn formats_date() {
        assert_eq!(format_date(&T), "Wednesday, 4 November");
        let bad = LocalTime { weekday: 9, month: 0, ..T };
        assert_eq!(format_date(&bad), ", 4 ", "out-of-range fields never panic");
    }

    #[test]
    fn locale_defaults() {
        assert!(!locale_prefers_24h("en-US"));
        assert!(!locale_prefers_24h("en_UG.UTF-8"));
        assert!(locale_prefers_24h("en-GB"));
        assert!(locale_prefers_24h("de-DE"));
        assert!(locale_prefers_24h(""));
    }

    #[test]
    fn next_minute() {
        assert_eq!(secs_to_next_minute(&T), 18.0);
        assert_eq!(secs_to_next_minute(&LocalTime { second: 0, ..T }), 60.0);
        assert_eq!(secs_to_next_minute(&LocalTime { second: 60, ..T }), 1.0, "leap second");
    }
}
