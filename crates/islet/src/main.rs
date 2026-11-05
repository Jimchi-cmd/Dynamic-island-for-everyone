//! Islet island process: a native, webview-free overlay.
//!
//! Environment:
//! - `ISLET_PERF_DEMO=1`: expand/collapse every 1.2 s (perf harness)
//! - `ISLET_DEMO=tour`: walk through every presentation (visual review)
//! - `ISLET_DEMO=media`: a scripted music + battery session (no OS services)
//! - `ISLET_REDUCE_MOTION=1`: snap instead of animating
//!
//! Right-click the island to quit (until the context menu exists).
//! `islet --help` lists the commands (navigation, API key).

// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod autostart;
mod cli;
mod clock;
mod config;
mod content;
mod control;
mod net;
mod platform;
mod providers;
mod secrets;
mod sound;

use app::{AlarmOut, App, Demo, UiAction, FRAME, WINDOW};
use content::NavControl;
use control::Control;
use islet_core::activity::Report;
use islet_core::nav;
use islet_render::text::TextEngine;
use platform::{Backend, Event, Native, WindowSpec};
use providers::navigation::{self, NavMsg};
use std::io::Write;
use std::time::{Duration, Instant};
use tiny_skia::Pixmap;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(result) = cli::run(&args) {
        attach_console();
        match result {
            Ok(msg) => println!("{msg}"),
            Err(e) => {
                eprintln!("islet: {e}");
                std::process::exit(2);
            }
        }
        return;
    }
    if let Err(e) = run() {
        eprintln!("islet: {e}");
        std::process::exit(1);
    }
}

