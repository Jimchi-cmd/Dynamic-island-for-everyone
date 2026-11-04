//! macOS: battery levels through IOKit.
//!
//! - The laptop: IOKit power sources, with a run-loop notification on every
//!   change (charger plug/unplug, level).
//! - Bluetooth mice, keyboards and trackpads: the `BatteryPercent` IOKit
//!   publishes on `AppleDeviceManagementHIDEventService`. Matching and
//!   termination notifications report connects and disconnects; levels are
//!   re-read every 10 minutes only while such a device is connected, since
//!   IOKit sends no notification when they change.
//!
//! AirPods-style per-earbud levels need private APIs and come later.

use super::{debug_log, kind_from_name, Sink};
use islet_core::activity::{BatteryDevice, DeviceKind, Report};
use std::collections::HashMap;
use std::ffi::{c_char, c_void, CStr};
use std::sync::{Arc, Mutex};
use std::time::Duration;

type CFTypeRef = *const c_void;
type CFStringRef = *const c_void;
type CFDictionaryRef = *const c_void;
type CFArrayRef = *const c_void;
type CFRunLoopRef = *const c_void;
type CFRunLoopSourceRef = *const c_void;
type IoObject = u32;

#[link(name = "IOKit", kind = "framework")]
extern "C" {
    fn IOServiceMatching(name: *const c_char) -> CFDictionaryRef;
    fn IOServiceGetMatchingServices(main_port: u32, matching: CFDictionaryRef, existing: *mut IoObject) -> i32;
    fn IOIteratorNext(iterator: IoObject) -> IoObject;
    fn IOObjectRelease(object: IoObject) -> i32;
    fn IORegistryEntryCreateCFProperty(
        entry: IoObject,
        key: CFStringRef,
        allocator: CFTypeRef,
        options: u32,
    ) -> CFTypeRef;
    fn IORegistryEntryGetRegistryEntryID(entry: IoObject, id: *mut u64) -> i32;
    fn IONotificationPortCreate(main_port: u32) -> *mut c_void;
    fn IONotificationPortGetRunLoopSource(port: *mut c_void) -> CFRunLoopSourceRef;
    fn IOServiceAddMatchingNotification(
        port: *mut c_void,
        notification_type: *const c_char,
        matching: CFDictionaryRef,
        callback: extern "C" fn(*mut c_void, IoObject),
        refcon: *mut c_void,
        iterator: *mut IoObject,
    ) -> i32;
    fn IOPSCopyPowerSourcesInfo() -> CFTypeRef;
    fn IOPSCopyPowerSourcesList(blob: CFTypeRef) -> CFArrayRef;
    fn IOPSGetPowerSourceDescription(blob: CFTypeRef, source: CFTypeRef) -> CFDictionaryRef;
    fn IOPSNotificationCreateRunLoopSource(
        callback: extern "C" fn(*mut c_void),
        context: *mut c_void,
    ) -> CFRunLoopSourceRef;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    static kCFRunLoopDefaultMode: CFStringRef;
    fn CFRunLoopGetMain() -> CFRunLoopRef;
    fn CFRunLoopAddSource(run_loop: CFRunLoopRef, source: CFRunLoopSourceRef, mode: CFStringRef);
    fn CFStringCreateWithCString(alloc: CFTypeRef, s: *const c_char, encoding: u32) -> CFStringRef;
    fn CFStringGetCString(s: CFStringRef, buffer: *mut c_char, size: isize, encoding: u32) -> u8;
    fn CFNumberGetValue(number: CFTypeRef, number_type: isize, out: *mut c_void) -> u8;
    fn CFGetTypeID(cf: CFTypeRef) -> usize;
    fn CFNumberGetTypeID() -> usize;
    fn CFStringGetTypeID() -> usize;
    fn CFBooleanGetTypeID() -> usize;
    fn CFBooleanGetValue(b: CFTypeRef) -> u8;
    fn CFDictionaryGetValue(dict: CFDictionaryRef, key: CFTypeRef) -> CFTypeRef;
    fn CFArrayGetCount(array: CFArrayRef) -> isize;
    fn CFArrayGetValueAtIndex(array: CFArrayRef, index: isize) -> CFTypeRef;
    fn CFRelease(cf: CFTypeRef);
}

const UTF8: u32 = 0x0800_0100;
const NUMBER_SINT64: isize = 4;
const HID_SERVICE: &CStr = c"AppleDeviceManagementHIDEventService";

/// A CFString for `s` (caller releases).
unsafe fn cfstr(s: &CStr) -> CFStringRef {
    CFStringCreateWithCString(std::ptr::null(), s.as_ptr(), UTF8)
}

