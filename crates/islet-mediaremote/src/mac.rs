//! The MediaRemote side of the helper (runs inside /usr/bin/perl).

use crate::wire::{base64_encode, escape};
use block2::RcBlock;
use objc2_foundation::{NSNotification, NSNotificationCenter, NSOperationQueue, NSString};
use std::ffi::{c_char, c_void, CStr, CString};
use std::io::{BufRead, Write};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

type CFTypeRef = *const c_void;
type QueueFn = unsafe extern "C" fn(queue: *const c_void, block: *const c_void);

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFDictionaryGetValue(dict: CFTypeRef, key: CFTypeRef) -> CFTypeRef;
    fn CFGetTypeID(cf: CFTypeRef) -> usize;
    fn CFStringGetTypeID() -> usize;
    fn CFNumberGetTypeID() -> usize;
    fn CFDataGetTypeID() -> usize;
    fn CFDateGetTypeID() -> usize;
    fn CFStringGetLength(s: CFTypeRef) -> isize;
    fn CFStringGetMaximumSizeForEncoding(length: isize, encoding: u32) -> isize;
    fn CFStringGetCString(s: CFTypeRef, buffer: *mut c_char, size: isize, encoding: u32) -> u8;
    fn CFNumberGetValue(number: CFTypeRef, number_type: isize, out: *mut c_void) -> u8;
    fn CFDataGetLength(data: CFTypeRef) -> isize;
    fn CFDataGetBytePtr(data: CFTypeRef) -> *const u8;
    fn CFDateGetAbsoluteTime(date: CFTypeRef) -> f64;
    fn CFAbsoluteTimeGetCurrent() -> f64;
    fn CFRunLoopRun();
}

const UTF8: u32 = 0x0800_0100;
const NUMBER_FLOAT64: isize = 6;
const FRAMEWORK: &CStr = c"/System/Library/PrivateFrameworks/MediaRemote.framework/MediaRemote";

/// MRMediaRemoteCommand values.
const CMD_PLAY: u32 = 0;
const CMD_PAUSE: u32 = 1;
const CMD_TOGGLE: u32 = 2;
const CMD_NEXT: u32 = 4;
const CMD_PREVIOUS: u32 = 5;

struct Keys {
    title: CFTypeRef,
    artist: CFTypeRef,
    album: CFTypeRef,
    duration: CFTypeRef,
    elapsed: CFTypeRef,
    rate: CFTypeRef,
    timestamp: CFTypeRef,
    artwork: CFTypeRef,
    artwork_mime: CFTypeRef,
}

struct Api {
    get_info: QueueFn,
    get_is_playing: Option<QueueFn>,
    get_client: Option<QueueFn>,
    client_bundle: Option<unsafe extern "C" fn(client: CFTypeRef) -> CFTypeRef>,
    register: unsafe extern "C" fn(queue: *const c_void),
    send_command: unsafe extern "C" fn(command: u32, options: CFTypeRef) -> u8,
    set_elapsed: Option<unsafe extern "C" fn(seconds: f64)>,
    keys: Keys,
    notifications: Vec<CFTypeRef>,
}

// SAFETY: the API holds function pointers and immortal CFString constants
// from a framework that is never unloaded.
unsafe impl Send for Api {}
unsafe impl Sync for Api {}

static API: OnceLock<Api> = OnceLock::new();
static LAST_ART: AtomicU64 = AtomicU64::new(0);

unsafe fn sym(handle: *mut c_void, name: &str) -> *mut c_void {
    let c = CString::new(name).expect("symbol name");
    libc::dlsym(handle, c.as_ptr())
}

/// The value of an exported `CFStringRef` constant.
unsafe fn constant(handle: *mut c_void, name: &str) -> CFTypeRef {
    let p = sym(handle, name) as *const CFTypeRef;
    if p.is_null() {
        std::ptr::null()
    } else {
        *p
    }
}

