//! Portable island logic: pointer input and provider reports → presentation
//! state machine → springs → frame and content. No OS calls in here, so all
//! of it is unit-tested.

use crate::content::{
    self, AlarmControl, ClockRow, ConnHit, Control, NavControl, Panel, StopwatchControl, TimerControl, TIMER_ALARM_ID,
};
use crate::platform::{Button, Event, Key};
use islet_core::activity::{Activities, Call, Hud, Live, MediaCommand, MediaInfo, MicCommand, NavInfo, Report};
use islet_core::alarm::{Alarm, AlarmClock, AlarmEvent};
use islet_core::clock::{secs_to_next_minute, LocalTime};
use islet_core::connectivity::{BtState, ConnCommand, ConnUpdate, Radio, WifiState};
use islet_core::geometry::Rect;
use islet_core::island::{
    spring_for, target_size, Island, Presentation, Signal, AUTO_COLLAPSE_SECS, LONG_PRESS_SECS, MINIMAL_CIRCLE,
    MINIMAL_GAP,
};
use islet_core::prefs::IslandPrefs;
use islet_core::spring::{Animated, Spring};
use islet_core::stopwatch::{Stopwatch, StopwatchCommand};
use islet_core::timer::{Timer, TimerCommand, TimerEvent};
use islet_render::elements::Element;
use islet_render::text::TextEngine;
use islet_render::{Circle, PillFrame, Squircle};
use std::sync::Arc;
use std::time::Duration;

/// Logical window size: the largest state plus spring overshoot (design spec §2).
pub const WINDOW: (f64, f64) = (420.0, 260.0);
/// Gap between the screen's top edge and the pill.
pub const TOP_GAP: f64 = 0.0;
const SMOOTHING: f32 = 0.6;

/// Frame interval while a spring is moving (60 fps)…
pub const FRAME: Duration = Duration::from_micros(16_667);
/// …and while only the waveform moves (30 fps).
pub const WAVE_FRAME: Duration = Duration::from_micros(33_333);

/// Distance from the pill's round end to the centre of the detached circle
/// in Minimal: half the circle, the gap, and the other half.
const BUBBLE_OFFSET: f64 = MINIMAL_CIRCLE / 2.0 + MINIMAL_GAP + MINIMAL_CIRCLE / 2.0;
/// Merging back is quick and bounce-free.
const BUBBLE_MERGE: Spring = Spring::new(0.32, 1.0);
/// Above this pill height the bubble is fully merged.
const BUBBLE_MAX_H: f64 = 60.0;
/// Expanded content never shows below this pill height…
const CONTENT_FROM_H: f64 = 55.0;
/// …fades in over (at most) the last this-many px of the card's growth…
const CONTENT_FADE_H: f64 = 60.0;
/// …and cards are treated as at least this tall for the fade.
const CONTENT_MIN_FULL_H: f64 = 84.0;
const IDLE_W: f64 = 126.0;
/// Extra width a compact pill takes for the camera / microphone dot.
const PRIVACY_W: f64 = 14.0;
/// The dot's centre, from the pill's right edge, and its diameter.
const PRIVACY_INSET: f32 = 14.0;
const PRIVACY_DOT: f32 = 6.0;
/// At most one activity switch per this many seconds of scrolling.
const SWITCH_COOLDOWN: f64 = 0.35;
/// Height of the expanded clock and media cards.
const CLOCK_H: f64 = 168.0 + content::QUICK_H;
const MEDIA_H: f64 = 160.0;
/// How quickly the empty island fades in and out when it hides until
/// hovered (Settings → Look).
const SHOW_SPRING: Spring = Spring::new(0.22, 1.0);

fn smooth(c: f64) -> f64 {
    let c = c.clamp(0.0, 1.0);
    c * c * (3.0 - 2.0 * c)
}

/// Scripted demos, selected with `ISLET_DEMO` (or the legacy
/// `ISLET_PERF_DEMO`), used by the perf harness and for visual review.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Demo {
    None,
    /// Expand/collapse every 1.2 s (perf harness).
    Toggle,
    /// Walk through every presentation every 1.6 s.
    Tour,
}

impl Demo {
    fn period(self) -> f64 {
        match self {
            Demo::Tour => 1.6,
            _ => 1.2,
        }
    }

    fn signals(self) -> &'static [Signal] {
        match self {
            Demo::None => &[],
            Demo::Toggle => &[Signal::Click],
            Demo::Tour => &[
                Signal::LiveActivities(1), // Compact
                Signal::LiveActivities(2), // Minimal: the bubble splits off
                Signal::Click,             // Expanded
                Signal::LongPress,         // Large
                Signal::Escape,            // back to Minimal
                Signal::LiveActivities(1), // the bubble merges back
                Signal::LiveActivities(0), // Idle
                Signal::PointerEnter,      // Peek
                Signal::PointerLeave,
            ],
        }
    }
}

struct Press {
    at: f64,
    long_fired: bool,
}

/// What the main loop should do about alarms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AlarmOut {
    /// Start (true) or stop (false) the alarm sound.
    Sound(bool),
    /// Play the chime once (a focus phase changed).
    Chime,
    /// A one-time alarm rang: save it as turned off.
    Disable(u32),
}

/// What a screen reader is told: the island as a whole, and the buttons
/// on it right now (logical px, window coordinates).
#[derive(Debug, Clone, PartialEq)]
pub struct A11y {
    pub label: String,
    pub expanded: bool,
    pub bounds: Rect,
    pub buttons: Vec<(String, Rect)>,
}

/// Things the main loop opens for the island.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiAction {
    /// A link or settings URI (Windows Bluetooth settings).
    Url(String),
    /// A file or folder from the shelf, in its default app.
    Path(String),
    /// Islet Settings at a page ("wifi:Home": join Home with a password).
    Settings(String),
    /// Turn a setting on or off and save it (`islet set <key> on|off`).
    SetSwitch(&'static str, bool),
    /// Start the OS's own screenshot tool.
    Screenshot,
    /// Lock the computer.
    Lock,
}

/// Windows has no public API to connect Bluetooth devices: its settings do.
const BLUETOOTH_SETTINGS: &str = "ms-settings:bluetooth";

/// How long "Hide for an hour" and "Pause pop-ups" last.
const QUIET_SECS: f64 = 3600.0;
/// One tap of the lyrics-timing buttons.
const LYRICS_STEP: f64 = 0.5;

