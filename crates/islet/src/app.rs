//! Portable island logic: pointer input and provider reports → presentation
//! state machine → springs → frame and content. No OS calls in here, so all
//! of it is unit-tested.

use crate::content::{self, AlarmControl, ConnHit, Control, NavControl, Panel, TimerControl, TIMER_ALARM_ID};
use crate::platform::{Button, Event, Key};
use islet_core::activity::{Activities, Hud, Live, MediaCommand, MediaInfo, NavInfo, Report};
use islet_core::alarm::{Alarm, AlarmClock, AlarmEvent};
use islet_core::clock::{secs_to_next_minute, LocalTime};
use islet_core::connectivity::{ConnCommand, ConnUpdate};
use islet_core::geometry::Rect;
use islet_core::island::{
    spring_for, target_size, Island, Presentation, Signal, AUTO_COLLAPSE_SECS, LONG_PRESS_SECS, MINIMAL_CIRCLE,
    MINIMAL_GAP,
};
use islet_core::prefs::IslandPrefs;
use islet_core::spring::{Animated, Spring};
use islet_core::timer::{Timer, TimerCommand, TimerEvent};
use islet_render::elements::Element;
use islet_render::text::TextEngine;
use islet_render::{Circle, PillFrame, Squircle};
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
/// At most one activity switch per this many seconds of scrolling.
const SWITCH_COOLDOWN: f64 = 0.35;
/// Height of the expanded clock and media cards.
const CLOCK_H: f64 = 168.0;
const MEDIA_H: f64 = 160.0;

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

/// Things the main loop opens for the island.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiAction {
    /// A link or settings URI (Windows Bluetooth settings).
    Url(String),
    /// A file or folder from the shelf, in its default app.
    Path(String),
    /// Islet Settings at a page ("wifi:Home": join Home with a password).
    Settings(String),
}

/// Windows has no public API to connect Bluetooth devices: its settings do.
const BLUETOOTH_SETTINGS: &str = "ms-settings:bluetooth";

