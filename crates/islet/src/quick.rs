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

// ---- Brightness and dark mode (the Controls panel) ---------------------------

const GNOME_POWER: [&str; 3] =
    ["org.gnome.SettingsDaemon.Power", "/org/gnome/SettingsDaemon/Power", "org.gnome.SettingsDaemon.Power.Screen"];
const WIN_THEME: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";
const POWERSHELL: [&str; 4] = ["powershell", "-NoProfile", "-NonInteractive", "-Command"];

fn argv(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|p| (*p).to_string()).collect()
}

/// Ways to ask for the screen's brightness, best first. macOS has no tool
/// for it, so the slider doesn't show there.
pub fn brightness_read_commands(os: &str) -> Vec<Vec<String>> {
    match os {
        "windows" => vec![argv(&[
            POWERSHELL[0],
            POWERSHELL[1],
            POWERSHELL[2],
            POWERSHELL[3],
            "(Get-CimInstance -Namespace root/WMI -ClassName WmiMonitorBrightness).CurrentBrightness",
        ])],
        "macos" => Vec::new(),
        _ => vec![
            argv(&["busctl", "--user", "get-property", GNOME_POWER[0], GNOME_POWER[1], GNOME_POWER[2], "Brightness"]),
            argv(&["brightnessctl", "-m"]),
        ],
    }
}

/// Ways to set the screen's brightness to `pct` percent, best first.
pub fn brightness_set_commands(os: &str, pct: u8) -> Vec<Vec<String>> {
    let pct = pct.min(100).to_string();
    match os {
        "windows" => {
            let script = format!(
                "(Get-CimInstance -Namespace root/WMI -ClassName WmiMonitorBrightnessMethods | \
                 Invoke-CimMethod -MethodName WmiSetBrightness -Arguments @{{Timeout=1;Brightness={pct}}})"
            );
            vec![argv(&[POWERSHELL[0], POWERSHELL[1], POWERSHELL[2], POWERSHELL[3], &script])]
        }
        "macos" => Vec::new(),
        _ => vec![
            argv(&[
                "busctl",
                "--user",
                "set-property",
                GNOME_POWER[0],
                GNOME_POWER[1],
                GNOME_POWER[2],
                "Brightness",
                "i",
                &pct,
            ]),
            argv(&["brightnessctl", "set", &format!("{pct}%")]),
        ],
    }
}

/// The level (0..=1) in what a brightness command printed: "i 50" (busctl),
/// "intel_backlight,backlight,12000,50%,24000" (brightnessctl) or "50".
pub fn parse_brightness(out: &str) -> Option<f32> {
    let out = out.trim();
    let pct = if let Some(field) = out.split(',').find(|f| f.ends_with('%')) {
        field.trim_end_matches('%').trim().parse::<f32>().ok()?
    } else {
        out.split_whitespace().last()?.parse::<f32>().ok()?
    };
    (0.0..=100.0).contains(&pct).then_some(pct / 100.0)
}

/// How to ask whether the dark look is on.
pub fn dark_read_command(os: &str) -> Vec<String> {
    match os {
        "windows" => argv(&["reg", "query", WIN_THEME, "/v", "AppsUseLightTheme"]),
        "macos" => argv(&["defaults", "read", "-g", "AppleInterfaceStyle"]),
        _ => argv(&["gsettings", "get", "org.gnome.desktop.interface", "color-scheme"]),
    }
}

/// Is the dark look on, going by what `dark_read_command` printed? (On
/// macOS the command fails in light mode, which reads as "not dark".)
pub fn parse_dark(os: &str, out: &str) -> bool {
    let out = out.to_lowercase();
    match os {
        // "    AppsUseLightTheme    REG_DWORD    0x0"
        "windows" => out.split_whitespace().last().is_some_and(|v| v == "0x0"),
        _ => out.contains("dark"),
    }
}

