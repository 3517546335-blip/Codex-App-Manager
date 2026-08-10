use url::Url;

pub const PRIMARY_MIRROR_HOST: &str = "codexapp.agentsmirror.com";
pub const R2_MIRROR_HOST: &str = "codexapp-r2.agentsmirror.com";

const RETRYABLE_CURL_EXITS: &[i32] = &[
    5, 6, 7, 28, 35, 52, 53, 54, 55, 56, 58, 59, 60, 67, 77, 80, 82, 83, 91,
];

pub fn r2_fallback_url(raw: &str) -> Option<String> {
    let mut url = Url::parse(raw).ok()?;
    if url.scheme() != "https"
        || url.host_str() != Some(PRIMARY_MIRROR_HOST)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
    {
        return None;
    }
    url.set_host(Some(R2_MIRROR_HOST)).ok()?;
    Some(url.into())
}

pub fn is_retryable_mirror_error(message: &str) -> bool {
    let curl_exit = message
        .split("exit=")
        .skip(1)
        .filter_map(|suffix| {
            let digits = suffix
                .chars()
                .take_while(|ch| ch.is_ascii_digit())
                .collect::<String>();
            (!digits.is_empty())
                .then(|| digits.parse::<i32>().ok())
                .flatten()
        })
        .last();
    if curl_exit.is_some_and(|code| RETRYABLE_CURL_EXITS.contains(&code)) {
        return true;
    }

    let lower = message.to_ascii_lowercase();
    lower.contains("download stalled")
        || lower.contains("download exceeded total deadline")
        || lower.contains("timeout was reached")
        || lower.contains("operation timed out")
        || lower.contains("failed to connect")
        || (500..=599).any(|status| {
            [
                format!("http {status}"),
                format!("http status {status}"),
                format!("error: {status}"),
                format!("status code: {status}"),
            ]
            .iter()
            .any(|needle| lower.contains(needle))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites_only_the_exact_trusted_primary_host() {
        assert_eq!(
            r2_fallback_url("https://codexapp.agentsmirror.com/latest/win-x64?x=1").as_deref(),
            Some("https://codexapp-r2.agentsmirror.com/latest/win-x64?x=1")
        );
        assert!(r2_fallback_url("http://codexapp.agentsmirror.com/latest/win-x64").is_none());
        assert!(r2_fallback_url("https://evil.example/latest/win-x64").is_none());
        assert!(r2_fallback_url("https://codexapp.agentsmirror.com.evil.example/file").is_none());
        assert!(r2_fallback_url("https://codexapp.agentsmirror.com:444/file").is_none());
    }

    #[test]
    fn retries_transport_and_server_failures_but_not_integrity_or_http_404() {
        assert!(is_retryable_mirror_error(
            "curl failed exit=28: Timeout was reached"
        ));
        assert!(is_retryable_mirror_error(
            "curl failed exit=22: The requested URL returned error: 522"
        ));
        assert!(is_retryable_mirror_error(
            "curl download stalled for 120 seconds"
        ));
        assert!(!is_retryable_mirror_error("curl failed exit=22: HTTP 404"));
        assert!(!is_retryable_mirror_error("SHA256 mismatch"));
        assert!(!is_retryable_mirror_error("download cancelled"));
    }
}
