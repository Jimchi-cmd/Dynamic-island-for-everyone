//! HTTPS for the features that use the network (only when the user turns
//! them on): the OS TLS stack on macOS (Security.framework) and Windows
//! (SChannel); rustls on Linux. All of them trust the system's root
//! certificates, so corporate proxies with their own CA keep working.

use std::time::Duration;

pub fn agent() -> ureq::Agent {
    agent_for(15)
}

/// The same, with its own time limit for each request.
fn agent_for(secs: u64) -> ureq::Agent {
    let tls = ureq::tls::TlsConfig::builder().root_certs(ureq::tls::RootCerts::PlatformVerifier);
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    let tls = tls.provider(ureq::tls::TlsProvider::NativeTls);
    ureq::Agent::config_builder()
        .tls_config(tls.build())
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(secs)))
        .user_agent(concat!("islet/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

/// Download a small file (artwork): at most `max` bytes, 5-second timeout.
pub fn fetch_bytes(url: &str, max: u64) -> Result<Vec<u8>, String> {
    let mut resp = agent()
        .get(url)
        .config()
        .timeout_global(Some(Duration::from_secs(5)))
        .build()
        .call()
        .map_err(|e| e.to_string())?;
    resp.body_mut().with_config().limit(max).read_to_vec().map_err(|e| e.to_string())
}

/// The latest published release on GitHub: (version tag, release page).
pub fn latest_release() -> Result<(String, String), String> {
    let repo = env!("CARGO_PKG_REPOSITORY").trim_start_matches("https://github.com/");
    let url = format!("https://api.github.com/repos/{repo}/releases/latest");
    let mut resp = agent()
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("network: {e}"))?;
    let status = resp.status();
    let body = resp.body_mut().read_to_string().map_err(|e| format!("network: {e}"))?;
    if status == 404 {
        return Err("no releases published yet".into());
    }
    parse_release(&body)
}

/// The newest release of all, early builds (pre-releases) included.
pub fn newest_release() -> Result<(String, String), String> {
    let repo = env!("CARGO_PKG_REPOSITORY").trim_start_matches("https://github.com/");
    let url = format!("https://api.github.com/repos/{repo}/releases?per_page=10");
    let mut resp = agent()
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("network: {e}"))?;
    let body = resp.body_mut().read_to_string().map_err(|e| format!("network: {e}"))?;
    parse_releases(&body)
}

/// The highest version in GitHub's list of releases (drafts don't count).
pub fn parse_releases(json: &str) -> Result<(String, String), String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| format!("bad response: {e}"))?;
    if let Some(msg) = v.get("message").and_then(|m| m.as_str()) {
        return Err(msg.to_string());
    }
    let mut best: Option<(String, String)> = None;
    for r in v.as_array().map(Vec::as_slice).unwrap_or_default() {
        let (Some(tag), false) = (r["tag_name"].as_str(), r["draft"].as_bool().unwrap_or(false)) else { continue };
        if best.as_ref().is_none_or(|(b, _)| islet_core::version::is_newer(tag, b)) {
            best = Some((tag.to_string(), r["html_url"].as_str().unwrap_or_default().to_string()));
        }
    }
    best.ok_or_else(|| "no releases published yet".to_string())
}

pub fn parse_release(json: &str) -> Result<(String, String), String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| format!("bad response: {e}"))?;
    if let Some(msg) = v.get("message").and_then(|m| m.as_str()) {
        return Err(msg.to_string());
    }
    let tag = v["tag_name"].as_str().ok_or("release has no tag")?;
    let page = v["html_url"].as_str().unwrap_or_default();
    Ok((tag.to_string(), page.to_string()))
}

