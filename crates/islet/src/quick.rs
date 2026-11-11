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
