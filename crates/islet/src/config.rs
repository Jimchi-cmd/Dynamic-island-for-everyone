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

/// Read a JSON file from the config folder.
pub fn load_json<T: serde::de::DeserializeOwned>(name: &str) -> Option<T> {
    serde_json::from_slice(&std::fs::read(file(name)?).ok()?).ok()
}

/// Write a JSON file to the config folder (atomically).
pub fn save_json<T: serde::Serialize>(name: &str, value: &T) -> Result<(), String> {
    let path = file(name).ok_or("no config folder")?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

pub fn remove(name: &str) {
    if let Some(p) = file(name) {
        let _ = std::fs::remove_file(p);
    }
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

/// The island's preferences, in island.json.
pub fn load_prefs() -> islet_core::prefs::IslandPrefs {
    file("island.json")
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn save_prefs(prefs: &islet_core::prefs::IslandPrefs) -> Result<(), String> {
    let path = file("island.json").ok_or("no config folder")?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(prefs).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

/// The file shelf, in shelf.json (paths only).
pub fn load_shelf() -> Vec<islet_core::shelf::ShelfItem> {
    file("shelf.json")
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn save_shelf(items: &[islet_core::shelf::ShelfItem]) {
    if let Some(path) = file("shelf.json") {
        let _ = serde_json::to_vec_pretty(items).map(|b| std::fs::write(path, b));
    }
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
