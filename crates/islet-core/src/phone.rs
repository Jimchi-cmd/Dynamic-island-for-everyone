//! The phone companion's link (Islet Pro): an Android phone on the same
//! network sends its notifications, music and battery level to the island.
//!
//! Pairing: the computer makes a random 256-bit key and shows it in a QR
//! code, with its address. The phone scans it. Nothing else ever carries
//! the key, and it never leaves the two devices.
//!
//! Each connection: the computer sends 16 random bytes, the phone answers
//! with 16 of its own, and both derive a session key from the pairing key
//! and the two (HMAC-SHA-256). Every message after that is AES-256-GCM
//! under the session key, with a counter that only goes up: a recorded
//! message can't be replayed, in this connection or a later one.
//!
//! The Android app implements the same thing in Kotlin; both sides test
//! against the vectors in this file's tests.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// What the computer says first, before its 16 random bytes.
pub const GREETING: &[u8; 6] = b"ISLET1";
pub const KEY_LEN: usize = 32;
pub const NONCE_LEN: usize = 16;
/// The largest message, sealed. Anything longer ends the connection.
pub const MAX_FRAME: usize = 64 * 1024;
const LABEL: &[u8] = b"islet-phone-v1";

/// HMAC-SHA-256 (RFC 2104).
pub fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    let mut block = [0u8; 64];
    if key.len() > 64 {
        block[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        block[..key.len()].copy_from_slice(key);
    }
    let inner = Sha256::new().chain_update(block.map(|b| b ^ 0x36)).chain_update(data).finalize();
    Sha256::new().chain_update(block.map(|b| b ^ 0x5c)).chain_update(inner).finalize().into()
}

/// The key for one connection, from the pairing key and both sides' random bytes.
pub fn session_key(pairing_key: &[u8; KEY_LEN], server: &[u8; NONCE_LEN], client: &[u8; NONCE_LEN]) -> [u8; 32] {
    let mut data = Vec::with_capacity(LABEL.len() + 2 * NONCE_LEN);
    data.extend_from_slice(LABEL);
    data.extend_from_slice(server);
    data.extend_from_slice(client);
    hmac_sha256(pairing_key, &data)
}

/// Which side is sealing: the two never share a nonce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Computer = 1,
    Phone = 2,
}

/// One end of a connection, once both sides have the session key.
pub struct Channel {
    cipher: Aes256Gcm,
    side: Side,
    sent: u64,
    received: u64,
}

impl Channel {
    pub fn new(session_key: &[u8; 32], side: Side) -> Self {
        Self { cipher: Aes256Gcm::new(session_key.into()), side, sent: 0, received: 0 }
    }

    fn nonce(side: Side, counter: u64) -> [u8; 12] {
        let mut n = [0u8; 12];
        n[0] = side as u8;
        n[4..].copy_from_slice(&counter.to_be_bytes());
        n
    }

    /// Seal a message: its counter (8 bytes), then the ciphertext and tag.
    pub fn seal(&mut self, plain: &[u8]) -> Vec<u8> {
        self.sent += 1;
        let nonce = Self::nonce(self.side, self.sent);
        let sealed = self.cipher.encrypt(Nonce::from_slice(&nonce), plain).unwrap_or_default();
        let mut out = Vec::with_capacity(8 + sealed.len());
        out.extend_from_slice(&self.sent.to_be_bytes());
        out.extend_from_slice(&sealed);
        out
    }

    /// Open the other side's message. Fails on a wrong key, a changed
    /// byte, or a counter that doesn't move forward (a replay).
    pub fn open(&mut self, frame: &[u8]) -> Result<Vec<u8>, &'static str> {
        let (counter, sealed) = frame.split_first_chunk::<8>().ok_or("too short")?;
        let counter = u64::from_be_bytes(*counter);
        if counter <= self.received {
            return Err("an old message, sent again");
        }
        let from = if self.side == Side::Computer { Side::Phone } else { Side::Computer };
        let nonce = Self::nonce(from, counter);
        let plain = self.cipher.decrypt(Nonce::from_slice(&nonce), sealed).map_err(|_| "not sealed with our key")?;
        self.received = counter;
        Ok(plain)
    }
}

