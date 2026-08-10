use std::cmp::Ordering;

fn parse_parts(version: &str) -> Option<[u64; 4]> {
    let mut out = [0; 4];
    let parts: Vec<&str> = version.trim().split('.').collect();
    if parts.is_empty() || parts.len() > 4 {
        return None;
    }
    for (idx, part) in parts.iter().enumerate() {
        out[idx] = part.parse().ok()?;
    }
    Some(out)
}

pub fn compare_versions(a: &str, b: &str) -> Ordering {
    match (parse_parts(a), parse_parts(b)) {
        (Some(a), Some(b)) => a.cmp(&b),
        _ => a.cmp(b),
    }
}

/// Stable compact key for the shared provenance store's numeric `build` field.
pub fn version_key(version: &str) -> u64 {
    let Some(parts) = parse_parts(version) else {
        return 0;
    };
    ((parts[0].min(u16::MAX as u64)) << 48)
        | ((parts[1].min(u16::MAX as u64)) << 32)
        | ((parts[2].min(u16::MAX as u64)) << 16)
        | parts[3].min(u16::MAX as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_dotted_windows_versions_numerically() {
        assert_eq!(
            compare_versions("26.602.3474.0", "26.602.999.0"),
            Ordering::Greater
        );
        assert_eq!(
            compare_versions("26.602.3474.0", "26.602.3474.0"),
            Ordering::Equal
        );
        assert_eq!(
            compare_versions("26.602.3474.0", "27.0.0.0"),
            Ordering::Less
        );
    }

    #[test]
    fn builds_stable_provenance_key() {
        assert_eq!(version_key("26.602.3474.0"), version_key("26.602.3474.0"));
        assert_ne!(version_key("26.602.3474.0"), version_key("26.602.3475.0"));
    }
}
