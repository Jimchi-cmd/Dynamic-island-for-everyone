//! Optional Spotify Web API: playback on the user's other devices (Spotify
//! Connect: a phone, a speaker) shown on the island, with controls.
//!
//! Bring your own Spotify app: the user registers one at
//! developer.spotify.com with the redirect URI below, and signs in with
//! PKCE in the browser (Islet never sees the password). The refresh token
//! lives in the OS keychain. The Web API can't push changes, so this is the
//! one feature that polls: every 10 s while something plays, every 60 s
//! otherwise, and only while it's on.

use super::{debug_log, Sink};
use crate::secrets;
use islet_core::activity::{MediaCaps, MediaCommand, MediaInfo, Report};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

pub const REDIRECT_URI: &str = "http://127.0.0.1:43821/callback";
const PORT: u16 = 43821;
const AUTH: &str = "https://accounts.spotify.com/authorize";
const TOKEN: &str = "https://accounts.spotify.com/api/token";
const API: &str = "https://api.spotify.com/v1/me/player";
const SCOPES: &str = "user-read-playback-state user-modify-playback-state";
pub const SOURCE: &str = "spotify-connect";

fn b64url(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, b)| n | (u32::from(*b) << (16 - 8 * i)));
        for i in 0..=chunk.len() {
            out.push(A[(n >> (18 - 6 * i) & 63) as usize] as char);
        }
    }
    out
}

fn random(n: usize) -> Result<String, String> {
    let mut buf = vec![0u8; n];
    getrandom::fill(&mut buf).map_err(|e| e.to_string())?;
    Ok(b64url(&buf))
}

/// PKCE: the S256 challenge for a verifier.
pub fn challenge(verifier: &str) -> String {
    use sha2::Digest;
    b64url(&sha2::Sha256::digest(verifier.as_bytes()))
}

fn form(pairs: &[(&str, &str)]) -> String {
    pairs.iter().map(|(k, v)| format!("{k}={}", enc(v))).collect::<Vec<_>>().join("&")
}

fn enc(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

struct Creds {
    client_id: String,
    refresh_token: String,
}

fn load() -> Option<Creds> {
    let v: serde_json::Value = serde_json::from_str(&secrets::spotify()?).ok()?;
    Some(Creds {
        client_id: v["client_id"].as_str()?.to_string(),
        refresh_token: v["refresh_token"].as_str()?.to_string(),
    })
}

fn save(c: &Creds) -> Result<(), String> {
    secrets::set_spotify(&serde_json::json!({ "client_id": c.client_id, "refresh_token": c.refresh_token }).to_string())
}

fn token_request(body: String) -> Result<serde_json::Value, String> {
    let mut resp = crate::net::agent()
        .post(TOKEN)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .send(body)
        .map_err(|e| format!("network: {e}"))?;
    let v: serde_json::Value = serde_json::from_str(&resp.body_mut().read_to_string().map_err(|e| e.to_string())?)
        .map_err(|e| format!("bad response: {e}"))?;
    if let Some(err) = v.get("error") {
        return Err(format!(
            "Spotify: {}",
            v["error_description"].as_str().or(err.as_str()).unwrap_or("sign-in failed")
        ));
    }
    Ok(v)
}

/// Sign in through the browser (PKCE) and keep the refresh token.
pub fn login(client_id: &str) -> Result<String, String> {
    let listener = TcpListener::bind(("127.0.0.1", PORT))
        .map_err(|e| format!("port {PORT} is busy ({e}); close whatever uses it and try again"))?;
    let (verifier, state) = (random(48)?, random(16)?);
    let url = format!(
        "{AUTH}?{}",
        form(&[
            ("client_id", client_id),
            ("response_type", "code"),
            ("redirect_uri", REDIRECT_URI),
            ("code_challenge_method", "S256"),
            ("code_challenge", &challenge(&verifier)),
            ("scope", SCOPES),
            ("state", &state),
        ])
    );
    super::navigation::open_url(&url);
    eprintln!("Opening Spotify in your browser… (or open this link)\n{url}");
    listener.set_nonblocking(false).ok();
    let deadline = Instant::now() + Duration::from_secs(300);
    let code = loop {
        if Instant::now() > deadline {
            return Err("timed out waiting for Spotify".into());
        }
        let (mut stream, _) = listener.accept().map_err(|e| e.to_string())?;
        let mut line = String::new();
        BufReader::new(&stream).read_line(&mut line).map_err(|e| e.to_string())?;
        let query = line.split_whitespace().nth(1).and_then(|p| p.split_once('?')).map(|(_, q)| q.to_string());
        let param = |k: &str| {
            query
                .as_deref()
                .and_then(|q| q.split('&').find_map(|kv| kv.strip_prefix(&format!("{k}="))).map(String::from))
        };
        let (got_state, code, error) = (param("state"), param("code"), param("error"));
        let ok = got_state.as_deref() == Some(state.as_str()) && code.is_some();
        let page =
            if ok { "Islet is linked to Spotify. You can close this tab." } else { "Spotify sign-in didn't finish." };
        let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n<!doctype html><title>Islet</title><p style=\"font:16px system-ui;margin:3em\">{page}</p>");
        if let Some(e) = error {
            return Err(format!("Spotify: {e}"));
        }
        if ok {
            break code.unwrap_or_default();
        }
    };
    let v = token_request(form(&[
        ("grant_type", "authorization_code"),
        ("code", &code),
        ("redirect_uri", REDIRECT_URI),
        ("client_id", client_id),
        ("code_verifier", &verifier),
    ]))?;
    let refresh = v["refresh_token"].as_str().ok_or("Spotify didn't return a refresh token")?;
    save(&Creds { client_id: client_id.to_string(), refresh_token: refresh.to_string() })?;
    Ok("Linked to Spotify.".into())
}

