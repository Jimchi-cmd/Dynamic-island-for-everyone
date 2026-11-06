//! Output volume on Windows: Core Audio calls us back when the default
//! output device's volume or mute changes. No polling.

use super::{debug_log, Sink};
use islet_core::activity::{Hud, Report};
use std::sync::Mutex;
use windows::core::implement;
use windows::Win32::Media::Audio::Endpoints::{
    IAudioEndpointVolume, IAudioEndpointVolumeCallback, IAudioEndpointVolumeCallback_Impl,
};
use windows::Win32::Media::Audio::{
    eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator, AUDIO_VOLUME_NOTIFICATION_DATA,
};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};

#[implement(IAudioEndpointVolumeCallback)]
struct Callback {
    sink: Sink,
    last: Mutex<(f32, bool)>,
}

impl IAudioEndpointVolumeCallback_Impl for Callback_Impl {
    fn OnNotify(&self, data: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> windows::core::Result<()> {
        // SAFETY: Core Audio passes valid notification data for the call.
        let Some(d) = (unsafe { data.as_ref() }) else { return Ok(()) };
        let now = (d.fMasterVolume, d.bMuted.as_bool());
        let mut last = self.last.lock().unwrap();
        if (now.0 - last.0).abs() > 0.001 || now.1 != last.1 {
            *last = now;
            self.sink.send(Report::Hud(Hud::Volume { level: now.0, muted: now.1 }));
        }
        Ok(())
    }
}

fn run(sink: Sink) -> windows::core::Result<()> {
    // SAFETY: COM calls on this thread after CoInitializeEx; interfaces are
    // reference-counted and kept alive below for the process lifetime.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let devices: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let device = devices.GetDefaultAudioEndpoint(eRender, eConsole)?;
        let volume: IAudioEndpointVolume = device.Activate(CLSCTX_ALL, None)?;
        let start = (volume.GetMasterVolumeLevelScalar()?, volume.GetMute()?.as_bool());
        sink.send(Report::Volume(start.0, start.1));
        let callback: IAudioEndpointVolumeCallback = Callback { sink, last: Mutex::new(start) }.into();
        volume.RegisterControlChangeNotify(&callback)?;
        // Callbacks arrive on Core Audio's threads; keep everything alive.
        loop {
            std::thread::park();
            let _ = (&volume, &callback);
        }
    }
}

/// The slider's requests, handled on one COM thread; a drag sends many, so
/// only the newest waiting one is applied.
static SETTER: std::sync::OnceLock<Mutex<std::sync::mpsc::Sender<f32>>> = std::sync::OnceLock::new();

/// Set the default output's volume (the Now Playing card's slider).
pub fn set(level: f32) {
    let tx = SETTER.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<f32>();
        let _ = std::thread::Builder::new().name("islet-volume-set".into()).spawn(move || {
            // SAFETY: COM on this thread after CoInitializeEx.
            let volume: Option<IAudioEndpointVolume> = unsafe {
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
                CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL)
                    .and_then(|d| d.GetDefaultAudioEndpoint(eRender, eConsole))
                    .and_then(|d| d.Activate(CLSCTX_ALL, None))
                    .ok()
            };
            while let Ok(mut level) = rx.recv() {
                while let Ok(newer) = rx.try_recv() {
                    level = newer;
                }
                if let Some(v) = &volume {
                    // SAFETY: a live endpoint interface; a null event context.
                    let _ = unsafe { v.SetMasterVolumeLevelScalar(level, std::ptr::null()) };
                }
            }
        });
        Mutex::new(tx)
    });
    let _ = tx.lock().unwrap().send(level);
}

pub fn start(sink: Sink) {
    let _ = std::thread::Builder::new().name("islet-volume".into()).spawn(move || {
        if let Err(e) = run(sink) {
            debug_log(&format!("volume pop-ups unavailable: {}", e.message()));
        }
    });
}
