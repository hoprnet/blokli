//! This crate contains various on-chain related modules and types.
use constants::{ERC_1820_DEPLOYER, ERC_1820_REGISTRY_DEPLOY_CODE, ETH_VALUE_FOR_ERC1820_DEPLOYER};
use hopr_bindings::{
    exports::alloy::{
        contract::Result as ContractResult,
        network::TransactionBuilder,
        primitives::{self, Address as AlloyAddress, aliases::U56},
        providers::Provider,
        rpc::types::TransactionRequest,
    },
    hopr_announcements::HoprAnnouncements::{self, HoprAnnouncementsInstance},
    hopr_channels::HoprChannels::{self, HoprChannelsInstance},
    hopr_node_management_module::HoprNodeManagementModule::{self, HoprNodeManagementModuleInstance},
    hopr_node_safe_migration::HoprNodeSafeMigration::HoprNodeSafeMigrationInstance,
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
pub mod channel;
pub mod constants;
pub mod errors;
// Various (mostly testing related) utility functions
pub mod utils;

/// Extension trait for converting between alloy and HOPR address types.
///
/// This trait provides convenient methods for converting between `hopr_bindings::exports::alloy::primitives::Address`
/// and `hopr_primitive_types::primitives::Address` types, eliminating verbose conversion boilerplate.
///
/// # Examples
///
/// ```rust,ignore
/// use blokli_chain_types::AlloyAddressExt;
///
/// // Convert alloy Address to HOPR Address
/// let alloy_addr: hopr_bindings::exports::alloy::primitives::Address = /* ... */;
/// let hopr_addr = alloy_addr.to_hopr_address();
///
/// // Convert HOPR Address to alloy Address
/// let hopr_addr: hopr_primitive_types::primitives::Address = /* ... */;
/// let alloy_addr = hopr_bindings::exports::alloy::primitives::Address::from_hopr_address(hopr_addr);
/// ```
pub trait AlloyAddressExt {
    /// Converts an alloy Address to a HOPR Address.
    fn to_hopr_address(self) -> Address;

    /// Creates an alloy Address from a HOPR Address.
    fn from_hopr_address(addr: Address) -> Self;
}

impl AlloyAddressExt for AlloyAddress {
    fn to_hopr_address(self) -> Address {
        Address::from(<[u8; 20]>::from(self))
    }

    fn from_hopr_address(addr: Address) -> Self {
        AlloyAddress::from(<[u8; 20]>::try_from(addr.as_ref()).expect("Address is 20 bytes"))
    }
}

/// Chain configuration containing blockchain-specific parameters.
///
/// This struct encapsulates chain-level configuration needed by the indexer and RPC operations.
/// Chain ID and contract addresses are resolved from hopr-bindings network definitions.
/// Expected block time is network-specific and affects indexer polling frequency.
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
    /// Expected block time in seconds (network-specific, affects indexer polling frequency)
    pub expected_block_time: u64,
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
    /// Node management module contract
    pub module_implementation: Address,
    /// Node safe migration contract
    pub node_safe_migration: Address,
    /// Safe registry contract
    pub node_safe_registry: Address,
    /// Price oracle contract
    pub ticket_price_oracle: Address,
    /// Minimum ticket winning probability contract
    pub winning_probability_oracle: Address,
    /// Stake factory contract
    pub node_stake_factory: Address,
}

/// Holds instances to contracts.
#[derive(Debug)]
pub struct ContractInstances<P> {
    pub token: HoprTokenInstance<P>,
    pub channels: HoprChannelsInstance<P>,
    pub announcements: HoprAnnouncementsInstance<P>,
    pub module_implementation: HoprNodeManagementModuleInstance<P>,
    pub node_safe_migration: HoprNodeSafeMigrationInstance<P>,
    pub node_safe_registry: HoprNodeSafeRegistryInstance<P>,
    pub ticket_price_oracle: HoprTicketPriceOracleInstance<P>,
    pub winning_probability_oracle: HoprWinningProbabilityOracleInstance<P>,
    pub node_stake_factory: HoprNodeStakeFactoryInstance<P>,
}

