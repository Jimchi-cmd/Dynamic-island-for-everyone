//! Islet island process: a native, webview-free overlay.
//!
//! Environment:
//! - `ISLET_PERF_DEMO=1`: expand/collapse every 1.2 s (perf harness)
//! - `ISLET_DEMO=tour`: walk through every presentation (visual review)
//! - `ISLET_DEMO=media`: a scripted music + battery session (no OS services)
//! - `ISLET_REDUCE_MOTION=1`: snap instead of animating
//!
//! Right-click the island for its menu (Settings, hide, pause pop-ups, quit).
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
#[cfg(target_os = "windows")]
mod msix;
mod net;
mod platform;
mod pro;
mod providers;
mod quick;
mod secrets;
mod sound;
mod switcher;

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
    // `--mirror <display>`: a copy of the island on another display, started
    // by the main island when Settings → Look → Display is "All displays".
    // It shows what's playing, calls, batteries, downloads and Control
    // Centre; alarms, timers, the shortcut and the network features stay
    // with the main island. It follows the main island's settings and quits
    // with it (its stdin closes).
    let args: Vec<String> = std::env::args().collect();
    let mirror = args.iter().position(|a| a == "--mirror").and_then(|i| args.get(i + 1)).cloned();
    // Relaunched by an island that is resizing itself: let it close first.
    if mirror.is_none() && std::env::var_os("ISLET_RELAUNCH").is_some() {
        let until = Instant::now() + std::time::Duration::from_secs(3);
        while control::send(&Control::Ping).is_ok() && Instant::now() < until {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }
    // One island per user: a second launch (login item + Settings) just exits.
    if mirror.is_none() && control::send(&Control::Ping).is_ok() {
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
    // The font the text engine was last given (Settings → Look → Font).
    let mut text_font: Option<String> = None;

    // Size and position (Settings → Look) apply at start: the window is made
    // for them. `k` scales everything; the offset moves the island down.
    let layout_prefs = if demo == Demo::None && std::env::var_os("ISLET_DEMO").is_none() {
        config::load_prefs()
    } else {
        Default::default()
    };
    let display = layout_prefs.display.clone();
    let all_displays = display == islet_core::prefs::ALL_DISPLAYS;
    // The display this island is on, for its own size there.
    let here = match &mirror {
        Some(name) => name.clone(),
        None if display.is_empty() || all_displays => platform::displays().into_iter().next().unwrap_or_default(),
        None => display.clone(),
    };
    let k = layout_prefs.size_factor_on(&here);
    let top = layout_prefs.top_offset();
    let placement = layout_prefs.placement();
    platform::set_placement(placement);
    platform::set_preferred_display(match &mirror {
        Some(name) => Some(name.clone()),
        None if all_displays => None, // the main island takes the primary display
        None => Some(display.clone()),
    });
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
    // The notch only matters for an island at the top centre.
    let notch = notch.filter(|_| placement == islet_core::geometry::Placement::Top);
    // A notched Mac's island hangs from the notch: no offset there.
    app.set_top(if notch.is_some() { 0.0 } else { top });
    app.set_placement(placement, 0.0);
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
    if mirror.is_none() {
        // Islet Pro: is the key still valid? Asked at most once a week, and
        // only once a key has been entered (see pro.rs).
        if std::env::var_os("ISLET_DEMO").is_none() && demo == Demo::None {
            let (tx, waker) = (control_tx.clone(), win.waker());
            std::thread::spawn(move || loop {
                let before = pro::active();
                if pro::check(false).is_ok() && pro::active() != before && tx.send(Control::PrefsChanged).is_ok() {
                    waker.wake();
                }
                std::thread::sleep(std::time::Duration::from_secs(6 * 3600));
            });
        }
        control::serve(control_tx, win.waker());
    } else {
        // A mirror hears from the main island on stdin: "prefs" when the
        // settings change; end of input when it quits.
        let waker = win.waker();
        std::thread::spawn(move || {
            use std::io::BufRead;
            for line in std::io::stdin().lock().lines().map_while(Result::ok) {
                if line.trim() == "prefs" && control_tx.send(Control::PrefsChanged).is_ok() {
                    waker.wake();
                }
            }
            let _ = control_tx.send(Control::Quit);
            waker.wake();
        });
    }
    trace("control channel open");
    let mut nav_card_open = false;
    let mut sysmon_on = false;
    let mut call_on = false;
    // The song whose lyrics were last asked for (key, length known).
    let mut lyrics_asked: Option<(String, bool)> = None;
    // Demos and perf runs leave the user's alarms alone.
    let scripted = demo != Demo::None || std::env::var_os("ISLET_DEMO").is_some();
    let load_prefs = || {
        let p = config::load_prefs();
        if mirror.is_some() {
            mirror_prefs(p)
        } else {
            p
        }
    };
    if !scripted {
        if mirror.is_none() {
            app.set_alarms(config::load_alarms());
        }
        app.set_prefs(load_prefs());
        app.set_pro(pro::active());
        app.set_shelf(config::load_shelf());
        app.set_todo(config::load_json("todo.json").unwrap_or_default());
        app.set_streak(config::load_json("focus.json").unwrap_or_default());
        app.set_timetable(config::load_json("timetable.json").unwrap_or_default(), 0.0);
    }
    win.watch_caps_lock(app.prefs().caps_lock_hud);
    trace("caps lock watched");
    apply_hotkeys(&mut win, app.prefs());
    providers.weather(app.prefs().weather.clone());
    providers.calendar(app.prefs().calendar);
    providers.notifications(app.prefs().notifications);
    providers::WEB_ARTWORK.store(app.prefs().web_artwork, std::sync::atomic::Ordering::Relaxed);
    providers.spotify(app.prefs().spotify && !scripted);
    providers.clipboard(app.prefs().clipboard && !scripted);
    providers.downloads(app.prefs().downloads && !scripted);
    providers.plugins(app.prefs().plugins && !scripted, false);
    providers.hog(app.prefs().performance_alert && !scripted);
    providers.screenshots(pro::active() && app.prefs().screenshot_preview && !scripted);
    providers.phone(phone_pairing(app.prefs()).filter(|_| !scripted && mirror.is_none()));
    let mut alarm_sound = sound::AlarmSound::default();
    // Windows' screen-reader adapter must exist before the window shows.
    let mut access = a11y::Bridge::new(win.native_handle(), win.waker());
    trace("screen-reader bridge ready");
    win.show();
    // Settings → Look → Display → All displays: an island on each other display.
    let mut mirrors: Vec<std::process::Child> = Vec::new();
    if all_displays && mirror.is_none() && !scripted {
        if let Ok(exe) = std::env::current_exe() {
            for name in platform::displays().into_iter().skip(1) {
                let child = std::process::Command::new(&exe)
                    .args(["--mirror", &name])
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::null())
                    .spawn();
                match child {
                    Ok(c) => mirrors.push(c),
                    Err(e) => providers::debug_log(&format!("island on {name}: {e}")),
                }
            }
        }
    }
    // Describe the island to screen readers once things settle (not on
    // every animation frame), and read the UTC offset once a minute.
    let mut a11y_pending = true;
    let mut utc = (u8::MAX, 0i64);

    // ISLET_FRAME_STATS=1: average drawing and presenting time per frame (stderr).
    let mut frame_stats = env_on("ISLET_FRAME_STATS").then(FrameStats::default);
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
            // Positions are "as of when they were read", for music here and
            // on another device alike.
            if let Report::Media(Some(m)) | Report::RemoteMedia(Some(m)) = &mut report {
                m.position_at = sampled.saturating_duration_since(epoch).as_secs_f64();
            }
            needs_frame |= app.report(report, t);
        }
        // Lyrics (when on): ask once per song, again if its length turns up.
        let wanted = app.lyrics_wanted();
        let asked = wanted.as_ref().map(|t| (t.key(), t.duration.is_some()));
        if asked != lyrics_asked {
            lyrics_asked = asked;
            providers.lyrics(wanted);
        }
        if let Some(level) = app.take_volume() {
            providers.set_volume(level);
        }
        if let Some(level) = app.take_brightness() {
            if !scripted {
                quick::set_brightness(level);
            }
        }
        // A call started: read the microphone's mute switch for its card.
        if app.call_active() != call_on {
            call_on = !call_on;
            if call_on {
                providers.mic(islet_core::activity::MicCommand::Query);
            }
        }
        for cmd in app.take_mic_commands() {
            providers.mic(cmd);
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
                Control::Stopwatch(cmd) => needs_frame |= app.stopwatch_command(cmd, t),
                Control::PrefsChanged if !scripted => {
                    app.set_prefs(load_prefs());
                    app.set_pro(pro::active());
                    // `islet todo …` and Settings edit todo.json too.
                    app.set_todo(config::load_json("todo.json").unwrap_or_default());
                    // …and `islet class …` edits timetable.json.
                    needs_frame |= app.set_timetable(config::load_json("timetable.json").unwrap_or_default(), t);
                    // The islands on other displays follow.
                    for m in &mut mirrors {
                        if let Some(stdin) = m.stdin.as_mut() {
                            use std::io::Write;
                            let _ = writeln!(stdin, "prefs");
                        }
                    }
                    // A new size or gap needs a new window: start afresh (a
                    // mirror is restarted by the main island instead).
                    let p = app.prefs();
                    let moved = (p.size_factor_on(&here) - k).abs() > 1e-9
                        || (p.top_offset() - top).abs() > 1e-9
                        || p.placement() != placement
                        // Another display's size: its mirror starts afresh with the main island.
                        || p.display_sizes != layout_prefs.display_sizes
                        || (mirror.is_none() && p.display != display);
                    if moved && mirror.is_none() {
                        if let Ok(exe) = std::env::current_exe() {
                            if std::process::Command::new(exe).env("ISLET_RELAUNCH", "1").spawn().is_ok() {
                                return Ok(());
                            }
                        }
                    }
                    win.watch_caps_lock(app.prefs().caps_lock_hud);
                    apply_hotkeys(&mut win, app.prefs());
                    providers.weather(app.prefs().weather.clone());
                    providers.calendar(app.prefs().calendar);
                    providers.notifications(app.prefs().notifications);
                    providers::WEB_ARTWORK.store(app.prefs().web_artwork, std::sync::atomic::Ordering::Relaxed);
                    providers.spotify(app.prefs().spotify);
                    providers.clipboard(app.prefs().clipboard);
                    providers.downloads(app.prefs().downloads);
                    providers.plugins(app.prefs().plugins, false);
                    providers.hog(app.prefs().performance_alert);
                    providers.screenshots(pro::active() && app.prefs().screenshot_preview);
                    providers.phone(phone_pairing(app.prefs()).filter(|_| mirror.is_none()));
                }
                Control::PrefsChanged => {}
                Control::ShelfChanged if !scripted => app.set_shelf(config::load_shelf()),
                Control::ShelfChanged => {}
                Control::Activity(c) => needs_frame |= app.report(Report::Custom(c), t),
                Control::Toggle => needs_frame |= app.toggle(t),
                Control::PluginsReload if !scripted => providers.plugins(app.prefs().plugins, true),
                Control::PluginsReload => {}
                Control::CapsLock(on) => needs_frame |= app.handle(Event::CapsLock(on), t),
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
        if let Some(todo) = app.take_todo_changes() {
            if !scripted {
                let _ = config::save_json("todo.json", &todo);
            }
        }
        // Only the main island keeps the focus streak (a copy on another
        // display would count each round again).
        if let Some(streak) = app.take_streak_changes() {
            if !scripted && mirror.is_none() {
                let _ = config::save_json("focus.json", &streak);
            }
        }
        if app.sysmon_wanted() != sysmon_on {
            sysmon_on = !sysmon_on;
            providers.sysmon(sysmon_on);
        }
        for clip in app.take_clipboard_copies() {
            providers.clipboard_copy(clip);
        }
        for cmd in app.take_conn_commands() {
            providers.conn(cmd);
        }
        for action in app.take_ui_actions() {
            match action {
                UiAction::Url(url) => navigation::open_url(&url),
                UiAction::Settings(page) => open_settings(&page),
                UiAction::Path(path) => open_path(&path),
                UiAction::DragOut(paths) if !scripted => {
                    // Where the system can't drag files, the press is a tap again.
                    if !win.start_drag(&paths) {
                        trace("dragging files out isn't available here");
                    }
                }
                UiAction::DragOut(_) => {}
                // From the island's menu: save it like `islet set` would.
                UiAction::SetSwitch(key, on) if !scripted => {
                    let mut prefs = config::load_prefs();
                    if prefs.set_switch(key, on).is_ok() && config::save_prefs(&prefs).is_ok() {
                        app.set_prefs(prefs);
                    }
                }
                UiAction::SetSwitch(..) => {}
                UiAction::Screenshot if !scripted => quick::screenshot(),
                UiAction::Lock if !scripted => quick::lock(),
                UiAction::Shortcut(target) if !scripted => quick::open_shortcut(&target),
                UiAction::ReadBrightness if !scripted => quick::read_brightness(providers.sink()),
                UiAction::Launcher if !scripted => open_settings("launcher"),
                UiAction::QuickNote if !scripted => open_settings("note"),
                UiAction::Launcher | UiAction::QuickNote => {}
                UiAction::CloseProgram(pid) if !scripted => providers::hog::close(pid),
                UiAction::CloseProgram(_) => {}
                UiAction::ReadBatteryHealth if !scripted => providers::sysmon::read_battery_health(providers.sink()),
                UiAction::ReadBatteryHealth => {}
                UiAction::DarkMode if !scripted => quick::toggle_dark_mode(),
                UiAction::ReadMixer if !scripted => providers::mixer::read(providers.sink()),
                UiAction::SetAppVolume(id, level) if !scripted => {
                    providers::mixer::set(providers.sink(), id, f32::from(level) / 100.0)
                }
                UiAction::ReadMixer | UiAction::SetAppVolume(..) => {}
                UiAction::Screenshot
                | UiAction::Lock
                | UiAction::Shortcut(_)
                | UiAction::ReadBrightness
                | UiAction::DarkMode => {}
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
            let drawing = Instant::now();
            let scale = (win.scale() * k) as f32;
            let frame = app.frame(t);
            islet_render::draw(&mut pixmap, &frame, scale);
            if app.has_content(t) {
                let engine = text.get_or_insert_with(TextEngine::new);
                let font = &app.prefs().font;
                if text_font.as_ref() != Some(font) {
                    let files: Vec<std::path::PathBuf> =
                        app.prefs().font_files.iter().map(std::path::PathBuf::from).collect();
                    let choice = (!font.is_empty()).then_some((font.as_str(), files.as_slice()));
                    if !engine.set_font(choice) {
                        trace(&format!("font {font} couldn't be read; using the built-in one"));
                    }
                    text_font = Some(font.clone());
                }
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
            let presenting = Instant::now();
            win.present(&pixmap)?;
            last_frame = Instant::now();
            if let Some(s) = &mut frame_stats {
                s.add(presenting - drawing, last_frame - presenting);
            }
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

/// Time spent per frame drawing it and handing it to the OS, printed every
/// 120 frames (ISLET_FRAME_STATS=1; for measuring, not for users).
#[derive(Default)]
struct FrameStats {
    frames: u32,
    draw: Duration,
    present: Duration,
}

impl FrameStats {
    fn add(&mut self, draw: Duration, present: Duration) {
        self.frames += 1;
        self.draw += draw;
        self.present += present;
        if self.frames == 120 {
            let ms = |d: Duration| d.as_secs_f64() * 1000.0 / f64::from(self.frames);
            eprintln!("ISLET_FRAMES n={} draw={:.3}ms present={:.3}ms", self.frames, ms(self.draw), ms(self.present));
            *self = FrameStats::default();
        }
    }
}

/// What a mirror island (another display) leaves to the main island:
/// alarms and timers ring there, the shortcut and clipboard are global,
/// and network features shouldn't run twice.
fn mirror_prefs(mut p: islet_core::prefs::IslandPrefs) -> islet_core::prefs::IslandPrefs {
    p.hotkey.clear();
    p.launcher_hotkey.clear();
    p.note_hotkey.clear();
    p.weather = None;
    p.calendar = false;
    p.spotify = false;
    p.clipboard = false;
    p.notifications = false;
    p
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
/// What the phone link needs to listen: the pairing key, the port and
/// this computer's name. None unless a phone is paired, the link is on
/// and Islet Pro is active.
fn phone_pairing(prefs: &islet_core::prefs::IslandPrefs) -> Option<([u8; islet_core::phone::KEY_LEN], u16, String)> {
    if !prefs.phone || !pro::active() {
        return None;
    }
    let name = match pro::computer_name() {
        n if n.is_empty() => "Computer".to_string(),
        n => n,
    };
    Some((secrets::phone_key()?, cli::phone_port(), name))
}

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

/// Register the global shortcuts from the preferences ("" removes one):
/// open the island, the launcher, and a quick note.
fn apply_hotkeys(win: &mut Native, p: &islet_core::prefs::IslandPrefs) {
    use platform::{HOTKEY_LAUNCHER, HOTKEY_NOTE, HOTKEY_TOGGLE};
    for (slot, spec) in
        [(HOTKEY_TOGGLE, &p.hotkey), (HOTKEY_LAUNCHER, &p.launcher_hotkey), (HOTKEY_NOTE, &p.note_hotkey)]
    {
        let hotkey = match spec.trim() {
            "" => None,
            s => match islet_core::hotkey::Hotkey::parse(s) {
                Ok(h) => Some(h),
                Err(e) => {
                    providers::debug_log(&format!("shortcut: {e}"));
                    continue;
                }
            },
        };
        if let Err(e) = win.set_hotkey(slot, hotkey) {
            eprintln!("islet: shortcut: {e}");
            providers::debug_log(&format!("shortcut: {e}"));
        }
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
