//! Islet Pro: a one-time purchase that unlocks the extra features. Keys are
//! sold through a merchant of record (Polar or Lemon Squeezy, whichever the
//! maintainer sells through), and checked with that store's public
//! license-key API, which needs no secret in the app.
//!
//! Only this module's pure parts live here: building the requests, reading
//! the answers, and deciding whether Pro is on. The HTTP calls are in the
//! `islet` crate, and they happen only after the user enters a key: when it
//! is activated, then at most once a week to see that it is still valid
//! (a refunded key stops working). Being offline never switches Pro off.
//!
//! The key itself goes in the OS keychain; `pro.json` keeps only what is
//! needed to show the status (the activation id, the key's last characters
//! and when it was last checked).

use serde::{Deserialize, Serialize};

/// Where keys are sold. The ids are public (they appear in every checkout
/// link) and are set when a release is built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Store {
    /// Polar: the organization id (a UUID).
    Polar { org: String },
    /// Lemon Squeezy: the store id and, optionally, the product id.
    Lemon { store: u64, product: Option<u64> },
}

impl Store {
    pub fn id(&self) -> &'static str {
        match self {
            Store::Polar { .. } => "polar",
            Store::Lemon { .. } => "lemon",
        }
    }

    /// From the build's settings: a Polar organization id, or a Lemon
    /// Squeezy store id (and product id). Polar wins if both are set.
    pub fn from_config(
        polar_org: Option<&str>,
        lemon_store: Option<&str>,
        lemon_product: Option<&str>,
    ) -> Option<Store> {
        let polar_org = polar_org.map(str::trim).filter(|s| !s.is_empty());
        if let Some(org) = polar_org {
            return Some(Store::Polar { org: org.to_string() });
        }
        let store = lemon_store.and_then(|s| s.trim().parse().ok())?;
        let product = lemon_product.and_then(|s| s.trim().parse().ok());
        Some(Store::Lemon { store, product })
    }
}

/// An HTTP request for the islet crate to send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub url: String,
    /// `application/json` (Polar) or `application/x-www-form-urlencoded`
    /// (Lemon Squeezy).
    pub content_type: &'static str,
    pub body: String,
}

const POLAR: &str = "https://api.polar.sh/v1/customer-portal/license-keys";
const LEMON: &str = "https://api.lemonsqueezy.com/v1/licenses";
const JSON: &str = "application/json";
const FORM: &str = "application/x-www-form-urlencoded";

/// Take up one of the key's device slots. `label` names this computer in
/// the buyer's account (e.g. "Islet on Windows").
pub fn activate_request(store: &Store, key: &str, label: &str) -> Request {
    match store {
        Store::Polar { org } => Request {
            url: format!("{POLAR}/activate"),
            content_type: JSON,
            body: serde_json::json!({ "key": key, "organization_id": org, "label": label }).to_string(),
        },
        Store::Lemon { .. } => Request {
            url: format!("{LEMON}/activate"),
            content_type: FORM,
            body: form(&[("license_key", key), ("instance_name", label)]),
        },
    }
}

/// Is the key (and this computer's activation) still good?
pub fn validate_request(store: &Store, key: &str, instance: &str) -> Request {
    match store {
        Store::Polar { org } => Request {
            url: format!("{POLAR}/validate"),
            content_type: JSON,
            body: serde_json::json!({ "key": key, "organization_id": org, "activation_id": instance }).to_string(),
        },
        Store::Lemon { .. } => Request {
            url: format!("{LEMON}/validate"),
            content_type: FORM,
            body: form(&[("license_key", key), ("instance_id", instance)]),
        },
    }
}

/// Give this computer's slot back (to move Pro to another computer).
pub fn deactivate_request(store: &Store, key: &str, instance: &str) -> Request {
    match store {
        Store::Polar { org } => Request {
            url: format!("{POLAR}/deactivate"),
            content_type: JSON,
            body: serde_json::json!({ "key": key, "organization_id": org, "activation_id": instance }).to_string(),
        },
        Store::Lemon { .. } => Request {
            url: format!("{LEMON}/deactivate"),
            content_type: FORM,
            body: form(&[("license_key", key), ("instance_id", instance)]),
        },
    }
}

