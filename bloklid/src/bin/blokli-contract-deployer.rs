use std::{
    any::Any,
    backtrace::Backtrace,
    borrow::Cow,
    error::Error,
    io::{self, Write, stdout},
    panic,
    path::{Component, Path, PathBuf},
    str::FromStr,
    sync::Once,
};

use blokli_chain_types::ContractAddresses as BlokliContractAddresses;
use clap::Parser;
#[cfg(feature = "curvy-test-deployment")]
use curvy_bindings::{CurvyContractAddresses, config::CurvyContractInstances};
use hopli_lib::utils::{a2h, h2a};
use hopr_bindings::{
    config::ContractInstances,
    exports::alloy::{
        primitives::{U256, aliases::U56},
        providers::{Provider, ProviderBuilder},
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
use tempfile::NamedTempFile;
use tracing_subscriber::{Layer as _, prelude::*};
use url::Url;

const DEFAULT_ANVIL_PRIVATE_KEY: &str = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";
const ANVIL_CHAIN_ID: u64 = 31_337;
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

    /// Also deploy the Curvy v2 local-development contract suite
    #[arg(long, default_value_t = false)]
    with_curvy: bool,

    /// Curvy Ignition-compatible address JSON output path
    #[arg(long)]
    curvy_json_out: Option<PathBuf>,

    /// Allow Curvy deployment outside Anvil's default chain ID
    #[arg(long, env = "BLOKLI_DEPLOYER_ALLOW_UNSAFE_CHAIN", default_value_t = false)]
    allow_unsafe_chain: bool,
}

#[derive(Debug, Serialize)]
struct ContractsOutput {
    contracts: BlokliContractAddresses,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    install_tracing()?;

    let args = Args::parse();
    validate_args(&args)?;

    let signer = PrivateKeySigner::from_str(&args.private_key)?;
    let signer_chain_key = ChainKeypair::from_secret(signer.to_bytes().as_ref())?;
    let signer_address = signer.address();

    let rpc_url = Url::parse(&args.rpc_url)?;
    let rpc_client = ClientBuilder::default().http(rpc_url);
    let provider = ProviderBuilder::new().wallet(signer).connect_client(rpc_client);
    let chain_id = provider.get_chain_id().await?;
    if args.with_curvy && chain_id != ANVIL_CHAIN_ID && !args.allow_unsafe_chain {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "refusing local-development deployment on chain {chain_id}; expected {ANVIL_CHAIN_ID} (use --allow-unsafe-chain to override)"
            ),
        )
        .into());
    }

    let instances =
        ContractInstances::deploy_for_testing(provider.clone(), a2h(signer_chain_key.public().to_address())).await?;
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

    let curvy_json: Option<String> = if args.with_curvy {
        #[cfg(feature = "curvy-test-deployment")]
        {
            tracing::info!("deploying Curvy v2 local-development contracts");
            let curvy_instances = CurvyContractInstances::deploy_for_testing(provider, signer_address).await?;
            let curvy_contracts = CurvyContractAddresses::from(&curvy_instances);
            tracing::info!(
                aggregator = %curvy_contracts.aggregator_proxy,
                vault = %curvy_contracts.vault_proxy,
                portal_factory = %curvy_contracts.portal_factory,
                "Curvy contracts ready"
            );
            Some(format!(
                "{}\n",
                serde_json::to_string_pretty(&curvy_contracts.to_ignition_json())?
            ))
        }
        #[cfg(not(feature = "curvy-test-deployment"))]
        unreachable!("validate_args rejects Curvy without compiled support")
    } else {
        None
    };

    // Publish outputs only after every requested deployment and serialization succeeds.
    if let Some(path) = args.output.as_deref() {
        atomic_write(path, toml_output.as_bytes())?;
        tracing::info!(path = %path.display(), "wrote HOPR contract configuration");
    } else {
        print!("{toml_output}");
    }
    if let (Some(path), Some(json)) = (args.curvy_json_out.as_deref(), curvy_json.as_deref()) {
        atomic_write(path, json.as_bytes())?;
        tracing::info!(path = %path.display(), "wrote Curvy contract addresses");
    }

    Ok(())
}

fn validate_args(args: &Args) -> io::Result<()> {
    if args.curvy_json_out.is_some() && !args.with_curvy {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "--curvy-json-out requires --with-curvy",
        ));
    }
    if let (Some(hopr), Some(curvy)) = (args.output.as_deref(), args.curvy_json_out.as_deref())
        && normalize_path(hopr)? == normalize_path(curvy)?
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "HOPR and Curvy output paths must be different",
        ));
    }
    if args.with_curvy && !cfg!(feature = "curvy-test-deployment") {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Curvy deployment requires a binary built with the curvy-test-deployment feature",
        ));
    }
    Ok(())
}

