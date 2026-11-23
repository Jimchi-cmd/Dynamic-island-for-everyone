//! The words Islet shows, in the language picked in Settings.
//!
//! Every translation lives in one table, `locales/strings.tsv`, shared with
//! the Settings app and the phone app: one line per English sentence, one
//! column per language. Code writes the English text, `t("Wi-Fi")`, and gets
//! it back in the chosen language; anything not in the table stays English.
//! Text with numbers or names in it uses `{}` for each one, in order, and
//! `tf` to fill them in; a translation that needs them in another order
//! writes `{0}`, `{1}`, ... instead. A short word that could mean two
//! things carries a note for translators after `‖` ("May‖short month"),
//! which is never shown.

use std::collections::HashMap;
use std::sync::RwLock;

/// The table: a header line of language codes, then one line per sentence.
pub const TABLE: &str = include_str!("../../../locales/strings.tsv");

/// The languages Islet speaks, by code, with each one's own name for itself.
/// "auto" follows the computer's (or phone's) language.
pub const LANGUAGES: [(&str, &str); 11] = [
    ("auto", "Automatic"),
    ("en", "English"),
    ("es", "Español"),
    ("fr", "Français"),
    ("de", "Deutsch"),
    ("pt", "Português"),
    ("ru", "Русский"),
    ("zh", "中文（简体）"),
    ("ja", "日本語"),
    ("ar", "العربية"),
    ("hi", "हिन्दी"),
];

/// Languages written in a script the bundled Inter font doesn't have: the
/// island borrows the system's fonts for them.
pub const NEEDS_SYSTEM_FONTS: [&str; 4] = ["zh", "ja", "ar", "hi"];

/// The language to use: the setting, or with "auto" the system's locale
/// ("es-MX", "pt_BR.UTF-8", "zh-Hans-CN") when Islet speaks it; else English.
pub fn resolve(setting: &str, system_locale: &str) -> &'static str {
    let pick = |code: &str| LANGUAGES.iter().skip(1).find(|(c, _)| *c == code).map(|(c, _)| *c);
    if setting != "auto" {
        if let Some(c) = pick(setting) {
            return c;
        }
    }
    let base = system_locale.split(['-', '_', '.']).next().unwrap_or("").to_ascii_lowercase();
    pick(&base).unwrap_or("en")
}

/// One language's sentences, keyed by the English.
type Catalog = HashMap<&'static str, &'static str>;

struct Current {
    code: &'static str,
    words: &'static Catalog,
}

static CURRENT: RwLock<Option<Current>> = RwLock::new(None);
static LOADED: RwLock<Vec<(&'static str, &'static Catalog)>> = RwLock::new(Vec::new());

/// Parse one column of the table (kept for the rest of the run; at most one
/// per language, and only the ones that were picked).
fn catalog(code: &'static str) -> &'static Catalog {
    if let Some((_, c)) = LOADED.read().unwrap().iter().find(|(c, _)| *c == code) {
        return c;
    }
    let mut lines = TABLE.lines();
    let header: Vec<&str> = lines.next().unwrap_or("").split('\t').collect();
    let col = header.iter().position(|h| *h == code);
    let mut words = Catalog::new();
    if let Some(col) = col.filter(|&c| c > 0) {
        for line in lines {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let cells: Vec<&'static str> = line.split('\t').collect();
            if let Some(&tr) = cells.get(col).filter(|s| !s.is_empty()) {
                words.insert(cells[0], tr);
            }
        }
    }
    let leaked: &'static Catalog = Box::leak(Box::new(words));
    LOADED.write().unwrap().push((code, leaked));
    leaked
}

/// Switch every later `t` and `tf` to this language ("en", "fr", ...).
pub fn set_language(code: &str) {
    let code = LANGUAGES.iter().skip(1).find(|(c, _)| *c == code).map_or("en", |(c, _)| *c);
    let words = catalog(code);
    *CURRENT.write().unwrap() = Some(Current { code, words });
}

/// The language now in use.
pub fn language() -> &'static str {
    CURRENT.read().unwrap().as_ref().map_or("en", |c| c.code)
}

