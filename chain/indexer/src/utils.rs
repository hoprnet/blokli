//! Utility functions for the indexer module.

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
    // Parse the URL to extract components
    if let Some(scheme_end) = url.find("://") {
        let scheme = &url[..scheme_end + 3];
        let rest = &url[scheme_end + 3..];

        // Check if there's an @ sign indicating credentials
        let (host_part, has_credentials) = if let Some(at_pos) = rest.find('@') {
            (&rest[at_pos + 1..], true)
        } else {
            (rest, false)
        };

        // Extract host and port (everything before the first / or ?)
        let host_end = host_part
            .find('/')
            .or_else(|| host_part.find('?'))
            .unwrap_or(host_part.len());
        let host = &host_part[..host_end];
        let has_path = host_end < host_part.len();

        // Reconstruct URL
        if has_credentials || has_path {
            format!("{}{}/REDACTED", scheme, host)
        } else {
            // No credentials or path, return as-is
            url.to_string()
        }
    } else {
        // Not a URL format, return as-is
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
    fn test_redact_url_basic_auth() {
        assert_eq!(
            redact_url("https://admin:password123@internal.example.com/api"),
            "https://internal.example.com/REDACTED"
        );
    }
}
