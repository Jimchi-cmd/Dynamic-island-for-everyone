//! Plugin widgets: the user's own scripts, each in a folder of its own
//! under `plugins` in Islet's settings folder, with a `plugin.json`:
//!
//! ```json
//! { "name": "Unread mail", "command": ["./unread.sh"], "every": 60 }
//! ```
//!
//! Islet runs the command every `every` seconds (only while plugins are
//! switched on) and shows what it prints as a live activity: the first line
//! as the title, the second as the subtitle; or a JSON object with
//! `title`, `subtitle`, `progress` (0..1), `color` ([r, g, b]) and `open`.
//! Printing nothing takes the activity away.

use crate::activity::Custom;
use serde::{Deserialize, Serialize};

/// At most this many plugins run (the island shows 8 activities at most).
pub const MAX_PLUGINS: usize = crate::activity::MAX_CUSTOM;
/// No plugin runs more often than this, or less often than once a day.
pub const MIN_EVERY: u64 = 5;
pub const MAX_EVERY: u64 = 86_400;
/// A run that takes longer than this is stopped.
pub const TIMEOUT_SECS: u64 = 10;
/// Output read from each run, at most.
pub const MAX_OUTPUT: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Manifest {
    pub name: String,
    /// The program and its arguments, run in the plugin's folder (never
    /// through a shell). A first word starting with "./" is in that folder.
    pub command: Vec<String>,
    /// Seconds between runs.
    #[serde(default = "default_every")]
    pub every: u64,
}

fn default_every() -> u64 {
    60
}

/// Read and check a `plugin.json`.
pub fn parse_manifest(json: &str) -> Result<Manifest, String> {
    let mut m: Manifest = serde_json::from_str(json).map_err(|e| format!("plugin.json: {e}"))?;
    m.name = m.name.trim().to_string();
    if m.name.is_empty() || m.name.chars().count() > 40 {
        return Err("plugin.json: \"name\" needs 1 to 40 characters".into());
    }
    if m.command.first().is_none_or(|c| c.trim().is_empty()) {
        return Err("plugin.json: \"command\" needs a program to run".into());
    }
    m.every = m.every.clamp(MIN_EVERY, MAX_EVERY);
    Ok(m)
}

/// The activity id for the plugin in folder `dir_name`.
pub fn activity_id(dir_name: &str) -> String {
    let safe: String =
        dir_name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '-' }).take(40).collect();
    format!("plugin-{safe}")
}

#[derive(Deserialize)]
struct Shown {
    title: String,
    #[serde(default)]
    subtitle: String,
    #[serde(default)]
    progress: Option<f32>,
    #[serde(default)]
    color: Option<[u8; 3]>,
    #[serde(default)]
    open: Option<String>,
}

fn clip(s: &str, max: usize) -> String {
    s.trim().chars().take(max).collect()
}

/// What a run printed, as the activity to show (None: show nothing).
pub fn parse_output(id: &str, out: &str) -> Option<Custom> {
    let out = out.trim();
    if out.is_empty() {
        return None;
    }
    let shown = if out.starts_with('{') {
        serde_json::from_str::<Shown>(out).ok()?
    } else {
        let mut lines = out.lines().map(str::trim).filter(|l| !l.is_empty());
        let title = lines.next()?.to_string();
        Shown { title, subtitle: lines.next().unwrap_or_default().to_string(), progress: None, color: None, open: None }
    };
    let title = clip(&shown.title, 80);
    if title.is_empty() {
        return None;
    }
    Some(Custom {
        id: id.to_string(),
        title,
        subtitle: clip(&shown.subtitle, 120),
        progress: shown.progress.filter(|p| p.is_finite()).map(|p| p.clamp(0.0, 1.0)),
        color: shown.color,
        // Only web links and files open from a tap.
        open: shown.open.filter(|o| o.starts_with("https://") || o.starts_with('/') || o.get(1..3) == Some(":\\")),
        image: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_manifest() {
        let m = parse_manifest(r#"{"name":" Unread mail ","command":["./unread.sh","--inbox"],"every":1}"#).unwrap();
        assert_eq!(m.name, "Unread mail");
        assert_eq!(m.command, ["./unread.sh", "--inbox"]);
        assert_eq!(m.every, MIN_EVERY, "never more often than every 5 seconds");
        assert_eq!(parse_manifest(r#"{"name":"x","command":["date"]}"#).unwrap().every, 60);
        assert!(parse_manifest(r#"{"name":"","command":["date"]}"#).is_err());
        assert!(parse_manifest(r#"{"name":"x","command":[]}"#).is_err());
        assert!(parse_manifest("nope").is_err());
    }

    #[test]
    fn shows_plain_lines_or_json() {
        let c = parse_output("plugin-mail", "3 unread\nfrom Alex, Sam\nignored\n").unwrap();
        assert_eq!((c.title.as_str(), c.subtitle.as_str(), c.progress), ("3 unread", "from Alex, Sam", None));
        let c = parse_output(
            "plugin-build",
            r#"{"title":"Build","subtitle":"tests","progress":1.7,"color":[0,200,0],"open":"https://ci.example"}"#,
        )
        .unwrap();
        assert_eq!(
            (c.progress, c.color, c.open.as_deref()),
            (Some(1.0), Some([0, 200, 0]), Some("https://ci.example"))
        );
        let c = parse_output("p", r#"{"title":"x","open":"javascript:alert(1)"}"#).unwrap();
        assert_eq!(c.open, None, "only web links and files");
        assert!(parse_output("p", "  \n ").is_none(), "nothing printed: nothing shown");
        assert!(parse_output("p", r#"{"title":"  "}"#).is_none());
    }

    #[test]
    fn ids_are_plain() {
        assert_eq!(activity_id("mail"), "plugin-mail");
        assert_eq!(activity_id("my plugin/../x"), "plugin-my-plugin----x");
    }
}
