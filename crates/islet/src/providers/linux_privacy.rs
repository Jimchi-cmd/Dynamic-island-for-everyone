//! Camera and microphone in-use dots on Linux, event-driven.
//!
//! - Camera: inotify tells us when a `/dev/video*` node is opened or
//!   closed; then `/proc/*/fd` says which of the user's processes hold one.
//! - Microphone: `pactl subscribe` (PulseAudio or PipeWire's pulse server)
//!   prints a line when a recording stream starts or stops; then
//!   `pactl list source-outputs` names the apps. Volume meters and streams
//!   recording what the speakers play (monitor sources) don't count.
//!
//! Both threads block until something happens: no polling, no wakeups.

use super::Sink;
use islet_core::activity::{Privacy, Report};
use std::collections::BTreeSet;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::Stdio;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

/// Let a burst of opens and closes settle before looking.
const SETTLE: Duration = Duration::from_millis(200);

#[derive(Debug, Clone, Copy)]
enum Dirty {
    Camera,
    Mic,
}

pub fn start(sink: Sink) {
    let (tx, rx) = channel();
    let cam = tx.clone();
    let _ = std::thread::Builder::new().name("privacy-camera".into()).spawn(move || watch_cameras(cam));
    let _ = std::thread::Builder::new().name("privacy-mic".into()).spawn(move || watch_mic(tx));
    let _ = std::thread::Builder::new().name("privacy".into()).spawn(move || scanner(rx, sink));
}

