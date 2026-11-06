//! macOS: Now Playing for every app (browsers, Spotify, Music, podcasts…),
//! with artwork, through Islet's MediaRemote helper (crates/islet-mediaremote).
//!
//! macOS 15.4+ only lets entitled clients read MediaRemote. The system perl
//! still can, so the helper library is loaded into `/usr/bin/perl` and talks
//! to Islet over a pipe: Now Playing records on stdout, commands on stdin.
//! When Islet exits, the helper sees EOF and exits too.
//!
//! If the helper is missing or MediaRemote refuses it, this falls back to
//! the Spotify / Apple Music notifications (macos_media.rs).

use super::{art, debug_log, macos_media, Sink};
use islet_core::activity::{ArtImage, MediaCaps, MediaCommand, MediaInfo, Report};
use islet_mediaremote::wire;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};

/// Perl that loads the helper library and calls its entry point (an XSUB
/// that never returns).
const LOADER: &str = concat!(
    "use DynaLoader;",
    "my $l = DynaLoader::dl_load_file($ENV{ISLET_MR_LIB}, 0) or die DynaLoader::dl_error();",
    "my $s = DynaLoader::dl_find_symbol($l, 'islet_mediaremote_main') or die DynaLoader::dl_error();",
    "my $x = DynaLoader::dl_install_xsub('main::islet_mediaremote', $s);",
    "&$x();"
);

const LIB_NAME: &str = "libislet_mediaremote.dylib";

/// Where the helper library lives: `ISLET_MR_LIB`, next to the executable,
/// cargo's `deps/` folder (development), or the app bundle's Resources.
pub fn find_helper() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("ISLET_MR_LIB").map(PathBuf::from) {
        return p.is_file().then_some(p);
    }
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    [dir.join(LIB_NAME), dir.join("deps").join(LIB_NAME), dir.join("../Resources").join(LIB_NAME)]
        .into_iter()
        .find(|p| p.is_file())
}

fn spawn(lib: &PathBuf, probe: bool) -> std::io::Result<Child> {
    let mut cmd = Command::new("/usr/bin/perl");
    cmd.arg("-e")
        .arg(LOADER)
        .env("ISLET_MR_LIB", lib)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if probe {
        cmd.env("ISLET_MR_MODE", "probe");
    }
    cmd.spawn()
}

/// `islet --mediaremote-probe`: does the perl route work on this Mac?
/// Prints the helper's verdict; returns true when MediaRemote answered.
pub fn probe() -> bool {
    let Some(lib) = find_helper() else {
        println!("mediaremote: helper library {LIB_NAME} not found");
        return false;
    };
    match spawn(&lib, true).and_then(|c| c.wait_with_output()) {
        Ok(out) => {
            let text = String::from_utf8_lossy(&out.stdout);
            println!("mediaremote: {}", text.trim());
            text.starts_with("P\tok")
        }
        Err(e) => {
            println!("mediaremote: could not run /usr/bin/perl: {e}");
            false
        }
    }
}

/// Friendly name from a bundle identifier.
pub fn app_name(bundle: &str) -> String {
    match bundle {
        "com.spotify.client" => "Spotify".into(),
        "com.apple.Music" => "Music".into(),
        "com.apple.Safari" => "Safari".into(),
        "com.google.Chrome" => "Chrome".into(),
        "org.mozilla.firefox" => "Firefox".into(),
        "com.microsoft.edgemac" => "Microsoft Edge".into(),
        "com.apple.podcasts" => "Podcasts".into(),
        "com.apple.TV" => "TV".into(),
        "" => "Now Playing".into(),
        other => {
            let last = other.rsplit('.').next().unwrap_or(other);
            let mut c = last.chars();
            c.next().map_or_else(String::new, |f| f.to_uppercase().collect::<String>() + c.as_str())
        }
    }
}

/// Parse an `R` record. `art` resolves the record's artwork (cached by id).
pub fn parse_record(line: &str, art: &mut ArtCache) -> MediaInfo {
    let fields = wire::fields(line);
    let get = |k: &str| fields.iter().find(|(key, _)| key == k).map(|(_, v)| v.as_str());
    let num = |k: &str| get(k).and_then(|v| v.parse::<f64>().ok());
    let bundle = get("bundle").unwrap_or_default().to_string();
    let rate = num("rate").unwrap_or(1.0);
    MediaInfo {
        source: bundle.clone(),
        app_name: app_name(&bundle),
        title: get("title").unwrap_or_default().to_string(),
        artist: get("artist").unwrap_or_default().to_string(),
        album: get("album").unwrap_or_default().to_string(),
        playing: get("playing") == Some("1"),
        position: num("elapsed"),
        position_at: 0.0,
        duration: num("duration").filter(|d| *d > 0.0),
        rate: if rate > 0.0 { rate } else { 1.0 },
        art: art.resolve(get("art_id"), get("art")),
        file: None,
        caps: MediaCaps { play_pause: true, next: true, previous: true, seek: true },
    }
}

