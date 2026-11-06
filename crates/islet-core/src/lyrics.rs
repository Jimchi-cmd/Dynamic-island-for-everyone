//! Synced lyrics (opt-in): the lines of the playing song with the time each
//! is sung, from LRCLIB (lrclib.net, a free, open lyrics database meant for
//! apps like this; no account or key). Nothing is fetched until the user
//! turns lyrics on, and then only for the song that is playing.
//!
//! This module is pure: LRC parsing, the request URLs, response parsing and
//! where the lyric strip in the pill is scrolled to at a moment in the song.

/// One sung line and when it starts (seconds into the track).
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub at: f64,
    pub text: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Lyrics {
    /// Sorted by time. Empty for an instrumental.
    pub lines: Vec<Line>,
    pub instrumental: bool,
}

/// Shown for a blank line (a pause in the singing).
pub const PAUSE: &str = "• • •";

/// The last line counts as sung for this long (when nothing follows it).
const LAST_LINE_SECS: f64 = 6.0;

/// Lines are highlighted this much ahead of their timestamp, so the
/// highlight lands with the voice rather than just after it.
pub const LEAD: f64 = 0.15;

/// "mm:ss", "mm:ss.xx" or "mm:ss:xx" → seconds.
fn parse_stamp(s: &str) -> Option<f64> {
    let (m, rest) = s.split_once(':')?;
    let m: u32 = m.trim().parse().ok()?;
    // Some files write the hundredths after a second colon.
    let rest = rest.replacen(':', ".", 1);
    let secs: f64 = rest.trim().parse().ok()?;
    (secs.is_finite() && (0.0..60.0).contains(&secs)).then(|| f64::from(m) * 60.0 + secs)
}

/// Remove enhanced-LRC word stamps ("<00:12.34>word").
fn strip_word_stamps(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('<') {
        match rest[i..].find('>') {
            Some(j) if parse_stamp(&rest[i + 1..i + j]).is_some() => {
                out.push_str(&rest[..i]);
                rest = &rest[i + j + 1..];
            }
            _ => {
                out.push_str(&rest[..=i]);
                rest = &rest[i + 1..];
            }
        }
    }
    out.push_str(rest);
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Parse LRC text: "[01:02.50]words", several stamps per line allowed,
/// "[offset:+250]" (ms; positive shows lines sooner), metadata tags
/// ignored. Lines without a time are skipped.
pub fn parse_lrc(s: &str) -> Lyrics {
    let mut offset = 0.0;
    let mut lines = Vec::new();
    for raw in s.lines() {
        let mut rest = raw.trim();
        let mut stamps = Vec::new();
        while let Some(tag) = rest.strip_prefix('[') {
            let Some(end) = tag.find(']') else { break };
            let inner = &tag[..end];
            if let Some(ms) = inner.strip_prefix("offset:") {
                offset = ms.trim().parse::<f64>().unwrap_or(0.0) / 1000.0;
            } else if let Some(at) = parse_stamp(inner) {
                stamps.push(at);
            }
            rest = tag[end + 1..].trim_start();
        }
        let text = strip_word_stamps(rest);
        for at in stamps {
            lines.push(Line { at, text: text.clone() });
        }
    }
    for l in &mut lines {
        l.at = (l.at - offset).max(0.0);
    }
    lines.sort_by(|a, b| a.at.total_cmp(&b.at));
    // A trailing blank line just marks the end of the song.
    while lines.last().is_some_and(|l| l.text.is_empty()) {
        lines.pop();
    }
    Lyrics { lines, instrumental: false }
}

impl Lyrics {
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// The line being sung at `pos` seconds (None before the first).
    pub fn current(&self, pos: f64) -> Option<usize> {
        let pos = pos + LEAD;
        match self.lines.partition_point(|l| l.at <= pos) {
            0 => None,
            n => Some(n - 1),
        }
    }

    /// How long line `i` lasts (until the next one starts).
    pub fn duration_of(&self, i: usize) -> f64 {
        let at = self.lines.get(i).map_or(0.0, |l| l.at);
        self.lines.get(i + 1).map_or(LAST_LINE_SECS, |n| (n.at - at).max(0.1))
    }

    /// The text to show for line `i` (a blank line is a pause).
    pub fn text(&self, i: usize) -> &str {
        match self.lines.get(i) {
            Some(l) if !l.text.is_empty() => &l.text,
            Some(_) => PAUSE,
            None => "",
        }
    }
}

// ---- LRCLIB ------------------------------------------------------------------

/// What to ask for: the playing song.
#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    pub title: String,
    pub artist: String,
    pub album: String,
    /// Whole seconds, when the player says.
    pub duration: Option<u32>,
    /// The song's file, when the player says (an .lrc may sit beside it).
    pub file: Option<String>,
}

