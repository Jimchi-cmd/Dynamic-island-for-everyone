//! The speed test: run only when asked (`islet speedtest`, or the button in
//! Settings), against Cloudflare's public speed-test servers, the ones its
//! own open-source speed-test library uses. It moves about 36 MB.

use serde::Serialize;

/// Where the test downloads from (`bytes=` how much) and uploads to.
pub const DOWN_URL: &str = "https://speed.cloudflare.com/__down?bytes=";
pub const UP_URL: &str = "https://speed.cloudflare.com/__up";
/// How much is moved: a warm-up, the timed download, the timed upload.
pub const WARM_BYTES: u64 = 1_000_000;
pub const DOWN_BYTES: u64 = 25_000_000;
pub const UP_BYTES: usize = 10_000_000;
/// Round trips timed for the latency.
pub const PINGS: usize = 6;
/// A slow line stops early: whatever arrived in this time counts.
pub const MAX_SECS: f64 = 10.0;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeedResult {
    pub latency_ms: f64,
    pub down_mbps: f64,
    pub up_mbps: f64,
    /// Bytes downloaded and uploaded in all.
    pub bytes_used: u64,
}

/// Megabits a second for `bytes` in `secs`.
pub fn mbps(bytes: u64, secs: f64) -> f64 {
    if secs <= 0.0 {
        return 0.0;
    }
    bytes as f64 * 8.0 / secs / 1_000_000.0
}

/// The middle value (the mean of the middle two for an even count).
pub fn median(values: &[f64]) -> Option<f64> {
    let mut v: Vec<f64> = values.iter().copied().filter(|x| x.is_finite()).collect();
    if v.is_empty() {
        return None;
    }
    v.sort_by(f64::total_cmp);
    let mid = v.len() / 2;
    Some(if v.len() % 2 == 0 { (v[mid - 1] + v[mid]) / 2.0 } else { v[mid] })
}

/// "94.2" or "940": a decimal only for small numbers.
pub fn format_mbps(v: f64) -> String {
    if v < 100.0 {
        format!("{v:.1}")
    } else {
        format!("{v:.0}")
    }
}

/// "Down 94.2 Mbps · Up 18.5 Mbps · 23 ms"
pub fn describe(r: &SpeedResult) -> String {
    format!("Down {} Mbps · Up {} Mbps · {:.0} ms", format_mbps(r.down_mbps), format_mbps(r.up_mbps), r.latency_ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rates_and_medians() {
        assert!((mbps(25_000_000, 2.0) - 100.0).abs() < 1e-9);
        assert_eq!(mbps(10, 0.0), 0.0);
        assert_eq!(median(&[30.0, 10.0, 20.0]), Some(20.0));
        assert_eq!(median(&[4.0, 1.0, 3.0, 2.0]), Some(2.5));
        assert_eq!(median(&[f64::NAN]), None);
    }

    #[test]
    fn a_short_summary() {
        let r = SpeedResult { latency_ms: 23.4, down_mbps: 94.24, up_mbps: 512.6, bytes_used: 36_000_000 };
        assert_eq!(describe(&r), "Down 94.2 Mbps · Up 513 Mbps · 23 ms");
        assert_eq!(
            serde_json::to_string(&r).unwrap(),
            r#"{"latencyMs":23.4,"downMbps":94.24,"upMbps":512.6,"bytesUsed":36000000}"#
        );
    }
}
