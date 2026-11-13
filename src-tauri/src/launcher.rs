//! The quick launcher's app list: the programs installed on this computer,
//! by name, and starting one. The list is read from the places each system
//! keeps its launchers; nothing is sent anywhere.

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AppEntry {
    pub name: String,
    pub path: String,
}

/// At most this many folders deep, and this many apps.
const MAX_DEPTH: usize = 4;
const MAX_APPS: usize = 2000;

fn env(key: &str) -> Option<PathBuf> {
    std::env::var_os(key).filter(|v| !v.is_empty()).map(PathBuf::from)
}

/// Where this system keeps its app launchers.
fn roots() -> Vec<PathBuf> {
    let mut v = Vec::new();
    #[cfg(target_os = "macos")]
    {
        v.extend([PathBuf::from("/Applications"), PathBuf::from("/System/Applications")]);
        v.extend(env("HOME").map(|h| h.join("Applications")));
    }
    #[cfg(target_os = "windows")]
    {
        let menu = Path::new("Microsoft").join("Windows").join("Start Menu").join("Programs");
        v.extend(env("ProgramData").map(|d| d.join(&menu)));
        v.extend(env("APPDATA").map(|d| d.join(&menu)));
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        v.push(PathBuf::from("/usr/share/applications"));
        v.push(PathBuf::from("/usr/local/share/applications"));
        v.push(PathBuf::from("/var/lib/flatpak/exports/share/applications"));
        v.push(PathBuf::from("/var/lib/snapd/desktop/applications"));
        if let Some(home) = env("HOME") {
            v.push(home.join(".local/share/applications"));
            v.push(home.join(".local/share/flatpak/exports/share/applications"));
        }
    }
    v
}

/// The extension a launcher has on this system.
const KIND: &str = if cfg!(target_os = "macos") {
    "app"
} else if cfg!(target_os = "windows") {
    "lnk"
} else {
    "desktop"
};

/// A `.desktop` file's name, unless it says it isn't for menus.
pub fn desktop_name(contents: &str) -> Option<String> {
    let mut in_entry = false;
    let (mut name, mut hidden, mut app) = (None, false, true);
    for line in contents.lines().map(str::trim) {
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry {
            continue;
        }
        if let Some(v) = line.strip_prefix("Name=") {
            name.get_or_insert_with(|| v.trim().to_string());
        } else if line == "NoDisplay=true" || line == "Hidden=true" {
            hidden = true;
        } else if let Some(v) = line.strip_prefix("Type=") {
            app = v.trim() == "Application";
        }
    }
    name.filter(|n| !n.is_empty() && !hidden && app)
}

fn entry(path: &Path) -> Option<AppEntry> {
    let stem = path.file_stem()?.to_string_lossy().into_owned();
    let name = if KIND == "desktop" { desktop_name(&std::fs::read_to_string(path).ok()?)? } else { stem };
    // Uninstallers and help pages clutter the Windows Start menu.
    let lower = name.to_lowercase();
    if lower.contains("uninstall") || lower.ends_with(" help") || lower.ends_with(" documentation") {
        return None;
    }
    Some(AppEntry { name, path: path.to_string_lossy().into_owned() })
}

fn scan(dir: &Path, depth: usize, out: &mut Vec<AppEntry>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        if out.len() >= MAX_APPS {
            return;
        }
        let path = e.path();
        let is_kind = path.extension().is_some_and(|x| x.eq_ignore_ascii_case(KIND));
        if is_kind {
            out.extend(entry(&path));
        } else if depth < MAX_DEPTH && path.is_dir() {
            scan(&path, depth + 1, out);
        }
    }
}

/// Every app the launcher can start, by name (one entry per name).
pub fn list() -> Vec<AppEntry> {
    let mut out = Vec::new();
    for root in roots() {
        scan(&root, 0, &mut out);
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out.dedup_by(|a, b| a.name.eq_ignore_ascii_case(&b.name));
    out
}

/// Start an app. Only something the list holds can be started: the page
/// can't hand in a path of its own.
pub fn launch(path: &str) -> Result<(), String> {
    if !list().iter().any(|a| a.path == path) {
        return Err("that isn't one of the listed apps".into());
    }
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = Command::new("open");
        c.arg(path);
        c
    };
    #[cfg(target_os = "windows")]
    let mut cmd = {
        // Explorer opens a shortcut the way a double-click does.
        let mut c = Command::new("explorer");
        c.arg(path);
        c
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut cmd = {
        let mut c = Command::new("gio");
        c.args(["launch", path]);
        c
    };
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map(drop).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_desktop_files() {
        let app = "[Desktop Entry]\nType=Application\nName=Text Editor\nName[de]=Texteditor\nExec=gedit %U\n\n[Desktop Action new]\nName=New Window\n";
        assert_eq!(desktop_name(app).as_deref(), Some("Text Editor"));
        assert_eq!(desktop_name("[Desktop Entry]\nName=Hidden\nNoDisplay=true\n"), None);
        assert_eq!(desktop_name("[Desktop Entry]\nType=Link\nName=A link\n"), None);
        assert_eq!(desktop_name("[Other]\nName=Nope\n"), None);
    }

    #[test]
    fn only_listed_apps_can_be_started() {
        assert!(launch("/bin/sh").is_err());
        assert!(launch("").is_err());
        // The list itself never panics, whatever is installed.
        let apps = list();
        assert!(apps.windows(2).all(|w| !w[0].name.eq_ignore_ascii_case(&w[1].name)), "one entry per name");
    }
}
