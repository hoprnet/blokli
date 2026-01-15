use std::{error::Error, fs, path::PathBuf, str::FromStr};

use blokli_chain_types::{ContractAddresses, ContractInstances};
use clap::Parser;
use hopr_bindings::exports::alloy::{
    providers::ProviderBuilder, rpc::client::ClientBuilder, signers::local::PrivateKeySigner,
};
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
    contracts: ContractAddresses,
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
    let output = ContractsOutput { contracts };
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
    println!("Minter role granted to Anvil account {signer_address}");

    // Mint 1M tokens to Anvil account 0
    instances
        .token
        .mint(
            signer_address,
            "1000000000000000000000000".parse()?,
            Default::default(),
            Default::default(),
        )
        .send()
        .await?
        .watch()
        .await?;
    println!("10M tokens minted to Anvil account {signer_address}");

    if let Some(path) = args.output {
        fs::write(path, toml_output)?;
    } else {
        print!("{toml_output}");
    }

    Ok(())
}