unsafe fn string_value(v: CFTypeRef) -> Option<String> {
    if v.is_null() || CFGetTypeID(v) != CFStringGetTypeID() {
        return None;
    }
    let mut buf = [0 as c_char; 256];
    (CFStringGetCString(v, buf.as_mut_ptr(), buf.len() as isize, UTF8) != 0)
        .then(|| CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned())
}

unsafe fn int_value(v: CFTypeRef) -> Option<i64> {
    if v.is_null() {
        return None;
    }
    if CFGetTypeID(v) == CFBooleanGetTypeID() {
        return Some(CFBooleanGetValue(v) as i64);
    }
    if CFGetTypeID(v) != CFNumberGetTypeID() {
        return None;
    }
    let mut out: i64 = 0;
    (CFNumberGetValue(v, NUMBER_SINT64, (&mut out as *mut i64).cast()) != 0).then_some(out)
}

/// Read a registry property (owned copy) with `read`, then release it.
unsafe fn property<T>(entry: IoObject, key: &CStr, read: unsafe fn(CFTypeRef) -> Option<T>) -> Option<T> {
    let k = cfstr(key);
    let v = IORegistryEntryCreateCFProperty(entry, k, std::ptr::null(), 0);
    CFRelease(k);
    if v.is_null() {
        return None;
    }
    let out = read(v);
    CFRelease(v);
    out
}

unsafe fn entry_id(entry: IoObject) -> u64 {
    let mut id = 0u64;
    IORegistryEntryGetRegistryEntryID(entry, &mut id);
    id
}

/// A Bluetooth HID peripheral with a battery, from its registry entry.
unsafe fn peripheral(entry: IoObject) -> Option<BatteryDevice> {
    let percent = property(entry, c"BatteryPercent", int_value)?;
    let name = property(entry, c"Product", string_value).unwrap_or_else(|| "Accessory".into());
    let kind = match kind_from_name(&name) {
        DeviceKind::Headphones => DeviceKind::Other, // HID devices aren't headphones
        k => k,
    };
    Some(BatteryDevice {
        id: format!("io:{}", entry_id(entry)),
        name,
        kind,
        percent: percent.clamp(0, 100) as u8,
        charging: false,
        parts: vec![],
    })
}

fn current_peripherals() -> Vec<BatteryDevice> {
    let mut out = Vec::new();
    // SAFETY: standard IOKit enumeration; every object is released.
    unsafe {
        let mut iter: IoObject = 0;
        if IOServiceGetMatchingServices(0, IOServiceMatching(HID_SERVICE.as_ptr()), &mut iter) != 0 {
            return out;
        }
        loop {
            let entry = IOIteratorNext(iter);
            if entry == 0 {
                break;
            }
            out.extend(peripheral(entry));
            IOObjectRelease(entry);
        }
        IOObjectRelease(iter);
    }
    out
}

/// The laptop's internal battery, or None on a desktop Mac.
fn laptop() -> Option<BatteryDevice> {
    // SAFETY: IOPS blob/list are owned copies, released before returning;
    // descriptions and their values are borrowed from the blob.
    unsafe {
        let blob = IOPSCopyPowerSourcesInfo();
        if blob.is_null() {
            return None;
        }
        let list = IOPSCopyPowerSourcesList(blob);
        let mut found = None;
        if !list.is_null() {
            for i in 0..CFArrayGetCount(list) {
                let desc = IOPSGetPowerSourceDescription(blob, CFArrayGetValueAtIndex(list, i));
                if desc.is_null() {
                    continue;
                }
                let get = |key: &CStr| {
                    let k = cfstr(key);
                    let v = CFDictionaryGetValue(desc, k);
                    CFRelease(k);
                    v
                };
                if string_value(get(c"Type")).as_deref() != Some("InternalBattery") {
                    continue;
                }
                let current = int_value(get(c"Current Capacity")).unwrap_or(0);
                let max = int_value(get(c"Max Capacity")).unwrap_or(100).max(1);
                let on_ac = string_value(get(c"Power Source State")).as_deref() == Some("AC Power");
                found = Some(BatteryDevice {
                    id: "laptop".into(),
                    name: "This Mac".into(),
                    kind: DeviceKind::Laptop,
                    percent: ((current * 100) / max).clamp(0, 100) as u8,
                    charging: on_ac,
                    parts: vec![],
                });
                break;
            }
            CFRelease(list);
        }
        CFRelease(blob);
        found
    }
}

