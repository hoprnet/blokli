use std::{
    any::Any, backtrace::Backtrace, error::Error, fs, io::stdout, panic, path::PathBuf, str::FromStr, sync::Once,
};

use blokli_chain_types::ContractAddresses as BlokliContractAddresses;
use clap::Parser;
use hopli_lib::utils::{a2h, h2a};
use hopr_bindings::{
    config::ContractInstances,
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
    primitive::{prelude::HoprBalance, primitives::Address, traits::IntoEndian},
};
use serde::Serialize;
use tracing_subscriber::{Layer as _, prelude::*};
use url::Url;

const DEFAULT_ANVIL_PRIVATE_KEY: &str = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";
static PANIC_HOOK_INSTALLED: Once = Once::new();

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

    /// Minimum ticket price to set on the deployed HoprTicketPriceOracle.
    /// Defaults to the live rotsee value so the local cluster behaves like rotsee.
    /// Read once via: cast call 0xca2c60433eC6a10dDEabBbE3Ce7f9737b1a0628C
    ///   "currentTicketPrice()(uint256)" --rpc-url https://rpc.gnosischain.com
    #[arg(long, env = "BLOKLI_DEPLOYER_TICKET_PRICE", default_value = "100 wei wxHOPR")]
    ticket_price: HoprBalance,

    /// Minimum winning probability to set on the deployed HoprWinningProbabilityOracle.
    /// Defaults to the live rotsee value so the local cluster behaves like rotsee.
    /// Read once via: cast call 0x5136Bac09C78af89bDA56F5086A3F3E2Ee4EAfCa
    ///   "currentWinProb()(uint56)" --rpc-url https://rpc.gnosischain.com
    /// Use 1.0 to restore the legacy "always wins" behaviour.
    #[arg(long, env = "BLOKLI_DEPLOYER_WINNING_PROBABILITY", default_value = "0.000125")]
    winning_probability: WinningProbability,

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
    install_tracing()?;

    let args = Args::parse();

    let signer = PrivateKeySigner::from_str(&args.private_key)?;
    let signer_chain_key = ChainKeypair::from_secret(signer.to_bytes().as_ref())?;
    let signer_address = signer.address();

    let rpc_url = Url::parse(&args.rpc_url)?;
    let rpc_client = ClientBuilder::default().http(rpc_url);
    let provider = ProviderBuilder::new().wallet(signer).connect_client(rpc_client);

    let instances =
        ContractInstances::deploy_for_testing(provider, a2h(signer_chain_key.public().to_address())).await?;
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
            xhopr_token: Address::default(), // xHOPR is not deployed by this script, so we set it to zero address
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
    tracing::info!(%signer_address, "granted minter role to Anvil account");

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
    tracing::info!(%signer_address, "minted tokens to Anvil account");

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

    instances
        .price_oracle
        .setTicketPrice(U256::from_be_bytes(args.ticket_price.amount().to_be_bytes()))
        .send()
        .await?
        .watch()
        .await?;
    tracing::info!("ticket price oracle set to {}", args.ticket_price);

    let win_prob_u56 = U56::from_be_slice(&args.winning_probability.as_encoded());
    instances
        .win_prob_oracle
        .setWinProb(win_prob_u56)
        .send()
        .await?
        .watch()
        .await?;
    tracing::info!(
        "winning probability oracle set to {} (U56 {win_prob_u56})",
        args.winning_probability.as_f64()
    );

    if let Some(path) = args.output {
        fs::write(path, toml_output)?;
    } else {
        print!("{toml_output}");
    }

    Ok(())
}

fn install_tracing() -> Result<(), Box<dyn Error>> {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    let format = tracing_subscriber::fmt::layer()
        .with_writer(stdout)
        .with_target(true)
        .with_level(true)
        .with_thread_ids(true)
        .with_thread_names(false);
    let subscriber = tracing_subscriber::registry().with(env_filter).with(
        if std::env::var("BLOKLI_LOG_FORMAT")
            .map(|value| value.eq_ignore_ascii_case("json"))
            .unwrap_or(false)
        {
            format.json().boxed()
        } else {
            format.boxed()
        },
    );

    tracing::subscriber::set_global_default(subscriber)?;
    install_panic_hook();
    Ok(())
}

fn install_panic_hook() {
    PANIC_HOOK_INSTALLED.call_once(|| {
        panic::set_hook(Box::new(|info| {
            let payload = panic_payload_to_string(info.payload());
            let location = info.location();
            let panic_file = location.map(|value| value.file()).unwrap_or("unknown");
            let panic_line = location.map(|value| value.line()).unwrap_or(0);
            let panic_column = location.map(|value| value.column()).unwrap_or(0);
            let thread = std::thread::current();
            let thread_name = thread.name().unwrap_or("unnamed");
            let backtrace = Backtrace::force_capture().to_string();

            tracing::error!(
                panic_payload = %payload,
                panic_file,
                panic_line,
                panic_column,
                thread_name,
                thread_id = ?thread.id(),
                backtrace = %backtrace,
                "process panic"
            );
        }));
    });
}

fn panic_payload_to_string(payload: &(dyn Any + Send)) -> String {
    if let Some(payload) = payload.downcast_ref::<&str>() {
        (*payload).to_string()
    } else if let Some(payload) = payload.downcast_ref::<String>() {
        payload.clone()
    } else {
        "non-string panic payload".to_string()
    }
}

#[cfg(test)]
mod tests {
    use std::any::Any;

    use anyhow::Result;
    use clap::Parser;
    use hopr_types::{internal::prelude::WinningProbability, primitive::traits::IntoEndian};

    use super::{Args, panic_payload_to_string};

    #[test]
    fn test_panic_payload_to_string_from_str() {
        let payload: &(dyn Any + Send) = &"boom";
        assert_eq!(panic_payload_to_string(payload), "boom");
    }

    #[test]
    fn test_panic_payload_to_string_from_string() {
        let payload: &(dyn Any + Send) = &"boom".to_string();
        assert_eq!(panic_payload_to_string(payload), "boom");
    }

    #[test]
    fn ticket_price_arg_default_parses() -> Result<()> {
        let args = Args::try_parse_from(["blokli-contract-deployer"])?;
        // 100 wei: amount() returns the raw U256 value
        assert_eq!(args.ticket_price.amount().to_be_bytes(), {
            let mut b = [0u8; 32];
            b[31] = 100;
            b
        });
        Ok(())
    }

    #[test]
    fn winning_probability_arg_default_parses() -> Result<()> {
        let args = Args::try_parse_from(["blokli-contract-deployer"])?;
        let roundtrip = args.winning_probability.as_f64();
        assert!((roundtrip - 0.000125_f64).abs() < WinningProbability::EPSILON);
        Ok(())
    }
}
