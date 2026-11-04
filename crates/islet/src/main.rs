//! Islet island process: a native, webview-free overlay.
//!
//! Environment:
//! - `ISLET_PERF_DEMO=1`: expand/collapse every 1.2 s (perf harness)
//! - `ISLET_DEMO=tour`: walk through every presentation (visual review)
//! - `ISLET_DEMO=media`: a scripted music + battery session (no OS services)
//! - `ISLET_REDUCE_MOTION=1`: snap instead of animating
//!
//! Right-click the island to quit (until the context menu exists).

// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod clock;
mod content;
mod platform;
mod providers;

use app::{App, Demo, FRAME, WINDOW};
use islet_core::activity::Report;
use islet_render::text::TextEngine;
use platform::{Backend, Event, Native, WindowSpec};
use std::io::Write;
use std::time::{Duration, Instant};
use tiny_skia::Pixmap;

fn main() {
    if let Err(e) = run() {
        eprintln!("islet: {e}");
        std::process::exit(1);
    }
}

fn run() -> platform::Result<()> {
    let started = Instant::now();
    let env_on = |k: &str| std::env::var_os(k).is_some_and(|v| v != "0");
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
    let new_pixmap = |win: &Native| {
        let (w, h) = win.size_px();
        Pixmap::new(w.max(1), h.max(1)).ok_or_else(|| "could not allocate frame buffer".to_string())
    };
    let mut pixmap = new_pixmap(&win)?;
    let epoch = Instant::now();
    let now = || epoch.elapsed().as_secs_f64();

    let providers = providers::Providers::start(win.waker());

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
        if app.wants_quit() {
            return Ok(());
        }
        needs_frame |= app.set_time(clock::now(), t);
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
            win.present(&pixmap)?;
            last_frame = Instant::now();
            needs_frame = false;
            if first_frame {
                first_frame = false;
                println!("ISLET_VISIBLE {}", started.elapsed().as_millis());
                let _ = std::io::stdout().flush();
            }
        }
    }
}
