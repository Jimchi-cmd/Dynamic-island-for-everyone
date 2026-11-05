//! Native Wayland backend: a wlr layer-shell surface on the overlay layer,
//! anchored to the top edge (KDE Plasma, sway, Hyprland, river, Wayfire…).
//! GNOME has no layer-shell; there `Linux::create` falls back to X11
//! (XWayland).
//!
//! - pixels: shared-memory ARGB8888 buffers (premultiplied, as tiny-skia);
//! - click-through: `wl_surface.set_input_region` covers only the pill;
//! - never takes keyboard focus (keyboard interactivity: none);
//! - files dragged from a file manager drop onto the shelf through
//!   `wl_data_device` (text/uri-list);
//! - the loop blocks in `poll(2)` on the Wayland socket and an eventfd
//!   (the waker): zero wakeups when idle.

use super::{Button, Event, Result, Waker, WindowSpec};
use islet_core::geometry::Rect;
use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState, Region};
use smithay_client_toolkit::data_device_manager::data_device::{DataDevice, DataDeviceHandler};
use smithay_client_toolkit::data_device_manager::data_offer::{DataOfferHandler, DragOffer};
use smithay_client_toolkit::data_device_manager::data_source::DataSourceHandler;
use smithay_client_toolkit::data_device_manager::{DataDeviceManagerState, WritePipe};
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
    delegate_compositor, delegate_data_device, delegate_layer, delegate_output, delegate_pointer, delegate_registry,
    delegate_seat, delegate_shm, registry_handlers,
};
use std::os::fd::{AsFd, AsRawFd, FromRawFd, OwnedFd};
use std::sync::Arc;
use std::time::Instant;
use tiny_skia::Pixmap;
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::wl_data_device_manager::DndAction;
use wayland_client::protocol::{wl_data_device, wl_data_source, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface};
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
    /// Drag-and-drop (None when the compositor has no data-device manager).
    data_manager: Option<DataDeviceManagerState>,
    data_device: Option<DataDevice>,
    /// The current drag offers file links.
    drag_files: bool,
    /// A drop whose file list is still arriving through its pipe.
    pending_drop: Option<PendingDrop>,
    /// Tests: the last button press's serial (to start a drag from), and
    /// what this client sends when it is the drag source.
    #[cfg(test)]
    press_serial: Option<u32>,
    #[cfg(test)]
    payload: Option<String>,
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
        Self::create_anchored(spec, Anchor::TOP)
    }

    fn create_anchored(spec: WindowSpec, anchor: Anchor) -> Result<Self> {
        let conn = Connection::connect_to_env().map_err(we)?;
        let (globals, mut queue) = registry_queue_init::<State>(&conn).map_err(we)?;
        let qh = queue.handle();
        let compositor = CompositorState::bind(&globals, &qh).map_err(we)?;
        let layer_shell = LayerShell::bind(&globals, &qh).map_err(|e| format!("no layer-shell ({e})"))?;
        let shm = Shm::bind(&globals, &qh).map_err(we)?;
        let data_manager = DataDeviceManagerState::bind(&globals, &qh).ok();
        let surface = compositor.create_surface(&qh);
        let layer = layer_shell.create_layer_surface(&qh, surface, Layer::Overlay, Some("islet"), None);
        layer.set_anchor(anchor);
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
            data_manager,
            data_device: None,
            drag_files: false,
            pending_drop: None,
            #[cfg(test)]
            press_serial: None,
            #[cfg(test)]
            payload: None,
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
                // A pending drop gives up at its own deadline.
                let deadline = match (&self.state.pending_drop, deadline) {
                    (Some(p), Some(d)) => Some(d.min(p.until)),
                    (Some(p), None) => Some(p.until),
                    (None, d) => d,
                };
                let timeout = deadline.map_or(-1, |d| {
                    let ms = d.saturating_duration_since(Instant::now()).as_millis();
                    ms.min(i32::MAX as u128) as i32
                });
                let drop_fd = self.state.pending_drop.as_ref().map_or(-1, |p| p.pipe.as_raw_fd());
                let mut fds = [
                    libc::pollfd { fd: guard.connection_fd().as_raw_fd(), events: libc::POLLIN, revents: 0 },
                    libc::pollfd { fd: self.wake_fd.as_fd().as_raw_fd(), events: libc::POLLIN, revents: 0 },
                    // Negative descriptors are ignored by poll.
                    libc::pollfd { fd: drop_fd, events: libc::POLLIN, revents: 0 },
                ];
                // SAFETY: three valid pollfd entries.
                let n = unsafe { libc::poll(fds.as_mut_ptr(), 3, timeout) };
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
                if drop_fd >= 0 {
                    self.state.read_drop(fds[2].revents != 0);
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
        if self.data_device.is_none() {
            self.data_device = self.data_manager.as_ref().map(|m| m.get_data_device(qh, &seat));
        }
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
                PointerEventKind::Press { button: b, serial, .. } => {
                    #[cfg(test)]
                    {
                        self.press_serial = Some(*serial);
                    }
                    let _ = serial;
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

const URI_LIST: &str = "text/uri-list";

impl State {
    fn drag_offer(&self) -> Option<DragOffer> {
        self.data_device.as_ref()?.data().drag_offer()
    }
}

impl DataDeviceHandler for State {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_data_device::WlDataDevice,
        x: f64,
        y: f64,
        _: &wl_surface::WlSurface,
    ) {
        let Some(offer) = self.drag_offer() else { return };
        self.drag_files = offer.with_mime_types(|m| m.iter().any(|t| t == URI_LIST));
        // Accept only file links, as a copy (the shelf keeps references).
        offer.accept_mime_type(offer.serial, self.drag_files.then(|| URI_LIST.to_string()));
        if self.drag_files {
            offer.set_actions(DndAction::Copy, DndAction::Copy);
            self.events.push(Event::DragOver { x, y });
        } else {
            offer.set_actions(DndAction::empty(), DndAction::empty());
        }
    }

    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_data_device::WlDataDevice) {
        if std::mem::take(&mut self.drag_files) {
            self.events.push(Event::DragLeave);
        }
    }

    fn motion(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_data_device::WlDataDevice, x: f64, y: f64) {
        if self.drag_files {
            self.events.push(Event::DragOver { x, y });
        }
    }

    fn selection(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_data_device::WlDataDevice) {}

    fn drop_performed(&mut self, conn: &Connection, _: &QueueHandle<Self>, _: &wl_data_device::WlDataDevice) {
        let Some(offer) = self.drag_offer() else { return };
        if !std::mem::take(&mut self.drag_files) {
            return;
        }
        match offer.receive(URI_LIST.to_string()) {
            Ok(pipe) => {
                let _ = conn.flush();
                // The source writes the list in its own time; the loop reads
                // it as it arrives (see `read_drop`).
                let until = Instant::now() + std::time::Duration::from_secs(3);
                self.pending_drop = Some(PendingDrop { pipe, offer, data: Vec::new(), until });
            }
            Err(_) => {
                offer.finish();
                offer.destroy();
                self.events.push(Event::DragLeave);
            }
        }
    }
}

struct PendingDrop {
    pipe: smithay_client_toolkit::data_device_manager::ReadPipe,
    offer: DragOffer,
    data: Vec<u8>,
    until: Instant,
}

impl State {
    /// Read what has arrived of a dropped file list; once the source closes
    /// the pipe (or takes too long), finish the drop.
    fn read_drop(&mut self, readable: bool) {
        let Some(p) = &mut self.pending_drop else { return };
        let mut done = Instant::now() >= p.until;
        if readable {
            let mut buf = [0u8; 4096];
            // SAFETY: reading into a local buffer from the pipe we hold.
            let n = unsafe { libc::read(p.pipe.as_raw_fd(), buf.as_mut_ptr().cast(), buf.len()) };
            if n > 0 {
                p.data.extend_from_slice(&buf[..n as usize]);
                done |= p.data.len() > 1 << 20;
            } else if n == 0 || std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted {
                // End of the list, or a broken pipe.
                let list = String::from_utf8_lossy(&p.data).into_owned();
                let paths = islet_core::shelf::paths_from_uri_list(&list);
                p.offer.finish();
                p.offer.destroy();
                self.pending_drop = None;
                self.events.push(if paths.is_empty() { Event::DragLeave } else { Event::Drop(paths) });
                return;
            }
        }
        if done {
            p.offer.destroy();
            self.pending_drop = None;
            self.events.push(Event::DragLeave);
        }
    }
}

impl DataOfferHandler for State {
    fn source_actions(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &mut DragOffer, _: DndAction) {}
    fn selected_action(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &mut DragOffer, _: DndAction) {}
}

/// Islet never offers data itself; the trait comes with the data device.
impl DataSourceHandler for State {
    fn accept_mime(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_data_source::WlDataSource,
        _: Option<String>,
    ) {
    }
    fn send_request(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_data_source::WlDataSource,
        _: String,
        pipe: WritePipe,
    ) {
        #[cfg(test)]
        if let Some(p) = &self.payload {
            use std::io::Write;
            let mut pipe = pipe;
            let _ = pipe.write_all(p.as_bytes());
            return;
        }
        drop(pipe);
    }
    fn cancelled(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_data_source::WlDataSource) {}
    fn dnd_dropped(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_data_source::WlDataSource) {}
    fn dnd_finished(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_data_source::WlDataSource) {}
    fn action(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_data_source::WlDataSource, _: DndAction) {}
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
delegate_data_device!(State);

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

    #[test]
    fn files_dropped_on_the_island() {
        if std::env::var_os("WAYLAND_DISPLAY").is_none() {
            eprintln!("skipping: no Wayland display");
            return;
        }
        let Ok(mut island) = Wayland::create(WindowSpec { width: 420.0, height: 260.0 }) else {
            eprintln!("skipping: no layer-shell compositor");
            return;
        };
        if island.state.data_manager.is_none() {
            eprintln!("skipping: no data-device manager");
            return;
        }
        island.set_input_region(Rect::new(147.0, 0.0, 126.0, 37.0)).unwrap();
        // A second client at the bottom edge plays the file manager.
        let mut source = Wayland::create_anchored(WindowSpec { width: 200.0, height: 200.0 }, Anchor::BOTTOM).unwrap();
        source.set_input_region(Rect::new(0.0, 0.0, 200.0, 200.0)).unwrap();
        for w in [&mut island, &mut source] {
            let (pw, ph) = w.size_px();
            let mut frame = Pixmap::new(pw, ph).unwrap();
            frame.fill(tiny_skia::Color::from_rgba8(0, 0, 0, 255));
            w.present(&frame).unwrap();
        }
        source.state.payload = Some("# a comment\r\nfile:///tmp/My%20Notes.txt\r\n".into());

        let conn = Connection::connect_to_env().unwrap();
        let (globals, mut queue) = registry_queue_init::<Driver>(&conn).unwrap();
        let qh = queue.handle();
        let Ok(manager) = globals.bind::<mgr::ZwlrVirtualPointerManagerV1, _, _>(&qh, 1..=2, ()) else {
            eprintln!("skipping: no virtual-pointer protocol");
            return;
        };
        let pointer = manager.create_virtual_pointer(None, &qh, ());
        queue.roundtrip(&mut Driver).unwrap();

        let mut island_events = Vec::new();
        let mut source_events = Vec::new();
        let mut pump = |island: &mut Wayland, source: &mut Wayland, ms: u64| {
            let until = Instant::now() + std::time::Duration::from_millis(ms);
            while Instant::now() < until {
                let soon = || Some(Instant::now() + std::time::Duration::from_millis(10));
                island.wait(soon(), &mut island_events).unwrap();
                source.wait(soon(), &mut source_events).unwrap();
            }
        };
        pump(&mut island, &mut source, 300);
        let mut t = 0;
        let mut send = |f: &dyn Fn(u32)| {
            t += 10;
            f(t);
            pointer.frame();
            conn.flush().unwrap();
        };
        use wayland_client::protocol::wl_pointer::ButtonState;
        // Press on the source (bottom centre of a 1280×800 output)…
        send(&|t| pointer.motion_absolute(t, 640, 700, 1280, 800));
        send(&|t| pointer.button(t, BTN_LEFT, ButtonState::Pressed));
        pump(&mut island, &mut source, 300);
        let serial = source.state.press_serial.expect("the source saw the press");
        let qh_source = source.queue.handle();
        let drag = source.state.data_manager.as_ref().unwrap().create_drag_and_drop_source(
            &qh_source,
            [URI_LIST],
            DndAction::Copy,
        );
        drag.start_drag(source.state.data_device.as_ref().unwrap(), source.state.layer.wl_surface(), None, serial);
        source.conn.flush().unwrap();
        pump(&mut island, &mut source, 200);
        // …drag up onto the island and let go.
        for y in [400, 100, 20, 18] {
            send(&|t| pointer.motion_absolute(t, 640, y, 1280, 800));
            pump(&mut island, &mut source, 100);
        }
        send(&|t| pointer.button(t, BTN_LEFT, ButtonState::Released));
        pump(&mut island, &mut source, 600);
        drop(drag);

        assert!(island_events.iter().any(|e| matches!(e, Event::DragOver { .. })), "{island_events:?}");
        assert!(
            island_events.iter().any(|e| matches!(e, Event::Drop(p) if p == &["/tmp/My Notes.txt".to_string()])),
            "{island_events:?}"
        );
    }
}