pub fn linked() -> bool {
    load().is_some()
}

struct Session {
    creds: Creds,
    access: Option<(String, Instant)>,
}

impl Session {
    fn token(&mut self) -> Result<String, String> {
        if let Some((t, until)) = &self.access {
            if Instant::now() < *until {
                return Ok(t.clone());
            }
        }
        let v = token_request(form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", &self.creds.refresh_token),
            ("client_id", &self.creds.client_id),
        ]))?;
        let token = v["access_token"].as_str().ok_or("no access token")?.to_string();
        let life = v["expires_in"].as_u64().unwrap_or(3600).saturating_sub(60);
        if let Some(r) = v["refresh_token"].as_str() {
            // Spotify may rotate the refresh token.
            self.creds.refresh_token = r.to_string();
            let _ = save(&self.creds);
        }
        self.access = Some((token.clone(), Instant::now() + Duration::from_secs(life)));
        Ok(token)
    }

    /// GET the player state; None when nothing is playing anywhere.
    fn player(&mut self) -> Result<Option<serde_json::Value>, String> {
        let token = self.token()?;
        let mut resp = crate::net::agent()
            .get(API)
            .header("Authorization", &format!("Bearer {token}"))
            .call()
            .map_err(|e| format!("network: {e}"))?;
        match resp.status().as_u16() {
            204 => Ok(None),
            200 => serde_json::from_str(&resp.body_mut().read_to_string().map_err(|e| e.to_string())?)
                .map(Some)
                .map_err(|e| e.to_string()),
            401 => {
                self.access = None;
                Err("token expired".into())
            }
            s => Err(format!("Spotify answered {s}")),
        }
    }

    fn command(&mut self, cmd: MediaCommand, playing: bool) -> Result<(), String> {
        let token = self.token()?;
        let agent = crate::net::agent();
        let auth = format!("Bearer {token}");
        let r = match cmd {
            MediaCommand::PlayPause => agent
                .put(format!("{API}/{}", if playing { "pause" } else { "play" }))
                .header("Authorization", &auth)
                .send_empty(),
            MediaCommand::Next => agent.post(format!("{API}/next")).header("Authorization", &auth).send_empty(),
            MediaCommand::Previous => agent.post(format!("{API}/previous")).header("Authorization", &auth).send_empty(),
            MediaCommand::SeekTo(s) => agent
                .put(format!("{API}/seek?position_ms={}", (s.max(0.0) * 1000.0) as u64))
                .header("Authorization", &auth)
                .send_empty(),
        };
        let status = r.map_err(|e| format!("network: {e}"))?.status().as_u16();
        if status < 300 {
            Ok(())
        } else if status == 403 {
            Err("Spotify Premium is needed to control playback".into())
        } else {
            Err(format!("Spotify answered {status}"))
        }
    }
}

