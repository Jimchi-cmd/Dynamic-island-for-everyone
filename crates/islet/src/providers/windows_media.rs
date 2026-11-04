//! Windows: the system media session (SMTC / GlobalSystemMediaTransport-
//! Controls), the same source as the volume flyout's media controls. Covers
//! Spotify, Apple Music for Windows, browsers, Media Player and anything
//! else that publishes a media session.
//!
//! Event-driven: CurrentSessionChanged, MediaPropertiesChanged,
//! PlaybackInfoChanged and TimelinePropertiesChanged wake one provider
//! thread, which reads the session and reports. Album art comes from the
//! session's thumbnail stream (local; nothing is downloaded).

use super::{art, debug_log, Sink};
use islet_core::activity::{ArtImage, MediaCaps, MediaCommand, MediaInfo, Report};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{SystemTime, UNIX_EPOCH};
use windows::Foundation::TypedEventHandler;
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session, GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
};
use windows::Storage::Streams::DataReader;
use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};

enum Msg {
    SessionChanged,
    Refresh,
    Command(MediaCommand),
}

/// 100-ns ticks between 1601-01-01 (Windows epoch) and 1970-01-01.
const EPOCH_DIFF_TICKS: i64 = 116_444_736_000_000_000;

fn now_ticks() -> i64 {
    let since_unix = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    EPOCH_DIFF_TICKS + (since_unix.as_nanos() / 100) as i64
}

/// "Spotify.exe" → "Spotify"; "SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify" → "Spotify".
pub fn app_name(aumid: &str) -> String {
    let id = aumid.rsplit('!').next().unwrap_or(aumid);
    let id = id.strip_suffix(".exe").or_else(|| id.strip_suffix(".EXE")).unwrap_or(id);
    let last = id.rsplit('.').next().unwrap_or(id);
    match last.to_ascii_lowercase().as_str() {
        "zunemusic" | "media" => "Media Player".into(),
        "msedge" => "Microsoft Edge".into(),
        "chrome" => "Chrome".into(),
        "firefox" => "Firefox".into(),
        "applemusic" => "Apple Music".into(),
        _ if last.is_empty() => "Media".into(),
        _ => last.to_string(),
    }
}

fn read_thumbnail(
    props: &windows::Media::Control::GlobalSystemMediaTransportControlsSessionMediaProperties,
) -> Option<Vec<u8>> {
    let stream = props.Thumbnail().ok()?.OpenReadAsync().ok()?.join().ok()?;
    let size = u32::try_from(stream.Size().ok()?).ok()?;
    if size == 0 || size > 8 * 1024 * 1024 {
        return None;
    }
    let reader = DataReader::CreateDataReader(&stream.GetInputStreamAt(0).ok()?).ok()?;
    reader.LoadAsync(size).ok()?.join().ok()?;
    let mut buf = vec![0u8; size as usize];
    reader.ReadBytes(&mut buf).ok()?;
    Some(buf)
}

struct Reader {
    art: Option<(String, Option<ArtImage>)>,
}

impl Reader {
    fn snapshot(&mut self, s: &Session) -> windows::core::Result<MediaInfo> {
        let props = s.TryGetMediaPropertiesAsync()?.join()?;
        let info = s.GetPlaybackInfo()?;
        let controls = info.Controls()?;
        let timeline = s.GetTimelineProperties()?;
        let playing = info.PlaybackStatus()? == Status::Playing;
        let (title, artist, album) =
            (props.Title()?.to_string(), props.Artist()?.to_string(), props.AlbumTitle()?.to_string());

        let key = format!("{title}\u{1f}{artist}\u{1f}{album}");
        let art = match &self.art {
            Some((k, img)) if *k == key => img.clone(),
            _ => {
                let img = read_thumbnail(&props).and_then(|b| art::decode(&b));
                self.art = Some((key, img.clone()));
                img
            }
        };

        // Timeline values are as of LastUpdatedTime; bring them up to now.
        let end = timeline.EndTime()?.Duration;
        let mut pos = timeline.Position()?.Duration;
        let updated = timeline.LastUpdatedTime()?.UniversalTime;
        if playing && updated > 0 {
            pos += (now_ticks() - updated).max(0);
        }
        let ticks = |t: i64| t as f64 / 1e7;
        Ok(MediaInfo {
            source: s.SourceAppUserModelId()?.to_string(),
            app_name: app_name(&s.SourceAppUserModelId()?.to_string()),
            title,
            artist,
            album,
            playing,
            position: (end > 0).then(|| ticks(pos.clamp(0, end))),
            position_at: 0.0,
            duration: (end > 0).then(|| ticks(end)),
            rate: 1.0,
            art,
            caps: MediaCaps {
                play_pause: controls.IsPlayPauseToggleEnabled()?
                    || controls.IsPlayEnabled()?
                    || controls.IsPauseEnabled()?,
                next: controls.IsNextEnabled()?,
                previous: controls.IsPreviousEnabled()?,
                seek: controls.IsPlaybackPositionEnabled()?,
            },
        })
    }
}

