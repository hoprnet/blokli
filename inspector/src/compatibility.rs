use anyhow::{Context, Result, anyhow};
use blokli_client::{
    BlokliClient, CLIENT_VERSION,
    api::{BlokliQueryClient, types::Compatibility},
};
use semver::{Version, VersionReq};

pub(crate) fn validate_client_compatibility(compatibility: &Compatibility) -> Result<()> {
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

pub(crate) async fn ensure_client_compatibility(client: &BlokliClient) -> Result<()> {
    let compatibility = client
        .query_compatibility()
        .await
        .with_context(|| format!("failed to query client compatibility from {}", client.base_url()))?;

    validate_client_compatibility(&compatibility)
}

#[cfg(test)]
mod tests {
    use blokli_client::api::types::Compatibility;

    use super::validate_client_compatibility;

    #[test]
    fn accepts_matching_client_version() {
        validate_client_compatibility(&Compatibility {
            api_version: "0.19.1".to_string(),
            supported_client_versions: "^0.24".to_string(),
        })
        .expect("compatibility check should pass");
    }

    #[test]
    fn rejects_out_of_range_client_version() {
        let error = validate_client_compatibility(&Compatibility {
            api_version: "0.19.1".to_string(),
            supported_client_versions: "^0.25".to_string(),
        })
        .expect_err("compatibility check should fail");

        assert!(error.to_string().contains("server supports client versions ^0.25"));
    }
}
