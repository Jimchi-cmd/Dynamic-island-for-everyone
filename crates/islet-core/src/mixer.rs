//! The per-app volume mixer: each app playing sound, with its own volume.

/// One app's sound.
#[derive(Debug, Clone, PartialEq)]
pub struct AppVolume {
    /// What the system calls it, to set it ("42" for a PulseAudio sink
    /// input, a process id on Windows).
    pub id: String,
    pub name: String,
    /// 0..=1 (the system may allow more; shown as full).
    pub level: f32,
    pub muted: bool,
}

/// `LC_ALL=C pactl list sink-inputs`: one entry per stream. Streams of the
/// same app are kept apart (each has its own volume).
pub fn parse_sink_inputs(out: &str) -> Vec<AppVolume> {
    let mut v: Vec<AppVolume> = Vec::new();
    let mut binary = String::new();
    for line in out.lines() {
        let t = line.trim();
        if let Some(n) = t.strip_prefix("Sink Input #") {
            // The previous entry falls back to its program's name.
            if let Some(last) = v.last_mut().filter(|l| l.name.is_empty()) {
                last.name = std::mem::take(&mut binary);
            }
            binary.clear();
            v.push(AppVolume { id: n.trim().to_string(), name: String::new(), level: 1.0, muted: false });
            continue;
        }
        let Some(cur) = v.last_mut() else { continue };
        if let Some(rest) = t.strip_prefix("Volume:") {
            let pcts: Vec<f32> =
                rest.split('/').filter_map(|p| p.trim().strip_suffix('%')?.trim().parse::<f32>().ok()).collect();
            if !pcts.is_empty() {
                cur.level = pcts.iter().sum::<f32>() / pcts.len() as f32 / 100.0;
            }
        } else if let Some(m) = t.strip_prefix("Mute:") {
            cur.muted = m.trim() == "yes";
        } else if let Some(name) = quoted(t, "application.name = ") {
            cur.name = name;
        } else if let Some(bin) = quoted(t, "application.process.binary = ") {
            binary = bin;
        }
    }
    if let Some(last) = v.last_mut().filter(|l| l.name.is_empty()) {
        last.name = binary;
    }
    v.retain(|a| !a.name.is_empty());
    v
}

fn quoted(line: &str, key: &str) -> Option<String> {
    let rest = line.strip_prefix(key)?.trim();
    Some(rest.strip_prefix('"')?.strip_suffix('"')?.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_each_app_stream() {
        let out = r#"Sink Input #42
	Driver: protocol-native.c
	Sink: 1
	Volume: front-left: 32768 /  50% / -18.06 dB,   front-right: 32768 /  50% / -18.06 dB
	        balance 0.00
	Mute: no
	Properties:
		application.name = "Firefox"
		application.process.binary = "firefox"
Sink Input #57
	Volume: mono: 65536 / 100% / 0.00 dB
	Mute: yes
	Properties:
		application.process.binary = "mpv"
Sink Input #60
	Volume: front-left: 65536 / 100% / 0.00 dB
	Mute: no
"#;
        let v = parse_sink_inputs(out);
        assert_eq!(v.len(), 2, "a stream nobody named is left out: {v:?}");
        assert_eq!(v[0], AppVolume { id: "42".into(), name: "Firefox".into(), level: 0.5, muted: false });
        assert_eq!(v[1], AppVolume { id: "57".into(), name: "mpv".into(), level: 1.0, muted: true });
        assert!(parse_sink_inputs("").is_empty());
    }
}
