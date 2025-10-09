mod config;
mod constants;
mod errors;

use std::{
    path::PathBuf,
    sync::{Arc, RwLock},
    time::Duration,
};

use async_signal::{Signal, Signals};
use blokli_chain_api::{BlokliChain, SignificantChainEvent};
use blokli_chain_types::ContractAddresses;
use blokli_db_sql::db::{BlokliDb, BlokliDbConfig};
use clap::{Parser, Subcommand};
use futures::TryStreamExt;
use hopr_chain_config::ChainNetworkConfig;
use sea_orm::Database;
use tracing::{error, info, warn};
use tracing_subscriber::{EnvFilter, fmt};
use validator::Validate;

use crate::{
    config::Config,
    errors::{BloklidError, ConfigError},
};

/// Bloklid: Daemon for indexing HOPR on-chain events and executing HOPR-related on-chain transactions
#[derive(Debug, Parser)]
#[command(
    name = "bloklid",
    about = "Daemon for indexing HOPR on-chain events and executing HOPR-related on-chain transactions",
    version = crate::constants::APP_VERSION_COERCED
)]
struct Args {
    /// Increase output verbosity (-v, -vv)
    #[arg(short = 'v', long = "verbose", action = clap::ArgAction::Count, global = true)]
    verbose: u8,

    /// Optional path to a configuration file
    #[arg(short = 'c', long = "config", value_name = "FILE", global = true)]
    config: Option<PathBuf>,

    /// Command to execute
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Generate a template configuration file
    GenerateConfig {
        /// Path where the configuration file should be created
        #[arg(value_name = "FILE")]
        output: PathBuf,
    },
}

/// Generates a template configuration file using the embedded example config
fn generate_config_template() -> String {
    // Embed the example config file at compile time
    include_str!("../example-config.toml").to_string()
}

impl Args {
    /// Loads the configuration file specified by `config` command line argument.
    ///
    /// If no configuration file is specified,
    /// the default configuration is loaded if `use_default` is set.
    pub fn load_config(&self, use_default: bool) -> errors::Result<Config> {
        if use_default && self.config.is_none() {
            warn!("no configuration file specified; using default configuration");
            return Ok(Config::default());
        }

        let config_content = std::fs::read_to_string(self.config.as_ref().ok_or(ConfigError::NoConfiguration)?)?;
        let mut config: Config = toml::from_str(&config_content).map_err(|e| ConfigError::Parse(e.to_string()))?;

        config.validate().map_err(ConfigError::Validation)?;

        // coerce with protocol config
        let chain_network_config = ChainNetworkConfig::new_no_version_check(
            &config.network,
            Some(&config.rpc_url),
            Some(config.max_rpc_requests_per_sec),
            &mut config.protocols,
        )
        .map_err(|e| {
            let available_networks: Vec<String> = config.protocols.networks.keys().cloned().collect();

            if e.contains("Could not find network") {
                BloklidError::NonSpecific(format!(
                    "Failed to resolve blockchain environment: {e}\n\nSupported networks: {}",
                    available_networks.join(", ")
                ))
            } else if e.contains("unsupported network error") {
                BloklidError::NonSpecific(format!(
                    "Failed to resolve blockchain environment: {e}\n\nThis network exists but is not compatible with \
                     version {} of bloklid.\nSupported networks: {}",
                    crate::constants::APP_VERSION_COERCED,
                    available_networks.join(", ")
                ))
            } else {
                BloklidError::NonSpecific(format!("Failed to resolve blockchain environment: {e}"))
            }
        })?;

        config.chain_network = Some(chain_network_config.clone());

        let contract_addresses = ContractAddresses {
            token: chain_network_config.token,
            channels: chain_network_config.channels,
            announcements: chain_network_config.announcements,
            safe_registry: chain_network_config.node_safe_registry,
            price_oracle: chain_network_config.ticket_price_oracle,
            win_prob_oracle: chain_network_config.winning_probability_oracle,
            stake_factory: chain_network_config.node_stake_v2_factory,
        };
        config.contracts = contract_addresses;

        info!(
            contract_addresses = ?contract_addresses,
            "Resolved contract addresses",
        );

        config.validate().map_err(ConfigError::Validation)?;
        Ok(config)
    }
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("Error: {}", error);
        std::process::exit(1);
    }
}

