//! Release versions ("v0.2.1", "0.2.1-beta.2") and whether one is newer.

/// (major, minor, patch, pre-release). A pre-release sorts before its release.
fn parse(v: &str) -> Option<(u64, u64, u64, Option<String>)> {
    let v = v.trim().trim_start_matches('v');
    let (core, pre) = match v.split_once('-') {
        Some((c, p)) => (c, Some(p.to_string())),
        None => (v, None),
    };
    let mut it = core.split('.').map(|n| n.parse::<u64>().ok());
    let (a, b, c) = (it.next()??, it.next().unwrap_or(Some(0))?, it.next().unwrap_or(Some(0))?);
    Some((a, b, c, pre))
}

/// True if `candidate` is a newer release than `current`.
pub fn is_newer(candidate: &str, current: &str) -> bool {
    let (Some(a), Some(b)) = (parse(candidate), parse(current)) else { return false };
    match (a.0, a.1, a.2).cmp(&(b.0, b.1, b.2)) {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Less => false,
        std::cmp::Ordering::Equal => match (&a.3, &b.3) {
            (None, Some(_)) => true,
            (Some(x), Some(y)) => x > y,
            _ => false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::is_newer;

    #[test]
    fn compares_versions() {
        assert!(is_newer("v0.2.0", "0.1.9"));
        assert!(is_newer("0.10.0", "0.9.0"), "numeric, not lexical");
        assert!(is_newer("1.0.0", "1.0.0-beta.1"));
        assert!(is_newer("1.0.0-beta.2", "1.0.0-beta.1"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.1.0-rc.1", "0.1.0"));
        assert!(!is_newer("garbage", "0.1.0"));
        assert!(is_newer("v1.2", "1.1.9"));
    }
}
