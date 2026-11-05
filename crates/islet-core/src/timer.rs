//! Countdown timers: a plain timer, or a focus (Pomodoro) session that
//! alternates work and break. Times are seconds on the island's app clock;
//! the island shifts them if the computer slept (see `Timer::shift`).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// A plain timer, or a focus session's work stretch.
    Work,
    Break,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Timer {
    pub label: String,
    /// A focus session (work/break rounds) rather than a plain timer.
    pub focus: bool,
    pub phase: Phase,
    /// 1-based round, and how many (1 for a plain timer).
    pub round: u32,
    pub rounds: u32,
    pub work: f64,
    pub brk: f64,
    /// When the current phase ends (running), or None while paused.
    pub ends_at: Option<f64>,
    /// Seconds left while paused.
    pub left: f64,
}

/// Requests from the command line or Settings.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", tag = "do")]
pub enum TimerCommand {
    Start { secs: f64, label: String },
    Focus { work: f64, brk: f64, rounds: u32 },
    Pause,
    Resume,
    Stop,
}

/// What happened when a phase ran out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerEvent {
    BreakStarted,
    /// Work round n started (after a break).
    WorkStarted(u32),
    /// Everything finished.
    Done,
}

impl Timer {
    pub fn plain(secs: f64, label: &str, now: f64) -> Self {
        Self {
            label: if label.trim().is_empty() { "Timer".into() } else { label.trim().into() },
            focus: false,
            phase: Phase::Work,
            round: 1,
            rounds: 1,
            work: secs.max(1.0),
            brk: 0.0,
            ends_at: Some(now + secs.max(1.0)),
            left: secs.max(1.0),
        }
    }

    pub fn focus(work: f64, brk: f64, rounds: u32, now: f64) -> Self {
        Self {
            label: "Focus".into(),
            focus: true,
            rounds: rounds.max(1),
            brk: brk.max(0.0),
            ..Self::plain(work, "Focus", now)
        }
    }

    pub fn phase_len(&self) -> f64 {
        match self.phase {
            Phase::Work => self.work,
            Phase::Break => self.brk,
        }
    }

    pub fn remaining(&self, now: f64) -> f64 {
        match self.ends_at {
            Some(end) => (end - now).max(0.0),
            None => self.left,
        }
    }

    /// Share of the current phase that has passed, 0..=1.
    pub fn progress(&self, now: f64) -> f32 {
        (1.0 - self.remaining(now) / self.phase_len().max(1.0)).clamp(0.0, 1.0) as f32
    }

    pub fn running(&self) -> bool {
        self.ends_at.is_some()
    }

    pub fn pause(&mut self, now: f64) {
        if let Some(end) = self.ends_at.take() {
            self.left = (end - now).max(0.0);
        }
    }

    pub fn resume(&mut self, now: f64) {
        if self.ends_at.is_none() {
            self.ends_at = Some(now + self.left);
        }
    }

    /// The app clock jumped by `delta` seconds less than the wall clock
    /// (the computer slept): a running timer keeps wall-clock time.
    pub fn shift(&mut self, delta: f64) {
        if let Some(end) = &mut self.ends_at {
            *end -= delta;
        }
    }

    /// Move to the next phase if this one has run out.
    pub fn advance(&mut self, now: f64) -> Option<TimerEvent> {
        let end = self.ends_at?;
        if now < end {
            return None;
        }
        match self.phase {
            Phase::Work if self.focus && self.round < self.rounds && self.brk > 0.0 => {
                self.phase = Phase::Break;
                self.ends_at = Some(end + self.brk);
                Some(TimerEvent::BreakStarted)
            }
            Phase::Break => {
                self.phase = Phase::Work;
                self.round += 1;
                self.ends_at = Some(end + self.work);
                Some(TimerEvent::WorkStarted(self.round))
            }
            Phase::Work => {
                self.ends_at = None;
                self.left = 0.0;
                Some(TimerEvent::Done)
            }
        }
    }

