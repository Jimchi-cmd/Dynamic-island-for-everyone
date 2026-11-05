//! Where Islet keeps its files (shared by the island and Settings).
//! Linux: `$XDG_CONFIG_HOME/islet` (or `~/.config/islet`); macOS:
//! `~/Library/Application Support/Islet`; Windows: `%APPDATA%\Islet`.
//! `ISLET_CONFIG_DIR` overrides it (tests, portable installs).

use std::path::PathBuf;

pub fn config_dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("ISLET_CONFIG_DIR") {
        return Some(PathBuf::from(d));
    }
    let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    if cfg!(target_os = "windows") {
        env("APPDATA").map(|a| a.join("Islet"))
    } else if cfg!(target_os = "macos") {
        env("HOME").map(|h| h.join("Library/Application Support/Islet"))
    } else {
        env("XDG_CONFIG_HOME").or_else(|| env("HOME").map(|h| h.join(".config"))).map(|c| c.join("islet"))
    }
}
