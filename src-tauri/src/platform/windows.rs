use super::{Result, WindowPlacement};
use crate::geometry::PhysRect;
use tauri::WebviewWindow;
use windows::Win32::Graphics::Gdi::{CreateRectRgn, SetWindowRgn};

pub struct Native;

impl WindowPlacement for Native {
    fn prepare(_window: &WebviewWindow) -> Result<()> {
        // always_on_top and skip_taskbar come from tauri.conf.json.
        Ok(())
    }

    fn set_input_region(window: &WebviewWindow, r: PhysRect) -> Result<()> {
        let hwnd = window.hwnd().map_err(|e| e.to_string())?;
        // SetWindowRgn clips both input and drawing; everything outside the
        // pill is transparent anyway. The system owns the region afterwards.
        unsafe {
            let rgn = CreateRectRgn(r.x, r.y, r.x + r.w, r.y + r.h);
            if rgn.is_invalid() {
                return Err("CreateRectRgn failed".into());
            }
            if SetWindowRgn(hwnd, Some(rgn), true) == 0 {
                return Err("SetWindowRgn failed".into());
            }
        }
        Ok(())
    }
}