/// The commands that switch the look: every one of them is run.
pub fn dark_set_commands(os: &str, dark: bool) -> Vec<Vec<String>> {
    match os {
        "windows" => {
            let light = if dark { "0" } else { "1" };
            ["AppsUseLightTheme", "SystemUsesLightTheme"]
                .iter()
                .map(|name| argv(&["reg", "add", WIN_THEME, "/v", name, "/t", "REG_DWORD", "/d", light, "/f"]))
                .collect()
        }
        "macos" => {
            let script =
                format!("tell application \"System Events\" to tell appearance preferences to set dark mode to {dark}");
            vec![argv(&["osascript", "-e", &script])]
        }
        _ => {
            let (scheme, theme) = if dark { ("prefer-dark", "Adwaita-dark") } else { ("default", "Adwaita") };
            vec![
                argv(&["gsettings", "set", "org.gnome.desktop.interface", "color-scheme", scheme]),
                // Older GTK apps follow the theme's name instead.
                argv(&["gsettings", "set", "org.gnome.desktop.interface", "gtk-theme", theme]),
            ]
        }
    }
}

fn output_of(cmd: &[String]) -> Option<String> {
    let out = Command::new(cmd.first()?).args(&cmd[1..]).stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

fn succeeds(cmd: &[String]) -> bool {
    let Some(program) = cmd.first() else { return false };
    Command::new(program)
        .args(&cmd[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Read the screen's brightness on another thread and report it (nothing
/// is reported where it can't be read, and the slider then stays away).
pub fn read_brightness(sink: crate::providers::Sink) {
    std::thread::spawn(move || {
        let level = brightness_read_commands(std::env::consts::OS)
            .iter()
            .find_map(|cmd| output_of(cmd).as_deref().and_then(parse_brightness));
        if let Some(level) = level {
            sink.send(islet_core::activity::Report::Brightness(level));
        }
    });
}

/// Set the screen's brightness (0..=1) with the first tool that works.
pub fn set_brightness(level: f32) {
    let pct = (level.clamp(0.0, 1.0) * 100.0).round() as u8;
    std::thread::spawn(move || {
        if !brightness_set_commands(std::env::consts::OS, pct).iter().any(|cmd| succeeds(cmd)) {
            crate::providers::debug_log("could not set the brightness: no tool for it was found");
        }
    });
}

/// Switch between the dark and the light look.
pub fn toggle_dark_mode() {
    std::thread::spawn(move || {
        let os = std::env::consts::OS;
        let dark = output_of(&dark_read_command(os)).is_some_and(|out| parse_dark(os, &out));
        for cmd in dark_set_commands(os, !dark) {
            if !succeeds(&cmd) {
                crate::providers::debug_log(&format!("could not switch the look ({})", cmd[0]));
            }
        }
    });
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

    #[test]
    fn reads_brightness_from_each_tool() {
        assert_eq!(parse_brightness("i 50\n"), Some(0.5));
        assert_eq!(parse_brightness("intel_backlight,backlight,12000,25%,48000"), Some(0.25));
        assert_eq!(parse_brightness("80\r\n"), Some(0.8));
        assert_eq!(parse_brightness("i -1"), None, "no backlight");
        assert_eq!(parse_brightness(""), None);
        assert!(brightness_read_commands("macos").is_empty() && brightness_set_commands("macos", 50).is_empty());
        let set = brightness_set_commands("linux", 130);
        assert_eq!(set[0].last().map(String::as_str), Some("100"), "never past 100%");
        assert_eq!(set[1], ["brightnessctl", "set", "100%"]);
        assert!(brightness_set_commands("windows", 40)[0].last().unwrap().contains("Brightness=40"));
    }

    #[test]
    fn knows_the_dark_look_and_how_to_switch_it() {
        assert!(parse_dark("linux", "'prefer-dark'\n"));
        assert!(!parse_dark("linux", "'default'\n"));
        assert!(parse_dark("macos", "Dark\n"));
        assert!(!parse_dark("macos", ""));
        assert!(parse_dark("windows", "\r\n    AppsUseLightTheme    REG_DWORD    0x0\r\n"));
        assert!(!parse_dark("windows", "    AppsUseLightTheme    REG_DWORD    0x1\r\n"));
        let win = dark_set_commands("windows", true);
        assert_eq!(win.len(), 2, "apps and the taskbar");
        assert!(win.iter().all(|c| c[0] == "reg" && c.contains(&"0".to_string())));
        assert!(dark_set_commands("macos", false)[0][2].ends_with("set dark mode to false"));
        assert_eq!(dark_set_commands("linux", true)[0][4], "prefer-dark");
    }
}
