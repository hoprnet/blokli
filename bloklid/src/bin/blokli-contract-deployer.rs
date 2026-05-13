use std::{error::Error, fs, path::PathBuf, str::FromStr};

use blokli_chain_types::ContractAddresses as BlokliContractAddresses;
use clap::Parser;
use hopli_lib::utils::{ContractInstances, h2a};
use hopr_bindings::{
    exports::alloy::{
        primitives::{U256, aliases::U56},
        providers::ProviderBuilder,
        rpc::client::ClientBuilder,
        signers::local::PrivateKeySigner,
    },
    hopr_node_stake_factory::HoprNodeStakeFactory::HoprNetwork,
};
use hopr_types::{
    chain::ContractAddresses,
    crypto::keypairs::{ChainKeypair, Keypair},
    internal::prelude::WinningProbability,
};
use serde::Serialize;
use url::Url;

const DEFAULT_ANVIL_PRIVATE_KEY: &str = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";

/// Live rotsee ticket price as of 2025-05 (in wei, 1e-16 wxHOPR). Read once via:
/// `cast call 0xca2c60433eC6a10dDEabBbE3Ce7f9737b1a0628C "currentTicketPrice()(uint256)" \
///   --rpc-url https://rpc.gnosischain.com`
const DEFAULT_TICKET_PRICE_WEI: &str = "100";

/// Live rotsee minimum winning probability as of 2025-05 (~1/8000). Read once via:
/// `cast call 0x5136Bac09C78af89bDA56F5086A3F3E2Ee4EAfCa "currentWinProb()(uint56)" \
///   --rpc-url https://rpc.gnosischain.com`
/// Override with `1.0` to restore the legacy "always wins" behaviour.
const DEFAULT_WINNING_PROBABILITY: f64 = 0.000125;

#[derive(Debug, Parser)]
#[command(
    name = "blokli-contract-deployer",
    about = "Deploy HOPR contracts and emit config overrides"
)]
struct Args {
    /// RPC endpoint URL for the chain
    #[arg(long, env = "BLOKLI_DEPLOYER_RPC_URL", default_value = "http://127.0.0.1:8545")]
    rpc_url: String,

    /// Private key used to deploy contracts
    #[arg(long, env = "ANVIL_DEPLOYER_PRIVATE_KEY", default_value = DEFAULT_ANVIL_PRIVATE_KEY)]
    private_key: String,

    /// Minimum ticket price to set on the deployed HoprTicketPriceOracle (raw wei).
    /// Defaults to the live rotsee value so the local cluster behaves like rotsee.
    #[arg(long, env = "BLOKLI_DEPLOYER_TICKET_PRICE_WEI", default_value = DEFAULT_TICKET_PRICE_WEI)]
    ticket_price_wei: String,

    /// Minimum winning probability to set on the deployed HoprWinningProbabilityOracle (float in [0.0, 1.0]).
    /// Defaults to the live rotsee value so the local cluster behaves like rotsee.
    #[arg(long, env = "BLOKLI_DEPLOYER_WINNING_PROBABILITY", default_value_t = DEFAULT_WINNING_PROBABILITY)]
    winning_probability: f64,

    /// Optional output path for TOML configuration
    #[arg(long)]
    output: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
struct ContractsOutput {
    contracts: BlokliContractAddresses,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stdout)
        .with_target(true)
        .with_level(true)
        .init();

    let args = Args::parse();

    let signer = PrivateKeySigner::from_str(&args.private_key)?;
    let signer_chain_key = ChainKeypair::from_secret(signer.to_bytes().as_ref())?;
    let signer_address = signer.address();

    let rpc_url = Url::parse(&args.rpc_url)?;
    let rpc_client = ClientBuilder::default().http(rpc_url);
    let provider = ProviderBuilder::new().wallet(signer).connect_client(rpc_client);

    let instances = ContractInstances::deploy_for_testing(provider, &signer_chain_key).await?;
    let contracts = ContractAddresses::from(&instances);
    let output = ContractsOutput {
        contracts: BlokliContractAddresses {
            token: h2a(contracts.token),
            channels: h2a(contracts.channels),
            announcements: h2a(contracts.announcements),
            module_implementation: h2a(contracts.module_implementation),
            node_safe_migration: h2a(contracts.node_safe_migration),
            node_safe_registry: h2a(contracts.node_safe_registry),
            ticket_price_oracle: h2a(contracts.ticket_price_oracle),
            winning_probability_oracle: h2a(contracts.winning_probability_oracle),
            node_stake_factory: h2a(contracts.node_stake_factory),
        },
    };
    let toml_output = toml::to_string(&output)?;

    // Assign minter role to Anvil account 0
    let minter_role = instances.token.MINTER_ROLE().call().await?;
    instances
        .token
        .grantRole(minter_role, signer_address)
        .send()
        .await?
        .watch()
        .await?;
    tracing::info!("Minter role granted to Anvil account {signer_address}");

    // Mint 10M tokens to Anvil account 0
    instances
        .token
        .mint(
            signer_address,
            "10000000000000000000000000".parse()?,
            Default::default(),
            Default::default(),
        )
        .send()
        .await?
        .watch()
        .await?;
    tracing::info!("10M tokens minted to Anvil account {signer_address}");

    // Update the stake factory to use correct addresses
    let network = instances.stake_factory.defaultHoprNetwork().call().await?;
    instances
        .stake_factory
        .updateHoprNetwork(HoprNetwork {
            tokenAddress: *instances.token.address(),
            defaultTokenAllowance: network.defaultTokenAllowance,
            defaultAnnouncementTarget: network.defaultAnnouncementTarget,
        })
        .send()
        .await?
        .watch()
        .await?;
    tracing::info!("updated stake factory contract");

    let ticket_price = U256::from_str_radix(args.ticket_price_wei.trim(), 10)
        .map_err(|e| format!("invalid --ticket-price-wei {:?}: {e}", args.ticket_price_wei))?;
    instances
        .price_oracle
        .setTicketPrice(ticket_price)
        .send()
        .await?
        .watch()
        .await?;
    tracing::info!("ticket price oracle set to {ticket_price} wei");

    let win_prob = WinningProbability::try_from(args.winning_probability)
        .map_err(|e| format!("invalid --winning-probability {}: {e}", args.winning_probability))?;
    let win_prob_u56 = U56::from_be_slice(&win_prob.as_encoded());
    instances
        .win_prob_oracle
        .setWinProb(win_prob_u56)
        .send()
        .await?
        .watch()
        .await?;
    tracing::info!(
        "winning probability oracle set to {} (U56 {win_prob_u56})",
        args.winning_probability
    );

    if let Some(path) = args.output {
        fs::write(path, toml_output)?;
    } else {
        print!("{toml_output}");
    }

    Ok(())
}