fn normalize_path(path: &Path) -> io::Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    Ok(normalized)
}

fn atomic_write(path: &Path, contents: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temporary = NamedTempFile::new_in(parent)?;
    temporary.write_all(contents)?;
    #[cfg(unix)]
    temporary
        .as_file()
        .set_permissions(std::os::unix::fs::PermissionsExt::from_mode(0o644))?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
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
    set_panic_hook();
    Ok(())
}

fn set_panic_hook() {
    PANIC_HOOK_INSTALLED.call_once(|| {
        panic::set_hook(Box::new(|info| {
            let payload = panic_payload_to_str(info.payload());
            let location = info.location();
            let panic_file = location.map(|value| value.file()).unwrap_or("unknown");
            let panic_line = location.map(|value| value.line()).unwrap_or(0);
            let panic_column = location.map(|value| value.column()).unwrap_or(0);
            let thread = std::thread::current();
            let thread_name = thread.name().unwrap_or("unnamed");
            let backtrace = Backtrace::capture().to_string();

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

fn panic_payload_to_str(payload: &(dyn Any + Send)) -> Cow<'static, str> {
    if let Some(payload) = payload.downcast_ref::<&'static str>() {
        Cow::Borrowed(payload)
    } else if let Some(payload) = payload.downcast_ref::<String>() {
        Cow::Owned(payload.clone())
    } else {
        Cow::Borrowed("non-string panic payload")
    }
}

#[cfg(test)]
mod tests {
    use std::{any::Any, fs};

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt as _;

    use anyhow::Result;
    use clap::Parser;
    use hopr_types::{internal::prelude::WinningProbability, primitive::traits::IntoEndian};
    use tempfile::tempdir;

    use super::{Args, atomic_write, panic_payload_to_str, validate_args};

    #[test]
    fn test_panic_payload_to_str_from_str() {
        let payload: &(dyn Any + Send) = &"boom";
        assert_eq!(panic_payload_to_str(payload), "boom");
    }

    #[test]
    fn test_panic_payload_to_str_from_string() {
        let payload: &(dyn Any + Send) = &"boom".to_string();
        assert_eq!(panic_payload_to_str(payload), "boom");
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

    #[test]
    fn curvy_defaults_off() -> Result<()> {
        let args = Args::try_parse_from(["blokli-contract-deployer"])?;
        assert!(!args.with_curvy);
        assert!(args.curvy_json_out.is_none());
        Ok(())
    }

    #[test]
    fn curvy_output_requires_explicit_activation() -> Result<()> {
        let args = Args::try_parse_from(["blokli-contract-deployer", "--curvy-json-out", "curvy.json"])?;
        let error = validate_args(&args).expect_err("output alone must not enable Curvy deployment");
        assert_eq!(error.to_string(), "--curvy-json-out requires --with-curvy");
        Ok(())
    }

    #[test]
    fn output_paths_must_not_collide() -> Result<()> {
        let args = Args::try_parse_from([
            "blokli-contract-deployer",
            "--with-curvy",
            "--output",
            "deployments.toml",
            "--curvy-json-out",
            "./deployments.toml",
        ])?;
        let error = validate_args(&args).expect_err("colliding paths must fail before deployment");
        assert_eq!(error.to_string(), "HOPR and Curvy output paths must be different");
        Ok(())
    }

    #[cfg(not(feature = "curvy-test-deployment"))]
    #[test]
    fn feature_off_binary_rejects_curvy() -> Result<()> {
        let args = Args::try_parse_from(["blokli-contract-deployer", "--with-curvy"])?;
        let error = validate_args(&args).expect_err("feature-off binary must reject Curvy deployment");
        assert!(error.to_string().contains("curvy-test-deployment"));
        Ok(())
    }

    #[cfg(feature = "curvy-test-deployment")]
    #[test]
    fn feature_on_binary_accepts_curvy() -> Result<()> {
        let args = Args::try_parse_from(["blokli-contract-deployer", "--with-curvy"])?;
        validate_args(&args)?;
        Ok(())
    }

    #[test]
    fn atomic_write_replaces_complete_file() -> Result<()> {
        let directory = tempdir()?;
        let path = directory.path().join("contracts.toml");
        fs::write(&path, "old")?;

        atomic_write(&path, b"new\n")?;

        assert_eq!(fs::read_to_string(&path)?, "new\n");
        #[cfg(unix)]
        assert_eq!(fs::metadata(path)?.permissions().mode() & 0o777, 0o644);
        Ok(())
    }
}
