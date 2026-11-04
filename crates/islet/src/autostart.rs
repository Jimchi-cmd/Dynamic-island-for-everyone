//! Start Islet at login, the way each OS expects for a per-user app:
//! an XDG autostart entry on Linux, a LaunchAgent on macOS and the
//! per-user `Run` key on Windows. No admin rights, nothing system-wide.

use std::path::{Path, PathBuf};

/// The executable to start at login. Inside an AppImage the binary path is
/// a temporary mount, so the AppImage file itself is used.
fn exe() -> Result<PathBuf, String> {
    if let Some(appimage) = std::env::var_os("APPIMAGE") {
        return Ok(PathBuf::from(appimage));
    }
    std::env::current_exe().map_err(|e| e.to_string())
}

/// Desktop-entry `Exec` quoting: wrap in quotes, escape `"`, `` ` ``, `$`, `\`.
/// `%` is a field code and must be doubled.
#[cfg_attr(any(target_os = "macos", target_os = "windows"), allow(dead_code))]
pub fn desktop_entry(exe: &Path) -> String {
    let mut q = String::from("\"");
    for c in exe.to_string_lossy().chars() {
        match c {
            '"' | '`' | '$' | '\\' => {
                q.push('\\');
                q.push(c);
            }
            '%' => q.push_str("%%"),
            c => q.push(c),
        }
    }
    q.push('"');
    format!(
        "[Desktop Entry]\nType=Application\nName=Islet\nComment=Dynamic Island-style overlay\nExec={q}\nTerminal=false\nX-GNOME-Autostart-enabled=true\nNoDisplay=true\n"
    )
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub const LAUNCH_AGENT_LABEL: &str = "app.islet.desktop";

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn launch_agent_plist(exe: &Path) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{LAUNCH_AGENT_LABEL}</string>
  <key>ProgramArguments</key><array><string>{}</string></array>
  <key>RunAtLoad</key><true/>
  <key>ProcessType</key><string>Interactive</string>
</dict>
</plist>
"#,
        xml_escape(&exe.to_string_lossy())
    )
}

#[cfg(not(target_os = "windows"))]
fn entry_path() -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME").map(PathBuf::from).ok_or("HOME is not set")?;
    #[cfg(target_os = "macos")]
    return Ok(home.join("Library/LaunchAgents").join(format!("{LAUNCH_AGENT_LABEL}.plist")));
    #[cfg(not(target_os = "macos"))]
    Ok(std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".config"))
        .join("autostart/islet.desktop"))
}

#[cfg(not(target_os = "windows"))]
pub fn set(on: bool) -> Result<(), String> {
    let path = entry_path()?;
    if !on {
        return match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
            _ => Ok(()),
        };
    }
    let exe = exe()?;
    #[cfg(target_os = "macos")]
    let contents = launch_agent_plist(&exe);
    #[cfg(not(target_os = "macos"))]
    let contents = desktop_entry(&exe);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, contents).map_err(|e| e.to_string())
}

#[cfg(not(target_os = "windows"))]
pub fn is_on() -> bool {
    entry_path().is_ok_and(|p| p.exists())
}

#[cfg(target_os = "windows")]
mod win {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::Foundation::ERROR_FILE_NOT_FOUND;
    use windows::Win32::System::Registry::{
        RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ,
    };

    const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const NAME: &str = "Islet";

    pub fn set(on: bool) -> Result<(), String> {
        let (key, name) = (HSTRING::from(RUN), HSTRING::from(NAME));
        unsafe {
            if on {
                let cmd = format!("\"{}\"", super::exe()?.display());
                let wide: Vec<u16> = cmd.encode_utf16().chain(std::iter::once(0)).collect();
                RegSetKeyValueW(
                    HKEY_CURRENT_USER,
                    PCWSTR(key.as_ptr()),
                    PCWSTR(name.as_ptr()),
                    REG_SZ.0,
                    Some(wide.as_ptr().cast()),
                    (wide.len() * 2) as u32,
                )
                .ok()
                .map_err(|e| e.to_string())
            } else {
                let r = RegDeleteKeyValueW(HKEY_CURRENT_USER, PCWSTR(key.as_ptr()), PCWSTR(name.as_ptr()));
                if r == ERROR_FILE_NOT_FOUND {
                    Ok(())
                } else {
                    r.ok().map_err(|e| e.to_string())
                }
            }
        }
    }

    pub fn is_on() -> bool {
        let (key, name) = (HSTRING::from(RUN), HSTRING::from(NAME));
        unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                PCWSTR(key.as_ptr()),
                PCWSTR(name.as_ptr()),
                RRF_RT_REG_SZ,
                None,
                None,
                None,
            )
            .is_ok()
        }
    }
}

#[cfg(target_os = "windows")]
pub use win::{is_on, set};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_entry_quotes_the_path() {
        let e = desktop_entry(Path::new("/opt/My Apps/is$let 100%"));
        assert!(e.contains("Exec=\"/opt/My Apps/is\\$let 100%%\"\n"), "{e}");
        assert!(e.starts_with("[Desktop Entry]\n"));
    }

    #[test]
    fn launch_agent_escapes_xml() {
        let p = launch_agent_plist(Path::new("/Applications/A&B.app/Contents/MacOS/islet"));
        assert!(p.contains("<string>/Applications/A&amp;B.app/Contents/MacOS/islet</string>"));
        assert!(p.contains("<key>RunAtLoad</key><true/>"));
    }
}
