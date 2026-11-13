//! Islet Pro on this computer: activating a key, the weekly check, and
//! whether Pro is on (see `islet_core::pro` for the rules).
//!
//! The store's ids are set when a release is built (`ISLET_POLAR_ORG`, or
//! `ISLET_LEMON_STORE` and `ISLET_LEMON_PRODUCT`; `ISLET_BUY_URL` is the
//! checkout page). Builds without them say Pro isn't on sale.

use crate::{config, net, secrets};
use islet_core::pro::{self, ProState, Store, Verdict};

pub const BUY_URL: Option<&str> = option_env!("ISLET_BUY_URL");

pub fn store() -> Option<Store> {
    Store::from_config(
        option_env!("ISLET_POLAR_ORG"),
        option_env!("ISLET_LEMON_STORE"),
        option_env!("ISLET_LEMON_PRODUCT"),
    )
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

pub fn load() -> ProState {
    config::load_json("pro.json").unwrap_or_default()
}

fn save(s: &ProState) -> Result<(), String> {
    config::save_json("pro.json", s)
}

/// Is Pro on? Development builds can try it with `ISLET_PRO=1`.
pub fn active() -> bool {
    if cfg!(debug_assertions) && std::env::var("ISLET_PRO").is_ok_and(|v| v == "1") {
        return true;
    }
    load().active(now())
}

fn label() -> String {
    let os = match std::env::consts::OS {
        "macos" => "Mac",
        "windows" => "Windows",
        _ => "Linux",
    };
    let host = computer_name();
    if host.is_empty() {
        format!("Islet on {os}")
    } else {
        format!("Islet on {host} ({os})")
    }
}

fn send(req: &pro::Request) -> Result<(u16, String), String> {
    let mut resp = net::agent()
        .post(&req.url)
        .header("Content-Type", req.content_type)
        .header("Accept", "application/json")
        .send(req.body.as_str())
        .map_err(|e| format!("Couldn't reach the store ({e}). Check your connection and try again."))?;
    let status = resp.status().as_u16();
    let body = resp.body_mut().with_config().limit(64 * 1024).read_to_string().map_err(|e| e.to_string())?;
    Ok((status, body))
}

/// Activate a key on this computer.
pub fn activate(key: &str) -> Result<String, String> {
    let key = key.trim();
    if key.is_empty() || key.len() > 200 {
        return Err("Paste the key from your receipt.".into());
    }
    let store = store().ok_or("Islet Pro isn't on sale in this build.")?;
    let (status, body) = send(&pro::activate_request(&store, key, &label()))?;
    match pro::parse_answer(&store, status, &body, None)? {
        Verdict::Valid { instance, expires } => {
            secrets::set_pro_key(key)?;
            save(&ProState::activated(&store, key, instance, expires, now()))?;
            Ok("Islet Pro is on. Thank you for supporting Islet!".into())
        }
        Verdict::Refused(why) => Err(why),
    }
}

/// Free this computer's slot and switch Pro off here.
pub fn deactivate() -> Result<String, String> {
    let state = load();
    if let (Some(store), Some(key)) = (store(), secrets::pro_key()) {
        if !state.instance.is_empty() {
            // Best effort: the slot is freed if the store can be reached.
            let _ = send(&pro::deactivate_request(&store, &key, &state.instance));
        }
    }
    secrets::clear_pro_key()?;
    config::remove("pro.json");
    Ok("Islet Pro is off on this computer. You can use your key on another one.".into())
}

/// Ask the store whether the key is still good (weekly; also `islet pro
/// check`). Being offline changes nothing.
pub fn check(force: bool) -> Result<bool, String> {
    let mut state = load();
    if state.instance.is_empty() || !(force || state.due(now())) {
        return Ok(state.active(now()));
    }
    let store = store().ok_or("Islet Pro isn't on sale in this build.")?;
    let key = secrets::pro_key().ok_or("The key is missing from the keychain. Enter it again.")?;
    let (status, body) = send(&pro::validate_request(&store, &key, &state.instance))?;
    let verdict = pro::parse_answer(&store, status, &body, Some(&state.instance))?;
    state.checked(&verdict, now());
    save(&state)?;
    Ok(state.active(now()))
}

/// For `islet pro status --json` (and Settings).
pub fn status_json() -> String {
    let s = load();
    serde_json::json!({
        "active": active(),
        "forSale": store().is_some(),
        "buyUrl": BUY_URL,
        "keyHint": s.key_hint,
        "store": s.store,
        "activatedAt": s.activated_at,
        "checkedAt": s.checked_at,
        "expiresAt": s.expires_at,
        "refused": !s.instance.is_empty() && !s.valid,
        "features": pro::FEATURES,
    })
    .to_string()
}

pub fn status_text() -> String {
    let s = load();
    if active() {
        format!("Islet Pro is on (key {}).", s.key_hint)
    } else if !s.instance.is_empty() && !s.valid {
        "Islet Pro is off: the store no longer accepts this key.".into()
    } else {
        "Islet Pro is off.".into()
    }
}

/// This computer's name, as far as the environment says (may be empty).
pub fn computer_name() -> String {
    let host = std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")).unwrap_or_default();
    host.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').take(32).collect()
}
