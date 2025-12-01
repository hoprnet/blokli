//! Network enum and related functionality for identifying blockchain networks.
//!
//! This module provides the [`Network`] enum which represents supported HOPR networks
//! and provides conversions between string identifiers and network definitions.

use std::{fmt, str::FromStr};

use hopr_bindings::config::{NetworksWithContractAddresses, SingleNetworkContractAddresses};
use serde::{Deserialize, Serialize};

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
    /// Dufour testnet
    #[default]
    #[serde(alias = "dufour")]
    Dufour,
    /// Rotsee mainnet
    #[serde(alias = "rotsee")]
    Rotsee,
}

impl Network {
    /// Returns all available networks as a vector.
    ///
    /// This is useful for generating error messages that show users
    /// what networks are supported.
    pub fn all() -> Vec<Network> {
        vec![Network::AnvilLocalhost, Network::Dufour, Network::Rotsee]
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
            Network::Dufour => "dufour",
            Network::Rotsee => "rotsee",
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
    /// let network = Network::Dufour;
    /// if let Some(config) = network.resolve() {
    ///     println!("Start block: {}", config.indexer_start_block_number);
    /// }
    /// ```
    pub fn resolve(&self) -> Option<SingleNetworkContractAddresses> {
        let networks = NetworksWithContractAddresses::default();
        networks.networks.get(self.as_str()).copied()
    }

    /// Returns the chain ID for this network.
    pub fn chain_id(&self) -> u64 {
        match self {
            Network::AnvilLocalhost => 31337,
            Network::Dufour => 100, // Gnosis Chain
            Network::Rotsee => 100, // Gnosis Chain
        }
    }

    /// Returns the expected block time in milliseconds.
    pub fn block_time(&self) -> u64 {
        match self {
            Network::AnvilLocalhost => 1000, // 1 second
            Network::Dufour => 5000,         // 5 seconds
            Network::Rotsee => 5000,         // 5 seconds
        }
    }

    /// Returns the transaction polling interval in milliseconds.
    pub fn tx_polling_interval(&self) -> u64 {
        match self {
            Network::AnvilLocalhost => 100,
            Network::Dufour => 1000,
            Network::Rotsee => 1000,
        }
    }

    /// Returns the number of confirmations (finality).
    pub fn confirmations(&self) -> u16 {
        match self {
            Network::AnvilLocalhost => 1,
            Network::Dufour => 5,
            Network::Rotsee => 12,
        }
    }

    /// Returns the maximum block range for RPC queries.
    pub fn max_block_range(&self) -> u32 {
        match self {
            Network::AnvilLocalhost => 10000,
            Network::Dufour => 1000,
            Network::Rotsee => 1000,
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
            "dufour" => Ok(Network::Dufour),
            "rotsee" => Ok(Network::Rotsee),
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
        assert_eq!("dufour".parse::<Network>().unwrap(), Network::Dufour);
        assert_eq!("DUFOUR".parse::<Network>().unwrap(), Network::Dufour);
        assert_eq!("rotsee".parse::<Network>().unwrap(), Network::Rotsee);
        assert_eq!("anvil-localhost".parse::<Network>().unwrap(), Network::AnvilLocalhost);
        assert_eq!("localhost".parse::<Network>().unwrap(), Network::AnvilLocalhost);
    }

    #[test]
    fn test_network_from_str_invalid() {
        let result = "invalid-network".parse::<Network>();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("invalid-network"));
        assert!(err.to_string().contains("dufour"));
    }

    #[test]
    fn test_network_display() {
        assert_eq!(Network::Dufour.to_string(), "dufour");
        assert_eq!(Network::Rotsee.to_string(), "rotsee");
        assert_eq!(Network::AnvilLocalhost.to_string(), "anvil-localhost");
    }

    #[test]
    fn test_network_all() {
        let networks = Network::all();
        assert_eq!(networks.len(), 3);
        assert!(networks.contains(&Network::AnvilLocalhost));
        assert!(networks.contains(&Network::Dufour));
        assert!(networks.contains(&Network::Rotsee));
    }

    #[test]
    fn test_network_default() {
        assert_eq!(Network::default(), Network::Dufour);
    }

    #[test]
    fn test_network_resolve() {
        // Test that networks can be resolved
        // Note: This test depends on hopr-bindings having these networks defined
        let dufour = Network::Dufour.resolve();
        assert!(dufour.is_some(), "Dufour network should be defined in hopr-bindings");
    }
}
