//! Network enum and related functionality for identifying blockchain networks.
//!
//! This module provides the [`Network`] enum which represents supported HOPR networks
//! and provides conversions between string identifiers and network definitions.

use std::{fmt, str::FromStr};

use hopr_bindings::config::{NetworksWithContractAddresses, SingleNetworkContractAddresses};
//use hopr_bindings_v4_13::config::NetworksWithContractAddresses as NetworksWithContractAddressesV4_13;
use hopr_types::primitive::prelude::Address;
use serde::{Deserialize, Serialize};

/// Contracts releases whose deployment metadata Blokli can use for historical indexing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContractsRelease {
    // /// Contracts release v4.13.0.
    // #[serde(rename = "v4.13.0")]
    // V4_13_0,
}

impl fmt::Display for ContractsRelease {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // ContractsRelease::V4_13_0 => formatter.write_str("v4.13.0"),
        }
    }
}

/// Stake factory deployment metadata normalized from a historical bindings release.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StakeFactoryDeployment {
    pub address: Address,
    pub chain_id: u64,
    pub indexer_start_block_number: u32,
}

/// Supported HOPR networks.
///
/// This enum represents the blockchain networks that can be used with blokli.
/// Network names are case-insensitive when parsing from strings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Network {
    /// Local Anvil development network
    #[serde(alias = "anvil_localhost", alias = "anvil-localhost", alias = "localhost")]
    AnvilLocalhost,
    /// Jura development (development/testing network)
    #[default]
    #[serde(alias = "jura-dev")]
    JuraDev,
    /// Jura production (staging network)
    #[serde(alias = "jura-prod")]
    JuraProd,
    /// PizPalu development network (development/testing network)
    #[serde(alias = "piz-palu-dev")]
    PizPaluDev,
    /// PizPalu staging network (staging network)
    #[serde(alias = "piz-palu-staging")]
    PizPaluStaging,
}

impl Network {
    /// Returns all available networks as a vector.
    ///
    /// This is useful for generating error messages that show users
    /// what networks are supported.
    pub fn all() -> Vec<Network> {
        vec![
            Network::AnvilLocalhost,
            Network::JuraDev,
            Network::JuraProd,
            Network::PizPaluDev,
            Network::PizPaluStaging,
        ]
    }

    /// Returns all available network names as strings.
    ///
    /// This is useful for generating error messages.
    pub fn all_names() -> Vec<String> {
        Self::all().iter().map(|n| n.to_string()).collect()
    }

    /// Returns the network identifier string used by hopr-bindings.
    ///
    /// This is the canonical string identifier for the network
    /// in the HOPR ecosystem.
    pub fn as_str(&self) -> &'static str {
        match self {
            Network::AnvilLocalhost => "anvil-localhost",
            Network::JuraDev => "jura-dev",
            Network::JuraProd => "jura-prod",
            Network::PizPaluDev => "piz-palu-dev",
            Network::PizPaluStaging => "piz-palu-staging",
        }
    }

    /// Resolves the network to its contract addresses from hopr-bindings.
    ///
    /// # Returns
    ///
    /// Returns the network configuration if found, or `None` if not defined.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use bloklid::network::Network;
    ///
    /// let network = Network::JuraDev;
    /// if let Some(config) = network.resolve() {
    ///     println!("Start block: {}", config.indexer_start_block_number);
    /// }
    /// ```
    pub fn resolve(&self) -> Option<SingleNetworkContractAddresses> {
        let networks = NetworksWithContractAddresses::default();
        networks.networks.get(self.as_str()).copied()
    }

    /// Resolves a historical StakeFactory deployment for this network.
    pub fn resolve_stake_factory(&self, release: ContractsRelease) -> Option<StakeFactoryDeployment> {
        match release {
            // ContractsRelease::V4_13_0 => {
            //     let networks = NetworksWithContractAddressesV4_13::default();
            //     let network = networks.networks.get(self.as_str())?;
            //     let address_bytes: [u8; 20] = network.addresses.node_stake_factory.as_slice().try_into().ok()?;

            //     Some(StakeFactoryDeployment {
            //         address: Address::from(address_bytes),
            //         chain_id: network.chain_id,
            //         indexer_start_block_number: network.indexer_start_block_number,
            //     })
            // }
        }
    }

    /// Returns the transaction polling interval in milliseconds.
    pub fn tx_polling_interval(&self) -> u64 {
        match self {
            Network::AnvilLocalhost => 100,
            Network::JuraDev => 1000,
            Network::JuraProd => 1000,
            Network::PizPaluDev => 1000,
            Network::PizPaluStaging => 1000,
        }
    }

    /// Returns the number of confirmations (finality).
    pub fn confirmations(&self) -> u16 {
        match self {
            Network::AnvilLocalhost => 1,
            Network::JuraDev => 3,
            Network::JuraProd => 3,
            Network::PizPaluDev => 3,
            Network::PizPaluStaging => 3,
        }
    }

    /// Returns the expected block time in seconds.
    pub fn expected_block_time(&self) -> u64 {
        match self {
            Network::AnvilLocalhost => 1,
            Network::JuraDev => 5,
            Network::JuraProd => 5,
            Network::PizPaluDev => 5,
            Network::PizPaluStaging => 5,
        }
    }
}