fn load() -> Result<Api, String> {
    // SAFETY: dlopen/dlsym on the system framework; pointers are checked.
    unsafe {
        let h = libc::dlopen(FRAMEWORK.as_ptr(), libc::RTLD_NOW);
        if h.is_null() {
            return Err("MediaRemote.framework could not be loaded".into());
        }
        let required = |name: &str| {
            let p = sym(h, name);
            if p.is_null() {
                Err(format!("missing {name}"))
            } else {
                Ok(p)
            }
        };
        let optional = |name: &str| Some(sym(h, name)).filter(|p| !p.is_null());
        let keys = Keys {
            title: constant(h, "kMRMediaRemoteNowPlayingInfoTitle"),
            artist: constant(h, "kMRMediaRemoteNowPlayingInfoArtist"),
            album: constant(h, "kMRMediaRemoteNowPlayingInfoAlbum"),
            duration: constant(h, "kMRMediaRemoteNowPlayingInfoDuration"),
            elapsed: constant(h, "kMRMediaRemoteNowPlayingInfoElapsedTime"),
            rate: constant(h, "kMRMediaRemoteNowPlayingInfoPlaybackRate"),
            timestamp: constant(h, "kMRMediaRemoteNowPlayingInfoTimestamp"),
            artwork: constant(h, "kMRMediaRemoteNowPlayingInfoArtworkData"),
            artwork_mime: constant(h, "kMRMediaRemoteNowPlayingInfoArtworkMIMEType"),
        };
        if keys.title.is_null() {
            return Err("MediaRemote keys unavailable".into());
        }
        let notifications = [
            "kMRMediaRemoteNowPlayingInfoDidChangeNotification",
            "kMRMediaRemoteNowPlayingApplicationIsPlayingDidChangeNotification",
            "kMRMediaRemoteNowPlayingApplicationDidChangeNotification",
        ]
        .iter()
        .map(|n| constant(h, n))
        .filter(|p| !p.is_null())
        .collect();
        Ok(Api {
            get_info: std::mem::transmute::<*mut c_void, QueueFn>(required("MRMediaRemoteGetNowPlayingInfo")?),
            get_is_playing: optional("MRMediaRemoteGetNowPlayingApplicationIsPlaying")
                .map(|p| std::mem::transmute::<*mut c_void, QueueFn>(p)),
            get_client: optional("MRMediaRemoteGetNowPlayingClient")
                .map(|p| std::mem::transmute::<*mut c_void, QueueFn>(p)),
            client_bundle: optional("MRNowPlayingClientGetBundleIdentifier")
                .map(|p| std::mem::transmute::<*mut c_void, unsafe extern "C" fn(CFTypeRef) -> CFTypeRef>(p)),
            register: std::mem::transmute::<*mut c_void, unsafe extern "C" fn(*const c_void)>(required(
                "MRMediaRemoteRegisterForNowPlayingNotifications",
            )?),
            send_command: std::mem::transmute::<*mut c_void, unsafe extern "C" fn(u32, CFTypeRef) -> u8>(required(
                "MRMediaRemoteSendCommand",
            )?),
            set_elapsed: optional("MRMediaRemoteSetElapsedTime")
                .map(|p| std::mem::transmute::<*mut c_void, unsafe extern "C" fn(f64)>(p)),
            keys,
            notifications,
        })
    }
}

fn main_queue() -> *const c_void {
    dispatch2::DispatchQueue::main() as *const dispatch2::DispatchQueue as *const c_void
}

unsafe fn cf_string(v: CFTypeRef) -> Option<String> {
    if v.is_null() || CFGetTypeID(v) != CFStringGetTypeID() {
        return None;
    }
    let cap = CFStringGetMaximumSizeForEncoding(CFStringGetLength(v), UTF8) + 1;
    let mut buf = vec![0 as c_char; cap.max(1) as usize];
    (CFStringGetCString(v, buf.as_mut_ptr(), cap, UTF8) != 0)
        .then(|| CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned())
}

