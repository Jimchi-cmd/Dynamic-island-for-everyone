//! Calendar events from an iCalendar (ICS) feed: the private "iCal link"
//! that Google Calendar, Outlook and iCloud offer. Parsing, time zones
//! (from the feed's own VTIMEZONE blocks) and recurrence expansion are here;
//! the island fetches the feed every 15 minutes while calendar is on.
//!
//! Times are Unix seconds (UTC). Supported recurrence: FREQ=DAILY/WEEKLY/
//! MONTHLY/YEARLY with INTERVAL, COUNT, UNTIL, BYDAY (weekly), and EXDATE.

use crate::alarm::days_from_civil;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct CalEvent {
    pub summary: String,
    pub start: i64,
    pub end: i64,
    pub all_day: bool,
    pub location: Option<String>,
    /// Where to join, when the event carries a video-call link.
    pub link: Option<String>,
}

/// Video-call services whose links get a Join button. Only these are
/// offered, so a stray link in an invitation is never opened by a tap.
const MEETING_HOSTS: [&str; 14] = [
    "zoom.us",
    "zoom.com",
    "meet.google.com",
    "teams.microsoft.com",
    "teams.live.com",
    "webex.com",
    "whereby.com",
    "meet.jit.si",
    "gotomeeting.com",
    "gotomeet.me",
    "bluejeans.com",
    "chime.aws",
    "join.skype.com",
    "discord.gg",
];

/// The first video-call link in a piece of an event (its location, notes…).
pub fn meeting_link(text: &str) -> Option<String> {
    let mut rest = text;
    while let Some(at) = rest.find("https://") {
        let from = &rest[at..];
        let end = from
            .find(|c: char| {
                c.is_whitespace() || matches!(c, '"' | '<' | '>' | '\\' | '\'' | '|' | '{' | '}' | '^' | '`')
            })
            .unwrap_or(from.len());
        let url = from[..end].trim_end_matches(['.', ',', ';', ':', ')', ']', '!', '?']);
        rest = &from[end.min(from.len()).max(8)..];
        let authority = url["https://".len()..].split(['/', '?', '#']).next().unwrap_or("");
        // "https://zoom.us@elsewhere.example/" goes elsewhere: no user part, no port tricks.
        if authority.is_empty() || authority.contains('@') || url.len() > 600 {
            continue;
        }
        let host = authority.split(':').next().unwrap_or("").to_ascii_lowercase();
        if MEETING_HOSTS.iter().any(|h| host == *h || host.ends_with(&format!(".{h}"))) {
            return Some(url.to_string());
        }
    }
    None
}

/// Refresh the feed this often while calendar is on.
pub const REFRESH_SECS: u64 = 15 * 60;
/// Show "up next" on the island this long before an event starts…
pub const SOON_SECS: i64 = 10 * 60;
/// …and until this long after.
pub const STARTED_SECS: i64 = 5 * 60;

/// (year, month, day) for days since 1970-01-01.
pub fn civil_from_days(z: i64) -> (i32, u8, u8) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u8;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u8;
    ((yoe + era * 400 + i64::from(m <= 2)) as i32, m, d)
}

/// A wall-clock time with no zone: seconds since 1970-01-01T00:00 "local".
type Naive = i64;

fn naive(y: i32, m: u8, d: u8, hh: i64, mm: i64, ss: i64) -> Naive {
    days_from_civil(y, m, d) * 86_400 + hh * 3600 + mm * 60 + ss
}

/// 0 = Sunday.
fn weekday(days: i64) -> i64 {
    (days + 4).rem_euclid(7)
}

/// "20261104T093000" / "20261104T093000Z" / "20261104" → (naive, utc?, date-only?)
fn parse_datetime(v: &str) -> Option<(Naive, bool, bool)> {
    let v = v.trim();
    let num = |a: usize, b: usize| v.get(a..b)?.parse::<i64>().ok();
    let (y, m, d) = (num(0, 4)? as i32, num(4, 6)? as u8, num(6, 8)? as u8);
    if v.len() == 8 {
        return Some((naive(y, m, d, 0, 0, 0), false, true));
    }
    if v.as_bytes().get(8) != Some(&b'T') {
        return None;
    }
    let (hh, mm, ss) = (num(9, 11)?, num(11, 13)?, num(13, 15).unwrap_or(0));
    Some((naive(y, m, d, hh, mm, ss), v.ends_with('Z'), false))
}