fn command(s: &Session, cmd: MediaCommand) -> windows::core::Result<bool> {
    match cmd {
        MediaCommand::PlayPause => s.TryTogglePlayPauseAsync()?.join(),
        MediaCommand::Next => s.TrySkipNextAsync()?.join(),
        MediaCommand::Previous => s.TrySkipPreviousAsync()?.join(),
        MediaCommand::SeekTo(secs) => s.TryChangePlaybackPositionAsync((secs.max(0.0) * 1e7) as i64)?.join(),
    }
}

struct Subscription {
    session: Session,
    tokens: [i64; 3],
}

impl Drop for Subscription {
    fn drop(&mut self) {
        let _ = self.session.RemoveMediaPropertiesChanged(self.tokens[0]);
        let _ = self.session.RemovePlaybackInfoChanged(self.tokens[1]);
        let _ = self.session.RemoveTimelinePropertiesChanged(self.tokens[2]);
    }
}

fn subscribe(session: Session, tx: &Sender<Msg>) -> windows::core::Result<Subscription> {
    let (a, b, c) = (tx.clone(), tx.clone(), tx.clone());
    let tokens = [
        session.MediaPropertiesChanged(&TypedEventHandler::new(move |_, _| {
            let _ = a.send(Msg::Refresh);
            Ok(())
        }))?,
        session.PlaybackInfoChanged(&TypedEventHandler::new(move |_, _| {
            let _ = b.send(Msg::Refresh);
            Ok(())
        }))?,
        session.TimelinePropertiesChanged(&TypedEventHandler::new(move |_, _| {
            let _ = c.send(Msg::Refresh);
            Ok(())
        }))?,
    ];
    Ok(Subscription { session, tokens })
}

fn run(sink: Sink, tx: Sender<Msg>, rx: Receiver<Msg>) -> windows::core::Result<()> {
    // SAFETY: initialises WinRT for this thread (multithreaded apartment).
    let _ = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };
    let manager = Manager::RequestAsync()?.join()?;
    {
        let tx = tx.clone();
        manager.CurrentSessionChanged(&TypedEventHandler::new(move |_, _| {
            let _ = tx.send(Msg::SessionChanged);
            Ok(())
        }))?;
    }
    let mut sub: Option<Subscription> = None;
    let mut reader = Reader { art: None };
    let _ = tx.send(Msg::SessionChanged);
    while let Ok(first) = rx.recv() {
        // Events arrive in bursts (track change = several); handle them once.
        let mut session_changed = false;
        let mut pending = vec![first];
        pending.extend(rx.try_iter());
        for msg in pending {
            match msg {
                Msg::SessionChanged => session_changed = true,
                Msg::Refresh => {}
                Msg::Command(cmd) => {
                    if let Some(s) = &sub {
                        if let Err(e) = command(&s.session, cmd) {
                            debug_log(&format!("media command failed: {e}"));
                        }
                    }
                }
            }
        }
        if session_changed {
            sub = None; // unsubscribes the previous session
            if let Ok(s) = manager.GetCurrentSession() {
                sub = subscribe(s, &tx).ok();
            }
        }
        let report = match &sub {
            Some(s) => match reader.snapshot(&s.session) {
                Ok(m) => Report::Media(Some(m)),
                Err(e) => {
                    debug_log(&format!("reading the media session failed: {e}"));
                    continue;
                }
            },
            None => Report::Media(None),
        };
        sink.send(report);
    }
    Ok(())
}

pub fn start(sink: Sink) -> Option<Sender<MediaCommand>> {
    let (tx, rx) = channel::<Msg>();
    let (cmd_tx, cmd_rx) = channel::<MediaCommand>();
    let forward = tx.clone();
    std::thread::Builder::new()
        .name("islet-smtc-cmd".into())
        .spawn(move || {
            for cmd in cmd_rx {
                if forward.send(Msg::Command(cmd)).is_err() {
                    break;
                }
            }
        })
        .ok()?;
    std::thread::Builder::new()
        .name("islet-smtc".into())
        .spawn(move || {
            if let Err(e) = run(sink, tx, rx) {
                debug_log(&format!("media sessions unavailable: {e}"));
            }
        })
        .ok()?;
    Some(cmd_tx)
}

#[cfg(test)]
mod tests {
    use super::app_name;

    #[test]
    fn friendly_app_names() {
        assert_eq!(app_name("Spotify.exe"), "Spotify");
        assert_eq!(app_name("SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify"), "Spotify");
        assert_eq!(app_name("Microsoft.ZuneMusic_8wekyb3d8bbwe!Microsoft.ZuneMusic"), "Media Player");
        assert_eq!(app_name("MSEdge"), "Microsoft Edge");
    }
}
