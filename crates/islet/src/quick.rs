//! The clock card's Screenshot and Lock buttons: each starts the OS's own
//! tool (never through a shell). Islet takes no screenshots itself and never
//! sees the picture.

use std::process::{Command, Stdio};

type Argv = &'static [&'static str];

/// Ways to start the screenshot tool, best first.
pub fn screenshot_commands(os: &str) -> &'static [Argv] {
    match os {
        // The Snipping Tool's overlay.
        "windows" => &[&["rundll32", "url.dll,FileProtocolHandler", "ms-screenclip:"]],
        // The Screenshot toolbar (also on Shift-Command-5).
        "macos" => &[&["open", "-a", "Screenshot"]],
        // The desktop's own picker through the portal (GNOME, KDE, wlroots),
        // then the common tools.
        _ => &[
            &[
                "gdbus",
                "call",
                "--session",
                "--dest",
                "org.freedesktop.portal.Desktop",
                "--object-path",
                "/org/freedesktop/portal/desktop",
                "--method",
                "org.freedesktop.portal.Screenshot.Screenshot",
                "",
                "{'interactive': <true>}",
            ],
            &["spectacle", "--region"],
            &["gnome-screenshot", "--interactive"],
            &["xfce4-screenshooter"],
            &["flameshot", "gui"],
        ],
    }
}

/// Ways to lock the session, best first.
pub fn lock_commands(os: &str) -> &'static [Argv] {
    match os {
        "windows" => &[&["rundll32", "user32.dll,LockWorkStation"]],
        // Sleeps the display; macOS then asks for the password if it is set
        // to (System Settings → Lock Screen → Require password).
        "macos" => &[&["pmset", "displaysleepnow"]],
        _ => &[&["loginctl", "lock-session"], &["xdg-screensaver", "lock"]],
    }
}

/// Run the first command that starts and succeeds. Runs on its own thread:
/// some of these wait for the user.
fn run_first(commands: &'static [Argv], what: &'static str) {
    std::thread::spawn(move || {
        for argv in commands {
            let status = Command::new(argv[0])
                .args(&argv[1..])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
            if status.is_ok_and(|s| s.success()) {
                return;
            }
        }
        crate::providers::debug_log(&format!("could not {what}: no tool for it was found"));
    });
}

pub fn screenshot() {
    run_first(screenshot_commands(std::env::consts::OS), "take a screenshot");
}

pub fn lock() {
    run_first(lock_commands(std::env::consts::OS), "lock the computer");
}

/// Open one of the user's shortcuts: a web link in the browser; a folder,
/// file or script as the file manager would (so a program or script runs).
pub fn open_shortcut(target: &str) {
    if target.starts_with("https://") || target.starts_with("http://") {
        crate::providers::navigation::open_url(target);
        return;
    }
    let path = std::path::Path::new(target);
    // Linux file managers don't run scripts through xdg-open: a file marked
    // executable is started directly, in its own folder.
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        use std::os::unix::fs::PermissionsExt;
        let runnable = std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0);
        if runnable {
            let mut cmd = Command::new(path);
            if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
                cmd.current_dir(dir);
            }
            let started = cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
            if let Err(e) = started {
                crate::providers::debug_log(&format!("could not run a shortcut: {e}"));
            }
            return;
        }
    }
    #[cfg(target_os = "macos")]
    let r = Command::new("open").arg(path).spawn();
    #[cfg(target_os = "windows")]
    let r = Command::new("explorer").arg(path).spawn();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let r = Command::new("xdg-open").arg(path).spawn();
    if let Err(e) = r {
        crate::providers::debug_log(&format!("could not open a shortcut: {e}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_os_has_a_tool_for_both() {
        for os in ["windows", "macos", "linux"] {
            for list in [screenshot_commands(os), lock_commands(os)] {
                assert!(!list.is_empty(), "{os}");
                assert!(list.iter().all(|argv| !argv.is_empty() && !argv[0].contains(' ')), "{os}: program names only");
            }
        }
        assert_eq!(lock_commands("windows")[0], ["rundll32", "user32.dll,LockWorkStation"]);
        assert_eq!(lock_commands("linux")[0], ["loginctl", "lock-session"]);
    }
}
