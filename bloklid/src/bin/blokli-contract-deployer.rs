use std::{error::Error, fs, path::PathBuf, str::FromStr};

use blokli_chain_types::ContractAddresses as BlokliContractAddresses;
use clap::Parser;
use hopli_lib::utils::{ContractInstances, h2a};
use hopr_bindings::{
    exports::alloy::{providers::ProviderBuilder, rpc::client::ClientBuilder, signers::local::PrivateKeySigner},
    hopr_node_stake_factory::HoprNodeStakeFactory::HoprNetwork,
};
use hopr_chain_types::ContractAddresses;
use hopr_crypto_types::keypairs::{ChainKeypair, Keypair};
use serde::Serialize;
use url::Url;

const DEFAULT_ANVIL_PRIVATE_KEY: &str = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";

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
    eprintln!("Minter role granted to Anvil account {signer_address}");

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
    eprintln!("10M tokens minted to Anvil account {signer_address}");

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
    eprintln!("updated stake factory contract");

    if let Some(path) = args.output {
        fs::write(path, toml_output)?;
    } else {
        print!("{toml_output}");
    }

    Ok(())
}
