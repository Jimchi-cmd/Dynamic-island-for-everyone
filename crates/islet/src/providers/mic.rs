//! The microphone's mute switch, for the call card: read when a call
//! starts, set from its mute button. A helper thread sleeps on its channel
//! until asked (no wakeups otherwise) and reports the switch as it is after
//! each request.
//! - Linux: the default source through `pactl`;
//! - Windows: the default capture device's endpoint volume (Core Audio);
//! - macOS: the default input device's mute property (CoreAudio).

use super::{debug_log, Sink};
use islet_core::activity::{MicCommand, Report};
use std::sync::mpsc::{channel, Sender};

pub fn start(sink: Sink) -> Sender<MicCommand> {
    let (tx, rx) = channel::<MicCommand>();
    let _ = std::thread::Builder::new().name("islet-mic".into()).spawn(move || {
        let mut mic = Mic::new();
        while let Ok(cmd) = rx.recv() {
            if let MicCommand::Set(muted) = cmd {
                if let Err(e) = mic.set(muted) {
                    debug_log(&format!("microphone: {e}"));
                }
            }
            match mic.muted() {
                Some(m) => sink.send(Report::MicMuted(m)),
                None => debug_log("microphone: can't read its mute switch"),
            }
        }
    });
    tx
}

// ---- Linux ---------------------------------------------------------------------

#[cfg(target_os = "linux")]
struct Mic;

#[cfg(target_os = "linux")]
impl Mic {
    fn new() -> Self {
        Mic
    }

    fn pactl(args: &[&str]) -> Option<String> {
        let out = crate::hidden::command("pactl").args(args).stderr(std::process::Stdio::null()).output().ok()?;
        out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
    }

    fn muted(&mut self) -> Option<bool> {
        super::linux_volume::parse_mute(&Self::pactl(&["get-source-mute", "@DEFAULT_SOURCE@"])?)
    }

    fn set(&mut self, muted: bool) -> Result<(), String> {
        Self::pactl(&["set-source-mute", "@DEFAULT_SOURCE@", if muted { "1" } else { "0" }])
            .map(|_| ())
            .ok_or_else(|| "pactl couldn't change the microphone".into())
    }
}

// ---- Windows -------------------------------------------------------------------

#[cfg(target_os = "windows")]
struct Mic {
    endpoint: Option<windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume>,
}

#[cfg(target_os = "windows")]
impl Mic {
    fn new() -> Self {
        use windows::Win32::Media::Audio::{eCapture, eCommunications, IMMDeviceEnumerator, MMDeviceEnumerator};
        use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};
        // SAFETY: COM on this thread after CoInitializeEx; the interface is
        // reference-counted and kept for the thread's lifetime.
        let endpoint = unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .and_then(|d| d.GetDefaultAudioEndpoint(eCapture, eCommunications))
                .and_then(|d| d.Activate(CLSCTX_ALL, None))
                .ok()
        };
        Mic { endpoint }
    }

    fn muted(&mut self) -> Option<bool> {
        // SAFETY: a live endpoint interface.
        self.endpoint.as_ref().and_then(|e| unsafe { e.GetMute() }.ok()).map(|b| b.as_bool())
    }

    fn set(&mut self, muted: bool) -> Result<(), String> {
        let e = self.endpoint.as_ref().ok_or("no microphone")?;
        // SAFETY: a live endpoint interface; a null event context.
        unsafe { e.SetMute(muted, std::ptr::null()) }.map_err(|e| e.message())
    }
}

// ---- macOS ---------------------------------------------------------------------

#[cfg(target_os = "macos")]
struct Mic;

#[cfg(target_os = "macos")]
impl Mic {
    const SYSTEM: u32 = 1;
    const DEFAULT_INPUT: u32 = u32::from_be_bytes(*b"dIn ");
    const GLOBAL: u32 = u32::from_be_bytes(*b"glob");
    const INPUT: u32 = u32::from_be_bytes(*b"inpt");
    const MUTE: u32 = u32::from_be_bytes(*b"mute");

    fn new() -> Self {
        Mic
    }

    fn device() -> Option<u32> {
        Self::get(Self::SYSTEM, Self::DEFAULT_INPUT, Self::GLOBAL)
    }

    fn get(object: u32, selector: u32, scope: u32) -> Option<u32> {
        use super::macos_volume::{Address, AudioObjectGetPropertyData};
        let address = Address { selector, scope, element: 0 };
        let (mut value, mut size) = (0u32, std::mem::size_of::<u32>() as u32);
        // SAFETY: a u32 buffer for a u32 property.
        let status = unsafe {
            AudioObjectGetPropertyData(
                object,
                &address,
                0,
                std::ptr::null(),
                &mut size,
                (&mut value as *mut u32).cast(),
            )
        };
        (status == 0).then_some(value)
    }

    fn muted(&mut self) -> Option<bool> {
        Self::get(Self::device()?, Self::MUTE, Self::INPUT).map(|v| v != 0)
    }

    fn set(&mut self, muted: bool) -> Result<(), String> {
        use super::macos_volume::{Address, AudioObjectSetPropertyData};
        let device = Self::device().ok_or("no microphone")?;
        let address = Address { selector: Self::MUTE, scope: Self::INPUT, element: 0 };
        let value = u32::from(muted);
        // SAFETY: a u32 for the input mute property.
        let status = unsafe {
            AudioObjectSetPropertyData(
                device,
                &address,
                0,
                std::ptr::null(),
                std::mem::size_of::<u32>() as u32,
                (&value as *const u32).cast(),
            )
        };
        if status == 0 {
            Ok(())
        } else {
            Err("this microphone has no mute switch".into())
        }
    }
}
