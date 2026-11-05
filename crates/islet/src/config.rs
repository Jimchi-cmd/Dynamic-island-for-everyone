//! Islet's files (see `islet_core::paths`) and the saved trip.

use islet_core::alarm::Alarm;
use islet_core::nav::Trip;
use std::path::PathBuf;

pub fn dir() -> Option<PathBuf> {
    islet_core::paths::config_dir()
}

fn file(name: &str) -> Option<PathBuf> {
    let d = dir()?;
    std::fs::create_dir_all(&d).ok()?;
    Some(d.join(name))
}

pub fn control_file() -> Option<PathBuf> {
    file("control.json")
}

/// The active trip survives restarts until it is stopped.
pub fn load_trip() -> Option<Trip> {
    serde_json::from_slice(&std::fs::read(file("trip.json")?).ok()?).ok()
}

pub fn save_trip(trip: Option<&Trip>) {
    let Some(path) = file("trip.json") else { return };
    let _ = match trip {
        Some(t) => serde_json::to_vec_pretty(t).map_err(std::io::Error::other).and_then(|b| std::fs::write(&path, b)),
        None => std::fs::remove_file(&path),
    };
}

/// Alarms, in alarms.json.
pub fn load_alarms() -> Vec<Alarm> {
    file("alarms.json")
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn save_alarms(alarms: &[Alarm]) -> Result<(), String> {
    let path = file("alarms.json").ok_or("no config folder")?;
    let json = serde_json::to_vec_pretty(alarms).map_err(|e| e.to_string())?;
    // Write then rename, so the island never reads a half-written file.
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

/// Write a file only the current user can read (the control token).
pub fn write_private(path: &PathBuf, contents: &str) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        use std::os::unix::fs::PermissionsExt;
        let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(path)?;
        // An older file may have been created with looser permissions.
        f.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        f.write_all(contents.as_bytes())
    }
    #[cfg(not(unix))]
    std::fs::write(path, contents)
}