/// "PT1H30M", "P1D", "-PT15M" → seconds.
fn parse_duration(v: &str) -> Option<i64> {
    let (neg, v) = match v.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, v.strip_prefix('+').unwrap_or(v)),
    };
    let v = v.strip_prefix('P')?;
    let (mut total, mut num, mut in_time) = (0i64, String::new(), false);
    for c in v.chars() {
        match c {
            'T' => in_time = true,
            '0'..='9' => num.push(c),
            _ => {
                let n: i64 = num.parse().ok()?;
                num.clear();
                total += n * match (c, in_time) {
                    ('W', _) => 604_800,
                    ('D', _) => 86_400,
                    ('H', true) => 3600,
                    ('M', true) => 60,
                    ('S', true) => 1,
                    _ => return None,
                };
            }
        }
    }
    Some(if neg { -total } else { total })
}

/// "+0300" / "-0530" → seconds.
fn parse_offset(v: &str) -> Option<i64> {
    let v = v.trim();
    let sign = match v.as_bytes().first()? {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    let hh: i64 = v.get(1..3)?.parse().ok()?;
    let mm: i64 = v.get(3..5)?.parse().ok()?;
    Some(sign * (hh * 3600 + mm * 60))
}

#[derive(Debug, Clone, Default)]
struct Rule {
    freq: String,
    interval: i64,
    count: Option<u32>,
    until: Option<(Naive, bool)>,
    /// Weekdays (0 = Sunday) with an optional ordinal ("-1SU" → (-1, 0)).
    byday: Vec<(i64, i64)>,
    bymonth: Vec<u8>,
}

fn parse_rule(v: &str) -> Rule {
    let mut r = Rule { interval: 1, ..Rule::default() };
    for part in v.split(';') {
        let Some((k, val)) = part.split_once('=') else { continue };
        match k {
            "FREQ" => r.freq = val.to_string(),
            "INTERVAL" => r.interval = val.parse().unwrap_or(1).max(1),
            "COUNT" => r.count = val.parse().ok(),
            "UNTIL" => r.until = parse_datetime(val).map(|(n, utc, _)| (n, utc)),
            "BYMONTH" => r.bymonth = val.split(',').filter_map(|m| m.parse().ok()).collect(),
            "BYDAY" => {
                for d in val.split(',') {
                    let (ord, name) = d.split_at(d.len().saturating_sub(2));
                    let day = ["SU", "MO", "TU", "WE", "TH", "FR", "SA"].iter().position(|n| *n == name);
                    if let Some(day) = day {
                        r.byday.push((ord.parse().unwrap_or(0), day as i64));
                    }
                }
            }
            _ => {}
        }
    }
    r
}

/// The nth (1-based, negative from the end) weekday of a month, as days.
fn nth_weekday(y: i32, m: u8, nth: i64, wd: i64) -> i64 {
    if nth >= 0 {
        let first = days_from_civil(y, m, 1);
        first + (wd - weekday(first)).rem_euclid(7) + 7 * (nth.max(1) - 1)
    } else {
        let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
        let last = days_from_civil(ny, nm, 1) - 1;
        last - (weekday(last) - wd).rem_euclid(7) + 7 * (nth + 1)
    }
}

/// A zone from a VTIMEZONE block: its STANDARD/DAYLIGHT observances.
#[derive(Debug, Clone, Default)]
struct Zone {
    /// (start, offset before, offset after, yearly rule)
    observances: Vec<(Naive, i64, i64, Option<Rule>)>,
}

impl Zone {
    /// UTC offset (seconds) in effect at a wall-clock time.
    fn offset_at(&self, t: Naive) -> Option<i64> {
        let (y, _, _) = civil_from_days(t.div_euclid(86_400));
        let mut best: Option<(Naive, i64)> = None;
        let mut earliest: Option<(Naive, i64)> = None;
        for (start, from, to, rule) in &self.observances {
            if earliest.is_none_or(|(s, _)| *start < s) {
                earliest = Some((*start, *from));
            }
            let mut transitions = vec![*start];
            if let Some(r) = rule.as_ref().filter(|r| r.freq == "YEARLY") {
                let tod = start.rem_euclid(86_400);
                for year in [y - 1, y, y + 1] {
                    let month =
                        r.bymonth.first().copied().unwrap_or_else(|| civil_from_days(start.div_euclid(86_400)).1);
                    if let Some((nth, wd)) = r.byday.first() {
                        transitions.push(nth_weekday(year, month, *nth, *wd) * 86_400 + tod);
                    }
                }
            }
            for tr in transitions.into_iter().filter(|tr| *tr >= *start && *tr <= t) {
                if best.is_none_or(|(b, _)| tr > b) {
                    best = Some((tr, *to));
                }
            }
        }
        best.or(earliest).map(|(_, off)| off)
    }
}

struct Raw {
    props: Vec<(String, HashMap<String, String>, String)>,
}

impl Raw {
    fn get(&self, name: &str) -> Option<(&HashMap<String, String>, &str)> {
        self.props.iter().find(|(n, _, _)| n == name).map(|(_, p, v)| (p, v.as_str()))
    }
}

/// Undo line folding and split "NAME;PARAM=x:VALUE".
fn lines(ics: &str) -> Vec<(String, HashMap<String, String>, String)> {
    let mut unfolded: Vec<String> = Vec::new();
    for line in ics.split('\n').map(|l| l.trim_end_matches('\r')) {
        match (line.strip_prefix(' ').or_else(|| line.strip_prefix('\t')), unfolded.last_mut()) {
            (Some(rest), Some(last)) => last.push_str(rest),
            _ => unfolded.push(line.to_string()),
        }
    }
    unfolded
        .into_iter()
        .filter_map(|l| {
            let colon = l.find(':')?;
            let (head, value) = (&l[..colon], &l[colon + 1..]);
            let mut parts = head.split(';');
            let name = parts.next()?.to_ascii_uppercase();
            let params = parts
                .filter_map(|p| {
                    p.split_once('=').map(|(k, v)| (k.to_ascii_uppercase(), v.trim_matches('"').to_string()))
                })
                .collect();
            Some((name, params, value.to_string()))
        })
        .collect()
}

fn unescape(v: &str) -> String {
    // RFC 5545 text escapes: \n, \N, \, \; and \\.
    let mut out = String::with_capacity(v.len());
    let mut chars = v.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n' | 'N') => out.push('\n'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

/// Events overlapping [from, to] (Unix seconds), recurrences expanded.
/// `local_offset` is this computer's UTC offset, used for floating times.
pub fn parse_ics(ics: &str, from: i64, to: i64, local_offset: i64) -> Vec<CalEvent> {
    let mut zones: HashMap<String, Zone> = HashMap::new();
    let mut events: Vec<Raw> = Vec::new();
    // Walk the component tree.
    let mut stack: Vec<String> = Vec::new();
    let mut cur_event: Option<Raw> = None;
    let mut cur_zone: Option<(String, Zone)> = None;
    let mut cur_obs: Option<Raw> = None;
    for (name, params, value) in lines(ics) {
        match name.as_str() {
            "BEGIN" => {
                let comp = value.trim().to_ascii_uppercase();
                match comp.as_str() {
                    "VEVENT" => cur_event = Some(Raw { props: Vec::new() }),
                    "VTIMEZONE" => cur_zone = Some((String::new(), Zone::default())),
                    "STANDARD" | "DAYLIGHT" => cur_obs = Some(Raw { props: Vec::new() }),
                    _ => {}
                }
                stack.push(comp);
            }
            "END" => match stack.pop().as_deref() {
                Some("VEVENT") => events.extend(cur_event.take()),
                Some("VTIMEZONE") => {
                    if let Some((id, z)) = cur_zone.take() {
                        zones.insert(id, z);
                    }
                }
                Some("STANDARD" | "DAYLIGHT") => {
                    if let (Some(obs), Some((_, z))) = (cur_obs.take(), cur_zone.as_mut()) {
                        let start = obs.get("DTSTART").and_then(|(_, v)| parse_datetime(v)).map(|(n, _, _)| n);
                        let from = obs.get("TZOFFSETFROM").and_then(|(_, v)| parse_offset(v));
                        let to = obs.get("TZOFFSETTO").and_then(|(_, v)| parse_offset(v));
                        if let (Some(s), Some(f), Some(t)) = (start, from, to) {
                            z.observances.push((s, f, t, obs.get("RRULE").map(|(_, v)| parse_rule(v))));
                        }
                    }
                }
                _ => {}
            },
            _ => {
                if let Some(obs) = cur_obs.as_mut() {
                    obs.props.push((name, params, value));
                } else if let Some(ev) = cur_event.as_mut() {
                    ev.props.push((name, params, value));
                } else if let Some((id, _)) = cur_zone.as_mut() {
                    if name == "TZID" {
                        *id = value.trim().to_string();
                    }
                }
            }
        }
    }

    // Wall-clock time in a zone → UTC.
    let to_utc = |n: Naive, utc: bool, tzid: Option<&String>| -> i64 {
        if utc {
            return n;
        }
        let off = tzid.and_then(|id| zones.get(id)).and_then(|z| z.offset_at(n)).unwrap_or(local_offset);
        n - off
    };

    let mut out = Vec::new();
    for ev in &events {
        if ev.get("STATUS").is_some_and(|(_, v)| v.eq_ignore_ascii_case("CANCELLED")) {
            continue;
        }
        let Some((sp, sv)) = ev.get("DTSTART") else { continue };
        let Some((start_n, start_utc, all_day)) = parse_datetime(sv) else { continue };
        let tzid = sp.get("TZID");
        let end_n = match ev.get("DTEND").and_then(|(_, v)| parse_datetime(v)) {
            Some((n, _, _)) => n,
            None => {
                start_n
                    + ev.get("DURATION").and_then(|(_, v)| parse_duration(v)).unwrap_or(if all_day {
                        86_400
                    } else {
                        0
                    })
            }
        };
        let length = (end_n - start_n).max(0);
        let exdates: Vec<Naive> = ev
            .props
            .iter()
            .filter(|(n, _, _)| n == "EXDATE")
            .flat_map(|(_, _, v)| v.split(',').filter_map(parse_datetime).map(|(n, _, _)| n).collect::<Vec<_>>())
            .collect();
        let summary = ev.get("SUMMARY").map(|(_, v)| unescape(v)).unwrap_or_else(|| "(No title)".into());
        let location = ev.get("LOCATION").map(|(_, v)| unescape(v)).filter(|l| !l.trim().is_empty());
        let link = ["X-GOOGLE-CONFERENCE", "URL", "LOCATION", "DESCRIPTION"]
            .iter()
            .find_map(|name| ev.get(name).and_then(|(_, v)| meeting_link(&unescape(v))));
        // All-day events are dates on the local calendar.
        let utc_of = |n: Naive| if all_day { n - local_offset } else { to_utc(n, start_utc, tzid) };
        let mut push = |n: Naive| {
            if exdates.contains(&n) {
                return;
            }
            let (s, e) = (utc_of(n), utc_of(n) + length);
            if e > from && s < to {
                out.push(CalEvent {
                    summary: summary.clone(),
                    start: s,
                    end: e,
                    all_day,
                    location: location.clone(),
                    link: link.clone(),
                });
            }
        };
        let Some(rule) = ev.get("RRULE").map(|(_, v)| parse_rule(v)) else {
            push(start_n);
            continue;
        };
        // Expand in wall-clock time so a 9:00 meeting stays at 9:00 across DST.
        let until = rule.until.map(|(n, utc)| if utc { n + local_offset } else { n });
        let horizon = to + local_offset + 86_400;
        let (tod, day0) = (start_n.rem_euclid(86_400), start_n.div_euclid(86_400));
        let mut emitted = 0u32;
        let mut step = 0i64;
        'expand: loop {
            let mut batch: Vec<Naive> = match rule.freq.as_str() {
                "DAILY" => vec![start_n + step * rule.interval * 86_400],
                "WEEKLY" => {
                    let week0 = day0 - weekday(day0) + step * rule.interval * 7;
                    if rule.byday.is_empty() {
                        vec![(week0 + weekday(day0)) * 86_400 + tod]
                    } else {
                        let mut v: Vec<Naive> = rule.byday.iter().map(|(_, wd)| (week0 + wd) * 86_400 + tod).collect();
                        v.sort();
                        v
                    }
                }
                "MONTHLY" | "YEARLY" => {
                    let (y0, m0, d0) = civil_from_days(day0);
                    let months = if rule.freq == "MONTHLY" { step * rule.interval } else { step * rule.interval * 12 };
                    let total = i64::from(m0) - 1 + months;
                    let (y, m) = (y0 + total.div_euclid(12) as i32, (total.rem_euclid(12) + 1) as u8);
                    // Skip months without that day (31st), as RFC 5545 does.
                    let (cy, cm, cd) = civil_from_days(days_from_civil(y, m, d0));
                    if (cy, cm, cd) == (y, m, d0) {
                        vec![days_from_civil(y, m, d0) * 86_400 + tod]
                    } else {
                        Vec::new()
                    }
                }
                _ => {
                    push(start_n);
                    break 'expand;
                }
            };
            batch.retain(|n| *n >= start_n);
            for n in batch.drain(..) {
                if until.is_some_and(|u| n > u) || rule.count.is_some_and(|c| emitted >= c) || n > horizon {
                    break 'expand;
                }
                emitted += 1;
                push(n);
            }
            step += 1;
            if step > 5000 {
                break;
            }
        }
    }
    out.sort_by_key(|e| (e.start, e.end));
    out
}

/// The event to show as "up next": starting within `SOON_SECS`, or begun
/// less than `STARTED_SECS` ago (timed events only).
pub fn up_next(events: &[CalEvent], now: i64) -> Option<&CalEvent> {
    events.iter().find(|e| !e.all_day && e.start - SOON_SECS <= now && now < e.start + STARTED_SECS && now < e.end)
}

/// The next moment `up_next` can change (for a precise wakeup).
pub fn next_change(events: &[CalEvent], now: i64) -> Option<i64> {
    events
        .iter()
        .filter(|e| !e.all_day)
        .flat_map(|e| [e.start - SOON_SECS, (e.start + STARTED_SECS).min(e.end)])
        .filter(|t| *t > now)
        .min()
}

/// "in 8 min", "now", "in 1 h 05".
pub fn relative(start: i64, now: i64) -> String {
    let mins = (start - now + 59).div_euclid(60);
    if mins <= 0 {
        "now".into()
    } else if mins < 60 {
        format!("in {mins} min")
    } else {
        format!("in {} h {:02}", mins / 60, mins % 60)
    }
}

/// Words that make an event a deadline: something due, not somewhere to be.
const DEADLINE_WORDS: [&str; 14] = [
    "due",
    "deadline",
    "exam",
    "exams",
    "test",
    "quiz",
    "assignment",
    "homework",
    "coursework",
    "submit",
    "submission",
    "midterm",
    "finals",
    "hand-in",
];

/// Is this event a deadline, going by its title? Whole words only, so
/// "Contest" and "Duet practice" are not.
pub fn is_deadline(summary: &str) -> bool {
    let lower = summary.to_lowercase();
    lower.contains("hand in")
        || lower.split(|c: char| !(c.is_alphanumeric() || c == '-')).any(|w| DEADLINE_WORDS.contains(&w))
}

/// How long until a deadline: "in 40 min", "in 5 h", "tomorrow", "in 3 days".
/// An all-day one counts whole days on the local calendar ("today").
pub fn countdown(e: &CalEvent, now: i64, utc_offset: i64) -> String {
    let today = (now + utc_offset).div_euclid(86_400);
    // All-day events are already dates on the local calendar.
    let day = if e.all_day { e.start.div_euclid(86_400) } else { (e.start + utc_offset).div_euclid(86_400) };
    let left = e.start - now;
    if !e.all_day && left <= 0 {
        return "now".into();
    }
    if !e.all_day && left < 3600 {
        return format!("in {} min", (left + 59) / 60);
    }
    if !e.all_day && left < 24 * 3600 && day - today <= 1 {
        return format!("in {} h", (left + 1800) / 3600);
    }
    match day - today {
        i64::MIN..=0 => "today".into(),
        1 => "tomorrow".into(),
        n => format!("in {n} days"),
    }
}

/// Days in a month (1–12).
pub fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        4 | 6 | 9 | 11 => 30,
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        _ => 31,
    }
}

