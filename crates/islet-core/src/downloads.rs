//! Downloads in progress (opt-in), from the files browsers write while
//! downloading: Chrome, Edge and Brave `name.crdownload`, Firefox
//! `name.part`, Safari a `name.download` bundle. The island shows the
//! newest one with its size and speed (browsers don't say how big the
//! whole file will be), then "Downloaded", which opens the file.

/// Partial-file suffixes and the browsers that use them.
const SUFFIXES: [&str; 4] = [".crdownload", ".part", ".download", ".partial"];

/// The live activity's id.
pub const ACTIVITY_ID: &str = "islet-download";

/// If `name` is a download in progress: the finished file's name, when the
/// browser has chosen it ("Unconfirmed 123.crdownload" hasn't).
pub fn partial(name: &str) -> Option<Option<String>> {
    let lower = name.to_lowercase();
    let suffix = SUFFIXES.iter().find(|s| lower.ends_with(*s) && lower.len() > s.len())?;
    let stem = &name[..name.len() - suffix.len()];
    if stem.starts_with("Unconfirmed ") || stem.starts_with('.') {
        return Some(None);
    }
    Some(Some(stem.to_string()))
}

/// "812 KB", "12.3 MB", "1.4 GB".
pub fn format_size(bytes: u64) -> String {
    const K: f64 = 1024.0;
    let b = bytes as f64;
    if b < K * K {
        format!("{:.0} KB", (b / K).max(if bytes > 0 { 1.0 } else { 0.0 }))
    } else if b < K * K * K {
        format!("{:.1} MB", b / (K * K))
    } else {
        format!("{:.2} GB", b / (K * K * K))
    }
}

/// "12.3 MB · 2.1 MB/s" (the speed once known).
pub fn progress_line(bytes: u64, per_sec: Option<f64>) -> String {
    match per_sec.filter(|r| *r > 0.0) {
        Some(r) => format!("{} · {}/s", format_size(bytes), format_size(r as u64)),
        None => format_size(bytes),
    }
}

/// The Downloads folder from `~/.config/user-dirs.dirs`
/// (`XDG_DOWNLOAD_DIR="$HOME/Downloads"`), if it names one.
pub fn xdg_download_dir(user_dirs: &str, home: &str) -> Option<String> {
    let line = user_dirs.lines().map(str::trim).find(|l| l.starts_with("XDG_DOWNLOAD_DIR="))?;
    let value = line["XDG_DOWNLOAD_DIR=".len()..].trim().trim_matches('"');
    let value = value.replace("$HOME", home);
    (!value.is_empty()).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_downloads() {
        assert_eq!(partial("setup.exe.crdownload"), Some(Some("setup.exe".into())));
        assert_eq!(partial("film.mkv.part"), Some(Some("film.mkv".into())));
        assert_eq!(partial("Report.pdf.download"), Some(Some("Report.pdf".into())));
        assert_eq!(partial("Unconfirmed 123456.crdownload"), Some(None));
        assert_eq!(partial("notes.txt"), None);
        assert_eq!(partial(".part"), None);
    }

    #[test]
    fn sizes_and_speeds() {
        assert_eq!(format_size(500), "1 KB");
        assert_eq!(format_size(0), "0 KB");
        assert_eq!(format_size(12_900_000), "12.3 MB");
        assert_eq!(format_size(1_500_000_000), "1.40 GB");
        assert_eq!(progress_line(12_900_000, Some(2_200_000.0)), "12.3 MB · 2.1 MB/s");
        assert_eq!(progress_line(12_900_000, None), "12.3 MB");
    }

    #[test]
    fn xdg_folder() {
        let dirs = "# comment\nXDG_DESKTOP_DIR=\"$HOME/Desktop\"\nXDG_DOWNLOAD_DIR=\"$HOME/Téléchargements\"\n";
        assert_eq!(xdg_download_dir(dirs, "/home/a").as_deref(), Some("/home/a/Téléchargements"));
        assert_eq!(xdg_download_dir("", "/home/a"), None);
    }
}