struct Context {
    sink: Sink,
    /// Connected peripherals by id, for change detection and the refresher.
    known: Arc<Mutex<HashMap<String, BatteryDevice>>>,
    /// False until the initial matching iteration is drained.
    armed: bool,
    /// The level refresher, woken when a device connects.
    refresher: std::thread::Thread,
}

extern "C" fn power_changed(ctx: *mut c_void) {
    // SAFETY: ctx is the leaked Context registered below; it lives forever.
    let ctx = unsafe { &*(ctx as *const Context) };
    if let Some(d) = laptop() {
        ctx.sink.send(Report::Laptop(d));
    }
}

extern "C" fn device_added(ctx: *mut c_void, iter: IoObject) {
    // SAFETY: ctx is the leaked Context; draining the iterator re-arms it.
    let ctx = unsafe { &mut *(ctx as *mut Context) };
    unsafe {
        loop {
            let entry = IOIteratorNext(iter);
            if entry == 0 {
                break;
            }
            if let Some(d) = peripheral(entry) {
                ctx.known.lock().unwrap().insert(d.id.clone(), d.clone());
                ctx.sink.send(if ctx.armed { Report::DeviceConnected(d) } else { Report::DeviceChanged(d) });
                ctx.refresher.unpark();
            }
            IOObjectRelease(entry);
        }
    }
}

extern "C" fn device_removed(ctx: *mut c_void, iter: IoObject) {
    // SAFETY: as above.
    let ctx = unsafe { &*(ctx as *const Context) };
    unsafe {
        loop {
            let entry = IOIteratorNext(iter);
            if entry == 0 {
                break;
            }
            let id = format!("io:{}", entry_id(entry));
            if ctx.known.lock().unwrap().remove(&id).is_some() {
                ctx.sink.send(Report::DeviceGone(id));
            }
            IOObjectRelease(entry);
        }
    }
}

/// Register the notifications on the main run loop (they fire while the
/// island's event loop runs) and start the slow level refresher.
pub fn start(sink: Sink) {
    let known: Arc<Mutex<HashMap<String, BatteryDevice>>> = Arc::new(Mutex::new(HashMap::new()));

    // IOKit doesn't notify level changes: re-read every 10 minutes, but only
    // while something is connected. With nothing connected the thread is
    // parked indefinitely (no wakeups); a connect unparks it.
    let refresher = {
        let (known, sink) = (known.clone(), sink.clone());
        std::thread::Builder::new().name("islet-battery-refresh".into()).spawn(move || loop {
            if known.lock().unwrap().is_empty() {
                std::thread::park();
                continue;
            }
            std::thread::park_timeout(Duration::from_secs(600));
            for d in current_peripherals() {
                let changed = known.lock().unwrap().get(&d.id).is_some_and(|old| old.percent != d.percent);
                if changed {
                    known.lock().unwrap().insert(d.id.clone(), d.clone());
                    sink.send(Report::DeviceChanged(d));
                }
            }
        })
    };
    let Ok(refresher) = refresher else { return };

    let ctx = Box::into_raw(Box::new(Context { sink, known, armed: false, refresher: refresher.thread().clone() }));
    // SAFETY: IOKit/CF registration with a context that is never freed.
    unsafe {
        let main = CFRunLoopGetMain();
        let power = IOPSNotificationCreateRunLoopSource(power_changed, ctx.cast());
        if !power.is_null() {
            CFRunLoopAddSource(main, power, kCFRunLoopDefaultMode);
        }
        power_changed(ctx.cast()); // baseline

        let port = IONotificationPortCreate(0);
        if port.is_null() {
            debug_log("IOKit notifications unavailable");
            return;
        }
        CFRunLoopAddSource(main, IONotificationPortGetRunLoopSource(port), kCFRunLoopDefaultMode);
        let mut added: IoObject = 0;
        let mut removed: IoObject = 0;
        // Each registration consumes its matching dictionary.
        IOServiceAddMatchingNotification(
            port,
            c"IOServiceFirstMatch".as_ptr(),
            IOServiceMatching(HID_SERVICE.as_ptr()),
            device_added,
            ctx.cast(),
            &mut added,
        );
        IOServiceAddMatchingNotification(
            port,
            c"IOServiceTerminate".as_ptr(),
            IOServiceMatching(HID_SERVICE.as_ptr()),
            device_removed,
            ctx.cast(),
            &mut removed,
        );
        // Drain once: existing devices are recorded without a pop-up, and
        // draining arms the notifications.
        device_added(ctx.cast(), added);
        device_removed(ctx.cast(), removed);
        (*ctx).armed = true;
    }
}
