use super::{Result, WindowPlacement};
use crate::geometry::PhysRect;
use objc2::msg_send;
use objc2::runtime::AnyObject;
use tauri::WebviewWindow;

pub struct Native;

/// NSStatusWindowLevel: above the menu bar so the pill can sit at y = 0.
const STATUS_WINDOW_LEVEL: isize = 25;
// NSWindowCollectionBehavior flags.
const CAN_JOIN_ALL_SPACES: usize = 1 << 0;
const STATIONARY: usize = 1 << 4;
const IGNORES_CYCLE: usize = 1 << 6;
const FULL_SCREEN_AUXILIARY: usize = 1 << 8;

impl WindowPlacement for Native {
    fn prepare(window: &WebviewWindow) -> Result<()> {
        let ns_window = window.ns_window().map_err(|e| e.to_string())? as *mut AnyObject;
        if ns_window.is_null() {
            return Err("no NSWindow".into());
        }
        // SAFETY: ns_window is a live NSWindow owned by Tauri; called on the main thread
        // from the setup hook.
        unsafe {
            let _: () = msg_send![ns_window, setLevel: STATUS_WINDOW_LEVEL];
            let behavior = CAN_JOIN_ALL_SPACES | STATIONARY | IGNORES_CYCLE | FULL_SCREEN_AUXILIARY;
            let _: () = msg_send![ns_window, setCollectionBehavior: behavior];
            let _: () = msg_send![ns_window, setHasShadow: false];
        }
        Ok(())
    }

    fn set_input_region(_window: &WebviewWindow, _r: PhysRect) -> Result<()> {
        // Phase 0: the whole (small) window takes input on macOS. Phase 1
        // adds a hit-testing NSView so clicks outside the pill fall through.
        Ok(())
    }
}
