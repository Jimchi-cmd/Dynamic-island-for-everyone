//! Islet island process: a native, webview-free overlay.
//!
//! Environment:
//! - `ISLET_PERF_DEMO=1`: expand/collapse every 1.2 s (perf harness)
//! - `ISLET_REDUCE_MOTION=1`: snap instead of animating

// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod platform;

use app::{App, FRAME, WINDOW};
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
    let mut app = App::new(env_on("ISLET_PERF_DEMO"), env_on("ISLET_REDUCE_MOTION"));

    let mut win = Native::create(WindowSpec { width: WINDOW.0, height: WINDOW.1 })?;
    let new_pixmap = |win: &Native| {
        let (w, h) = win.size_px();
        Pixmap::new(w.max(1), h.max(1)).ok_or_else(|| "could not allocate frame buffer".to_string())
    };
    let mut pixmap = new_pixmap(&win)?;
    let epoch = Instant::now();
    let now = || epoch.elapsed().as_secs_f64();

    let mut needs_frame = true;
    let mut first_frame = true;
    let mut last_frame = epoch;
    let mut region = app.input_region(0.0);
    win.set_input_region(region)?;
    let mut events = Vec::new();

    loop {
        let t = now();
        // Sleep until the next animation frame, the next timer, or an event.
        let mut deadline: Option<Instant> = None;
        if needs_frame || app.animating(t) {
            deadline = Some((last_frame + FRAME).max(Instant::now()));
        }
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
        needs_frame |= app.tick(t);

        let new_region = app.input_region(t);
        if new_region != region {
            region = new_region;
            win.set_input_region(region)?;
        }

        let frame_due = Instant::now() >= last_frame + FRAME;
        if (needs_frame || app.animating(t)) && frame_due {
            islet_render::draw(&mut pixmap, &app.frame(t), win.scale() as f32);
            win.present(&pixmap)?;
            last_frame = Instant::now();
            // One extra frame after settling draws the exact final shape.
            needs_frame = app.animating(t);
            if first_frame {
                first_frame = false;
                println!("ISLET_VISIBLE {}", started.elapsed().as_millis());
                let _ = std::io::stdout().flush();
            }
        }
    }
}
