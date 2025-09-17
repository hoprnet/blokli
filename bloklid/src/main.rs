mod config;
mod errors;

use std::{
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
    time::Duration,
};

use async_signal::{Signal, Signals};
use blokli_chain_api::{BlokliChain, SignificantChainEvent};
use blokli_chain_types::ContractAddresses;
use blokli_db_sql::db::{BlokliDb, BlokliDbConfig};
use clap::Parser;
use futures::TryStreamExt;
use hopr_crypto_types::prelude::*;
use hopr_primitive_types::prelude::*;
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
    about = "Daemon for indexing HOPR on-chain events and executing HOPR-related on-chain transactions"
)]
struct Args {
    /// Increase output verbosity (-v, -vv)
    #[arg(short = 'v', long = "verbose", action = clap::ArgAction::Count)]
    verbose: u8,

    /// Optional path to a configuration file
    #[arg(short = 'c', long = "config", value_name = "FILE")]
    config: Option<PathBuf>,
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

        let config: Config = serde_yaml::from_reader(std::fs::File::open(
            self.config.as_ref().ok_or(ConfigError::NoConfiguration)?,
        )?)
        .map_err(|e| ConfigError::Parse(e.to_string()))?;

        config.validate().map_err(ConfigError::Validation)?;
        Ok(config)
    }
}

#[tokio::main]
async fn main() -> errors::Result<()> {
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

    info!(
        verbosity = args.verbose,
        config = args.config.as_ref().map(|p| p.display().to_string()).as_deref(),
        "bloklid starting"
    );

    // Initial config load
    let config = Arc::new(RwLock::new(args.load_config(true)?));

    // Initialize components
    let process_handles = {
        // Extract all needed values from config before any await
        let (database_path, chain_cfg, indexer_cfg, data_directory) = {
            let cfg = config
                .read()
                .map_err(|_| BloklidError::NonSpecific("failed to lock config".into()))?;

            (
                cfg.database_path.clone(),
                cfg.chain.clone(),
                cfg.indexer.clone(),
                cfg.data_directory.clone(),
            )
        };

        info!("Initializing database at: {}", database_path);

        // Initialize database
        let db_config = BlokliDbConfig {
            create_if_missing: true,
            force_create: false,
            log_slow_queries: Duration::from_secs(1),
        };
        let db_path = Path::new(&database_path);
        let db = BlokliDb::new(db_path, db_config).await?;

        info!("Connecting to RPC endpoint: {}", chain_cfg.chain.default_provider);

        // Default contract addresses - should be configured properly
        let contract_addresses = ContractAddresses {
            token: Address::default(),
            channels: Address::default(),
            announcements: Address::default(),
            network_registry: Address::default(),
            network_registry_proxy: Address::default(),
            safe_registry: Address::default(),
            price_oracle: Address::default(),
            win_prob_oracle: Address::default(),
            stake_factory: Address::default(),
            module_implementation: Address::default(),
        };

        // Configure indexer
        let indexer_config = blokli_chain_indexer::IndexerConfig {
            start_block_number: indexer_cfg.start_block_number,
            fast_sync: indexer_cfg.fast_sync,
            enable_logs_snapshot: indexer_cfg.enable_logs_snapshot,
            logs_snapshot_url: indexer_cfg.logs_snapshot_url,
            data_directory,
        };

        // Create channel for chain events
        let (tx_events, _rx_events) = async_channel::unbounded::<SignificantChainEvent>();

        // Create BlokliChain instance
        let blokli_chain = BlokliChain::new(
            db,
            chain_cfg.into(), // Convert to the API's ChainNetworkConfig
            contract_addresses,
            indexer_config,
            tx_events,
        )?;

        info!("Starting BlokliChain processes");

        // Start all chain processes
        let process_handles = blokli_chain.start().await?;

        info!("BlokliChain started successfully");

        process_handles
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

    info!("bloklid stopped gracefully");
    Ok(())
}