/// The most recent artwork, keyed by the helper's art id.
#[derive(Default)]
pub struct ArtCache {
    id: Option<String>,
    image: Option<ArtImage>,
}

impl ArtCache {
    fn resolve(&mut self, id: Option<&str>, data: Option<&str>) -> Option<ArtImage> {
        let id = id?;
        if let Some(b64) = data {
            self.image = wire::base64_decode(b64).and_then(|bytes| art::decode(&bytes));
            self.id = Some(id.to_string());
        }
        (self.id.as_deref() == Some(id)).then(|| self.image.clone()).flatten()
    }
}

/// Where commands go: the helper's stdin, or the fallback provider.
enum Route {
    Helper(ChildStdin),
    Fallback(Sender<MediaCommand>),
    None,
}

fn command_line(cmd: MediaCommand) -> String {
    match cmd {
        MediaCommand::PlayPause => "toggle".into(),
        MediaCommand::Next => "next".into(),
        MediaCommand::Previous => "previous".into(),
        MediaCommand::SeekTo(s) => format!("seek {:.3}", s.max(0.0)),
    }
}

pub fn start(sink: Sink) -> Option<Sender<MediaCommand>> {
    let route = Arc::new(Mutex::new(Route::None));
    let fallback = {
        let (route, sink) = (route.clone(), sink.clone());
        move |why: &str| {
            debug_log(&format!("MediaRemote helper unavailable ({why}); using the Spotify/Music listener"));
            if let Some(tx) = macos_media::start(sink.clone()) {
                *route.lock().unwrap() = Route::Fallback(tx);
            }
        }
    };

    match find_helper().map(|lib| spawn(&lib, false)) {
        Some(Ok(mut child)) => {
            let stdout = child.stdout.take();
            if let Some(stdin) = child.stdin.take() {
                *route.lock().unwrap() = Route::Helper(stdin);
            }
            let sink = sink.clone();
            let _ = std::thread::Builder::new().name("islet-mediaremote".into()).spawn(move || {
                let _child = child; // keep the handle for the helper's lifetime
                let mut art = ArtCache::default();
                let mut working = false;
                if let Some(out) = stdout {
                    for line in BufReader::new(out).lines().map_while(Result::ok) {
                        match line.chars().next() {
                            Some('R') => {
                                working = true;
                                sink.send(Report::Media(Some(parse_record(&line, &mut art))));
                            }
                            Some('N') => {
                                working = true;
                                sink.send(Report::Media(None));
                            }
                            Some('E') => {
                                debug_log(&format!("MediaRemote helper: {}", wire::unescape(&line[1..]).trim()))
                            }
                            _ => {}
                        }
                    }
                }
                if !working {
                    fallback("the helper exited without reporting");
                } else {
                    debug_log("MediaRemote helper exited");
                }
            });
        }
        Some(Err(e)) => fallback(&format!("perl failed to start: {e}")),
        None => fallback("helper library not found"),
    }

    let (tx, rx) = channel::<MediaCommand>();
    std::thread::Builder::new()
        .name("islet-media-cmd".into())
        .spawn(move || {
            for cmd in rx {
                match &mut *route.lock().unwrap() {
                    Route::Helper(stdin) => {
                        let _ = writeln!(stdin, "{}", command_line(cmd)).and_then(|_| stdin.flush());
                    }
                    Route::Fallback(tx) => {
                        let _ = tx.send(cmd);
                    }
                    Route::None => {}
                }
            }
        })
        .ok()?;
    Some(tx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_records_and_caches_art() {
        let mut cache = ArtCache::default();
        let line = "R\ttitle=Song\tartist=Band\tplaying=1\trate=1\telapsed=12.5\tduration=200\tbundle=com.apple.Safari\tart_id=ab";
        let m = parse_record(line, &mut cache);
        assert_eq!((m.title.as_str(), m.artist.as_str(), m.app_name.as_str()), ("Song", "Band", "Safari"));
        assert!(m.playing);
        assert_eq!((m.position, m.duration), (Some(12.5), Some(200.0)));
        assert!(m.art.is_none(), "art id without data and nothing cached");
    }

    #[test]
    fn friendly_names_and_commands() {
        assert_eq!(app_name("com.spotify.client"), "Spotify");
        assert_eq!(app_name("com.example.radioplayer"), "Radioplayer");
        assert_eq!(command_line(MediaCommand::SeekTo(42.0)), "seek 42.000");
        assert_eq!(command_line(MediaCommand::PlayPause), "toggle");
    }
}
