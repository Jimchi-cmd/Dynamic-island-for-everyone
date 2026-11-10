//! Islet Settings: a small window for setup and preferences. It is started
//! on demand and exits when closed; the island itself never loads a webview.
//!
//! Settings holds no logic of its own for secrets, trips or login items: it
//! runs the `islet` binary installed next to it with an allow-listed set of
//! commands (see [`allowed`]), so there is one tested implementation.

use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use tauri::AppHandle;

/// Ed25519 public key for signed updates, supplied by the release workflow.
/// Builds without it (development, CI) offer a link to the release page.
const UPDATER_PUBKEY: Option<&str> = option_env!("ISLET_UPDATER_PUBKEY");
const REPO: &str = env!("CARGO_PKG_REPOSITORY");

/// Commands Settings may run. Anything else is refused.
pub fn allowed(args: &[String]) -> bool {
    let a: Vec<&str> = args.iter().map(String::as_str).collect();
    match a.as_slice() {
        ["status", "--json"] | ["autostart", "on" | "off"] | ["nav", "stop"] | ["quit"] => true,
        ["key", "routes"] | ["key", "routes", "--clear"] | ["update", "check", "--json"] => true,
        ["pro", "activate" | "check" | "deactivate"] | ["pro", "status", "--json"] => true,
        ["stopwatch", "start" | "pause" | "resume" | "lap" | "reset"] => true,
        ["world", "--json"] | ["world", "cities", "--json"] => true,
        ["todo", "--json"] | ["todo", "add" | "clear-done"] => true,
        ["todo", "done" | "remove", id] => id.parse::<u32>().is_ok(),
        ["world", "add" | "remove", city] => islet_core::worldclock::find(city).is_some(),
        ["alarm", "list", "--json"] | ["alarm", "stop" | "snooze"] => true,
        ["wifi", "list", "--json"] | ["wifi", "list", "--scan", "--json"] | ["bt", "list", "--json"] => true,
        ["wifi", "connect", ssid] | ["wifi", "connect", ssid, "--password-stdin"] => !ssid.starts_with('-'),
        ["bt", "connect" | "disconnect", id] => !id.starts_with('-'),
        ["timer", "pause" | "resume" | "stop"] => true,
        ["prefs", "--json"] | ["weather", "off"] | ["shelf", "--json"] | ["shelf", "clear"] => true,
        ["media", "hide" | "show", app] => !app.starts_with('-'),
        ["spotify", "logout" | "on" | "off"] => true,
        ["spotify", "login", id] => id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric()),
        ["calendar", "set"] | ["calendar", "on" | "off" | "clear"] | ["calendar", "--json"] => true,
        ["weather", "search", q, "--json"] => !q.starts_with('-'),
        ["weather", "set", "--lat", lat, "--lon", lon, "--name", name] => {
            lat.parse::<f64>().is_ok() && lon.parse::<f64>().is_ok() && !name.starts_with('-')
        }
        ["set", key, "on" | "off"] => islet_core::prefs::SWITCHES.contains(key),
        ["hotkey", "off"] | ["wifi" | "bt", "on" | "off"] => true,
        ["hotkey", combo] => islet_core::hotkey::Hotkey::parse(combo).is_ok(),
        ["look", "accent", name] => islet_core::prefs::ACCENTS.iter().any(|(n, _)| n == name),
        ["look", "size", name] => islet_core::prefs::SIZES.iter().any(|(n, _)| n == name),
        ["displays", "--json"] | ["look", "display", "primary"] => true,
        ["places", q, "--json"] => !q.starts_with('-'),
        ["places", q, "--json", "--session", tok] => {
            !q.starts_with('-') && tok.len() <= 64 && tok.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        }
        ["look", "display", name] => !name.starts_with('-'),
        ["look", "offset", px] => px.parse::<u32>().is_ok_and(|v| v <= islet_core::prefs::MAX_OFFSET),
        ["timer", "start", dur, rest @ ..] => {
            !dur.starts_with('-') && rest.chunks(2).all(|kv| matches!(kv, ["--label", v] if !v.starts_with('-')))
        }
        ["focus", rest @ ..] => {
            rest.chunks(2).all(|kv| matches!(kv, ["--work" | "--break" | "--rounds", v] if !v.starts_with('-')))
        }
        ["alarm", "remove" | "on" | "off", id] => id.parse::<u32>().is_ok(),
        ["alarm", "add", time, rest @ ..] => {
            !time.starts_with('-')
                && rest.chunks(2).all(|kv| matches!(kv, ["--label" | "--days", v] if !v.starts_with('-')))
        }
        ["nav", "start", rest @ ..] => {
            // Values are passed as separate arguments (never through a
            // shell); only the known options may appear.
            let mut it = rest.iter();
            let mut destination = false;
            while let Some(arg) = it.next() {
                match *arg {
                    "--from" | "--label" | "--mode" | "--place-id" => {
                        if it.next().is_none() {
                            return false;
                        }
                    }
                    a if a.starts_with('-') => return false,
                    _ => destination = true,
                }
            }
            destination
        }
        _ => false,
    }
}