impl Track {
    /// Identifies the song (lyrics are kept per song, not per play).
    pub fn key(&self) -> String {
        format!("{}\u{1f}{}", self.artist.trim().to_lowercase(), self.title.trim().to_lowercase())
    }
}

// ---- Local .lrc files ------------------------------------------------------

/// Characters file names can't hold on some systems.
fn file_safe(s: &str) -> String {
    s.chars()
        .filter(|c| !matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
        .collect::<String>()
        .trim()
        .to_lowercase()
}

/// File names (lower case) in Islet's lyrics folder that hold this song's
/// lyrics: "Artist - Title.lrc", or just "Title.lrc".
pub fn local_names(t: &Track) -> Vec<String> {
    let (artist, title) = (file_safe(&t.artist), file_safe(&t.title));
    let mut v = Vec::new();
    if !artist.is_empty() {
        v.push(format!("{artist} - {title}.lrc"));
    }
    v.push(format!("{title}.lrc"));
    v
}

/// The .lrc next to the song's file ("Song.flac" → "Song.lrc").
pub fn beside(file: &str) -> std::path::PathBuf {
    std::path::Path::new(file).with_extension("lrc")
}

const API: &str = "https://lrclib.net/api";

/// The exact-match lookup. LRCLIB matches the duration within a couple of
/// seconds, so it's sent only when known.
pub fn get_url(t: &Track) -> String {
    use crate::nav::encode;
    let mut url = format!("{API}/get?track_name={}&artist_name={}", encode(&t.title), encode(&t.artist));
    if !t.album.trim().is_empty() {
        url.push_str(&format!("&album_name={}", encode(&t.album)));
    }
    if let Some(d) = t.duration {
        url.push_str(&format!("&duration={d}"));
    }
    url
}

/// The fallback search, with the title tidied ("Song - Remastered 2011" →
/// "Song").
pub fn search_url(t: &Track) -> String {
    use crate::nav::encode;
    format!("{API}/search?track_name={}&artist_name={}", encode(&clean_title(&t.title)), encode(&t.artist))
}

/// Drop the decorations players add to titles: " - Remastered 2011",
/// " (feat. Someone)", " [Live]".
pub fn clean_title(title: &str) -> String {
    const NOISE: [&str; 9] = ["remaster", "feat.", "ft.", "featuring", "live", "version", "edit", "mono", "stereo"];
    let noisy = |s: &str| {
        let s = s.to_lowercase();
        NOISE.iter().any(|n| s.contains(n))
    };
    let mut t = title.trim().to_string();
    if let Some((head, tail)) = t.split_once(" - ") {
        if noisy(tail) {
            t = head.to_string();
        }
    }
    for (open, close) in [('(', ')'), ('[', ']')] {
        while let Some(i) = t.find(open) {
            let Some(j) = t[i..].find(close) else { break };
            if !noisy(&t[i..i + j]) {
                break;
            }
            t = format!("{}{}", &t[..i], &t[i + j + 1..]);
        }
    }
    let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.is_empty() {
        title.trim().to_string()
    } else {
        t
    }
}

fn from_record(v: &serde_json::Value) -> Option<Lyrics> {
    if let Some(s) = v.get("syncedLyrics").and_then(|s| s.as_str()).filter(|s| !s.trim().is_empty()) {
        let l = parse_lrc(s);
        return (!l.is_empty()).then_some(l);
    }
    // Only plain lyrics: without times there is nothing to sync, so none.
    v.get("instrumental")
        .and_then(|i| i.as_bool())
        .unwrap_or(false)
        .then(|| Lyrics { lines: vec![], instrumental: true })
}

/// A `get` response: Ok(None) when the song has no synced lyrics.
pub fn parse_get(json: &str) -> Result<Option<Lyrics>, String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| format!("bad response: {e}"))?;
    if v.get("code").is_some() && v.get("id").is_none() {
        return Ok(None); // {"code":404,"name":"TrackNotFound",…}
    }
    Ok(from_record(&v))
}

