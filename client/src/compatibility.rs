use anyhow::{Context, Result, anyhow};
use semver::{Version, VersionReq};

use crate::{CLIENT_VERSION, api::types::Compatibility};

pub fn validate_client_compatibility(compatibility: &Compatibility) -> Result<()> {
    let current_version =
        Version::parse(CLIENT_VERSION).context("failed to parse the current blokli-client version")?;
    let supported_versions = VersionReq::parse(&compatibility.supported_client_versions)
        .context("failed to parse the server-supported blokli-client semver range")?;

    if supported_versions.matches(&current_version) {
        return Ok(());
    }

    Err(anyhow!(
        "incompatible blokli-client version {} for blokli-api {}: server supports client versions {}",
        CLIENT_VERSION,
        compatibility.api_version,
        compatibility.supported_client_versions
    ))
}

#[cfg(test)]
mod tests {
    use super::validate_client_compatibility;
    use crate::{CLIENT_VERSION, api::types::Compatibility};

    #[test]
    fn accepts_matching_client_version() {
        validate_client_compatibility(&Compatibility {
            api_version: "0.19.1".to_string(),
            supported_client_versions: format!("={CLIENT_VERSION}"),
        })
        .expect("compatibility check should pass");
    }

    #[test]
    fn rejects_out_of_range_client_version() {
        let supported_versions = "<0.0.0".to_string();
        let error = validate_client_compatibility(&Compatibility {
            api_version: "0.19.1".to_string(),
            supported_client_versions: supported_versions.clone(),
        })
        .expect_err("compatibility check should fail");

        assert!(
            error
                .to_string()
                .contains(&format!("server supports client versions {}", supported_versions))
        );
    }
}