/// What an open island shows.
enum OpenContent<'a> {
    Agenda,
    Notes,
    Alarm(&'a Alarm),
    Panel(Panel),
    Timer(&'a Timer),
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
    /// The file shelf; whether files are being dragged over the island (and
    /// whether that opened it); and a pending save.
    shelf: Vec<islet_core::shelf::ShelfItem>,
    dragging: bool,
    drag_opened: bool,
    shelf_dirty: bool,
    /// The notifications card was opened from a notification: it stays
    /// until the island closes, even after the peek ends.
    notes_open: bool,
    /// island.json: which pop-ups are on, weather, calendar…
    prefs: IslandPrefs,
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
            shelf: Vec::new(),
            dragging: false,
            drag_opened: false,
            shelf_dirty: false,
            notes_open: false,
            prefs: IslandPrefs::default(),
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
        if !self.nav_card_open(t) || self.animating(t) {
            return None;
        }
        let cr = self.content_rect(self.pill_rect(t));
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
        if !self.island.presentation().is_open() || self.animating(t) {
            return None;
        }
        if !matches!(self.open_content(&self.live(t)), OpenContent::Timer(_)) {
            return None;
        }
        let cr = self.content_rect(self.pill_rect(t));
        content::timer_controls(cr).into_iter().find(|(r, _)| r.contains(x, y)).map(|(_, c)| c)
    }

    /// The Snooze/Stop button under (x, y) on an open alarm card.
    fn alarm_control_at(&self, x: f64, y: f64, t: f64) -> Option<AlarmControl> {
        if !self.island.presentation().is_open() || self.animating(t) {
            return None;
        }
        if !matches!(self.open_content(&self.live(t)), OpenContent::Alarm(_)) {
            return None;
        }
        let cr = self.content_rect(self.pill_rect(t));
        content::alarm_controls(cr).into_iter().find(|(r, _)| r.contains(x, y)).map(|(_, c)| c)
    }

    /// Wi-Fi / Bluetooth requests for the connectivity worker.
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
        if !self.island.presentation().is_open() || self.animating(t) {
            return None;
        }
        let cr = self.content_rect(self.pill_rect(t));
        let c = self.activities.connectivity();
        match self.open_content(&self.live(t)) {
            OpenContent::Clock => content::clock_chips(cr, !self.shelf.is_empty())
                .into_iter()
                .find(|(r, _)| r.contains(x, y))
                .map(|(_, p)| ConnHit::Open(p)),
            OpenContent::Panel(p) => {
                let (rows, footer) = match p {
                    Panel::Wifi => (c.wifi.networks.len(), content::wifi_footer(c)),
                    Panel::Bluetooth => (c.bt.devices.len(), content::bt_footer(c)),
                    Panel::Shelf => (self.shelf.len(), None),
                };
                if content::list_header(cr).contains(x, y) {
                    return Some(ConnHit::Back);
                }
                let list = content::list_rows(cr, rows);
                if let Some(i) = list.iter().position(|r| r.contains(x, y)) {
                    if p == Panel::Shelf && content::shelf_remove_hit(list[i], x) {
                        return Some(ConnHit::Remove(i));
                    }
                    return Some(ConnHit::Row(i));
                }
                (footer.is_some_and(|(_, button)| button) && content::list_footer(cr, rows).contains(x, y))
                    .then_some(ConnHit::Footer)
            }
            _ => None,
        }
    }

    fn conn_act(&mut self, hit: ConnHit, t: f64) -> bool {
        let c = self.activities.connectivity().clone();
        match hit {
            ConnHit::Open(p) => {
                self.panel = Some(p);
                self.activities.report(Report::Connectivity(ConnUpdate::ClearFailure), t);
                match p {
                    Panel::Wifi => self.conn_commands.push(ConnCommand::ScanWifi),
                    Panel::Bluetooth => self.conn_commands.push(ConnCommand::ListBluetooth),
                    Panel::Shelf => {}
                }
            }
            ConnHit::Remove(i) => {
                if i < self.shelf.len() {
                    self.shelf.remove(i);
                    self.shelf_dirty = true;
                }
            }
            ConnHit::Back => self.panel = None,
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
                None => {}
            },
            ConnHit::Footer => match (self.panel, &c.failure) {
                (Some(Panel::Wifi), Some((_, Some(ssid)))) => {
                    self.ui_actions.push(UiAction::Settings(format!("wifi:{ssid}")))
                }
                (Some(Panel::Bluetooth), _) => self.ui_actions.push(UiAction::Url(BLUETOOTH_SETTINGS.into())),
                _ => {}
            },
        }
        self.sync_live(t);
        true
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

    pub fn set_prefs(&mut self, prefs: IslandPrefs) {
        self.prefs = prefs;
    }

    pub fn prefs(&self) -> &IslandPrefs {
        &self.prefs
    }

    /// Apply a provider report. Returns true if a redraw is needed.
    pub fn report(&mut self, r: Report, t: f64) -> bool {
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
        // A HUD is only a moment's notice: the card shows what is under it.
        let live = match live.first() {
            Some(Live::Hud(_)) => &live[1..],
            _ => live,
        };
        if let (Some(Live::Alarm(_)), Some(a)) = (live.first(), self.activities.alarm()) {
            OpenContent::Alarm(a)
        } else if let Some(p) = self.panel {
            OpenContent::Panel(p)
        } else if self.notes_open || matches!(live.first(), Some(Live::Note(_))) {
            OpenContent::Notes
        } else if let Some(Live::Event { .. }) = live.first() {
            OpenContent::Agenda
        } else if let (Some(Live::Timer(_)), Some(tm)) = (live.first(), self.activities.timer()) {
            OpenContent::Timer(tm)
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

    fn clock_shown(&self, t: f64) -> bool {
        (self.expanded_opacity(t) > 0.0 || self.island.presentation().is_open())
            && matches!(self.open_content(&self.live(t)), OpenContent::Clock)
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
        self.expanded_opacity(t) > 0.0 || self.compact_opacity(t, &self.live(t)) > 0.0
    }

    /// Everything drawn inside the island this frame.
    pub fn elements(&self, t: f64, engine: &mut TextEngine) -> Vec<Element> {
        let live = self.live(t);
        let p = self.pill_rect(t);
        let mut out = Vec::new();
        let eo = self.expanded_opacity(t);
        let cr = self.content_rect(p);
        if eo > 0.0 {
            match self.open_content(&live) {
                OpenContent::Alarm(a) => out.extend(content::expanded_alarm(cr, a, self.h24, t, eo, engine)),
                OpenContent::Timer(tm) => out.extend(content::expanded_timer(cr, tm, t, eo, engine)),
                OpenContent::Agenda => out.extend(content::expanded_agenda(cr, &self.agenda(), eo, engine)),
                OpenContent::Notes => out.extend(content::expanded_notes(cr, self.activities.notes(), eo, engine)),
                OpenContent::Panel(Panel::Wifi) => {
                    out.extend(content::expanded_wifi(cr, self.activities.connectivity(), eo, engine))
                }
                OpenContent::Panel(Panel::Bluetooth) => {
                    out.extend(content::expanded_bluetooth(cr, self.activities.connectivity(), eo, engine))
                }
                OpenContent::Panel(Panel::Shelf) => {
                    out.extend(content::expanded_shelf(cr, &self.shelf, self.dragging, eo, engine))
                }
                OpenContent::Media(m) => out.extend(content::expanded_media(cr, m, t, eo, self.scrub, engine)),
                OpenContent::Nav(n) => out.extend(content::expanded_nav(cr, n, self.clock.as_ref(), eo, engine)),
                OpenContent::Battery => out.extend(content::expanded_battery(
                    cr,
                    &self.activities.devices(),
                    self.activities.laptop(),
                    eo,
                    engine,
                )),
                OpenContent::Clock => {
                    if let Some(now) = self.clock {
                        out.extend(content::expanded_clock(cr, &now, self.h24, eo, engine));
                    }
                    out.extend(content::clock_chip_elements(
                        cr,
                        self.activities.connectivity(),
                        self.shelf.len(),
                        eo,
                        engine,
                    ));
                    if let Some(w) = self.activities.weather() {
                        out.extend(content::clock_weather(cr, w, self.prefs.fahrenheit, eo));
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
                (_, Some(first)) => out.extend(content::compact(p, first, t, co, engine)),
                _ => {}
            }
        }
        out
    }

    /// Size for a presentation, adapted to what it shows.
    fn target_dims(&self, p: Presentation, t: f64) -> (f64, f64, f64) {
        let (w, h, r) = target_size(p, None);
        let live = self.live(t);
        let dims = match p {
            Presentation::Compact => (live.first().map_or(w, content::compact_width), h, r),
            Presentation::Expanded => {
                let h = match self.open_content(&live) {
                    OpenContent::Alarm(_) => content::ALARM_H,
                    OpenContent::Timer(_) => content::TIMER_H,
                    OpenContent::Agenda => content::agenda_height(self.agenda().len()),
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
                            Panel::Shelf => content::list_height(
                                self.shelf.len(),
                                content::shelf_footer(self.shelf.len(), self.dragging).is_some(),
                            ),
                        }
                    }
                    OpenContent::Media(_) | OpenContent::Nav(_) => MEDIA_H,
                    OpenContent::Clock => CLOCK_H,
                    OpenContent::Battery => {
                        let rows = self.activities.devices().len() + usize::from(self.activities.laptop().is_some());
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
            self.notes_open = false;
        } else if !from.is_open() && matches!(self.open_content(&self.live(t)), OpenContent::Notes) {
            self.notes_open = true;
        } else if !from.is_open() && matches!(self.open_content(&self.live(t)), OpenContent::Clock) {
            // Show the current network and device on the clock card's buttons.
            self.conn_commands.push(ConnCommand::Status);
        }
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
        if inside {
            self.left_open_at = None;
            self.signal(Signal::PointerEnter, t)
        } else {
            if self.island.presentation().is_open() {
                self.left_open_at = Some(t);
            }
            self.press = None;
            self.signal(Signal::PointerLeave, t)
        }
    }

    /// The media control under (x, y) on an open "Now Playing" card.
    fn control_at(&self, x: f64, y: f64, t: f64) -> Option<Control> {
        if !self.island.presentation().is_open() || self.animating(t) {
            return None;
        }
        if !matches!(self.open_content(&self.live(t)), OpenContent::Media(_)) {
            return None;
        }
        let cr = self.content_rect(self.pill_rect(t));
        content::media_controls(cr).into_iter().find(|(r, _)| r.contains(x, y)).map(|(_, c)| c)
    }

    /// Returns true when the event changed what is on screen.
    pub fn handle(&mut self, e: Event, t: f64) -> bool {
        match e {
            Event::PointerMove { x, .. } if self.control == Some(Control::Scrub) => {
                self.scrub = Some(content::scrub_fraction(self.content_rect(self.pill_rect(t)), x));
                true
            }
            Event::PointerMove { x, y } => self.pointer_at(x, y, t),
            Event::PointerLeave => self.pointer_at(f64::NAN, f64::NAN, t),
            Event::PointerDown { x, y, button: Button::Primary } if self.hit(x, y, t) => {
                if let Some(c) = self.timer_control_at(x, y, t) {
                    self.timer_control = Some(c);
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
                        if c == Control::Scrub && self.activities.media().is_some_and(|m| m.caps.seek) {
                            self.scrub = Some(content::scrub_fraction(self.content_rect(self.pill_rect(t)), x));
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
                    Some(p) if !p.long_fired => self.signal(Signal::Click, t),
                    _ => false,
                }
            }
            // Until the context menu exists: right-click the island to quit.
            Event::PointerDown { x, y, button: Button::Secondary } if self.hit(x, y, t) => {
                self.quit = true;
                false
            }
            Event::Key(Key::Escape) => self.signal(Signal::Escape, t),
            Event::CapsLock(on) => self.report(Report::Hud(Hud::CapsLock(on)), t),
            Event::DragOver { x, y } => self.drag_over(x, y, t),
            Event::DragLeave => self.drag_over(f64::NAN, f64::NAN, t),
            Event::Scroll { dx, dy } if self.hovered => self.scroll(dx, dy, t),
            Event::Redraw | Event::ScaleChanged(_) => true,
            _ => false,
        }
    }

    /// Advance timers. Returns true if anything changed.
    pub fn tick(&mut self, t: f64) -> bool {
        let mut changed = self.timer_tick(t);
        changed |= self.sync_live(t);
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
        !(self.w.is_settled(t) && self.h.is_settled(t) && self.r.is_settled(t) && self.bubble.is_settled(t))
    }

    /// How often to draw while something moves on its own: 60 fps for the
    /// shape, 30 fps for a waveform; None when nothing moves (zero wakeups).
    pub fn frame_interval(&self, t: f64) -> Option<Duration> {
        if self.animating(t) {
            return Some(FRAME);
        }
        let live = self.live(t);
        // The bell swings while an alarm rings and is on screen.
        if self.activities.alarm().is_some() && self.has_content(t) {
            return Some(WAVE_FRAME);
        }
        let playing = self.activities.media().is_some_and(|m| m.playing);
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
        // Alarms are checked each minute, only while one is set.
        if let Some(secs) = self.clock.and_then(|now| self.alarms.next_check(&now)) {
            at(t + secs);
        }
        next.map(|n| (n - t).max(0.0))
    }

    fn pill_at(&self, w: f64, h: f64) -> Rect {
        Rect::new((WINDOW.0 - w) / 2.0, TOP_GAP, w, h)
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
    fn escape_collapses_and_right_click_quits() {
        let mut app = App::new(Demo::None, false);
        click(&mut app, 0.0);
        app.handle(Event::Key(Key::Escape), 1.0);
        assert_eq!(app.presentation(), Idle);
        let (x, y) = centre(&app, 6.0);
        app.handle(Event::PointerDown { x, y, button: Button::Secondary }, 6.0);
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
        assert!((app.pill_rect(t).h - MEDIA_H).abs() < 0.01);
        assert!(texts(&app, t, &mut engine).contains(&"Song".to_string()));

        let controls = content::media_controls(app.pill_rect(t));
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
        };
        let route =
            RouteInfo { duration: 900.0, free_flow: 900.0, distance_m: 5000.0, first_step: None, maneuver: None };
        app.report(Report::Media(Some(song(true))), 0.0);
        app.report(Report::Navigation(Some(NavInfo { trip, route: Some(route), error: None })), 0.0);
        assert_eq!(app.presentation(), Minimal, "navigation and music together");
        click(&mut app, 1.0);
        assert!(app.nav_card_open(5.0), "the open card is navigation, not music");
        let (waze, _) = content::nav_controls(app.content_rect(app.pill_rect(5.0)))[1];
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
        press(&mut app, content::clock_chips(cr, false)[0].0, 5.0);
        assert_eq!(app.take_conn_commands(), [ConnCommand::ScanWifi]);
        let rows = content::list_rows(app.content_rect(app.pill_rect(9.0)), 2);
        assert!(texts(&app, 9.0, &mut engine).iter().any(|s| s == "Cafe"));

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
        press(&mut app, content::clock_chips(cr, false)[1].0, 18.0);
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
    fn idle_island_shows_no_clock_and_never_wakes_for_it() {
        let mut app = App::new(Demo::None, false);
        assert!(!app.set_time(NOW, 0.0), "nothing to redraw while idle");
        let mut engine = TextEngine::new();
        assert!(app.elements(0.0, &mut engine).is_empty());
        assert_eq!(app.next_timer(0.0), None, "zero wakeups when idle");
        assert_eq!(app.frame_interval(0.0), None);
    }

    #[test]
    fn open_island_shows_the_clock_and_ticks_each_minute() {
        let mut app = App::new(Demo::None, false);
        app.set_time(NOW, 0.0);
        click(&mut app, 0.0);
        let mut engine = TextEngine::new();
        assert_eq!(texts(&app, 5.0, &mut engine), ["14:05", "Wednesday, 4 November", "Wi-Fi", "Bluetooth"]);
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
        assert!((card.h - (MEDIA_H + 32.0)).abs() < 0.01, "the card grows by the notch height");
        let (r, _) = content::media_controls(app.content_rect(card))[0];
        assert!(r.y >= card.y + 32.0, "controls are below the notch");
    }

    #[test]
    fn every_state_fits_the_window_with_overshoot() {
        for p in [Idle, Peek, Compact, Minimal, Expanded, Large] {
            let (w, h, _) = target_size(p, None);
            let w = if p == Compact { content::COMPACT_BATTERY_W } else { w };
            let extra = if p == Minimal { MINIMAL_GAP + MINIMAL_CIRCLE } else { 0.0 };
            assert!((w + extra) * 1.03 <= WINDOW.0 && h * 1.03 + TOP_GAP <= WINDOW.1, "{p:?}");
        }
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
            ],
            can_connect: true,
            note: None,
        };
        let make = |panel: Option<usize>, fail: bool| {
            let mut a = App::new(Demo::None, false);
            a.set_time(NOW, 0.0);
            a.report(Report::Connectivity(ConnUpdate::Wifi(w.clone())), 0.0);
            a.report(Report::Connectivity(ConnUpdate::Bluetooth(bt.clone())), 0.0);
            click(&mut a, 0.0);
            if let Some(i) = panel {
                let cr = a.content_rect(a.pill_rect(5.0));
                press(&mut a, content::clock_chips(cr, false)[i].0, 5.0);
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