impl fmt::Display for Network {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for Network {
    type Err = NetworkParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "anvil-localhost" | "anvil_localhost" | "localhost" => Ok(Network::AnvilLocalhost),
            "jura-dev" => Ok(Network::JuraDev),
            "jura-prod" => Ok(Network::JuraProd),
            "piz-palu-dev" => Ok(Network::PizPaluDev),
            "piz-palu-staging" => Ok(Network::PizPaluStaging),
            _ => Err(NetworkParseError::UnknownNetwork {
                name: s.to_string(),
                available: Self::all_names(),
            }),
        }
    }
}

/// Error type for network parsing failures.
#[derive(Debug, Clone, thiserror::Error)]
pub enum NetworkParseError {
    /// The specified network name is not recognized.
    #[error("Unknown network '{name}'. Supported networks: {}", available.join(", "))]
    UnknownNetwork {
        /// The network name that was provided
        name: String,
        /// List of available network names
        available: Vec<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_from_str() {
        assert_eq!("jura-dev".parse::<Network>().unwrap(), Network::JuraDev);
        assert_eq!("JURA-DEV".parse::<Network>().unwrap(), Network::JuraDev);
        assert_eq!("jura-prod".parse::<Network>().unwrap(), Network::JuraProd);
        assert_eq!("piz-palu-dev".parse::<Network>().unwrap(), Network::PizPaluDev);
        assert_eq!("piz-palu-staging".parse::<Network>().unwrap(), Network::PizPaluStaging);
        assert_eq!("anvil-localhost".parse::<Network>().unwrap(), Network::AnvilLocalhost);
        assert_eq!("localhost".parse::<Network>().unwrap(), Network::AnvilLocalhost);
    }

    #[test]
    fn test_network_from_str_invalid() {
        let result = "invalid-network".parse::<Network>();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("invalid-network"));
        assert!(err.to_string().contains("jura-dev"));
        assert!(err.to_string().contains("jura-prod"));
        assert!(err.to_string().contains("piz-palu-dev"));
        assert!(err.to_string().contains("piz-palu-staging"));
    }

    #[test]
    fn test_network_display() {
        assert_eq!(Network::AnvilLocalhost.to_string(), "anvil-localhost");
        assert_eq!(Network::JuraDev.to_string(), "jura-dev");
        assert_eq!(Network::JuraProd.to_string(), "jura-prod");
        assert_eq!(Network::PizPaluDev.to_string(), "piz-palu-dev");
        assert_eq!(Network::PizPaluStaging.to_string(), "piz-palu-staging");
    }

    #[test]
    fn test_network_all() {
        let networks = Network::all();
        assert_eq!(networks.len(), 5);
        assert!(networks.contains(&Network::AnvilLocalhost));
        assert!(networks.contains(&Network::JuraDev));
        assert!(networks.contains(&Network::JuraProd));
        assert!(networks.contains(&Network::PizPaluDev));
        assert!(networks.contains(&Network::PizPaluStaging));
    }

    #[test]
    fn test_network_default() {
        assert_eq!(Network::default(), Network::JuraDev);
    }

    #[test]
    fn test_network_resolve() {
        // Test that networks can be resolved
        // Note: This test depends on hopr-bindings having these networks defined
        let anvil = Network::AnvilLocalhost.resolve();
        assert!(
            anvil.is_some(),
            "AnvilLocalhost network should be defined in hopr-bindings"
        );

        let jura_dev = Network::JuraDev.resolve();
        assert!(jura_dev.is_some(), "JuraDev network should be defined in hopr-bindings");

        let jura_prod = Network::JuraProd.resolve();
        assert!(
            jura_prod.is_some(),
            "JuraProd network should be defined in hopr-bindings"
        );

        let piz_palu_dev = Network::PizPaluDev.resolve();
        assert!(
            piz_palu_dev.is_some(),
            "PizPaluDev network should be defined in hopr-bindings"
        );

        let piz_palu_staging = Network::PizPaluStaging.resolve();
        assert!(
            piz_palu_staging.is_some(),
            "PizPaluStaging network should be defined in hopr-bindings"
        );
    }

    #[test]
    fn test_resolve_v4_13_stake_factory() {
        let deployment = Network::PizPaluStaging
            .resolve_stake_factory(ContractsRelease::V4_13_0)
            .expect("v4.13.0 should define piz-palu-staging");

        assert_eq!(deployment.chain_id, 100);
        assert_eq!(deployment.indexer_start_block_number, 47_638_474);
        assert_eq!(
            deployment.address.to_string(),
            "0x5b16003552bafc1be2aaa21d961fb90b1da23f17"
        );
    }
}