    /// "Focus · 2 of 4", "Break", or the timer's name.
    pub fn title(&self) -> String {
        match (self.focus, self.phase) {
            (true, Phase::Break) => "Break".into(),
            (true, Phase::Work) if self.rounds > 1 => format!("{} · {} of {}", self.label, self.round, self.rounds),
            _ => self.label.clone(),
        }
    }
}

/// "25m", "1h30m", "90s", "1:30" (minutes:seconds), "25" (minutes).
pub fn parse_duration(s: &str) -> Option<f64> {
    let s = s.trim().to_ascii_lowercase().replace(' ', "");
    if s.is_empty() {
        return None;
    }
    if let Some((m, sec)) = s.split_once(':') {
        let (m, sec): (f64, f64) = (m.parse().ok()?, sec.parse().ok()?);
        return (sec < 60.0 && m >= 0.0).then_some(m * 60.0 + sec).filter(|t| *t > 0.0);
    }
    if let Ok(mins) = s.parse::<f64>() {
        return (mins > 0.0).then_some(mins * 60.0);
    }
    let (mut total, mut num) = (0.0, String::new());
    for c in s.chars() {
        match c {
            '0'..='9' | '.' => num.push(c),
            'h' | 'm' | 's' => {
                let n: f64 = num.parse().ok()?;
                num.clear();
                total += n * match c {
                    'h' => 3600.0,
                    'm' => 60.0,
                    _ => 1.0,
                };
            }
            _ => return None,
        }
    }
    (num.is_empty() && total > 0.0).then_some(total)
}

/// "4:05", "24:59", "1:02:03" (rounded up, so it never shows 0:00 early).
pub fn format_countdown(secs: f64) -> String {
    let s = secs.max(0.0).ceil() as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_durations() {
        assert_eq!(parse_duration("25m"), Some(1500.0));
        assert_eq!(parse_duration("1h30m"), Some(5400.0));
        assert_eq!(parse_duration("90s"), Some(90.0));
        assert_eq!(parse_duration("1:30"), Some(90.0));
        assert_eq!(parse_duration("25"), Some(1500.0));
        assert_eq!(parse_duration("0"), None);
        assert_eq!(parse_duration("soon"), None);
        assert_eq!(parse_duration("5x"), None);
    }

    #[test]
    fn formats_countdowns() {
        assert_eq!(format_countdown(245.2), "4:06");
        assert_eq!(format_countdown(1499.0), "24:59");
        assert_eq!(format_countdown(3723.0), "1:02:03");
        assert_eq!(format_countdown(-1.0), "0:00");
    }

    #[test]
    fn plain_timer_pauses_and_finishes() {
        let mut t = Timer::plain(60.0, "Tea", 0.0);
        assert_eq!(t.remaining(15.0), 45.0);
        t.pause(15.0);
        assert_eq!(t.remaining(100.0), 45.0, "paused time doesn't count");
        t.resume(100.0);
        assert_eq!(t.advance(144.0), None);
        assert_eq!(t.advance(145.0), Some(TimerEvent::Done));
        assert!(!t.running());
    }

    #[test]
    fn focus_session_alternates_work_and_break() {
        let mut t = Timer::focus(1500.0, 300.0, 2, 0.0);
        assert_eq!(t.title(), "Focus · 1 of 2");
        assert_eq!(t.advance(1500.0), Some(TimerEvent::BreakStarted));
        assert_eq!((t.title().as_str(), t.remaining(1500.0)), ("Break", 300.0));
        assert_eq!(t.advance(1800.0), Some(TimerEvent::WorkStarted(2)));
        assert_eq!(t.title(), "Focus · 2 of 2");
        assert_eq!(t.advance(3300.0), Some(TimerEvent::Done), "no break after the last round");
    }

    #[test]
    fn keeps_wall_time_across_sleep() {
        let mut t = Timer::plain(600.0, "", 0.0);
        t.shift(120.0); // slept two minutes the app clock didn't count
        assert_eq!(t.remaining(100.0), 380.0);
        assert!((t.progress(100.0) - 0.3667).abs() < 0.001);
    }
}