/// What the phone tells the computer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum FromPhone {
    /// First message of a connection.
    Hello {
        name: String,
    },
    Battery {
        level: u8,
        charging: bool,
    },
    /// A notification on the phone.
    Note {
        app: String,
        title: String,
        text: String,
    },
    /// What is playing on the phone (`title` empty: nothing).
    Media {
        #[serde(default)]
        title: String,
        #[serde(default)]
        artist: String,
        #[serde(default)]
        app: String,
        #[serde(default)]
        playing: bool,
        /// Seconds; 0 when the player doesn't say.
        #[serde(default)]
        duration: f64,
        #[serde(default)]
        position: f64,
    },
    Ping,
}

/// What the computer tells the phone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ToPhone {
    /// The hello was read: the keys match.
    Ok {
        name: String,
    },
    /// A button on the island's music card: "play_pause", "next" or "previous".
    Command {
        r#do: String,
    },
    Pong,
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Base64 for links (RFC 4648 §5), without padding.
pub fn base64url(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = u32::from(chunk[0]) << 16
            | u32::from(*chunk.get(1).unwrap_or(&0)) << 8
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for i in 0..=chunk.len() {
            out.push(B64[(n >> (18 - 6 * i) & 63) as usize] as char);
        }
    }
    out
}

pub fn base64url_decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in text.bytes() {
        let v = B64.iter().position(|b| *b == c)? as u32;
        acc = acc << 6 | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// What the QR code says: where the computer is, and the pairing key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pairing {
    /// The computer's addresses on the local network (the phone tries each).
    pub hosts: Vec<String>,
    pub port: u16,
    pub key: [u8; KEY_LEN],
    pub name: String,
}

impl Pairing {
    /// `islet://pair?h=192.168.1.20,10.0.0.5&p=47821&k=…&n=My%20laptop`
    pub fn to_uri(&self) -> String {
        let name: String = self
            .name
            .bytes()
            .map(|b| if b.is_ascii_alphanumeric() { (b as char).to_string() } else { format!("%{b:02X}") })
            .collect();
        format!("islet://pair?h={}&p={}&k={}&n={name}", self.hosts.join(","), self.port, base64url(&self.key))
    }

    pub fn from_uri(uri: &str) -> Option<Self> {
        let query = uri.trim().strip_prefix("islet://pair?")?;
        let (mut hosts, mut port, mut key, mut name) = (Vec::new(), None, None, String::new());
        for part in query.split('&') {
            let (k, v) = part.split_once('=')?;
            match k {
                "h" => hosts = v.split(',').filter(|h| !h.is_empty()).map(String::from).collect(),
                "p" => port = v.parse::<u16>().ok(),
                "k" => key = base64url_decode(v).and_then(|b| <[u8; KEY_LEN]>::try_from(b).ok()),
                "n" => name = percent_decode(v),
                _ => {}
            }
        }
        let ok = !hosts.is_empty()
            && hosts.iter().all(|h| h.chars().all(|c| c.is_ascii_alphanumeric() || ".:-".contains(c)));
        if !ok {
            return None;
        }
        Some(Self { hosts, port: port?, key: key?, name })
    }
}