unsafe fn cf_f64(v: CFTypeRef) -> Option<f64> {
    if v.is_null() || CFGetTypeID(v) != CFNumberGetTypeID() {
        return None;
    }
    let mut out = 0f64;
    (CFNumberGetValue(v, NUMBER_FLOAT64, (&mut out as *mut f64).cast()) != 0).then_some(out)
}

unsafe fn cf_bytes(v: CFTypeRef) -> Option<&'static [u8]> {
    if v.is_null() || CFGetTypeID(v) != CFDataGetTypeID() {
        return None;
    }
    let len = CFDataGetLength(v);
    let ptr = CFDataGetBytePtr(v);
    (len > 0 && !ptr.is_null()).then(|| std::slice::from_raw_parts(ptr, len as usize))
}

fn write_line(line: &str) {
    let mut out = std::io::stdout().lock();
    if writeln!(out, "{line}").and_then(|_| out.flush()).is_err() {
        std::process::exit(0); // Islet went away
    }
}

fn fnv(data: &[u8]) -> u64 {
    data.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, &b| (h ^ b as u64).wrapping_mul(0x100_0000_01b3))
}

/// Print one Now Playing record from MediaRemote's info dictionary.
unsafe fn emit(api: &Api, info: CFTypeRef, playing: bool, bundle: Option<String>) {
    if info.is_null() {
        write_line("N");
        return;
    }
    let k = &api.keys;
    let get = |key: CFTypeRef| if key.is_null() { std::ptr::null() } else { CFDictionaryGetValue(info, key) };
    let mut line = String::from("R");
    let mut field = |key: &str, value: &str| {
        line.push('\t');
        line.push_str(key);
        line.push('=');
        line.push_str(&escape(value));
    };
    for (name, key) in [("title", k.title), ("artist", k.artist), ("album", k.album)] {
        if let Some(s) = cf_string(get(key)) {
            field(name, &s);
        }
    }
    let rate = cf_f64(get(k.rate)).unwrap_or(if playing { 1.0 } else { 0.0 });
    // The elapsed time is as of the info's timestamp; bring it up to now.
    let mut elapsed = cf_f64(get(k.elapsed));
    if let (Some(e), true) = (elapsed.as_mut(), playing && rate > 0.0) {
        let ts = get(k.timestamp);
        if !ts.is_null() && CFGetTypeID(ts) == CFDateGetTypeID() {
            *e += (CFAbsoluteTimeGetCurrent() - CFDateGetAbsoluteTime(ts)).max(0.0) * rate;
        }
    }
    field("playing", if playing { "1" } else { "0" });
    field("rate", &format!("{rate}"));
    if let Some(e) = elapsed {
        field("elapsed", &format!("{e:.3}"));
    }
    if let Some(d) = cf_f64(get(k.duration)) {
        field("duration", &format!("{d:.3}"));
    }
    if let Some(b) = bundle {
        field("bundle", &b);
    }
    if let Some(art) = cf_bytes(get(k.artwork)) {
        // Send the artwork only when it changes; Islet caches it by id.
        let id = fnv(art);
        field("art_id", &format!("{id:x}"));
        if LAST_ART.swap(id, Ordering::Relaxed) != id {
            field("art", &base64_encode(art));
            if let Some(mime) = cf_string(get(k.artwork_mime)) {
                field("art_mime", &mime);
            }
        }
    }
    write_line(&line);
}

