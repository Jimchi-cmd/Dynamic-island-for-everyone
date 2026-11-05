//! Output volume on macOS: CoreAudio calls back when the default output
//! device's volume or mute changes (and when the default device changes).

use super::{debug_log, Sink};
use islet_core::activity::{Hud, Report};
use std::ffi::c_void;
use std::sync::Mutex;

#[repr(C)]
pub(super) struct Address {
    pub(super) selector: u32,
    pub(super) scope: u32,
    pub(super) element: u32,
}

pub(super) type Listener =
    unsafe extern "C" fn(object: u32, count: u32, addresses: *const Address, data: *mut c_void) -> i32;

#[link(name = "CoreAudio", kind = "framework")]
extern "C" {
    pub(super) fn AudioObjectGetPropertyData(
        object: u32,
        address: *const Address,
        qualifier_size: u32,
        qualifier: *const c_void,
        size: *mut u32,
        data: *mut c_void,
    ) -> i32;
    pub(super) fn AudioObjectAddPropertyListener(
        object: u32,
        address: *const Address,
        listener: Listener,
        data: *mut c_void,
    ) -> i32;
}

const fn code(s: &[u8; 4]) -> u32 {
    u32::from_be_bytes(*s)
}

const SYSTEM: u32 = 1;
const DEFAULT_OUTPUT: u32 = code(b"dOut");
const GLOBAL: u32 = code(b"glob");
const OUTPUT: u32 = code(b"outp");
/// The device's overall volume, even when it only has per-channel controls.
const MAIN_VOLUME: u32 = code(b"vmvc");
const MUTE: u32 = code(b"mute");

fn get<T: Default>(object: u32, selector: u32, scope: u32) -> Option<T> {
    let address = Address { selector, scope, element: 0 };
    let mut value = T::default();
    let mut size = std::mem::size_of::<T>() as u32;
    // SAFETY: `value` is a properly sized, writable buffer for this property.
    let status = unsafe {
        AudioObjectGetPropertyData(object, &address, 0, std::ptr::null(), &mut size, (&mut value as *mut T).cast())
    };
    (status == 0).then_some(value)
}

struct Ctx {
    sink: Sink,
    last: Mutex<(f32, bool)>,
    watched: Mutex<Vec<u32>>,
}

fn read(device: u32) -> Option<(f32, bool)> {
    let level = get::<f32>(device, MAIN_VOLUME, OUTPUT)?;
    let muted = get::<u32>(device, MUTE, OUTPUT).unwrap_or(0) != 0;
    Some((level, muted))
}

fn watch(ctx: &'static Ctx, device: u32) {
    let mut watched = ctx.watched.lock().unwrap();
    if watched.contains(&device) {
        return;
    }
    watched.push(device);
    for selector in [MAIN_VOLUME, MUTE] {
        let address = Address { selector, scope: OUTPUT, element: 0 };
        // SAFETY: `ctx` lives for the process lifetime (leaked).
        unsafe { AudioObjectAddPropertyListener(device, &address, changed, (ctx as *const Ctx).cast_mut().cast()) };
    }
}

unsafe extern "C" fn changed(object: u32, _count: u32, _addresses: *const Address, data: *mut c_void) -> i32 {
    // SAFETY: `data` is the leaked `Ctx` registered with this listener.
    let ctx: &'static Ctx = unsafe { &*(data as *const Ctx) };
    let Some(device) = get::<u32>(SYSTEM, DEFAULT_OUTPUT, GLOBAL) else { return 0 };
    if object == SYSTEM {
        // The default output changed (headphones plugged in): follow it,
        // without a pop-up for the switch itself.
        watch(ctx, device);
        if let Some(now) = read(device) {
            *ctx.last.lock().unwrap() = now;
        }
        return 0;
    }
    if object != device {
        return 0;
    }
    if let Some(now) = read(device) {
        let mut last = ctx.last.lock().unwrap();
        if (now.0 - last.0).abs() > 0.001 || now.1 != last.1 {
            *last = now;
            ctx.sink.send(Report::Hud(Hud::Volume { level: now.0, muted: now.1 }));
        }
    }
    0
}

pub fn start(sink: Sink) {
    let Some(device) = get::<u32>(SYSTEM, DEFAULT_OUTPUT, GLOBAL) else {
        debug_log("volume pop-ups unavailable (no default output device)");
        return;
    };
    let start = read(device).unwrap_or((0.0, false));
    let ctx: &'static Ctx = Box::leak(Box::new(Ctx { sink, last: Mutex::new(start), watched: Mutex::new(Vec::new()) }));
    watch(ctx, device);
    let address = Address { selector: DEFAULT_OUTPUT, scope: GLOBAL, element: 0 };
    // SAFETY: as in `watch`.
    unsafe { AudioObjectAddPropertyListener(SYSTEM, &address, changed, (ctx as *const Ctx).cast_mut().cast()) };
}
