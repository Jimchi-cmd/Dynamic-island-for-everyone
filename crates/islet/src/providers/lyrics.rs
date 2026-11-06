//! Synced lyrics, only while the user has lyrics on and only for the song
//! that is playing: first from an .lrc file beside the song or in Islet's
//! `lyrics` folder (no network at all), else from LRCLIB. Results (including "this song has none")
//! are kept in memory for the session, so replaying a song costs nothing.
//! The thread sleeps on its channel: no wakeups while nothing changes.

use super::{debug_log, Sink};
use islet_core::activity::Report;
use islet_core::lyrics::{self, Lyrics, Track};
use std::collections::{HashMap, VecDeque};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::Duration;

/// Songs remembered (lyrics are a few KB each).
const CACHE: usize = 64;
/// Wait this long after a song change before asking, so skipping through a
/// playlist doesn't send a request per song.
const SETTLE: Duration = Duration::from_millis(600);

/// LRCLIB asks apps to say who they are, with a link.
fn user_agent() -> String {
    format!("Islet/{} ({})", env!("CARGO_PKG_VERSION"), env!("CARGO_PKG_REPOSITORY"))
}

fn call(url: &str) -> Result<(u16, String), String> {
    let mut resp =
        crate::net::agent().get(url).header("User-Agent", &user_agent()).call().map_err(|e| format!("network: {e}"))?;
    let status = resp.status().as_u16();
    let body = resp.body_mut().with_config().limit(1 << 20).read_to_string().map_err(|e| format!("network: {e}"))?;
    Ok((status, body))
}

/// An .lrc beside the song's file, or in the `lyrics` folder of Islet's
/// settings folder ("Artist - Title.lrc" or "Title.lrc", any case).
pub fn local(t: &Track) -> Option<Lyrics> {
    let read = |p: &std::path::Path| {
        let l = lyrics::parse_lrc(&std::fs::read_to_string(p).ok()?);
        (!l.is_empty()).then_some(l)
    };
    if let Some(l) = t.file.as_deref().and_then(|f| read(&lyrics::beside(f))) {
        return Some(l);
    }
    let dir = islet_core::paths::config_dir()?.join("lyrics");
    let names = lyrics::local_names(t);
    let entries: Vec<_> = std::fs::read_dir(&dir).ok()?.flatten().collect();
    names.iter().find_map(|want| {
        entries.iter().find(|e| e.file_name().to_string_lossy().to_lowercase() == *want).and_then(|e| read(&e.path()))
    })
}

/// Exact match first, then a search with a tidied title.
pub fn fetch(t: &Track) -> Result<Option<Lyrics>, String> {
    let (status, body) = call(&lyrics::get_url(t))?;
    match status {
        200 => {
            if let Some(l) = lyrics::parse_get(&body)? {
                return Ok(Some(l));
            }
        }
        404 => {}
        s => return Err(format!("LRCLIB answered {s}")),
    }
    let (status, body) = call(&lyrics::search_url(t))?;
    match status {
        200 => lyrics::parse_search(&body, t),
        s => Err(format!("LRCLIB answered {s}")),
    }
}

/// Send the song that is playing (None: nothing, or lyrics are off).
pub fn start(sink: Sink) -> Sender<Option<Track>> {
    let (tx, rx) = channel::<Option<Track>>();
    let _ = std::thread::Builder::new().name("islet-lyrics".into()).spawn(move || run(&rx, &sink));
    tx
}

fn run(rx: &Receiver<Option<Track>>, sink: &Sink) {
    let mut cache: HashMap<String, Option<Arc<Lyrics>>> = HashMap::new();
    let mut order: VecDeque<String> = VecDeque::new();
    while let Ok(mut want) = rx.recv() {
        // Settle: keep taking newer requests until none arrives for a moment.
        loop {
            match rx.recv_timeout(SETTLE) {
                Ok(newer) => want = newer,
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
        let Some(track) = want else { continue };
        let key = track.key();
        // Your own files first (and always: you may have just added one).
        if let Some(l) = local(&track) {
            sink.send(Report::Lyrics(key, Some(Arc::new(l))));
            continue;
        }
        if let Some(hit) = cache.get(&key) {
            sink.send(Report::Lyrics(key, hit.clone()));
            continue;
        }
        match fetch(&track) {
            Ok(found) => {
                let found = found.map(Arc::new);
                if order.len() >= CACHE {
                    if let Some(old) = order.pop_front() {
                        cache.remove(&old);
                    }
                }
                order.push_back(key.clone());
                cache.insert(key.clone(), found.clone());
                sink.send(Report::Lyrics(key, found));
            }
            // Not remembered: it may work next time the song comes round.
            Err(e) => debug_log(&format!("lyrics: {e}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_lrc_beside_the_song_is_used() {
        let dir = std::env::temp_dir().join(format!("islet-lyrics-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let song = dir.join("Song.flac");
        std::fs::write(dir.join("Song.lrc"), "[00:01.00]From my own file\n").unwrap();
        let t = Track {
            title: "Song".into(),
            artist: "Me".into(),
            album: String::new(),
            duration: None,
            file: Some(song.to_string_lossy().into_owned()),
        };
        let l = local(&t).expect("found beside the song");
        assert_eq!(l.lines[0].text, "From my own file");
        let none = Track { file: Some(dir.join("Other.flac").to_string_lossy().into_owned()), ..t };
        assert!(none.file.as_deref().map(lyrics::beside).is_some_and(|p| !p.exists()));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