/// What an open island shows.
enum OpenContent<'a> {
    /// The right-click menu.
    Menu,
    Call(Call),
    Agenda,
    Custom(&'a islet_core::activity::Custom),
    Notes,
    Alarm(&'a Alarm),
    Panel(Panel),
    Timer(&'a Timer),
    /// The stopwatch card (None: not started yet).
    Stopwatch(Option<&'a Stopwatch>),
    Nav(&'a NavInfo),
    Media(&'a MediaInfo),
    Battery,
    Clock,
}

pub struct App {
    island: Island,
    activities: Activities,
    w: Animated,
    h: Animated,
    r: Animated,
    /// Bubble centre relative to the pill's round end: 0 = merged.
    bubble: Animated,
    hovered: bool,
    press: Option<Press>,
    /// A media control being pressed, and the scrub position while dragging.
    control: Option<Control>,
    /// A navigation-card button being pressed, and pressed buttons to act on.
    nav_control: Option<NavControl>,
    nav_actions: Vec<NavControl>,
    /// Alarms: the schedule, a pressed Snooze/Stop button, and work for the loop.
    alarms: AlarmClock,
    alarm_control: Option<AlarmControl>,
    alarm_out: Vec<AlarmOut>,
    /// Open the island briefly on the next tick (a focus phase changed).
    pending_peek: bool,
    /// The timer (authoritative copy), how it was started (for Repeat),
    /// whether its finish is ringing, a pressed button, and the second shown.
    timer: Option<Timer>,
    timer_last: Option<TimerCommand>,
    timer_ringing: bool,
    timer_control: Option<TimerControl>,
    timer_sec: i64,
    /// The stopwatch (Islet Pro), the card button being pressed, and the
    /// last tenth/second drawn.
    stopwatch: Option<Stopwatch>,
    stopwatch_control: Option<StopwatchControl>,
    stopwatch_shown: i64,
    /// The file shelf; whether files are being dragged over the island (and
    /// whether that opened it); and a pending save.
    shelf: Vec<islet_core::shelf::ShelfItem>,
    dragging: bool,
    drag_opened: bool,
    shelf_dirty: bool,
    /// Notes and to-dos (Islet Pro; todo.json), and whether to save them.
    todo: islet_core::todo::Todo,
    todo_dirty: bool,
    /// The quick timer's length (an index into `content::QUICK_MINUTES`).
    quick: usize,
    /// The month the calendar card shows (None: this month).
    cal_view: Option<(i32, u8)>,
    /// Clipboard history (in memory only; empty while the feature is off).
    clip: islet_core::clipboard::History,
    /// Entries to put back on the clipboard (for main to hand over).
    clip_out: Vec<String>,
    /// Gap above the island (Settings → Look), in island px.
    top: f64,
    /// The notifications card was opened from a notification: it stays
    /// until the island closes, even after the peek ends.
    notes_open: bool,
    /// island.json: which pop-ups are on, weather, calendar…
    prefs: IslandPrefs,
    pro: bool,
    /// Unix time and UTC offset at the last `set_wall` (for calendar events).
    wall: f64,
    utc_offset: i64,
    /// The open Wi-Fi or Bluetooth list, a pressed entry, and requests.
    panel: Option<Panel>,
    conn_hit: Option<ConnHit>,
    conn_commands: Vec<ConnCommand>,
    ui_actions: Vec<UiAction>,
    scrub: Option<f64>,
    commands: Vec<MediaCommand>,
    /// When the pointer left an open island (auto-collapse timer).
    left_open_at: Option<f64>,
    demo: Demo,
    demo_step: usize,
    demo_next: f64,
    reduce_motion: bool,
    /// Input region while animating: start and end of the transition.
    anim_region: Rect,
    quit: bool,
    /// Latest wall-clock time (the clock), and 12/24-hour style.
    clock: Option<LocalTime>,
    h24: bool,
    /// Height of the card when the island last opened: content fades
    /// relative to it, so short cards (a device list) fade in fully too.
    open_h: f64,
    /// The last closed presentation (Compact or Minimal): its layout is
    /// used for compact content while the island opens or closes.
    closed: Presentation,
    /// The MacBook notch (width, height), when the island wraps one.
    notch: Option<(f64, f64)>,
    /// Scroll-to-switch: how far the live activities are rotated, the
    /// scroll accumulated towards the next step, and when the last step was.
    rotation: usize,
    scroll_acc: f64,
    last_switch: f64,
    live_count: usize,
    /// Synced lyrics for a song (`Track::key`; None: the song has none).
    lyrics: Option<(String, Option<Arc<islet_core::lyrics::Lyrics>>)>,
    /// Control Centre (the clock card) was opened from an activity's card.
    home: bool,
    /// The island's visibility (1 = shown). Below 1 only while the empty
    /// island hides until hovered.
    shown: Animated,
    /// How far the open list is scrolled, and scroll gathered towards the
    /// next row.
    list_scroll: usize,
    list_acc: f64,
    /// An app is fullscreen on the island's display: the island hides
    /// until the pointer comes to it.
    fullscreen: bool,
    /// The right-click menu is open; the island is hidden (until hovered)
    /// or pop-ups are paused until these times.
    menu: bool,
    hidden_until: Option<f64>,
    quiet_until: Option<f64>,
    /// Lyrics shown this many seconds later (negative: sooner), per song.
    lyrics_delay: std::collections::HashMap<String, f64>,
    /// The output volume and mute (for the card's slider), a level the
    /// slider asked for, and when (its own change shouldn't pop up the HUD).
    volume: Option<(f32, bool)>,
    volume_out: Option<f32>,
    volume_set_at: f64,
    /// Microphone requests (mute, read), and the call's second shown.
    mic_commands: Vec<MicCommand>,
    call_sec: i64,
    /// The call card's mute button is being pressed.
    call_press: bool,
}

impl App {
    pub fn new(demo: Demo, reduce_motion: bool) -> Self {
        let (w, h, r) = target_size(Presentation::Idle, None);
        let s = Spring::new(0.5, 0.8);
        let mut app = Self {
            island: Island::new(),
            activities: Activities::new(),
            w: Animated::new(w, s),
            h: Animated::new(h, s),
            r: Animated::new(r, s),
            bubble: Animated::new(0.0, s),
            hovered: false,
            press: None,
            control: None,
            nav_control: None,
            nav_actions: Vec::new(),
            alarms: AlarmClock::new(),
            alarm_control: None,
            alarm_out: Vec::new(),
            pending_peek: false,
            timer: None,
            timer_last: None,
            timer_ringing: false,
            timer_control: None,
            timer_sec: -1,
            stopwatch: None,
            stopwatch_control: None,
            stopwatch_shown: -1,
            shelf: Vec::new(),
            dragging: false,
            drag_opened: false,
            shelf_dirty: false,
            todo: Default::default(),
            todo_dirty: false,
            quick: content::QUICK_DEFAULT,
            cal_view: None,
            clip: Default::default(),
            clip_out: Vec::new(),
            top: TOP_GAP,
            notes_open: false,
            prefs: IslandPrefs::default(),
            pro: false,
            wall: 0.0,
            utc_offset: 0,
            panel: None,
            conn_hit: None,
            conn_commands: Vec::new(),
            ui_actions: Vec::new(),
            scrub: None,
            commands: Vec::new(),
            left_open_at: None,
            demo,
            demo_step: 0,
            demo_next: demo.period(),
            reduce_motion,
            anim_region: Rect::new(0.0, 0.0, 0.0, 0.0),
            quit: false,
            clock: None,
            h24: true,
            open_h: MEDIA_H,
            closed: Presentation::Idle,
            notch: None,
            rotation: 0,
            scroll_acc: 0.0,
            last_switch: f64::NEG_INFINITY,
            live_count: 0,
            lyrics: None,
            home: false,
            shown: Animated::new(1.0, SHOW_SPRING),
            list_scroll: 0,
            list_acc: 0.0,
            fullscreen: false,
            menu: false,
            hidden_until: None,
            quiet_until: None,
            lyrics_delay: std::collections::HashMap::new(),
            volume: None,
            volume_out: None,
            volume_set_at: f64::NEG_INFINITY,
            mic_commands: Vec::new(),
            call_sec: -1,
            call_press: false,
        };
        app.anim_region = app.target_bounds();
        app
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn presentation(&self) -> Presentation {
        self.island.presentation()
    }

    pub fn wants_quit(&self) -> bool {
        self.quit
    }

    pub fn set_clock_style(&mut self, h24: bool) {
        self.h24 = h24;
    }

    /// Wrap a physical notch (width, height in logical px): idle becomes the
    /// notch itself, compact content sits either side of it and expanded
    /// content below it, and the top corners are square.
    pub fn set_notch(&mut self, notch: Option<(f64, f64)>, t: f64) {
        self.notch = notch;
        let p = self.island.presentation();
        let dims = self.target_dims(p, t);
        for (a, v) in [(&mut self.w, dims.0), (&mut self.h, dims.1), (&mut self.r, dims.2)] {
            a.snap(v);
        }
        self.anim_region = self.target_bounds();
    }

    /// Where expanded content goes: below the notch, if there is one.
    fn content_rect(&self, pill: Rect) -> Rect {
        match self.notch {
            Some((_, nh)) => Rect::new(pill.x, pill.y + nh, pill.w, (pill.h - nh).max(0.0)),
            None => pill,
        }
    }

    /// Activity cards start with a "Control Centre" button; their content
    /// goes below it.
    fn has_header(content: &OpenContent) -> bool {
        !matches!(content, OpenContent::Clock | OpenContent::Panel(_) | OpenContent::Alarm(_) | OpenContent::Menu)
    }

    /// Where an open card's own content goes (below the notch and header).
    fn card_rect(&self, t: f64) -> Rect {
        let cr = self.content_rect(self.settled_pill());
        if Self::has_header(&self.open_content(&self.live(t))) {
            content::below_header(cr)
        } else {
            cr
        }
    }

    /// Navigation buttons pressed since the last call (open Google Maps,
    /// open Waze, stop).
    pub fn take_nav_actions(&mut self) -> Vec<NavControl> {
        std::mem::take(&mut self.nav_actions)
    }

    /// Whether the navigation card is open (it refreshes more often then).
    pub fn nav_card_open(&self, t: f64) -> bool {
        self.island.presentation().is_open() && matches!(self.open_content(&self.live(t)), OpenContent::Nav(_))
    }

    /// The navigation button under (x, y) on an open navigation card.
    fn nav_control_at(&self, x: f64, y: f64, t: f64) -> Option<NavControl> {
        if !self.nav_card_open(t) {
            return None;
        }
        let cr = self.card_rect(t);
        content::nav_controls(cr).into_iter().find(|(r, _)| r.contains(x, y)).map(|(_, c)| c)
    }

    /// The active trip, if any.
    pub fn nav_trip(&self) -> Option<&islet_core::nav::Trip> {
        self.activities.nav().map(|n| &n.trip)
    }

    /// Replace the alarm list (loaded from alarms.json).
    pub fn set_alarms(&mut self, alarms: Vec<Alarm>) {
        self.alarms.set_alarms(alarms);
    }

    /// Sound and persistence work for the main loop, since the last call.
    pub fn take_alarm_out(&mut self) -> Vec<AlarmOut> {
        std::mem::take(&mut self.alarm_out)
    }

    /// Stop the ringing alarm (Stop button, or `islet alarm stop`).
    pub fn stop_alarm(&mut self, t: f64) -> bool {
        if self.timer_ringing {
            self.timer_ringing = false;
            return self.end_ringing(t);
        }
        if !self.alarms.stop() {
            return false;
        }
        self.end_ringing(t)
    }

    /// Ring again in 9 minutes (a finished timer starts over instead).
    pub fn snooze_alarm(&mut self, t: f64) -> bool {
        if self.timer_ringing {
            self.timer_ringing = false;
            let changed = self.end_ringing(t);
            if let Some(cmd) = self.timer_last.clone() {
                self.timer_command(cmd, t);
            }
            return changed;
        }
        let Some(now) = self.clock else { return false };
        if !self.alarms.snooze(&now) {
            return false;
        }
        self.end_ringing(t)
    }

    /// The global shortcut / `islet toggle`: open the island, or close it.
    pub fn toggle(&mut self, t: f64) -> bool {
        if self.island.presentation().is_open() {
            self.signal(Signal::Escape, t)
        } else {
            self.signal(Signal::Click, t)
        }
    }

    fn end_ringing(&mut self, t: f64) -> bool {
        self.alarm_out.push(AlarmOut::Sound(false));
        let changed = self.report(Report::Alarm(None), t);
        if self.island.presentation().is_open() && self.activities.live(t).is_empty() {
            self.signal(Signal::Escape, t);
        }
        changed
    }

    fn alarm_events(&mut self, now: &LocalTime, t: f64) -> bool {
        let mut changed = false;
        for e in self.alarms.tick(now) {
            match e {
                AlarmEvent::Ring(a) => {
                    self.alarm_out.push(AlarmOut::Sound(true));
                    changed |= self.report(Report::Alarm(Some(a)), t);
                    // Like a phone's alarm screen: open the island to show it.
                    if !self.island.presentation().is_open() {
                        self.rotation = 0;
                        changed |= self.signal(Signal::Click, t);
                    }
                }
                AlarmEvent::TimedOut => {
                    self.alarm_out.push(AlarmOut::Sound(false));
                    changed |= self.report(Report::Alarm(None), t);
                }
                AlarmEvent::Finished(id) => self.alarm_out.push(AlarmOut::Disable(id)),
            }
        }
        changed
    }

    /// Start, pause, lap or reset the stopwatch (Islet Pro only).
    pub fn stopwatch_command(&mut self, cmd: StopwatchCommand, t: f64) -> bool {
        if !self.pro {
            return false;
        }
        match (&mut self.stopwatch, cmd) {
            (None, StopwatchCommand::Start | StopwatchCommand::Resume) => self.stopwatch = Some(Stopwatch::started(t)),
            (Some(sw), StopwatchCommand::Start | StopwatchCommand::Resume) => sw.resume(t),
            (Some(sw), StopwatchCommand::Pause) => sw.pause(t),
            (Some(sw), StopwatchCommand::Lap) => sw.lap(t),
            (_, StopwatchCommand::Reset) => {
                self.stopwatch = None;
                if self.panel.is_none() && self.island.presentation().is_open() {
                    // Stay on the (now empty) stopwatch card.
                    self.panel = Some(Panel::Stopwatch);
                }
            }
            (None, _) => return false,
        }
        self.stopwatch_shown = -1;
        self.report(Report::Stopwatch(self.stopwatch.clone()), t)
    }

    /// The Lap/Reset or Start/Pause button under (x, y) on the stopwatch card.
    fn stopwatch_control_at(&self, x: f64, y: f64, t: f64) -> Option<StopwatchControl> {
        if !self.island.presentation().is_open()
            || !matches!(self.open_content(&self.live(t)), OpenContent::Stopwatch(_))
        {
            return None;
        }
        content::stopwatch_controls(self.card_rect(t)).into_iter().find(|(r, _)| r.contains(x, y)).map(|(_, c)| c)
    }

    /// Start, pause, resume or stop the timer.
    pub fn timer_command(&mut self, cmd: TimerCommand, t: f64) -> bool {
        match &cmd {
            TimerCommand::Start { secs, label } => {
                self.timer = Some(Timer::plain(*secs, label, t));
                self.timer_last = Some(cmd.clone());
            }
            TimerCommand::Focus { work, brk, rounds } => {
                self.timer = Some(Timer::focus(*work, *brk, *rounds, t));
                self.timer_last = Some(cmd.clone());
            }
            TimerCommand::Pause => {
                if let Some(tm) = &mut self.timer {
                    tm.pause(t);
                }
            }
            TimerCommand::Resume => {
                if let Some(tm) = &mut self.timer {
                    tm.resume(t);
                }
            }
            TimerCommand::Stop => {
                self.timer = None;
                if self.timer_ringing {
                    self.timer_ringing = false;
                    self.end_ringing(t);
                }
            }
        }
        self.timer_sec = -1;
        self.report(Report::Timer(self.timer.clone()), t)
    }

    /// The app clock fell `delta` seconds behind the wall clock (sleep):
    /// timers keep wall-clock time.
    pub fn clock_jumped(&mut self, delta: f64, t: f64) {
        if let Some(tm) = &mut self.timer {
            tm.shift(delta);
            self.report(Report::Timer(self.timer.clone()), t);
        }
    }

    fn timer_tick(&mut self, t: f64) -> bool {
        let Some(tm) = &mut self.timer else { return false };
        let mut changed = false;
        while let Some(e) = tm.advance(t) {
            changed = true;
            match e {
                TimerEvent::BreakStarted | TimerEvent::WorkStarted(_) => {
                    self.alarm_out.push(AlarmOut::Chime);
                    // Show the new phase for a moment.
                    if !self.island.presentation().is_open() && self.activities.alarm().is_none() {
                        self.pending_peek = true;
                    }
                }
                TimerEvent::Done => break,
            }
        }
        if tm.running() || tm.remaining(t) > 0.0 {
            let sec = tm.remaining(t).ceil() as i64;
            changed |= sec != self.timer_sec;
            self.timer_sec = sec;
            let snapshot = self.timer.clone();
            if changed {
                self.activities.report(Report::Timer(snapshot), t);
            }
            return changed;
        }
        // Finished: ring like an alarm, with Repeat and Stop.
        let label = tm.label.clone();
        let focus = tm.focus;
        self.timer = None;
        self.activities.report(Report::Timer(None), t);
        let (hour, minute) = self.clock.map_or((0, 0), |c| (c.hour, c.minute));
        let title = if focus { "Focus session done".to_string() } else { format!("{label} done") };
        let ring = Alarm { id: TIMER_ALARM_ID, hour, minute, label: title, days: 0, enabled: true };
        self.timer_ringing = true;
        self.alarm_out.push(AlarmOut::Sound(true));
        self.report(Report::Alarm(Some(ring)), t);
        if !self.island.presentation().is_open() {
            self.rotation = 0;
            self.signal(Signal::Click, t);
        }
        true
    }

    /// The Pause/Stop button under (x, y) on an open timer card.
    fn timer_control_at(&self, x: f64, y: f64, t: f64) -> Option<TimerControl> {
        if !self.island.presentation().is_open() {
            return None;
        }
        if !matches!(self.open_content(&self.live(t)), OpenContent::Timer(_)) {
            return None;
        }
        let cr = self.card_rect(t);
        content::timer_controls(cr).into_iter().find(|(r, _)| r.contains(x, y)).map(|(_, c)| c)
    }

    /// The Snooze/Stop button under (x, y) on an open alarm card.
    fn alarm_control_at(&self, x: f64, y: f64, t: f64) -> Option<AlarmControl> {
        if !self.island.presentation().is_open() {
            return None;
        }
        if !matches!(self.open_content(&self.live(t)), OpenContent::Alarm(_)) {
            return None;
        }
        let cr = self.content_rect(self.settled_pill());
        content::alarm_controls(cr).into_iter().find(|(r, _)| r.contains(x, y)).map(|(_, c)| c)
    }

    /// Wi-Fi / Bluetooth requests for the connectivity worker.
    /// Whether the system monitor panel is on screen (sample only then).
    pub fn sysmon_wanted(&self) -> bool {
        self.panel == Some(Panel::System) && self.island.presentation().is_open()
    }

    pub fn take_conn_commands(&mut self) -> Vec<ConnCommand> {
        std::mem::take(&mut self.conn_commands)
    }

    pub fn set_shelf(&mut self, items: Vec<islet_core::shelf::ShelfItem>) {
        self.shelf = items;
    }

    /// The shelf, if it changed since the last call (to save it).
    pub fn take_shelf_changes(&mut self) -> Option<Vec<islet_core::shelf::ShelfItem>> {
        std::mem::take(&mut self.shelf_dirty).then(|| self.shelf.clone())
    }

    pub fn set_todo(&mut self, todo: islet_core::todo::Todo) {
        self.todo = todo;
    }

    /// The to-do list after a tick or removal on the island, to save.
    pub fn take_todo_changes(&mut self) -> Option<islet_core::todo::Todo> {
        std::mem::take(&mut self.todo_dirty).then(|| self.todo.clone())
    }

    /// Files were dropped on the island: (path, is a folder).
    pub fn drop_files(&mut self, dropped: Vec<(String, bool)>, t: f64) -> bool {
        islet_core::shelf::add(&mut self.shelf, &dropped);
        self.shelf_dirty = true;
        self.dragging = false;
        self.drag_opened = false;
        self.panel = Some(Panel::Shelf);
        if !self.island.presentation().is_open() {
            self.signal(Signal::Click, t);
        }
        // Show what landed, then close by itself unless the pointer is on it.
        if !self.hovered {
            self.left_open_at = Some(t);
        }
        self.sync_live(t);
        true
    }

    fn drag_over(&mut self, x: f64, y: f64, t: f64) -> bool {
        let open = self.island.presentation().is_open();
        let over = self.hit(x, y, t) || (open && x.is_finite() && self.bounds(t).inflate(8.0).contains(x, y));
        if over == self.dragging {
            return false;
        }
        self.dragging = over;
        if over {
            self.panel = Some(Panel::Shelf);
            if !open {
                self.drag_opened = true;
                self.signal(Signal::Click, t);
            }
            self.left_open_at = None;
        } else if std::mem::take(&mut self.drag_opened) {
            self.signal(Signal::Escape, t);
        }
        self.sync_live(t);
        true
    }

    /// Links and Settings pages to open.
    pub fn take_ui_actions(&mut self) -> Vec<UiAction> {
        std::mem::take(&mut self.ui_actions)
    }

    /// The clock-card button or list entry under (x, y).
    fn conn_hit_at(&self, x: f64, y: f64, t: f64) -> Option<ConnHit> {
        if !self.island.presentation().is_open() {
            return None;
        }
        let cr = self.content_rect(self.settled_pill());
        let c = self.activities.connectivity();
        let open = self.open_content(&self.live(t));
        if Self::has_header(&open) && content::home_button(cr).contains(x, y) {
            return Some(ConnHit::Home);
        }
        if matches!(open, OpenContent::Clock) && self.home && content::return_button(cr).contains(x, y) {
            return Some(ConnHit::Return);
        }
        if let OpenContent::Panel(_) = open {
            if content::back_button(cr).contains(x, y) {
                return Some(ConnHit::Back);
            }
        }
        if let OpenContent::Menu = open {
            let items = self.menu_items(t);
            let rows = content::menu_rows(cr, items.len());
            let i = rows.iter().position(|r| r.contains(x, y))?;
            if items[i].action == content::MenuAction::LyricsTiming {
                let (minus, plus) = content::menu_stepper(rows[i]);
                return if minus.contains(x, y) {
                    Some(ConnHit::LyricsShift(-1))
                } else if plus.contains(x, y) {
                    Some(ConnHit::LyricsShift(1))
                } else {
                    None
                };
            }
            return Some(ConnHit::Menu(i));
        }
        match open {
            OpenContent::Clock => content::clock_chips(cr, self.clock_row())
                .into_iter()
                .find(|(r, _)| r.contains(x, y))
                .map(|(_, p)| ConnHit::Open(p))
                .or_else(|| {
                    content::quick_row(cr, self.clock_row())
                        .into_iter()
                        .find(|(r, _)| r.contains(x, y))
                        .map(|(_, q)| ConnHit::Quick(q))
                }),
            OpenContent::Panel(Panel::Calendar) => content::calendar_arrows(cr)
                .into_iter()
                .find(|(r, _)| r.contains(x, y))
                .map(|(_, dir)| ConnHit::Month(dir))
                .or_else(|| content::list_header(cr).contains(x, y).then_some(ConnHit::Back)),
            OpenContent::Panel(p) => {
                let (total, footer) = self.panel_list(p);
                let scroll = self.scroll_of(p);
                let rows = content::shown_rows(total, scroll, footer.is_some());
                let has_radio = match p {
                    Panel::Wifi => c.wifi.available,
                    Panel::Bluetooth => c.bt.available,
                    _ => false,
                };
                if has_radio && content::power_switch_hit(cr).contains(x, y) {
                    return Some(ConnHit::Power);
                }
                if content::list_header(cr).contains(x, y) {
                    return Some(ConnHit::Back);
                }
                let list = content::list_rows(cr, rows);
                if let Some(i) = list.iter().position(|r| r.contains(x, y)) {
                    if matches!(p, Panel::Shelf | Panel::Todo) && content::shelf_remove_hit(list[i], x) {
                        return Some(ConnHit::Remove(i + scroll));
                    }
                    return Some(ConnHit::Row(i + scroll));
                }
                (footer.is_some_and(|(_, button)| button) && content::list_footer(cr, rows).contains(x, y))
                    .then_some(ConnHit::Footer)
            }
            _ => None,
        }
    }

    /// The right-click menu's rows, as they stand now.
    fn menu_items(&self, t: f64) -> Vec<content::MenuItem> {
        use content::{MenuAction, MenuItem};
        use islet_render::elements::Icon;
        let left = |until: f64| format!("{} min left", ((until - t) / 60.0).ceil().max(1.0) as i64);
        let item =
            |action, icon, label: &str| MenuItem { action, icon, label: label.into(), detail: None, switch: None };
        let mut v = vec![item(MenuAction::Settings, Icon::Controls, "Islet Settings")];
        v.push(match self.hidden_until {
            Some(h) => {
                MenuItem { detail: Some(left(h)), ..item(MenuAction::Hide, Icon::Moon, "Show the island again") }
            }
            None => item(MenuAction::Hide, Icon::Moon, "Hide for an hour"),
        });
        v.push(match self.quiet_until.filter(|q| t < *q) {
            Some(q) => MenuItem { detail: Some(left(q)), ..item(MenuAction::Quiet, Icon::Bell(0), "Resume pop-ups") },
            None => item(MenuAction::Quiet, Icon::Bell(0), "Pause pop-ups for an hour"),
        });
        v.push(MenuItem { switch: Some(self.prefs.lyrics), ..item(MenuAction::Lyrics, Icon::Music, "Lyrics") });
        if self.song_lyrics().is_some() {
            let d = self.song_key().and_then(|k| self.lyrics_delay.get(&k).copied()).unwrap_or(0.0);
            let detail = if d == 0.0 { "On time".to_string() } else { format!("{d:+.1} s") };
            v.push(MenuItem { detail: Some(detail), ..item(MenuAction::LyricsTiming, Icon::Music, "Lyrics timing") });
        }
        v.push(item(MenuAction::Quit, Icon::Close, "Quit Islet"));
        v
    }

    fn menu_act(&mut self, i: usize, t: f64) -> bool {
        use content::MenuAction;
        let Some(item) = self.menu_items(t).into_iter().nth(i) else { return false };
        match item.action {
            MenuAction::Settings => {
                self.ui_actions.push(UiAction::Settings("general".into()));
                self.menu = false;
                self.signal(Signal::Escape, t);
            }
            MenuAction::Hide => {
                self.hidden_until = if self.hidden_until.is_some() { None } else { Some(t + QUIET_SECS) };
                self.menu = false;
                self.signal(Signal::Escape, t);
                self.update_shown(t);
            }
            MenuAction::Quiet => {
                self.quiet_until = if self.quiet_until.is_some_and(|q| t < q) { None } else { Some(t + QUIET_SECS) };
            }
            MenuAction::Lyrics => {
                let on = !self.prefs.lyrics;
                self.prefs.lyrics = on;
                if !on {
                    self.lyrics = None;
                }
                self.ui_actions.push(UiAction::SetSwitch("lyrics", on));
            }
            MenuAction::LyricsTiming => {}
            MenuAction::Quit => self.quit = true,
        }
        self.sync_live(t);
        true
    }

    /// Is (x, y) on the call card's mute button?
    fn call_mute_at(&self, x: f64, y: f64, t: f64) -> bool {
        self.island.presentation().is_open()
            && matches!(self.open_content(&self.live(t)), OpenContent::Call(_))
            && content::call_mute_button(self.card_rect(t)).contains(x, y)
    }

    /// Mute or unmute the microphone: shown at once, applied by the OS.
    fn toggle_mic(&mut self, t: f64) -> bool {
        let Some(c) = self.activities.call() else { return false };
        let muted = !c.muted.unwrap_or(false);
        self.mic_commands.push(MicCommand::Set(muted));
        self.activities.report(Report::MicMuted(muted), t);
        true
    }

    /// Microphone requests for the main loop: read the mute switch when a
    /// call starts, set it from the mute button.
    pub fn take_mic_commands(&mut self) -> Vec<MicCommand> {
        std::mem::take(&mut self.mic_commands)
    }

    /// Whether a call (the microphone in use) is going on.
    pub fn call_active(&self) -> bool {
        self.prefs.calls && self.activities.call().is_some()
    }

    /// The volume slider dragged to x: show it at once and ask for it.
    fn volume_from(&mut self, x: f64, t: f64) -> bool {
        let level = content::volume_fraction(self.card_rect(t), x);
        self.volume = Some((level, false));
        self.volume_out = Some(level);
        self.volume_set_at = t;
        true
    }

    /// The level the volume slider asked for since the last call.
    pub fn take_volume(&mut self) -> Option<f32> {
        self.volume_out.take()
    }

    /// The playing song's key (`Track::key`).
    fn song_key(&self) -> Option<String> {
        let m = self.activities.media()?;
        Some(format!("{}\u{1f}{}", m.artist.trim().to_lowercase(), m.title.trim().to_lowercase()))
    }

    /// Where the song is for its lyrics: the position, nudged by the timing set in the menu.
    fn lyrics_pos(&self, m: &MediaInfo, t: f64) -> f64 {
        let delay = self.song_key().and_then(|k| self.lyrics_delay.get(&k).copied()).unwrap_or(0.0);
        m.position_now(t).unwrap_or(0.0) - delay
    }

    /// A list panel's length and its footer (line, is a button).
    fn panel_list(&self, p: Panel) -> (usize, Option<(String, bool)>) {
        let c = self.activities.connectivity();
        match p {
            Panel::Wifi => (c.wifi.networks.len(), content::wifi_footer(c)),
            Panel::Bluetooth => (c.bt.devices.len(), content::bt_footer(c)),
            Panel::Shelf => (self.shelf.len(), content::shelf_footer(self.shelf.len(), self.dragging)),
            Panel::System | Panel::Stopwatch | Panel::Calendar => (0, None),
            Panel::Todo => (self.todo.items.len(), Some((content::TODO_ADD.to_string(), true))),
            Panel::Clipboard => {
                let n = self.clip.len();
                (n, (n > 0).then(|| (content::CLIPBOARD_CLEAR.to_string(), true)))
            }
        }
    }

    /// How far the open list is scrolled (kept in range as it changes).
    fn scroll_of(&self, p: Panel) -> usize {
        let (total, footer) = self.panel_list(p);
        self.list_scroll.min(content::max_scroll(total, footer.is_some()))
    }

    /// Scrolling over an open list moves it a row at a time.
    fn scroll_list(&mut self, p: Panel, dy: f64) -> bool {
        self.list_acc += dy;
        if self.list_acc.abs() < 1.0 {
            return false;
        }
        let down = self.list_acc > 0.0;
        self.list_acc = 0.0;
        let (total, footer) = self.panel_list(p);
        let max = content::max_scroll(total, footer.is_some());
        let now = self.scroll_of(p);
        let next = if down { (now + 1).min(max) } else { now.saturating_sub(1) };
        self.list_scroll = next;
        next != now
    }

    fn conn_act(&mut self, hit: ConnHit, t: f64) -> bool {
        let c = self.activities.connectivity().clone();
        match hit {
            ConnHit::Open(p) => {
                self.panel = Some(p);
                self.list_scroll = 0;
                self.activities.report(Report::Connectivity(ConnUpdate::ClearFailure), t);
                match p {
                    Panel::Wifi => self.conn_commands.push(ConnCommand::ScanWifi),
                    Panel::Bluetooth => self.conn_commands.push(ConnCommand::ListBluetooth),
                    Panel::Shelf
                    | Panel::System
                    | Panel::Clipboard
                    | Panel::Stopwatch
                    | Panel::Todo
                    | Panel::Calendar => {}
                }
            }
            ConnHit::Quick(q) => {
                use content::Quick;
                match q {
                    Quick::Step(dir) => {
                        let max = content::QUICK_MINUTES.len() - 1;
                        self.quick = if dir < 0 { self.quick.saturating_sub(1) } else { (self.quick + 1).min(max) };
                    }
                    Quick::Start => {
                        let secs = f64::from(content::QUICK_MINUTES[self.quick]) * 60.0;
                        // The island shows the new timer's card instead of Control Centre.
                        self.home = false;
                        return self.timer_command(TimerCommand::Start { secs, label: String::new() }, t);
                    }
                    Quick::Calendar => {
                        self.panel = Some(Panel::Calendar);
                        self.cal_view = None;
                    }
                    // The island gets out of the way (and out of the picture).
                    Quick::Screenshot => {
                        self.ui_actions.push(UiAction::Screenshot);
                        return self.signal(Signal::Escape, t);
                    }
                    Quick::Lock => {
                        self.ui_actions.push(UiAction::Lock);
                        return self.signal(Signal::Escape, t);
                    }
                }
            }
            ConnHit::Month(dir) => {
                let (y, m) = self.cal_month();
                self.cal_view = Some(islet_core::calendar::shift_month(y, m, i32::from(dir)));
            }
            ConnHit::Remove(i) if self.panel == Some(Panel::Todo) => {
                if let Some(id) = self.todo.ordered().get(i).map(|item| item.id) {
                    self.todo.remove(id);
                    self.todo_dirty = true;
                }
            }
            ConnHit::Remove(i) => {
                if i < self.shelf.len() {
                    self.shelf.remove(i);
                    self.shelf_dirty = true;
                }
            }
            ConnHit::Back => {
                self.panel = None;
                self.list_scroll = 0;
            }
            ConnHit::Home => {
                self.home = true;
                self.panel = None;
                self.control = None;
                self.scrub = None;
                // Show the current network and device on the buttons.
                self.conn_commands.push(ConnCommand::Status);
            }
            ConnHit::Return => self.home = false,
            ConnHit::Menu(i) => return self.menu_act(i, t),
            ConnHit::LyricsShift(dir) => {
                if let Some(key) = self.song_key() {
                    let d = self.lyrics_delay.entry(key).or_insert(0.0);
                    *d = ((*d + f64::from(dir) * LYRICS_STEP) * 2.0).round() / 2.0;
                }
            }
            // The switch flips at once; the radio's real state follows (and
            // flips it back, with a message, if the OS says no).
            ConnHit::Power => match self.panel {
                Some(Panel::Wifi) => {
                    let on = !c.wifi.enabled;
                    self.conn_commands.push(ConnCommand::SetPower(Radio::Wifi, on));
                    let wifi = WifiState { enabled: on, ..c.wifi.clone() };
                    self.activities.report(Report::Connectivity(ConnUpdate::Wifi(wifi)), t);
                }
                Some(Panel::Bluetooth) => {
                    let on = !c.bt.powered;
                    self.conn_commands.push(ConnCommand::SetPower(Radio::Bluetooth, on));
                    let bt = BtState { powered: on, ..c.bt.clone() };
                    self.activities.report(Report::Connectivity(ConnUpdate::Bluetooth(bt)), t);
                }
                _ => {}
            },
            ConnHit::Row(i) => match self.panel {
                Some(Panel::Wifi) => {
                    if let Some(n) = c.wifi.networks.get(i).filter(|n| !n.active) {
                        self.conn_commands.push(ConnCommand::ConnectWifi { ssid: n.ssid.clone(), password: None });
                    }
                }
                Some(Panel::Bluetooth) => {
                    if let Some(d) = c.bt.devices.get(i) {
                        if !c.bt.can_connect {
                            self.ui_actions.push(UiAction::Url(BLUETOOTH_SETTINGS.into()));
                        } else if d.connected {
                            self.conn_commands.push(ConnCommand::DisconnectBluetooth(d.id.clone()));
                        } else {
                            self.conn_commands.push(ConnCommand::ConnectBluetooth(d.id.clone()));
                        }
                    }
                }
                Some(Panel::Shelf) => {
                    if let Some(item) = self.shelf.get(i) {
                        self.ui_actions.push(UiAction::Path(item.path.clone()));
                    }
                }
                Some(Panel::Clipboard) => {
                    if let Some(text) = self.clip.get(i).cloned() {
                        self.clip.push(&text);
                        self.clip_out.push(text);
                    }
                }
                Some(Panel::Todo) => {
                    if let Some(id) = self.todo.ordered().get(i).map(|item| item.id) {
                        self.todo.toggle(id);
                        self.todo_dirty = true;
                    }
                }
                Some(Panel::System | Panel::Stopwatch | Panel::Calendar) | None => {}
            },
            ConnHit::Footer => match (self.panel, &c.failure) {
                (Some(Panel::Wifi), Some((_, Some(ssid)))) => {
                    self.ui_actions.push(UiAction::Settings(format!("wifi:{ssid}")))
                }
                (Some(Panel::Bluetooth), _) => self.ui_actions.push(UiAction::Url(BLUETOOTH_SETTINGS.into())),
                (Some(Panel::Clipboard), _) => self.clip.clear(),
                (Some(Panel::Todo), _) => self.ui_actions.push(UiAction::Settings("pro".into())),
                _ => {}
            },
        }
        self.sync_live(t);
        true
    }

    /// The island described for screen readers.
    pub fn accessibility(&self, t: f64) -> A11y {
        use islet_core::activity::Hud;
        let live = self.live(t);
        let pct = |v: f32| (v * 100.0).round() as i32;
        let label = match live.first() {
            None => match self.clock {
                Some(now) if self.island.presentation().is_open() || self.prefs.idle_clock => {
                    let (time, suffix) = islet_core::clock::format_time(&now, self.h24);
                    format!("Islet. {time}{}", suffix.map_or(String::new(), |s| format!(" {s}")))
                }
                _ => "Islet".to_string(),
            },
            Some(Live::Alarm(a)) => format!("Alarm ringing: {}", a.title()),
            Some(Live::Call(c)) => format!(
                "On a call{}, {}{}",
                if c.app.is_empty() { String::new() } else { format!(" in {}", c.app) },
                content::format_duration(t - c.since),
                if c.muted == Some(true) { ", microphone muted" } else { "" }
            ),
            Some(Live::Hud(Hud::Volume { muted: true, .. })) => "Volume muted".to_string(),
            Some(Live::Hud(Hud::Volume { level, .. })) => format!("Volume {}%", pct(*level)),
            Some(Live::Hud(Hud::Brightness(b))) => format!("Brightness {}%", pct(*b)),
            Some(Live::Hud(Hud::CapsLock(on))) => format!("Caps Lock {}", if *on { "on" } else { "off" }),
            Some(Live::Note(n)) => format!("Notification from {}: {}", n.app, n.summary),
            Some(Live::Battery {
                device: d,
                reason:
                    reason @ (islet_core::activity::PeekReason::ChargerChanged | islet_core::activity::PeekReason::Full),
            }) => format!("{}, {}%", content::charging_label(d, *reason), d.percent),
            Some(Live::Battery { device: d, .. }) => {
                format!("{} {}%{}", d.name, d.percent, if d.charging { ", charging" } else { "" })
            }
            Some(Live::Custom(c)) => match c.progress {
                Some(p) => format!("{}, {}%", c.title, (p * 100.0).round() as i32),
                None => format!("{}, in progress", c.title),
            },
            Some(Live::Stopwatch(sw)) => format!(
                "Stopwatch, {}{}",
                islet_core::stopwatch::format(sw.elapsed(t), false),
                if sw.running() { "" } else { ", paused" }
            ),
            Some(Live::Timer(tm)) => format!(
                "{}, {} left{}",
                tm.title(),
                islet_core::timer::format_countdown(tm.remaining(t)),
                if tm.running() { "" } else { ", paused" }
            ),
            Some(Live::Event { event, starts_at }) => {
                format!("{} {}", event.summary, islet_core::calendar::relative(starts_at.ceil() as i64, t as i64))
            }
            Some(Live::Navigation(n)) => match &n.route {
                Some(r) => format!("Navigation to {}: {}", n.trip.title(), islet_core::nav::format_minutes(r.duration)),
                None => format!("Navigation to {}", n.trip.title()),
            },
            Some(Live::Media(m)) => format!(
                "{}: {}{}{}",
                if m.playing { "Now playing" } else { "Paused" },
                m.title,
                if m.artist.is_empty() { String::new() } else { format!(" by {}", m.artist) },
                if m.app_name.is_empty() { String::new() } else { format!(", in {}", m.app_name) }
            ),
        };
        let label = match self.privacy_shown().then(|| self.activities.privacy().describe()).flatten() {
            Some(p) => format!("{label}. {p}"),
            None => label,
        };
        let open = self.island.presentation().is_open();
        let mut buttons = Vec::new();
        if open && !self.animating(t) {
            let outer = self.content_rect(self.pill_rect(t));
            let cr = self.card_rect(t);
            let c = self.activities.connectivity();
            let shown = self.open_content(&live);
            if Self::has_header(&shown) {
                buttons.push(("Control Centre".to_string(), content::home_button(outer)));
            }
            if self.home && matches!(shown, OpenContent::Clock) {
                buttons.push((format!("Back to {}", self.activity_label(&live)), content::return_button(outer)));
            }
            match shown {
                OpenContent::Call(c) => {
                    let name = if c.muted == Some(true) { "Unmute microphone" } else { "Mute microphone" };
                    buttons.push((name.into(), content::call_mute_button(cr)));
                }
                OpenContent::Menu => {
                    let items = self.menu_items(t);
                    for (item, r) in items.iter().zip(content::menu_rows(cr, items.len())) {
                        if item.action == content::MenuAction::LyricsTiming {
                            let (minus, plus) = content::menu_stepper(r);
                            buttons.push(("Lyrics sooner".into(), minus));
                            buttons.push(("Lyrics later".into(), plus));
                            continue;
                        }
                        let state = match item.switch {
                            Some(on) => format!(", {}", if on { "on" } else { "off" }),
                            None => String::new(),
                        };
                        buttons.push((format!("{}{state}", item.label), r));
                    }
                }
                OpenContent::Media(m) => {
                    for (r, ctl) in content::media_controls(cr, self.volume.is_some()) {
                        let name = match ctl {
                            Control::Previous => "Previous",
                            Control::PlayPause if m.playing => "Pause",
                            Control::PlayPause => "Play",
                            Control::Next => "Next",
                            Control::Scrub | Control::Volume => continue,
                        };
                        buttons.push((name.to_string(), r));
                    }
                }
                OpenContent::Alarm(a) => {
                    let [(snooze, _), (stop, _)] = content::alarm_controls(cr);
                    buttons.push((if a.id == TIMER_ALARM_ID { "Repeat" } else { "Snooze" }.into(), snooze));
                    buttons.push(("Stop".into(), stop));
                }
                OpenContent::Timer(tm) => {
                    let [(pause, _), (stop, _)] = content::timer_controls(cr);
                    buttons.push((if tm.running() { "Pause" } else { "Resume" }.into(), pause));
                    buttons.push(("Stop timer".into(), stop));
                }
                OpenContent::Nav(_) => {
                    for (r, ctl) in content::nav_controls(cr) {
                        let name = match ctl {
                            NavControl::GoogleMaps => "Open in Google Maps",
                            NavControl::Waze => "Open in Waze",
                            NavControl::Stop => "Stop navigation",
                        };
                        buttons.push((name.into(), r));
                    }
                }
                OpenContent::Clock => {
                    for (r, p) in content::clock_chips(cr, self.clock_row()) {
                        let name = match p {
                            Panel::Wifi => {
                                format!("Wi-Fi{}", c.wifi.current.as_ref().map_or(String::new(), |n| format!(", {n}")))
                            }
                            Panel::Bluetooth => {
                                format!(
                                    "Bluetooth{}",
                                    c.connected_device().map_or(String::new(), |d| format!(", {}", d.name))
                                )
                            }
                            Panel::Shelf => format!("Shelf, {} items", self.shelf.len()),
                            Panel::System => "System monitor".into(),
                            Panel::Stopwatch => "Stopwatch".into(),
                            Panel::Todo => format!("To-do, {} left", self.todo.left()),
                            Panel::Calendar => "Calendar".into(),
                            Panel::Clipboard => format!("Clipboard history, {} items", self.clip.len()),
                        };
                        buttons.push((name, r));
                    }
                    let minutes = content::QUICK_MINUTES[self.quick];
                    for (r, q) in content::quick_row(cr, self.clock_row()) {
                        let name = match q {
                            content::Quick::Step(d) if d < 0 => "Shorter timer".to_string(),
                            content::Quick::Step(_) => "Longer timer".to_string(),
                            content::Quick::Start => format!("Start a {minutes}-minute timer"),
                            content::Quick::Calendar => "Calendar".to_string(),
                            content::Quick::Screenshot => "Take a screenshot".to_string(),
                            content::Quick::Lock => "Lock the computer".to_string(),
                        };
                        buttons.push((name, r));
                    }
                }
                OpenContent::Panel(p) => {
                    buttons.push(("Back".into(), content::list_header(cr)));
                    if p == Panel::Calendar {
                        for (r, dir) in content::calendar_arrows(cr) {
                            buttons.push((if dir < 0 { "Previous month" } else { "Next month" }.into(), r));
                        }
                    }
                    let radio = match p {
                        Panel::Wifi if c.wifi.available => Some(("Wi-Fi", c.wifi.enabled)),
                        Panel::Bluetooth if c.bt.available => Some(("Bluetooth", c.bt.powered)),
                        _ => None,
                    };
                    if let Some((name, on)) = radio {
                        let what = if on { "off" } else { "on" };
                        buttons.push((format!("Turn {name} {what}"), content::power_switch(cr)));
                    }
                    let names: Vec<String> = match p {
                        Panel::Wifi => c
                            .wifi
                            .networks
                            .iter()
                            .map(|n| {
                                format!(
                                    "{}{}",
                                    n.ssid,
                                    if n.active {
                                        ", connected"
                                    } else if n.secure && !n.known {
                                        ", needs a password"
                                    } else {
                                        ""
                                    }
                                )
                            })
                            .collect(),
                        Panel::Bluetooth => c
                            .bt
                            .devices
                            .iter()
                            .map(|d| format!("{}, {}", d.name, if d.connected { "connected" } else { "not connected" }))
                            .collect(),
                        Panel::Shelf => self.shelf.iter().map(|i| format!("Open {}", i.name())).collect(),
                        Panel::System | Panel::Stopwatch | Panel::Calendar => Vec::new(),
                        Panel::Todo => self
                            .todo
                            .ordered()
                            .iter()
                            .map(|i| format!("{}{}", i.text, if i.done { ", done" } else { ", to do" }))
                            .collect(),
                        Panel::Clipboard => {
                            self.clip.items().map(|t| format!("Copy {}", islet_core::clipboard::preview(t))).collect()
                        }
                    };
                    let (total, footer) = self.panel_list(p);
                    let scroll = self.scroll_of(p);
                    let rows = content::list_rows(cr, content::shown_rows(total, scroll, footer.is_some()));
                    for (name, r) in names.into_iter().skip(scroll).zip(rows) {
                        buttons.push((name, r));
                    }
                }
                _ => {}
            }
        }
        A11y { label, expanded: open, bounds: self.bounds(t), buttons }
    }

    /// A screen reader pressed the island (None) or one of its buttons.
    pub fn a11y_press(&mut self, button: Option<usize>, t: f64) -> bool {
        let Some(i) = button else { return self.signal(Signal::Click, t) };
        let Some((_, r)) = self.accessibility(t).buttons.get(i).cloned() else { return false };
        let (x, y) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
        self.handle(Event::PointerDown { x, y, button: Button::Primary }, t);
        self.handle(Event::PointerUp { x, y, button: Button::Primary }, t)
    }

    /// Where media commands go: the shown session's source.
    pub fn media_source(&self) -> String {
        self.activities.media().map(|m| m.source.clone()).unwrap_or_default()
    }

    /// Commands for the media session, collected since the last call.
    pub fn take_commands(&mut self) -> Vec<MediaCommand> {
        std::mem::take(&mut self.commands)
    }

    /// The wall clock (Unix seconds) and UTC offset, for calendar events.
    /// Returns true when "up next" changed.
    pub fn set_wall(&mut self, epoch: f64, utc_offset: i64, t: f64) -> bool {
        self.wall = epoch;
        self.utc_offset = utc_offset;
        self.update_up_next(t)
    }

    fn update_up_next(&mut self, t: f64) -> bool {
        let now = self.wall as i64;
        let next = islet_core::calendar::up_next(self.activities.calendar(), now)
            .map(|e| (e.clone(), t + (e.start as f64 - self.wall)));
        self.report(Report::UpNext(next), t)
    }

    /// The next few events for the agenda card.
    fn agenda(&self) -> Vec<content::AgendaRow> {
        let now = self.wall as i64;
        self.activities
            .calendar()
            .iter()
            .filter(|e| e.end > now && e.start < now + 24 * 3600)
            .take(content::AGENDA_ROWS)
            .map(|e| {
                let local = (e.start + self.utc_offset).rem_euclid(86_400);
                let time = if e.all_day {
                    "All day".to_string()
                } else {
                    islet_core::alarm::format_hm((local / 3600) as u8, (local / 60 % 60) as u8, self.h24)
                };
                content::AgendaRow {
                    time,
                    title: e.summary.clone(),
                    location: e.location.clone(),
                    now: !e.all_day && e.start <= now,
                }
            })
            .collect()
    }

    /// Move the island down from the top edge (the window grows to match).
    pub fn set_top(&mut self, top: f64) {
        self.top = top;
    }

    /// Islet Pro is on (its features show; see `islet_core::pro`).
    pub fn set_pro(&mut self, on: bool) {
        self.pro = on;
    }

    pub fn set_prefs(&mut self, prefs: IslandPrefs) {
        content::set_accent(prefs.accent_rgb());
        self.prefs = prefs;
        if !self.prefs.lyrics {
            self.lyrics = None;
        }
        if !self.prefs.clipboard {
            // Turning history off forgets it.
            self.clip.clear();
            if self.panel == Some(Panel::Clipboard) {
                self.panel = None;
            }
        }
    }

    /// The song whose lyrics to look up: the one playing, while lyrics are on.
    pub fn lyrics_wanted(&self) -> Option<islet_core::lyrics::Track> {
        if !self.prefs.lyrics {
            return None;
        }
        let m = self.activities.media().filter(|m| !m.title.trim().is_empty())?;
        Some(islet_core::lyrics::Track {
            title: m.title.clone(),
            artist: m.artist.clone(),
            album: m.album.clone(),
            duration: m.duration.filter(|d| d.is_finite() && *d > 0.0).map(|d| d.round() as u32),
            file: m.file.clone(),
        })
    }

    /// Synced lyrics for the song on the island, when lyrics are on and
    /// the song has them.
    fn song_lyrics(&self) -> Option<&islet_core::lyrics::Lyrics> {
        if !self.prefs.lyrics {
            return None;
        }
        let key = self.song_key()?;
        match &self.lyrics {
            Some((k, Some(l))) if *k == key && !l.is_empty() => Some(l),
            _ => None,
        }
    }

    /// Lyrics run through the compact pill (not beside a notch: it would
    /// hide the middle of every line).
    fn lyrics_in_bar(&self, live: &[Live]) -> bool {
        self.notch.is_none() && matches!(live.first(), Some(Live::Media(_))) && self.song_lyrics().is_some()
    }

    /// The name on Control Centre's button back to the activity.
    fn activity_label(&self, live: &[Live]) -> String {
        match self.open_content_as(live, false) {
            OpenContent::Media(_) => "Now Playing".into(),
            OpenContent::Timer(tm) => tm.title().to_string(),
            OpenContent::Nav(_) => "Navigation".into(),
            OpenContent::Notes => "Notifications".into(),
            OpenContent::Agenda => "Calendar".into(),
            OpenContent::Battery => "Battery".into(),
            OpenContent::Custom(c) => c.title.clone(),
            OpenContent::Call(_) => "Call".into(),
            _ => "Back".into(),
        }
    }

    /// Whether the empty island is hidden until hovered: Settings' switch,
    /// and nothing live, hovered, dragged over or in use (camera dot).
    fn shown_target(&self) -> f64 {
        let hide = self.prefs.auto_hide
            && self.notch.is_none()
            && self.island.presentation() == Presentation::Idle
            && !self.dragging
            && !self.privacy_shown();
        if hide || self.hidden_for_fullscreen() || self.hidden_for_now() {
            0.0
        } else {
            1.0
        }
    }

    /// Over a fullscreen video or game the whole island steps aside, music
    /// and all, until the pointer comes to the top of the screen. It still
    /// shows when it's opened, when files are dragged to it, and for a
    /// ringing alarm or timer.
    fn hidden_for_fullscreen(&self) -> bool {
        self.fullscreen
            && !self.hovered
            && !self.dragging
            && !self.island.presentation().is_open()
            && self.activities.alarm().is_none()
            && !self.timer_ringing
    }

    /// "Hide for an hour" from the menu: like fullscreen, it still shows
    /// while pointed at, while open, and for alarms. (The deadline is
    /// checked by `tick`.)
    fn hidden_for_now(&self) -> bool {
        self.hidden_until.is_some()
            && !self.hovered
            && !self.dragging
            && !self.island.presentation().is_open()
            && self.activities.alarm().is_none()
            && !self.timer_ringing
    }

    fn update_shown(&mut self, t: f64) -> bool {
        let v = self.shown_target();
        if (self.shown.target() - v).abs() < 1e-9 {
            return false;
        }
        if self.reduce_motion {
            self.shown.snap(v);
        } else {
            self.shown.set_target(v, t, SHOW_SPRING);
        }
        true
    }

    /// History entries the user chose to copy again.
    pub fn take_clipboard_copies(&mut self) -> Vec<String> {
        std::mem::take(&mut self.clip_out)
    }

    pub fn prefs(&self) -> &IslandPrefs {
        &self.prefs
    }

    /// Apply a provider report. Returns true if a redraw is needed.
    pub fn report(&mut self, r: Report, t: f64) -> bool {
        // Hidden apps' sessions don't reach the island.
        let r = match r {
            Report::Media(Some(m)) if self.prefs.excludes(&m.app_name, &m.source) => Report::Media(None),
            r => r,
        };
        if let Report::Volume(level, muted) = r {
            self.volume = Some((level, muted));
            return self.island.presentation().is_open();
        }
        if let Report::Hud(Hud::Volume { level, muted }) = &r {
            self.volume = Some((*level, *muted));
            // The slider's own change: no pop-up over the card.
            if t - self.volume_set_at < 1.0 {
                return self.island.presentation().is_open();
            }
        }
        if let Report::Lyrics(key, l) = r {
            self.lyrics = Some((key, l));
            self.sync_live(t);
            return true;
        }
        if let Report::Clipboard(text) = &r {
            let changed = self.prefs.clipboard && self.clip.push(text);
            return changed && self.panel == Some(Panel::Clipboard) && self.island.presentation().is_open();
        }
        if let Report::Calendar(_) = &r {
            let changed = self.activities.report(r, t);
            return self.update_up_next(t) || self.sync_live(t) || changed;
        }
        if let Report::Hud(h) = &r {
            let on = match h {
                Hud::Volume { .. } => self.prefs.volume_hud,
                Hud::Brightness(_) => self.prefs.brightness_hud,
                Hud::CapsLock(_) => self.prefs.caps_lock_hud,
            };
            if !on {
                return false;
            }
        }
        let changed = self.activities.report(r, t);
        self.sync_live(t) || changed
    }

    /// Store the current wall-clock time. Returns true when the displayed
    /// minute changed while the clock is on screen (a redraw is needed).
    pub fn set_time(&mut self, now: LocalTime, t: f64) -> bool {
        let changed = self.clock.is_none_or(|c| (c.hour, c.minute, c.day) != (now.hour, now.minute, now.day));
        self.clock = Some(now);
        let alarm = changed && self.alarm_events(&now, t);
        (changed && self.clock_shown(t)) || alarm
    }

    fn live(&self, t: f64) -> Vec<Live> {
        let mut v = self.activities.live(t);
        if !self.prefs.calls {
            v.retain(|l| !matches!(l, Live::Call(_)));
        }
        // Pop-ups paused from the menu: brief notices wait, lasting things stay.
        if self.quiet_until.is_some_and(|q| t < q) {
            v.retain(|l| !matches!(l, Live::Hud(_) | Live::Battery { .. } | Live::Note(_) | Live::Event { .. }));
        }
        if v.len() > 1 {
            let n = self.rotation % v.len();
            v.rotate_left(n);
        }
        v
    }

    /// Scroll or two-finger swipe over the island: step to the next live
    /// activity once enough has accumulated (one step per gesture).
    fn scroll(&mut self, dx: f64, dy: f64, t: f64) -> bool {
        if self.activities.live(t).len() < 2 {
            return false;
        }
        self.scroll_acc += if dy.abs() >= dx.abs() { dy } else { dx };
        if self.scroll_acc.abs() < 1.0 || t - self.last_switch < SWITCH_COOLDOWN {
            return false;
        }
        self.scroll_acc = 0.0;
        self.last_switch = t;
        self.rotation = self.rotation.wrapping_add(1);
        let start = self.bounds(t);
        let p = self.island.presentation();
        self.set_targets(self.target_dims(p, t), spring_for(p, p), t);
        self.anim_region = start.union(&self.target_bounds());
        true
    }

    fn open_content(&self, live: &[Live]) -> OpenContent<'_> {
        self.open_content_as(live, self.home)
    }

    /// What the island opens on; `home`: Control Centre unless an alarm rings.
    fn open_content_as(&self, live: &[Live], home: bool) -> OpenContent<'_> {
        // A HUD is only a moment's notice: the card shows what is under it.
        let live = match live.first() {
            Some(Live::Hud(_)) => &live[1..],
            _ => live,
        };
        if let (Some(Live::Alarm(_)), Some(a)) = (live.first(), self.activities.alarm()) {
            OpenContent::Alarm(a)
        } else if self.menu {
            OpenContent::Menu
        } else if self.panel == Some(Panel::Stopwatch) {
            OpenContent::Stopwatch(self.activities.stopwatch())
        } else if let Some(p) = self.panel {
            OpenContent::Panel(p)
        } else if home {
            OpenContent::Clock
        } else if self.notes_open || matches!(live.first(), Some(Live::Note(_))) {
            OpenContent::Notes
        } else if let Some(Live::Event { .. }) = live.first() {
            OpenContent::Agenda
        } else if let (Some(Live::Custom(_)), Some(c)) = (live.first(), self.activities.custom().first()) {
            OpenContent::Custom(c)
        } else if let Some(Live::Call(c)) = live.first() {
            OpenContent::Call(c.clone())
        } else if let (Some(Live::Timer(_)), Some(tm)) = (live.first(), self.activities.timer()) {
            OpenContent::Timer(tm)
        } else if let (Some(Live::Stopwatch(_)), Some(sw)) = (live.first(), self.activities.stopwatch()) {
            OpenContent::Stopwatch(Some(sw))
        } else if matches!(live.first(), Some(Live::Battery { .. })) {
            OpenContent::Battery
        } else if let (Some(Live::Navigation(_)) | None, Some(n)) = (live.first(), self.activities.nav()) {
            OpenContent::Nav(n)
        } else if let Some(m) = self.activities.media() {
            OpenContent::Media(m)
        } else {
            OpenContent::Clock
        }
    }

    /// What the clock card shows besides the time.
    fn clock_row(&self) -> ClockRow {
        ClockRow {
            shelf: !self.shelf.is_empty(),
            clipboard: self.prefs.clipboard,
            pro: self.pro,
            todo_left: self.todo.left(),
            world: self.pro && self.prefs.world_clocks.iter().any(|c| islet_core::worldclock::find(c).is_some()),
        }
    }

    /// The chosen world clocks (Islet Pro): name, local time, days ahead.
    fn world_clocks(&self) -> Vec<(String, islet_core::clock::LocalTime, i32)> {
        if !self.pro {
            return Vec::new();
        }
        let here = (self.utc_offset / 60) as i32;
        self.prefs
            .world_clocks
            .iter()
            .take(islet_core::worldclock::MAX)
            .filter_map(|c| {
                let (name, _, _) = islet_core::worldclock::find(c)?;
                let (now, days) = islet_core::worldclock::city_time(name, self.wall as i64, here)?;
                Some((name.to_string(), now, days))
            })
            .collect()
    }

    /// The month on the calendar card.
    fn cal_month(&self) -> (i32, u8) {
        self.cal_view.or(self.clock.map(|now| (now.year, now.month))).unwrap_or((1970, 1))
    }

    /// The next alarm for the clock card: "7:30 · in 8 h 15 min".
    fn next_alarm_label(&self) -> Option<String> {
        let now = self.clock?;
        let (a, mins) = self.alarms.upcoming(&now)?;
        Some(format!("{} · {}", islet_core::alarm::format_hm(a.hour, a.minute, self.h24), content::until_label(mins)))
    }

    /// The next few events for the calendar card: (when, what).
    fn upcoming_events(&self) -> Vec<(String, String)> {
        let now = self.wall as i64;
        let today = (now + self.utc_offset).div_euclid(86_400);
        self.activities
            .calendar()
            .iter()
            .filter(|e| e.end > now)
            .take(4)
            .map(|e| {
                let local = e.start + if e.all_day { 0 } else { self.utc_offset };
                let day = local.div_euclid(86_400);
                let (_, month, d) = islet_core::calendar::civil_from_days(day);
                let date = match day - today {
                    i64::MIN..=0 => "Today".to_string(),
                    1 => "Tomorrow".to_string(),
                    _ => {
                        let name = islet_core::clock::month_name(month);
                        format!("{d} {}", &name[..name.len().min(3)])
                    }
                };
                let when = if e.all_day {
                    date
                } else {
                    let secs = local.rem_euclid(86_400);
                    let hm = islet_core::alarm::format_hm((secs / 3600) as u8, (secs / 60 % 60) as u8, self.h24);
                    format!("{date} · {hm}")
                };
                (when, e.summary.clone())
            })
            .collect()
    }

    fn stopwatch_card_open(&self, t: f64) -> bool {
        self.island.presentation().is_open() && matches!(self.open_content(&self.live(t)), OpenContent::Stopwatch(_))
    }

    fn clock_shown(&self, t: f64) -> bool {
        let card = (self.expanded_opacity(t) > 0.0 || self.island.presentation().is_open())
            && matches!(self.open_content(&self.live(t)), OpenContent::Clock);
        card || self.idle_clock_opacity(t, &self.live(t)) > 0.0
    }

    /// The time on the empty pill: on unless switched off, while nothing is
    /// live, fading as the pill grows into a card (and with the pill when it
    /// hides until hovered). Not on a notched Mac, where the notch hides it.
    fn idle_clock_opacity(&self, t: f64, live: &[Live]) -> f32 {
        if !self.prefs.idle_clock || self.notch.is_some() || self.clock.is_none() || !live.is_empty() {
            return 0.0;
        }
        let p = self.pill_rect(t);
        let shape = smooth((60.0 - p.h) / 16.0) * smooth((190.0 - p.w) / 40.0);
        let o = shape as f32;
        if o < 0.01 || self.shown.value(t) < 0.01 {
            0.0
        } else {
            o
        }
    }

    /// Expanded content fades and rises in as the pill grows taller, and out
    /// as it shrinks, so it never spills outside a closing island.
    pub fn expanded_opacity(&self, t: f64) -> f32 {
        // Fade over the last stretch of the card's growth (at most 60 px),
        // and never below 55 px so nothing shows in a compact-sized pill.
        let full = self.open_h.max(CONTENT_MIN_FULL_H);
        let from = (full - CONTENT_FADE_H).max(CONTENT_FROM_H);
        smooth((self.h.value(t) - from) / (full - from)) as f32
    }

    /// Compact content shows while the pill is wide and short.
    fn compact_opacity(&self, t: f64, live: &[Live]) -> f32 {
        let p = self.pill_rect(t);
        let full_w = match self.island.presentation() {
            Presentation::Minimal => target_size(Presentation::Minimal, None).0,
            _ => live.first().map_or(content::COMPACT_MEDIA_W, content::compact_width),
        };
        let span = (full_w.min(190.0) - IDLE_W - 6.0).max(1.0);
        (smooth((p.w - IDLE_W - 6.0) / span) * smooth((60.0 - p.h) / 20.0)) as f32
    }

    /// Whether anything is drawn inside the island right now (cheap check, so
    /// fonts are only loaded once there is something to show).
    pub fn has_content(&self, t: f64) -> bool {
        let live = self.live(t);
        self.expanded_opacity(t) > 0.0
            || self.compact_opacity(t, &live) > 0.0
            || self.privacy_opacity(t) > 0.0
            || self.idle_clock_opacity(t, &live) > 0.0
    }

    /// The camera / microphone dot shows while the pill is closed (idle or
    /// compact) and fades as it opens; the two-at-once layout has no room.
    fn privacy_opacity(&self, t: f64) -> f32 {
        if !self.privacy_shown() || self.island.presentation() == Presentation::Minimal {
            return 0.0;
        }
        smooth((56.0 - self.pill_rect(t).h) / 12.0) as f32
    }

    /// The camera / microphone dot is on (in use, and not switched off).
    /// A call filling the pill already shows the microphone, in orange.
    fn privacy_shown(&self) -> bool {
        let call_in_pill =
            self.prefs.calls && self.activities.privacy().camera.is_none() && self.activities.call().is_some();
        self.prefs.privacy_dots && self.activities.privacy().active() && !call_in_pill
    }

    /// Green for the camera (it takes precedence), orange for the microphone.
    fn privacy_dot(&self, p: Rect, o: f32) -> Element {
        let color = if self.activities.privacy().camera.is_some() { content::GREEN } else { content::ORANGE };
        let d = PRIVACY_DOT;
        let (cx, cy) = (p.right() as f32 - PRIVACY_INSET, (p.y + p.h / 2.0) as f32);
        Element::Bar { x: cx - d / 2.0, y: cy - d / 2.0, w: d, h: d, color, opacity: o }
    }

    /// Everything drawn inside the island this frame.
    pub fn elements(&self, t: f64, engine: &mut TextEngine) -> Vec<Element> {
        let live = self.live(t);
        let p = self.pill_rect(t);
        let mut out = Vec::new();
        let eo = self.expanded_opacity(t);
        let cr = self.content_rect(p);
        if eo > 0.0 {
            let open = self.open_content(&live);
            // Activity cards: "‹ Control Centre" on top, the card below it.
            let card = if Self::has_header(&open) {
                out.extend(content::home_button_elements(cr, eo, engine));
                content::below_header(cr)
            } else {
                cr
            };
            match open {
                OpenContent::Menu => out.extend(content::expanded_menu(cr, &self.menu_items(t), eo, engine)),
                OpenContent::Call(c) => out.extend(content::expanded_call(card, &c, t, eo, engine)),
                OpenContent::Alarm(a) => out.extend(content::expanded_alarm(cr, a, self.h24, t, eo, engine)),
                OpenContent::Timer(tm) => out.extend(content::expanded_timer(card, tm, t, eo, engine)),
                OpenContent::Stopwatch(sw) => out.extend(content::expanded_stopwatch(card, sw, t, eo, engine)),
                OpenContent::Agenda => out.extend(content::expanded_agenda(card, &self.agenda(), eo, engine)),
                OpenContent::Custom(c) => out.extend(content::expanded_custom(card, c, t, eo, engine)),
                OpenContent::Notes => out.extend(content::expanded_notes(card, self.activities.notes(), eo, engine)),
                OpenContent::Panel(Panel::Wifi) => out.extend(content::expanded_wifi(
                    cr,
                    self.activities.connectivity(),
                    self.scroll_of(Panel::Wifi),
                    eo,
                    engine,
                )),
                OpenContent::Panel(Panel::Bluetooth) => out.extend(content::expanded_bluetooth(
                    cr,
                    self.activities.connectivity(),
                    self.scroll_of(Panel::Bluetooth),
                    eo,
                    engine,
                )),
                OpenContent::Panel(Panel::Clipboard) => out.extend(content::expanded_clipboard(
                    cr,
                    &self.clip,
                    self.scroll_of(Panel::Clipboard),
                    eo,
                    engine,
                )),
                OpenContent::Panel(Panel::System) => {
                    out.extend(content::expanded_system(cr, self.activities.system(), eo, engine))
                }
                // Shown as OpenContent::Stopwatch instead.
                OpenContent::Panel(Panel::Stopwatch) => {}
                OpenContent::Panel(Panel::Calendar) => {
                    let (year, month) = self.cal_month();
                    let days =
                        islet_core::calendar::event_days(self.activities.calendar(), year, month, self.utc_offset);
                    let today = self.clock.unwrap_or(islet_core::clock::LocalTime {
                        year: 1970,
                        month: 1,
                        day: 1,
                        weekday: 4,
                        hour: 0,
                        minute: 0,
                        second: 0,
                    });
                    out.extend(content::expanded_calendar(
                        cr,
                        (year, month),
                        &today,
                        &days,
                        &self.upcoming_events(),
                        eo,
                        engine,
                    ));
                }
                OpenContent::Panel(Panel::Todo) => {
                    out.extend(content::expanded_todo(cr, &self.todo, self.scroll_of(Panel::Todo), eo, engine))
                }
                OpenContent::Panel(Panel::Shelf) => out.extend(content::expanded_shelf(
                    cr,
                    &self.shelf,
                    self.dragging,
                    self.scroll_of(Panel::Shelf),
                    eo,
                    engine,
                )),
                OpenContent::Media(m) => {
                    out.extend(content::expanded_media(card, m, t, eo, self.scrub, self.volume, engine));
                    if let Some(l) = self.song_lyrics() {
                        let pos = self.lyrics_pos(m, t);
                        out.extend(content::card_lyrics(card, l, pos, eo, engine));
                    }
                }
                OpenContent::Nav(n) => out.extend(content::expanded_nav(card, n, self.clock.as_ref(), eo, engine)),
                OpenContent::Battery => out.extend(content::expanded_battery(
                    card,
                    &self.activities.devices(),
                    self.activities.laptop(),
                    eo,
                    engine,
                )),
                OpenContent::Clock => {
                    if let Some(now) = self.clock {
                        let alarm = self.next_alarm_label();
                        out.extend(content::expanded_clock(cr, &now, self.h24, alarm.as_deref(), eo, engine));
                    }
                    let minutes = content::QUICK_MINUTES[self.quick];
                    out.extend(content::quick_row_elements(cr, self.clock_row(), minutes, eo));
                    let world = self.world_clocks();
                    if !world.is_empty() {
                        out.extend(content::world_clocks(cr, &world, self.h24, eo, engine));
                    }
                    out.extend(content::clock_chip_elements(
                        cr,
                        self.activities.connectivity(),
                        self.shelf.len(),
                        self.clock_row(),
                        eo,
                        engine,
                    ));
                    if let Some(w) = self.activities.weather() {
                        out.extend(content::clock_weather(cr, w, self.prefs.fahrenheit, eo));
                    }
                    if self.home {
                        let label = self.activity_label(&live);
                        out.extend(content::return_button_elements(cr, &label, eo, engine));
                    }
                }
            }
        }
        let co = self.compact_opacity(t, &live);
        if co > 0.0 {
            // While opening or closing, keep the layout of the closed state.
            let shape = match self.island.presentation() {
                p if p.is_open() => self.closed,
                p => p,
            };
            match (shape, live.first()) {
                (Presentation::Minimal, Some(first)) => {
                    // The lasting activity (music) keeps the pill; a transient
                    // battery peek is the one that buds off and merges back.
                    let (pill, bubble) = match (first, live.get(1)) {
                        (Live::Battery { .. }, Some(second)) if self.rotation == 0 => (second, Some(first)),
                        _ => (first, live.get(1)),
                    };
                    out.extend(content::minimal(p, self.bubble_circle(t), pill, bubble, t, co, self.h24));
                }
                (_, Some(first)) => {
                    // Leave the right end to the camera / microphone dot.
                    let room = if self.privacy_shown() { PRIVACY_W } else { 0.0 };
                    let inner = Rect::new(p.x, p.y, (p.w - room).max(1.0), p.h);
                    match (first, self.song_lyrics()) {
                        (Live::Media(m), Some(l)) if self.lyrics_in_bar(&live) => {
                            out.extend(content::compact_lyrics(inner, m, l, self.lyrics_pos(m, t), t, co, engine))
                        }
                        _ => out.extend(content::compact(inner, first, t, co, engine)),
                    }
                }
                _ => {}
            }
        }
        let io = self.idle_clock_opacity(t, &live);
        if let (true, Some(now)) = (io > 0.0, self.clock) {
            // Centred in what the camera / microphone dot leaves free.
            let room = if self.privacy_shown() { PRIVACY_W } else { 0.0 };
            let inner = Rect::new(p.x, p.y, (p.w - room).max(1.0), p.h);
            out.extend(content::idle_clock(inner, &now, self.h24, io, engine));
        }
        let po = self.privacy_opacity(t);
        if po > 0.0 {
            out.push(self.privacy_dot(p, po));
        }
        // The island fading out (hover-only, fullscreen): its content too.
        let shown = self.shown.value(t).clamp(0.0, 1.0) as f32;
        if shown < 0.01 {
            return Vec::new();
        }
        if shown < 0.999 {
            out = out.into_iter().map(|e| e.faded(shown)).collect();
        }
        out
    }

    /// Size for a presentation, adapted to what it shows.
    fn target_dims(&self, p: Presentation, t: f64) -> (f64, f64, f64) {
        let (w, h, r) = target_size(p, None);
        let live = self.live(t);
        let dims = match p {
            Presentation::Compact => {
                let room = if self.privacy_shown() { PRIVACY_W } else { 0.0 };
                let cw = if self.lyrics_in_bar(&live) {
                    content::COMPACT_LYRICS_W
                } else {
                    live.first().map_or(w, content::compact_width)
                };
                (cw + room, h, r)
            }
            Presentation::Expanded => {
                let open = self.open_content(&live);
                let header = if Self::has_header(&open) { content::CARD_HEADER_H } else { 0.0 };
                let h = header
                    + match open {
                        OpenContent::Menu => content::menu_height(self.menu_items(t).len()),
                        OpenContent::Call(_) => content::CALL_H,
                        OpenContent::Alarm(_) => content::ALARM_H,
                        OpenContent::Timer(_) => content::TIMER_H,
                        OpenContent::Stopwatch(_) => content::STOPWATCH_H,
                        OpenContent::Agenda => content::agenda_height(self.agenda().len()),
                        OpenContent::Custom(_) => content::CUSTOM_H,
                        OpenContent::Notes => content::notes_height(self.activities.notes().count()),
                        OpenContent::Panel(p) => {
                            let c = self.activities.connectivity();
                            match p {
                                Panel::Wifi => {
                                    content::list_height(c.wifi.networks.len(), content::wifi_footer(c).is_some())
                                }
                                Panel::Bluetooth => {
                                    content::list_height(c.bt.devices.len(), content::bt_footer(c).is_some())
                                }
                                Panel::System => content::SYSTEM_H,
                                Panel::Stopwatch => content::STOPWATCH_H,
                                Panel::Todo => content::list_height(self.todo.items.len(), true),
                                Panel::Calendar => content::CALENDAR_H,
                                Panel::Clipboard => content::list_height(self.clip.len(), !self.clip.is_empty()),
                                Panel::Shelf => content::list_height(
                                    self.shelf.len(),
                                    content::shelf_footer(self.shelf.len(), self.dragging).is_some(),
                                ),
                            }
                        }
                        OpenContent::Media(_) if self.song_lyrics().is_some() => MEDIA_H + content::CARD_LYRICS_H,
                        OpenContent::Media(_) | OpenContent::Nav(_) => MEDIA_H,
                        OpenContent::Clock => CLOCK_H + if self.clock_row().world { content::WORLD_H } else { 0.0 },
                        OpenContent::Battery => {
                            let rows =
                                self.activities.devices().len() + usize::from(self.activities.laptop().is_some());
                            content::battery_list_height(rows)
                        }
                    };
                (w, h, r)
            }
            _ => (w, h, r),
        };
        match self.notch {
            None => dims,
            Some((nw, nh)) => notch_dims(p, dims, nw, nh),
        }
    }

    fn set_targets(&mut self, (w, h, r): (f64, f64, f64), spring: Spring, t: f64) {
        for (a, v) in [(&mut self.w, w), (&mut self.h, h), (&mut self.r, r)] {
            if self.reduce_motion {
                a.snap(v);
            } else if (a.target() - v).abs() > 1e-9 {
                a.set_target(v, t, spring);
            }
        }
    }

    /// Feed a signal to the state machine and retarget the springs.
    fn signal(&mut self, s: Signal, t: f64) -> bool {
        let from = self.island.presentation();
        let Some(to) = self.island.apply(s) else { return false };
        let spring = spring_for(from, to);
        let start = self.bounds(t);
        let dims = self.target_dims(to, t);
        self.set_targets(dims, spring, t);
        if to.is_open() {
            self.open_h = dims.1;
        } else {
            self.closed = to;
        }
        // The bubble buds off with the bouncy split spring but is absorbed
        // quickly, before the pill has grown much (as on iOS).
        let (bubble, bubble_spring) =
            if to == Presentation::Minimal { (BUBBLE_OFFSET, spring) } else { (0.0, BUBBLE_MERGE) };
        if self.reduce_motion {
            self.bubble.snap(bubble);
        } else if self.bubble.target() != bubble {
            self.bubble.set_target(bubble, t, bubble_spring);
        }
        self.anim_region = start.union(&self.target_bounds());
        if !to.is_open() {
            self.left_open_at = None;
            self.control = None;
            self.scrub = None;
            self.panel = None;
            self.list_scroll = 0;
            self.home = false;
            self.menu = false;
            self.notes_open = false;
        } else if !from.is_open() && matches!(self.open_content(&self.live(t)), OpenContent::Notes) {
            self.notes_open = true;
        } else if !from.is_open() && matches!(self.open_content(&self.live(t)), OpenContent::Clock) {
            // Show the current network and device on the clock card's buttons.
            self.conn_commands.push(ConnCommand::Status);
        }
        self.update_shown(t);
        true
    }

    /// Bring the presentation and size in line with what is live.
    fn sync_live(&mut self, t: f64) -> bool {
        let n = self.live(t).len();
        if n != self.live_count {
            self.live_count = n;
            self.rotation = 0; // a new set of activities starts from the top
        }
        let mut changed = self.signal(Signal::LiveActivities(n), t);
        // Same presentation, new content size (a battery peek handing over to
        // media): glide to the new size.
        let p = self.island.presentation();
        let dims = self.target_dims(p, t);
        if (self.w.target() - dims.0).abs() > 0.5 || (self.h.target() - dims.1).abs() > 0.5 {
            let start = self.bounds(t);
            self.set_targets(dims, spring_for(p, p), t);
            if p.is_open() {
                self.open_h = dims.1;
            }
            self.anim_region = start.union(&self.target_bounds());
            changed = true;
        }
        changed
    }

    fn pointer_at(&mut self, x: f64, y: f64, t: f64) -> bool {
        let inside = self.hit(x, y, t);
        if inside == self.hovered {
            return false;
        }
        self.hovered = inside;
        let changed = if inside {
            self.left_open_at = None;
            self.signal(Signal::PointerEnter, t)
        } else {
            if self.island.presentation().is_open() {
                self.left_open_at = Some(t);
            }
            self.press = None;
            self.signal(Signal::PointerLeave, t)
        };
        // A hidden island (fullscreen video) shows while the pointer is on it.
        self.update_shown(t) || changed
    }

    /// The media control under (x, y) on an open "Now Playing" card.
    fn control_at(&self, x: f64, y: f64, t: f64) -> Option<Control> {
        if !self.island.presentation().is_open() {
            return None;
        }
        if !matches!(self.open_content(&self.live(t)), OpenContent::Media(_)) {
            return None;
        }
        let cr = self.card_rect(t);
        content::media_controls(cr, self.volume.is_some()).into_iter().find(|(r, _)| r.contains(x, y)).map(|(_, c)| c)
    }

    /// Returns true when the event changed what is on screen.
    pub fn handle(&mut self, e: Event, t: f64) -> bool {
        match e {
            Event::PointerMove { x, .. } if self.control == Some(Control::Volume) => self.volume_from(x, t),
            Event::PointerMove { x, .. } if self.control == Some(Control::Scrub) => {
                self.scrub = Some(content::scrub_fraction(self.card_rect(t), x));
                true
            }
            Event::PointerMove { x, y } => self.pointer_at(x, y, t),
            Event::PointerLeave => self.pointer_at(f64::NAN, f64::NAN, t),
            Event::PointerDown { x, y, button: Button::Primary } if self.hit(x, y, t) => {
                if let Some(c) = self.timer_control_at(x, y, t) {
                    self.timer_control = Some(c);
                    return true;
                }
                if let Some(c) = self.stopwatch_control_at(x, y, t) {
                    self.stopwatch_control = Some(c);
                    return true;
                }
                if self.call_mute_at(x, y, t) {
                    self.call_press = true;
                    return true;
                }
                if let Some(h) = self.conn_hit_at(x, y, t) {
                    self.conn_hit = Some(h);
                    return true;
                }
                if let Some(c) = self.alarm_control_at(x, y, t) {
                    self.alarm_control = Some(c);
                    return true;
                }
                if let Some(c) = self.nav_control_at(x, y, t) {
                    self.nav_control = Some(c);
                    return true;
                }
                match self.control_at(x, y, t) {
                    Some(c) => {
                        self.control = Some(c);
                        if c == Control::Volume {
                            self.volume_from(x, t);
                        }
                        if c == Control::Scrub && self.activities.media().is_some_and(|m| m.caps.seek) {
                            self.scrub = Some(content::scrub_fraction(self.card_rect(t), x));
                        }
                        true
                    }
                    None => {
                        self.press = Some(Press { at: t, long_fired: false });
                        false
                    }
                }
            }
            Event::PointerUp { x, y, button: Button::Primary } => {
                if std::mem::take(&mut self.call_press) {
                    if self.call_mute_at(x, y, t) {
                        return self.toggle_mic(t);
                    }
                    return true;
                }
                if let Some(c) = self.stopwatch_control.take() {
                    if self.stopwatch_control_at(x, y, t) != Some(c) {
                        return true;
                    }
                    let (exists, running) = match &self.stopwatch {
                        Some(sw) => (true, sw.running()),
                        None => (false, false),
                    };
                    let cmd = match c {
                        StopwatchControl::LapReset if running => StopwatchCommand::Lap,
                        StopwatchControl::LapReset if exists => StopwatchCommand::Reset,
                        StopwatchControl::LapReset => return true,
                        StopwatchControl::StartPause if running => StopwatchCommand::Pause,
                        StopwatchControl::StartPause => StopwatchCommand::Start,
                    };
                    if cmd == StopwatchCommand::Start && self.panel == Some(Panel::Stopwatch) {
                        // From Control Centre: once started it is a live activity.
                        self.panel = None;
                    }
                    return self.stopwatch_command(cmd, t);
                }
                if let Some(c) = self.timer_control.take() {
                    if self.timer_control_at(x, y, t) != Some(c) {
                        return true;
                    }
                    let running = self.timer.as_ref().is_some_and(|tm| tm.running());
                    let cmd = match c {
                        TimerControl::PauseResume if running => TimerCommand::Pause,
                        TimerControl::PauseResume => TimerCommand::Resume,
                        TimerControl::Stop => TimerCommand::Stop,
                    };
                    return self.timer_command(cmd, t);
                }
                if let Some(h) = self.conn_hit.take() {
                    return if self.conn_hit_at(x, y, t) == Some(h) { self.conn_act(h, t) } else { true };
                }
                if let Some(c) = self.alarm_control.take() {
                    return match c {
                        _ if self.alarm_control_at(x, y, t) != Some(c) => true, // released elsewhere
                        AlarmControl::Snooze => self.snooze_alarm(t),
                        AlarmControl::Stop => self.stop_alarm(t),
                    };
                }
                if let Some(c) = self.nav_control.take() {
                    if self.nav_control_at(x, y, t) == Some(c) {
                        self.nav_actions.push(c);
                    }
                    return true;
                }
                if let Some(c) = self.control.take() {
                    let scrub = self.scrub.take();
                    let media = self.activities.media();
                    let command = match c {
                        Control::Volume => None,
                        Control::Scrub => {
                            scrub.zip(media.and_then(|m| m.duration)).map(|(f, d)| MediaCommand::SeekTo(f * d))
                        }
                        _ if self.control_at(x, y, t) != Some(c) => None, // released elsewhere
                        Control::PlayPause => Some(MediaCommand::PlayPause),
                        Control::Next => Some(MediaCommand::Next),
                        Control::Previous => Some(MediaCommand::Previous),
                    };
                    self.commands.extend(command);
                    return true;
                }
                match self.press.take() {
                    Some(p) if !p.long_fired => {
                        // A finished download (or a script's link): tapping its card opens it.
                        if self.island.presentation().is_open() {
                            if let OpenContent::Custom(c) = self.open_content(&self.live(t)) {
                                if let Some(target) = c.open.clone() {
                                    self.ui_actions.push(UiAction::Path(target));
                                }
                            }
                        }
                        self.signal(Signal::Click, t)
                    }
                    _ => false,
                }
            }
            // Right-click: the menu (Settings, hide, pause pop-ups, lyrics, quit).
            Event::PointerDown { x, y, button: Button::Secondary } if self.hit(x, y, t) => {
                self.press = None;
                if self.menu && self.island.presentation().is_open() {
                    self.menu = false;
                    return self.signal(Signal::Escape, t);
                }
                self.menu = true;
                self.panel = None;
                self.home = false;
                if self.island.presentation().is_open() {
                    self.sync_live(t);
                    true
                } else {
                    self.signal(Signal::Click, t)
                }
            }
            Event::Key(Key::Escape) => self.signal(Signal::Escape, t),
            Event::Hotkey => self.toggle(t),
            Event::Fullscreen(on) => {
                self.fullscreen = on;
                self.update_shown(t)
            }
            Event::CapsLock(on) => self.report(Report::Hud(Hud::CapsLock(on)), t),
            Event::DragOver { x, y } => self.drag_over(x, y, t),
            Event::A11yPress(button) => self.a11y_press(button, t),
            Event::DragLeave => self.drag_over(f64::NAN, f64::NAN, t),
            Event::Scroll { dy, .. }
                if self.hovered && self.island.presentation().is_open() && self.panel.is_some() =>
            {
                let p = self.panel.unwrap_or(Panel::Wifi);
                self.scroll_list(p, dy)
            }
            Event::Scroll { dx, dy } if self.hovered => self.scroll(dx, dy, t),
            Event::Redraw | Event::ScaleChanged(_) => true,
            _ => false,
        }
    }

    /// Advance timers. Returns true if anything changed.
    pub fn tick(&mut self, t: f64) -> bool {
        let mut changed = self.timer_tick(t);
        if self.hidden_until.is_some_and(|h| t >= h) {
            self.hidden_until = None;
            changed = true;
        }
        if self.quiet_until.is_some_and(|q| t >= q) {
            self.quiet_until = None;
            changed = true;
        }
        changed |= self.sync_live(t);
        changed |= self.update_shown(t);
        // A running stopwatch redraws as its shown digits change: tenths on
        // its card, seconds on the pill.
        let shown = match &self.stopwatch {
            Some(sw) if sw.running() && self.shown.value(t) > 0.01 => {
                let e = sw.elapsed(t);
                if self.stopwatch_card_open(t) {
                    (e * 10.0).floor() as i64 * 2 + 1
                } else {
                    e.floor() as i64 * 2
                }
            }
            _ => -1,
        };
        if shown != self.stopwatch_shown {
            self.stopwatch_shown = shown;
            changed = true;
        }
        // A call on screen shows its length: redraw as each second passes.
        let sec = match self.live(t).iter().find_map(|l| if let Live::Call(c) = l { Some(c.since) } else { None }) {
            Some(since) if self.shown.value(t) > 0.01 => (t - since).floor() as i64,
            _ => -1,
        };
        if sec != self.call_sec {
            self.call_sec = sec;
            changed = true;
        }
        if std::mem::take(&mut self.pending_peek) && !self.island.presentation().is_open() {
            // A focus phase changed: open briefly, then close by itself.
            changed |= self.signal(Signal::Click, t);
            self.left_open_at = Some(t);
        }
        if let Some(p) = &mut self.press {
            if !p.long_fired && t - p.at >= LONG_PRESS_SECS {
                p.long_fired = true;
                changed |= self.signal(Signal::LongPress, t);
            }
        }
        if let Some(left) = self.left_open_at {
            if t - left >= AUTO_COLLAPSE_SECS {
                self.left_open_at = None;
                changed |= self.signal(Signal::AutoCollapse, t);
            }
        }
        let signals = self.demo.signals();
        if !signals.is_empty() && t >= self.demo_next {
            self.demo_next += self.demo.period();
            let s = signals[self.demo_step % signals.len()];
            self.demo_step += 1;
            changed |= self.signal(s, t);
        }
        changed
    }

    pub fn animating(&self, t: f64) -> bool {
        !(self.w.is_settled(t)
            && self.h.is_settled(t)
            && self.r.is_settled(t)
            && self.bubble.is_settled(t)
            && self.shown.is_settled(t))
    }

    /// How often to draw while something moves on its own: 60 fps for the
    /// shape, 30 fps for a waveform; None when nothing moves (zero wakeups).
    pub fn frame_interval(&self, t: f64) -> Option<Duration> {
        if self.animating(t) {
            return Some(FRAME);
        }
        // Hidden (over a fullscreen video, say): a waveform nobody sees
        // mustn't keep waking the computer.
        if self.shown.value(t) < 0.01 {
            return None;
        }
        let live = self.live(t);
        // An activity without a known amount animates its ring while shown.
        if self.activities.custom().first().is_some_and(|c| c.progress.is_none())
            && matches!(live.first(), Some(Live::Custom(_)))
            && self.has_content(t)
        {
            return Some(WAVE_FRAME);
        }
        // The bell swings while an alarm rings and is on screen.
        if self.activities.alarm().is_some() && self.has_content(t) {
            return Some(WAVE_FRAME);
        }
        let playing = self.activities.media().is_some_and(|m| m.playing);
        // Lyrics glide at 60 fps while they move: sliding to the next line,
        // or scrolling across one too long for the bar (about 7 px a letter).
        if let (true, Some(l), Some(m)) = (playing, self.song_lyrics(), self.activities.media()) {
            let pos = self.lyrics_pos(m, t);
            let long = self.lyrics_in_bar(&live)
                && l.current(pos)
                    .is_some_and(|i| l.text(i).chars().count() as f64 * 7.0 > content::COMPACT_LYRICS_W - 90.0);
            if (islet_core::lyrics::sliding(l, pos) || long) && self.has_content(t) {
                return Some(FRAME);
            }
        }
        let compact_wave = matches!(live.first(), Some(Live::Media(_))) && self.compact_opacity(t, &live) > 0.0;
        let card_wave = self.expanded_opacity(t) > 0.0 && matches!(self.open_content(&live), OpenContent::Media(_));
        (playing && (compact_wave || card_wave)).then_some(WAVE_FRAME)
    }

    /// Seconds from `t` until the next timer, if any.
    pub fn next_timer(&self, t: f64) -> Option<f64> {
        let mut next: Option<f64> = None;
        let mut at = |deadline: f64| next = Some(next.map_or(deadline, |n: f64| n.min(deadline)));
        if let Some(p) = &self.press {
            if !p.long_fired {
                at(p.at + LONG_PRESS_SECS);
            }
        }
        if let Some(left) = self.left_open_at {
            at(left + AUTO_COLLAPSE_SECS);
        }
        if self.demo != Demo::None {
            at(self.demo_next);
        }
        if let Some(change) = self.activities.next_change(t) {
            at(change);
        }
        // The clock wakes the island once a minute, and only while it is shown.
        if let Some(now) = self.clock.filter(|_| self.clock_shown(t)) {
            at(t + secs_to_next_minute(&now));
        }
        // A call on screen ticks each second.
        if let Some(since) = self.live(t).iter().find_map(|l| if let Live::Call(c) = l { Some(c.since) } else { None })
        {
            if self.shown.value(t) > 0.01 {
                at(since + (t - since).floor() + 1.0);
            }
        }
        if let Some(sw) = self.stopwatch.as_ref().filter(|_| self.shown.value(t) > 0.01) {
            if let Some(next) = sw.next_tick(t, self.stopwatch_card_open(t)) {
                at(next);
            }
        }
        // A running timer redraws as each second passes (and ends on time).
        if let Some(tm) = self.timer.as_ref().filter(|tm| tm.running()) {
            let rem = tm.remaining(t);
            at(t + (rem - rem.ceil() + 1.0).min(rem).max(0.01));
        }
        // Calendar: wake exactly when "up next" appears or goes, and each
        // minute while it shows (its "in 8 min" label).
        if !self.activities.calendar().is_empty() {
            if let Some(change) = islet_core::calendar::next_change(self.activities.calendar(), self.wall as i64) {
                at(t + (change as f64 - self.wall).max(0.0));
            }
            if let Some(Live::Event { starts_at, .. }) =
                self.activities.live(t).iter().find(|l| matches!(l, Live::Event { .. }))
            {
                let rem = starts_at - t;
                if rem > 0.0 {
                    at(t + rem.rem_euclid(60.0) + 0.05);
                }
            }
        }
        for deadline in [self.hidden_until, self.quiet_until].into_iter().flatten() {
            at(deadline);
        }
        // Alarms are checked each minute, only while one is set.
        if let Some(secs) = self.clock.and_then(|now| self.alarms.next_check(&now)) {
            at(t + secs);
        }
        next.map(|n| (n - t).max(0.0))
    }

    fn pill_at(&self, w: f64, h: f64) -> Rect {
        Rect::new((WINDOW.0 - w) / 2.0, self.top, w, h)
    }

    /// Where the pill is heading. Buttons are hit-tested here, so a tap
    /// lands on its button while the card is still settling instead of
    /// counting as a tap on the card (which closes it).
    fn settled_pill(&self) -> Rect {
        self.pill_at(self.w.target().max(0.0), self.h.target().max(0.0))
    }

    pub fn pill_rect(&self, t: f64) -> Rect {
        self.pill_at(self.w.value(t).max(0.0), self.h.value(t).max(0.0))
    }

    /// The detached circle, while it is (or is becoming) visible.
    fn bubble_circle(&self, t: f64) -> Option<Circle> {
        let p = self.pill_rect(t);
        // The circle only separates while the pill is capsule-height: it is
        // pulled in as the pill grows into a card and buds off only once a
        // card has shrunk back, so it never pokes out of a big corner.
        let c = ((BUBBLE_MAX_H - p.h) / (BUBBLE_MAX_H - MINIMAL_CIRCLE)).clamp(0.0, 1.0);
        let off = self.bubble.value(t) * c * c * (3.0 - 2.0 * c);
        if off.abs() < 0.25 {
            return None; // merged: hidden inside the pill
        }
        // Anchored on the centre of the top-right corner's circle, so a merged
        // bubble is always fully inside the outline.
        let corner = self.r.value(t).max(0.0).min(p.w / 2.0).min(p.h / 2.0);
        let r = MINIMAL_CIRCLE / 2.0;
        Some(Circle { x: (p.right() - corner + off) as f32, y: (p.y + corner.max(r)) as f32, r: r as f32 })
    }

    /// Pill plus bubble.
    fn bounds(&self, t: f64) -> Rect {
        let pill = self.pill_rect(t);
        match self.bubble_circle(t) {
            Some(c) => pill.union(&circle_rect(c)),
            None => pill,
        }
    }

    fn target_bounds(&self) -> Rect {
        let pill = self.pill_at(self.w.target(), self.h.target());
        if self.bubble.target() > 0.0 {
            let r = MINIMAL_CIRCLE / 2.0;
            let cx = pill.right() - r + self.bubble.target();
            pill.union(&Rect::new(cx - r, pill.y, 2.0 * r, 2.0 * r))
        } else {
            pill
        }
    }

    /// Is (x, y) on the island (pill or bubble)?
    fn hit(&self, x: f64, y: f64, t: f64) -> bool {
        if !(x.is_finite() && y.is_finite()) {
            return false;
        }
        let in_bubble = self.bubble_circle(t).is_some_and(|c| {
            let (dx, dy) = (x - c.x as f64, y - c.y as f64);
            dx * dx + dy * dy <= (c.r as f64) * (c.r as f64)
        });
        self.pill_rect(t).contains(x, y) || in_bubble
    }

    /// Where pointer input should be accepted. While animating it covers both
    /// ends of the animation, so it changes twice per transition, not per frame.
    pub fn input_region(&self, t: f64) -> Rect {
        let r = if self.animating(t) { self.anim_region } else { self.target_bounds() };
        r.inflate(1.0)
    }

    pub fn frame(&self, t: f64) -> PillFrame {
        let r = self.pill_rect(t);
        PillFrame {
            shape: Squircle {
                x: r.x as f32,
                y: r.y as f32,
                w: r.w as f32,
                h: r.h as f32,
                radius: self.r.value(t).max(0.0) as f32,
                smoothing: SMOOTHING,
            },
            bubble: self.bubble_circle(t),
            highlight: 0.0,
            flat_top: self.notch.is_some(),
            opacity: self.shown.value(t).clamp(0.0, 1.0) as f32,
        }
    }
}

/// Space beside the notch for compact content (art on the left, waveform
/// or ring on the right); the notch itself hides whatever is drawn there.
const NOTCH_SIDE: f64 = 46.0;
/// The physical notch's bottom corner radius (approximate).
const NOTCH_R: f64 = 9.0;

/// Sizes when wrapping a notch of `nw` × `nh`.
fn notch_dims(p: Presentation, (w, h, r): (f64, f64, f64), nw: f64, nh: f64) -> (f64, f64, f64) {
    match p {
        Presentation::Idle => (nw, nh, NOTCH_R),
        Presentation::Peek => (nw + 12.0, nh + 3.0, NOTCH_R + 2.0),
        Presentation::Compact => (w.max(nw + 2.0 * NOTCH_SIDE), nh, NOTCH_R + 4.0),
        Presentation::Minimal => (nw + 2.0 * NOTCH_SIDE - 12.0, nh, NOTCH_R + 4.0),
        Presentation::Expanded | Presentation::Large => (w.max(nw + 40.0), h + nh, r),
    }
}

fn circle_rect(c: Circle) -> Rect {
    let (x, y, r) = (c.x as f64, c.y as f64, c.r as f64);
    Rect::new(x - r, y - r, 2.0 * r, 2.0 * r)
}

#[cfg(test)]
mod tests {
    use super::*;
    use islet_core::activity::{BatteryDevice, DeviceKind, MediaCaps, PEEK_CONNECTED};
    use Presentation::*;

    fn centre(app: &App, t: f64) -> (f64, f64) {
        let r = app.pill_rect(t);
        (r.x + r.w / 2.0, r.y + r.h / 2.0)
    }

    fn click(app: &mut App, t: f64) {
        let (x, y) = centre(app, t);
        app.handle(Event::PointerDown { x, y, button: Button::Primary }, t);
        app.handle(Event::PointerUp { x, y, button: Button::Primary }, t + 0.05);
    }

    fn texts(app: &App, t: f64, engine: &mut TextEngine) -> Vec<String> {
        app.elements(t, engine)
            .into_iter()
            .filter_map(|e| if let Element::Text(i) = e { Some(i.text) } else { None })
            .collect()
    }

    fn song(playing: bool) -> MediaInfo {
        MediaInfo {
            source: "test".into(),
            app_name: "Player".into(),
            title: "Song".into(),
            artist: "Artist".into(),
            playing,
            position: Some(30.0),
            position_at: 0.0,
            duration: Some(180.0),
            rate: 1.0,
            caps: MediaCaps { play_pause: true, next: true, previous: true, seek: true },
            ..MediaInfo::default()
        }
    }

    fn buds() -> BatteryDevice {
        BatteryDevice {
            id: "b".into(),
            name: "Earbuds".into(),
            kind: DeviceKind::Earbuds,
            percent: 87,
            charging: false,
            parts: vec![],
        }
    }

    const NOW: LocalTime = LocalTime { year: 2026, month: 11, day: 4, weekday: 3, hour: 14, minute: 5, second: 42 };

    fn lyrics() -> Arc<islet_core::lyrics::Lyrics> {
        Arc::new(islet_core::lyrics::parse_lrc(
            "[00:29.00]Before the light\n[00:31.00]Hold on to the night\n[00:35.00]Every word you sing\n[00:40.00]Fades into the sky\n",
        ))
    }

    fn lyrics_on() -> IslandPrefs {
        IslandPrefs { lyrics: true, ..IslandPrefs::default() }
    }

    /// Lyric lines on the strip: (text, opacity).
    fn strip_lines(app: &App, t: f64, engine: &mut TextEngine) -> Vec<(String, f32)> {
        app.elements(t, engine)
            .into_iter()
            .filter_map(|e| match e {
                Element::Text(i) if i.window.is_some() => Some((i.text, i.style.opacity)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn synced_lyrics_run_through_the_bar() {
        let mut engine = TextEngine::new();
        let mut app = App::new(Demo::None, false);
        app.report(Report::Media(Some(song(true))), 0.0);
        assert_eq!(app.lyrics_wanted(), None, "nothing is looked up until lyrics are turned on");
        app.set_prefs(lyrics_on());
        let want = app.lyrics_wanted().expect("the playing song");
        assert_eq!((want.title.as_str(), want.artist.as_str(), want.duration), ("Song", "Artist", Some(180)));

        // Another song's lyrics don't show.
        app.report(Report::Lyrics("someone\u{1f}else".into(), Some(lyrics())), 1.0);
        assert!((app.pill_rect(3.0).w - content::COMPACT_MEDIA_W).abs() < 0.5);
        assert!(strip_lines(&app, 3.0, &mut engine).is_empty());

        // This song's do: the bar widens and the line being sung is lit.
        app.report(Report::Lyrics(want.key(), Some(lyrics())), 3.0);
        let t = 6.0; // 36 s into the song: "Every word" is being sung
        assert!((app.pill_rect(t).w - content::COMPACT_LYRICS_W).abs() < 0.5);
        let lines = strip_lines(&app, t, &mut engine);
        let lit = lines.iter().find(|(s, _)| s == "Every word you sing").map(|l| l.1);
        assert!(lit.is_some_and(|o| o > 0.99), "{lines:?}");
        let dim = lines.iter().find(|(s, _)| s == "Hold on to the night" || s == "Fades into the sky");
        assert!(dim.is_some_and(|(_, o)| *o > 0.2 && *o < 0.5), "lines not being sung stay visible, dimmer: {lines:?}");
        // The strip slides at 60 fps as a new line starts, then calms down.
        assert_eq!(app.frame_interval(10.0 - islet_core::lyrics::LEAD + 0.1), Some(FRAME));
        assert_eq!(app.frame_interval(12.0), Some(WAVE_FRAME));

        // Music keeps the island visible even with hover-only on.
        app.set_prefs(IslandPrefs { auto_hide: true, ..lyrics_on() });
        app.tick(12.0);
        assert_eq!(app.frame(13.0).opacity, 1.0);

        // Turning lyrics off forgets them and the bar goes back to normal.
        app.set_prefs(IslandPrefs::default());
        app.tick(13.0);
        assert!((app.pill_rect(16.0).w - content::COMPACT_MEDIA_W).abs() < 0.5);
        assert_eq!(app.lyrics_wanted(), None);
    }

    #[test]
    fn lyrics_on_the_now_playing_card_and_not_beside_a_notch() {
        let mut engine = TextEngine::new();
        let mut app = App::new(Demo::None, false);
        app.set_prefs(lyrics_on());
        app.report(Report::Media(Some(song(true))), 0.0);
        let key = app.lyrics_wanted().unwrap().key();
        app.report(Report::Lyrics(key.clone(), Some(lyrics())), 0.0);
        click(&mut app, 1.0);
        let t = 4.0;
        let want_h = MEDIA_H + content::CARD_HEADER_H + content::CARD_LYRICS_H;
        assert!((app.pill_rect(t).h - want_h).abs() < 0.01);
        let shown = texts(&app, t, &mut engine);
        assert!(shown.iter().any(|s| s == "Hold on to the night") && shown.iter().any(|s| s == "Every word you sing"));

        // Songs without synced lyrics: the plain card.
        app.report(Report::Lyrics(key, None), 5.0);
        assert!((app.pill_rect(9.0).h - MEDIA_H - content::CARD_HEADER_H).abs() < 0.01);

        // A notch would hide the middle of each line: no lyrics in the bar.
        let mut notched = App::new(Demo::None, false);
        notched.set_notch(Some((200.0, 32.0)), 0.0);
        notched.set_prefs(lyrics_on());
        notched.report(Report::Media(Some(song(true))), 0.0);
        let key = notched.lyrics_wanted().unwrap().key();
        notched.report(Report::Lyrics(key, Some(lyrics())), 0.0);
        assert!(strip_lines(&notched, 4.0, &mut engine).is_empty());
    }

    #[test]
    fn control_centre_from_an_activity_and_back() {
        let mut engine = TextEngine::new();
        let mut app = App::new(Demo::None, false);
        app.set_time(NOW, 0.0);
        app.report(Report::Media(Some(song(true))), 0.0);
        click(&mut app, 1.0);
        let t = 4.0;
        let cr = app.content_rect(app.pill_rect(t));
        assert!(texts(&app, t, &mut engine).iter().any(|s| s == "Control Centre"));

        // "‹ Control Centre" opens the clock card with Wi-Fi, Bluetooth…
        press(&mut app, content::home_button(cr), t);
        assert_eq!(app.take_conn_commands(), [ConnCommand::Status]);
        let t = 8.0;
        assert!((app.pill_rect(t).h - CLOCK_H).abs() < 0.01);
        let shown = texts(&app, t, &mut engine);
        assert!(shown.iter().any(|s| s == "Wi-Fi") && shown.iter().any(|s| s == "Now Playing"), "{shown:?}");

        // Into the Wi-Fi list; its back button returns to Control Centre.
        let cr = app.content_rect(app.pill_rect(t));
        press(&mut app, content::clock_chips(cr, ClockRow::default())[0].0, t);
        let t = 12.0;
        assert!(texts(&app, t, &mut engine).iter().any(|s| s == "Wi-Fi"));
        let r = content::back_button(app.content_rect(app.pill_rect(t)));
        press(&mut app, r, t);
        let t = 16.0;
        assert!((app.pill_rect(t).h - CLOCK_H).abs() < 0.01, "back on Control Centre");

        // "Now Playing ›" goes back to the music.
        assert!(app.accessibility(t).buttons.iter().any(|(n, _)| n == "Back to Now Playing"));
        let r = content::return_button(app.content_rect(app.pill_rect(t)));
        press(&mut app, r, t);
        let t = 20.0;
        assert!((app.pill_rect(t).h - MEDIA_H - content::CARD_HEADER_H).abs() < 0.01);

        // Control Centre lasts until the island closes.
        let r = content::home_button(app.content_rect(app.pill_rect(t)));
        press(&mut app, r, t);
        app.handle(Event::Key(Key::Escape), 21.0);
        click(&mut app, 25.0);
        assert!((app.pill_rect(29.0).h - MEDIA_H - content::CARD_HEADER_H).abs() < 0.01, "reopens on the music");
    }

    #[test]
    fn hover_only_island_hides_until_the_pointer_arrives() {
        let mut app = App::new(Demo::None, false);
        assert_eq!(app.frame(0.0).opacity, 1.0, "always shown by default");
        app.set_prefs(IslandPrefs { auto_hide: true, ..IslandPrefs::default() });
        assert!(app.tick(0.0));
        assert!(app.frame(2.0).opacity < 0.01);
        assert!(!app.animating(2.0) && app.frame_interval(2.0).is_none(), "hidden and idle: no wakeups");

        // The hidden island still notices the pointer and slides out.
        let (x, y) = centre(&app, 2.0);
        assert!(app.input_region(2.0).contains(x, y));
        app.handle(Event::PointerMove { x, y }, 2.0);
        assert_eq!(app.presentation(), Peek);
        assert!(app.frame(3.0).opacity > 0.99);
        app.handle(Event::PointerLeave, 3.0);
        assert!(app.frame(5.0).opacity < 0.01, "and hides again when it leaves");

        // Anything live shows; switching the setting off shows the island.
        app.report(Report::Hud(Hud::CapsLock(true)), 5.0);
        assert!(app.frame(6.0).opacity > 0.99);
        let mut plain = App::new(Demo::None, false);
        plain.set_prefs(IslandPrefs { auto_hide: true, ..IslandPrefs::default() });
        plain.tick(0.0);
        plain.set_prefs(IslandPrefs::default());
        plain.tick(2.0);
        assert!(plain.frame(4.0).opacity > 0.99);
    }

    #[test]
    fn the_volume_slider_sets_the_volume_without_a_pop_up() {
        let mut app = App::new(Demo::None, false);
        app.report(Report::Media(Some(song(true))), 0.0);
        app.report(Report::Volume(0.3, false), 0.0);
        click(&mut app, 1.0);
        let t = 5.0;
        let card = app.card_rect(t);
        let (slider, _) = content::media_controls(card, true).into_iter().find(|(_, c)| *c == Control::Volume).unwrap();
        let y = slider.y + slider.h / 2.0;
        app.handle(Event::PointerDown { x: slider.x + 10.0, y, button: Button::Primary }, t);
        assert_eq!(app.take_volume(), Some(0.0));
        app.handle(Event::PointerMove { x: slider.right() - 10.0, y }, t + 0.1);
        assert_eq!(app.take_volume(), Some(1.0));
        app.handle(Event::PointerUp { x: slider.right() - 10.0, y, button: Button::Primary }, t + 0.2);
        assert_eq!(app.presentation(), Expanded, "dragging the slider doesn't close the card");
        assert!(app.take_commands().is_empty());
        // The system echoes the change: no volume pop-up for it.
        app.report(Report::Hud(Hud::Volume { level: 1.0, muted: false }), t + 0.3);
        assert!(!app.live(t + 0.4).iter().any(|l| matches!(l, Live::Hud(_))));
        // A change from the keyboard later does pop up.
        app.report(Report::Hud(Hud::Volume { level: 0.8, muted: false }), t + 5.0);
        assert!(app.live(t + 5.1).iter().any(|l| matches!(l, Live::Hud(_))));
    }

    fn zoom_call() -> Report {
        Report::Privacy(islet_core::activity::Privacy { camera: None, mic: Some(vec!["Zoom".into()]) })
    }

    #[test]
    fn a_call_shows_its_length_and_mutes_the_microphone() {
        let mut engine = TextEngine::new();
        let mut app = App::new(Demo::None, false);
        app.report(zoom_call(), 0.0);
        app.tick(0.0);
        assert!(app.call_active());
        assert_eq!(app.presentation(), Idle, "not for the first moment");
        app.tick(islet_core::activity::CALL_AFTER);
        assert_eq!(app.presentation(), Compact);
        let shown = texts(&app, 65.0, &mut engine);
        assert!(shown.iter().any(|s| s == "Zoom") && shown.iter().any(|s| s == "1:05"), "{shown:?}");
        let wake = app.next_timer(65.2).unwrap();
        assert!((wake - 0.8).abs() < 1e-6, "ticks on the second: {wake}");

        // Open it and mute.
        click(&mut app, 70.0);
        let t = 74.0;
        let r = content::call_mute_button(app.card_rect(t));
        press(&mut app, r, t);
        assert_eq!(app.take_mic_commands(), [MicCommand::Set(true)]);
        assert!(app.accessibility(t + 1.0).buttons.iter().any(|(n, _)| n == "Unmute microphone"));
        assert!(texts(&app, t + 1.0, &mut engine).iter().any(|s| s.starts_with("Muted")));
        // The microphone goes off: the call ends.
        app.report(Report::Privacy(Default::default()), 80.0);
        assert!(!app.call_active());
        // Switched off in Settings: no call activity.
        let mut off = App::new(Demo::None, false);
        off.set_prefs(IslandPrefs { calls: false, ..IslandPrefs::default() });
        off.report(zoom_call(), 0.0);
        off.tick(10.0);
        assert_eq!(off.presentation(), Idle);
    }

    fn right_click(app: &mut App, t: f64) {
        let (x, y) = centre(app, t);
        app.handle(Event::PointerDown { x, y, button: Button::Secondary }, t);
    }

    fn menu_row(app: &App, t: f64, action: content::MenuAction) -> Rect {
        let items = app.menu_items(t);
        let i = items.iter().position(|m| m.action == action).expect("in the menu");
        content::menu_rows(app.content_rect(app.pill_rect(t)), items.len())[i]
    }

    #[test]
    fn the_menu_hides_the_island_pauses_pop_ups_and_times_lyrics() {
        use content::MenuAction;
        let mut app = App::new(Demo::None, false);
        app.set_time(NOW, 0.0);
        // Hide for an hour: gone (even the clock), back when pointed at,
        // and by itself after the hour.
        right_click(&mut app, 0.0);
        let r = menu_row(&app, 4.0, MenuAction::Hide);
        press(&mut app, r, 4.0);
        assert_eq!(app.presentation(), Idle);
        assert!(app.frame(6.0).opacity < 0.01);
        assert!(app.next_timer(6.0).is_some_and(|s| s <= QUIET_SECS));
        app.tick(5.0 + QUIET_SECS);
        assert!(app.frame(5.0 + QUIET_SECS + 2.0).opacity > 0.99, "back after the hour");

        // Pause pop-ups: a volume change or a headset doesn't pop up.
        let t = 4000.0;
        right_click(&mut app, t);
        let r = menu_row(&app, t + 4.0, MenuAction::Quiet);
        press(&mut app, r, t + 4.0);
        app.handle(Event::Key(Key::Escape), t + 5.0);
        app.report(Report::Hud(Hud::Volume { level: 0.5, muted: false }), t + 9.0);
        app.report(Report::DeviceConnected(buds()), t + 9.0);
        assert!(app.live(t + 9.5).is_empty(), "quiet");
        assert_eq!(app.presentation(), Idle);

        // Lyrics: the switch, and timing for this song.
        app.report(Report::Media(Some(song(true))), t + 10.0);
        right_click(&mut app, t + 11.0);
        let r = menu_row(&app, t + 15.0, MenuAction::Lyrics);
        press(&mut app, r, t + 15.0);
        assert!(app.prefs.lyrics);
        assert_eq!(app.take_ui_actions(), [UiAction::SetSwitch("lyrics", true)]);
        let key = app.lyrics_wanted().unwrap().key();
        app.report(Report::Lyrics(key, Some(lyrics())), t + 15.0);
        let row = menu_row(&app, t + 19.0, MenuAction::LyricsTiming);
        let (_, plus) = content::menu_stepper(row);
        press(&mut app, plus, t + 19.0);
        press(&mut app, plus, t + 19.5);
        let m = app.activities.media().unwrap().clone();
        assert!((app.lyrics_pos(&m, t + 20.0) - (m.position_now(t + 20.0).unwrap() - 1.0)).abs() < 1e-9);
        let item = app.menu_items(t + 20.0).into_iter().find(|m| m.action == MenuAction::LyricsTiming).unwrap();
        assert_eq!(item.detail.as_deref(), Some("+1.0 s"));
        // Right-click again closes the menu.
        right_click(&mut app, t + 21.0);
        assert!(!app.menu);
    }

    #[test]
    fn a_fullscreen_video_hides_the_island_until_the_pointer_comes() {
        let mut engine = TextEngine::new();
        let mut app = App::new(Demo::None, false);
        app.set_time(NOW, 0.0);
        app.report(Report::Media(Some(song(true))), 0.0);
        assert_eq!(app.presentation(), Compact);
        assert!(app.handle(Event::Fullscreen(true), 2.0));
        assert!(app.frame(4.0).opacity < 0.01, "music and all step aside");
        assert!(app.elements(4.0, &mut engine).is_empty());
        assert_eq!(app.frame_interval(4.0), None, "a hidden waveform doesn't wake the computer");

        // Pointing at the top brings it back; leaving hides it again.
        let (x, y) = centre(&app, 4.0);
        assert!(app.handle(Event::PointerMove { x, y }, 4.0));
        assert!(app.frame(5.0).opacity > 0.99 && !app.elements(5.0, &mut engine).is_empty());
        app.handle(Event::PointerLeave, 5.0);
        assert!(app.frame(7.0).opacity < 0.01);

        // A ringing alarm still shows; leaving fullscreen shows everything.
        app.report(
            Report::Alarm(Some(Alarm { id: 9, hour: 7, minute: 0, label: "Up".into(), days: 0, enabled: true })),
            7.0,
        );
        app.tick(7.0);
        assert!(app.frame(9.0).opacity > 0.99, "an alarm is never hidden");
        app.report(Report::Alarm(None), 9.0);
        app.tick(9.0);
        app.handle(Event::Fullscreen(false), 9.0);
        assert!(app.frame(11.0).opacity > 0.99);
    }

    /// Lyrics, the back buttons and Control Centre for visual review:
    /// `ISLET_LYRICS_SHEET=/path/out.png cargo test -p islet lyrics_sheet`
    #[test]
    fn lyrics_sheet() {
        let Some(out) = std::env::var_os("ISLET_LYRICS_SHEET") else { return };
        use islet_core::activity::ArtImage;
        use tiny_skia::{Color, Pixmap, PixmapPaint, Transform};
        let (aw, ah) = (64u32, 64u32);
        let mut rgba = Vec::with_capacity((aw * ah * 4) as usize);
        for y in 0..ah {
            for x in 0..aw {
                rgba.extend_from_slice(&[(200 - x * 2) as u8, (60 + y * 2) as u8, 120, 255]);
            }
        }
        let art = ArtImage { width: aw, height: ah, rgba: Arc::new(rgba), dominant: [210, 90, 120] };
        let m = MediaInfo {
            art: Some(art),
            title: "Midnight Signals".into(),
            artist: "The Placeholders".into(),
            ..song(true)
        };
        let mut engine = TextEngine::new();
        let base = || {
            let mut a = App::new(Demo::None, false);
            a.set_time(NOW, 0.0);
            a.set_prefs(lyrics_on());
            a.report(Report::Media(Some(m.clone())), 0.0);
            a.report(Report::Volume(0.6, false), 0.0);
            let key = a.lyrics_wanted().unwrap().key();
            a.report(Report::Lyrics(key, Some(lyrics())), 0.0);
            a
        };
        // (app, time, cell height)
        let compact = base();
        let mut card = base();
        click(&mut card, 1.0);
        let mut home = base();
        click(&mut home, 1.0);
        let r = content::home_button(home.content_rect(home.pill_rect(4.0)));
        press(&mut home, r, 4.0);
        let mut list = base();
        click(&mut list, 1.0);
        let r = content::home_button(list.content_rect(list.pill_rect(4.0)));
        press(&mut list, r, 4.0);
        let cr = list.content_rect(list.pill_rect(8.0));
        press(&mut list, content::clock_chips(cr, ClockRow::default())[0].0, 8.0);
        let mut idle = App::new(Demo::None, false);
        idle.set_time(NOW, 0.0);
        let mut idle12 = App::new(Demo::None, false);
        idle12.set_clock_style(false);
        idle12.set_time(NOW, 0.0);
        let mut menu = base();
        let (mx, my) = centre(&menu, 1.0);
        menu.handle(Event::PointerDown { x: mx, y: my, button: Button::Secondary }, 1.0);
        let mut call = App::new(Demo::None, false);
        call.report(zoom_call(), 0.0);
        call.tick(4.0);
        let mut call_card = App::new(Demo::None, false);
        call_card.report(zoom_call(), 0.0);
        call_card.tick(4.0);
        click(&mut call_card, 5.0);
        let shots: Vec<(&App, f64, u32)> = vec![
            (&call, 754.0, 46),
            (&call_card, 754.0, 150),
            (&menu, 6.0, 250),
            (&idle, 1.0, 46),
            (&idle12, 1.0, 46),
            (&compact, 3.0, 46),
            (&compact, 5.1, 46), // sliding onto the next line
            (&compact, 7.5, 46),
            (&card, 6.0, 240),
            (&home, 9.0, 180),
            (&list, 12.0, 200),
        ];
        let cw = WINDOW.0 as u32;
        let total: u32 = shots.iter().map(|s| s.2).sum();
        let mut sheet = Pixmap::new(cw, total).unwrap();
        sheet.fill(Color::from_rgba8(226, 230, 236, 255));
        let mut y = 0;
        for (a, t, h) in shots {
            let mut cell = Pixmap::new(cw, WINDOW.1 as u32).unwrap();
            islet_render::draw(&mut cell, &a.frame(t), 1.0);
            let els = a.elements(t, &mut engine);
            let mask = islet_render::clip_mask(&a.frame(t), cw, WINDOW.1 as u32, 1.0);
            islet_render::elements::draw_elements(&mut cell, &els, &mut engine, 1.0, mask.as_ref());
            sheet.draw_pixmap(0, y as i32, cell.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
            y += h;
        }
        sheet.save_png(out).unwrap();
    }

    #[test]
    fn camera_and_microphone_dots() {
        use islet_core::activity::{Custom, Privacy};
        let mut engine = TextEngine::new();
        let dot = |els: &[Element]| {
            els.iter().find_map(|e| match e {
                Element::Bar { w, h, color, .. }
                    if (*w - PRIVACY_DOT).abs() < 0.01 && (*h - PRIVACY_DOT).abs() < 0.01 =>
                {
                    Some(*color)
                }
                _ => None,
            })
        };
        let mut app = App::new(Demo::None, false);
        // The dots on their own (with calls on, a call shows the microphone).
        app.set_prefs(IslandPrefs { calls: false, idle_clock: false, ..IslandPrefs::default() });
        assert!(!app.has_content(0.0));
        let mic = Privacy { camera: None, mic: Some(vec!["Zoom".into()]) };
        assert!(app.report(Report::Privacy(mic), 0.0));
        assert_eq!(app.presentation(), Idle, "a dot never opens the island");
        assert!(app.has_content(0.0));
        assert_eq!(dot(&app.elements(0.0, &mut engine)), Some(content::ORANGE));
        assert!(app.accessibility(0.0).label.ends_with("Microphone in use by Zoom"));
        // The camera wins the colour; a compact activity makes room for it.
        let both = Privacy { camera: Some(vec![]), mic: Some(vec!["Zoom".into()]) };
        app.report(Report::Privacy(both), 1.0);
        let job = Custom {
            id: "b".into(),
            title: "Build".into(),
            subtitle: String::new(),
            progress: Some(0.5),
            color: None,
            open: None,
        };
        app.report(Report::Custom(job), 1.0);
        assert_eq!(app.presentation(), Compact);
        let w = app.pill_rect(5.0).w;
        assert!((w - (content::COMPACT_BATTERY_W + PRIVACY_W)).abs() < 0.5, "{w}");
        assert_eq!(dot(&app.elements(5.0, &mut engine)), Some(content::GREEN));
        // Opening the card hides the dot.
        click(&mut app, 5.0);
        assert_eq!(dot(&app.elements(10.0, &mut engine)), None);
        app.handle(Event::Key(Key::Escape), 10.0);
        app.report(Report::Privacy(Privacy::default()), 10.0);
        assert!((app.pill_rect(15.0).w - content::COMPACT_BATTERY_W).abs() < 0.5);
        // Switched off in Settings: no dot even while the mic is in use.
        app.set_prefs(IslandPrefs { privacy_dots: false, ..Default::default() });
        app.report(Report::Privacy(Privacy { camera: None, mic: Some(vec![]) }), 16.0);
        assert_eq!(dot(&app.elements(20.0, &mut engine)), None);
        assert_eq!(dot(&app.elements(15.0, &mut engine)), None);
    }

    #[test]
    fn shortcut_opens_and_closes() {
        let mut app = App::new(Demo::None, false);
        assert!(app.handle(Event::Hotkey, 0.0));
        assert_eq!(app.presentation(), Expanded);
        app.handle(Event::Hotkey, 5.0);
        assert_eq!(app.presentation(), Idle);
    }

    #[test]
    fn hover_peeks_and_leaving_restores_idle() {
        let mut app = App::new(Demo::None, false);
        let (x, y) = centre(&app, 0.0);
        assert!(app.handle(Event::PointerMove { x, y }, 0.0));
        assert_eq!(app.presentation(), Peek);
        assert!(app.handle(Event::PointerLeave, 1.0));
        assert_eq!(app.presentation(), Idle);
    }

    #[test]
    fn click_expands_and_click_again_collapses() {
        let mut app = App::new(Demo::None, false);
        click(&mut app, 0.0);
        assert_eq!(app.presentation(), Expanded);
        assert!(app.animating(0.1));
        assert!(!app.animating(5.0));
        assert!((app.pill_rect(5.0).w - 372.0).abs() < 0.01);
        click(&mut app, 5.0);
        assert_ne!(app.presentation(), Expanded);
    }

    #[test]
    fn holding_opens_large_without_a_click_on_release() {
        let mut app = App::new(Demo::None, false);
        let (x, y) = centre(&app, 0.0);
        app.handle(Event::PointerMove { x, y }, 0.0);
        app.handle(Event::PointerDown { x, y, button: Button::Primary }, 0.0);
        assert!((app.next_timer(0.0).unwrap() - LONG_PRESS_SECS).abs() < 1e-9);
        assert!(app.tick(LONG_PRESS_SECS));
        assert_eq!(app.presentation(), Large);
        app.handle(Event::PointerUp { x, y, button: Button::Primary }, 0.6);
        assert_eq!(app.presentation(), Large, "release after a long press is not a click");
    }

    #[test]
    fn open_island_auto_collapses_after_pointer_leaves() {
        let mut app = App::new(Demo::None, false);
        let (x, y) = centre(&app, 0.0);
        app.handle(Event::PointerMove { x, y }, 0.0);
        click(&mut app, 0.0);
        app.handle(Event::PointerMove { x: 1.0, y: 250.0 }, 3.0);
        assert!(!app.tick(5.0));
        assert!(app.tick(3.0 + AUTO_COLLAPSE_SECS));
        assert_eq!(app.presentation(), Idle);
    }

    #[test]
    fn escape_collapses_and_right_click_opens_the_menu() {
        let mut app = App::new(Demo::None, false);
        click(&mut app, 0.0);
        app.handle(Event::Key(Key::Escape), 1.0);
        assert_eq!(app.presentation(), Idle);
        let (x, y) = centre(&app, 6.0);
        app.handle(Event::PointerDown { x, y, button: Button::Secondary }, 6.0);
        assert!(!app.wants_quit(), "right-click no longer quits by accident");
        assert_eq!(app.presentation(), Expanded);
        let mut engine = TextEngine::new();
        let shown = texts(&app, 10.0, &mut engine);
        assert!(shown.iter().any(|s| s == "Quit Islet") && shown.iter().any(|s| s == "Islet Settings"), "{shown:?}");
        // "Quit Islet" quits.
        let rows = content::menu_rows(app.content_rect(app.pill_rect(10.0)), app.menu_items(10.0).len());
        press(&mut app, *rows.last().unwrap(), 10.0);
        assert!(app.wants_quit());
    }

    #[test]
    fn clicks_outside_the_island_are_ignored() {
        let mut app = App::new(Demo::None, false);
        app.handle(Event::PointerDown { x: 1.0, y: 250.0, button: Button::Primary }, 0.0);
        app.handle(Event::PointerUp { x: 1.0, y: 250.0, button: Button::Primary }, 0.1);
        assert_eq!(app.presentation(), Idle);
    }

    #[test]
    fn playing_music_shows_compact_and_animates_the_waveform() {
        let mut app = App::new(Demo::None, false);
        let mut engine = TextEngine::new();
        assert!(app.report(Report::Media(Some(song(true))), 0.0));
        assert_eq!(app.presentation(), Compact);
        assert!((app.pill_rect(5.0).w - content::COMPACT_MEDIA_W).abs() < 0.01);
        let els = app.elements(5.0, &mut engine);
        assert!(els.iter().any(|e| matches!(e, Element::Art { .. })), "album art on the left");
        assert_eq!(els.iter().filter(|e| matches!(e, Element::Bar { .. })).count(), 4, "waveform on the right");
        assert_eq!(app.frame_interval(5.0), Some(WAVE_FRAME), "the waveform moves at 30 fps");

        app.report(Report::Media(Some(song(false))), 6.0);
        assert_eq!(app.frame_interval(6.5), None, "paused: nothing moves, no frames");
        app.tick(6.0 + 15.0);
        assert_eq!(app.presentation(), Idle, "a paused session leaves after the linger");
    }

    #[test]
    fn expanded_player_sends_commands_and_seeks_by_scrubbing() {
        let mut app = App::new(Demo::None, false);
        let mut engine = TextEngine::new();
        app.report(Report::Media(Some(song(true))), 0.0);
        click(&mut app, 1.0);
        assert_eq!(app.presentation(), Expanded);
        let t = 4.0;
        assert!((app.pill_rect(t).h - MEDIA_H - content::CARD_HEADER_H).abs() < 0.01);
        assert!(texts(&app, t, &mut engine).contains(&"Song".to_string()));

        let controls = content::media_controls(app.card_rect(t), false);
        let (play, _) = controls.iter().find(|(_, c)| *c == Control::PlayPause).unwrap();
        let (px, py) = (play.x + play.w / 2.0, play.y + play.h / 2.0);
        app.handle(Event::PointerDown { x: px, y: py, button: Button::Primary }, t);
        app.handle(Event::PointerUp { x: px, y: py, button: Button::Primary }, t + 0.05);
        assert_eq!(app.take_commands(), [MediaCommand::PlayPause]);
        assert_eq!(app.presentation(), Expanded, "pressing a control does not collapse the card");

        let (bar, _) = controls.iter().find(|(_, c)| *c == Control::Scrub).unwrap();
        let y = bar.y + bar.h / 2.0;
        app.handle(Event::PointerDown { x: bar.x, y, button: Button::Primary }, t);
        app.handle(Event::PointerMove { x: bar.x + bar.w / 2.0, y }, t + 0.1);
        app.handle(Event::PointerUp { x: bar.x + bar.w / 2.0, y, button: Button::Primary }, t + 0.2);
        match app.take_commands()[..] {
            [MediaCommand::SeekTo(s)] => assert!((s - 90.0).abs() < 0.5, "seek to the middle: {s}"),
            ref other => panic!("expected a seek, got {other:?}"),
        }
    }

    #[test]
    fn connecting_a_device_peeks_wide_then_hands_back_to_music() {
        let mut app = App::new(Demo::None, false);
        let mut engine = TextEngine::new();
        app.report(Report::Media(Some(song(true))), 0.0);
        app.report(Report::DeviceConnected(buds()), 2.0);
        // Battery peek + media = two live activities: Minimal (pill + bubble).
        assert_eq!(app.presentation(), Minimal);
        assert!(app.frame(5.0).bubble.is_some());
        assert!(
            app.elements(5.0, &mut engine).iter().any(|e| matches!(e, Element::Ring { .. })),
            "battery ring in the bubble"
        );
        app.tick(2.0 + PEEK_CONNECTED);
        assert_eq!(app.presentation(), Compact, "the peek ends, music stays");

        // With nothing else live, a connect peek is the wide compact pill.
        let mut solo = App::new(Demo::None, false);
        solo.report(Report::DeviceConnected(buds()), 0.0);
        assert_eq!(solo.presentation(), Compact);
        assert!((solo.pill_rect(3.0).w - content::COMPACT_BATTERY_W).abs() < 0.01);
        assert_eq!(texts(&solo, 3.0, &mut engine), ["Earbuds", "87%"]);
    }

    #[test]
    fn scrolling_switches_between_live_activities() {
        let mut app = App::new(Demo::None, false);
        let mut engine = TextEngine::new();
        app.report(Report::Media(Some(song(true))), 0.0);
        app.report(Report::DeviceConnected(buds()), 0.0);
        let (x, y) = centre(&app, 0.5);
        app.handle(Event::PointerMove { x, y }, 0.5);
        let ring_in_bubble = |app: &App, t, engine: &mut TextEngine| {
            app.elements(t, engine).iter().any(|e| matches!(e, Element::Ring { .. }))
        };
        assert!(ring_in_bubble(&app, 1.0, &mut engine), "battery peek in the bubble");
        assert!(!app.handle(Event::Scroll { dx: 0.0, dy: 0.4 }, 1.0), "a small scroll accumulates");
        assert!(app.handle(Event::Scroll { dx: 0.0, dy: 0.7 }, 1.1), "a full step switches");
        assert!(!app.handle(Event::Scroll { dx: 0.0, dy: 3.0 }, 1.2), "one switch per gesture");
        let first_art =
            app.elements(1.5, &mut engine).iter().any(|e| matches!(e, Element::Art { size, .. } if *size == 25.0));
        assert!(first_art, "music art still shown after switching");
    }

    #[test]
    fn navigation_takes_priority_and_its_buttons_act() {
        use islet_core::nav::{RouteInfo, TravelMode, Trip};
        let mut app = App::new(Demo::None, false);
        let trip = Trip {
            label: "Work".into(),
            destination: "Kampala Road".into(),
            origin: "Ntinda".into(),
            mode: TravelMode::Drive,
            place_id: None,
        };
        let route =
            RouteInfo { duration: 900.0, free_flow: 900.0, distance_m: 5000.0, first_step: None, maneuver: None };
        app.report(Report::Media(Some(song(true))), 0.0);
        app.report(Report::Navigation(Some(NavInfo { trip, route: Some(route), error: None })), 0.0);
        assert_eq!(app.presentation(), Minimal, "navigation and music together");
        click(&mut app, 1.0);
        assert!(app.nav_card_open(5.0), "the open card is navigation, not music");
        let (waze, _) = content::nav_controls(app.card_rect(5.0))[1];
        let (x, y) = (waze.x + waze.w / 2.0, waze.y + waze.h / 2.0);
        app.handle(Event::PointerDown { x, y, button: Button::Primary }, 5.0);
        app.handle(Event::PointerUp { x, y, button: Button::Primary }, 5.1);
        assert_eq!(app.take_nav_actions(), [NavControl::Waze]);
        assert_eq!(app.presentation(), Expanded, "a button press doesn't collapse the card");
        app.report(Report::Navigation(None), 6.0);
        assert!(!app.nav_card_open(6.0));
    }

    fn press(app: &mut App, r: Rect, t: f64) {
        let (x, y) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
        app.handle(Event::PointerDown { x, y, button: Button::Primary }, t);
        app.handle(Event::PointerUp { x, y, button: Button::Primary }, t + 0.05);
    }

    fn bell_shown(app: &App, t: f64, engine: &mut TextEngine) -> bool {
        app.elements(t, engine)
            .iter()
            .any(|e| matches!(e, Element::Icon { icon: islet_render::elements::Icon::Bell(_), .. }))
    }

    #[test]
    fn a_ringing_alarm_opens_the_island_and_shows_a_bell() {
        let mut app = App::new(Demo::None, false);
        app.set_prefs(IslandPrefs { idle_clock: false, ..IslandPrefs::default() });
        let mut engine = TextEngine::new();
        let alarm = Alarm { id: 1, hour: 14, minute: 6, label: "Tea".into(), days: 0, enabled: true };
        app.set_alarms(vec![alarm]);
        app.set_time(NOW, 0.0);
        assert!((app.next_timer(0.0).unwrap() - 18.05).abs() < 1e-9, "checks at the next minute");
        let ring = LocalTime { minute: 6, second: 0, ..NOW };
        assert!(app.set_time(ring, 18.0));
        assert_eq!(app.take_alarm_out(), [AlarmOut::Disable(1), AlarmOut::Sound(true)]);
        assert_eq!(app.presentation(), Expanded, "the alarm opens the island");
        assert!(texts(&app, 22.0, &mut engine).iter().any(|s| s == "Tea"));
        assert!(bell_shown(&app, 22.0, &mut engine));
        assert_eq!(app.frame_interval(22.0), Some(WAVE_FRAME), "the bell swings");

        // Collapsed, the compact pill keeps the bell at its right end.
        app.handle(Event::Key(Key::Escape), 23.0);
        assert_eq!(app.presentation(), Compact);
        let pill = app.pill_rect(27.0);
        let bell_x = app.elements(27.0, &mut engine).iter().find_map(|e| match e {
            Element::Icon { icon: islet_render::elements::Icon::Bell(_), x, .. } => Some(*x as f64),
            _ => None,
        });
        assert!(bell_x.is_some_and(|x| x > pill.x + pill.w * 0.7), "bell on the right");

        // Open the card again and press Snooze.
        click(&mut app, 28.0);
        let (snooze, _) = content::alarm_controls(app.content_rect(app.pill_rect(32.0)))[0];
        press(&mut app, snooze, 32.0);
        assert_eq!(app.take_alarm_out(), [AlarmOut::Sound(false)]);
        assert!(!bell_shown(&app, 36.0, &mut engine));
        assert_eq!(app.presentation(), Idle, "nothing else live: the island closes");

        // Nine minutes later it rings again; Stop ends it for good.
        app.set_time(LocalTime { minute: 15, second: 0, ..NOW }, 600.0);
        assert_eq!(app.take_alarm_out(), [AlarmOut::Sound(true)]);
        let (_, stop) = content::alarm_controls(app.content_rect(app.pill_rect(605.0)))[1];
        assert_eq!(stop, AlarmControl::Stop);
        let (stop_rect, _) = content::alarm_controls(app.content_rect(app.pill_rect(605.0)))[1];
        press(&mut app, stop_rect, 605.0);
        assert_eq!(app.take_alarm_out(), [AlarmOut::Sound(false)]);
        assert_eq!(app.next_timer(610.0), None, "the one-time alarm is done: zero wakeups");
    }

    fn wifi(current: Option<&str>) -> islet_core::connectivity::WifiState {
        use islet_core::connectivity::{WifiNetwork, WifiState};
        let net = |ssid: &str, known: bool| WifiNetwork {
            ssid: ssid.into(),
            strength: 80,
            secure: true,
            known,
            active: current == Some(ssid),
        };
        WifiState {
            available: true,
            enabled: true,
            current: current.map(String::from),
            networks: vec![net("Home", true), net("Cafe", false)],
            note: None,
        }
    }

    #[test]
    fn wifi_and_bluetooth_lists_open_from_the_clock_card() {
        use islet_core::connectivity::{BtDevice, BtState, ConnUpdate};
        let mut app = App::new(Demo::None, false);
        app.set_time(NOW, 0.0);
        click(&mut app, 0.0);
        assert_eq!(app.take_conn_commands(), [ConnCommand::Status], "opening the clock asks for the current network");
        app.report(Report::Connectivity(ConnUpdate::Wifi(wifi(Some("Home")))), 0.5);
        let mut engine = TextEngine::new();
        assert!(texts(&app, 5.0, &mut engine).iter().any(|s| s == "Home"), "the Wi-Fi button names the network");

        // Open the Wi-Fi list.
        let cr = app.content_rect(app.pill_rect(5.0));
        press(&mut app, content::clock_chips(cr, ClockRow::default())[0].0, 5.0);
        assert_eq!(app.take_conn_commands(), [ConnCommand::ScanWifi]);
        let rows = content::list_rows(app.content_rect(app.pill_rect(9.0)), 2);
        assert!(texts(&app, 9.0, &mut engine).iter().any(|s| s == "Cafe"));

        // The header switch turns Wi-Fi off (and says so to screen readers).
        assert!(app.accessibility(9.0).buttons.iter().any(|(n, _)| n == "Turn Wi-Fi off"));
        let switch = content::power_switch(app.content_rect(app.pill_rect(9.0)));
        press(&mut app, switch, 9.0);
        assert_eq!(app.take_conn_commands(), [ConnCommand::SetPower(Radio::Wifi, false)]);

        // Tap a new secured network: the backend says it needs a password.
        press(&mut app, rows[1], 9.0);
        assert_eq!(app.take_conn_commands(), [ConnCommand::ConnectWifi { ssid: "Cafe".into(), password: None }]);
        app.report(
            Report::Connectivity(ConnUpdate::Failed {
                message: "Cafe needs a password".into(),
                needs_password: Some("Cafe".into()),
            }),
            9.5,
        );
        let cr = app.content_rect(app.pill_rect(13.0));
        press(&mut app, content::list_footer(cr, 2), 13.0);
        assert_eq!(app.take_ui_actions(), [UiAction::Settings("wifi:Cafe".into())]);
        assert_eq!(app.presentation(), Expanded, "list taps don't close the island");

        // Back to the clock, then Bluetooth on a system that can't connect from apps.
        press(&mut app, content::list_header(cr), 14.0);
        app.report(
            Report::Connectivity(ConnUpdate::Bluetooth(BtState {
                available: true,
                powered: true,
                devices: vec![BtDevice {
                    id: "b".into(),
                    name: "Buds".into(),
                    kind: DeviceKind::Earbuds,
                    connected: false,
                }],
                can_connect: false,
                note: Some("Windows connects Bluetooth devices in its own settings".into()),
            })),
            14.0,
        );
        let cr = app.content_rect(app.pill_rect(18.0));
        press(&mut app, content::clock_chips(cr, ClockRow::default())[1].0, 18.0);
        assert_eq!(app.take_conn_commands(), [ConnCommand::ListBluetooth]);
        let rows = content::list_rows(app.content_rect(app.pill_rect(22.0)), 1);
        press(&mut app, rows[0], 22.0);
        assert_eq!(app.take_ui_actions(), [UiAction::Url("ms-settings:bluetooth".into())]);

        // Closing the island forgets the list.
        app.handle(Event::Key(Key::Escape), 23.0);
        click(&mut app, 30.0);
        assert!(!texts(&app, 35.0, &mut engine).iter().any(|s| s == "Buds"), "back on the clock card");
    }

    #[test]
    fn timer_counts_down_rings_and_repeats() {
        let mut app = App::new(Demo::None, false);
        let mut engine = TextEngine::new();
        app.set_time(NOW, 0.0);
        app.timer_command(TimerCommand::Start { secs: 90.0, label: "Tea".into() }, 0.0);
        app.tick(0.0);
        assert_eq!(app.presentation(), Compact);
        assert!(texts(&app, 5.0, &mut engine).iter().any(|s| s == "1:25"), "{:?}", texts(&app, 5.0, &mut engine));
        let wake = app.next_timer(5.2).unwrap();
        assert!(wake > 0.0 && wake <= 1.0, "wakes for the next second: {wake}");

        // Paused: no wakeups for it.
        app.timer_command(TimerCommand::Pause, 10.0);
        assert!(app.next_timer(12.0).is_none_or(|w| w > 5.0));
        app.timer_command(TimerCommand::Resume, 20.0);

        // It finishes (80 s left after the pause) and rings with Repeat.
        app.tick(100.0);
        assert_eq!(app.take_alarm_out(), [AlarmOut::Sound(true)]);
        assert_eq!(app.presentation(), Expanded);
        assert!(texts(&app, 104.0, &mut engine).iter().any(|s| s == "Repeat"));
        let (repeat, _) = content::alarm_controls(app.content_rect(app.pill_rect(104.0)))[0];
        press(&mut app, repeat, 104.0);
        assert_eq!(app.take_alarm_out(), [AlarmOut::Sound(false)]);
        assert!(app.timer.as_ref().is_some_and(|tm| tm.remaining(105.0) > 88.0), "Repeat starts it again");
    }

    #[test]
    fn stopwatch_is_pro_and_starts_from_control_centre() {
        let mut app = App::new(Demo::None, false);
        let mut engine = TextEngine::new();
        app.set_time(NOW, 0.0);
        assert!(!app.stopwatch_command(StopwatchCommand::Start, 0.0), "free: no stopwatch");
        click(&mut app, 0.0);
        let cr = app.content_rect(app.pill_rect(5.0));
        assert!(!content::clock_chips(cr, app.clock_row()).iter().any(|(_, p)| *p == Panel::Stopwatch));

        app.set_pro(true);
        let chips = content::clock_chips(cr, app.clock_row());
        let (button, _) = *chips.iter().find(|(_, p)| *p == Panel::Stopwatch).expect("Pro: a stopwatch button");
        press(&mut app, button, 5.0);
        let shown = texts(&app, 9.0, &mut engine);
        assert!(shown.contains(&"0:00.0".to_string()) && shown.contains(&"Start".to_string()), "{shown:?}");

        // Start: it becomes a live activity, ticking in tenths on its card.
        let [(lap, _), (start, _)] = content::stopwatch_controls(app.card_rect(9.0));
        press(&mut app, start, 9.0);
        assert!(app.stopwatch.as_ref().is_some_and(Stopwatch::running));
        assert!(matches!(app.live(9.5).first(), Some(Live::Stopwatch(_))));
        let wake = app.next_timer(10.33).unwrap();
        assert!(wake > 0.0 && wake <= 0.1 + 1e-9, "tenths while the card is open: {wake}");
        assert!(texts(&app, 10.5, &mut engine).iter().any(|s| s == "0:01.4"), "{:?}", texts(&app, 10.5, &mut engine));

        // Lap, then pause: Lap turns into Reset.
        let [(lap2, _), (pause, _)] = content::stopwatch_controls(app.card_rect(12.0));
        assert_eq!(lap, lap2);
        press(&mut app, lap2, 12.0);
        assert_eq!(app.stopwatch.as_ref().unwrap().lap_count(), 1);
        press(&mut app, pause, 14.0);
        assert!(!app.stopwatch.as_ref().unwrap().running());
        assert!(texts(&app, 15.0, &mut engine).iter().any(|s| s == "Reset"));
        assert!(app.next_timer(15.0).is_none_or(|w| w > 1.0), "paused: no ticking");

        // Closed: the pill shows whole seconds.
        app.handle(Event::Key(Key::Escape), 16.0);
        app.tick(20.0);
        assert_eq!(app.presentation(), Compact);
        assert!(texts(&app, 20.0, &mut engine).iter().any(|s| s == "0:05"), "{:?}", texts(&app, 20.0, &mut engine));
        app.stopwatch_command(StopwatchCommand::Reset, 21.0);
        app.tick(21.0);
        assert!(app.live(21.0).is_empty());
    }

    fn quick_button(app: &App, t: f64, q: content::Quick) -> Rect {
        let cr = app.content_rect(app.pill_rect(t));
        content::quick_row(cr, app.clock_row()).into_iter().find(|(_, b)| *b == q).unwrap().0
    }

    fn press_quick(app: &mut App, t: f64, q: content::Quick) {
        let r = quick_button(app, t, q);
        press(app, r, t);
    }

    #[test]
    fn quick_timer_starts_from_the_clock_card() {
        use content::Quick;
        let mut app = App::new(Demo::None, false);
        let mut engine = TextEngine::new();
        app.set_time(NOW, 0.0);
        click(&mut app, 0.0);
        // 5 → 10 → 15 minutes, back to 10, and it stops at the ends.
        let plus = quick_button(&app, 5.0, Quick::Step(1));
        press(&mut app, plus, 5.0);
        press(&mut app, plus, 5.2);
        press_quick(&mut app, 5.4, Quick::Step(-1));
        assert!(texts(&app, 6.0, &mut engine).contains(&"10 min".to_string()));
        assert_eq!(app.presentation(), Expanded, "stepping doesn't close the island");
        for i in 0..20 {
            press_quick(&mut app, 6.0 + f64::from(i) * 0.1, Quick::Step(-1));
        }
        assert!(texts(&app, 9.0, &mut engine).contains(&"1 min".to_string()));
        assert!(app.accessibility(9.0).buttons.iter().any(|(n, _)| n == "Start a 1-minute timer"));

        press_quick(&mut app, 9.0, Quick::Start);
        assert!(app.timer.as_ref().is_some_and(|tm| (tm.remaining(9.1) - 60.0).abs() < 1.0));
        assert!(texts(&app, 13.0, &mut engine).iter().any(|s| s == "Pause"), "the island shows the timer's card");
    }

    #[test]
    fn screenshot_and_lock_close_the_island_first() {
        use content::Quick;
        for (button, action) in [(Quick::Screenshot, UiAction::Screenshot), (Quick::Lock, UiAction::Lock)] {
            let mut app = App::new(Demo::None, false);
            app.set_time(NOW, 0.0);
            click(&mut app, 0.0);
            press_quick(&mut app, 5.0, button);
            assert_eq!(app.take_ui_actions(), [action]);
            assert_eq!(app.presentation(), Idle, "out of the picture");
        }
    }

    #[test]
    fn calendar_card_shows_months_and_events() {
        use content::Quick;
        let mut app = App::new(Demo::None, false);
        let mut engine = TextEngine::new();
        app.set_time(NOW, 0.0);
        // 4 November 2026, 14:05 UTC.
        app.set_wall(1_793_801_100.0, 0, 0.0);
        let at = |d: i64, h: i64| 1_793_750_400 + (d - 4) * 86_400 + h * 3600;
        app.report(
            Report::Calendar(Some(vec![
                islet_core::calendar::CalEvent {
                    summary: "Dentist".into(),
                    start: at(4, 16),
                    end: at(4, 17),
                    all_day: false,
                    location: None,
                },
                islet_core::calendar::CalEvent {
                    summary: "Trip".into(),
                    start: at(20, 0),
                    end: at(22, 0),
                    all_day: true,
                    location: None,
                },
            ])),
            0.0,
        );
        click(&mut app, 0.0);
        press_quick(&mut app, 5.0, Quick::Calendar);
        let shown = texts(&app, 9.0, &mut engine);
        assert!(shown.contains(&"November 2026".to_string()), "{shown:?}");
        assert!(shown.contains(&"30".to_string()) && !shown.contains(&"31".to_string()), "30 days: {shown:?}");
        assert!(shown.contains(&"Today · 16:00".to_string()) && shown.contains(&"Dentist".to_string()), "{shown:?}");
        assert!(shown.contains(&"20 Nov".to_string()) && shown.contains(&"Trip".to_string()), "{shown:?}");
        assert!((app.pill_rect(9.0).h - content::CALENDAR_H).abs() < 0.5);

        // › twice: January 2027 (31 days); ‹ goes back; the header returns to Control Centre.
        let cr = app.content_rect(app.pill_rect(9.0));
        let [(prev, _), (next, _)] = content::calendar_arrows(cr);
        press(&mut app, next, 9.0);
        press(&mut app, next, 9.2);
        let shown = texts(&app, 9.5, &mut engine);
        assert!(shown.contains(&"January 2027".to_string()) && shown.contains(&"31".to_string()), "{shown:?}");
        press(&mut app, prev, 9.6);
        assert!(texts(&app, 9.8, &mut engine).contains(&"December 2026".to_string()));
        press(&mut app, content::back_button(cr), 10.0);
        assert!(texts(&app, 14.0, &mut engine).contains(&"Timer".to_string()), "back on the clock card");
    }

    #[test]
    fn clock_card_names_the_next_alarm() {
        let mut app = App::new(Demo::None, false);
        let mut engine = TextEngine::new();
        app.set_time(NOW, 0.0);
        app.set_alarms(vec![Alarm { id: 1, hour: 7, minute: 30, label: "Up".into(), days: 0, enabled: true }]);
        click(&mut app, 0.0);
        let shown = texts(&app, 5.0, &mut engine);
        // 14:05 → 07:30 tomorrow is 17 h 25 min away.
        assert!(shown.contains(&"07:30 · in 17 h 25 min".to_string()), "{shown:?}");
        assert!(shown.contains(&"Wed 4 Nov".to_string()), "the date shortens to make room: {shown:?}");
        assert_eq!(content::until_label(45), "in 45 min");
        assert_eq!(content::until_label(120), "in 2 h");
        assert_eq!(content::until_label(30 * 60), "tomorrow");
        assert_eq!(content::until_label(3 * 1440 + 5), "in 3 days");
    }

    #[test]
    fn labels_are_never_cut_short_on_a_crowded_clock_card() {
        let mut engine = TextEngine::new();
        // The fullest row: a shelf, clipboard history and Pro's buttons.
        let mut app = App::new(Demo::None, false);
        app.set_time(NOW, 0.0);
        app.set_pro(true);
        let mut p = app.prefs().clone();
        p.clipboard = true;
        app.set_prefs(p);
        app.set_shelf(vec![islet_core::shelf::ShelfItem { path: "/tmp/a.pdf".into(), dir: false }]);
        click(&mut app, 0.0);
        let shown = texts(&app, 5.0, &mut engine);
        assert!(!shown.iter().any(|s| s.contains('…')), "nothing is shortened: {shown:?}");
        let cr = app.content_rect(app.pill_rect(5.0));
        let chips = content::clock_chips(cr, app.clock_row());
        assert!(chips.iter().any(|(r, p)| *p == Panel::Shelf && r.w < 40.0), "the shelf is a round button");
        assert!(chips.windows(2).all(|w| w[0].0.right() <= w[1].0.x + 0.01), "no chip overlaps the next");

        // The lists' footers fit their line whatever they say.
        use islet_render::text::TextStyle;
        let wide = content::list_footer(cr, 3).w as f32;
        let button = TextStyle { size: 13.0, weight: 600, ..TextStyle::default() };
        let note = TextStyle { size: 12.5, ..TextStyle::default() };
        let (shelf_line, _) = content::shelf_footer(3, false).unwrap();
        assert!(engine.measure(&shelf_line, &note) <= wide, "{shelf_line}");
        let c = islet_core::connectivity::Connectivity {
            failure: Some(("needs a password".into(), Some("A very long network name indeed 5G".into()))),
            ..Default::default()
        };
        let (wifi_line, is_button) = content::wifi_footer(&c).unwrap();
        assert!(is_button && engine.measure(&wifi_line, &button) <= wide - 20.0, "{wifi_line}");
    }

    #[test]
    fn todo_list_ticks_and_removes_from_the_island() {
        let mut app = App::new(Demo::None, false);
        let mut engine = TextEngine::new();
        app.set_time(NOW, 0.0);
        app.set_pro(true);
        let mut todo = islet_core::todo::Todo::default();
        let milk = todo.add("Buy milk").unwrap();
        todo.add("Call mum").unwrap();
        app.set_todo(todo);
        click(&mut app, 0.0);
        let cr = app.content_rect(app.pill_rect(5.0));
        assert!(texts(&app, 5.0, &mut engine).iter().any(|s| s == "2"), "the button's badge counts what's left");
        let (button, _) =
            *content::clock_chips(cr, app.clock_row()).iter().find(|(_, p)| *p == Panel::Todo).expect("a to-do button");
        press(&mut app, button, 5.0);
        let shown = texts(&app, 9.0, &mut engine);
        assert!(shown.contains(&"Buy milk".to_string()) && shown.contains(&"2 to do".to_string()), "{shown:?}");

        // Tap the first row: ticked, saved, and it moves below the other.
        let rows = content::list_rows(app.content_rect(app.pill_rect(9.0)), 2);
        press(&mut app, rows[0], 9.0);
        let saved = app.take_todo_changes().expect("saved after a tick");
        assert!(saved.items.iter().find(|i| i.id == milk).unwrap().done);
        assert_eq!(app.todo.ordered()[1].id, milk);
        assert!(texts(&app, 10.0, &mut engine).contains(&"1 to do".to_string()));

        // The × on a row deletes it.
        let r = rows[1];
        let (x, y) = (r.x + r.w - 20.0, r.y + r.h / 2.0);
        app.handle(Event::PointerDown { x, y, button: Button::Primary }, 11.0);
        app.handle(Event::PointerUp { x, y, button: Button::Primary }, 11.05);
        assert_eq!(app.take_todo_changes().map(|t| t.items.len()), Some(1));

        // Typing happens in Settings: the footer opens it.
        let footer = content::list_footer(app.content_rect(app.pill_rect(12.0)), 1);
        press(&mut app, footer, 12.0);
        assert_eq!(app.take_ui_actions(), [UiAction::Settings("pro".into())]);
    }

    #[test]
    fn world_clocks_show_on_the_clock_card_with_pro() {
        let mut app = App::new(Demo::None, false);
        let mut engine = TextEngine::new();
        app.set_time(NOW, 0.0);
        // 4 November 2026, 14:05 UTC; here is UTC.
        app.set_wall(1_793_801_100.0, 0, 0.0);
        let mut p = app.prefs().clone();
        p.world_clocks = vec!["Tokyo".into(), "Atlantis".into(), "New York".into()];
        app.set_prefs(p);
        click(&mut app, 0.0);
        let free_h = app.pill_rect(5.0).h;
        assert!(!texts(&app, 5.0, &mut engine).iter().any(|s| s.starts_with("Tokyo")), "free: none");

        app.set_pro(true);
        app.sync_live(5.0);
        let shown = texts(&app, 9.0, &mut engine);
        assert!(shown.contains(&"Tokyo ".to_string()) && shown.contains(&"23:05".to_string()), "{shown:?}");
        assert!(shown.contains(&"09:05".to_string()), "New York is on EST in November: {shown:?}");
        assert!(app.pill_rect(9.0).h > free_h, "the card grows a line");
    }

    #[test]
    fn focus_phases_chime_and_peek() {
        let mut app = App::new(Demo::None, false);
        app.set_time(NOW, 0.0);
        app.timer_command(TimerCommand::Focus { work: 60.0, brk: 30.0, rounds: 2 }, 0.0);
        app.tick(0.0);
        app.tick(60.5);
        assert_eq!(app.take_alarm_out(), [AlarmOut::Chime]);
        assert_eq!(app.presentation(), Expanded, "the break is announced");
        let mut engine = TextEngine::new();
        assert!(texts(&app, 64.0, &mut engine).iter().any(|s| s == "Break"));
        app.tick(64.0 + AUTO_COLLAPSE_SECS);
        assert_eq!(app.presentation(), Compact, "and the island closes again");
    }

    #[test]
    fn hud_pops_up_briefly_and_respects_its_switch() {
        use islet_core::activity::HUD_SECS;
        let mut app = App::new(Demo::None, false);
        let mut engine = TextEngine::new();
        assert!(app.report(Report::Hud(Hud::Volume { level: 0.5, muted: false }), 0.0));
        app.tick(0.0);
        assert_eq!(app.presentation(), Compact);
        assert!(app.next_timer(0.5).is_some_and(|w| (w - (HUD_SECS - 0.5)).abs() < 1e-9), "wakes once, to hide it");
        app.tick(HUD_SECS + 0.1);
        assert_eq!(app.presentation(), Idle);

        // Caps lock from the window system, with its switch off: nothing.
        let mut p = app.prefs().clone();
        p.caps_lock_hud = false;
        app.set_prefs(p);
        assert!(!app.handle(Event::CapsLock(true), 5.0));
        assert_eq!(app.presentation(), Idle);

        // With music playing, the HUD takes the pill and music the bubble.
        app.report(Report::Media(Some(song(true))), 10.0);
        app.report(Report::Hud(Hud::Brightness(0.8)), 10.0);
        app.tick(10.0);
        assert_eq!(app.presentation(), Minimal);
        let sun = app
            .elements(11.4, &mut engine)
            .iter()
            .any(|e| matches!(e, Element::Icon { icon: islet_render::elements::Icon::Sun, .. }));
        assert!(sun);
    }

    fn standup(start: i64) -> islet_core::calendar::CalEvent {
        islet_core::calendar::CalEvent {
            summary: "Standup".into(),
            start,
            end: start + 900,
            all_day: false,
            location: Some("Room 4".into()),
        }
    }

    #[test]
    fn calendar_shows_up_next_and_the_agenda() {
        let mut app = App::new(Demo::None, false);
        let mut engine = TextEngine::new();
        let wall0 = 1_800_000_000.0; // the wall clock when t = 0
        app.set_wall(wall0, 3 * 3600, 0.0);
        app.report(Report::Calendar(Some(vec![standup(1_800_001_200), standup(1_800_009_000)])), 0.0);
        app.tick(0.0);
        assert_eq!(app.presentation(), Idle, "20 minutes away: nothing yet");
        let wake = app.next_timer(0.0).unwrap();
        assert!((wake - 600.0).abs() < 1e-6, "wakes exactly 10 minutes before: {wake}");

        app.set_wall(wall0 + 610.0, 3 * 3600, 610.0);
        app.tick(610.0);
        assert_eq!(app.presentation(), Compact);
        let shown = texts(&app, 614.0, &mut engine);
        assert!(shown.iter().any(|s| s == "Standup") && shown.iter().any(|s| s == "in 10 min"), "{shown:?}");

        click(&mut app, 620.0);
        let card = texts(&app, 625.0, &mut engine);
        assert!(card.iter().any(|s| s == "Up next") && card.iter().any(|s| s == "Room 4"), "{card:?}");

        // Five minutes after it starts, it's gone.
        app.handle(Event::Key(Key::Escape), 630.0);
        app.set_wall(wall0 + 1200.0 + 301.0, 3 * 3600, 1501.0);
        app.tick(1501.0);
        assert_eq!(app.presentation(), Idle);
    }

    fn note(app: &str, summary: &str, body: &str) -> Report {
        Report::Notification(islet_core::activity::Note { app: app.into(), summary: summary.into(), body: body.into() })
    }

    #[test]
    fn notifications_peek_and_list() {
        use islet_core::activity::NOTE_SECS;
        let mut app = App::new(Demo::None, false);
        let mut engine = TextEngine::new();
        app.report(note("Chat", "Amina", "See you at 5"), 0.0);
        app.report(note("Mail", "Invoice #42", "Due Friday"), 1.0);
        app.tick(1.0);
        assert_eq!(app.presentation(), Compact);
        assert!(texts(&app, 2.0, &mut engine).iter().any(|s| s == "Invoice #42"), "the newest shows");
        click(&mut app, 2.0);
        let card = texts(&app, 6.0, &mut engine);
        assert!(card.iter().any(|s| s == "Notifications") && card.iter().any(|s| s == "See you at 5"), "{card:?}");
        app.handle(Event::Key(Key::Escape), 6.1);
        app.tick(1.0 + NOTE_SECS + 0.1);
        assert_eq!(app.presentation(), Idle);
    }

    #[test]
    fn files_dropped_on_the_island_land_on_the_shelf() {
        let mut app = App::new(Demo::None, false);
        let mut engine = TextEngine::new();
        let (x, y) = centre(&app, 0.0);
        assert!(app.handle(Event::DragOver { x, y }, 0.0));
        assert_eq!(app.presentation(), Expanded, "a drag over the island opens the shelf");
        assert!(texts(&app, 4.0, &mut engine).iter().any(|s| s == "Drop to keep here"));
        app.drop_files(vec![("/home/a/Report final.pdf".into(), false), ("/home/a/Photos".into(), true)], 4.0);
        assert_eq!(app.take_shelf_changes().map(|v| v.len()), Some(2), "saved");
        let shown = texts(&app, 6.0, &mut engine);
        assert!(shown.iter().any(|s| s == "Report final.pdf") && shown.iter().any(|s| s == "Photos"), "{shown:?}");

        // Tap a file to open it; × removes it.
        let rows = content::list_rows(app.content_rect(app.pill_rect(6.0)), 2);
        press(&mut app, Rect::new(rows[0].x, rows[0].y, 100.0, rows[0].h), 6.0);
        assert_eq!(app.take_ui_actions(), [UiAction::Path("/home/a/Report final.pdf".into())]);
        press(&mut app, Rect::new(rows[1].right() - 30.0, rows[1].y, 20.0, rows[1].h), 7.0);
        assert_eq!(app.take_shelf_changes().map(|v| v.len()), Some(1));

        // A drag that leaves again closes what it opened.
        app.handle(Event::Key(Key::Escape), 8.0);
        app.tick(8.0 + AUTO_COLLAPSE_SECS);
        let (x, y) = centre(&app, 20.0);
        app.handle(Event::DragOver { x, y }, 20.0);
        app.handle(Event::DragLeave, 21.0);
        assert_eq!(app.presentation(), Idle);
    }

    #[test]
    fn hidden_apps_dont_reach_the_island() {
        let mut app = App::new(Demo::None, false);
        let mut p = app.prefs().clone();
        p.media_exclude = vec!["Player".into()];
        app.set_prefs(p);
        app.report(Report::Media(Some(song(true))), 0.0);
        app.tick(0.0);
        assert_eq!(app.presentation(), Idle, "{:?}", song(true).app_name);
    }

    #[test]
    fn screen_readers_hear_what_shows_and_can_press_buttons() {
        let mut app = App::new(Demo::None, false);
        app.report(Report::Media(Some(song(true))), 0.0);
        app.tick(0.0);
        let a = app.accessibility(1.0);
        assert!(a.label.starts_with("Now playing: "), "{}", a.label);
        assert!(!a.expanded && a.buttons.is_empty());
        assert!(app.a11y_press(None, 1.0), "pressing the island opens it");
        let a = app.accessibility(5.0);
        let names: Vec<&str> = a.buttons.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["Control Centre", "Previous", "Pause", "Next"]);
        app.a11y_press(Some(2), 5.0);
        assert_eq!(app.take_commands(), [MediaCommand::PlayPause]);
    }

    #[test]
    fn idle_island_shows_no_clock_and_never_wakes_for_it() {
        let mut app = App::new(Demo::None, false);
        app.set_prefs(IslandPrefs { idle_clock: false, ..IslandPrefs::default() });
        assert!(!app.set_time(NOW, 0.0), "nothing to redraw while idle");
        let mut engine = TextEngine::new();
        assert!(app.elements(0.0, &mut engine).is_empty());
        assert_eq!(app.next_timer(0.0), None, "zero wakeups when idle");
        assert_eq!(app.frame_interval(0.0), None);
    }

    #[test]
    fn the_empty_island_shows_the_time_and_ticks_once_a_minute() {
        let mut engine = TextEngine::new();
        let mut app = App::new(Demo::None, false);
        assert!(app.set_time(NOW, 0.0), "the time is on the empty pill by default");
        assert_eq!(texts(&app, 0.0, &mut engine), ["14:05"]);
        let wake = app.next_timer(0.0).expect("it wakes for the next minute");
        assert!((wake - 18.0).abs() < 1e-6, "{wake}");
        assert_eq!(app.frame_interval(0.0), None, "and draws nothing in between");
        assert!(app.accessibility(0.0).label.contains("14:05"));
        app.set_clock_style(false);
        assert_eq!(texts(&app, 0.0, &mut engine), ["2:05", "PM"]);

        // Music takes its place; the time comes back when it's gone.
        app.report(Report::Media(Some(song(true))), 1.0);
        assert!(!texts(&app, 4.0, &mut engine).iter().any(|s| s.contains(":05")));
        // Hover-only mode hides it with the pill, and then it doesn't wake.
        let mut hidden = App::new(Demo::None, false);
        hidden.set_prefs(IslandPrefs { auto_hide: true, ..IslandPrefs::default() });
        hidden.set_time(NOW, 0.0);
        hidden.tick(0.0);
        assert!(texts(&hidden, 3.0, &mut engine).is_empty());
        assert_eq!(hidden.next_timer(3.0), None);
    }

    #[test]
    fn open_island_shows_the_clock_and_ticks_each_minute() {
        let mut app = App::new(Demo::None, false);
        app.set_time(NOW, 0.0);
        click(&mut app, 0.0);
        let mut engine = TextEngine::new();
        assert_eq!(
            texts(&app, 5.0, &mut engine),
            ["14:05", "Wednesday, 4 November", "5 min", "Timer", "Wi-Fi", "Bluetooth"]
        );
        assert!((app.next_timer(5.0).unwrap() - 18.0).abs() < 1e-9, "wakes at the next minute");
        let later = LocalTime { minute: 6, second: 0, ..NOW };
        assert!(app.set_time(later, 23.0), "a new minute redraws");
        assert!(!app.set_time(LocalTime { second: 1, ..later }, 24.0), "seconds alone don't");
        app.set_clock_style(false);
        assert_eq!(&texts(&app, 25.0, &mut engine)[..2], ["2:06", "PM"]);
    }

    #[test]
    fn content_fades_out_before_the_pill_is_small() {
        let mut app = App::new(Demo::None, false);
        app.set_time(NOW, 0.0);
        click(&mut app, 0.0);
        click(&mut app, 5.0);
        let gone = (0..400).map(|i| 5.0 + i as f64 * 0.005).find(|&t| app.expanded_opacity(t) == 0.0).unwrap();
        let h = app.pill_rect(gone).h;
        assert!(h > 55.0, "content was still visible until the pill was {h} px tall");
    }

    #[test]
    fn idle_without_demo_has_no_timers_or_animation() {
        let app = App::new(Demo::None, false);
        assert!(!app.animating(0.0));
        assert_eq!(app.next_timer(0.0), None);
    }

    #[test]
    fn reduced_motion_snaps() {
        let mut app = App::new(Demo::None, true);
        click(&mut app, 0.0);
        assert!(!app.animating(0.05));
        assert_eq!(app.pill_rect(0.05).w, 372.0);
    }

    #[test]
    fn demos_drive_the_state_machine() {
        let mut toggle = App::new(Demo::Toggle, false);
        assert!(toggle.tick(1.2));
        assert_eq!(toggle.presentation(), Expanded);
        let mut tour = App::new(Demo::Tour, false);
        let mut seen = std::collections::HashSet::new();
        for i in 1..=10 {
            tour.tick(i as f64 * 1.6);
            seen.insert(tour.presentation());
        }
        for p in [Compact, Minimal, Expanded, Large, Idle, Peek] {
            assert!(seen.contains(&p), "tour never showed {p:?}");
        }
    }

    #[test]
    fn input_region_covers_both_ends_and_is_stable_while_animating() {
        let mut app = App::new(Demo::None, false);
        click(&mut app, 0.0);
        let region = app.input_region(0.06);
        assert!(region.w >= 372.0 && region.h >= CLOCK_H);
        assert_eq!(region, app.input_region(0.2));
        assert!(app.input_region(10.0).w < 380.0);
    }

    #[test]
    fn notch_mode_hides_idle_in_the_notch_and_keeps_content_beside_or_below_it() {
        let mut app = App::new(Demo::None, false);
        let mut engine = TextEngine::new();
        app.set_notch(Some((200.0, 32.0)), 0.0);
        let idle = app.pill_rect(0.0);
        assert_eq!((idle.w, idle.h), (200.0, 32.0), "idle is exactly the notch");
        assert!(app.frame(0.0).flat_top);

        app.report(Report::Media(Some(song(true))), 0.0);
        let p = app.pill_rect(5.0);
        let notch_left = (WINDOW.0 - 200.0) / 2.0;
        let art = app.elements(5.0, &mut engine).into_iter().find_map(|e| match e {
            Element::Art { x, size, .. } => Some((x as f64, size as f64)),
            _ => None,
        });
        let (ax, asize) = art.expect("album art in compact");
        assert!(ax + asize <= notch_left, "art sits left of the notch, not under it");
        assert!(p.w >= 200.0 + 2.0 * NOTCH_SIDE);

        click(&mut app, 5.0);
        let t = 10.0;
        let card = app.pill_rect(t);
        assert!(
            (card.h - (MEDIA_H + content::CARD_HEADER_H + 32.0)).abs() < 0.01,
            "the card grows by the notch height"
        );
        let (r, _) = content::media_controls(app.card_rect(t), false)[0];
        assert!(r.y >= card.y + 32.0 + content::CARD_HEADER_H, "controls are below the notch and header");
        assert!(content::home_button(app.content_rect(card)).y >= card.y + 32.0, "so is Control Centre's button");
    }

    #[test]
    fn every_state_fits_the_window_with_overshoot() {
        for p in [Idle, Peek, Compact, Minimal, Expanded, Large] {
            let (w, h, _) = target_size(p, None);
            let w = if p == Compact { content::COMPACT_BATTERY_W } else { w };
            let extra = if p == Minimal { MINIMAL_GAP + MINIMAL_CIRCLE } else { 0.0 };
            assert!((w + extra) * 1.03 <= WINDOW.0 && h * 1.03 + TOP_GAP <= WINDOW.1, "{p:?}");
        }
        // The tallest cards: a full device list with its footer, and Now
        // Playing with its header and lyrics; the widest bar: lyrics.
        let tallest = [
            content::list_height(content::MAX_ROWS, true),
            MEDIA_H + content::CARD_HEADER_H + content::CARD_LYRICS_H,
            CLOCK_H,
        ];
        for h in tallest {
            assert!(h + (h - 37.0) * 0.03 <= WINDOW.1, "a {h} px card is cut off");
        }
        assert!(content::COMPACT_LYRICS_W * 1.03 <= WINDOW.0);
    }

    fn bt_list(powered: bool) -> Report {
        use islet_core::connectivity::{BtDevice, BtState};
        let devices = (0..5)
            .map(|i| BtDevice {
                id: format!("d{i}"),
                name: format!("Device {i}"),
                kind: DeviceKind::Mouse,
                connected: false,
            })
            .collect();
        let bt = BtState { available: true, powered, devices, can_connect: false, note: Some("note".into()) };
        Report::Connectivity(ConnUpdate::Bluetooth(bt))
    }

    #[test]
    fn the_radio_switch_flips_at_once_and_follows_the_real_state() {
        let mut app = App::new(Demo::None, false);
        app.set_time(NOW, 0.0);
        app.report(bt_list(true), 0.0);
        click(&mut app, 0.0);
        let cr = app.content_rect(app.pill_rect(5.0));
        press(&mut app, content::clock_chips(cr, ClockRow::default())[1].0, 5.0);
        app.take_conn_commands();
        // Five devices and the footer: four rows show, the card fits the
        // window, and scrolling reaches the fifth.
        let card = app.pill_rect(9.0);
        assert!(card.bottom() + 1.0 <= WINDOW.1, "{card:?}");
        assert!(content::list_footer(app.content_rect(card), 4).bottom() <= card.bottom() - 10.0);
        let mut engine = TextEngine::new();
        assert!(!texts(&app, 9.0, &mut engine).iter().any(|s| s == "Device 4"));
        assert!(app.handle(Event::PointerMove { x: card.x + 60.0, y: card.y + 100.0 }, 9.0) || app.hovered);
        assert!(app.handle(Event::Scroll { dx: 0.0, dy: 1.0 }, 9.0));
        assert!(!app.handle(Event::Scroll { dx: 0.0, dy: 1.0 }, 9.1), "already at the end");
        let shown = texts(&app, 9.2, &mut engine);
        assert!(shown.iter().any(|s| s == "Device 4") && !shown.iter().any(|s| s == "Device 0"));
        // A tap on the last row picks the fifth device.
        let last = content::list_rows(app.content_rect(card), 4)[3];
        assert_eq!(app.conn_hit_at(last.x + 20.0, last.y + 10.0, 9.2), Some(ConnHit::Row(4)));
        app.handle(Event::Scroll { dx: 0.0, dy: -1.0 }, 9.3);

        // A tap a little beside the switch still counts, and it flips at once.
        let sw = content::power_switch(app.content_rect(card));
        press(&mut app, Rect::new(sw.x - 8.0, sw.y - 6.0, 4.0, 4.0), 9.0);
        assert_eq!(app.take_conn_commands(), [ConnCommand::SetPower(Radio::Bluetooth, false)]);
        assert!(!app.activities.connectivity().bt.powered, "off straight away");
        // The OS refused: the real state puts it back.
        app.report(bt_list(true), 9.5);
        assert!(app.activities.connectivity().bt.powered);
    }

    #[test]
    fn buttons_work_while_the_card_is_still_settling() {
        let mut app = App::new(Demo::None, false);
        app.set_time(NOW, 0.0);
        app.report(Report::Media(Some(song(true))), 0.0);
        click(&mut app, 1.0);
        let t = 1.35; // the card is still growing
        assert!(app.animating(t));
        let r = content::home_button(app.content_rect(app.settled_pill()));
        press(&mut app, r, t);
        assert_eq!(app.presentation(), Expanded, "the tap didn't close the island");
        assert!((app.pill_rect(t + 4.0).h - CLOCK_H).abs() < 0.01, "it opened Control Centre");
    }

    /// Shelf views for visual review:
    /// `ISLET_SHELF_SHEET=/path/out.png cargo test -p islet shelf_sheet`
    #[test]
    fn shelf_sheet() {
        let Some(out) = std::env::var_os("ISLET_SHELF_SHEET") else { return };
        use tiny_skia::{Color, Pixmap, PixmapPaint, Transform};
        let mut engine = TextEngine::new();
        let mut dragging = App::new(Demo::None, false);
        let (x, y) = centre(&dragging, 0.0);
        dragging.handle(Event::DragOver { x, y }, 0.0);
        let mut full = App::new(Demo::None, false);
        full.drop_files(
            vec![
                ("/home/a/Report final.pdf".into(), false),
                ("/home/a/Photos/Kampala trip".into(), true),
                ("/home/a/island-motion.mov".into(), false),
            ],
            0.0,
        );
        let mut clock = App::new(Demo::None, false);
        clock.set_shelf(full.shelf.clone());
        clock.set_time(NOW, 0.0);
        click(&mut clock, 0.0);
        let apps = [dragging, full, clock];
        let (cw, ch) = (WINDOW.0 as u32, 230u32);
        let mut sheet = Pixmap::new(cw * 3, ch).unwrap();
        sheet.fill(Color::from_rgba8(226, 230, 236, 255));
        for (i, a) in apps.iter().enumerate() {
            let t = 4.0;
            let mut cell = Pixmap::new(cw, WINDOW.1 as u32).unwrap();
            let frame = a.frame(t);
            islet_render::draw(&mut cell, &frame, 1.0);
            let clip = islet_render::clip_mask(&frame, cw, WINDOW.1 as u32, 1.0);
            let els = a.elements(t, &mut engine);
            islet_render::elements::draw_elements(&mut cell, &els, &mut engine, 1.0, clip.as_ref());
            sheet.draw_pixmap(
                i as i32 * cw as i32,
                0,
                cell.as_ref(),
                &PixmapPaint::default(),
                Transform::identity(),
                None,
            );
        }
        sheet.save_png(out).unwrap();
    }

    fn open_system_panel(app: &mut App, t: f64) {
        app.set_time(NOW, t);
        click(app, t);
        let cr = app.content_rect(app.pill_rect(t + 4.0));
        let (r, _) =
            content::clock_chips(cr, ClockRow::default()).into_iter().find(|(_, p)| *p == Panel::System).unwrap();
        let (x, y) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
        app.handle(Event::PointerMove { x, y }, t + 4.0);
        app.handle(Event::PointerDown { x, y, button: Button::Primary }, t + 4.0);
        app.handle(Event::PointerUp { x, y, button: Button::Primary }, t + 4.05);
    }

    /// Where a frame's time goes during expand/collapse at 2× (Retina):
    /// Per-frame cost with music and a long song's lyrics, in the bar and on
    /// the card: `cargo test --release -p islet lyrics_frame_cost -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn lyrics_frame_cost() {
        use std::time::Instant;
        let scale = 2.0f32;
        let mut engine = TextEngine::new();
        let mut app = App::new(Demo::None, false);
        app.set_time(NOW, 0.0);
        app.set_prefs(lyrics_on());
        app.report(Report::Media(Some(MediaInfo { position: Some(0.0), duration: Some(240.0), ..song(true) })), 0.0);
        let lrc: String = (0..80)
            .map(|i| format!("[{:02}:{:02}.00]This is lyric line number {i} of the song\n", i * 3 / 60, i * 3 % 60))
            .collect();
        let key = app.lyrics_wanted().unwrap().key();
        app.report(Report::Lyrics(key, Some(Arc::new(islet_core::lyrics::parse_lrc(&lrc)))), 0.0);
        let (w, h) = ((WINDOW.0 * 2.0) as u32, (WINDOW.1 * 2.0) as u32);
        let mut pixmap = tiny_skia::Pixmap::new(w, h).unwrap();
        let mut out = vec![0u8; (w * h * 4) as usize];
        for (name, open) in [("bar", false), ("card", true)] {
            if open {
                click(&mut app, 30.0);
            }
            let (mut els_t, mut paint_t, mut n) = (0.0, 0.0, 0);
            for i in 0..600 {
                let t = 31.0 + i as f64 / 60.0;
                let frame = app.frame(t);
                islet_render::draw(&mut pixmap, &frame, scale);
                let a = Instant::now();
                let els = app.elements(t, &mut engine);
                let clip = islet_render::elements::needs_clip(&frame, &els, &mut engine)
                    .then(|| islet_render::clip_mask(&frame, w, h, scale))
                    .flatten();
                let b = Instant::now();
                islet_render::elements::draw_elements_in(
                    &mut pixmap,
                    &els,
                    &mut engine,
                    scale,
                    clip.as_ref(),
                    Some(&frame),
                );
                islet_render::rgba_to_bgra(pixmap.data(), &mut out);
                paint_t += b.elapsed().as_secs_f64();
                els_t += (b - a).as_secs_f64();
                n += 1;
            }
            eprintln!(
                "{name}: layout {:.3} ms, paint {:.3} ms per frame",
                els_t * 1e3 / n as f64,
                paint_t * 1e3 / n as f64
            );
        }
    }

    /// `cargo test --release -p islet frame_cost -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn frame_cost() {
        use std::time::Instant;
        let scale = 2.0f32;
        let mut engine = TextEngine::new();
        let mut app = App::new(Demo::None, false);
        app.set_time(NOW, 0.0);
        let (w, h) = ((WINDOW.0 * 2.0) as u32, (WINDOW.1 * 2.0) as u32);
        let mut pixmap = tiny_skia::Pixmap::new(w, h).unwrap();
        let (mut t_draw, mut t_clip, mut t_els, mut t_paint, mut t_swizzle) = (0.0, 0.0, 0.0, 0.0, 0.0);
        let mut out = vec![0u8; (w * h * 4) as usize];
        let mut by_kind = [0.0f64; 4];
        let mut frames = 0;
        for cycle in 0..10 {
            let t0 = cycle as f64 * 2.0;
            click(&mut app, t0);
            for i in 0..120 {
                let t = t0 + i as f64 / 60.0;
                let frame = app.frame(t);
                let a = Instant::now();
                islet_render::draw(&mut pixmap, &frame, scale);
                let b = Instant::now();
                if app.has_content(t) {
                    let els = app.elements(t, &mut engine);
                    let clip = islet_render::elements::needs_clip(&frame, &els, &mut engine)
                        .then(|| islet_render::clip_mask(&frame, w, h, scale))
                        .flatten();
                    let c = Instant::now();
                    let d = Instant::now();
                    for el in &els {
                        let k = match el {
                            Element::Text(_) => 0,
                            Element::Bar { .. } => 1,
                            Element::Icon { .. } => 2,
                            _ => 3,
                        };
                        let s0 = Instant::now();
                        islet_render::elements::draw_elements(
                            &mut pixmap,
                            std::slice::from_ref(el),
                            &mut engine,
                            scale,
                            clip.as_ref(),
                        );
                        by_kind[k] += s0.elapsed().as_secs_f64();
                    }
                    let e = Instant::now();
                    t_clip += (c - b).as_secs_f64();
                    t_els += (d - c).as_secs_f64();
                    t_paint += (e - d).as_secs_f64();
                }
                let f = Instant::now();
                islet_render::rgba_to_bgra(pixmap.data(), &mut out);
                t_swizzle += f.elapsed().as_secs_f64();
                t_draw += (b - a).as_secs_f64();
                frames += 1;
            }
        }
        let ms = |x: f64| x * 1000.0 / frames as f64;
        eprintln!(
            "per frame: draw {:.3} ms, clip {:.3} ms, elements {:.3} ms, paint {:.3} ms, swizzle/copy {:.3} ms",
            ms(t_draw),
            ms(t_clip),
            ms(t_els),
            ms(t_paint),
            ms(t_swizzle)
        );
        eprintln!(
            "paint by kind: text {:.3} ms, bars {:.3} ms, icons {:.3} ms, other {:.3} ms",
            ms(by_kind[0]),
            ms(by_kind[1]),
            ms(by_kind[2]),
            ms(by_kind[3])
        );
    }

    #[test]
    fn clipboard_history_is_opt_in() {
        let mut app = App::new(Demo::None, false);
        app.report(Report::Clipboard("ignored".into()), 0.0);
        assert!(app.clip.is_empty(), "off by default");
        let mut prefs = IslandPrefs { clipboard: true, ..Default::default() };
        app.set_prefs(prefs.clone());
        app.report(Report::Clipboard("first".into()), 0.0);
        app.report(Report::Clipboard("second copy\nwith two lines".into()), 0.0);
        app.set_time(NOW, 0.0);
        click(&mut app, 0.0);
        let cr = app.content_rect(app.pill_rect(5.0));
        let chips = content::clock_chips(cr, ClockRow { clipboard: true, ..ClockRow::default() });
        let (chip, _) = chips.iter().find(|(_, p)| *p == Panel::Clipboard).copied().unwrap();
        press(&mut app, chip, 5.0);
        let mut engine = TextEngine::new();
        let shown = texts(&app, 9.0, &mut engine);
        assert!(shown.iter().any(|s| s == "second copy …"), "{shown:?}");
        // Tapping the older entry copies it again (and moves it to the top).
        let rows = content::list_rows(app.content_rect(app.pill_rect(9.0)), 2);
        press(&mut app, rows[1], 9.0);
        assert_eq!(app.take_clipboard_copies(), ["first"]);
        assert_eq!(app.clip.get(0).map(String::as_str), Some("first"));
        // Clear history.
        let footer = content::list_footer(app.content_rect(app.pill_rect(9.0)), 2);
        press(&mut app, footer, 9.0);
        assert!(app.clip.is_empty());
        // Turning it off forgets everything and closes the panel.
        app.report(Report::Clipboard("third".into()), 10.0);
        prefs.clipboard = false;
        app.set_prefs(prefs);
        assert!(app.clip.is_empty() && app.panel.is_none());
    }

    #[test]
    fn system_monitor_samples_only_while_open() {
        let mut app = App::new(Demo::None, false);
        assert!(!app.sysmon_wanted());
        open_system_panel(&mut app, 0.0);
        assert!(app.sysmon_wanted());
        assert!((app.pill_rect(9.0).h - content::SYSTEM_H).abs() < 0.5);
        let gb = 1 << 30;
        let stats = islet_core::sysmon::Stats {
            cpu: 0.23,
            mem_used: 5 * gb,
            mem_total: 16 * gb,
            disk_used: 400 * gb,
            disk_total: 512 * gb,
        };
        assert!(app.report(Report::System(Some(stats)), 9.0));
        let mut engine = TextEngine::new();
        let texts: Vec<String> = app
            .elements(9.0, &mut engine)
            .into_iter()
            .filter_map(|e| match e {
                Element::Text(t) => Some(t.text),
                _ => None,
            })
            .collect();
        assert!(texts.iter().any(|t| t == "23%") && texts.iter().any(|t| t == "5.0 of 16 GB"), "{texts:?}");
        // Back to the clock card stops sampling; so does closing the island.
        let cr = app.content_rect(app.pill_rect(9.0));
        let back = content::list_header(cr);
        let (x, y) = (back.x + 20.0, back.y + 20.0);
        app.handle(Event::PointerDown { x, y, button: Button::Primary }, 9.0);
        app.handle(Event::PointerUp { x, y, button: Button::Primary }, 9.05);
        assert!(!app.sysmon_wanted());
        app.handle(Event::Key(Key::Escape), 10.0);
        open_system_panel(&mut app, 20.0);
        assert!(app.sysmon_wanted());
        app.handle(Event::Key(Key::Escape), 30.0);
        assert!(!app.sysmon_wanted());
    }

    /// System monitor and camera/microphone dots for visual review:
    /// `ISLET_SYSTEM_SHEET=/path/out.png cargo test -p islet system_sheet`
    #[test]
    fn system_sheet() {
        let Some(out) = std::env::var_os("ISLET_SYSTEM_SHEET") else { return };
        use islet_core::activity::{Custom, Privacy};
        use tiny_skia::{Color, Pixmap, PixmapPaint, Transform};
        let mut engine = TextEngine::new();
        let gb = 1 << 30;
        let mut clock = App::new(Demo::None, false);
        clock.set_time(NOW, 0.0);
        click(&mut clock, 0.0);
        let mut system = App::new(Demo::None, false);
        open_system_panel(&mut system, 0.0);
        let stats = islet_core::sysmon::Stats {
            cpu: 0.37,
            mem_used: 13 * gb,
            mem_total: 16 * gb,
            disk_used: 470 * gb,
            disk_total: 512 * gb,
        };
        system.report(Report::System(Some(stats)), 0.0);
        let mut idle = App::new(Demo::None, false);
        idle.report(Report::Privacy(Privacy { camera: None, mic: Some(vec![]) }), 0.0);
        let mut compact = App::new(Demo::None, false);
        compact.report(Report::Privacy(Privacy { camera: Some(vec![]), mic: None }), 0.0);
        let job = Custom {
            id: "b".into(),
            title: "Build".into(),
            subtitle: String::new(),
            progress: Some(0.6),
            color: None,
            open: None,
        };
        compact.report(Report::Custom(job), 0.0);
        let apps = [(clock, 4.0), (system, 9.0), (idle, 4.0), (compact, 4.0)];
        let (cw, ch) = (WINDOW.0 as u32, 240u32);
        let mut sheet = Pixmap::new(cw * apps.len() as u32, ch).unwrap();
        sheet.fill(Color::from_rgba8(226, 230, 236, 255));
        for (i, (a, t)) in apps.iter().enumerate() {
            let mut cell = Pixmap::new(cw, WINDOW.1 as u32).unwrap();
            let frame = a.frame(*t);
            islet_render::draw(&mut cell, &frame, 1.0);
            let clip = islet_render::clip_mask(&frame, cw, WINDOW.1 as u32, 1.0);
            let els = a.elements(*t, &mut engine);
            islet_render::elements::draw_elements(&mut cell, &els, &mut engine, 1.0, clip.as_ref());
            sheet.draw_pixmap(
                i as i32 * cw as i32,
                0,
                cell.as_ref(),
                &PixmapPaint::default(),
                Transform::identity(),
                None,
            );
        }
        sheet.save_png(out).unwrap();
    }

    /// Notification views for visual review:
    /// `ISLET_NOTE_SHEET=/path/out.png cargo test -p islet note_sheet`
    #[test]
    fn note_sheet() {
        let Some(out) = std::env::var_os("ISLET_NOTE_SHEET") else { return };
        use tiny_skia::{Color, Pixmap, PixmapPaint, Transform};
        let mut engine = TextEngine::new();
        let make = |open: bool| {
            let mut a = App::new(Demo::None, false);
            a.report(note("Calendar", "Dentist tomorrow", "9:30 at Kampala Dental"), 0.0);
            a.report(note("Chat", "Amina", "See you at 5, bring the charger please"), 0.0);
            a.report(
                note("Mail", "Invoice #42 from Islet Supplies", "Due Friday. Reply to this email with questions."),
                0.0,
            );
            a.tick(0.0);
            if open {
                click(&mut a, 0.1);
            }
            a
        };
        let apps = [make(false), make(true)];
        let (cw, ch) = (WINDOW.0 as u32, 240u32);
        let mut sheet = Pixmap::new(cw * 2, ch).unwrap();
        sheet.fill(Color::from_rgba8(226, 230, 236, 255));
        for (i, a) in apps.iter().enumerate() {
            let t = 3.0;
            let mut cell = Pixmap::new(cw, WINDOW.1 as u32).unwrap();
            let frame = a.frame(t);
            islet_render::draw(&mut cell, &frame, 1.0);
            let clip = islet_render::clip_mask(&frame, cw, WINDOW.1 as u32, 1.0);
            let els = a.elements(t, &mut engine);
            islet_render::elements::draw_elements(&mut cell, &els, &mut engine, 1.0, clip.as_ref());
            sheet.draw_pixmap(
                i as i32 * cw as i32,
                0,
                cell.as_ref(),
                &PixmapPaint::default(),
                Transform::identity(),
                None,
            );
        }
        sheet.save_png(out).unwrap();
    }

    /// Calendar views for visual review:
    /// `ISLET_CAL_SHEET=/path/out.png cargo test -p islet calendar_sheet`
    #[test]
    fn calendar_sheet() {
        let Some(out) = std::env::var_os("ISLET_CAL_SHEET") else { return };
        use tiny_skia::{Color, Pixmap, PixmapPaint, Transform};
        let mut engine = TextEngine::new();
        let make = |open: bool| {
            let mut a = App::new(Demo::None, false);
            let wall0 = 1_800_000_000.0;
            a.set_wall(wall0, 3 * 3600, 0.0);
            let mut lunch = standup(1_800_005_400);
            lunch.summary = "Lunch with Amina".into();
            lunch.location = None;
            let mut review = standup(1_800_012_600);
            review.summary = "Design review: island motion".into();
            review.location = Some("Google Meet".into());
            a.report(Report::Calendar(Some(vec![standup(1_800_000_480), lunch, review])), 0.0);
            a.tick(0.0);
            if open {
                click(&mut a, 0.1);
            }
            a
        };
        let apps = [make(false), make(true)];
        let (cw, ch) = (WINDOW.0 as u32, 230u32);
        let mut sheet = Pixmap::new(cw * 2, ch).unwrap();
        sheet.fill(Color::from_rgba8(226, 230, 236, 255));
        for (i, a) in apps.iter().enumerate() {
            let t = 5.0;
            let mut cell = Pixmap::new(cw, WINDOW.1 as u32).unwrap();
            let frame = a.frame(t);
            islet_render::draw(&mut cell, &frame, 1.0);
            let clip = islet_render::clip_mask(&frame, cw, WINDOW.1 as u32, 1.0);
            let els = a.elements(t, &mut engine);
            islet_render::elements::draw_elements(&mut cell, &els, &mut engine, 1.0, clip.as_ref());
            sheet.draw_pixmap(
                i as i32 * cw as i32,
                0,
                cell.as_ref(),
                &PixmapPaint::default(),
                Transform::identity(),
                None,
            );
        }
        sheet.save_png(out).unwrap();
    }

    /// The clock card with weather, for visual review:
    /// `ISLET_WEATHER_SHEET=/path/out.png cargo test -p islet weather_sheet`
    #[test]
    fn weather_sheet() {
        let Some(out) = std::env::var_os("ISLET_WEATHER_SHEET") else { return };
        use islet_core::weather::{Sky, Weather};
        use tiny_skia::{Color, Pixmap, PixmapPaint, Transform};
        let mut engine = TextEngine::new();
        let skies = [(Sky::Rain, true), (Sky::PartlyCloudy, true), (Sky::Clear, false), (Sky::Storm, true)];
        let (cw, ch) = (WINDOW.0 as u32, 180u32);
        let mut sheet = Pixmap::new(cw * 2, ch * 2).unwrap();
        sheet.fill(Color::from_rgba8(226, 230, 236, 255));
        for (i, (sky, is_day)) in skies.into_iter().enumerate() {
            let mut a = App::new(Demo::None, false);
            a.set_time(NOW, 0.0);
            let w =
                Weather { place: "Kampala".into(), temp_c: 24.3, high_c: Some(27.0), low_c: Some(18.0), sky, is_day };
            a.report(Report::Weather(Some(w)), 0.0);
            click(&mut a, 0.0);
            let t = 5.0;
            let mut cell = Pixmap::new(cw, WINDOW.1 as u32).unwrap();
            let frame = a.frame(t);
            islet_render::draw(&mut cell, &frame, 1.0);
            let clip = islet_render::clip_mask(&frame, cw, WINDOW.1 as u32, 1.0);
            let els = a.elements(t, &mut engine);
            islet_render::elements::draw_elements(&mut cell, &els, &mut engine, 1.0, clip.as_ref());
            let (x, y) = ((i % 2) as i32 * cw as i32, (i / 2) as i32 * ch as i32);
            sheet.draw_pixmap(x, y, cell.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
        }
        sheet.save_png(out).unwrap();
    }

    fn laptop(percent: u8, charging: bool) -> islet_core::activity::BatteryDevice {
        islet_core::activity::BatteryDevice {
            id: "laptop".into(),
            name: "Battery".into(),
            kind: islet_core::activity::DeviceKind::Laptop,
            percent,
            charging,
            parts: vec![],
        }
    }

    #[test]
    fn plugging_in_shows_the_charging_notification() {
        let mut engine = TextEngine::new();
        let mut app = App::new(Demo::None, false);
        app.report(Report::Laptop(laptop(57, false)), 0.0);
        assert!(app.report(Report::Laptop(laptop(57, true)), 1.0));
        assert_eq!(app.presentation(), Compact);
        let shown = texts(&app, 1.5, &mut engine);
        assert!(shown.iter().any(|s| s == "Charging") && shown.iter().any(|s| s == "57%"), "{shown:?}");
        assert!(app
            .elements(1.5, &mut engine)
            .iter()
            .any(|e| matches!(e, Element::Icon { icon: islet_render::elements::Icon::Battery(57), .. })));
        assert_eq!(app.accessibility(1.5).label, "Charging, 57%");
        // It goes away by itself, and unplugging says so.
        assert!(app.tick(10.0) || app.presentation() != Compact);
        app.report(Report::Laptop(laptop(57, false)), 20.0);
        assert!(texts(&app, 20.5, &mut engine).iter().any(|s| s == "On battery"));
    }

    /// Charging notifications for visual review:
    /// `ISLET_CHARGE_SHEET=/path/out.png cargo test -p islet charge_sheet`
    #[test]
    fn charge_sheet() {
        let Some(out) = std::env::var_os("ISLET_CHARGE_SHEET") else { return };
        use tiny_skia::{Color, Pixmap, PixmapPaint, Transform};
        let mut engine = TextEngine::new();
        let state = |before: (u8, bool), after: (u8, bool)| {
            let mut a = App::new(Demo::None, false);
            a.report(Report::Laptop(laptop(before.0, before.1)), 0.0);
            a.report(Report::Laptop(laptop(after.0, after.1)), 1.0);
            a
        };
        let mut buds = App::new(Demo::None, false);
        let b = |charging| islet_core::activity::BatteryDevice {
            id: "b".into(),
            name: "Galaxy Buds2".into(),
            kind: islet_core::activity::DeviceKind::Earbuds,
            percent: 64,
            charging,
            parts: vec![],
        };
        buds.report(Report::DeviceConnected(b(false)), -100.0);
        buds.report(Report::DeviceChanged(b(true)), 0.0);
        let apps =
            [state((57, false), (57, true)), state((18, true), (18, false)), state((99, true), (100, true)), buds];
        let (cw, ch) = (WINDOW.0 as u32, 50u32);
        let mut sheet = Pixmap::new(cw * 2, ch * 2).unwrap();
        sheet.fill(Color::from_rgba8(226, 230, 236, 255));
        for (i, a) in apps.iter().enumerate() {
            let t = 1.6;
            let mut cell = Pixmap::new(cw, WINDOW.1 as u32).unwrap();
            let frame = a.frame(t);
            islet_render::draw(&mut cell, &frame, 1.0);
            let els = a.elements(t, &mut engine);
            islet_render::elements::draw_elements(&mut cell, &els, &mut engine, 1.0, None);
            let (x, y) = ((i % 2) as i32 * cw as i32, (i / 2) as i32 * ch as i32);
            sheet.draw_pixmap(x, y, cell.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
        }
        sheet.save_png(out).unwrap();
    }

    /// HUD screens for visual review:
    /// `ISLET_HUD_SHEET=/path/out.png cargo test -p islet hud_sheet`
    #[test]
    fn hud_sheet() {
        let Some(out) = std::env::var_os("ISLET_HUD_SHEET") else { return };
        use tiny_skia::{Color, Pixmap, PixmapPaint, Transform};
        let mut engine = TextEngine::new();
        let huds = [
            Hud::Volume { level: 0.62, muted: false },
            Hud::Volume { level: 0.62, muted: true },
            Hud::Brightness(0.35),
            Hud::CapsLock(true),
        ];
        let (cw, ch) = (WINDOW.0 as u32, 50u32);
        let mut sheet = Pixmap::new(cw * 2, ch * 3).unwrap();
        sheet.fill(Color::from_rgba8(226, 230, 236, 255));
        let mut cells: Vec<App> = huds
            .iter()
            .map(|h| {
                let mut a = App::new(Demo::None, false);
                a.report(Report::Hud(*h), 0.0);
                a.tick(0.0);
                a
            })
            .collect();
        let mut with_music = App::new(Demo::None, false);
        with_music.report(Report::Media(Some(song(true))), 0.0);
        with_music.report(Report::Hud(Hud::Volume { level: 0.3, muted: false }), 0.0);
        with_music.tick(0.0);
        cells.push(with_music);
        for (i, a) in cells.iter().enumerate() {
            let t = 0.9;
            let mut cell = Pixmap::new(cw, WINDOW.1 as u32).unwrap();
            let frame = a.frame(t);
            islet_render::draw(&mut cell, &frame, 1.0);
            let clip = islet_render::clip_mask(&frame, cw, WINDOW.1 as u32, 1.0);
            let els = a.elements(t, &mut engine);
            islet_render::elements::draw_elements(&mut cell, &els, &mut engine, 1.0, clip.as_ref());
            let (x, y) = ((i % 2) as i32 * cw as i32, (i / 2) as i32 * ch as i32);
            sheet.draw_pixmap(x, y, cell.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
        }
        sheet.save_png(out).unwrap();
    }

    /// Timer screens for visual review:
    /// `ISLET_TIMER_SHEET=/path/out.png cargo test -p islet timer_sheet`
    #[test]
    fn timer_sheet() {
        let Some(out) = std::env::var_os("ISLET_TIMER_SHEET") else { return };
        use tiny_skia::{Color, Pixmap, PixmapPaint, Transform};
        let mut engine = TextEngine::new();
        let start = |cmd: TimerCommand, music: bool, open: bool| {
            let mut a = App::new(Demo::None, false);
            a.set_time(NOW, 0.0);
            if music {
                a.report(Report::Media(Some(song(true))), 0.0);
            }
            a.timer_command(cmd, 0.0);
            a.tick(0.0);
            if open {
                click(&mut a, 0.1);
            }
            a
        };
        let tea = || TimerCommand::Start { secs: 600.0, label: "Tea".into() };
        let focus = || TimerCommand::Focus { work: 1500.0, brk: 300.0, rounds: 4 };
        let mut paused = start(focus(), false, false);
        paused.timer_command(TimerCommand::Pause, 0.2);
        click(&mut paused, 0.3);
        let apps = [start(tea(), false, false), start(tea(), true, false), start(focus(), false, true), paused];
        let (cw, ch) = (WINDOW.0 as u32, 170u32);
        let mut sheet = Pixmap::new(cw * 2, ch * 2).unwrap();
        sheet.fill(Color::from_rgba8(226, 230, 236, 255));
        for (i, a) in apps.iter().enumerate() {
            let t = 250.0;
            let mut cell = Pixmap::new(cw, WINDOW.1 as u32).unwrap();
            let frame = a.frame(t);
            islet_render::draw(&mut cell, &frame, 1.0);
            let clip = islet_render::clip_mask(&frame, cw, WINDOW.1 as u32, 1.0);
            let els = a.elements(t, &mut engine);
            islet_render::elements::draw_elements(&mut cell, &els, &mut engine, 1.0, clip.as_ref());
            let (x, y) = ((i % 2) as i32 * cw as i32, (i / 2) as i32 * ch as i32);
            sheet.draw_pixmap(x, y, cell.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
        }
        sheet.save_png(out).unwrap();
    }

    /// The clock card, calendar, stopwatch and to-do list for visual review:
    /// `ISLET_PRO_SHEET=/path/out.png cargo test -p islet pro_sheet`
    #[test]
    fn pro_sheet() {
        let Some(out) = std::env::var_os("ISLET_PRO_SHEET") else { return };
        use content::Quick;
        use tiny_skia::{Color, Pixmap, PixmapPaint, Transform};
        let mut engine = TextEngine::new();
        let open = |pro: bool| {
            let mut a = App::new(Demo::None, false);
            a.set_time(NOW, 0.0);
            a.set_wall(1_793_801_100.0, 0, 0.0);
            a.set_pro(pro);
            a.set_alarms(vec![Alarm { id: 1, hour: 7, minute: 30, label: "Up".into(), days: 0, enabled: true }]);
            let mut p = a.prefs().clone();
            p.world_clocks = vec!["London".into(), "Tokyo".into()];
            a.set_prefs(p);
            let mut todo = islet_core::todo::Todo::default();
            for (i, s) in ["Buy milk", "Call mum", "Finish the essay"].iter().enumerate() {
                let id = todo.add(s).unwrap();
                if i == 0 {
                    todo.toggle(id);
                }
            }
            a.set_todo(todo);
            let at = |d: i64, h: i64| 1_793_750_400 + (d - 4) * 86_400 + h * 3600;
            let ev = |summary: &str, start, end, all_day| islet_core::calendar::CalEvent {
                summary: summary.into(),
                start,
                end,
                all_day,
                location: None,
            };
            let events = vec![
                ev("Dentist", at(4, 16), at(4, 17), false),
                ev("Football practice", at(5, 9), at(5, 10), false),
                ev("Trip to Jinja", at(20, 0), at(22, 0), true),
            ];
            a.report(Report::Calendar(Some(events)), 0.0);
            click(&mut a, 0.0);
            a
        };
        let free = open(false);
        let pro = open(true);
        let mut calendar = open(true);
        press_quick(&mut calendar, 5.0, Quick::Calendar);
        let panel = |p: Panel| {
            let mut a = open(true);
            let cr = a.content_rect(a.pill_rect(5.0));
            let (r, _) = *content::clock_chips(cr, a.clock_row()).iter().find(|(_, x)| *x == p).unwrap();
            press(&mut a, r, 5.0);
            a
        };
        let todo = panel(Panel::Todo);
        let mut stopwatch = panel(Panel::Stopwatch);
        let [_, (start, _)] = content::stopwatch_controls(stopwatch.card_rect(9.0));
        press(&mut stopwatch, start, 9.0);
        let [(lap, _), _] = content::stopwatch_controls(stopwatch.card_rect(40.0));
        press(&mut stopwatch, lap, 40.0);
        let mut pill = open(true);
        pill.stopwatch_command(StopwatchCommand::Start, 6.0);
        pill.handle(Event::Key(Key::Escape), 7.0);
        pill.tick(8.0);
        let apps = [free, pro, calendar, todo, stopwatch, pill];
        let (cw, ch) = (WINDOW.0 as u32, WINDOW.1 as u32);
        let mut sheet = Pixmap::new(cw * 2, ch * 3).unwrap();
        sheet.fill(Color::from_rgba8(226, 230, 236, 255));
        for (i, a) in apps.iter().enumerate() {
            let t = 250.0;
            let mut cell = Pixmap::new(cw, ch).unwrap();
            let frame = a.frame(t);
            islet_render::draw(&mut cell, &frame, 1.0);
            let clip = islet_render::clip_mask(&frame, cw, ch, 1.0);
            let els = a.elements(t, &mut engine);
            islet_render::elements::draw_elements(&mut cell, &els, &mut engine, 1.0, clip.as_ref());
            let (x, y) = ((i % 2) as i32 * cw as i32, (i / 2) as i32 * ch as i32);
            sheet.draw_pixmap(x, y, cell.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
        }
        sheet.save_png(out).unwrap();
    }

    /// Wi-Fi and Bluetooth screens for visual review:
    /// `ISLET_CONN_SHEET=/path/out.png cargo test -p islet conn_sheet`
    #[test]
    fn conn_sheet() {
        let Some(out) = std::env::var_os("ISLET_CONN_SHEET") else { return };
        use islet_core::connectivity::{BtDevice, BtState, ConnUpdate, WifiNetwork};
        use tiny_skia::{Color, Pixmap, PixmapPaint, Transform};
        let mut engine = TextEngine::new();
        let mut w = wifi(Some("Home"));
        w.networks.push(WifiNetwork {
            ssid: "Library guest".into(),
            strength: 30,
            secure: false,
            known: false,
            active: false,
        });
        w.networks.push(WifiNetwork {
            ssid: "Neighbour 5G".into(),
            strength: 50,
            secure: true,
            known: false,
            active: false,
        });
        let bt = BtState {
            available: true,
            powered: true,
            devices: vec![
                BtDevice { id: "1".into(), name: "Galaxy Buds2".into(), kind: DeviceKind::Earbuds, connected: true },
                BtDevice { id: "2".into(), name: "MX Master 3".into(), kind: DeviceKind::Mouse, connected: false },
                BtDevice { id: "3".into(), name: "Keychron K2".into(), kind: DeviceKind::Keyboard, connected: false },
                BtDevice {
                    id: "4".into(),
                    name: "DualSense Wireless Controller".into(),
                    kind: DeviceKind::Gamepad,
                    connected: false,
                },
                BtDevice { id: "5".into(), name: "Watch".into(), kind: DeviceKind::Phone, connected: false },
            ],
            can_connect: false,
            note: Some("Windows connects Bluetooth devices in its own settings".into()),
        };
        let make = |panel: Option<usize>, fail: bool| {
            let mut a = App::new(Demo::None, false);
            a.set_time(NOW, 0.0);
            a.report(Report::Connectivity(ConnUpdate::Wifi(w.clone())), 0.0);
            a.report(Report::Connectivity(ConnUpdate::Bluetooth(bt.clone())), 0.0);
            click(&mut a, 0.0);
            if let Some(i) = panel {
                let cr = a.content_rect(a.pill_rect(5.0));
                press(&mut a, content::clock_chips(cr, ClockRow::default())[i].0, 5.0);
                if fail {
                    a.report(
                        Report::Connectivity(ConnUpdate::Failed {
                            message: String::new(),
                            needs_password: Some("Neighbour 5G".into()),
                        }),
                        5.2,
                    );
                }
            }
            a
        };
        let apps = [make(None, false), make(Some(0), false), make(Some(0), true), make(Some(1), false)];
        let (cw, ch) = (WINDOW.0 as u32, WINDOW.1 as u32);
        let mut sheet = Pixmap::new(cw * 2, ch * 2).unwrap();
        sheet.fill(Color::from_rgba8(226, 230, 236, 255));
        for (i, a) in apps.iter().enumerate() {
            let mut cell = Pixmap::new(cw, ch).unwrap();
            let frame = a.frame(12.0);
            islet_render::draw(&mut cell, &frame, 1.0);
            let clip = islet_render::clip_mask(&frame, cw, ch, 1.0);
            let els = a.elements(12.0, &mut engine);
            islet_render::elements::draw_elements(&mut cell, &els, &mut engine, 1.0, clip.as_ref());
            let (x, y) = ((i % 2) as i32 * cw as i32, (i / 2) as i32 * ch as i32);
            sheet.draw_pixmap(x, y, cell.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
        }
        sheet.save_png(out).unwrap();
    }

    /// Alarm frames for visual review:
    /// `ISLET_ALARM_SHEET=/path/out.png cargo test -p islet alarm_sheet`
    #[test]
    fn alarm_sheet() {
        let Some(out) = std::env::var_os("ISLET_ALARM_SHEET") else { return };
        use tiny_skia::{Color, Pixmap, PixmapPaint, Transform};
        let mut engine = TextEngine::new();
        let alarm = Alarm {
            id: 1,
            hour: 6,
            minute: 30,
            label: "Wake up".into(),
            days: islet_core::alarm::WEEKDAYS,
            enabled: true,
        };
        let mut app = App::new(Demo::None, false);
        app.set_clock_style(false);
        app.set_alarms(vec![alarm]);
        app.set_time(LocalTime { hour: 6, minute: 29, ..NOW }, 0.0);
        app.set_time(LocalTime { hour: 6, minute: 30, second: 0, ..NOW }, 1.0);
        let mut music = App::new(Demo::None, false);
        music.set_clock_style(false);
        music.report(Report::Media(Some(song(true))), 0.0);
        music.set_alarms(app.alarms.alarms().to_vec());
        music.set_time(LocalTime { hour: 6, minute: 29, ..NOW }, 0.0);
        music.set_time(LocalTime { hour: 6, minute: 30, second: 0, ..NOW }, 1.0);
        music.handle(Event::Key(Key::Escape), 1.5);
        let mut frames: Vec<(&App, f64)> = vec![(&app, 4.0), (&app, 4.1), (&app, 4.2)];
        let mut compact = App::new(Demo::None, false);
        compact.set_alarms(app.alarms.alarms().to_vec());
        compact.set_time(LocalTime { hour: 6, minute: 29, ..NOW }, 0.0);
        compact.set_time(LocalTime { hour: 6, minute: 30, second: 0, ..NOW }, 1.0);
        compact.handle(Event::Key(Key::Escape), 1.5);
        frames.extend([(&compact, 5.0), (&compact, 5.05), (&music, 5.1)]);
        let (cw, ch) = (WINDOW.0 as u32, 180u32);
        let mut sheet = Pixmap::new(cw * 3, ch * 2).unwrap();
        sheet.fill(Color::from_rgba8(226, 230, 236, 255));
        for (i, (a, t)) in frames.iter().enumerate() {
            let mut cell = Pixmap::new(cw, WINDOW.1 as u32).unwrap();
            let frame = a.frame(*t);
            islet_render::draw(&mut cell, &frame, 1.0);
            let clip = islet_render::clip_mask(&frame, cw, WINDOW.1 as u32, 1.0);
            let els = a.elements(*t, &mut engine);
            islet_render::elements::draw_elements(&mut cell, &els, &mut engine, 1.0, clip.as_ref());
            let (x, y) = ((i % 3) as i32 * cw as i32, (i / 3) as i32 * ch as i32);
            sheet.draw_pixmap(x, y, cell.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
        }
        sheet.save_png(out).unwrap();
    }

    /// Renders a scripted session (music, a device connecting, the open
    /// player, the device list) into one PNG for visual review:
    /// `ISLET_CONTACT_SHEET=/path/out.png cargo test -p islet contact_sheet`
    #[test]
    fn contact_sheet() {
        let Some(out) = std::env::var_os("ISLET_CONTACT_SHEET") else { return };
        use islet_core::activity::ArtImage;
        use std::sync::Arc;
        use tiny_skia::{Color, Pixmap, PixmapPaint, Transform};
        // A generated gradient stands in for album art.
        let (aw, ah) = (64u32, 64u32);
        let mut rgba = Vec::with_capacity((aw * ah * 4) as usize);
        for y in 0..ah {
            for x in 0..aw {
                rgba.extend_from_slice(&[(40 + x * 3) as u8, (20 + y * 2) as u8, 160, 255]);
            }
        }
        let art = ArtImage { width: aw, height: ah, rgba: Arc::new(rgba), dominant: [120, 70, 160] };
        let mut app = App::new(Demo::None, false);
        app.set_time(NOW, 0.0);
        if std::env::var_os("ISLET_CONTACT_NOTCH").is_some() {
            app.set_notch(Some((200.0, 32.0)), 0.0);
        }
        let mut engine = TextEngine::new();
        let mut m = MediaInfo {
            art: Some(art),
            title: "Midnight Signals".into(),
            artist: "The Placeholders".into(),
            ..song(true)
        };
        m.position = Some(37.0);
        type Step = (f64, Box<dyn Fn(&mut App, f64)>);
        let script: Vec<Step> = vec![
            (
                0.2,
                Box::new(move |a: &mut App, t| {
                    a.report(Report::Media(Some(m.clone())), t);
                }),
            ),
            (
                2.0,
                Box::new(|a: &mut App, t| {
                    a.report(Report::DeviceConnected(buds()), t);
                }),
            ),
            (7.0, Box::new(|a: &mut App, t| click(a, t))),
            (10.0, Box::new(|a: &mut App, t| click(a, t))),
            (
                12.0,
                Box::new(|a: &mut App, t| {
                    a.report(
                        Report::DeviceConnected(BatteryDevice {
                            id: "m".into(),
                            name: "Wireless Mouse".into(),
                            kind: DeviceKind::Mouse,
                            percent: 15,
                            charging: false,
                            parts: vec![],
                        }),
                        t,
                    );
                }),
            ),
            (12.6, Box::new(|a: &mut App, t| click(a, t))),
            (14.4, Box::new(|a: &mut App, t| click(a, t))),
            (
                15.0,
                Box::new(|a: &mut App, t| {
                    use islet_core::nav::{RouteInfo, TravelMode, Trip};
                    a.report(
                        Report::Navigation(Some(NavInfo {
                            trip: Trip {
                                label: "Work".into(),
                                destination: "Plot 5, Kampala Road".into(),
                                origin: "Ntinda".into(),
                                mode: TravelMode::Drive,
                                place_id: None,
                            },
                            route: Some(RouteInfo {
                                duration: 1530.0,
                                free_flow: 1200.0,
                                distance_m: 12345.0,
                                first_step: Some("Turn left onto Kampala Rd".into()),
                                maneuver: Some("TURN_LEFT".into()),
                            }),
                            error: None,
                        })),
                        t,
                    );
                }),
            ),
            (16.5, Box::new(|a: &mut App, t| click(a, t))),
        ];
        let times: Vec<f64> = (0..60).map(|i| i as f64 * 0.3).collect();
        let cols = 6usize;
        let rows = times.len().div_ceil(cols);
        let (cw, ch) = (WINDOW.0 as u32, WINDOW.1 as u32);
        let mut sheet = Pixmap::new(cw * cols as u32, ch * rows as u32).unwrap();
        sheet.fill(Color::from_rgba8(226, 230, 236, 255));
        let mut cell = Pixmap::new(cw, ch).unwrap();
        let mut next = 0;
        for (i, &t) in times.iter().enumerate() {
            while next < script.len() && script[next].0 <= t {
                (script[next].1)(&mut app, script[next].0);
                next += 1;
            }
            app.tick(t);
            let frame = app.frame(t);
            islet_render::draw(&mut cell, &frame, 1.0);
            let clip = islet_render::clip_mask(&frame, cw, ch, 1.0);
            let els = app.elements(t, &mut engine);
            islet_render::elements::draw_elements(&mut cell, &els, &mut engine, 1.0, clip.as_ref());
            let (x, y) = ((i % cols) as i32 * cw as i32, (i / cols) as i32 * ch as i32);
            sheet.draw_pixmap(x, y, cell.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
        }
        sheet.save_png(out).unwrap();
    }
}
