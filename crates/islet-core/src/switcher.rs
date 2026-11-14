//! The app switcher's list: the windows open on the desktop, by title and
//! app, so the launcher can bring one to the front. Each system's own way
//! of listing them is in the island's `switcher` module; the readers of
//! the Wayland compositors' JSON are here, where they can be tested.

use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OpenWindow {
    /// How the system names the window (an X window, a handle, a sway node,
    /// a Hyprland address, a macOS process id).
    pub id: String,
    pub title: String,
    pub app: String,
}

/// Windows without a title, and Islet's own, aren't worth switching to.
pub fn keep(w: &OpenWindow) -> bool {
    !w.title.trim().is_empty() && !w.app.eq_ignore_ascii_case("islet")
}

/// An id the switcher accepts back: digits, or a 0x address.
pub fn valid_id(id: &str) -> bool {
    let digits = id.strip_prefix("0x").unwrap_or(id);
    !digits.is_empty()
        && digits.len() <= 20
        && digits.chars().all(|c| c.is_ascii_hexdigit())
        && (id.starts_with("0x") || digits.chars().all(|c| c.is_ascii_digit()))
}

/// sway's `swaymsg -t get_tree -r`: every node with a process is a window.
pub fn parse_sway_tree(json: &str) -> Vec<OpenWindow> {
    fn walk(n: &Value, out: &mut Vec<OpenWindow>) {
        if n.get("pid").and_then(Value::as_u64).is_some() {
            let app = n
                .get("app_id")
                .and_then(Value::as_str)
                .or_else(|| n.pointer("/window_properties/class").and_then(Value::as_str))
                .unwrap_or_default();
            if let Some(id) = n.get("id").and_then(Value::as_u64) {
                out.push(OpenWindow {
                    id: id.to_string(),
                    title: n.get("name").and_then(Value::as_str).unwrap_or_default().to_string(),
                    app: app.to_string(),
                });
            }
        }
        for key in ["nodes", "floating_nodes"] {
            for child in n.get(key).and_then(Value::as_array).into_iter().flatten() {
                walk(child, out);
            }
        }
    }
    let mut out = Vec::new();
    if let Ok(tree) = serde_json::from_str::<Value>(json) {
        walk(&tree, &mut out);
    }
    out.retain(keep);
    out
}

/// Hyprland's `hyprctl clients -j`: the mapped, shown windows.
pub fn parse_hyprland_clients(json: &str) -> Vec<OpenWindow> {
    let Ok(Value::Array(list)) = serde_json::from_str::<Value>(json) else { return Vec::new() };
    let mut out: Vec<OpenWindow> = list
        .iter()
        .filter(|c| c.get("mapped").and_then(Value::as_bool).unwrap_or(true))
        .filter(|c| !c.get("hidden").and_then(Value::as_bool).unwrap_or(false))
        .filter_map(|c| {
            Some(OpenWindow {
                id: c.get("address")?.as_str()?.to_string(),
                title: c.get("title").and_then(Value::as_str).unwrap_or_default().to_string(),
                app: c.get("class").and_then(Value::as_str).unwrap_or_default().to_string(),
            })
        })
        .collect();
    out.retain(keep);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_sways_tree() {
        let tree = r#"{"id":1,"type":"root","nodes":[{"id":2,"type":"output","nodes":[
            {"id":3,"type":"workspace","nodes":[
                {"id":10,"name":"Inbox - Thunderbird","pid":400,"app_id":"thunderbird","nodes":[]},
                {"id":11,"type":"con","nodes":[{"id":12,"name":"vim notes.md","pid":401,"app_id":null,
                    "window_properties":{"class":"XTerm"},"nodes":[]}]}],
             "floating_nodes":[{"id":13,"name":"","pid":402,"app_id":"popup","nodes":[]},
                {"id":14,"name":"Islet","pid":403,"app_id":"islet","nodes":[]}]}]}]}"#;
        let w = parse_sway_tree(tree);
        assert_eq!(
            w,
            [
                OpenWindow { id: "10".into(), title: "Inbox - Thunderbird".into(), app: "thunderbird".into() },
                OpenWindow { id: "12".into(), title: "vim notes.md".into(), app: "XTerm".into() },
            ],
            "untitled windows and the island itself are left out"
        );
        assert!(parse_sway_tree("not json").is_empty());
    }

    #[test]
    fn reads_hyprlands_clients() {
        let json = r#"[{"address":"0x55d1a2b0","mapped":true,"hidden":false,"class":"firefox","title":"News"},
            {"address":"0x55d1a2c0","mapped":false,"hidden":false,"class":"x","title":"Gone"},
            {"address":"0x55d1a2d0","mapped":true,"hidden":true,"class":"y","title":"Hidden"}]"#;
        let w = parse_hyprland_clients(json);
        assert_eq!(w, [OpenWindow { id: "0x55d1a2b0".into(), title: "News".into(), app: "firefox".into() }]);
    }

    #[test]
    fn only_plain_ids_come_back() {
        assert!(valid_id("12345") && valid_id("0x55d1a2b0"));
        assert!(!valid_id("") && !valid_id("0x") && !valid_id("12a") && !valid_id("1; rm -rf ~"));
        assert!(!valid_id("address:0x1"), "a Hyprland dispatcher can't be slipped in");
    }
}
