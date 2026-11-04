use super::{Result, WindowPlacement};
use crate::geometry::PhysRect;
use gtk::cairo::{RectangleInt, Region};
use gtk::prelude::*;
use tauri::WebviewWindow;

pub struct Native;

/// Phase 0: run under X11/XWayland everywhere, so window positioning and
/// always-on-top work on every desktop. Phase 1 adds gtk-layer-shell for
/// wlroots/KDE Wayland and keeps XWayland only for GNOME.
pub fn early_init() {
    if std::env::var_os("GDK_BACKEND").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_some() {
        std::env::set_var("GDK_BACKEND", "x11");
    }
}

impl WindowPlacement for Native {
    fn prepare(window: &WebviewWindow) -> Result<()> {
        let gtk_window = window.gtk_window().map_err(|e| e.to_string())?;
        gtk_window.set_type_hint(gtk::gdk::WindowTypeHint::Dock);
        gtk_window.set_keep_above(true);
        gtk_window.stick();
        Ok(())
    }

    fn set_input_region(window: &WebviewWindow, r: PhysRect) -> Result<()> {
        let gtk_window = window.gtk_window().map_err(|e| e.to_string())?;
        // GTK works in device-independent units; convert back from physical.
        let scale = gtk_window.scale_factor().max(1);
        let region = Region::create_rectangle(&RectangleInt::new(
            r.x / scale,
            r.y / scale,
            (r.w + scale - 1) / scale,
            (r.h + scale - 1) / scale,
        ));
        // X11: XShape input region. Wayland: wl_surface.set_input_region.
        gtk_window.input_shape_combine_region(Some(&region));
        Ok(())
    }
}