fn percent_decode(v: &str) -> String {
    let b = v.as_bytes();
    let (mut out, mut i) = (Vec::with_capacity(b.len()), 0);
    while i < b.len() {
        let hex = (b[i] == b'%').then(|| v.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok())).flatten();
        match hex {
            Some(byte) => {
                out.push(byte);
                i += 3;
            }
            None => {
                out.push(b[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    // The same values are in the Android app's ProtocolTest.kt.
    const KEY: [u8; 32] = [
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29,
        30, 31,
    ];
    const SERVER: [u8; 16] = [0xA0; 16];
    const CLIENT: [u8; 16] = [0xB1; 16];

    #[test]
    fn hmac_matches_rfc_4231() {
        // Test case 2: key "Jefe", data "what do ya want for nothing?".
        assert_eq!(
            hex(&hmac_sha256(b"Jefe", b"what do ya want for nothing?")),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
        // Test case 6: a key longer than the block is hashed first.
        assert_eq!(
            hex(&hmac_sha256(&[0xaa; 131], b"Test Using Larger Than Block-Size Key - Hash Key First")),
            "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
        );
    }

    #[test]
    fn both_ends_agree_and_replays_are_refused() {
        let key = session_key(&KEY, &SERVER, &CLIENT);
        let (mut computer, mut phone) = (Channel::new(&key, Side::Computer), Channel::new(&key, Side::Phone));
        let hello = serde_json::to_vec(&FromPhone::Hello { name: "Pixel".into() }).unwrap();
        let frame = phone.seal(&hello);
        assert_eq!(computer.open(&frame).unwrap(), hello);
        // The same frame again, or an older one later, is refused.
        assert!(computer.open(&frame).is_err());
        let second = phone.seal(b"two");
        let third = phone.seal(b"three");
        assert_eq!(computer.open(&third).unwrap(), b"three");
        assert!(computer.open(&second).is_err(), "it arrived after a newer one");
        // A changed byte, or a frame bounced back to its sender, fails.
        let mut bent = phone.seal(b"four");
        *bent.last_mut().unwrap() ^= 1;
        assert!(computer.open(&bent).is_err());
        let own = computer.seal(b"hi phone");
        assert!(computer.open(&own).is_err(), "its own message isn't the phone's");
        assert_eq!(phone.open(&own).unwrap(), b"hi phone");
        // Another connection has another key.
        let other = session_key(&KEY, &[0xA1; 16], &CLIENT);
        assert!(Channel::new(&other, Side::Computer).open(&Channel::new(&key, Side::Phone).seal(b"x")).is_err());
    }

    #[test]
    fn test_vectors_for_the_android_app() {
        let key = session_key(&KEY, &SERVER, &CLIENT);
        assert_eq!(hex(&key), VECTOR_SESSION_KEY);
        let frame = Channel::new(&key, Side::Phone).seal(br#"{"t":"ping"}"#);
        assert_eq!(hex(&frame), VECTOR_FRAME);
    }
    const VECTOR_SESSION_KEY: &str = "2d4df88021832e07dedd23b231d0d9a846c014f644903c5bcfe8f959fa71f65e";
    const VECTOR_FRAME: &str = "0000000000000001b158e8ab0848a57bcbb1eb1276826dc2777d0e8c47f8ea6d07569641";

    #[test]
    fn messages_are_small_json() {
        let m: FromPhone = serde_json::from_str(r#"{"t":"battery","level":82,"charging":true}"#).unwrap();
        assert_eq!(m, FromPhone::Battery { level: 82, charging: true });
        let m: FromPhone =
            serde_json::from_str(r#"{"t":"media","title":"Song","artist":"Band","playing":true}"#).unwrap();
        assert!(
            matches!(m, FromPhone::Media { ref title, playing: true, duration, .. } if title == "Song" && duration == 0.0)
        );
        assert_eq!(serde_json::from_str::<FromPhone>(r#"{"t":"ping"}"#).unwrap(), FromPhone::Ping);
        assert!(
            serde_json::from_str::<FromPhone>(r#"{"t":"shell","cmd":"rm"}"#).is_err(),
            "nothing else is understood"
        );
        assert_eq!(
            serde_json::to_string(&ToPhone::Command { r#do: "next".into() }).unwrap(),
            r#"{"t":"command","do":"next"}"#
        );
    }

    #[test]
    fn the_pairing_link_round_trips() {
        let p = Pairing {
            hosts: vec!["192.168.1.20".into(), "10.0.0.5".into()],
            port: 47821,
            key: KEY,
            name: "Joram's laptop".into(),
        };
        let uri = p.to_uri();
        assert_eq!(
            uri,
            "islet://pair?h=192.168.1.20,10.0.0.5&p=47821&k=AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8&n=Joram%27s%20laptop"
        );
        assert_eq!(Pairing::from_uri(&uri), Some(p));
        assert_eq!(Pairing::from_uri("https://example.com"), None);
        assert_eq!(Pairing::from_uri("islet://pair?h=a&p=1&k=short"), None);
        assert_eq!(
            Pairing::from_uri("islet://pair?h=bad host&p=1&k=AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8"),
            None
        );
        assert_eq!(base64url_decode(&base64url(b"any bytes at all!")).unwrap(), b"any bytes at all!");
    }
}
