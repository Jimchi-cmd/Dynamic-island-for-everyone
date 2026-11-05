//! The alarm sound: an original two-note chime that Islet synthesises
//! itself (no bundled or system sound files), played with the OS's own
//! player: PlaySound on Windows, `afplay` on macOS, and PipeWire /
//! PulseAudio / ALSA tools on Linux.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
#[cfg(not(windows))]
use std::sync::atomic::Ordering;
use std::sync::Arc;

const RATE: u32 = 22_050;

/// One 1.6 s cycle: two bright notes twice, then a pause (16-bit mono WAV).
pub fn chime_wav() -> Vec<u8> {
    let notes = [(0.00, 880.0), (0.20, 1318.5), (0.45, 880.0), (0.65, 1318.5)];
    let total = (1.6 * RATE as f64) as usize;
    let mut samples = vec![0f64; total];
    for (start, freq) in notes {
        let s0 = (start * RATE as f64) as usize;
        for i in 0..(0.35 * RATE as f64) as usize {
            let t = i as f64 / RATE as f64;
            // Fast attack, exponential decay, a soft octave overtone.
            let env = (t / 0.004).min(1.0) * (-t * 9.0).exp();
            let tau = std::f64::consts::TAU;
            let v = (tau * freq * t).sin() + 0.25 * (tau * 2.0 * freq * t).sin();
            if let Some(s) = samples.get_mut(s0 + i) {
                *s += 0.45 * env * v;
            }
        }
    }
    let data: Vec<u8> =
        samples.iter().flat_map(|s| ((s.clamp(-1.0, 1.0) * i16::MAX as f64) as i16).to_le_bytes()).collect();
    let mut wav = Vec::with_capacity(44 + data.len());
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&1u16.to_le_bytes()); // mono
    wav.extend_from_slice(&RATE.to_le_bytes());
    wav.extend_from_slice(&(RATE * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(data.len() as u32).to_le_bytes());
    wav.extend_from_slice(&data);
    wav
}

/// The chime as a file the OS player can open (written once).
fn chime_file() -> Option<PathBuf> {
    let path = crate::config::dir()?.join("alarm-chime.wav");
    let wav = chime_wav();
    if std::fs::metadata(&path).map(|m| m.len()).ok() != Some(wav.len() as u64) {
        std::fs::create_dir_all(path.parent()?).ok()?;
        std::fs::write(&path, wav).ok()?;
    }
    Some(path)
}

#[derive(Default)]
pub struct AlarmSound {
    stop: Option<Arc<AtomicBool>>,
}

impl AlarmSound {
    pub fn set(&mut self, on: bool) {
        if on {
            self.start();
        } else {
            self.stop();
        }
    }

    #[cfg(windows)]
    fn start(&mut self) {
        use windows::core::HSTRING;
        use windows::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_FILENAME, SND_LOOP, SND_NODEFAULT};
        let Some(path) = chime_file() else { return };
        // SAFETY: a valid, NUL-terminated path; PlaySound copies what it needs.
        unsafe {
            let _ =
                PlaySoundW(&HSTRING::from(path.as_os_str()), None, SND_FILENAME | SND_ASYNC | SND_LOOP | SND_NODEFAULT);
        }
        self.stop = Some(Arc::new(AtomicBool::new(false)));
    }

    #[cfg(windows)]
    fn stop(&mut self) {
        if self.stop.take().is_some() {
            // SAFETY: a null sound stops whatever this process is playing.
            unsafe {
                let _ = windows::Win32::Media::Audio::PlaySoundW(None, None, Default::default());
            }
        }
    }

    #[cfg(not(windows))]
    fn start(&mut self) {
        if self.stop.is_some() {
            return;
        }
        let Some(path) = chime_file() else { return };
        let flag = Arc::new(AtomicBool::new(false));
        self.stop = Some(flag.clone());
        let _ = std::thread::Builder::new().name("islet-alarm-sound".into()).spawn(move || {
            #[cfg(target_os = "macos")]
            let players: &[&[&str]] = &[&["afplay"]];
            #[cfg(not(target_os = "macos"))]
            let players: &[&[&str]] = &[&["pw-play"], &["paplay"], &["aplay", "-q"]];
            let mut player = players.iter().copied().find(|p| which(p[0]));
            while !flag.load(Ordering::Relaxed) {
                let Some(cmd) = player else { return };
                let child = std::process::Command::new(cmd[0])
                    .args(&cmd[1..])
                    .arg(&path)
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn();
                let Ok(mut child) = child else {
                    player = None;
                    continue;
                };
                // Poll only while ringing, so Stop silences it promptly.
                loop {
                    if flag.load(Ordering::Relaxed) {
                        let _ = child.kill();
                        let _ = child.wait();
                        return;
                    }
                    match child.try_wait() {
                        Ok(Some(status)) if !status.success() => return, // no audio device
                        Ok(Some(_)) => break,
                        Ok(None) => std::thread::sleep(std::time::Duration::from_millis(100)),
                        Err(_) => return,
                    }
                }
            }
        });
    }

    #[cfg(not(windows))]
    fn stop(&mut self) {
        if let Some(flag) = self.stop.take() {
            flag.store(true, Ordering::Relaxed);
        }
    }
}

#[cfg(not(windows))]
fn which(cmd: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|paths| std::env::split_paths(&paths).any(|d| d.join(cmd).is_file()))
}

impl Drop for AlarmSound {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn chime_is_a_valid_wav() {
        let w = super::chime_wav();
        assert_eq!(&w[0..4], b"RIFF");
        assert_eq!(&w[8..16], b"WAVEfmt ");
        let data_len = u32::from_le_bytes(w[40..44].try_into().unwrap()) as usize;
        assert_eq!(w.len(), 44 + data_len);
        let peak = w[44..].chunks(2).map(|c| i16::from_le_bytes([c[0], c[1]]).unsigned_abs()).max().unwrap();
        assert!(peak > 8000 && peak < i16::MAX as u16, "audible, not clipped: {peak}");
    }
}
