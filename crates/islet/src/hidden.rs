//! Starting helper programs without a console window flashing up.

use std::ffi::OsStr;
use std::process::Command;

/// `Command::new`, except that on Windows a console program (PowerShell,
/// reg, netstat, taskkill…) runs without a window. The island has no
/// console of its own, so Windows would otherwise open a new console
/// window for each one, which flashes up for as long as the program runs.
pub fn command(program: impl AsRef<OsStr>) -> Command {
    #[allow(unused_mut)]
    let mut c = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    c
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    fn rust_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                rust_files(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
    }

    /// Every program the island starts goes through `command`, so none of
    /// them can open a console window on Windows.
    #[test]
    fn programs_are_only_started_through_command() {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        rust_files(&src, &mut files);
        assert!(files.len() > 20, "found the sources: {}", files.len());
        let needle = concat!("Command", "::new(");
        let offenders: Vec<_> = files
            .iter()
            .filter(|p| !p.ends_with("hidden.rs"))
            .filter(|p| std::fs::read_to_string(p).unwrap().contains(needle))
            .collect();
        assert!(offenders.is_empty(), "use crate::hidden::command instead in {offenders:?}");
    }

    #[test]
    fn commands_still_run_and_report_their_output() {
        #[cfg(windows)]
        let out = super::command("cmd").args(["/C", "echo hi"]).output().unwrap();
        #[cfg(not(windows))]
        let out = super::command("sh").args(["-c", "echo hi"]).output().unwrap();
        assert!(out.status.success());
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "hi");
    }
}