fn run() -> platform::Result<()> {
    let started = Instant::now();
    // One island per user: a second launch (login item + Settings) just exits.
    if control::send(&Control::Ping).is_ok() {
        println!("Islet is already running.");
        return Ok(());
    }
    // Diagnostics: does the macOS MediaRemote helper work on this Mac?
    #[cfg(target_os = "macos")]
    if std::env::args().any(|a| a == "--mediaremote-probe") {
        std::process::exit(if providers::macos_mediaremote::probe() { 0 } else { 1 });
    }
    let env_on = |k: &str| std::env::var_os(k).is_some_and(|v| v != "0");
    // ISLET_TRACE_STARTUP=1: time each startup stage (stderr).
    let trace_on = env_on("ISLET_TRACE_STARTUP");
    let trace = |stage: &str| {
        if trace_on {
            eprintln!("ISLET_TRACE {stage} {:.1} ms", started.elapsed().as_secs_f64() * 1000.0);
        }
    };
    trace("main");
    let demo = match std::env::var("ISLET_DEMO").as_deref() {
        Ok("tour") => Demo::Tour,
        Ok("toggle") => Demo::Toggle,
        _ if env_on("ISLET_PERF_DEMO") => Demo::Toggle,
        _ => Demo::None,
    };
    let mut app = App::new(demo, env_on("ISLET_REDUCE_MOTION"));
    app.set_clock_style(clock::prefers_24h());
    // Fonts load on first use, so an island that is never opened never pays for them.
    let mut text: Option<TextEngine> = None;

    let mut win = Native::create(WindowSpec { width: WINDOW.0, height: WINDOW.1 })?;
    trace("window created");
    let new_pixmap = |win: &Native| {
        let (w, h) = win.size_px();
        Pixmap::new(w.max(1), h.max(1)).ok_or_else(|| "could not allocate frame buffer".to_string())
    };
    let mut pixmap = new_pixmap(&win)?;
    // On a notched MacBook the island wraps the notch (ISLET_NOTCH=WxH fakes one for testing).
    let notch = std::env::var("ISLET_NOTCH")
        .ok()
        .and_then(|v| v.split_once('x').and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?))))
        .or(win.notch());
    app.set_notch(notch, 0.0);
    trace("notch checked");
    let epoch = Instant::now();
    // Wall clock minus app clock: it jumps ahead when the computer sleeps
    // (the monotonic clock pauses), and timers should keep wall-clock time.
    let skew_now = || {
        let wall = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64());
        wall - epoch.elapsed().as_secs_f64()
    };
    let mut skew = skew_now();
    let now = || epoch.elapsed().as_secs_f64();

    let providers = providers::Providers::start(win.waker());
    let (control_tx, control_rx) = std::sync::mpsc::channel::<Control>();
    control::serve(control_tx, win.waker());
    let mut nav_card_open = false;
    // Demos and perf runs leave the user's alarms alone.
    let scripted = demo != Demo::None || std::env::var_os("ISLET_DEMO").is_some();
    if !scripted {
        app.set_alarms(config::load_alarms());
        app.set_prefs(config::load_prefs());
    }
    win.watch_caps_lock(app.prefs().caps_lock_hud);
    providers.weather(app.prefs().weather.clone());
    let mut alarm_sound = sound::AlarmSound::default();
    trace("providers started");

    let mut needs_frame = true;
    let mut first_frame = true;
    let mut last_frame = epoch;
    let mut region = app.input_region(0.0);
    win.set_input_region(region)?;
    let mut events = Vec::new();

    loop {
        let t = now();
        // Sleep until the next frame (only while something moves), the next
        // timer, or an event. With nothing moving there is no deadline at all.
        let interval = if needs_frame { Some(FRAME) } else { app.frame_interval(t) };
        let mut deadline = interval.map(|iv| (last_frame + iv).max(Instant::now()));
        if let Some(secs) = app.next_timer(t) {
            let d = Instant::now() + Duration::from_secs_f64(secs);
            deadline = Some(deadline.map_or(d, |x| x.min(d)));
        }

        events.clear();
        win.wait(deadline, &mut events)?;

        let t = now();
        let s = skew_now();
        if s - skew > 2.0 {
            app.clock_jumped(s - skew, t);
            needs_frame = true;
        }
        skew = s;
        for e in events.drain(..) {
            match e {
                Event::Quit => return Ok(()),
                Event::ScaleChanged(_) => {
                    pixmap = new_pixmap(&win)?;
                    needs_frame = true;
                }
                _ => {}
            }
            needs_frame |= app.handle(e, t);
        }
        // Reports from the media and battery providers.
        while let Ok((mut report, sampled)) = providers.reports.try_recv() {
            if let Report::Media(Some(m)) = &mut report {
                m.position_at = sampled.saturating_duration_since(epoch).as_secs_f64();
            }
            needs_frame |= app.report(report, t);
        }
        for cmd in app.take_commands() {
            providers.command(cmd);
        }
        // `islet nav start/stop` from the command line.
        while let Ok(c) = control_rx.try_recv() {
            match c {
                Control::NavStart(trip) => providers.nav(NavMsg::Start(trip)),
                Control::NavStop => providers.nav(NavMsg::Stop),
                Control::Quit => return Ok(()),
                Control::Ping => {}
                Control::AlarmsChanged if !scripted => app.set_alarms(config::load_alarms()),
                Control::AlarmsChanged => {}
                Control::AlarmStop => needs_frame |= app.stop_alarm(t),
                Control::AlarmSnooze => needs_frame |= app.snooze_alarm(t),
                Control::Timer(cmd) => needs_frame |= app.timer_command(cmd, t),
                Control::PrefsChanged if !scripted => {
                    app.set_prefs(config::load_prefs());
                    win.watch_caps_lock(app.prefs().caps_lock_hud);
                    providers.weather(app.prefs().weather.clone());
                    providers.weather(app.prefs().weather.clone());
                }
                Control::PrefsChanged => {}
            }
        }
        for action in app.take_nav_actions() {
            let trip = app.nav_trip().cloned();
            match (action, trip) {
                (NavControl::GoogleMaps, Some(trip)) => navigation::open_url(&nav::google_maps_url(&trip)),
                (NavControl::Waze, Some(trip)) => navigation::open_url(&nav::waze_url(&trip)),
                (NavControl::Stop, _) => providers.nav(NavMsg::Stop),
                _ => {}
            }
        }
        for cmd in app.take_conn_commands() {
            providers.conn(cmd);
        }
        for action in app.take_ui_actions() {
            match action {
                UiAction::OpenUrl(url) => navigation::open_url(&url),
                UiAction::OpenSettings(page) => open_settings(&page),
            }
        }
        let open = app.nav_card_open(t);
        if open != nav_card_open {
            nav_card_open = open;
            providers.nav(NavMsg::CardOpen(open));
        }
        if app.wants_quit() {
            return Ok(());
        }
        needs_frame |= app.set_time(clock::now(), t);
        for out in app.take_alarm_out() {
            match out {
                AlarmOut::Sound(on) => alarm_sound.set(on),
                AlarmOut::Chime => sound::chime_once(),
                AlarmOut::Disable(id) if !scripted => {
                    let mut alarms = config::load_alarms();
                    if let Some(a) = alarms.iter_mut().find(|a| a.id == id) {
                        a.enabled = false;
                        let _ = config::save_alarms(&alarms);
                    }
                }
                AlarmOut::Disable(_) => {}
            }
        }
        needs_frame |= app.tick(t);

        let new_region = app.input_region(t);
        if new_region != region {
            region = new_region;
            win.set_input_region(region)?;
        }

        let interval = if needs_frame { Some(FRAME) } else { app.frame_interval(t) };
        if let Some(iv) = interval.filter(|iv| Instant::now() >= last_frame + *iv) {
            let _ = iv;
            let scale = win.scale() as f32;
            let frame = app.frame(t);
            islet_render::draw(&mut pixmap, &frame, scale);
            if app.has_content(t) {
                let engine = text.get_or_insert_with(TextEngine::new);
                let elements = app.elements(t, engine);
                let clip = islet_render::clip_mask(&frame, pixmap.width(), pixmap.height(), scale);
                islet_render::elements::draw_elements(&mut pixmap, &elements, engine, scale, clip.as_ref());
            }
            if first_frame {
                trace("first frame drawn");
            }
            win.present(&pixmap)?;
            last_frame = Instant::now();
            needs_frame = false;
            if first_frame {
                first_frame = false;
                trace("first frame presented");
                println!("ISLET_VISIBLE {}", started.elapsed().as_millis());
                let _ = std::io::stdout().flush();
            }
        }
    }
}

/// Windows release builds have no console; let CLI output reach the
/// terminal that started us.
fn attach_console() {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
        let _ = AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

/// Open Islet Settings (installed next to the island) at a page.
fn open_settings(page: &str) {
    let Some(exe) = std::env::current_exe().ok().and_then(|e| {
        let p = e.parent()?.join(format!("islet-settings{}", std::env::consts::EXE_SUFFIX));
        p.exists().then_some(p)
    }) else {
        providers::debug_log("Islet Settings is not installed next to the island");
        return;
    };
    let _ = std::process::Command::new(exe)
        .args(["--page", page])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}
