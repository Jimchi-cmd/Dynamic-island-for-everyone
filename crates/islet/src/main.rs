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

mod a11y;
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
    // Relaunched by an island that is resizing itself: let it close first.
    if std::env::var_os("ISLET_RELAUNCH").is_some() {
        let until = Instant::now() + std::time::Duration::from_secs(3);
        while control::send(&Control::Ping).is_ok() && Instant::now() < until {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }
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

    // Size and position (Settings → Look) apply at start: the window is made
    // for them. `k` scales everything; the offset moves the island down.
    let layout_prefs = if demo == Demo::None && std::env::var_os("ISLET_DEMO").is_none() {
        config::load_prefs()
    } else {
        Default::default()
    };
    let k = layout_prefs.size_factor();
    let top = layout_prefs.top_offset();
    let mut win = Native::create(WindowSpec { width: WINDOW.0 * k, height: (WINDOW.1 + top) * k })?;
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
    // A notched Mac's island hangs from the notch: no offset there.
    app.set_top(if notch.is_some() { 0.0 } else { top });
    app.set_notch(notch.map(|(w, h)| (w / k, h / k)), 0.0);
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

    let mut providers = providers::Providers::start(win.waker());
    trace("providers started");
    let (control_tx, control_rx) = std::sync::mpsc::channel::<Control>();
    control::serve(control_tx, win.waker());
    trace("control channel open");
    let mut nav_card_open = false;
    let mut sysmon_on = false;
    // Demos and perf runs leave the user's alarms alone.
    let scripted = demo != Demo::None || std::env::var_os("ISLET_DEMO").is_some();
    if !scripted {
        app.set_alarms(config::load_alarms());
        app.set_prefs(config::load_prefs());
        app.set_shelf(config::load_shelf());
    }
    win.watch_caps_lock(app.prefs().caps_lock_hud);
    trace("caps lock watched");
    apply_hotkey(&mut win, &app.prefs().hotkey);
    providers.weather(app.prefs().weather.clone());
    providers.calendar(app.prefs().calendar);
    providers.notifications(app.prefs().notifications);
    providers::WEB_ARTWORK.store(app.prefs().web_artwork, std::sync::atomic::Ordering::Relaxed);
    providers.spotify(app.prefs().spotify && !scripted);
    providers.clipboard(app.prefs().clipboard && !scripted);
    let mut alarm_sound = sound::AlarmSound::default();
    // Windows' screen-reader adapter must exist before the window shows.
    let mut access = a11y::Bridge::new(win.native_handle(), win.waker());
    trace("screen-reader bridge ready");
    win.show();
    // Describe the island to screen readers once things settle (not on
    // every animation frame), and read the UTC offset once a minute.
    let mut a11y_pending = true;
    let mut utc = (u8::MAX, 0i64);

    let mut needs_frame = true;
    let mut first_frame = true;
    let mut last_frame = epoch;
    let mut region = app.input_region(0.0);
    win.set_input_region(scale_rect(region, k))?;
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
        if first_frame {
            trace("first wait returned");
        }

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
            if let Event::Drop(paths) = e {
                // The island keeps references only: note which are folders.
                let dropped = paths.into_iter().map(|p| {
                    let dir = std::path::Path::new(&p).is_dir();
                    (p, dir)
                });
                needs_frame |= app.drop_files(dropped.collect(), t);
                continue;
            }
            needs_frame |= app.handle(unscale(e, k), t);
        }
        // Reports from the media and battery providers.
        while let Ok((mut report, sampled)) = providers.reports.try_recv() {
            if let Report::Media(Some(m)) = &mut report {
                m.position_at = sampled.saturating_duration_since(epoch).as_secs_f64();
            }
            needs_frame |= app.report(report, t);
        }
        let source = app.media_source();
        for cmd in app.take_commands() {
            providers.command_for(&source, cmd);
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
                    // A new size or gap needs a new window: start afresh.
                    let p = app.prefs();
                    if (p.size_factor() - k).abs() > 1e-9 || (p.top_offset() - top).abs() > 1e-9 {
                        if let Ok(exe) = std::env::current_exe() {
                            if std::process::Command::new(exe).env("ISLET_RELAUNCH", "1").spawn().is_ok() {
                                return Ok(());
                            }
                        }
                    }
                    win.watch_caps_lock(app.prefs().caps_lock_hud);
                    apply_hotkey(&mut win, &app.prefs().hotkey);
                    providers.weather(app.prefs().weather.clone());
                    providers.calendar(app.prefs().calendar);
                    providers.notifications(app.prefs().notifications);
                    providers::WEB_ARTWORK.store(app.prefs().web_artwork, std::sync::atomic::Ordering::Relaxed);
                    providers.spotify(app.prefs().spotify);
                    providers.clipboard(app.prefs().clipboard);
                }
                Control::PrefsChanged => {}
                Control::ShelfChanged if !scripted => app.set_shelf(config::load_shelf()),
                Control::ShelfChanged => {}
                Control::Activity(c) => needs_frame |= app.report(Report::Custom(c), t),
                Control::Toggle => needs_frame |= app.toggle(t),
                Control::ActivityEnd(id) => needs_frame |= app.report(Report::CustomEnded(id), t),
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
        if let Some(items) = app.take_shelf_changes() {
            if !scripted {
                config::save_shelf(&items);
            }
        }
        if app.sysmon_wanted() != sysmon_on {
            sysmon_on = !sysmon_on;
            providers.sysmon(sysmon_on);
        }
        for text in app.take_clipboard_copies() {
            providers.clipboard_copy(text);
        }
        for cmd in app.take_conn_commands() {
            providers.conn(cmd);
        }
        for action in app.take_ui_actions() {
            match action {
                UiAction::Url(url) => navigation::open_url(&url),
                UiAction::Settings(page) => open_settings(&page),
                UiAction::Path(path) => open_path(&path),
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
        let minute = clock::now().minute;
        if minute != utc.0 {
            utc = (minute, clock::utc_offset());
        }
        needs_frame |= app.set_wall(clock::epoch(), utc.1, t);
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
        for press in access.take_presses() {
            needs_frame |= app.a11y_press(press, t);
        }
        a11y_pending |= needs_frame;
        if a11y_pending && access.active() && !app.animating(t) {
            a11y_pending = false;
            let described = app.accessibility(t);
            win.set_a11y_label(&described.label);
            access.update(&described, win.screen_origin(), win.scale() * k);
        }

        let new_region = app.input_region(t);
        if new_region != region {
            region = new_region;
            win.set_input_region(scale_rect(region, k))?;
        }

        let interval = if needs_frame { Some(FRAME) } else { app.frame_interval(t) };
        if let Some(iv) = interval.filter(|iv| Instant::now() >= last_frame + *iv) {
            let _ = iv;
            let scale = (win.scale() * k) as f32;
            let frame = app.frame(t);
            islet_render::draw(&mut pixmap, &frame, scale);
            if app.has_content(t) {
                let engine = text.get_or_insert_with(TextEngine::new);
                let elements = app.elements(t, engine);
                // Clip only what reaches past the outline (mid-morph content).
                let clip = islet_render::elements::needs_clip(&frame, &elements, engine)
                    .then(|| islet_render::clip_mask(&frame, pixmap.width(), pixmap.height(), scale))
                    .flatten();
                islet_render::elements::draw_elements_in(
                    &mut pixmap,
                    &elements,
                    engine,
                    scale,
                    clip.as_ref(),
                    Some(&frame),
                );
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

/// Open a shelf item with the OS's default app (never through a shell).
fn open_path(path: &str) {
    #[cfg(target_os = "macos")]
    let r = std::process::Command::new("open").arg(path).spawn();
    #[cfg(target_os = "windows")]
    let r = std::process::Command::new("explorer").arg(path).spawn();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let r = std::process::Command::new("xdg-open").arg(path).spawn();
    if let Err(e) = r {
        providers::debug_log(&format!("could not open a shelf item: {e}"));
    }
}

/// Register the global shortcut from the preferences ("" removes it).
fn apply_hotkey(win: &mut Native, spec: &str) {
    let hotkey = match spec.trim() {
        "" => None,
        s => match islet_core::hotkey::Hotkey::parse(s) {
            Ok(h) => Some(h),
            Err(e) => {
                providers::debug_log(&format!("shortcut: {e}"));
                return;
            }
        },
    };
    if let Err(e) = win.set_hotkey(hotkey) {
        eprintln!("islet: shortcut: {e}");
        providers::debug_log(&format!("shortcut: {e}"));
    }
}

/// Island px → window px at size factor `k`.
fn scale_rect(r: islet_core::geometry::Rect, k: f64) -> islet_core::geometry::Rect {
    islet_core::geometry::Rect::new(r.x * k, r.y * k, r.w * k, r.h * k)
}

/// Window px → island px for pointer and drag positions.
fn unscale(e: Event, k: f64) -> Event {
    match e {
        Event::PointerMove { x, y } => Event::PointerMove { x: x / k, y: y / k },
        Event::PointerDown { x, y, button } => Event::PointerDown { x: x / k, y: y / k, button },
        Event::PointerUp { x, y, button } => Event::PointerUp { x: x / k, y: y / k, button },
        Event::DragOver { x, y } => Event::DragOver { x: x / k, y: y / k },
        e => e,
    }
}
