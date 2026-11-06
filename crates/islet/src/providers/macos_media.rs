//! macOS: Spotify and Apple Music, from the distributed notifications both
//! apps post on every play, pause and track change. Listening needs no
//! permission and costs nothing while nothing changes.
//!
//! Controls go through AppleScript (`osascript`), which makes macOS ask
//! once for permission to control the app (Automation).
//!
//! Limits, until the MediaRemote adapter lands (docs/roadmap.md): only
//! these two apps (not browsers), no album art, and a session already
//! playing at launch appears with the next play/pause or track change.

use super::{debug_log, Sink};
use block2::RcBlock;
use islet_core::activity::{MediaCaps, MediaCommand, MediaInfo, Report};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_foundation::{
    NSDictionary, NSDistributedNotificationCenter, NSNotification, NSNumber, NSObjectProtocol, NSOperationQueue,
    NSString,
};
use std::ptr::NonNull;
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum App {
    Spotify,
    Music,
}

impl App {
    fn script_name(self) -> &'static str {
        match self {
            App::Spotify => "Spotify",
            App::Music => "Music",
        }
    }
}

const SOURCES: [(App, &str); 2] =
    [(App::Spotify, "com.spotify.client.PlaybackStateChanged"), (App::Music, "com.apple.Music.playerInfo")];

fn string(info: &NSDictionary, key: &str) -> Option<String> {
    let value = info.objectForKey(&NSString::from_str(key))?;
    value.downcast_ref::<NSString>().map(|s| s.to_string())
}

fn number(info: &NSDictionary, key: &str) -> Option<f64> {
    let value = info.objectForKey(&NSString::from_str(key))?;
    value.downcast_ref::<NSNumber>().map(|n| n.doubleValue())
}

/// Parse a player notification's userInfo. Spotify reports the duration in
/// milliseconds ("Duration"), Music in milliseconds ("Total Time").
pub fn parse(app: App, info: &NSDictionary) -> Option<MediaInfo> {
    let state = string(info, "Player State").unwrap_or_default();
    if state == "Stopped" {
        return None;
    }
    let duration_ms = number(info, "Duration").or_else(|| number(info, "Total Time"));
    Some(MediaInfo {
        source: app.script_name().into(),
        app_name: if app == App::Music { "Music".into() } else { "Spotify".into() },
        title: string(info, "Name").unwrap_or_default(),
        artist: string(info, "Artist").unwrap_or_default(),
        album: string(info, "Album").unwrap_or_default(),
        playing: state == "Playing",
        position: number(info, "Playback Position"),
        position_at: 0.0,
        duration: duration_ms.map(|ms| ms / 1000.0).filter(|d| *d > 0.0),
        rate: 1.0,
        art: None,
        caps: MediaCaps { play_pause: true, next: true, previous: true, seek: true },
    })
}

/// The AppleScript for a command.
pub fn script(app: App, cmd: MediaCommand) -> String {
    let verb = match cmd {
        MediaCommand::PlayPause => "playpause".to_string(),
        MediaCommand::Next => "next track".to_string(),
        MediaCommand::Previous => "previous track".to_string(),
        MediaCommand::SeekTo(s) => format!("set player position to {:.1}", s.max(0.0)),
    };
    format!("tell application \"{}\" to {verb}", app.script_name())
}

/// Register the observers (call on the main thread; they fire there while
/// the event loop runs). Returns the command channel.
pub fn start(sink: Sink) -> Option<Sender<MediaCommand>> {
    let last: Arc<Mutex<Option<App>>> = Arc::new(Mutex::new(None));
    let center = NSDistributedNotificationCenter::defaultCenter();
    let queue = NSOperationQueue::mainQueue();
    let mut observers: Vec<Retained<ProtocolObject<dyn NSObjectProtocol>>> = Vec::new();
    for (app, name) in SOURCES {
        let (sink, last) = (sink.clone(), last.clone());
        let block = RcBlock::new(move |note: NonNull<NSNotification>| {
            // SAFETY: AppKit passes a valid notification for the block's duration.
            let note = unsafe { note.as_ref() };
            let Some(info) = note.userInfo() else { return };
            let media = parse(app, &info);
            *last.lock().unwrap() = Some(app);
            sink.send(Report::Media(media));
        });
        // SAFETY: a valid name and block; the observer stays registered for
        // the process lifetime (kept alive below).
        let observer = unsafe {
            center.addObserverForName_object_queue_usingBlock(
                Some(&NSString::from_str(name)),
                None,
                Some(&queue),
                &block,
            )
        };
        observers.push(observer);
    }
    std::mem::forget(observers);

    let (tx, rx) = channel::<MediaCommand>();
    std::thread::Builder::new()
        .name("islet-media-cmd".into())
        .spawn(move || {
            for cmd in rx {
                let Some(app) = *last.lock().unwrap() else { continue };
                let status = std::process::Command::new("/usr/bin/osascript").args(["-e", &script(app, cmd)]).status();
                if !status.as_ref().is_ok_and(|s| s.success()) {
                    debug_log(&format!(
                        "osascript failed for {cmd:?} ({status:?}); is Automation allowed in System Settings?"
                    ));
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
    fn scripts() {
        assert_eq!(script(App::Spotify, MediaCommand::PlayPause), "tell application \"Spotify\" to playpause");
        assert_eq!(
            script(App::Music, MediaCommand::SeekTo(42.0)),
            "tell application \"Music\" to set player position to 42.0"
        );
    }
}
