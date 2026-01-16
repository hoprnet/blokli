//! Utility functions for the indexer module.

use url::Url;

/// Redacts username, password, and path/query parameters from URLs while keeping protocol, host, and port visible
///
/// This function is used to sanitize URLs before logging them, preventing exposure of
/// sensitive credentials or API tokens that might be embedded in the URL.
///
/// Works with any URL format including:
/// - RPC endpoints with API keys in paths
/// - HTTP Basic Auth credentials
/// - Query parameters with tokens
/// - Snapshot URLs with authentication
///
/// # Examples
/// ```
/// # use blokli_chain_indexer::utils::redact_url;
/// assert_eq!(
///     redact_url("https://user:pass@api.infura.io/v3/key123"),
///     "https://api.infura.io/REDACTED"
/// );
/// assert_eq!(
///     redact_url("https://api.infura.io/v3/key123"),
///     "https://api.infura.io/REDACTED"
/// );
/// assert_eq!(
///     redact_url("https://snapshots.example.com/file.tar.xz?token=secret"),
///     "https://snapshots.example.com/REDACTED"
/// );
/// assert_eq!(redact_url("http://localhost:8545"), "http://localhost:8545");
/// ```
pub fn redact_url(url: &str) -> String {
    let parsed = match Url::parse(url) {
        Ok(parsed) => parsed,
        Err(_) => return url.to_string(),
    };

    let host = match parsed.host_str() {
        Some(host) => host,
        None => return url.to_string(),
    };

    let host = match parsed.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_string(),
    };

    let has_userinfo = !parsed.username().is_empty() || parsed.password().is_some();
    let has_path = {
        let path = parsed.path();
        !path.is_empty() && path != "/"
    };
    let has_query = parsed.query().is_some();
    let has_fragment = parsed.fragment().is_some();

    if has_userinfo || has_path || has_query || has_fragment {
        format!("{}://{}/REDACTED", parsed.scheme(), host)
    } else {
        url.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redact_url_with_credentials_and_path() {
        assert_eq!(
            redact_url("https://user:pass@api.infura.io/v3/key123"),
            "https://api.infura.io/REDACTED"
        );
    }

    #[test]
    fn test_redact_url_with_path_only() {
        assert_eq!(
            redact_url("https://api.infura.io/v3/key123"),
            "https://api.infura.io/REDACTED"
        );
    }

    #[test]
    fn test_redact_url_localhost() {
        assert_eq!(redact_url("http://localhost:8545"), "http://localhost:8545");
    }

    #[test]
    fn test_redact_url_with_query_params() {
        assert_eq!(
            redact_url("https://api.example.com?apikey=secret123"),
            "https://api.example.com/REDACTED"
        );
    }

    #[test]
    fn test_redact_url_simple_host() {
        assert_eq!(redact_url("https://rpc.example.com"), "https://rpc.example.com");
    }

    #[test]
    fn test_redact_url_snapshot_with_token() {
        assert_eq!(
            redact_url("https://snapshots.hoprnet.org/logs.tar.xz?token=SECRET"),
            "https://snapshots.hoprnet.org/REDACTED"
        );
    }

    #[test]
    fn test_redact_url_with_at_in_path() {
        assert_eq!(
            redact_url("https://example.com/user@domain.com/file"),
            "https://example.com/REDACTED"
        );
    }

    #[test]
    fn test_redact_url_basic_auth() {
        assert_eq!(
            redact_url("https://admin:password123@internal.example.com/api"),
            "https://internal.example.com/REDACTED"
        );
    }
}
