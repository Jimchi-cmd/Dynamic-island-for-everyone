//! Islet core.

pub use islet_core::geometry;
mod platform;

use geometry::Rect;
use platform::{Native, WindowPlacement};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::Instant;
use tauri::{Manager, PhysicalPosition, WebviewWindow};

const ISLAND_LABEL: &str = "island";
/// Logical window size; must match `WINDOW` in ui/lib/geometry.ts and tauri.conf.json.
pub const WINDOW_LOGICAL: (f64, f64) = (400.0, 220.0);

static STARTED: OnceLock<Instant> = OnceLock::new();
static READY_REPORTED: AtomicBool = AtomicBool::new(false);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BootConfig {
    perf_demo: bool,
}

#[tauri::command]
fn boot_config() -> BootConfig {
    BootConfig { perf_demo: std::env::var_os("ISLET_PERF_DEMO").is_some() }
}

/// First frame painted. Prints a marker the perf harness waits for.
#[tauri::command]
fn ui_ready() {
    if !READY_REPORTED.swap(true, Ordering::SeqCst) {
        let ms = STARTED.get().map(|t| t.elapsed().as_millis()).unwrap_or(0);
        println!("ISLET_VISIBLE {ms}");
    }
}

#[tauri::command]
fn set_hit_region(window: WebviewWindow, rect: Rect) -> Result<(), String> {
    let scale = window.scale_factor().map_err(|e| e.to_string())?;
    Native::set_input_region(&window, rect.to_physical(scale))
}

fn place_top_center(window: &WebviewWindow) -> tauri::Result<()> {
    let monitor = match window.primary_monitor()? {
        Some(m) => m,
        None => return Ok(()),
    };
    // Don't ask the window for its size: GTK reports 0 until it is mapped.
    let width = (WINDOW_LOGICAL.0 * monitor.scale_factor()).round() as u32;
    let pos = monitor.position();
    let msize = monitor.size();
    let (x, y) = geometry::top_center((pos.x, pos.y), (msize.width, msize.height), width);
    window.set_position(PhysicalPosition::new(x, y))
}

pub fn run() {
    STARTED.get_or_init(Instant::now);
    platform::early_init();

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![boot_config, ui_ready, set_hit_region])
        .setup(|app| {
            let window = app.get_webview_window(ISLAND_LABEL).ok_or("island window missing from tauri.conf.json")?;
            if let Err(e) = Native::prepare(&window) {
                eprintln!("islet: window prepare failed: {e}");
            }
            place_top_center(&window)?;
            window.show()?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Islet");
}
