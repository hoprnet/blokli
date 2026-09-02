//! Resolution of historical contract deployments from pinned bindings releases.

use std::{fmt, str::FromStr};

use hopr_types::primitive::prelude::Address;
use semver::Version;
use serde::{Deserialize, Serialize};

/// A canonical GitHub contracts release tag such as `v1.2.3`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ContractsRelease(String);

impl ContractsRelease {
    /// Returns the canonical release tag.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ContractsRelease {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for ContractsRelease {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let version = value
            .strip_prefix('v')
            .ok_or_else(|| "contracts release must start with 'v'".to_string())?;
        let version =
            Version::parse(version).map_err(|error| format!("invalid contracts release '{value}': {error}"))?;

        Ok(Self(format!("v{version}")))
    }
}

impl TryFrom<String> for ContractsRelease {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<ContractsRelease> for String {
    fn from(value: ContractsRelease) -> Self {
        value.0
    }
}

/// StakeFactory metadata normalized across historical bindings crate versions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StakeFactoryDeployment {
    /// Address of the historical StakeFactory.
    pub address: Address,
    /// Chain ID declared by the historical network manifest.
    pub chain_id: u64,
    /// Safe lower bound from which the historical release should be indexed.
    pub indexer_start_block_number: u32,
}

/// Generates the resolver for every supported historical bindings release.
///
/// Each entry maps a contracts release tag to an exactly pinned Cargo dependency alias. The
/// generated resolver normalizes the version-specific manifest into [`StakeFactoryDeployment`].
macro_rules! historical_bindings {
    ($($release:literal => $bindings:ident),* $(,)?) => {
        /// Resolves a StakeFactory from the deployment manifest of a supported historical release.
        pub fn resolve_stake_factory(
            release: &ContractsRelease,
            network_name: &str,
        ) -> Option<StakeFactoryDeployment> {
            let _ = network_name;
            match release.as_str() {
                $(
                    $release => {
                        let networks = $bindings::config::NetworksWithContractAddresses::default();
                        let network = networks.networks.get(network_name)?;
                        let address_bytes: [u8; 20] = network
                            .addresses
                            .node_stake_factory
                            .as_slice()
                            .try_into()
                            .ok()?;

                        Some(StakeFactoryDeployment {
                            address: Address::from(address_bytes),
                            chain_id: network.chain_id,
                            indexer_start_block_number: network.indexer_start_block_number,
                        })
                    }
                )*
                _ => None,
            }
        }
    };
}

// Add historical bindings releases here after declaring their pinned Cargo dependency aliases.
historical_bindings! {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contracts_release_parsing() {
        assert_eq!(
            "v1.2.3"
                .parse::<ContractsRelease>()
                .expect("release should parse")
                .as_str(),
            "v1.2.3"
        );
        assert_eq!(
            "v1.0.0-rc.1"
                .parse::<ContractsRelease>()
                .expect("pre-release should parse")
                .as_str(),
            "v1.0.0-rc.1"
        );
        assert!("1.2.3".parse::<ContractsRelease>().is_err());
        assert!("vnot-a-version".parse::<ContractsRelease>().is_err());
    }
}
