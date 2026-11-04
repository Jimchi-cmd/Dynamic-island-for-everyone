//! The Google Routes API key lives only in the OS keychain: Keychain on
//! macOS, Credential Manager on Windows, the Secret Service (GNOME Keyring,
//! KWallet) via `secret-tool` on Linux. It is never written to a file.
//! `ISLET_GOOGLE_ROUTES_KEY` overrides it for development only.

const SERVICE: &str = "islet";
const ACCOUNT: &str = "google-routes";

pub fn routes_key() -> Result<String, String> {
    if let Ok(k) = std::env::var("ISLET_GOOGLE_ROUTES_KEY") {
        if !k.trim().is_empty() {
            return Ok(k.trim().to_string());
        }
    }
    get()
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
        .ok_or_else(|| "No Google Routes API key yet. Add one with: islet key routes".to_string())
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn get() -> Option<String> {
    keyring::Entry::new(SERVICE, ACCOUNT).ok()?.get_password().ok()
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub fn set_routes_key(key: &str) -> Result<(), String> {
    keyring::Entry::new(SERVICE, ACCOUNT).and_then(|e| e.set_password(key)).map_err(|e| e.to_string())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn get() -> Option<String> {
    let out = std::process::Command::new("secret-tool")
        .args(["lookup", "application", SERVICE, "key", ACCOUNT])
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn set_routes_key(key: &str) -> Result<(), String> {
    use std::io::Write;
    let mut child = std::process::Command::new("secret-tool")
        .args(["store", "--label=Islet: Google Routes API key", "application", SERVICE, "key", ACCOUNT])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("secret-tool is needed to store secrets (package libsecret-tools): {e}"))?;
    child.stdin.take().ok_or("no stdin")?.write_all(key.as_bytes()).map_err(|e| e.to_string())?;
    let status = child.wait().map_err(|e| e.to_string())?;
    status.success().then_some(()).ok_or_else(|| "secret-tool could not store the key (is a keyring unlocked?)".into())
}