/// What the store said about a key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// The key is good. `instance` is this computer's activation id;
    /// `expires` a Unix time, if the key has one.
    Valid { instance: String, expires: Option<i64> },
    /// The store said no, with its reason (wrong key, refunded, no slots left…).
    Refused(String),
}

/// Read the answer to an activate or validate request. `Err` means the
/// answer couldn't be understood (a proxy page, an outage): try again later.
pub fn parse_answer(store: &Store, status: u16, body: &str, instance: Option<&str>) -> Result<Verdict, String> {
    let v: serde_json::Value =
        serde_json::from_str(body).map_err(|_| format!("unexpected answer from the store (HTTP {status})"))?;
    match store {
        Store::Polar { org } => parse_polar(org, status, &v, instance),
        Store::Lemon { store, product } => parse_lemon(*store, *product, status, &v, instance),
    }
}

fn parse_polar(org: &str, status: u16, v: &serde_json::Value, instance: Option<&str>) -> Result<Verdict, String> {
    match status {
        200 | 201 => {}
        // Polar answers 404 for wrong, revoked and expired keys, 403 when
        // every device slot is taken, 422 for a malformed key.
        403 | 404 | 422 => return Ok(Verdict::Refused(polar_reason(status, v))),
        _ => return Err(format!("the store isn't answering right now (HTTP {status})")),
    }
    // An activation wraps the key; a validation is the key itself.
    let (key, activation) = match v.get("license_key") {
        Some(k) => (k, Some(v)),
        None => (v, v.get("activation").filter(|a| !a.is_null())),
    };
    if key.get("organization_id").and_then(|o| o.as_str()) != Some(org) {
        return Ok(Verdict::Refused("This key is for a different product.".into()));
    }
    if key.get("status").and_then(|s| s.as_str()) != Some("granted") {
        return Ok(Verdict::Refused("This key is no longer active.".into()));
    }
    let id = activation.and_then(|a| a.get("id")).and_then(|i| i.as_str()).or(instance);
    let Some(id) = id else { return Err("the store didn't return an activation".into()) };
    Ok(Verdict::Valid {
        instance: id.to_string(),
        expires: key.get("expires_at").and_then(|e| e.as_str()).and_then(parse_time),
    })
}

fn polar_reason(status: u16, v: &serde_json::Value) -> String {
    let detail = v.get("detail").and_then(|d| d.as_str()).unwrap_or("");
    if status == 403 && detail.contains("limit") {
        return "This key is already in use on its maximum number of computers. Deactivate it on one of them first."
            .into();
    }
    if status == 404 && (detail.contains("expired") || detail.contains("no longer active")) {
        return detail.to_string();
    }
    if status == 422 || status == 404 {
        return "That key wasn't recognised. Check it against your receipt.".into();
    }
    if detail.is_empty() {
        "The store refused this key.".into()
    } else {
        detail.to_string()
    }
}

fn parse_lemon(
    store: u64,
    product: Option<u64>,
    status: u16,
    v: &serde_json::Value,
    instance: Option<&str>,
) -> Result<Verdict, String> {
    let ok = v.get("activated").or_else(|| v.get("valid")).and_then(|b| b.as_bool());
    let Some(ok) = ok else {
        return if (400..500).contains(&status) {
            Ok(Verdict::Refused(lemon_reason(v)))
        } else {
            Err(format!("the store isn't answering right now (HTTP {status})"))
        };
    };
    if !ok {
        return Ok(Verdict::Refused(lemon_reason(v)));
    }
    // Lemon Squeezy validates any key from any store: make sure this one is ours.
    let meta = v.get("meta");
    let num = |k: &str| meta.and_then(|m| m.get(k)).and_then(|x| x.as_u64());
    if num("store_id") != Some(store) || product.is_some_and(|p| num("product_id") != Some(p)) {
        return Ok(Verdict::Refused("This key is for a different product.".into()));
    }
    let key = v.get("license_key");
    let key_status = key.and_then(|k| k.get("status")).and_then(|s| s.as_str()).unwrap_or("");
    if matches!(key_status, "expired" | "disabled") {
        return Ok(Verdict::Refused(format!("This key is {key_status}.")));
    }
    let id = v.get("instance").and_then(|i| i.get("id")).and_then(|i| i.as_str()).or(instance);
    let Some(id) = id else { return Err("the store didn't return an activation".into()) };
    let expires = key.and_then(|k| k.get("expires_at")).and_then(|e| e.as_str()).and_then(parse_time);
    Ok(Verdict::Valid { instance: id.to_string(), expires })
}