/// The island's view of a player response.
pub fn parse_player(v: &serde_json::Value) -> Option<(MediaInfo, Option<String>)> {
    let item = v.get("item")?;
    let device = v.pointer("/device/name").and_then(|d| d.as_str()).unwrap_or("another device");
    let artists: Vec<&str> =
        item["artists"].as_array().map(|a| a.iter().filter_map(|x| x["name"].as_str()).collect()).unwrap_or_default();
    // The smallest cover that's still at least 64 px.
    let art = item.pointer("/album/images").and_then(|i| i.as_array()).and_then(|imgs| {
        imgs.iter()
            .filter(|i| i["width"].as_u64().unwrap_or(0) >= 64)
            .min_by_key(|i| i["width"].as_u64().unwrap_or(u64::MAX))
            .or(imgs.first())
            .and_then(|i| i["url"].as_str().map(String::from))
    });
    let info = MediaInfo {
        source: SOURCE.into(),
        app_name: format!("Spotify on {device}"),
        title: item["name"].as_str().unwrap_or_default().to_string(),
        artist: artists.join(", "),
        album: item.pointer("/album/name").and_then(|a| a.as_str()).unwrap_or_default().to_string(),
        playing: v["is_playing"].as_bool().unwrap_or(false),
        position: v["progress_ms"].as_f64().map(|ms| ms / 1000.0),
        position_at: 0.0,
        duration: item["duration_ms"].as_f64().map(|ms| ms / 1000.0),
        rate: 1.0,
        art: None,
        file: None,
        caps: MediaCaps { play_pause: true, next: true, previous: true, seek: true },
    };
    Some((info, art))
}

/// Send `Some(true/false)` to turn it on/off, or a playback command.
pub enum SpotifyMsg {
    Enable(bool),
    Command(MediaCommand),
}

pub fn start(sink: Sink) -> Sender<SpotifyMsg> {
    let (tx, rx) = channel::<SpotifyMsg>();
    let _ = std::thread::Builder::new().name("islet-spotify".into()).spawn(move || {
        let mut session: Option<Session> = None;
        let mut art: Option<(String, Option<islet_core::activity::ArtImage>)> = None;
        let mut playing = false;
        let mut backoff = Duration::from_secs(10);
        loop {
            let msg = match &session {
                Some(_) => rx
                    .recv_timeout(if playing { Duration::from_secs(10) } else { Duration::from_secs(60) }.max(backoff)),
                None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
            };
            match msg {
                Ok(SpotifyMsg::Enable(false)) => {
                    if session.take().is_some() {
                        sink.send(Report::RemoteMedia(None));
                    }
                    continue;
                }
                Ok(SpotifyMsg::Enable(true)) => {
                    if session.is_none() {
                        session = load().map(|creds| Session { creds, access: None });
                    }
                }
                Ok(SpotifyMsg::Command(cmd)) => {
                    if let Some(s) = &mut session {
                        if let Err(e) = s.command(cmd, playing) {
                            debug_log(&format!("spotify: {e}"));
                        }
                        std::thread::sleep(Duration::from_millis(300)); // let the change land
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
            let Some(s) = &mut session else { continue };
            match s.player() {
                Ok(Some(v)) => {
                    backoff = Duration::from_secs(10);
                    let Some((mut info, url)) = parse_player(&v) else {
                        sink.send(Report::RemoteMedia(None));
                        continue;
                    };
                    playing = info.playing;
                    if let Some(url) = url {
                        if art.as_ref().map(|(u, _)| u) != Some(&url) {
                            let img = crate::net::fetch_bytes(&url, 2 * 1024 * 1024)
                                .ok()
                                .and_then(|b| super::art::decode(&b));
                            art = Some((url, img));
                        }
                        info.art = art.as_ref().and_then(|(_, i)| i.clone());
                    }
                    sink.send(Report::RemoteMedia(Some(info)));
                }
                Ok(None) => {
                    playing = false;
                    sink.send(Report::RemoteMedia(None));
                }
                Err(e) => {
                    debug_log(&format!("spotify: {e}"));
                    backoff = (backoff * 2).min(Duration::from_secs(300));
                }
            }
        }
    });
    tx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_challenge_matches_rfc_7636() {
        // RFC 7636, appendix B.
        assert_eq!(
            challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
        assert_eq!(b64url(b"hi?"), "aGk_");
    }

    #[test]
    fn parses_playback() {
        let v: serde_json::Value = serde_json::from_str(
            r#"{"is_playing":true,"progress_ms":42000,"device":{"name":"Pixel 8","type":"Smartphone"},
            "item":{"name":"Midnight Signals","duration_ms":200000,"artists":[{"name":"A"},{"name":"B"}],
            "album":{"name":"Echoes","images":[{"url":"https://i/640","width":640},{"url":"https://i/300","width":300},{"url":"https://i/64","width":64}]}}}"#,
        )
        .unwrap();
        let (m, art) = parse_player(&v).unwrap();
        assert_eq!(
            (m.app_name.as_str(), m.title.as_str(), m.artist.as_str()),
            ("Spotify on Pixel 8", "Midnight Signals", "A, B")
        );
        assert_eq!((m.position, m.duration, m.playing), (Some(42.0), Some(200.0), true));
        assert_eq!(art.as_deref(), Some("https://i/64"));
    }
}