/// Ask MediaRemote for the current state: playing? → which app? → info.
fn refresh() {
    let Some(api) = API.get() else { return };
    let after_playing = move |playing: bool| {
        let after_client = move |bundle: Option<String>| {
            let info_block = RcBlock::new(move |info: CFTypeRef| {
                // SAFETY: MediaRemote passes a dictionary valid for the call.
                unsafe { emit(api, info, playing, bundle.clone()) }
            });
            // SAFETY: valid queue and block; MediaRemote copies the block.
            unsafe { (api.get_info)(main_queue(), RcBlock::as_ptr(&info_block).cast()) };
        };
        match (api.get_client, api.client_bundle) {
            (Some(get_client), Some(bundle_of)) => {
                let block = RcBlock::new(move |client: CFTypeRef| {
                    // SAFETY: the client object is valid for the call.
                    let bundle = if client.is_null() { None } else { unsafe { cf_string(bundle_of(client)) } };
                    after_client(bundle);
                });
                // SAFETY: as above.
                unsafe { get_client(main_queue(), RcBlock::as_ptr(&block).cast()) };
            }
            _ => after_client(None),
        }
    };
    match api.get_is_playing {
        Some(get_is_playing) => {
            let block = RcBlock::new(move |playing: u8| after_playing(playing != 0));
            // SAFETY: as above.
            unsafe { get_is_playing(main_queue(), RcBlock::as_ptr(&block).cast()) };
        }
        None => after_playing(true),
    }
}

fn command(api: &Api, line: &str) {
    let mut parts = line.split_whitespace();
    // SAFETY: MediaRemote's command functions are thread-safe C calls.
    unsafe {
        match parts.next() {
            Some("toggle") => _ = (api.send_command)(CMD_TOGGLE, std::ptr::null()),
            Some("play") => _ = (api.send_command)(CMD_PLAY, std::ptr::null()),
            Some("pause") => _ = (api.send_command)(CMD_PAUSE, std::ptr::null()),
            Some("next") => _ = (api.send_command)(CMD_NEXT, std::ptr::null()),
            Some("previous") => _ = (api.send_command)(CMD_PREVIOUS, std::ptr::null()),
            Some("seek") => {
                if let (Some(set), Some(secs)) = (api.set_elapsed, parts.next().and_then(|s| s.parse::<f64>().ok())) {
                    set(secs.max(0.0));
                }
            }
            _ => {}
        }
    }
}

/// Exit the helper. Declared as returning `()` (not `!`) so it can end a
/// block whose return type must be a real Objective-C type.
fn exit_ok() {
    std::process::exit(0);
}

pub fn run() {
    let api = match load() {
        Ok(api) => API.get_or_init(|| api),
        Err(e) => {
            write_line(&format!("E\t{}", escape(&e)));
            std::process::exit(3);
        }
    };
    let probe = std::env::var("ISLET_MR_MODE").as_deref() == Ok("probe");
    if probe {
        // Report that MediaRemote loaded and answered (or didn't) and exit.
        std::thread::spawn(|| {
            std::thread::sleep(std::time::Duration::from_secs(5));
            write_line("P\ttimeout");
            std::process::exit(0);
        });
        let block = RcBlock::new(|info: CFTypeRef| {
            write_line(&format!("P\tok\tinfo={}", u8::from(!info.is_null())));
            exit_ok();
        });
        // SAFETY: valid queue and block.
        unsafe { (api.get_info)(main_queue(), RcBlock::as_ptr(&block).cast()) };
    } else {
        // Commands arrive on stdin; EOF means Islet quit.
        std::thread::spawn(move || {
            for line in std::io::stdin().lock().lines() {
                match line {
                    Ok(l) => command(api, &l),
                    Err(_) => break,
                }
            }
            std::process::exit(0);
        });
        // SAFETY: registering for notifications on the main queue.
        unsafe { (api.register)(main_queue()) };
        let center = NSNotificationCenter::defaultCenter();
        let queue = NSOperationQueue::mainQueue();
        for &name in &api.notifications {
            let block = RcBlock::new(|_: NonNull<NSNotification>| refresh());
            // SAFETY: CFStringRef constants are toll-free bridged NSStrings;
            // observers stay registered for the helper's lifetime.
            unsafe {
                let name = &*(name as *const NSString);
                let observer =
                    center.addObserverForName_object_queue_usingBlock(Some(name), None, Some(&queue), &block);
                std::mem::forget(observer);
            }
        }
        refresh();
    }
    // SAFETY: runs the main run loop forever; the helper exits via the
    // stdin thread, a failed write, or the probe paths above.
    unsafe { CFRunLoopRun() };
    std::process::exit(0);
}
