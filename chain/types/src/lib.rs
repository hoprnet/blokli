//! This crate contains various on-chain related modules and types.
use alloy::{
    contract::Result as ContractResult, network::TransactionBuilder, primitives, rpc::types::TransactionRequest,
};
use constants::{ERC_1820_DEPLOYER, ERC_1820_REGISTRY_DEPLOY_CODE, ETH_VALUE_FOR_ERC1820_DEPLOYER};
use hopr_bindings::{
    hopr_announcements::HoprAnnouncements::{self, HoprAnnouncementsInstance},
    hopr_channels::HoprChannels::{self, HoprChannelsInstance},
    hopr_node_safe_registry::HoprNodeSafeRegistry::{self, HoprNodeSafeRegistryInstance},
    hopr_node_stake_factory::HoprNodeStakeFactory::{self, HoprNodeStakeFactoryInstance},
    hopr_ticket_price_oracle::HoprTicketPriceOracle::{self, HoprTicketPriceOracleInstance},
    hopr_token::HoprToken::{self, HoprTokenInstance},
    hopr_winning_probability_oracle::HoprWinningProbabilityOracle::{self, HoprWinningProbabilityOracleInstance},
};
use hopr_crypto_types::keypairs::{ChainKeypair, Keypair};
use hopr_primitive_types::primitives::Address;
use serde::{Deserialize, Serialize};

pub mod actions;
pub mod chain_events;
pub mod constants;
pub mod errors;
// Various (mostly testing related) utility functions
pub mod utils;

/// Chain configuration containing blockchain-specific parameters.
///
/// This struct encapsulates chain-level configuration needed by the indexer and RPC operations.
/// Chain ID and contract addresses are resolved from hopr-bindings network definitions.
#[derive(Clone, Debug)]
pub struct ChainConfig {
    /// Chain ID (e.g., 100 for Gnosis Chain) - read from hopr-bindings
    pub chain_id: u64,
    /// Transaction polling interval in milliseconds
    pub tx_polling_interval: u64,
    /// Number of confirmations required (finality)
    pub confirmations: u16,
    /// Maximum block range for RPC queries
    pub max_block_range: u32,
    /// Starting block number for channel contract (where indexing should begin)
    pub channel_contract_deploy_block: u32,
    /// Maximum RPC requests per second (None = unlimited)
    pub max_requests_per_sec: Option<u32>,
}

/// Holds addresses of all smart contracts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ContractAddresses {
    /// Token contract
    pub token: Address,
    /// Channels contract
    pub channels: Address,
    /// Announcements contract
    pub announcements: Address,
    /// Safe registry contract
    pub node_safe_registry: Address,
    /// Price oracle contract
    pub ticket_price_oracle: Address,
    /// Minimum ticket winning probability contract
    pub winning_probability_oracle: Address,
    /// Stake factory contract
    pub node_stake_v2_factory: Address,
}

/// Holds instances to contracts.
#[derive(Debug)]
pub struct ContractInstances<P> {
    pub token: HoprTokenInstance<P>,
    pub channels: HoprChannelsInstance<P>,
    pub announcements: HoprAnnouncementsInstance<P>,
    pub safe_registry: HoprNodeSafeRegistryInstance<P>,
    pub price_oracle: HoprTicketPriceOracleInstance<P>,
    pub win_prob_oracle: HoprWinningProbabilityOracleInstance<P>,
    pub stake_factory: HoprNodeStakeFactoryInstance<P>,
}

