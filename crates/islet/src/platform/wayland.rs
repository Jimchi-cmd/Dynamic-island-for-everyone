//! Native Wayland backend: a wlr layer-shell surface on the overlay layer,
//! anchored to the top edge (KDE Plasma, sway, Hyprland, river, Wayfire…).
//! GNOME has no layer-shell; there `Linux::create` falls back to X11
//! (XWayland).
//!
//! - pixels: shared-memory ARGB8888 buffers (premultiplied, as tiny-skia);
//! - click-through: `wl_surface.set_input_region` covers only the pill;
//! - never takes keyboard focus (keyboard interactivity: none);
//! - the loop blocks in `poll(2)` on the Wayland socket and an eventfd
//!   (the waker): zero wakeups when idle.

use super::{Button, Event, Result, Waker, WindowSpec};
use islet_core::geometry::Rect;
use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState, Region};
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::seat::pointer::{PointerEvent, PointerEventKind, PointerHandler};
use smithay_client_toolkit::seat::{Capability, SeatHandler, SeatState};
use smithay_client_toolkit::shell::wlr_layer::{
    Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface, LayerSurfaceConfigure,
};
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shm::slot::SlotPool;
use smithay_client_toolkit::shm::{Shm, ShmHandler};
use smithay_client_toolkit::{
    delegate_compositor, delegate_layer, delegate_output, delegate_pointer, delegate_registry, delegate_seat,
    delegate_shm, registry_handlers,
};
use std::os::fd::{AsFd, AsRawFd, FromRawFd, OwnedFd};
use std::sync::Arc;
use std::time::Instant;
use tiny_skia::Pixmap;
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{wl_output, wl_pointer, wl_seat, wl_shm, wl_surface};
use wayland_client::{Connection, EventQueue, QueueHandle};

/// Linux input event codes (linux/input-event-codes.h).
const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;
const BTN_MIDDLE: u32 = 0x112;

fn we<E: std::fmt::Display>(e: E) -> String {
    format!("Wayland: {e}")
}

struct State {
    registry: RegistryState,
    seats: SeatState,
    outputs: OutputState,
    shm: Shm,
    layer: LayerSurface,
    pointer: Option<wl_pointer::WlPointer>,
    configured: bool,
    scale: i32,
    events: Vec<Event>,
}

pub struct Wayland {
    conn: Connection,
    queue: EventQueue<State>,
    state: State,
    compositor: CompositorState,
    pool: SlotPool,
    spec: WindowSpec,
    wake_fd: Arc<OwnedFd>,
}

impl Wayland {
    /// Connect and create the overlay surface. Fails (so X11 is used) when
    /// there's no Wayland session or the compositor lacks layer-shell.
    pub fn create(spec: WindowSpec) -> Result<Self> {
        let conn = Connection::connect_to_env().map_err(we)?;
        let (globals, mut queue) = registry_queue_init::<State>(&conn).map_err(we)?;
        let qh = queue.handle();
        let compositor = CompositorState::bind(&globals, &qh).map_err(we)?;
        let layer_shell = LayerShell::bind(&globals, &qh).map_err(|e| format!("no layer-shell ({e})"))?;
        let shm = Shm::bind(&globals, &qh).map_err(we)?;
        let surface = compositor.create_surface(&qh);
        let layer = layer_shell.create_layer_surface(&qh, surface, Layer::Overlay, Some("islet"), None);
        layer.set_anchor(Anchor::TOP);
        layer.set_size(spec.width as u32, spec.height as u32);
        // Sit at the very top edge, over panels, like the iPhone's island.
        layer.set_exclusive_zone(-1);
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer.commit();
        let pool = SlotPool::new((spec.width * spec.height * 4.0 * 4.0) as usize, &shm).map_err(we)?;
        let mut state = State {
            registry: RegistryState::new(&globals),
            seats: SeatState::new(&globals, &qh),
            outputs: OutputState::new(&globals, &qh),
            shm,
            layer,
            pointer: None,
            configured: false,
            scale: 1,
            events: Vec::new(),
        };
        while !state.configured {
            queue.blocking_dispatch(&mut state).map_err(we)?;
        }
        // SAFETY: eventfd returns a new descriptor (or -1, checked) that we own.
        let raw = unsafe { libc::eventfd(0, libc::EFD_NONBLOCK | libc::EFD_CLOEXEC) };
        if raw < 0 {
            return Err(format!("eventfd: {}", std::io::Error::last_os_error()));
        }
        // SAFETY: `raw` is a valid, owned file descriptor.
        let wake_fd = Arc::new(unsafe { OwnedFd::from_raw_fd(raw) });
        state.events.clear();
        Ok(Self { conn, queue, state, compositor, pool, spec, wake_fd })
    }

    pub fn scale(&self) -> f64 {
        f64::from(self.state.scale)
    }

    pub fn size_px(&self) -> (u32, u32) {
        let s = self.state.scale.max(1) as f64;
        ((self.spec.width * s) as u32, (self.spec.height * s) as u32)
    }

