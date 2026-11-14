//! Plugin widgets (see `islet_core::plugin`): each plugin's command run on
//! its own schedule, on one thread, while plugins are switched on. What it
//! prints is shown as a live activity; an unchanged result sends nothing.

use super::Sink;
use islet_core::activity::{Custom, Report};
use islet_core::plugin::{self, Manifest};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The folder plugins live in.
pub fn folder() -> Option<PathBuf> {
    Some(crate::config::dir()?.join("plugins"))
}

/// A plugin found in the folder, or why it can't run.
pub struct Found {
    pub dir: PathBuf,
    pub id: String,
    pub manifest: Result<Manifest, String>,
}

/// The plugins in `folder`, by folder name (at most `MAX_PLUGINS` run).
pub fn scan(folder: &Path) -> Vec<Found> {
    let Ok(entries) = std::fs::read_dir(folder) else { return Vec::new() };
    let mut out: Vec<Found> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.join("plugin.json").is_file())
        .map(|dir| {
            let name = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let manifest = std::fs::read_to_string(dir.join("plugin.json"))
                .map_err(|e| e.to_string())
                .and_then(|s| plugin::parse_manifest(&s));
            Found { id: plugin::activity_id(&name), dir, manifest }
        })
        .collect();
    out.sort_by(|a, b| a.dir.cmp(&b.dir));
    out
}

/// Run a plugin once: what it printed, or None if it failed or took
/// longer than `limit`.
pub fn run_once(dir: &Path, m: &Manifest, limit: Duration) -> Option<String> {
    let first = &m.command[0];
    // "./name": the plugin's own file.
    let program =
        if first.starts_with("./") || first.starts_with(".\\") { dir.join(&first[2..]) } else { first.into() };
    let mut child = crate::hidden::command(program)
        .args(&m.command[1..])
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    // Read on the side, so a chatty plugin can't fill the pipe and stall.
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = (&mut stdout).take(plugin::MAX_OUTPUT as u64).read_to_end(&mut buf);
        // Drain the rest so the plugin can finish.
        let _ = std::io::copy(&mut stdout, &mut std::io::sink());
        buf
    });
    let deadline = Instant::now() + limit;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    let buf = reader.join().ok()?;
    Some(String::from_utf8_lossy(&buf).into_owned())
}

/// Stops the plugins when dropped (switched off in Settings), and takes
/// what they showed off the island then, so a runner started straight
/// after can't have its activities cleared by this one.
pub struct Runner {
    on: Arc<AtomicBool>,
    shown: Arc<Mutex<Vec<String>>>,
    sink: Sink,
}

impl Drop for Runner {
    fn drop(&mut self) {
        let shown = self.shown.lock().unwrap();
        self.on.store(false, Ordering::Relaxed);
        for id in shown.iter() {
            self.sink.send(Report::CustomEnded(id.clone()));
        }
    }
}

pub fn start(sink: Sink) -> Option<Runner> {
    let folder = folder()?;
    let on = Arc::new(AtomicBool::new(true));
    let shown = Arc::new(Mutex::new(Vec::new()));
    let (flag, ids, out) = (on.clone(), shown.clone(), sink.clone());
    std::thread::Builder::new().name("islet-plugins".into()).spawn(move || run(&folder, &flag, &ids, &out)).ok()?;
    Some(Runner { on, shown, sink })
}

fn run(folder: &Path, on: &AtomicBool, ids: &Mutex<Vec<String>>, sink: &Sink) {
    let plugins: Vec<(PathBuf, String, Manifest)> = scan(folder)
        .into_iter()
        .filter_map(|f| Some((f.dir, f.id, f.manifest.ok()?)))
        .take(plugin::MAX_PLUGINS)
        .collect();
    if plugins.is_empty() {
        return;
    }
    let mut due = vec![Instant::now(); plugins.len()];
    let mut shown: Vec<Option<Custom>> = vec![None; plugins.len()];
    while on.load(Ordering::Relaxed) {
        for (i, (dir, id, m)) in plugins.iter().enumerate() {
            if Instant::now() < due[i] || !on.load(Ordering::Relaxed) {
                continue;
            }
            due[i] = Instant::now() + Duration::from_secs(m.every);
            // A failed run keeps what was shown.
            let Some(out) = run_once(dir, m, Duration::from_secs(plugin::TIMEOUT_SECS)) else { continue };
            let now = plugin::parse_output(id, &out);
            if now != shown[i] {
                // Checked under the lock the runner's drop takes: once
                // switched off, nothing more is sent.
                let mut ids = ids.lock().unwrap();
                if !on.load(Ordering::Relaxed) {
                    return;
                }
                ids.retain(|x| x != id);
                match &now {
                    Some(c) => {
                        ids.push(id.clone());
                        sink.send(Report::Custom(c.clone()));
                    }
                    None => sink.send(Report::CustomEnded(id.clone())),
                }
                shown[i] = now;
            }
        }
        // Sleep until the next plugin is due (checking the switch each second).
        let next = due.iter().min().copied().unwrap_or_else(Instant::now);
        let wait = next.saturating_duration_since(Instant::now()).min(Duration::from_secs(1));
        std::thread::sleep(wait.max(Duration::from_millis(100)));
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn runs_a_plugin_and_finds_the_broken_ones() {
        let root = std::env::temp_dir().join(format!("islet-plugins-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let good = root.join("hello");
        std::fs::create_dir_all(&good).unwrap();
        std::fs::write(good.join("plugin.json"), r#"{"name":"Hello","command":["sh","./hello.sh"],"every":30}"#)
            .unwrap();
        std::fs::write(good.join("hello.sh"), "echo 'Hello'\necho \"from $(basename \"$PWD\")\"\n").unwrap();
        let bad = root.join("broken");
        std::fs::create_dir_all(&bad).unwrap();
        std::fs::write(bad.join("plugin.json"), r#"{"name":"","command":["x"]}"#).unwrap();
        std::fs::create_dir_all(root.join("not-a-plugin")).unwrap();

        let found = scan(&root);
        assert_eq!(found.len(), 2, "only folders with a plugin.json");
        assert!(found[0].manifest.is_err(), "the broken one says why");
        let m = found[1].manifest.clone().unwrap();
        let out = run_once(&found[1].dir, &m, Duration::from_secs(5)).unwrap();
        let c = plugin::parse_output(&found[1].id, &out).unwrap();
        assert_eq!((c.id.as_str(), c.title.as_str(), c.subtitle.as_str()), ("plugin-hello", "Hello", "from hello"));

        // One that never finishes is stopped.
        let slow = Manifest { name: "Slow".into(), command: vec!["sleep".into(), "30".into()], every: 60 };
        let t = Instant::now();
        assert_eq!(run_once(&good, &slow, Duration::from_millis(300)), None);
        assert!(t.elapsed() < Duration::from_secs(5));
        let _ = std::fs::remove_dir_all(&root);
    }
}
