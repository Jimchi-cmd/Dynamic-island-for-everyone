//! The file shelf: references to files dropped on the island (paths, never
//! copies), kept until removed. Newest first, at most `MAX` items.

use serde::{Deserialize, Serialize};

pub const MAX: usize = 12;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShelfItem {
    pub path: String,
    /// A folder (shown with a folder icon).
    #[serde(default)]
    pub dir: bool,
}

impl ShelfItem {
    /// The file or folder name, for display.
    pub fn name(&self) -> &str {
        let p = self.path.trim_end_matches(['/', '\\']);
        p.rsplit(['/', '\\']).next().filter(|n| !n.is_empty()).unwrap_or(p)
    }
}

/// Add dropped paths to the front; duplicates move up instead of repeating.
pub fn add(items: &mut Vec<ShelfItem>, dropped: &[(String, bool)]) {
    for (p, dir) in dropped.iter().rev() {
        items.retain(|i| &i.path != p);
        items.insert(0, ShelfItem { path: p.clone(), dir: *dir });
    }
    items.truncate(MAX);
}

/// Paths from a `text/uri-list` (XDND drops): `file://` URIs, percent-decoded.
pub fn paths_from_uri_list(list: &str) -> Vec<String> {
    list.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.strip_prefix("file://"))
        .map(|rest| {
            // "file://host/path" or "file:///path".
            let path = match rest.find('/') {
                Some(0) => rest,
                Some(i) => &rest[i..],
                None => rest,
            };
            percent_decode(path)
        })
        .collect()
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_order() {
        let item = |p: &str| ShelfItem { path: p.into(), dir: false };
        assert_eq!(item("/home/a/Report final.pdf").name(), "Report final.pdf");
        assert_eq!(item("C:\\Users\\a\\Photos\\").name(), "Photos");
        let mut items = vec![item("/a"), item("/b")];
        add(&mut items, &[("/c".into(), false), ("/b".into(), false)]);
        let paths: Vec<_> = items.iter().map(|i| i.path.as_str()).collect();
        assert_eq!(paths, ["/c", "/b", "/a"], "dropped order kept, /b moved up, not repeated");
        add(&mut items, &(0..20).map(|i| (format!("/f{i}"), false)).collect::<Vec<_>>());
        assert_eq!(items.len(), MAX);
    }

    #[test]
    fn parses_uri_lists() {
        let list =
            "# comment\r\nfile:///home/a/My%20Notes.txt\r\nfile://laptop/home/a/b.png\r\nhttps://example.com/x\r\n";
        assert_eq!(paths_from_uri_list(list), ["/home/a/My Notes.txt", "/home/a/b.png"]);
        assert_eq!(paths_from_uri_list("file:///tmp/%E2%9C%93.txt"), ["/tmp/✓.txt"]);
    }
}
