//! This crate contains various on-chain related modules and types.
use alloy::{
    contract::Result as ContractResult, network::TransactionBuilder, primitives, rpc::types::TransactionRequest,
};
use constants::{ERC_1820_DEPLOYER, ERC_1820_REGISTRY_DEPLOY_CODE, ETH_VALUE_FOR_ERC1820_DEPLOYER};
use hopr_bindings::{
    hoprannouncements::HoprAnnouncements::{self, HoprAnnouncementsInstance},
    hoprchannels::HoprChannels::{self, HoprChannelsInstance},
    hoprnodesaferegistry::HoprNodeSafeRegistry::{self, HoprNodeSafeRegistryInstance},
    hoprnodestakefactory::HoprNodeStakeFactory::{self, HoprNodeStakeFactoryInstance},
    hoprticketpriceoracle::HoprTicketPriceOracle::{self, HoprTicketPriceOracleInstance},
    hoprtoken::HoprToken::{self, HoprTokenInstance},
    hoprwinningprobabilityoracle::HoprWinningProbabilityOracle::{self, HoprWinningProbabilityOracleInstance},
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
    pub safe_registry: Address,
    /// Price oracle contract
    pub price_oracle: Address,
    /// Minimum ticket winning probability contract
    pub win_prob_oracle: Address,
    /// Stake factory contract
    pub stake_factory: Address,
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
            token: HoprTokenInstance::new(contract_addresses.token.into(), provider.clone()),
            channels: HoprChannelsInstance::new(contract_addresses.channels.into(), provider.clone()),
            announcements: HoprAnnouncementsInstance::new(contract_addresses.announcements.into(), provider.clone()),
            safe_registry: HoprNodeSafeRegistryInstance::new(contract_addresses.safe_registry.into(), provider.clone()),
            price_oracle: HoprTicketPriceOracleInstance::new(contract_addresses.price_oracle.into(), provider.clone()),
            win_prob_oracle: HoprWinningProbabilityOracleInstance::new(
                contract_addresses.win_prob_oracle.into(),
                provider.clone(),
            ),
            stake_factory: HoprNodeStakeFactoryInstance::new(contract_addresses.stake_factory.into(), provider.clone()),
        }
    }

    /// Deploys testing environment (with dummy network registry proxy) via the given provider.
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
        let self_address = deployer.public().to_address().into();

        let stake_factory = HoprNodeStakeFactory::deploy(provider.clone()).await?;
        let safe_registry = HoprNodeSafeRegistry::deploy(provider.clone()).await?;
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
        let announcements = HoprAnnouncements::deploy(
            provider.clone(),
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

    /// Deploys testing environment (with dummy network registry proxy) via the given provider.
    pub async fn deploy_for_testing(provider: P, deployer: &ChainKeypair) -> ContractResult<Self> {
        let instances = Self::inner_deploy_common_contracts_for_testing(provider.clone(), deployer).await?;

        Ok(Self { ..instances })
    }

    /// Deploys testing environment (with dummy network registry proxy) via the given provider.
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
            token: Into::<Address>::into(*instances.token.address()),
            channels: Into::<Address>::into(*instances.channels.address()),
            announcements: Into::<Address>::into(*instances.announcements.address()),
            safe_registry: Into::<Address>::into(*instances.safe_registry.address()),
            price_oracle: Into::<Address>::into(*instances.price_oracle.address()),
            win_prob_oracle: Into::<Address>::into(*instances.win_prob_oracle.address()),
            stake_factory: Into::<Address>::into(*instances.stake_factory.address()),
        }
    }
}
