//! Screenshot preview (Islet Pro): watches the folder the system saves
//! screenshots into and shows each new one on the island for a few
//! seconds. The thread sleeps until the folder changes.

use super::downloads::Change;
use super::{art, Sink};
use islet_core::activity::{Custom, Report};
use islet_core::screenshot::{self, ACTIVITY_ID, SHOWN_SECS};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

/// Pictures larger than this aren't read (a thumbnail isn't worth it).
const MAX_BYTES: u64 = 40 * 1024 * 1024;

/// Stops the watcher when dropped (turned off, or Pro ended).
pub struct Watcher {
    on: Arc<AtomicBool>,
}

impl Drop for Watcher {
    fn drop(&mut self) {
        self.on.store(false, Ordering::Relaxed);
    }
}

pub fn start(sink: Sink) -> Option<Watcher> {
    let (dir, dedicated) = folder()?;
    let on = Arc::new(AtomicBool::new(true));
    let flag = on.clone();
    std::thread::Builder::new()
        .name("islet-screenshots".into())
        .spawn(move || run(&dir, dedicated, &flag, &sink))
        .ok()?;
    Some(Watcher { on })
}

/// Where the system saves screenshots, and whether that folder holds
/// nothing else.
fn folder() -> Option<(PathBuf, bool)> {
    let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    #[cfg(target_os = "macos")]
    let candidates = vec![(PathBuf::from(env("HOME")?).join("Desktop"), false)];
    #[cfg(target_os = "windows")]
    let candidates = {
        let home = PathBuf::from(env("USERPROFILE")?);
        vec![
            (home.join("Pictures").join("Screenshots"), true),
            (home.join("OneDrive").join("Pictures").join("Screenshots"), true),
        ]
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let candidates = {
        let home = PathBuf::from(env("HOME")?);
        vec![(home.join("Pictures").join("Screenshots"), true), (home.join("Pictures"), false)]
    };
    candidates.into_iter().find(|(dir, _)| dir.is_dir())
}

fn pictures(dir: &Path, dedicated: bool) -> HashSet<String> {
    let Ok(entries) = std::fs::read_dir(dir) else { return HashSet::new() };
    entries
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| screenshot::is_screenshot(name, dedicated))
        .collect()
}

/// Read a new picture once it has stopped growing, as a thumbnail.
fn thumbnail(path: &Path) -> Option<islet_core::activity::ArtImage> {
    let mut last = 0;
    for _ in 0..10 {
        let size = std::fs::metadata(path).ok()?.len();
        if size > 0 && size == last {
            break;
        }
        last = size;
        std::thread::sleep(Duration::from_millis(150));
    }
    if last == 0 || last > MAX_BYTES {
        return None;
    }
    art::decode(&std::fs::read(path).ok()?)
}

fn run(dir: &Path, dedicated: bool, on: &AtomicBool, sink: &Sink) {
    let Some(mut change) = Change::new(dir) else {
        super::debug_log("screenshots: can't watch the folder");
        return;
    };
    let mut known = pictures(dir, dedicated);
    let mut shown_until: Option<Instant> = None;
    while on.load(Ordering::Relaxed) {
        // Sleep until the folder changes, or the preview has had its time.
        change.wait(shown_until.map(|u| u.saturating_duration_since(Instant::now()).max(Duration::from_millis(50))));
        if !on.load(Ordering::Relaxed) {
            break;
        }
        if shown_until.is_some_and(|u| Instant::now() >= u) {
            shown_until = None;
            sink.send(Report::CustomEnded(ACTIVITY_ID.into()));
        }
        let now = pictures(dir, dedicated);
        // Only a file that just appeared: not one copied in with an old date.
        let fresh = now.difference(&known).find(|name| {
            std::fs::metadata(dir.join(name))
                .and_then(|m| m.modified())
                .ok()
                .and_then(|m| SystemTime::now().duration_since(m).ok())
                .is_some_and(|age| age < Duration::from_secs(20))
        });
        if let Some(name) = fresh {
            let path = dir.join(name);
            if let Some(image) = thumbnail(&path) {
                sink.send(Report::Custom(Custom {
                    id: ACTIVITY_ID.into(),
                    title: "Screenshot".into(),
                    subtitle: name.clone(),
                    progress: Some(1.0),
                    color: None,
                    open: Some(path.to_string_lossy().into_owned()),
                    image: Some(image),
                }));
                shown_until = Some(Instant::now() + Duration::from_secs(SHOWN_SECS));
            }
        }
        known = now;
    }
    if shown_until.is_some() {
        sink.send(Report::CustomEnded(ACTIVITY_ID.into()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    /// Runs on every system in CI: a picture saved into the watched folder
    /// is reported with a thumbnail; other files are not.
    #[test]
    fn a_saved_screenshot_is_shown_with_its_thumbnail() {
        let dir = std::env::temp_dir().join(format!("islet-shots-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // One that was there before: never announced.
        let mut picture = tiny_skia::Pixmap::new(64, 40).unwrap();
        picture.fill(tiny_skia::Color::from_rgba8(30, 120, 220, 255));
        let png = picture.encode_png().unwrap();
        std::fs::write(dir.join("Screenshot old.png"), &png).unwrap();

        let (tx, rx) = channel();
        let sink = Sink { tx, waker: crate::platform::Waker::new(|| {}) };
        let on = Arc::new(AtomicBool::new(true));
        let (d, flag) = (dir.clone(), on.clone());
        std::thread::spawn(move || run(&d, true, &flag, &sink));
        std::thread::sleep(Duration::from_millis(300));
        std::fs::write(dir.join("notes.txt"), b"not a picture").unwrap();
        std::fs::write(dir.join("Screenshot new.png"), &png).unwrap();

        let (report, _) = rx.recv_timeout(Duration::from_secs(8)).expect("the new screenshot is reported");
        let Report::Custom(c) = report else { panic!("not an activity: {report:?}") };
        assert_eq!(
            (c.id.as_str(), c.title.as_str(), c.subtitle.as_str()),
            (ACTIVITY_ID, "Screenshot", "Screenshot new.png")
        );
        assert!(c.open.as_deref().is_some_and(|p| p.ends_with("Screenshot new.png")));
        let image = c.image.expect("with a thumbnail");
        assert!(image.width > 0 && image.rgba.len() == (image.width * image.height * 4) as usize);
        assert_eq!(image.dominant, [30, 120, 220]);
        on.store(false, Ordering::Relaxed);
        // Wake the watcher so it sees the flag and stops.
        std::fs::write(dir.join("bye.txt"), b"").unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