fn island_path() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let dir = exe.parent().ok_or("no install folder")?;
    let path = dir.join(format!("islet{}", std::env::consts::EXE_SUFFIX));
    if path.exists() {
        Ok(path)
    } else {
        Err(format!("the island program is missing ({})", path.display()))
    }
}

fn command(path: &PathBuf) -> Command {
    #[cfg_attr(not(windows), allow(unused_mut))]
    let mut c = Command::new(path);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    c
}

/// Run an allow-listed `islet` command; `input` is written to its stdin
/// (the API key, which therefore never appears in a process listing).
#[tauri::command]
async fn islet(args: Vec<String>, input: Option<String>) -> Result<String, String> {
    if !allowed(&args) {
        return Err("command not allowed".into());
    }
    let mut child = command(&island_path()?)
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    if let Some(mut stdin) = child.stdin.take() {
        if let Some(text) = input {
            stdin.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
            stdin.write_all(b"\n").map_err(|e| e.to_string())?;
        }
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        Err(err.trim().trim_start_matches("islet: ").to_string())
    }
}

/// Start the island in the background (it outlives this window).
#[tauri::command]
fn start_island() -> Result<(), String> {
    let mut c = command(&island_path()?);
    c.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        c.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        c.creation_flags(DETACHED_PROCESS);
    }
    c.spawn().map(drop).map_err(|e| e.to_string())
}

/// Open a web page in the default browser (https only).
#[tauri::command]
fn open_link(url: String) -> Result<(), String> {
    if !url.starts_with("https://") || url.chars().any(char::is_whitespace) {
        return Err("only https links".into());
    }
    #[cfg(target_os = "macos")]
    let r = Command::new("open").arg(&url).spawn();
    #[cfg(target_os = "windows")]
    let r = command(&PathBuf::from("rundll32")).args(["url.dll,FileProtocolHandler", &url]).spawn();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let r = Command::new("xdg-open").arg(&url).spawn();
    r.map(drop).map_err(|e| e.to_string())
}

/// `--page wifi:Cafe` from the island: open that page (join Cafe).
#[tauri::command]
fn initial_page() -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter().position(|a| a == "--page").and_then(|i| args.get(i + 1)).cloned()
}

/// Windows Bluetooth settings (the only non-https link Settings opens).
#[tauri::command]
fn open_bluetooth_settings() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        command(&PathBuf::from("explorer")).arg("ms-settings:bluetooth").spawn().map(drop).map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "windows"))]
    Err("only on Windows".into())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Info {
    os: &'static str,
    version: &'static str,
    repo: &'static str,
    /// Updates can be downloaded and installed in place.
    self_update: bool,
}

#[tauri::command]
fn info() -> Info {
    // Linux packages (.deb/.rpm) are updated by the package manager; only an
    // AppImage replaces itself.
    let linux_package = cfg!(target_os = "linux") && std::env::var_os("APPIMAGE").is_none();
    // The MSIX package lives in a read-only folder; new versions come as a new package.
    let msix = cfg!(windows)
        && std::env::current_exe().is_ok_and(|p| p.to_string_lossy().to_ascii_lowercase().contains("\\windowsapps\\"));
    Info {
        os: std::env::consts::OS,
        version: env!("CARGO_PKG_VERSION"),
        repo: REPO,
        self_update: UPDATER_PUBKEY.is_some() && !linux_package && !msix,
    }
}

/// Settings' own preferences (settings.json in the config folder).
#[derive(Serialize, Deserialize, Default, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Prefs {
    /// The first-run screen was completed.
    pub welcomed: bool,
    /// Check GitHub for a new release whenever Settings opens (off by default:
    /// no network use the user did not ask for).
    pub check_updates: bool,
}

fn prefs_path() -> Option<PathBuf> {
    islet_core::paths::config_dir().map(|d| d.join("settings.json"))
}

