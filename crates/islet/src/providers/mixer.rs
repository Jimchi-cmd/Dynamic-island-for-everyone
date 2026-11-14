//! The per-app volume mixer: the apps playing sound and their volumes,
//! read when the Mixer panel opens, and set when a slider is let go.
//! Linux: PulseAudio or PipeWire through `pactl`. Windows: Core Audio's
//! audio sessions. macOS has no public way to set one app's volume, so the
//! panel says so there.

use super::Sink;
use islet_core::activity::Report;
use islet_core::mixer::AppVolume;

/// Read the apps' volumes on another thread and report them.
pub fn read(sink: Sink) {
    std::thread::spawn(move || sink.send(Report::Mixer(platform::list())));
}

/// Set one app's volume (0..=1), then report the list as it now stands.
pub fn set(sink: Sink, id: String, level: f32) {
    std::thread::spawn(move || {
        platform::set(&id, level.clamp(0.0, 1.0));
        sink.send(Report::Mixer(platform::list()));
    });
}

#[cfg(target_os = "linux")]
mod platform {
    use super::AppVolume;
    use std::process::{Command, Stdio};

    fn pactl(args: &[&str]) -> Option<String> {
        let out = Command::new("pactl")
            .args(args)
            // Untranslated, so it can be read.
            .env("LC_ALL", "C")
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()?;
        out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
    }

    pub fn list() -> Option<Vec<AppVolume>> {
        Some(islet_core::mixer::parse_sink_inputs(&pactl(&["list", "sink-inputs"])?))
    }

    pub fn set(id: &str, level: f32) {
        if id.chars().all(|c| c.is_ascii_digit()) {
            let pct = format!("{}%", (level * 100.0).round() as u32);
            let _ = pactl(&["set-sink-input-volume", id, &pct]);
        }
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use super::AppVolume;
    use windows::core::Interface;
    use windows::Win32::Media::Audio::{
        eConsole, eRender, IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator, ISimpleAudioVolume,
        MMDeviceEnumerator,
    };
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};

    /// Each session playing on the default output, with its volume control.
    fn sessions() -> windows::core::Result<Vec<(u32, String, ISimpleAudioVolume)>> {
        // SAFETY: COM calls on this thread after CoInitializeEx.
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let devices: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            let device = devices.GetDefaultAudioEndpoint(eRender, eConsole)?;
            let manager: IAudioSessionManager2 = device.Activate(CLSCTX_ALL, None)?;
            let list = manager.GetSessionEnumerator()?;
            let mut out = Vec::new();
            for i in 0..list.GetCount()? {
                let Ok(control) = list.GetSession(i) else { continue };
                let Ok(control2) = control.cast::<IAudioSessionControl2>() else { continue };
                let Ok(pid) = control2.GetProcessId() else { continue };
                // The system's own sounds (pid 0) aren't an app.
                if pid == 0 {
                    continue;
                }
                let Ok(volume) = control.cast::<ISimpleAudioVolume>() else { continue };
                let name = crate::switcher::process_name(pid).unwrap_or_else(|| format!("App {pid}"));
                out.push((pid, name, volume));
            }
            Ok(out)
        }
    }

    pub fn list() -> Option<Vec<AppVolume>> {
        let sessions = sessions().ok()?;
        Some(
            sessions
                .into_iter()
                // SAFETY: live session interfaces.
                .map(|(pid, name, v)| unsafe {
                    AppVolume {
                        id: pid.to_string(),
                        name,
                        level: v.GetMasterVolume().unwrap_or(1.0),
                        muted: v.GetMute().map(|m| m.as_bool()).unwrap_or(false),
                    }
                })
                .collect(),
        )
    }

    pub fn set(id: &str, level: f32) {
        let Ok(pid) = id.parse::<u32>() else { return };
        let Ok(sessions) = sessions() else { return };
        for (_, _, v) in sessions.iter().filter(|(p, _, _)| *p == pid) {
            // SAFETY: a live session interface; a null event context.
            let _ = unsafe { v.SetMasterVolume(level, std::ptr::null()) };
        }
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::AppVolume;

    pub fn list() -> Option<Vec<AppVolume>> {
        None
    }

    pub fn set(_id: &str, _level: f32) {}
}
