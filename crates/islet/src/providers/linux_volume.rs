//! Output volume on Linux, from PulseAudio or PipeWire (pipewire-pulse)
//! through `pactl`: `pactl subscribe` blocks until something changes, then
//! the default sink's volume and mute state are read. No polling.

use super::{debug_log, Sink};
use islet_core::activity::{Hud, Report};
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

/// "Volume: front-left: 32768 /  50% / -18.06 dB,   front-right: …" → 0.5
pub fn parse_volume(out: &str) -> Option<f32> {
    let pcts: Vec<f32> =
        out.split('/').filter_map(|part| part.trim().strip_suffix('%')?.trim().parse::<f32>().ok()).collect();
    (!pcts.is_empty()).then(|| pcts.iter().sum::<f32>() / pcts.len() as f32 / 100.0)
}

/// "Mute: yes" → true
pub fn parse_mute(out: &str) -> Option<bool> {
    match out.trim().strip_prefix("Mute:")?.trim() {
        "yes" => Some(true),
        "no" => Some(false),
        _ => None,
    }
}

fn pactl(args: &[&str]) -> Option<String> {
    let out = Command::new("pactl").args(args).stderr(Stdio::null()).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

fn read() -> Option<(f32, bool)> {
    let level = parse_volume(&pactl(&["get-sink-volume", "@DEFAULT_SINK@"])?)?;
    let muted = parse_mute(&pactl(&["get-sink-mute", "@DEFAULT_SINK@"])?)?;
    Some((level, muted))
}

pub fn start(sink: Sink) {
    let _ = std::thread::Builder::new().name("islet-volume".into()).spawn(move || {
        let Some(mut last) = read() else {
            debug_log("volume pop-ups unavailable (pactl not found or no sound server)");
            return;
        };
        sink.send(Report::Volume(last.0, last.1));
        let Ok(mut child) = Command::new("pactl")
            .arg("subscribe")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
        else {
            return;
        };
        let Some(out) = child.stdout.take() else { return };
        for line in BufReader::new(out).lines().map_while(Result::ok) {
            // "Event 'change' on sink #56"; sink-input and other events are ignored.
            if !(line.contains("'change'") && line.contains(" sink #")) {
                continue;
            }
            let Some(now) = read() else { continue };
            // A key press can emit several events: only real changes show.
            if (now.0 - last.0).abs() > 0.001 || now.1 != last.1 {
                last = now;
                sink.send(Report::Hud(Hud::Volume { level: now.0, muted: now.1 }));
            }
        }
        let _ = child.wait();
    });
}

/// Set the default output's volume (the Now Playing card's slider), with
/// `pactl` on a helper thread. A drag sends many levels; only the newest
/// waiting one is applied, so `pactl` never runs more than one at a time.
/// The change comes back through `subscribe` like any other.
pub fn set(level: f32) {
    static SETTER: std::sync::OnceLock<std::sync::Mutex<std::sync::mpsc::Sender<f32>>> = std::sync::OnceLock::new();
    let tx = SETTER.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<f32>();
        let _ = std::thread::Builder::new().name("islet-volume-set".into()).spawn(move || {
            while let Ok(mut level) = rx.recv() {
                while let Ok(newer) = rx.try_recv() {
                    level = newer;
                }
                let pct = format!("{}%", (level * 100.0).round() as u32);
                let _ = pactl(&["set-sink-volume", "@DEFAULT_SINK@", &pct]);
            }
        });
        std::sync::Mutex::new(tx)
    });
    let _ = tx.lock().unwrap().send(level);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pactl_output() {
        let v = "Volume: front-left: 32768 /  50% / -18.06 dB,   front-right: 39321 /  60% / -13.31 dB\n        balance 0.00\n";
        assert!((parse_volume(v).unwrap() - 0.55).abs() < 1e-6);
        assert_eq!(parse_mute("Mute: yes\n"), Some(true));
        assert_eq!(parse_mute("Mute: no"), Some(false));
        assert_eq!(parse_volume("nonsense"), None);
    }
}