/// Look again whenever a watcher says something changed, and report only
/// real changes.
fn scanner(rx: Receiver<Dirty>, sink: Sink) {
    let mut state = Privacy::default();
    while let Ok(first) = rx.recv() {
        let (mut cam, mut mic) = (false, false);
        let mut mark = |d: Dirty| match d {
            Dirty::Camera => cam = true,
            Dirty::Mic => mic = true,
        };
        mark(first);
        loop {
            match rx.recv_timeout(SETTLE) {
                Ok(d) => mark(d),
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
        let mut next = state.clone();
        if cam {
            next.camera = camera_users(Path::new("/proc"), std::process::id());
        }
        if mic {
            next.mic = mic_users();
        }
        if next != state {
            state = next;
            sink.send(Report::Privacy(state.clone()));
        }
    }
}

// ---- camera ----------------------------------------------------------------

fn is_video_node(name: &str) -> bool {
    name.strip_prefix("video").is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// Which processes (by name) have a `/dev/video*` node open, or None.
pub(crate) fn camera_users(proc_root: &Path, own_pid: u32) -> Option<Vec<String>> {
    let mut names = BTreeSet::new();
    for entry in std::fs::read_dir(proc_root).ok()?.flatten() {
        let Some(pid) = entry.file_name().to_str().and_then(|s| s.parse::<u32>().ok()) else { continue };
        if pid == own_pid {
            continue;
        }
        let Ok(fds) = std::fs::read_dir(entry.path().join("fd")) else { continue };
        let holds = fds.flatten().any(|fd| {
            std::fs::read_link(fd.path()).is_ok_and(|t| {
                t.parent() == Some(Path::new("/dev"))
                    && t.file_name().and_then(|n| n.to_str()).is_some_and(is_video_node)
            })
        });
        if holds {
            let comm = std::fs::read_to_string(entry.path().join("comm")).unwrap_or_default();
            names.insert(app_name(comm.trim()));
        }
    }
    (!names.is_empty()).then(|| names.into_iter().collect())
}

/// "firefox-bin" → "Firefox", "chrome" → "Chrome".
fn app_name(comm: &str) -> String {
    let base = comm.strip_suffix("-bin").unwrap_or(comm);
    let mut c = base.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => "An app".into(),
    }
}

fn watch_cameras(tx: Sender<Dirty>) {
    // SAFETY: plain inotify syscalls on a descriptor this function owns;
    // the buffer outlives every read into it.
    unsafe {
        let fd = libc::inotify_init1(libc::IN_CLOEXEC);
        if fd < 0 {
            return;
        }
        let add = |path: &str, mask: u32| {
            if let Ok(c) = std::ffi::CString::new(path) {
                libc::inotify_add_watch(fd, c.as_ptr(), mask);
            }
        };
        const NODE: u32 = libc::IN_OPEN | libc::IN_CLOSE_WRITE | libc::IN_CLOSE_NOWRITE;
        // New cameras (plugged in later) appear in /dev.
        add("/dev", libc::IN_CREATE);
        if let Ok(dir) = std::fs::read_dir("/dev") {
            for e in dir.flatten() {
                if e.file_name().to_str().is_some_and(is_video_node) {
                    add(&e.path().to_string_lossy(), NODE);
                }
            }
        }
        // Something may already be using the camera.
        if tx.send(Dirty::Camera).is_err() {
            return;
        }
        let mut buf = [0u8; 4096];
        loop {
            let n = libc::read(fd, buf.as_mut_ptr().cast(), buf.len());
            if n <= 0 {
                if n < 0 && std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                break;
            }
            let mut off = 0usize;
            let mut relevant = false;
            let header = std::mem::size_of::<libc::inotify_event>();
            while off + header <= n as usize {
                let ev = std::ptr::read_unaligned(buf.as_ptr().add(off) as *const libc::inotify_event);
                let name_bytes = &buf[off + header..off + header + ev.len as usize];
                let name = std::str::from_utf8(name_bytes).unwrap_or("").trim_end_matches('\0');
                if ev.mask & libc::IN_CREATE != 0 {
                    if is_video_node(name) {
                        add(&format!("/dev/{name}"), NODE);
                    }
                } else {
                    relevant = true;
                }
                off += header + ev.len as usize;
            }
            if relevant && tx.send(Dirty::Camera).is_err() {
                break;
            }
        }
        libc::close(fd);
    }
}

// ---- microphone ------------------------------------------------------------

/// Apps whose recording streams are only level meters.
const METERS: &[&str] =
    &["org.PulseAudio.pavucontrol", "org.gnome.VolumeControl", "org.kde.kmixd", "org.kde.plasma-pa"];

fn watch_mic(tx: Sender<Dirty>) {
    let mut backoff = 5u64;
    loop {
        let child = crate::hidden::command("pactl")
            .arg("subscribe")
            .env("LC_ALL", "C")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();
        let Ok(mut child) = child else { return }; // no pactl: no microphone dot
        if tx.send(Dirty::Mic).is_err() {
            let _ = child.kill();
            return;
        }
        let started = std::time::Instant::now();
        if let Some(out) = child.stdout.take() {
            for line in BufReader::new(out).lines() {
                let Ok(line) = line else { break };
                if line.contains("source-output") && tx.send(Dirty::Mic).is_err() {
                    let _ = child.kill();
                    return;
                }
            }
        }
        let _ = child.wait();
        // The sound server went away (restart, logout): it's not recording.
        let _ = tx.send(Dirty::Mic);
        if started.elapsed() > Duration::from_secs(60) {
            backoff = 5;
        }
        std::thread::sleep(Duration::from_secs(backoff));
        backoff = (backoff * 2).min(300);
    }
}

fn pactl(args: &[&str]) -> Option<String> {
    let out = crate::hidden::command("pactl").args(args).env("LC_ALL", "C").stderr(Stdio::null()).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

fn mic_users() -> Option<Vec<String>> {
    let monitors = pactl(&["list", "short", "sources"]).map(|s| monitor_sources(&s)).unwrap_or_default();
    match pactl(&["-f", "json", "list", "source-outputs"]) {
        Some(json) => match parse_source_outputs(&json, &monitors) {
            Ok(v) => v,
            // An old pactl without JSON: say only that something records.
            Err(()) => pactl(&["list", "short", "source-outputs"]).and_then(|s| (!s.trim().is_empty()).then(Vec::new)),
        },
        None => None,
    }
}

/// Indexes of monitor sources (what the speakers play) in
/// `pactl list short sources`.
pub(crate) fn monitor_sources(short: &str) -> BTreeSet<u64> {
    short
        .lines()
        .filter_map(|l| {
            let mut f = l.split('\t');
            let idx = f.next()?.trim().parse().ok()?;
            f.next()?.ends_with(".monitor").then_some(idx)
        })
        .collect()
}

/// App names recording from a real input in `pactl -f json list
/// source-outputs`, or None when nothing records.
pub(crate) fn parse_source_outputs(json: &str, monitors: &BTreeSet<u64>) -> Result<Option<Vec<String>>, ()> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|_| ())?;
    let list = v.as_array().ok_or(())?;
    let mut names = BTreeSet::new();
    for so in list {
        if so["corked"].as_bool() == Some(true) || so["source"].as_u64().is_some_and(|s| monitors.contains(&s)) {
            continue;
        }
        let props = &so["properties"];
        let prop = |k: &str| props[k].as_str().unwrap_or("");
        if METERS.contains(&prop("application.id")) || prop("media.name").to_ascii_lowercase().contains("peak detect") {
            continue;
        }
        let name = [prop("application.name"), prop("application.process.binary")]
            .into_iter()
            .find(|s| !s.is_empty())
            .map_or_else(|| "An app".to_string(), str::to_string);
        names.insert(name);
    }
    Ok((!names.is_empty()).then(|| names.into_iter().collect()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_nodes() {
        assert!(is_video_node("video0"));
        assert!(is_video_node("video12"));
        assert!(!is_video_node("video"));
        assert!(!is_video_node("video0p"));
        assert!(!is_video_node("vhci"));
    }

    #[test]
    fn finds_processes_holding_a_camera() {
        let root = std::env::temp_dir().join(format!("islet-proc-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let mk = |pid: u32, comm: &str, targets: &[&str]| {
            let fd = root.join(pid.to_string()).join("fd");
            std::fs::create_dir_all(&fd).unwrap();
            std::fs::write(root.join(pid.to_string()).join("comm"), format!("{comm}\n")).unwrap();
            for (i, t) in targets.iter().enumerate() {
                std::os::unix::fs::symlink(t, fd.join(i.to_string())).unwrap();
            }
        };
        mk(100, "firefox-bin", &["/dev/null", "/dev/video0"]);
        mk(200, "zoom", &["/dev/video2"]);
        mk(300, "bash", &["/dev/pts/1", "/dev/video-not"]);
        mk(400, "islet", &["/dev/video0"]); // ourselves
        std::fs::create_dir_all(root.join("self")).unwrap();
        assert_eq!(camera_users(&root, 400), Some(vec!["Firefox".to_string(), "Zoom".to_string()]));
        std::fs::remove_dir_all(root.join("100")).unwrap();
        std::fs::remove_dir_all(root.join("200")).unwrap();
        assert_eq!(camera_users(&root, 400), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn microphone_streams() {
        let monitors = monitor_sources(
            "45\talsa_output.pci.analog-stereo.monitor\tPipeWire\ts32le 2ch 48000Hz\tSUSPENDED\n\
             46\talsa_input.pci.analog-stereo\tPipeWire\ts32le 2ch 48000Hz\tRUNNING\n",
        );
        assert_eq!(monitors, BTreeSet::from([45]));
        let json = r#"[
          {"index":1,"source":46,"corked":false,"properties":{"application.name":"Zoom","application.process.binary":"zoom"}},
          {"index":2,"source":46,"corked":false,"properties":{"application.process.binary":"arecord"}},
          {"index":3,"source":45,"corked":false,"properties":{"application.name":"OBS"}},
          {"index":4,"source":46,"corked":false,"properties":{"application.id":"org.PulseAudio.pavucontrol","application.name":"Volume Control"}},
          {"index":5,"source":46,"corked":false,"properties":{"application.name":"GNOME Settings","media.name":"Peak detect"}},
          {"index":6,"source":46,"corked":true,"properties":{"application.name":"Paused recorder"}}
        ]"#;
        assert_eq!(parse_source_outputs(json, &monitors), Ok(Some(vec!["Zoom".to_string(), "arecord".to_string()])));
        assert_eq!(parse_source_outputs("[]", &monitors), Ok(None));
        assert_eq!(parse_source_outputs("Failure: no JSON here", &monitors), Err(()));
    }
}