/// A `search` response: the best record with synced lyrics, closest in
/// length to the song (within 3 s when the length is known).
pub fn parse_search(json: &str, t: &Track) -> Result<Option<Lyrics>, String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| format!("bad response: {e}"))?;
    let records = v.as_array().ok_or("bad response: not a list")?;
    let off = |r: &serde_json::Value| match (t.duration, r.get("duration").and_then(|d| d.as_f64())) {
        (Some(want), Some(have)) => (have - f64::from(want)).abs(),
        _ => 0.0,
    };
    Ok(records
        .iter()
        .filter(|r| r.get("syncedLyrics").and_then(|s| s.as_str()).is_some_and(|s| !s.trim().is_empty()))
        .filter(|r| off(r) <= 3.0)
        .min_by(|a, b| off(a).total_cmp(&off(b)))
        .and_then(from_record))
}

// ---- The lyric strip ---------------------------------------------------------

/// Space between lines on the strip, in px.
pub const STRIP_GAP: f32 = 26.0;
/// How long the strip takes to slide to the next line.
pub const SLIDE_SECS: f64 = 0.4;
/// A line too long for the pill scrolls across while it's sung, with this
/// much margin at each end.
const LONG_MARGIN: f32 = 10.0;

/// Where the strip is at a moment: `scroll` is the strip position (px from
/// the first line's start) at the window's left edge; the current line is
/// highlighted by `mix` (0..1, rising as the strip slides onto it), and the
/// line before it fades by `1 - mix`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Strip {
    pub scroll: f32,
    pub current: Option<usize>,
    pub mix: f32,
}

/// Where each line starts along the strip.
pub fn starts(widths: &[f32]) -> Vec<f32> {
    let mut x = 0.0;
    widths
        .iter()
        .map(|w| {
            let s = x;
            x += w + STRIP_GAP;
            s
        })
        .collect()
}

fn ease_out(f: f64) -> f32 {
    let f = f.clamp(0.0, 1.0);
    (1.0 - (1.0 - f).powi(3)) as f32
}

/// Where the strip rests while line `i` is sung: centred when it fits,
/// otherwise scrolling across it so the words being sung stay in view.
fn rest(lyrics: &Lyrics, i: usize, starts: &[f32], widths: &[f32], room: f32, pos: f64) -> f32 {
    let (s, w) = (starts[i], widths[i]);
    if w <= room {
        return s + w / 2.0 - room / 2.0;
    }
    let (from, to) = (s - LONG_MARGIN, s + w - room + LONG_MARGIN);
    let d = lyrics.duration_of(i);
    let at = lyrics.lines[i].at - LEAD;
    let f = ((pos - at - 0.1 * d) / (0.75 * d)).clamp(0.0, 1.0) as f32;
    from + (to - from) * f
}

