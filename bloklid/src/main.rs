mod args;
mod config;
mod constants;
mod errors;
mod network;
mod telemetry;
mod telemetry_common;

use std::{
    process::ExitCode,
    sync::{Arc, RwLock},
    time::Duration,
};

use args::{Args, Command, generate_config_template, peek_verbosity_from_env_args};
use async_signal::{Signal, Signals};
use blokli_chain_api::BlokliChain;
use blokli_chain_indexer::{startup, utils::redact_url};
use blokli_db::db::{BlokliDb, BlokliDbConfig};
use clap::Parser;
use futures::TryStreamExt;
use sea_orm::Database;
use tokio::net::TcpListener;

use crate::{
    config::{Config, redact_database_url},
    errors::BloklidError,
};

#[tokio::main]
async fn main() -> ExitCode {
    const BIN_NAME: &str = env!("CARGO_PKG_NAME");
    let verbosity = peek_verbosity_from_env_args();

    if telemetry_common::install_base_subscriber(verbosity).is_err() {
        return ExitCode::FAILURE;
    }

    let args = match Args::try_parse() {
        Ok(args) => args,
        Err(error) => {
            if matches!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            ) {
                tracing::info!("{error}");
                return ExitCode::SUCCESS;
            }
            tracing::error!(%error, "error parsing '{BIN_NAME}' arguments");
            return ExitCode::FAILURE;
        }
    };

    if !matches!(args.command, Some(Command::GenerateConfig { .. })) {
        let config = match args.load_config(true) {
            Ok(config) => config,
            Err(error) => {
                tracing::error!(%error, "error loading '{BIN_NAME}' config");
                return ExitCode::FAILURE;
            }
        };

        if let Err(error) = telemetry::init(args.verbose, &config.telemetry) {
            tracing::error!(%error, "error initializing '{BIN_NAME}' telemetry");
            return ExitCode::FAILURE;
        }

        if let Err(error) = run(args, Some(config)).await {
            tracing::error!(%error, "error while running '{BIN_NAME}'");
            return ExitCode::FAILURE;
        }
    } else if let Err(error) = run(args, None).await {
        tracing::error!(%error, "error while running '{BIN_NAME}'");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

async fn run(args: Args, initial_config: Option<Config>) -> errors::Result<()> {
    // Handle subcommands
    if let Some(command) = args.command {
        match command {
            Command::GenerateConfig { output } => {
                tracing::info!("Generating configuration template at: {}", output.display());

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

                tracing::info!("Configuration template successfully written to: {}", output.display());
                return Ok(());
            }
        }
    }

    // Normal daemon operation
    tracing::info!(
        verbosity = args.verbose,
        config = args.config.as_ref().map(|p| p.display().to_string()).as_deref(),
        "bloklid starting"
    );

    // Initial config load
    let config = Arc::new(RwLock::new(match initial_config {
        Some(config) => config,
        None => args.load_config(true)?,
    }));

    // Log the final configuration with redacted secrets
    {
        let cfg = config
            .read()
            .map_err(|_| BloklidError::NonSpecific("failed to lock config for logging".into()))?;
        tracing::info!("{}", cfg.display_redacted());
    }

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

            let database = cfg.database.as_ref().ok_or_else(|| {
                BloklidError::DatabaseNotConfigured(
                    "Database configuration is missing. Ensure either [database] section is present in config file or \
                     BLOKLI_DATABASE_TYPE and BLOKLI_DATABASE_URL are set"
                        .to_string(),
                )
            })?;

            let indexer_config = blokli_chain_indexer::IndexerConfig {
                start_block_number: chain_network.channel_contract_deploy_block as u64,
                fast_sync: cfg.indexer.fast_sync,
                enable_logs_snapshot: cfg.indexer.enable_logs_snapshot,
                logs_snapshot_url: cfg.indexer.logs_snapshot_url.clone(),
                data_directory: cfg.data_directory.clone(),
                event_bus_capacity: cfg.indexer.subscription.event_bus_capacity,
                shutdown_signal_capacity: cfg.indexer.subscription.shutdown_signal_capacity,
            };

            (
                database.to_url(),
                database.to_logs_url(),
                chain_network,
                cfg.contracts,
                indexer_config,
                cfg.rpc_url.clone(),
                cfg.api.clone(),
            )
        };

        if let Some(logs_path) = &logs_database_path {
            tracing::info!("Initializing dual-database setup:");
            tracing::info!("  Index database: {}", redact_database_url(&database_path));
            tracing::info!("  Logs database: {}", redact_database_url(logs_path));
        } else {
            tracing::info!("Initializing single database: {}", redact_database_url(&database_path));
        }

        // Initialize database
        let db_config = {
            let cfg = config
                .read()
                .map_err(|_| BloklidError::NonSpecific("failed to lock config".into()))?;

            BlokliDbConfig {
                max_connections: {
                    cfg.database
                        .as_ref()
                        .ok_or_else(|| {
                            BloklidError::DatabaseNotConfigured(
                                "Failed to read database configuration during connection pool initialization"
                                    .to_string(),
                            )
                        })?
                        .max_connections()
                },
                log_slow_queries: Duration::from_secs(1),
                network_name: cfg.network.to_string(),
            }
        };

        let is_in_memory = {
            let cfg = config
                .read()
                .map_err(|_| BloklidError::NonSpecific("failed to lock config".into()))?;
            cfg.database
                .as_ref()
                .ok_or_else(|| {
                    BloklidError::DatabaseNotConfigured(
                        "Failed to read database configuration during connection pool initialization".to_string(),
                    )
                })?
                .is_in_memory()
        };

        let db = if is_in_memory {
            BlokliDb::new_in_memory().await?
        } else {
            BlokliDb::new(&database_path, logs_database_path.as_deref(), db_config).await?
        };

        // Initialize singleton entries for chain_info and node_info
        db.ensure_singletons()
            .await
            .map_err(|e| BloklidError::NonSpecific(format!("Failed to initialize database singletons: {e}")))?;

        tracing::info!("Connecting to RPC endpoint: {}", redact_url(&rpc_url));

        // Extract chain_id and network name for configuration
        let chain_id = chain_network.chain_id;
        let network = {
            let cfg = config
                .read()
                .map_err(|_| BloklidError::NonSpecific("failed to lock config".into()))?;
            cfg.network.to_string()
        };

        // Create BlokliChain instance
        let blokli_chain = BlokliChain::new(db, chain_network, contracts, indexer_config, rpc_url)?;

        // Verify RPC supports required capabilities (debug tracing)
        blokli_chain.verify_rpc_capabilities().await?;

        startup::refresh_preseeded_safe_modules(blokli_chain.db(), blokli_chain.rpc()).await?;

        // Get IndexerState for API subscriptions
        let indexer_state = blokli_chain.indexer_state();

        // Start API server if enabled (before starting indexer processes)
        // This ensures the API is available immediately even if indexer initialization takes time
        let api_handle = if api_config.enabled {
            tracing::info!("Starting blokli-api server on {}", api_config.bind_address);

            // Connect to database for API server
            let api_db = Database::connect(&database_path)
                .await
                .map_err(|e| BloklidError::NonSpecific(format!("Failed to connect API database: {}", e)))?;

            // Construct blokli-api ApiConfig from bloklid config
            // We need to get rpc_url and contracts from the original config
            let (rpc_url_for_api, _contracts_for_api, expected_block_time, finality) = {
                let cfg = config
                    .read()
                    .map_err(|_| BloklidError::NonSpecific("failed to lock config".into()))?;
                (
                    cfg.rpc_url.clone(),
                    cfg.contracts,
                    cfg.network.expected_block_time(),
                    cfg.network.confirmations(),
                )
            };

            let blokli_api_config = blokli_api::config::ApiConfig {
                bind_address: api_config.bind_address,
                playground_enabled: api_config.playground_enabled,
                database_url: database_path.clone(),
                tls: None,
                cors_allowed_origins: vec!["*".to_string()], // Permissive for now
                chain_id,
                rpc_url: rpc_url_for_api,
                contract_addresses: contracts,
                expected_block_time,
                gas_multiplier: api_config.gas_multiplier,
                sse_keepalive: blokli_api::config::SseKeepAliveConfig {
                    enabled: api_config.sse_keepalive.enabled,
                    interval: api_config.sse_keepalive.interval,
                    text: api_config.sse_keepalive.text.clone(),
                },
                health: blokli_api::config::HealthConfig {
                    max_indexer_lag: api_config.health.max_indexer_lag,
                    timeout: api_config.health.timeout,
                    readiness_check_interval: api_config.health.readiness_check_interval,
                },
            };

            // Get RPC operations from blokli_chain for balance queries
            let rpc_operations = Arc::new(blokli_chain.rpc().clone());

            // Build API app with indexer state for subscriptions and transaction components
            let api_app = blokli_api::server::build_app(
                api_db,
                network.clone(),
                blokli_api_config,
                expected_block_time,
                finality,
                indexer_state,
                blokli_chain.transaction_executor(),
                blokli_chain.transaction_store(),
                rpc_operations,
            )
            .await
            .map_err(|e| BloklidError::NonSpecific(format!("Failed to build API app: {}", e)))?;

            // Bind the listener first to ensure the port is available before spawning the server
            let listener = TcpListener::bind(api_config.bind_address)
                .await
                .map_err(|e| BloklidError::NonSpecific(format!("Failed to bind API server: {}", e)))?;

            tracing::info!("API server listening on {}", api_config.bind_address);
            if api_config.playground_enabled {
                tracing::info!(
                    "GraphQL Playground available at http://{}/graphql",
                    api_config.bind_address
                );
            }

            // Spawn API server as a background task
            let handle = tokio::spawn(async move {
                axum::serve(listener, api_app).await.expect("API server failed");
            });

            tracing::info!("API server started successfully");
            Some(handle)
        } else {
            tracing::info!("API server disabled in configuration");
            None
        };

        tracing::info!("Starting BlokliChain processes");

        // Start all chain processes
        let process_handles = blokli_chain.start().await?;

        tracing::info!("BlokliChain started successfully");

        (process_handles, api_handle)
    };

    tracing::info!("daemon running; send SIGHUP to reload config, SIGINT/SIGTERM to stop");

    let mut signals = Signals::new([Signal::Hup, Signal::Int, Signal::Term])?;
    while let Some(signal) = signals.try_next().await? {
        match signal {
            Signal::Hup => {
                tracing::info!("received SIGHUP; reloading configuration");
                match args.load_config(false) {
                    Ok(new_cfg) => {
                        let mut cfg_guard = config
                            .write()
                            .map_err(|_| BloklidError::NonSpecific("failed to lock config".into()))?;
                        *cfg_guard = new_cfg;
                        tracing::warn!(
                            "Configuration reloaded, but running indexer, API, and telemetry continue with original \
                             settings"
                        );
                    }
                    Err(error) => tracing::error!(%error, "failed to reload configuration"),
                }
            }
            Signal::Int | Signal::Term => {
                tracing::info!("received SIGINT/SIGTERM; shutting down");
                break;
            }
            _ => {
                tracing::warn!("received unknown signal; ignoring");
            }
        }
    }

    // Abort all chain processes
    for (process_type, handle) in process_handles {
        tracing::info!("Stopping {:?} process", process_type);
        handle.abort();
    }
    tracing::info!("All BlokliChain processes stopped");

    // Stop API server if it was started
    if let Some(handle) = api_handle {
        tracing::info!("Stopping API server");
        handle.abort();
        tracing::info!("API server stopped");
    }

    tracing::info!("bloklid stopped gracefully");
    Ok(())
}