/// Day of the week of a date: 0 = Sunday … 6 = Saturday.
pub fn weekday_of(year: i32, month: u8, day: u8) -> u8 {
    (crate::alarm::days_from_civil(year, month, day) + 4).rem_euclid(7) as u8
}

/// The month `delta` months from (year, month).
pub fn shift_month(year: i32, month: u8, delta: i32) -> (i32, u8) {
    let n = year * 12 + i32::from(month) - 1 + delta;
    (n.div_euclid(12), (n.rem_euclid(12) + 1) as u8)
}

/// Days of (year, month) on which an event falls, in local time
/// (`utc_offset` seconds). Multi-day events mark each of their days.
pub fn event_days(events: &[CalEvent], year: i32, month: u8, utc_offset: i64) -> Vec<u8> {
    let first = crate::alarm::days_from_civil(year, month, 1);
    let last = first + i64::from(days_in_month(year, month)) - 1;
    let mut days = Vec::new();
    for e in events {
        // All-day events are stored on UTC midnights; timed ones shift to local time.
        let off = if e.all_day { 0 } else { utc_offset };
        let from = (e.start + off).div_euclid(86_400);
        let to = (e.end.max(e.start + 1) - 1 + off).div_euclid(86_400);
        for d in from.max(first)..=to.min(last) {
            let day = (d - first + 1) as u8;
            if !days.contains(&day) {
                days.push(day);
            }
        }
    }
    days.sort_unstable();
    days
}