/// The strip at `pos` seconds, for lines `widths` px wide in a window
/// `room` px wide.
pub fn strip(lyrics: &Lyrics, widths: &[f32], room: f32, pos: f64) -> Strip {
    if lyrics.lines.is_empty() || widths.len() != lyrics.lines.len() {
        return Strip { scroll: 0.0, current: None, mix: 0.0 };
    }
    let starts = starts(widths);
    match lyrics.current(pos) {
        // Before the first line: it waits in view, not yet highlighted.
        None => Strip { scroll: rest(lyrics, 0, &starts, widths, room, 0.0), current: None, mix: 0.0 },
        Some(i) => {
            let to = rest(lyrics, i, &starts, widths, room, pos);
            let since = pos + LEAD - lyrics.lines[i].at;
            let mix = ease_out(since / SLIDE_SECS);
            let from = if i == 0 {
                rest(lyrics, 0, &starts, widths, room, 0.0)
            } else {
                rest(lyrics, i - 1, &starts, widths, room, lyrics.lines[i].at - LEAD)
            };
            Strip { scroll: from + (to - from) * mix, current: Some(i), mix }
        }
    }
}

/// Whether the strip is sliding between lines (draw at full frame rate).
pub fn sliding(lyrics: &Lyrics, pos: f64) -> bool {
    lyrics.current(pos).is_some_and(|i| pos + LEAD - lyrics.lines[i].at < SLIDE_SECS)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SONG: &str = "[ti:Example]\n[ar:Someone]\n[00:01.00]First line\n[00:04.50][00:20.00]Chorus <00:05.00>with <00:05.50>words\n[00:09.00]\n[00:12.25]Third line\n[00:30.00]\n";

    #[test]
    fn parses_lrc() {
        let l = parse_lrc(SONG);
        let lines: Vec<(f64, &str)> = l.lines.iter().map(|l| (l.at, l.text.as_str())).collect();
        assert_eq!(
            lines,
            [
                (1.0, "First line"),
                (4.5, "Chorus with words"),
                (9.0, ""),
                (12.25, "Third line"),
                (20.0, "Chorus with words")
            ]
        );
        assert_eq!(l.text(2), PAUSE);
        assert_eq!(parse_lrc("[offset:+500]\n[00:02.00]a\n").lines[0].at, 1.5);
        assert_eq!(parse_lrc("[01:02:50]x\n").lines[0].at, 62.5);
        assert!(parse_lrc("no times here\n[ar:x]").is_empty());
        assert_eq!(strip_word_stamps("a <b> c"), "a <b> c", "angle brackets that aren't stamps stay");
    }

    #[test]
    fn current_line_and_durations() {
        let l = parse_lrc(SONG);
        assert_eq!(l.current(0.5), None);
        assert_eq!(l.current(1.0 - LEAD), Some(0));
        assert_eq!(l.current(4.0), Some(0));
        assert_eq!(l.current(4.6), Some(1));
        assert_eq!(l.current(500.0), Some(4));
        assert_eq!(l.duration_of(0), 3.5);
        assert_eq!(l.duration_of(4), LAST_LINE_SECS);
    }

    fn track(duration: Option<u32>) -> Track {
        Track { title: "Hey & Ho".into(), artist: "The Band".into(), album: "".into(), duration, file: None }
    }

    #[test]
    fn request_urls() {
        assert_eq!(
            get_url(&track(Some(201))),
            "https://lrclib.net/api/get?track_name=Hey+%26+Ho&artist_name=The+Band&duration=201"
        );
        let t = Track { album: "Best Of".into(), ..track(None) };
        assert_eq!(
            get_url(&t),
            "https://lrclib.net/api/get?track_name=Hey+%26+Ho&artist_name=The+Band&album_name=Best+Of"
        );
        let t = Track { title: "Song - Remastered 2011".into(), ..track(None) };
        assert_eq!(search_url(&t), "https://lrclib.net/api/search?track_name=Song&artist_name=The+Band");
        assert_eq!(clean_title("Song (feat. X) [Live]"), "Song");
        assert_eq!(clean_title("Song (Part 2)"), "Song (Part 2)");
        assert_eq!(clean_title("Live - Forever"), "Live - Forever", "only decorations go");
        assert_eq!(track(None).key(), Track { title: " hey & ho".into(), ..track(Some(3)) }.key());
    }

    #[test]
    fn local_file_names() {
        let t = Track { title: "What? / Why".into(), artist: "AC/DC".into(), ..track(None) };
        assert_eq!(local_names(&t), ["acdc - what  why.lrc", "what  why.lrc"]);
        assert_eq!(beside("/music/a/Song.flac"), std::path::PathBuf::from("/music/a/Song.lrc"));
    }

    #[test]
    fn parses_responses() {
        let ok = r#"{"id":3396226,"trackName":"Hey","artistName":"The Band","albumName":"A","duration":233,
            "instrumental":false,"plainLyrics":"First line\nSecond","syncedLyrics":"[00:17.12] First line\n[00:20.00] Second"}"#;
        let l = parse_get(ok).unwrap().unwrap();
        assert_eq!(l.lines.len(), 2);
        assert_eq!(l.lines[0].text, "First line");
        let missing = r#"{"code":404,"name":"TrackNotFound","message":"Failed to find specified track"}"#;
        assert_eq!(parse_get(missing).unwrap(), None);
        let plain = r#"{"id":1,"instrumental":false,"plainLyrics":"words","syncedLyrics":null}"#;
        assert_eq!(parse_get(plain).unwrap(), None, "unsynced lyrics can't follow the song");
        let inst = r#"{"id":2,"instrumental":true,"plainLyrics":null,"syncedLyrics":null}"#;
        assert!(parse_get(inst).unwrap().unwrap().instrumental);
        assert!(parse_get("<html>").is_err());

        let search = r#"[
            {"id":1,"duration":180,"syncedLyrics":"[00:01.00]far"},
            {"id":2,"duration":201,"syncedLyrics":null,"plainLyrics":"x"},
            {"id":3,"duration":202,"syncedLyrics":"[00:01.00]close"}]"#;
        let best = parse_search(search, &track(Some(201))).unwrap().unwrap();
        assert_eq!(best.lines[0].text, "close");
        assert_eq!(parse_search(search, &track(Some(400))).unwrap(), None, "nothing near that length");
        assert_eq!(parse_search(search, &track(None)).unwrap().unwrap().lines[0].text, "far");
    }

    #[test]
    fn strip_centres_slides_and_scrolls_long_lines() {
        let l = parse_lrc("[00:01.00]a\n[00:05.00]b\n[00:09.00]long\n");
        let widths = [60.0, 80.0, 400.0];
        let room = 200.0;
        // Before the first line: line 0 centred, nothing highlighted.
        let s = strip(&l, &widths, room, 0.0);
        assert_eq!((s.current, s.scroll), (None, 30.0 - 100.0));
        // Line 0 sung: centred and fully highlighted once the slide ends.
        let s = strip(&l, &widths, room, 3.0);
        assert_eq!((s.current, s.mix, s.scroll), (Some(0), 1.0, -70.0));
        // Sliding onto line 1 (starts at 60 + gap).
        let mid = strip(&l, &widths, room, 5.0 - LEAD + SLIDE_SECS / 2.0);
        let end = 60.0 + STRIP_GAP + 40.0 - 100.0;
        assert!(mid.scroll > -70.0 && mid.scroll < end && mid.mix > 0.5 && mid.mix < 1.0, "{mid:?}");
        assert!(sliding(&l, 5.0) && !sliding(&l, 7.0));
        assert_eq!(strip(&l, &widths, room, 7.0).scroll, end);
        // A long line scrolls across while it's sung.
        let s2 = 60.0 + STRIP_GAP + 80.0 + STRIP_GAP;
        let a = strip(&l, &widths, room, 10.0).scroll;
        let b = strip(&l, &widths, room, 13.0).scroll;
        assert!(a < b && b <= s2 + 400.0 - room + LONG_MARGIN, "{a} {b}");
        assert_eq!(strip(&l, &widths, room, 60.0).scroll, s2 + 400.0 - room + LONG_MARGIN);
    }
}
