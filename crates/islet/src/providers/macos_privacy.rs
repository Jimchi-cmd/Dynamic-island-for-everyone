//! Camera and microphone in-use dots on macOS, event-driven.
//!
//! CoreAudio and CoreMediaIO report, for each device, whether any process
//! is running it ("is running somewhere"). Islet listens for changes to
//! that property on every audio input and every camera, and to the device
//! lists (for devices plugged in later). macOS has no public way to say
//! *which* app, so the dot carries no name.

use super::macos_volume::{Address, AudioObjectAddPropertyListener, AudioObjectGetPropertyData, Listener};
use super::Sink;
use islet_core::activity::{Privacy, Report};
use std::ffi::c_void;
use std::sync::Mutex;

type ObjectId = u32;
type OsStatus = i32;

#[link(name = "CoreAudio", kind = "framework")]
extern "C" {
    fn AudioObjectGetPropertyDataSize(
        id: ObjectId,
        addr: *const Address,
        qual_size: u32,
        qual: *const c_void,
        size: *mut u32,
    ) -> OsStatus;
    fn AudioObjectSetPropertyData(
        id: ObjectId,
        addr: *const Address,
        qual_size: u32,
        qual: *const c_void,
        size: u32,
        data: *const c_void,
    ) -> OsStatus;
}

#[link(name = "CoreMediaIO", kind = "framework")]
extern "C" {
    fn CMIOObjectGetPropertyDataSize(
        id: ObjectId,
        addr: *const Address,
        qual_size: u32,
        qual: *const c_void,
        size: *mut u32,
    ) -> OsStatus;
    fn CMIOObjectGetPropertyData(
        id: ObjectId,
        addr: *const Address,
        qual_size: u32,
        qual: *const c_void,
        size: u32,
        used: *mut u32,
        data: *mut c_void,
    ) -> OsStatus;
    fn CMIOObjectSetPropertyData(
        id: ObjectId,
        addr: *const Address,
        qual_size: u32,
        qual: *const c_void,
        size: u32,
        data: *const c_void,
    ) -> OsStatus;
    fn CMIOObjectAddPropertyListener(id: ObjectId, addr: *const Address, f: Listener, ctx: *mut c_void) -> OsStatus;
}

const fn fourcc(s: &[u8; 4]) -> u32 {
    u32::from_be_bytes(*s)
}

const SYSTEM: ObjectId = 1;
const GLOBAL: u32 = fourcc(b"glob");
const INPUT: u32 = fourcc(b"inpt");
const MAIN: u32 = 0;
const DEVICES: u32 = fourcc(b"dev#");
const RUN_LOOP: u32 = fourcc(b"rnlp");
const RUNNING_SOMEWHERE: u32 = fourcc(b"gone");
const STREAMS: u32 = fourcc(b"stm#");

const fn addr(selector: u32, scope: u32) -> Address {
    Address { selector, scope, element: MAIN }
}

#[derive(Clone, Copy, PartialEq)]
enum Api {
    Audio,
    Camera,
}

impl Api {
    fn size(self, id: ObjectId, a: &Address) -> u32 {
        let mut size = 0u32;
        // SAFETY: valid address struct and out pointer.
        let r = unsafe {
            match self {
                Api::Audio => AudioObjectGetPropertyDataSize(id, a, 0, std::ptr::null(), &mut size),
                Api::Camera => CMIOObjectGetPropertyDataSize(id, a, 0, std::ptr::null(), &mut size),
            }
        };
        if r == 0 {
            size
        } else {
            0
        }
    }

    fn read<T: Copy + Default>(self, id: ObjectId, a: &Address, n: usize) -> Vec<T> {
        let mut v = vec![T::default(); n];
        let bytes = (n * std::mem::size_of::<T>()) as u32;
        let mut size = bytes;
        // SAFETY: the buffer holds `bytes` bytes.
        let r = unsafe {
            match self {
                Api::Audio => AudioObjectGetPropertyData(id, a, 0, std::ptr::null(), &mut size, v.as_mut_ptr().cast()),
                Api::Camera => {
                    CMIOObjectGetPropertyData(id, a, 0, std::ptr::null(), bytes, &mut size, v.as_mut_ptr().cast())
                }
            }
        };
        if r != 0 {
            return Vec::new();
        }
        v.truncate(size as usize / std::mem::size_of::<T>());
        v
    }

    fn devices(self) -> Vec<ObjectId> {
        let a = addr(DEVICES, GLOBAL);
        let n = self.size(SYSTEM, &a) as usize / std::mem::size_of::<ObjectId>();
        let all = self.read::<ObjectId>(SYSTEM, &a, n);
        match self {
            // Only audio devices with an input (microphones).
            Api::Audio => all.into_iter().filter(|&d| self.size(d, &addr(STREAMS, INPUT)) > 0).collect(),
            Api::Camera => all,
        }
    }

    fn running(self, id: ObjectId) -> bool {
        self.read::<u32>(id, &addr(RUNNING_SOMEWHERE, GLOBAL), 1).first().is_some_and(|&v| v != 0)
    }

    fn listen(self, id: ObjectId, selector: u32) {
        let a = addr(selector, GLOBAL);
        // SAFETY: `changed` is a plain extern fn; no context pointer.
        unsafe {
            match self {
                Api::Audio => AudioObjectAddPropertyListener(id, &a, changed, std::ptr::null_mut()),
                Api::Camera => CMIOObjectAddPropertyListener(id, &a, changed, std::ptr::null_mut()),
            };
        }
    }
}

struct State {
    sink: Sink,
    listening: Vec<(Api, ObjectId)>,
    last: Privacy,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

/// Called by CoreAudio / CoreMediaIO on their own notification threads.
unsafe extern "C" fn changed(_id: ObjectId, _n: u32, _addrs: *const Address, _ctx: *mut c_void) -> OsStatus {
    refresh();
    0
}

/// Listen to any new device, then report if the picture changed.
fn refresh() {
    let Ok(mut guard) = STATE.lock() else { return };
    let Some(st) = guard.as_mut() else { return };
    let mut any = [false; 2];
    for (i, api) in [Api::Camera, Api::Audio].into_iter().enumerate() {
        for d in api.devices() {
            if !st.listening.contains(&(api, d)) {
                api.listen(d, RUNNING_SOMEWHERE);
                st.listening.push((api, d));
            }
            any[i] |= api.running(d);
        }
    }
    let next = Privacy { camera: any[0].then(Vec::new), mic: any[1].then(Vec::new) };
    if next != st.last {
        st.last = next.clone();
        st.sink.send(Report::Privacy(next));
    }
}

pub fn start(sink: Sink) {
    let _ = std::thread::Builder::new().name("privacy".into()).spawn(move || {
        // Deliver notifications on the frameworks' own threads rather than
        // the main run loop (which belongs to the island's window).
        let null: *const c_void = std::ptr::null();
        let size = std::mem::size_of::<*const c_void>() as u32;
        let rl = addr(RUN_LOOP, GLOBAL);
        // SAFETY: setting the documented run-loop property to NULL.
        unsafe {
            AudioObjectSetPropertyData(SYSTEM, &rl, 0, std::ptr::null(), size, (&null as *const *const c_void).cast());
            CMIOObjectSetPropertyData(SYSTEM, &rl, 0, std::ptr::null(), size, (&null as *const *const c_void).cast());
        }
        if let Ok(mut g) = STATE.lock() {
            *g = Some(State { sink, listening: Vec::new(), last: Privacy::default() });
        }
        Api::Audio.listen(SYSTEM, DEVICES);
        Api::Camera.listen(SYSTEM, DEVICES);
        refresh();
    });
}