fn lemon_reason(v: &serde_json::Value) -> String {
    let e = v.get("error").and_then(|e| e.as_str()).unwrap_or("");
    let lower = e.to_lowercase();
    if lower.contains("activation limit") {
        "This key is already in use on its maximum number of computers. Deactivate it on one of them first.".into()
    } else if lower.contains("not found") || lower.contains("invalid") || e.is_empty() {
        "That key wasn't recognised. Check it against your receipt.".into()
    } else {
        e.to_string()
    }
}

/// Pro's state on this computer (pro.json).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProState {
    /// "polar" or "lemon".
    pub store: String,
    /// This computer's activation id.
    pub instance: String,
    /// The key's last characters, to show which key is in use.
    pub key_hint: String,
    /// The last answer was "valid".
    pub valid: bool,
    /// Unix times: when the key was activated and last confirmed.
    pub activated_at: i64,
    pub checked_at: i64,
    /// When the key expires (Unix time), if it does.
    pub expires_at: Option<i64>,
}

/// A week between checks that the key still works.
pub const CHECK_EVERY: i64 = 7 * 24 * 3600;

impl ProState {
    /// Is Pro on right now?
    pub fn active(&self, now: i64) -> bool {
        self.valid && !self.instance.is_empty() && self.expires_at.is_none_or(|e| now < e)
    }

    /// Time to ask the store again?
    pub fn due(&self, now: i64) -> bool {
        self.valid && now - self.checked_at >= CHECK_EVERY
    }

    /// After a successful activation.
    pub fn activated(store: &Store, key: &str, instance: String, expires: Option<i64>, now: i64) -> ProState {
        ProState {
            store: store.id().into(),
            instance,
            key_hint: hint(key),
            valid: true,
            activated_at: now,
            checked_at: now,
            expires_at: expires,
        }
    }

    /// After a check. A refusal turns Pro off; `Err` (offline, outage)
    /// changes nothing, so it's tried again next time.
    pub fn checked(&mut self, verdict: &Verdict, now: i64) {
        match verdict {
            Verdict::Valid { expires, .. } => {
                self.valid = true;
                self.checked_at = now;
                self.expires_at = *expires;
            }
            Verdict::Refused(_) => {
                self.valid = false;
                self.checked_at = now;
            }
        }
    }
}

/// The last four characters of a key, e.g. "…7F2C".
pub fn hint(key: &str) -> String {
    let chars: Vec<char> = key.trim().chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    let tail: String = chars[chars.len().saturating_sub(4)..].iter().collect();
    format!("…{tail}")
}

/// What Pro unlocks, for Settings and the docs.
pub const FEATURES: [&str; 9] = [
    "No low-end limits: smoother motion, sharper album art, longer history",
    "Quick notes saved to a file, and a to-do list on the island",
    "Stopwatch with laps, and world clocks",
    "Looks: tinted and glass styles, the notch shape, a glow in the album's colour",
    "Moving bars as the song's progress bar",
    "Clock faces for the empty island: with the date, or with seconds",
    "Shortcut buttons of your own, on the island and in its right-click menu",
    "Power saver for long days on battery",
    "Supporter badge and early builds",
];

/// `2026-11-10T12:00:00Z`, `2026-11-10T12:00:00.000000Z` or
/// `2026-11-10T12:00:00+00:00` as Unix time (the offset is honoured).
pub fn parse_time(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() < 19 || b[4] != b'-' || b[7] != b'-' || b[13] != b':' || b[16] != b':' {
        return None;
    }
    let n = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    let (y, mo, d, h, mi, se) = (n(0..4)?, n(5..7)?, n(8..10)?, n(11..13)?, n(14..16)?, n(17..19)?);
    let days = crate::alarm::days_from_civil(y as i32, mo as u8, d as u8);
    let mut t = days * 86400 + h * 3600 + mi * 60 + se;
    // Skip fractional seconds, then read an offset.
    let rest = s[19..].trim_start_matches(|c: char| c == '.' || c.is_ascii_digit());
    if let Some(sign @ ('+' | '-')) = rest.chars().next() {
        let off = rest.get(1..6)?;
        let (oh, om) = (off.get(0..2)?.parse::<i64>().ok()?, off.get(3..5)?.parse::<i64>().ok()?);
        let secs = oh * 3600 + om * 60;
        t += if sign == '+' { -secs } else { secs };
    }
    Some(t)
}

