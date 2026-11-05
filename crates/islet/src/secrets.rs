//! Secrets live only in the OS keychain: Keychain on macOS, Credential
//! Manager on Windows, the Secret Service (GNOME Keyring, KWallet) via
//! `secret-tool` on Linux. They are never written to a file.
//!
//! - the Google Routes API key (`ISLET_GOOGLE_ROUTES_KEY` overrides it for
//!   development only);
//! - the private calendar (ICS) link, which contains its own secret token.

const SERVICE: &str = "islet";
const ROUTES: &str = "google-routes";
const CALENDAR: &str = "calendar-ics";

pub fn routes_key() -> Result<String, String> {
    if let Ok(k) = std::env::var("ISLET_GOOGLE_ROUTES_KEY") {
        if !k.trim().is_empty() {
            return Ok(k.trim().to_string());
        }
    }
    get(ROUTES).ok_or_else(|| "No Google Routes API key yet. Add one with: islet key routes".to_string())
}

pub fn set_routes_key(key: &str) -> Result<(), String> {
    set(ROUTES, key, "Islet: Google Routes API key")
}

pub fn clear_routes_key() -> Result<(), String> {
    clear(ROUTES)
}

pub fn calendar_url() -> Option<String> {
    get(CALENDAR)
}

pub fn set_calendar_url(url: &str) -> Result<(), String> {
    set(CALENDAR, url, "Islet: calendar link")
}

pub fn clear_calendar_url() -> Result<(), String> {
    clear(CALENDAR)
}

fn get(account: &str) -> Option<String> {
    read(account).map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn read(account: &str) -> Option<String> {
    keyring::Entry::new(SERVICE, account).ok()?.get_password().ok()
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn set(account: &str, value: &str, _label: &str) -> Result<(), String> {
    keyring::Entry::new(SERVICE, account).and_then(|e| e.set_password(value)).map_err(|e| e.to_string())
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn clear(account: &str) -> Result<(), String> {
    match keyring::Entry::new(SERVICE, account).and_then(|e| e.delete_credential()) {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn secret_tool_error(e: std::io::Error) -> String {
    if e.kind() == std::io::ErrorKind::NotFound {
        "secret-tool is not installed. Install libsecret-tools (Debian, Ubuntu) or libsecret (Fedora, Arch), then try again."
            .into()
    } else {
        format!("could not run secret-tool: {e}")
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn read(account: &str) -> Option<String> {
    let out = std::process::Command::new("secret-tool")
        .args(["lookup", "application", SERVICE, "key", account])
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn set(account: &str, value: &str, label: &str) -> Result<(), String> {
    use std::io::Write;
    let mut child = std::process::Command::new("secret-tool")
        .args(["store", &format!("--label={label}"), "application", SERVICE, "key", account])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .map_err(secret_tool_error)?;
    child.stdin.take().ok_or("no stdin")?.write_all(value.as_bytes()).map_err(|e| e.to_string())?;
    let status = child.wait().map_err(|e| e.to_string())?;
    status.success().then_some(()).ok_or_else(|| "secret-tool could not store it (is a keyring unlocked?)".into())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn clear(account: &str) -> Result<(), String> {
    let status = std::process::Command::new("secret-tool")
        .args(["clear", "application", SERVICE, "key", account])
        .status()
        .map_err(secret_tool_error)?;
    status.success().then_some(()).ok_or_else(|| "secret-tool could not remove it".into())
}