async fn run() -> errors::Result<()> {
    let args = Args::parse();

    // Initialize tracing subscriber. Precedence: RUST_LOG env > -v flag > default info
    let env_filter = if std::env::var(EnvFilter::DEFAULT_ENV).is_ok() {
        EnvFilter::from_default_env()
    } else {
        match args.verbose {
            0 => EnvFilter::new("info"),
            1 => EnvFilter::new("debug"),
            _ => EnvFilter::new("trace"),
        }
    };

    fmt()
        .with_env_filter(env_filter)
        .with_target(true)
        .with_thread_ids(true)
        .with_thread_names(true)
        .compact()
        .init();

    // Handle subcommands
    if let Some(command) = args.command {
        match command {
            Command::GenerateConfig { output } => {
                info!("Generating configuration template at: {}", output.display());

                // Generate the template content
                let template = generate_config_template();

                // Create parent directories if they don't exist
                if let Some(parent) = output.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| {
                        BloklidError::NonSpecific(format!("Failed to create parent directories: {}", e))
                    })?;
                }

                // Write the template to the specified file
                std::fs::write(&output, template)
                    .map_err(|e| BloklidError::NonSpecific(format!("Failed to write configuration template: {}", e)))?;

                info!("Configuration template successfully written to: {}", output.display());
                return Ok(());
            }
        }
    }

    // Normal daemon operation
    info!(
        verbosity = args.verbose,
        config = args.config.as_ref().map(|p| p.display().to_string()).as_deref(),
        "bloklid starting"
    );

    // Initial config load
    let config = Arc::new(RwLock::new(args.load_config(true)?));

    // Initialize components
    let (process_handles, api_handle) = {
        let (database_path, logs_database_path, chain_network, contracts, indexer_config, rpc_url, api_config) = {
            let cfg = config
                .read()
                .map_err(|_| BloklidError::NonSpecific("failed to lock config".into()))?;

            let chain_network = cfg
                .chain_network
                .as_ref()
                .ok_or_else(|| BloklidError::NonSpecific("Chain network not configured".into()))?
                .clone();

            let indexer_config = blokli_chain_indexer::IndexerConfig {
                start_block_number: chain_network.channel_contract_deploy_block as u64,
                fast_sync: cfg.indexer.fast_sync,
                enable_logs_snapshot: cfg.indexer.enable_logs_snapshot,
                logs_snapshot_url: cfg.indexer.logs_snapshot_url.clone(),
                data_directory: cfg.data_directory.clone(),
            };

            (
                cfg.database.to_url(),
                cfg.database.to_logs_url(),
                chain_network,
                cfg.contracts,
                indexer_config,
                cfg.rpc_url.clone(),
                cfg.api.clone(),
            )
        };

        if let Some(logs_path) = &logs_database_path {
            info!("Initializing dual-database setup:");
            info!("  Index database: {}", database_path);
            info!("  Logs database: {}", logs_path);
        } else {
            info!("Initializing single database: {}", database_path);
        }

        // Initialize database
        let db_config = BlokliDbConfig {
            max_connections: {
                let cfg = config
                    .read()
                    .map_err(|_| BloklidError::NonSpecific("failed to lock config".into()))?;
                cfg.database.max_connections()
            },
            log_slow_queries: Duration::from_secs(1),
        };
        let db = BlokliDb::new(&database_path, logs_database_path.as_deref(), db_config).await?;

        info!("Connecting to RPC endpoint: {}", rpc_url);

        // Create channel for chain events
        let (tx_events, _rx_events) = async_channel::unbounded::<SignificantChainEvent>();

        // Create BlokliChain instance
        let blokli_chain = BlokliChain::new(db, chain_network, contracts, indexer_config, tx_events, rpc_url)?;

        info!("Starting BlokliChain processes");

        // Start all chain processes
        let process_handles = blokli_chain.start().await?;

        info!("BlokliChain started successfully");

        // Start API server if enabled
        let api_handle = if api_config.enabled {
            info!("Starting blokli-api server on {}", api_config.bind_address);

            // Connect to database for API server
            let api_db = Database::connect(&database_path)
                .await
                .map_err(|e| BloklidError::NonSpecific(format!("Failed to connect API database: {}", e)))?;

            // Build API app
            let api_app = blokli_api::server::build_app(api_db)
                .await
                .map_err(|e| BloklidError::NonSpecific(format!("Failed to build API app: {}", e)))?;

            // Spawn API server as a background task
            let handle = tokio::spawn(async move {
                let listener = tokio::net::TcpListener::bind(api_config.bind_address)
                    .await
                    .expect("Failed to bind API server");

                info!("API server listening on {}", api_config.bind_address);
                if api_config.playground_enabled {
                    info!(
                        "GraphQL Playground available at http://{}/graphql",
                        api_config.bind_address
                    );
                }

                axum::serve(listener, api_app).await.expect("API server failed");
            });

            info!("API server started successfully");
            Some(handle)
        } else {
            info!("API server disabled in configuration");
            None
        };

        (process_handles, api_handle)
    };

    info!("daemon running; send SIGHUP to reload config, SIGINT/SIGTERM to stop");

    let mut signals = Signals::new([Signal::Hup, Signal::Int, Signal::Term])?;
    while let Some(signal) = signals.try_next().await? {
        match signal {
            Signal::Hup => {
                info!("received SIGHUP; reloading configuration");
                match args.load_config(false) {
                    Ok(new_cfg) => {
                        let mut cfg_guard = config
                            .write()
                            .map_err(|_| BloklidError::NonSpecific("failed to lock config".into()))?;
                        *cfg_guard = new_cfg;
                        warn!("Configuration reloaded, but indexer continues with original settings");
                    }
                    Err(error) => error!(%error, "failed to reload configuration"),
                }
            }
            Signal::Int | Signal::Term => {
                info!("received SIGINT/SIGTERM; shutting down");
                break;
            }
            _ => {
                warn!("received unknown signal; ignoring");
            }
        }
    }

    // Abort all chain processes
    for (process_type, handle) in process_handles {
        info!("Stopping {:?} process", process_type);
        handle.abort();
    }
    info!("All BlokliChain processes stopped");

    // Stop API server if it was started
    if let Some(handle) = api_handle {
        info!("Stopping API server");
        handle.abort();
        info!("API server stopped");
    }

    info!("bloklid stopped gracefully");
    Ok(())
}