    pub fn present(&mut self, frame: &Pixmap) -> Result<()> {
        let (w, h) = (frame.width() as i32, frame.height() as i32);
        let (buffer, canvas) = self.pool.create_buffer(w, h, w * 4, wl_shm::Format::Argb8888).map_err(we)?;
        // Premultiplied RGBA → ARGB8888 (BGRA bytes on little-endian).
        for (dst, src) in canvas.chunks_exact_mut(4).zip(frame.data().chunks_exact(4)) {
            dst.copy_from_slice(&[src[2], src[1], src[0], src[3]]);
        }
        let surface = self.state.layer.wl_surface();
        surface.set_buffer_scale(self.state.scale);
        buffer.attach_to(surface).map_err(we)?;
        surface.damage_buffer(0, 0, w, h);
        surface.commit();
        self.conn.flush().map_err(we)
    }

    pub fn set_input_region(&mut self, rect: Rect) -> Result<()> {
        let region = Region::new(&self.compositor).map_err(we)?;
        region.add(rect.x.floor() as i32, rect.y.floor() as i32, rect.w.ceil() as i32, rect.h.ceil() as i32);
        let surface = self.state.layer.wl_surface();
        surface.set_input_region(Some(region.wl_region()));
        surface.commit();
        self.conn.flush().map_err(we)
    }

    pub fn waker(&self) -> Waker {
        let fd = self.wake_fd.clone();
        Waker::new(move || {
            let one: u64 = 1;
            // SAFETY: writing 8 bytes to our eventfd.
            unsafe { libc::write(fd.as_raw_fd(), (&one as *const u64).cast(), 8) };
        })
    }

    pub fn wait(&mut self, deadline: Option<Instant>, out: &mut Vec<Event>) -> Result<()> {
        self.queue.dispatch_pending(&mut self.state).map_err(we)?;
        if self.state.events.is_empty() {
            self.conn.flush().map_err(we)?;
            if let Some(guard) = self.queue.prepare_read() {
                let timeout = deadline.map_or(-1, |d| {
                    let ms = d.saturating_duration_since(Instant::now()).as_millis();
                    ms.min(i32::MAX as u128) as i32
                });
                let mut fds = [
                    libc::pollfd { fd: guard.connection_fd().as_raw_fd(), events: libc::POLLIN, revents: 0 },
                    libc::pollfd { fd: self.wake_fd.as_fd().as_raw_fd(), events: libc::POLLIN, revents: 0 },
                ];
                // SAFETY: two valid pollfd entries.
                let n = unsafe { libc::poll(fds.as_mut_ptr(), 2, timeout) };
                if n > 0 && fds[0].revents & libc::POLLIN != 0 {
                    let _ = guard.read();
                } else {
                    drop(guard);
                }
                if fds[1].revents & libc::POLLIN != 0 {
                    let mut buf = 0u64;
                    // SAFETY: draining our eventfd into an 8-byte buffer.
                    unsafe { libc::read(self.wake_fd.as_raw_fd(), (&mut buf as *mut u64).cast(), 8) };
                }
            }
            self.queue.dispatch_pending(&mut self.state).map_err(we)?;
        }
        out.append(&mut self.state.events);
        Ok(())
    }
}

impl CompositorHandler for State {
    fn scale_factor_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, factor: i32) {
        if factor != self.scale && factor > 0 {
            self.scale = factor;
            self.events.push(Event::ScaleChanged(f64::from(factor)));
        }
    }

    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }

    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {}

    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for State {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }

    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}

    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}

    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl LayerShellHandler for State {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        self.events.push(Event::Quit);
    }

    fn configure(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface, _: LayerSurfaceConfigure, _: u32) {
        if self.configured {
            self.events.push(Event::Redraw);
        }
        self.configured = true;
    }
}