/// The English text of a table key: the key without its note.
pub fn english(key: &'static str) -> &'static str {
    match key.find('‖') {
        Some(i) => key[..i].trim_end(),
        None => key,
    }
}

/// `english` in a given language (without switching to it).
pub fn lookup(code: &str, key: &'static str) -> &'static str {
    let code = LANGUAGES.iter().skip(1).find(|(c, _)| *c == code).map_or("en", |(c, _)| *c);
    catalog(code).get(key).copied().unwrap_or_else(|| english(key))
}

/// `english` in the current language.
pub fn t(english: &'static str) -> &'static str {
    match CURRENT.read().unwrap().as_ref() {
        Some(c) => c.words.get(english).copied().unwrap_or_else(|| self::english(english)),
        None => self::english(english),
    }
}

/// `template` in the current language, with each `{}` (or `{0}`, `{1}`, ...)
/// filled in from `args`.
pub fn tf(template: &'static str, args: &[&dyn std::fmt::Display]) -> String {
    fill(t(template), args)
}

/// Fill `{}` in order and `{n}` by position.
pub fn fill(template: &str, args: &[&dyn std::fmt::Display]) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut next = 0;
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find('}') {
            Some(close) if after[..close].chars().all(|c| c.is_ascii_digit()) => {
                let index = if close == 0 {
                    next += 1;
                    next - 1
                } else {
                    after[..close].parse().unwrap_or(usize::MAX)
                };
                if let Some(a) = args.get(index) {
                    out.push_str(&a.to_string());
                }
                rest = &after[close + 1..];
            }
            _ => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// How many values a template takes.
pub fn holes(template: &str) -> usize {
    let mut n = 0;
    let mut most = 0;
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        match after.find('}') {
            Some(close) if after[..close].chars().all(|c| c.is_ascii_digit()) => {
                if close == 0 {
                    n += 1;
                } else if let Ok(i) = after[..close].parse::<usize>() {
                    most = most.max(i + 1);
                }
                rest = &after[close + 1..];
            }
            _ => rest = after,
        }
    }
    n.max(most)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_complete_and_every_sentence_keeps_its_values() {
        let mut lines = TABLE.lines();
        let header: Vec<&str> = lines.next().unwrap().split('\t').collect();
        let codes: Vec<&str> = LANGUAGES.iter().skip(1).map(|(c, _)| *c).collect();
        assert_eq!(header, codes, "one column per language, English first");
        let mut seen = std::collections::HashSet::new();
        for (n, line) in lines.enumerate() {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let cells: Vec<&str> = line.split('\t').collect();
            assert_eq!(cells.len(), header.len(), "line {}: {line:?}", n + 2);
            assert!(seen.insert(cells[0]), "line {}: {:?} twice", n + 2, cells[0]);
            for (code, cell) in header.iter().zip(&cells).skip(1) {
                assert!(!cell.is_empty(), "line {}: no {code} for {:?}", n + 2, cells[0]);
                let en = cells[0].split('‖').next().unwrap().trim_end();
                assert_eq!(holes(cell), holes(en), "line {}: {code} {cell:?} vs {:?}", n + 2, cells[0]);
                assert_eq!(cell.trim(), *cell, "line {}: {code} has spaces at an end", n + 2);
            }
        }
    }

    /// Every `t("…")` and `tf("…"` in the laptop island, the Settings app
    /// and the phone app is in the table, so nothing shows up in English by
    /// mistake.
    #[test]
    fn everything_the_apps_say_is_translated() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let keys: std::collections::HashSet<&str> = TABLE
            .lines()
            .skip(1)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(|l| l.split('\t').next().unwrap())
            .collect();
        let mut missing = Vec::new();
        let mut files = Vec::new();
        for dir in ["crates", "ui", "android/app/src/main/kotlin", "src-tauri/src"] {
            walk(&root.join(dir), &mut files);
        }
        let calls = ["t(\"", "tr(\"", "tf(\"", "T.t(\"", "T.f(\"", "$t('", "tr('", "tf('"];
        for f in files {
            let Ok(src) = std::fs::read_to_string(&f) else { continue };
            for call in calls {
                let quote = if call.ends_with('\'') { '\'' } else { '"' };
                let mut rest = src.as_str();
                while let Some(i) = rest.find(call) {
                    let before = rest[..i].chars().last();
                    rest = &rest[i + call.len()..];
                    if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                        continue;
                    }
                    let Some(end) = find_end(rest, quote) else { continue };
                    let key = rest[..end].replace("\\\"", "\"").replace("\\'", "'");
                    if !keys.contains(key.as_str()) {
                        missing.push(format!("{}: {key:?}", f.strip_prefix(&root).unwrap_or(&f).display()));
                    }
                }
            }
        }
        missing.sort();
        missing.dedup();
        assert!(missing.is_empty(), "not in locales/strings.tsv:\n{}", missing.join("\n"));
    }

    fn find_end(s: &str, quote: char) -> Option<usize> {
        let mut escaped = false;
        for (i, c) in s.char_indices() {
            match c {
                '\\' if !escaped => escaped = true,
                c if c == quote && !escaped => return Some(i),
                _ => escaped = false,
            }
        }
        None
    }

    fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for e in entries.flatten() {
            let p = e.path();
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if p.is_dir() {
                if !matches!(name, "target" | "node_modules" | "build" | "test" | "tests") {
                    walk(&p, out);
                }
            } else if (name.ends_with(".rs") && name != "i18n.rs")
                || name.ends_with(".kt")
                || name.ends_with(".svelte")
                || (name.ends_with(".ts") && !name.ends_with(".test.ts"))
            {
                out.push(p);
            }
        }
    }

    #[test]
    fn filling_in_values() {
        assert_eq!(fill("{} min", &[&5]), "5 min");
        assert_eq!(fill("{1} de {0}", &[&"a", &"b"]), "b de a");
        assert_eq!(fill("no values", &[]), "no values");
        assert_eq!(fill("{name} stays", &[&1]), "{name} stays");
        assert_eq!(holes("{} of {}"), 2);
        assert_eq!(holes("{1} {0}"), 2);
        assert_eq!(english("May‖short month"), "May");
        assert_eq!(english("May"), "May");
    }

    #[test]
    fn picking_the_language() {
        assert_eq!(resolve("auto", "es-MX"), "es");
        assert_eq!(resolve("auto", "pt_BR.UTF-8"), "pt");
        assert_eq!(resolve("auto", "zh-Hans-CN"), "zh");
        assert_eq!(resolve("auto", "sw-KE"), "en", "not one of Islet's: English");
        assert_eq!(resolve("fr", "de-DE"), "fr", "a chosen language wins");
        assert_eq!(resolve("xx", "de-DE"), "de", "an unknown setting acts as auto");
    }

    #[test]
    fn looking_words_up() {
        // (The language in use is global, so tests never switch it.)
        assert_eq!(lookup("fr", "Control Centre"), "Centre de contrôle");
        assert_eq!(lookup("ja", "Wi-Fi"), "Wi-Fi");
        assert_eq!(lookup("de", "a sentence nobody translated"), "a sentence nobody translated");
        assert_eq!(lookup("nonsense", "Control Centre"), "Control Centre", "unknown: English");
        assert_eq!(lookup("en", "Control Centre"), "Control Centre");
        assert_eq!(language(), "en", "nothing switched it");
    }
}