impl<P> ContractInstances<P>
where
    P: alloy::providers::Provider + Clone,
{
    pub fn new(contract_addresses: &ContractAddresses, provider: P, _use_dummy_nr: bool) -> Self {
        Self {
            token: HoprTokenInstance::new(
                primitives::Address::from(
                    <[u8; 20]>::try_from(contract_addresses.token.as_ref()).expect("Address is 20 bytes"),
                ),
                provider.clone(),
            ),
            channels: HoprChannelsInstance::new(
                primitives::Address::from(
                    <[u8; 20]>::try_from(contract_addresses.channels.as_ref()).expect("Address is 20 bytes"),
                ),
                provider.clone(),
            ),
            announcements: HoprAnnouncementsInstance::new(
                primitives::Address::from(
                    <[u8; 20]>::try_from(contract_addresses.announcements.as_ref()).expect("Address is 20 bytes"),
                ),
                provider.clone(),
            ),
            safe_registry: HoprNodeSafeRegistryInstance::new(
                primitives::Address::from(
                    <[u8; 20]>::try_from(contract_addresses.node_safe_registry.as_ref()).expect("Address is 20 bytes"),
                ),
                provider.clone(),
            ),
            price_oracle: HoprTicketPriceOracleInstance::new(
                primitives::Address::from(
                    <[u8; 20]>::try_from(contract_addresses.ticket_price_oracle.as_ref()).expect("Address is 20 bytes"),
                ),
                provider.clone(),
            ),
            win_prob_oracle: HoprWinningProbabilityOracleInstance::new(
                primitives::Address::from(
                    <[u8; 20]>::try_from(contract_addresses.winning_probability_oracle.as_ref())
                        .expect("Address is 20 bytes"),
                ),
                provider.clone(),
            ),
            stake_factory: HoprNodeStakeFactoryInstance::new(
                primitives::Address::from(
                    <[u8; 20]>::try_from(contract_addresses.node_stake_v2_factory.as_ref())
                        .expect("Address is 20 bytes"),
                ),
                provider.clone(),
            ),
        }
    }

    /// Deploys testing environment via the given provider.
    async fn inner_deploy_common_contracts_for_testing(provider: P, deployer: &ChainKeypair) -> ContractResult<Self> {
        {
            // Fund 1820 deployer and deploy ERC1820Registry
            let tx = TransactionRequest::default()
                .with_to(ERC_1820_DEPLOYER)
                .with_value(ETH_VALUE_FOR_ERC1820_DEPLOYER);

            // Sequentially executing the following transactions:
            // 1. Fund the deployer wallet
            provider.send_transaction(tx.clone()).await?.watch().await?;
            // 2. Use the fundedd deployer wallet to deploy ERC1820Registry with a signed txn
            provider
                .send_raw_transaction(&ERC_1820_REGISTRY_DEPLOY_CODE)
                .await?
                .watch()
                .await?;
        }

        // Get deployer address
        let deployer_hopr_address = deployer.public().to_address();
        let self_address = primitives::Address::from(
            <[u8; 20]>::try_from(deployer_hopr_address.as_ref()).expect("Address is 20 bytes"),
        );

        let safe_registry = HoprNodeSafeRegistry::deploy(provider.clone()).await?;
        let announcements = HoprAnnouncements::deploy(provider.clone()).await?;
        let stake_factory = HoprNodeStakeFactory::deploy(
            provider.clone(),
            primitives::Address::ZERO, // _moduleSingletonAddress - use zero for testing
            primitives::Address::from(announcements.address().as_ref()),
            self_address,
        )
        .await?;
        let price_oracle = HoprTicketPriceOracle::deploy(
            provider.clone(),
            self_address,
            primitives::U256::from(100000000000000000_u128), // U256::from(100000000000000000_u128),
        )
        .await?;
        let win_prob_oracle = HoprWinningProbabilityOracle::deploy(
            provider.clone(),
            self_address,
            primitives::aliases::U56::from(0xFFFFFFFFFFFFFF_u64), /* 0xFFFFFFFFFFFFFF in hex or 72057594037927935 in
                                                                   * decimal values */
        )
        .await?;
        let token = HoprToken::deploy(provider.clone()).await?;
        let channels = HoprChannels::deploy(
            provider.clone(),
            primitives::Address::from(token.address().as_ref()),
            1_u32,
            primitives::Address::from(safe_registry.address().as_ref()),
        )
        .await?;

        Ok(Self {
            token,
            channels,
            announcements,
            safe_registry,
            price_oracle,
            win_prob_oracle,
            stake_factory,
        })
    }

    /// Deploys testing environment via the given provider.
    pub async fn deploy_for_testing(provider: P, deployer: &ChainKeypair) -> ContractResult<Self> {
        let instances = Self::inner_deploy_common_contracts_for_testing(provider.clone(), deployer).await?;

        Ok(Self { ..instances })
    }

    /// Deploys testing environment via the given provider.
    pub async fn deploy_for_testing_with_staking_proxy(provider: P, deployer: &ChainKeypair) -> ContractResult<Self> {
        let instances = Self::inner_deploy_common_contracts_for_testing(provider.clone(), deployer).await?;

        Ok(Self { ..instances })
    }
}

impl<P> From<&ContractInstances<P>> for ContractAddresses
where
    P: alloy::providers::Provider + Clone,
{
    fn from(instances: &ContractInstances<P>) -> Self {
        Self {
            token: Address::from(<[u8; 20]>::from(*instances.token.address())),
            channels: Address::from(<[u8; 20]>::from(*instances.channels.address())),
            announcements: Address::from(<[u8; 20]>::from(*instances.announcements.address())),
            node_safe_registry: Address::from(<[u8; 20]>::from(*instances.safe_registry.address())),
            ticket_price_oracle: Address::from(<[u8; 20]>::from(*instances.price_oracle.address())),
            winning_probability_oracle: Address::from(<[u8; 20]>::from(*instances.win_prob_oracle.address())),
            node_stake_v2_factory: Address::from(<[u8; 20]>::from(*instances.stake_factory.address())),
        }
    }
}