/// "webcal://…" links are plain HTTPS.
pub fn feed_url(link: &str) -> String {
    let l = link.trim();
    match l.strip_prefix("webcal://") {
        Some(rest) => format!("https://{rest}"),
        None => l.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utc(y: i32, m: u8, d: u8, hh: i64, mm: i64) -> i64 {
        naive(y, m, d, hh, mm, 0)
    }

    #[test]
    fn month_grids() {
        assert_eq!(days_in_month(2026, 2), 28);
        assert_eq!(days_in_month(2028, 2), 29);
        assert_eq!(days_in_month(2100, 2), 28);
        assert_eq!(days_in_month(2026, 11), 30);
        assert_eq!(weekday_of(2026, 11, 4), 3, "4 November 2026 is a Wednesday");
        assert_eq!(weekday_of(2026, 11, 1), 0, "and the 1st a Sunday");
        assert_eq!(shift_month(2026, 12, 1), (2027, 1));
        assert_eq!(shift_month(2026, 1, -1), (2025, 12));
        assert_eq!(shift_month(2026, 11, -14), (2025, 9));
    }

    #[test]
    fn marks_the_days_events_fall_on() {
        let e = |start, end, all_day| CalEvent { summary: "x".into(), start, end, all_day, location: None, link: None };
        let events = [
            // 23:30–00:30 UTC on the 4th: at UTC+3 it is all on the 5th.
            e(utc(2026, 11, 4, 23, 30), utc(2026, 11, 5, 0, 30), false),
            // An all-day event on the 10th and 11th (ends at the 12th's midnight).
            e(utc(2026, 11, 10, 0, 0), utc(2026, 11, 12, 0, 0), true),
            // Last month into this one.
            e(utc(2026, 10, 30, 9, 0), utc(2026, 11, 1, 9, 0), false),
            // Next month: not shown.
            e(utc(2026, 12, 2, 9, 0), utc(2026, 12, 2, 10, 0), false),
        ];
        assert_eq!(event_days(&events, 2026, 11, 3 * 3600), vec![1, 5, 10, 11]);
        assert_eq!(event_days(&events, 2026, 11, 0), vec![1, 4, 5, 10, 11]);
        assert_eq!(event_days(&events, 2026, 12, 0), vec![2]);
    }

    const ZONE: &str = "BEGIN:VTIMEZONE\r\nTZID:Europe/Berlin\r\nBEGIN:DAYLIGHT\r\nTZOFFSETFROM:+0100\r\nTZOFFSETTO:+0200\r\nDTSTART:19700329T020000\r\nRRULE:FREQ=YEARLY;BYMONTH=3;BYDAY=-1SU\r\nEND:DAYLIGHT\r\nBEGIN:STANDARD\r\nTZOFFSETFROM:+0200\r\nTZOFFSETTO:+0100\r\nDTSTART:19701025T030000\r\nRRULE:FREQ=YEARLY;BYMONTH=10;BYDAY=-1SU\r\nEND:STANDARD\r\nEND:VTIMEZONE\r\n";

    fn cal(body: &str) -> String {
        format!("BEGIN:VCALENDAR\r\nVERSION:2.0\r\n{ZONE}{body}END:VCALENDAR\r\n")
    }

    #[test]
    fn finds_video_call_links_and_nothing_else() {
        assert_eq!(
            meeting_link("Join: https://us02web.zoom.us/j/123456?pwd=abc. See you!").as_deref(),
            Some("https://us02web.zoom.us/j/123456?pwd=abc")
        );
        assert_eq!(
            meeting_link("<https://meet.google.com/abc-defg-hij>").as_deref(),
            Some("https://meet.google.com/abc-defg-hij")
        );
        // Other links are skipped, and the call link after them is found.
        assert_eq!(
            meeting_link("Agenda https://docs.example.com/x then https://teams.microsoft.com/l/meetup-join/1")
                .as_deref(),
            Some("https://teams.microsoft.com/l/meetup-join/1")
        );
        assert_eq!(meeting_link("Room 4"), None);
        assert_eq!(meeting_link("https://example.com/zoom.us"), None, "the host decides, not the path");
        assert_eq!(meeting_link("https://zoom.us@elsewhere.example/j/1"), None, "no user part");
        assert_eq!(meeting_link("https://notzoom.us/j/1"), None);
        assert_eq!(meeting_link("http://zoom.us/j/1"), None, "https only");
        assert_eq!(meeting_link("https://"), None);
    }

    #[test]
    fn events_carry_their_call_link() {
        let ics = cal("BEGIN:VEVENT\r\nSUMMARY:Review\r\nDTSTART:20261020T090000Z\r\nDTEND:20261020T093000Z\r\nLOCATION:Online\r\nDESCRIPTION:Notes at https://docs.example.com/n\\nJoin https://meet.google.com/abc-defg-hij\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nSUMMARY:Lunch\r\nDTSTART:20261020T120000Z\r\nDTEND:20261020T130000Z\r\nLOCATION:Canteen\r\nEND:VEVENT\r\n");
        let ev = parse_ics(&ics, utc(2026, 10, 1, 0, 0), utc(2026, 11, 30, 0, 0), 0);
        assert_eq!(ev.len(), 2);
        assert_eq!(ev[0].link.as_deref(), Some("https://meet.google.com/abc-defg-hij"));
        assert_eq!(ev[1].link, None);
    }

    #[test]
    fn spots_deadlines_by_whole_words() {
        for yes in
            ["Essay due", "Maths EXAM", "Physics test (ch. 4)", "Submit lab report", "Hand in coursework", "Quiz 3"]
        {
            assert!(is_deadline(yes), "{yes}");
        }
        for no in ["Contest", "Duet practice", "Lunch with Amina", "Testing the mic", "Examine the plan"] {
            assert!(!is_deadline(no), "{no}");
        }
    }

    #[test]
    fn counts_down_to_a_deadline() {
        let now = utc(2026, 11, 4, 9, 0);
        let at = |start, all_day| CalEvent {
            summary: "x".into(),
            start,
            end: start + 3600,
            all_day,
            location: None,
            link: None,
        };
        assert_eq!(countdown(&at(now + 40 * 60, false), now, 0), "in 40 min");
        assert_eq!(countdown(&at(now + 5 * 3600, false), now, 0), "in 5 h");
        assert_eq!(countdown(&at(now + 20 * 3600, false), now, 0), "in 20 h");
        assert_eq!(countdown(&at(utc(2026, 11, 7, 23, 59), false), now, 0), "in 3 days");
        assert_eq!(countdown(&at(now - 60, false), now, 0), "now");
        // All-day deadlines count calendar days.
        assert_eq!(countdown(&at(utc(2026, 11, 4, 0, 0), true), now, 0), "today");
        assert_eq!(countdown(&at(utc(2026, 11, 5, 0, 0), true), now, 0), "tomorrow");
        assert_eq!(countdown(&at(utc(2026, 11, 14, 0, 0), true), now, 0), "in 10 days");
        // 23:00 local at UTC+3 is 20:00 UTC: a deadline at 01:00 local is tomorrow, 2 h away.
        let late = utc(2026, 11, 4, 20, 0);
        assert_eq!(countdown(&at(late + 2 * 3600, false), late, 3 * 3600), "in 2 h");
    }

    #[test]
    fn dates_round_trip() {
        for d in [-1000, 0, 19_000, 20_396] {
            let (y, m, dd) = civil_from_days(d);
            assert_eq!(days_from_civil(y, m, dd), d);
        }
        assert_eq!(civil_from_days(20_396), (2025, 11, 4));
    }

    #[test]
    fn parses_events_in_their_time_zone_across_dst() {
        let ics = cal("BEGIN:VEVENT\r\nSUMMARY:Standup\\, daily\r\nDTSTART;TZID=Europe/Berlin:20261020T090000\r\nDTEND;TZID=Europe/Berlin:20261020T091500\r\nRRULE:FREQ=DAILY;COUNT=10\r\nLOCATION:Room 4\r\nEND:VEVENT\r\n");
        let ev = parse_ics(&ics, utc(2026, 10, 1, 0, 0), utc(2026, 11, 30, 0, 0), 0);
        assert_eq!(ev.len(), 10);
        assert_eq!(ev[0].summary, "Standup, daily");
        assert_eq!(ev[0].location.as_deref(), Some("Room 4"));
        assert_eq!(ev[0].start, utc(2026, 10, 20, 7, 0), "09:00 CEST = 07:00 UTC");
        assert_eq!(ev[0].end - ev[0].start, 15 * 60);
        // DST ended on 25 October: 09:00 CET = 08:00 UTC.
        assert_eq!(ev[6].start, utc(2026, 10, 26, 8, 0));
    }

    #[test]
    fn weekly_byday_until_and_exdate() {
        let ics = cal("BEGIN:VEVENT\r\nSUMMARY:Gym\r\nDTSTART:20261102T170000Z\r\nDURATION:PT1H\r\nRRULE:FREQ=WEEKLY;BYDAY=MO,WE,FR;UNTIL=20261113T235959Z\r\nEXDATE:20261104T170000Z\r\nEND:VEVENT\r\n");
        let ev = parse_ics(&ics, utc(2026, 11, 1, 0, 0), utc(2026, 12, 1, 0, 0), 0);
        let days: Vec<_> = ev.iter().map(|e| civil_from_days(e.start.div_euclid(86_400)).2).collect();
        assert_eq!(days, [2, 6, 9, 11, 13], "Mon 2, (Wed 4 skipped), Fri 6, …, until Fri 13");
    }

    #[test]
    fn all_day_monthly_floating_and_cancelled() {
        let ics = cal("BEGIN:VEVENT\r\nSUMMARY:Rent\r\nDTSTART;VALUE=DATE:20260131\r\nRRULE:FREQ=MONTHLY\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nSUMMARY:Lunch\r\nDTSTART:20261104T120000\r\nDTEND:20261104T130000\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nSUMMARY:Gone\r\nSTATUS:CANCELLED\r\nDTSTART:20261104T150000Z\r\nEND:VEVENT\r\n");
        let ev = parse_ics(&ics, utc(2026, 1, 1, 0, 0), utc(2026, 12, 31, 0, 0), 3 * 3600);
        let rent: Vec<_> = ev
            .iter()
            .filter(|e| e.summary == "Rent")
            .map(|e| civil_from_days((e.start + 3 * 3600).div_euclid(86_400)).1)
            .collect();
        assert_eq!(rent, [1, 3, 5, 7, 8, 10, 12], "only months with a 31st");
        let lunch = ev.iter().find(|e| e.summary == "Lunch").unwrap();
        assert_eq!(lunch.start, utc(2026, 11, 4, 9, 0), "floating 12:00 in UTC+3");
        assert!(!ev.iter().any(|e| e.summary == "Gone"));
    }

    #[test]
    fn up_next_and_wakeups() {
        let e =
            |s| CalEvent { summary: "x".into(), start: s, end: s + 1800, all_day: false, location: None, link: None };
        let events = [e(1000), e(5000)];
        assert!(up_next(&events, 0).is_none(), "more than 10 minutes away");
        assert_eq!(up_next(&events, 500).map(|e| e.start), Some(1000));
        assert_eq!(up_next(&events, 1250).map(|e| e.start), Some(1000), "just started");
        assert!(up_next(&events, 1400).is_none());
        assert_eq!(next_change(&events, 0), Some(400));
        assert_eq!(relative(1000, 500), "in 9 min");
        assert_eq!(relative(1000, 1100), "now");
        assert_eq!(feed_url("webcal://p01.example.com/x.ics"), "https://p01.example.com/x.ics");
        assert_eq!(parse_duration("-PT15M"), Some(-900));
        assert_eq!(parse_duration("P1DT2H"), Some(93_600));
    }
}