fn form(pairs: &[(&str, &str)]) -> String {
    pairs.iter().map(|(k, v)| format!("{k}={}", encode(v))).collect::<Vec<_>>().join("&")
}

fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORG: &str = "1dbfc517-0bbf-4301-9ba8-555ca42b9737";

    fn polar() -> Store {
        Store::Polar { org: ORG.into() }
    }

    fn lemon() -> Store {
        Store::Lemon { store: 1234, product: Some(56) }
    }

    #[test]
    fn picks_the_configured_store() {
        assert_eq!(Store::from_config(None, None, None), None);
        assert_eq!(Store::from_config(Some(" "), Some("x"), None), None);
        assert_eq!(Store::from_config(Some(ORG), Some("1"), None), Some(polar()));
        assert_eq!(Store::from_config(None, Some("1234"), Some("56")), Some(lemon()));
        assert_eq!(Store::from_config(None, Some("1234"), None), Some(Store::Lemon { store: 1234, product: None }));
    }

    #[test]
    fn builds_polar_requests_as_json() {
        let r = activate_request(&polar(), "POLAR-ABC", "Islet on Linux");
        assert_eq!(r.url, "https://api.polar.sh/v1/customer-portal/license-keys/activate");
        assert_eq!(r.content_type, "application/json");
        let v: serde_json::Value = serde_json::from_str(&r.body).unwrap();
        assert_eq!(v["key"], "POLAR-ABC");
        assert_eq!(v["organization_id"], ORG);
        assert_eq!(v["label"], "Islet on Linux");
        let v: serde_json::Value = serde_json::from_str(&validate_request(&polar(), "K", "act-1").body).unwrap();
        assert_eq!(v["activation_id"], "act-1");
        assert!(deactivate_request(&polar(), "K", "a").url.ends_with("/deactivate"));
    }

    #[test]
    fn builds_lemon_requests_as_forms() {
        let r = activate_request(&lemon(), "38b1-46&0", "Islet on Windows");
        assert_eq!(r.url, "https://api.lemonsqueezy.com/v1/licenses/activate");
        assert_eq!(r.content_type, "application/x-www-form-urlencoded");
        assert_eq!(r.body, "license_key=38b1-46%260&instance_name=Islet+on+Windows");
        assert_eq!(validate_request(&lemon(), "K", "i-9").body, "license_key=K&instance_id=i-9");
    }

    fn polar_key(status: &str, org: &str, expires: &str) -> serde_json::Value {
        serde_json::json!({
            "id": "lk-1", "organization_id": org, "status": status, "key": "POLAR-X",
            "display_key": "****-X", "limit_activations": 3, "usage": 0,
            "expires_at": if expires.is_empty() { serde_json::Value::Null } else { expires.into() },
        })
    }

    #[test]
    fn reads_polar_activations_and_validations() {
        let act = serde_json::json!({ "id": "act-7", "label": "Islet", "license_key": polar_key("granted", ORG, "") });
        assert_eq!(
            parse_answer(&polar(), 200, &act.to_string(), None),
            Ok(Verdict::Valid { instance: "act-7".into(), expires: None })
        );
        let mut val = polar_key("granted", ORG, "2027-01-01T00:00:00Z");
        val["activation"] = serde_json::json!({ "id": "act-7" });
        assert_eq!(
            parse_answer(&polar(), 200, &val.to_string(), Some("act-7")),
            Ok(Verdict::Valid { instance: "act-7".into(), expires: Some(1798761600) })
        );
        // Someone else's organization.
        let other = serde_json::json!({ "id": "a", "license_key": polar_key("granted", "other", "") });
        assert!(matches!(parse_answer(&polar(), 200, &other.to_string(), None), Ok(Verdict::Refused(_))));
    }

    #[test]
    fn reads_polar_refusals() {
        let limit = r#"{"type":"NotPermitted","detail":"License key activation limit already reached"}"#;
        let Ok(Verdict::Refused(why)) = parse_answer(&polar(), 403, limit, None) else { panic!() };
        assert!(why.contains("maximum number of computers"));
        let gone = r#"{"type":"ResourceNotFound","detail":"License key is no longer active."}"#;
        assert_eq!(
            parse_answer(&polar(), 404, gone, None),
            Ok(Verdict::Refused("License key is no longer active.".into()))
        );
        let wrong = r#"{"type":"ResourceNotFound","detail":"Not found"}"#;
        let Ok(Verdict::Refused(why)) = parse_answer(&polar(), 404, wrong, None) else { panic!() };
        assert!(why.contains("wasn't recognised"));
        // Outages and proxy pages are not refusals: Pro stays as it was.
        assert!(parse_answer(&polar(), 502, r#"{"detail":"bad gateway"}"#, None).is_err());
        assert!(parse_answer(&polar(), 200, "<html>", None).is_err());
    }

    fn lemon_answer(ok_field: &str, ok: bool, store: u64, product: u64, status: &str) -> String {
        serde_json::json!({
            ok_field: ok,
            "error": if ok { serde_json::Value::Null } else { "license_key not found.".into() },
            "license_key": { "id": 1, "status": status, "key": "38b1", "activation_limit": 3,
                             "activation_usage": 1, "expires_at": null },
            "instance": { "id": "inst-3", "name": "Islet" },
            "meta": { "store_id": store, "product_id": product, "customer_email": "a@b.c" },
        })
        .to_string()
    }

    #[test]
    fn reads_lemon_answers() {
        assert_eq!(
            parse_answer(&lemon(), 200, &lemon_answer("activated", true, 1234, 56, "active"), None),
            Ok(Verdict::Valid { instance: "inst-3".into(), expires: None })
        );
        assert_eq!(
            parse_answer(&lemon(), 200, &lemon_answer("valid", true, 1234, 56, "active"), Some("inst-3")),
            Ok(Verdict::Valid { instance: "inst-3".into(), expires: None })
        );
        // Another seller's key, another product, an expired key, a wrong key.
        for body in [
            lemon_answer("valid", true, 999, 56, "active"),
            lemon_answer("valid", true, 1234, 57, "active"),
            lemon_answer("valid", true, 1234, 56, "expired"),
        ] {
            assert!(matches!(parse_answer(&lemon(), 200, &body, None), Ok(Verdict::Refused(_))), "{body}");
        }
        let Ok(Verdict::Refused(why)) = parse_answer(&lemon(), 404, &lemon_answer("activated", false, 0, 0, ""), None)
        else {
            panic!()
        };
        assert!(why.contains("wasn't recognised"));
        let limit = r#"{"activated":false,"error":"This license key has reached the activation limit.","license_key":{},"meta":{}}"#;
        let Ok(Verdict::Refused(why)) = parse_answer(&lemon(), 400, limit, None) else { panic!() };
        assert!(why.contains("maximum number"));
        assert!(parse_answer(&lemon(), 503, r#"{"message":"down"}"#, None).is_err());
    }

    #[test]
    fn pro_stays_on_until_the_store_says_otherwise() {
        let mut s = ProState::activated(&polar(), "POLAR-ab12-CD34", "act".into(), None, 1000);
        assert_eq!(s.key_hint, "…CD34");
        assert!(s.active(1000) && !s.due(1000));
        assert!(s.due(1000 + CHECK_EVERY));
        s.checked(&Verdict::Valid { instance: "act".into(), expires: Some(5000) }, 2000);
        assert!(s.active(4999) && !s.active(5000), "an expiry date is honoured");
        s.checked(&Verdict::Refused("refunded".into()), 3000);
        assert!(!s.active(3000) && !s.due(3000 + CHECK_EVERY), "a refused key isn't asked about again");
        assert!(!ProState::default().active(0));
    }

    #[test]
    fn parses_store_times() {
        assert_eq!(parse_time("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_time("2027-01-01T00:00:00.000000Z"), Some(1798761600));
        assert_eq!(parse_time("2027-01-01T02:00:00+02:00"), Some(1798761600));
        assert_eq!(parse_time("2026-12-31T19:00:00-05:00"), Some(1798761600));
        assert_eq!(parse_time("soon"), None);
    }
}
