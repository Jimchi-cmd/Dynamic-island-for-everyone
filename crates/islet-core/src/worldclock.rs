//! World clocks (Islet Pro): the time in a few other cities, on the clock
//! card. A small built-in table of cities with their UTC offsets and
//! daylight-saving rules, so it works the same on every OS and offline.

use crate::alarm::days_from_civil;
use crate::calendar::civil_from_days;
use crate::clock::LocalTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dst {
    None,
    /// USA and Canada: second Sunday of March to first Sunday of November, 02:00 local.
    Us,
    /// European Union and UK: last Sunday of March to last Sunday of October, 01:00 UTC.
    Eu,
    /// South-eastern Australia: first Sunday of October to first Sunday of April, 02:00 standard.
    AuSouth,
    /// New Zealand: last Sunday of September to first Sunday of April, 02:00 standard.
    Nz,
}

/// (name, standard offset from UTC in minutes, daylight-saving rule).
pub const CITIES: &[(&str, i32, Dst)] = &[
    ("Accra", 0, Dst::None),
    ("Addis Ababa", 180, Dst::None),
    ("Adelaide", 570, Dst::AuSouth),
    ("Amsterdam", 60, Dst::Eu),
    ("Anchorage", -540, Dst::Us),
    ("Athens", 120, Dst::Eu),
    ("Auckland", 720, Dst::Nz),
    ("Bangkok", 420, Dst::None),
    ("Beijing", 480, Dst::None),
    ("Berlin", 60, Dst::Eu),
    ("Bogotá", -300, Dst::None),
    ("Brisbane", 600, Dst::None),
    ("Buenos Aires", -180, Dst::None),
    ("Chicago", -360, Dst::Us),
    ("Dar es Salaam", 180, Dst::None),
    ("Denver", -420, Dst::Us),
    ("Dhaka", 360, Dst::None),
    ("Dubai", 240, Dst::None),
    ("Dublin", 0, Dst::Eu),
    ("Helsinki", 120, Dst::Eu),
    ("Hong Kong", 480, Dst::None),
    ("Honolulu", -600, Dst::None),
    ("Istanbul", 180, Dst::None),
    ("Jakarta", 420, Dst::None),
    ("Johannesburg", 120, Dst::None),
    ("Kampala", 180, Dst::None),
    ("Karachi", 300, Dst::None),
    ("Kigali", 120, Dst::None),
    ("Kyiv", 120, Dst::Eu),
    ("Lagos", 60, Dst::None),
    ("Lima", -300, Dst::None),
    ("Lisbon", 0, Dst::Eu),
    ("London", 0, Dst::Eu),
    ("Los Angeles", -480, Dst::Us),
    ("Madrid", 60, Dst::Eu),
    ("Manila", 480, Dst::None),
    ("Melbourne", 600, Dst::AuSouth),
    ("Mexico City", -360, Dst::None),
    ("Moscow", 180, Dst::None),
    ("Mumbai", 330, Dst::None),
    ("Nairobi", 180, Dst::None),
    ("New Delhi", 330, Dst::None),
    ("New York", -300, Dst::Us),
    ("Paris", 60, Dst::Eu),
    ("Perth", 480, Dst::None),
    ("Phoenix", -420, Dst::None),
    ("Reykjavik", 0, Dst::None),
    ("Riyadh", 180, Dst::None),
    ("Rome", 60, Dst::Eu),
    ("São Paulo", -180, Dst::None),
    ("Seoul", 540, Dst::None),
    ("Singapore", 480, Dst::None),
    ("Stockholm", 60, Dst::Eu),
    ("Sydney", 600, Dst::AuSouth),
    ("Tokyo", 540, Dst::None),
    ("Toronto", -300, Dst::Us),
    ("Vancouver", -480, Dst::Us),
    ("Warsaw", 60, Dst::Eu),
];

/// At most this many world clocks on the card.
pub const MAX: usize = 3;

pub fn find(name: &str) -> Option<(&'static str, i32, Dst)> {
    CITIES.iter().copied().find(|(n, _, _)| n.eq_ignore_ascii_case(name.trim()))
}

/// Unix time of 00:00 UTC on a date.
fn day_start(y: i32, m: u8, d: u8) -> i64 {
    days_from_civil(y, m, d) * 86400
}

/// Day of the month of the `n`th (1-based) Sunday, or the last one (`n` = 0).
fn sunday(y: i32, m: u8, n: u8) -> u8 {
    let weekday = |d: u8| (days_from_civil(y, m, d) + 4).rem_euclid(7) as u8; // 0 = Sunday
    if n == 0 {
        let last = match m {
            4 | 6 | 9 | 11 => 30,
            2 => 28,
            _ => 31,
        };
        last - weekday(last)
    } else {
        let first = 1 + (7 - weekday(1)) % 7;
        first + 7 * (n - 1)
    }
}

/// Is daylight-saving time on at Unix time `t` for a city on standard offset `std` (minutes)?
pub fn dst_on(rule: Dst, std: i32, t: i64) -> bool {
    let std = i64::from(std) * 60;
    let year = civil_from_days((t + std).div_euclid(86400)).0;
    // A switch at `hour` o'clock local standard time on day `d` of month `m`.
    let at = |m: u8, d: u8, hour: i64| day_start(year, m, d) + hour * 3600 - std;
    match rule {
        Dst::None => false,
        Dst::Us => t >= at(3, sunday(year, 3, 2), 2) && t < at(11, sunday(year, 11, 1), 1),
        Dst::Eu => {
            t >= day_start(year, 3, sunday(year, 3, 0)) + 3600 && t < day_start(year, 10, sunday(year, 10, 0)) + 3600
        }
        Dst::AuSouth => t >= at(10, sunday(year, 10, 1), 2) || t < at(4, sunday(year, 4, 1), 2),
        Dst::Nz => t >= at(9, sunday(year, 9, 0), 2) || t < at(4, sunday(year, 4, 1), 2),
    }
}