impl SeatHandler for State {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seats
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}

    fn new_capability(&mut self, _: &Connection, qh: &QueueHandle<Self>, seat: wl_seat::WlSeat, cap: Capability) {
        if cap == Capability::Pointer && self.pointer.is_none() {
            self.pointer = self.seats.get_pointer(qh, &seat).ok();
        }
    }

    fn remove_capability(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat, cap: Capability) {
        if cap == Capability::Pointer {
            if let Some(p) = self.pointer.take() {
                p.release();
            }
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl PointerHandler for State {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        let button = |b: u32| match b {
            BTN_LEFT => Some(Button::Primary),
            BTN_RIGHT => Some(Button::Secondary),
            BTN_MIDDLE => Some(Button::Middle),
            _ => None,
        };
        for e in events {
            let (x, y) = e.position;
            let ev = match &e.kind {
                PointerEventKind::Enter { .. } | PointerEventKind::Motion { .. } => Some(Event::PointerMove { x, y }),
                PointerEventKind::Leave { .. } => Some(Event::PointerLeave),
                PointerEventKind::Press { button: b, .. } => {
                    button(*b).map(|button| Event::PointerDown { x, y, button })
                }
                PointerEventKind::Release { button: b, .. } => {
                    button(*b).map(|button| Event::PointerUp { x, y, button })
                }
                PointerEventKind::Axis { horizontal, vertical, .. } => {
                    // Wheel clicks are 15 px; normalise to lines like the other backends.
                    let (dx, dy) = (horizontal.absolute / 15.0, vertical.absolute / 15.0);
                    (dx != 0.0 || dy != 0.0).then_some(Event::Scroll { dx, dy })
                }
            };
            self.events.extend(ev);
        }
    }
}

impl ShmHandler for State {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for State {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState, SeatState];
}

delegate_compositor!(State);
delegate_output!(State);
delegate_shm!(State);
delegate_seat!(State);
delegate_pointer!(State);
delegate_layer!(State);
delegate_registry!(State);

#[cfg(test)]
mod tests {
    //! Needs a wlroots compositor (`WAYLAND_DISPLAY`), e.g. headless sway:
    //! `WLR_BACKENDS=headless sway`; skips otherwise.
    use super::*;
    use wayland_client::protocol::wl_registry;
    use wayland_client::{Dispatch, QueueHandle as Qh};
    use wayland_protocols_wlr::virtual_pointer::v1::client::{
        zwlr_virtual_pointer_manager_v1 as mgr, zwlr_virtual_pointer_v1 as vp,
    };

    struct Driver;
    impl Dispatch<wl_registry::WlRegistry, wayland_client::globals::GlobalListContents> for Driver {
        fn event(
            _: &mut Self,
            _: &wl_registry::WlRegistry,
            _: wl_registry::Event,
            _: &wayland_client::globals::GlobalListContents,
            _: &Connection,
            _: &Qh<Self>,
        ) {
        }
    }
    impl Dispatch<mgr::ZwlrVirtualPointerManagerV1, ()> for Driver {
        fn event(
            _: &mut Self,
            _: &mgr::ZwlrVirtualPointerManagerV1,
            _: mgr::Event,
            _: &(),
            _: &Connection,
            _: &Qh<Self>,
        ) {
        }
    }
    impl Dispatch<vp::ZwlrVirtualPointerV1, ()> for Driver {
        fn event(_: &mut Self, _: &vp::ZwlrVirtualPointerV1, _: vp::Event, _: &(), _: &Connection, _: &Qh<Self>) {}
    }

    #[test]
    fn pointer_input_reaches_the_layer_surface() {
        if std::env::var_os("WAYLAND_DISPLAY").is_none() {
            eprintln!("skipping: no Wayland display");
            return;
        }
        let Ok(mut island) = Wayland::create(WindowSpec { width: 420.0, height: 260.0 }) else {
            eprintln!("skipping: no layer-shell compositor");
            return;
        };
        island.set_input_region(Rect::new(147.0, 0.0, 126.0, 37.0)).unwrap();
        // A surface is mapped (and gets input) once it has a buffer.
        let (w, h) = island.size_px();
        let mut frame = Pixmap::new(w, h).unwrap();
        frame.fill(tiny_skia::Color::from_rgba8(0, 0, 0, 255));
        island.present(&frame).unwrap();

        // A persistent virtual pointer on a second connection.
        let conn = Connection::connect_to_env().unwrap();
        let (globals, mut queue) = registry_queue_init::<Driver>(&conn).unwrap();
        let qh = queue.handle();
        let Ok(manager) = globals.bind::<mgr::ZwlrVirtualPointerManagerV1, _, _>(&qh, 1..=2, ()) else {
            eprintln!("skipping: no virtual-pointer protocol");
            return;
        };
        let pointer = manager.create_virtual_pointer(None, &qh, ());
        queue.roundtrip(&mut Driver).unwrap();

        let mut collected = Vec::new();
        let pump = |island: &mut Wayland, collected: &mut Vec<Event>, ms: u64| {
            let until = Instant::now() + std::time::Duration::from_millis(ms);
            while Instant::now() < until {
                island.wait(Some(Instant::now() + std::time::Duration::from_millis(20)), collected).unwrap();
            }
        };
        pump(&mut island, &mut collected, 300); // the island binds the new pointer
                                                // The island is centred at the top of a 1280 px wide output: (640, 18).
        let mut t = 0;
        let mut send = |f: &dyn Fn(u32)| {
            t += 10;
            f(t);
            pointer.frame();
            conn.flush().unwrap();
        };
        send(&|t| pointer.motion_absolute(t, 640, 18, 1280, 800));
        send(&|t| pointer.button(t, BTN_LEFT, wayland_client::protocol::wl_pointer::ButtonState::Pressed));
        send(&|t| pointer.button(t, BTN_LEFT, wayland_client::protocol::wl_pointer::ButtonState::Released));
        pump(&mut island, &mut collected, 500);

        let moved = collected
            .iter()
            .any(|e| matches!(e, Event::PointerMove { x, y } if (*x - 210.0).abs() < 2.0 && (*y - 18.0).abs() < 2.0));
        let pressed = collected.iter().any(|e| matches!(e, Event::PointerDown { button: Button::Primary, .. }));
        let released = collected.iter().any(|e| matches!(e, Event::PointerUp { button: Button::Primary, .. }));
        assert!(moved && pressed && released, "{collected:?}");
    }
}