/// Run the speed test (see `islet_core::speedtest`): latency, then a timed
/// download and upload. Only ever run when the user asks for it.
pub fn speed_test() -> Result<islet_core::speedtest::SpeedResult, String> {
    use islet_core::speedtest::*;
    use std::io::Read;
    use std::time::Instant;
    let agent = agent_for(30);
    let call = |r: Result<ureq::http::Response<ureq::Body>, ureq::Error>| {
        let r = r.map_err(|err| format!("speed test: {err}"))?;
        if r.status().is_success() {
            Ok(r)
        } else {
            Err(format!("the speed-test server answered {}; try again later", r.status()))
        }
    };
    let mut used = 0u64;

    // Latency: empty downloads on the one connection (the first opens it).
    let mut pings = Vec::new();
    for i in 0..PINGS {
        let start = Instant::now();
        let mut r = call(agent.get(format!("{DOWN_URL}0")).call())?;
        let _ = r.body_mut().read_to_vec();
        if i > 0 {
            pings.push(start.elapsed().as_secs_f64() * 1000.0);
        }
    }
    let latency_ms = median(&pings).unwrap_or(0.0);

    // Download: a warm-up, then the timed one (cut short on a slow line).
    let mut r = call(agent.get(format!("{DOWN_URL}{WARM_BYTES}")).call())?;
    used += r.body_mut().with_config().limit(WARM_BYTES + 1024).read_to_vec().map_or(0, |v| v.len() as u64);
    let start = Instant::now();
    let mut r = call(agent.get(format!("{DOWN_URL}{DOWN_BYTES}")).call())?;
    let mut reader = r.body_mut().with_config().limit(DOWN_BYTES + 1024).reader();
    let mut buf = vec![0u8; 64 * 1024];
    let mut got = 0u64;
    while start.elapsed().as_secs_f64() < MAX_SECS {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => got += n as u64,
            Err(err) => return Err(format!("speed test: {err}")),
        }
    }
    let down_mbps = mbps(got, start.elapsed().as_secs_f64());
    used += got;
    drop(reader);

    // Upload: zeros, timed until the server has them all.
    let body = vec![0u8; UP_BYTES];
    let start = Instant::now();
    let mut r = call(agent.post(UP_URL).send(&body[..]))?;
    let _ = r.body_mut().read_to_vec();
    let up_mbps = mbps(UP_BYTES as u64, start.elapsed().as_secs_f64());
    used += UP_BYTES as u64;

    Ok(SpeedResult { latency_ms, down_mbps, up_mbps, bytes_used: used })
}

#[cfg(test)]
mod tests {
    #[test]
    fn parses_a_release() {
        let (tag, url) =
            super::parse_release(r#"{"tag_name":"v0.2.0","html_url":"https://github.com/o/r/releases/tag/v0.2.0"}"#)
                .unwrap();
        assert_eq!((tag.as_str(), url.as_str()), ("v0.2.0", "https://github.com/o/r/releases/tag/v0.2.0"));
        assert_eq!(
            super::parse_release(r#"{"message":"API rate limit exceeded"}"#).unwrap_err(),
            "API rate limit exceeded"
        );
    }

    #[test]
    fn the_newest_release_counts_early_builds_but_not_drafts() {
        let list = r#"[
            {"tag_name":"v0.4.0-beta.1","html_url":"https://github.com/o/r/releases/tag/v0.4.0-beta.1","prerelease":true},
            {"tag_name":"v0.5.0","html_url":"https://github.com/o/r/releases/tag/v0.5.0","draft":true},
            {"tag_name":"v0.3.0","html_url":"https://github.com/o/r/releases/tag/v0.3.0"},
            {"tag_name":"v0.4.0-beta.2","html_url":"https://github.com/o/r/releases/tag/v0.4.0-beta.2","prerelease":true}
        ]"#;
        let (tag, url) = super::parse_releases(list).unwrap();
        assert_eq!(tag, "v0.4.0-beta.2");
        assert!(url.ends_with("/v0.4.0-beta.2"));
        assert_eq!(super::parse_releases("[]").unwrap_err(), "no releases published yet");
        assert_eq!(super::parse_releases(r#"{"message":"Not Found"}"#).unwrap_err(), "Not Found");
    }
}