/// The city's offset from UTC (minutes) at Unix time `t`.
pub fn offset(std: i32, rule: Dst, t: i64) -> i32 {
    std + if dst_on(rule, std, t) { 60 } else { 0 }
}

/// Local time at a UTC offset (minutes).
pub fn local_at(t: i64, offset_min: i32) -> LocalTime {
    let local = t + i64::from(offset_min) * 60;
    let days = local.div_euclid(86400);
    let secs = local.rem_euclid(86400);
    let (year, month, day) = civil_from_days(days);
    LocalTime {
        year,
        month,
        day,
        weekday: (days + 4).rem_euclid(7) as u8,
        hour: (secs / 3600) as u8,
        minute: (secs / 60 % 60) as u8,
        second: (secs % 60) as u8,
    }
}

/// A city's time now, and how many days it is ahead of (+) or behind (−) here.
pub fn city_time(name: &str, t: i64, here_offset_min: i32) -> Option<(LocalTime, i32)> {
    let (_, std, rule) = find(name)?;
    let there = local_at(t, offset(std, rule, t));
    let here = local_at(t, here_offset_min);
    let day = |l: &LocalTime| days_from_civil(l.year, l.month, l.day);
    Some((there, (day(&there) - day(&here)) as i32))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utc(y: i32, m: u8, d: u8, h: i64, min: i64) -> i64 {
        day_start(y, m, d) + h * 3600 + min * 60
    }

    #[test]
    fn finds_sundays() {
        assert_eq!(sunday(2026, 3, 2), 8); // US start 2026
        assert_eq!(sunday(2026, 11, 1), 1);
        assert_eq!(sunday(2026, 3, 0), 29); // EU start 2026
        assert_eq!(sunday(2026, 10, 0), 25);
        assert_eq!(sunday(2026, 9, 0), 27); // NZ start 2026
        assert_eq!(sunday(2026, 4, 1), 5);
    }

    #[test]
    fn us_rules_switch_at_two_local() {
        // 2026: 8 March 07:00 UTC (02:00 EST) to 1 November 06:00 UTC (02:00 EDT).
        assert!(!dst_on(Dst::Us, -300, utc(2026, 3, 8, 6, 59)));
        assert!(dst_on(Dst::Us, -300, utc(2026, 3, 8, 7, 0)));
        assert!(dst_on(Dst::Us, -300, utc(2026, 11, 1, 5, 59)));
        assert!(!dst_on(Dst::Us, -300, utc(2026, 11, 1, 6, 0)));
    }

    #[test]
    fn eu_rules_switch_at_one_utc() {
        assert!(!dst_on(Dst::Eu, 60, utc(2026, 3, 29, 0, 59)));
        assert!(dst_on(Dst::Eu, 60, utc(2026, 3, 29, 1, 0)));
        assert!(dst_on(Dst::Eu, 0, utc(2026, 10, 25, 0, 59)));
        assert!(!dst_on(Dst::Eu, 0, utc(2026, 10, 25, 1, 0)));
    }

    #[test]
    fn southern_summers_span_new_year() {
        // Sydney: 5 April 2026 03:00 AEDT ends it; 4 October 2026 02:00 AEST starts it.
        assert!(dst_on(Dst::AuSouth, 600, utc(2026, 1, 15, 0, 0)));
        assert!(!dst_on(Dst::AuSouth, 600, utc(2026, 6, 15, 0, 0)));
        assert!(dst_on(Dst::AuSouth, 600, utc(2026, 4, 4, 15, 59)));
        assert!(!dst_on(Dst::AuSouth, 600, utc(2026, 4, 4, 16, 0)));
        assert!(dst_on(Dst::AuSouth, 600, utc(2026, 10, 3, 16, 0)));
        assert!(dst_on(Dst::Nz, 720, utc(2026, 12, 25, 0, 0)));
        assert!(!dst_on(Dst::Nz, 720, utc(2026, 7, 1, 0, 0)));
    }

    #[test]
    fn tells_the_time_elsewhere() {
        let t = utc(2026, 11, 10, 22, 30); // 01:30 the next day in Kampala
        let (tokyo, ahead) = city_time("tokyo", t, 180).unwrap();
        assert_eq!((tokyo.hour, tokyo.minute, tokyo.day), (7, 30, 11));
        assert_eq!(ahead, 0, "Tokyo and Kampala are both on the 11th");
        let (la, behind) = city_time("Los Angeles", t, 180).unwrap();
        assert_eq!((la.hour, la.minute, la.day, behind), (14, 30, 10, -1));
        let (london, _) = city_time("London", utc(2026, 7, 1, 12, 0), 0).unwrap();
        assert_eq!(london.hour, 13, "BST in summer");
        assert!(city_time("Atlantis", t, 0).is_none());
    }

    #[test]
    fn cities_are_sorted_and_unique() {
        let names: Vec<&str> = CITIES.iter().map(|c| c.0).collect();
        let mut sorted = names.clone();
        // Alphabetical, reading "á" and "ã" as "a".
        sorted.sort_by_key(|n| n.to_lowercase().replace(['á', 'ã'], "a"));
        sorted.dedup();
        assert_eq!(names, sorted);
    }
}