impl<P> ContractInstances<P>
where
    P: Provider + Clone,
{
    pub fn new(contract_addresses: &ContractAddresses, provider: P, _use_dummy_nr: bool) -> Self {
        Self {
            token: HoprTokenInstance::new(
                AlloyAddress::from_hopr_address(contract_addresses.token),
                provider.clone(),
            ),
            channels: HoprChannelsInstance::new(
                AlloyAddress::from_hopr_address(contract_addresses.channels),
                provider.clone(),
            ),
            announcements: HoprAnnouncementsInstance::new(
                AlloyAddress::from_hopr_address(contract_addresses.announcements),
                provider.clone(),
            ),
            module_implementation: HoprNodeManagementModuleInstance::new(
                AlloyAddress::from_hopr_address(contract_addresses.module_implementation),
                provider.clone(),
            ),
            node_safe_migration: HoprNodeSafeMigrationInstance::new(
                AlloyAddress::from_hopr_address(contract_addresses.node_safe_migration),
                provider.clone(),
            ),
            node_safe_registry: HoprNodeSafeRegistryInstance::new(
                AlloyAddress::from_hopr_address(contract_addresses.node_safe_registry),
                provider.clone(),
            ),
            ticket_price_oracle: HoprTicketPriceOracleInstance::new(
                AlloyAddress::from_hopr_address(contract_addresses.ticket_price_oracle),
                provider.clone(),
            ),
            winning_probability_oracle: HoprWinningProbabilityOracleInstance::new(
                AlloyAddress::from_hopr_address(contract_addresses.winning_probability_oracle),
                provider.clone(),
            ),
            node_stake_factory: HoprNodeStakeFactoryInstance::new(
                AlloyAddress::from_hopr_address(contract_addresses.node_stake_factory),
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
        let self_address = AlloyAddress::from_hopr_address(deployer_hopr_address);

        let safe_registry = HoprNodeSafeRegistry::deploy(provider.clone()).await?;
        let announcements = HoprAnnouncements::deploy(provider.clone()).await?;
        let stake_factory = HoprNodeStakeFactory::deploy(
            provider.clone(),
            AlloyAddress::ZERO, // _moduleSingletonAddress - use zero for testing
            AlloyAddress::from(announcements.address().as_ref()),
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
            U56::from(0xFFFFFFFFFFFFFF_u64), /* 0xFFFFFFFFFFFFFF in hex or 72057594037927935 in
                                              * decimal values */
        )
        .await?;
        let token = HoprToken::deploy(provider.clone()).await?;
        let channels = HoprChannels::deploy(
            provider.clone(),
            AlloyAddress::from(token.address().as_ref()),
            1_u32,
            AlloyAddress::from(safe_registry.address().as_ref()),
        )
        .await?;
        let module_implementation = HoprNodeManagementModule::deploy(provider.clone()).await?;
        // Note: HoprNodeSafeMigration requires Safe Singleton to be deployed, which has complex dependencies.
        // For testing purposes, we create a minimal instance with zero address that won't be used in actual tests.
        // In production, these addresses are loaded from hopr-bindings network configuration.
        let node_safe_migration = HoprNodeSafeMigrationInstance::new(AlloyAddress::ZERO, provider.clone());

        Ok(Self {
            token,
            channels,
            announcements,
            module_implementation,
            node_safe_migration,
            node_safe_registry: safe_registry,
            ticket_price_oracle: price_oracle,
            winning_probability_oracle: win_prob_oracle,
            node_stake_factory: stake_factory,
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
    P: Provider + Clone,
{
    fn from(instances: &ContractInstances<P>) -> Self {
        Self {
            token: instances.token.address().to_hopr_address(),
            channels: instances.channels.address().to_hopr_address(),
            announcements: instances.announcements.address().to_hopr_address(),
            module_implementation: instances.module_implementation.address().to_hopr_address(),
            node_safe_migration: instances.node_safe_migration.address().to_hopr_address(),
            node_safe_registry: instances.node_safe_registry.address().to_hopr_address(),
            ticket_price_oracle: instances.ticket_price_oracle.address().to_hopr_address(),
            winning_probability_oracle: instances.winning_probability_oracle.address().to_hopr_address(),
            node_stake_factory: instances.node_stake_factory.address().to_hopr_address(),
        }
    }
}
