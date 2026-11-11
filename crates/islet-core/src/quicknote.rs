//! Quick notes (Islet Pro): one line at a time, added to the end of a plain
//! text file you can open in any editor.

use crate::clock::LocalTime;

/// The file's name, in your Documents folder.
pub const FILE_NAME: &str = "Islet notes.txt";
/// The longest note, in characters.
pub const MAX_LEN: usize = 500;

/// The line a note becomes: "2026-11-04 14:05  Ask about the essay".
/// Line breaks become spaces, so one note is always one line.
pub fn entry(text: &str, now: &LocalTime) -> Result<String, String> {
    let text: String = text.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(MAX_LEN).collect();
    if text.is_empty() {
        return Err("Write something first.".into());
    }
    Ok(format!("{:04}-{:02}-{:02} {:02}:{:02}  {text}\n", now.year, now.month, now.day, now.hour, now.minute))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_note_is_one_dated_line() {
        let now = LocalTime { year: 2026, month: 11, day: 4, weekday: 3, hour: 14, minute: 5, second: 30 };
        assert_eq!(entry("  Ask about\nthe   essay ", &now).unwrap(), "2026-11-04 14:05  Ask about the essay\n");
        assert!(entry(" \n ", &now).is_err());
        let long = entry(&"x".repeat(900), &now).unwrap();
        assert_eq!(long.trim_end().chars().count(), "2026-11-04 14:05  ".len() + MAX_LEN);
    }
}
