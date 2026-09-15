//! Utilities for safely displaying database connection details.

use url::Url;

/// Redacts credentials from a database URL while keeping non-sensitive connection details visible.
///
/// User information, query strings, and fragments can all carry credentials. Query strings and
/// fragments are therefore redacted in full rather than filtered using a denylist. Inputs that
/// are not supported database URLs are also redacted in full.
pub fn redact_database_url(url: &str) -> String {
    const REDACTED: &str = "REDACTED";

    let mut parsed = match Url::parse(url) {
        Ok(parsed) => parsed,
        Err(_) => return REDACTED.to_string(),
    };

    match parsed.scheme() {
        "postgres" | "postgresql" if parsed.host_str().is_some() => {}
        "sqlite" => {}
        _ => return REDACTED.to_string(),
    }

    if (!parsed.username().is_empty() || parsed.password().is_some())
        && (parsed.set_username(REDACTED).is_err() || parsed.set_password(Some(REDACTED)).is_err())
    {
        return REDACTED.to_string();
    }

    if parsed.query().is_some() {
        parsed.set_query(Some(REDACTED));
    }

    if parsed.fragment().is_some() {
        parsed.set_fragment(Some(REDACTED));
    }

    parsed.to_string()
}

#[cfg(test)]
mod tests {
    use super::redact_database_url;

    #[test]
    fn test_redact_database_url_with_credentials() {
        let redacted = redact_database_url("postgres://user:password@localhost:5432/mydb");

        assert_eq!(redacted, "postgres://REDACTED:REDACTED@localhost:5432/mydb");
    }

    #[test]
    fn test_redact_database_url_without_credentials() {
        let url = "postgresql://localhost:5432/mydb";

        assert_eq!(redact_database_url(url), url);
    }

    #[test]
    fn test_redact_database_url_with_at_sign_in_password() {
        let redacted = redact_database_url("postgresql://blokli:secret@fragment@localhost:5432/blokli");

        assert_eq!(redacted, "postgresql://REDACTED:REDACTED@localhost:5432/blokli");
        assert!(!redacted.contains("secret"));
        assert!(!redacted.contains("fragment"));
    }

    #[test]
    fn test_redact_database_url_does_not_treat_path_at_sign_as_userinfo() {
        let url = "postgresql://localhost:5432/blokli@archive";

        assert_eq!(redact_database_url(url), url);
    }

    #[test]
    fn test_redact_database_url_redacts_entire_query() {
        let password = redact_database_url("postgresql://localhost/blokli?password=secret&sslmode=require");
        let unknown = redact_database_url("postgresql://localhost/blokli?auth=hunter2");
        let secret_in_key = redact_database_url("postgresql://localhost/blokli?password%3Dhunter2");

        assert_eq!(password, "postgresql://localhost/blokli?REDACTED");
        assert_eq!(unknown, "postgresql://localhost/blokli?REDACTED");
        assert_eq!(secret_in_key, "postgresql://localhost/blokli?REDACTED");
    }

    #[test]
    fn test_redact_unsupported_database_url() {
        assert_eq!(redact_database_url("password=secret"), "REDACTED");
        assert_eq!(
            redact_database_url("postgresql:host=localhost password=secret"),
            "REDACTED"
        );
    }
}