#[tauri::command]
fn get_prefs() -> Prefs {
    prefs_path().and_then(|p| std::fs::read(p).ok()).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

#[tauri::command]
fn set_prefs(prefs: Prefs) -> Result<(), String> {
    let path = prefs_path().ok_or("no config folder")?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_vec_pretty(&prefs).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateInfo {
    current: String,
    latest: String,
    newer: bool,
    /// Release page, for builds that cannot install in place.
    url: String,
    notes: Option<String>,
}

#[tauri::command]
async fn check_update(app: AppHandle) -> Result<UpdateInfo, String> {
    let current = env!("CARGO_PKG_VERSION").to_string();
    if info().self_update {
        use tauri_plugin_updater::UpdaterExt;
        let update = app.updater().map_err(|e| e.to_string())?.check().await.map_err(|e| e.to_string())?;
        return Ok(match update {
            Some(u) => UpdateInfo {
                latest: u.version.clone(),
                newer: true,
                url: String::new(),
                notes: u.body.clone(),
                current,
            },
            None => UpdateInfo { latest: current.clone(), newer: false, url: String::new(), notes: None, current },
        });
    }
    let json = islet(vec!["update".into(), "check".into(), "--json".into()], None).await?;
    let v: serde_json::Value = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    Ok(UpdateInfo {
        current,
        latest: v["latest"].as_str().unwrap_or_default().to_string(),
        newer: v["newer"].as_bool().unwrap_or(false),
        url: v["url"].as_str().unwrap_or_default().to_string(),
        notes: None,
    })
}

/// Download, verify (Ed25519) and install the update, then restart.
#[tauri::command]
async fn install_update(app: AppHandle) -> Result<(), String> {
    use tauri_plugin_updater::UpdaterExt;
    if !info().self_update {
        return Err("this build updates from the release page".into());
    }
    let update = app.updater().map_err(|e| e.to_string())?.check().await.map_err(|e| e.to_string())?;
    let Some(update) = update else { return Ok(()) };
    let bytes = update.download(|_, _| {}, || {}).await.map_err(|e| e.to_string())?;
    // The island runs from the same install; close it before files are replaced.
    let _ = islet(vec!["quit".into()], None).await;
    update.install(bytes).map_err(|e| e.to_string())?;
    let _ = start_island();
    app.restart();
}

pub fn run() {
    let mut builder = tauri::Builder::default();
    if UPDATER_PUBKEY.is_some() {
        builder = builder.plugin(tauri_plugin_updater::Builder::new().build());
    }
    builder
        .invoke_handler(tauri::generate_handler![
            islet,
            start_island,
            open_link,
            info,
            get_prefs,
            set_prefs,
            check_update,
            install_update,
            initial_page,
            open_bluetooth_settings
        ])
        .run(tauri::generate_context!())
        .expect("error while running Islet Settings");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &[&str]) -> Vec<String> {
        s.iter().map(|a| a.to_string()).collect()
    }

    #[test]
    fn only_known_commands_are_allowed() {
        assert!(allowed(&args(&["status", "--json"])));
        assert!(allowed(&args(&["key", "routes"])));
        assert!(allowed(&args(&["nav", "start", "Kampala Road", "--from", "Ntinda", "--mode", "drive"])));
        assert!(allowed(&args(&["hotkey", "Ctrl+Alt+I"])) && allowed(&args(&["hotkey", "off"])));
        assert!(!allowed(&args(&["hotkey", "Shift+A"])), "not a valid shortcut");
        assert!(!allowed(&args(&["nav", "start", "--from", "Ntinda"])), "needs a destination");
        assert!(!allowed(&args(&["nav", "start", "X", "--from"])), "option without a value");
        assert!(!allowed(&args(&["nav", "start", "X", "--evil", "y"])));
        assert!(!allowed(&args(&["--mediaremote-probe"])));
        assert!(allowed(&args(&["alarm", "add", "07:30", "--label", "Gym", "--days", "weekdays"])));
        assert!(!allowed(&args(&["alarm", "add", "07:30", "--label"])), "option without a value");
        assert!(!allowed(&args(&["alarm", "remove", "all"])));
        assert!(allowed(&args(&["wifi", "connect", "Cafe", "--password-stdin"])));
        assert!(!allowed(&args(&["wifi", "connect", "--evil"])));
        assert!(!allowed(&args(&[])), "running the island itself goes through start_island");
        assert!(allowed(&args(&["pro", "activate"])) && allowed(&args(&["pro", "status", "--json"])));
        assert!(!allowed(&args(&["pro", "activate", "KEY-ON-THE-COMMAND-LINE"])), "keys go through stdin");
    }

    #[test]
    fn prefs_default_to_private() {
        let p: Prefs = serde_json::from_str("{}").unwrap();
        assert_eq!(p, Prefs { welcomed: false, check_updates: false });
    }
}
