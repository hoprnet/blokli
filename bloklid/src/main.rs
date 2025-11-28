mod config;
mod constants;
mod errors;

use std::{
    path::PathBuf,
    sync::{Arc, RwLock},
    time::Duration,
};

// Import config crate to avoid conflict with module `config`
use ::config as config_rs;
use async_signal::{Signal, Signals};
use blokli_chain_api::BlokliChain;
use blokli_chain_types::ContractAddresses;
use blokli_db::db::{BlokliDb, BlokliDbConfig};
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
        let mut builder = config_rs::Config::builder();

        // Layer 1: File (if provided)
        if let Some(config_path) = &self.config {
            builder = builder.add_source(config_rs::File::from(config_path.clone()));
        } else if use_default {
            warn!("no configuration file specified; using defaults and environment variables");
        } else {
            return Err(ConfigError::NoConfiguration.into());
        }

        // Layer 2: Environment Variables (Manual Mapping)
        // We manually map env vars to config keys to ensure precedence and specific naming
        let env_mappings = [
            // Root
            ("BLOKLI_HOST", "host"),
            ("BLOKLI_DATA_DIRECTORY", "data_directory"),
            ("BLOKLI_NETWORK", "network"),
            ("BLOKLI_RPC_URL", "rpc_url"),
            ("BLOKLI_MAX_RPC_REQUESTS_PER_SEC", "max_rpc_requests_per_sec"),
            // Database - Canonical
            ("DATABASE_URL", "database.url"),
            ("PGHOST", "database.host"),
            ("POSTGRES_HOST", "database.host"),
            ("PGPORT", "database.port"),
            ("POSTGRES_PORT", "database.port"),
            ("PGUSER", "database.username"),
            ("POSTGRES_USER", "database.username"),
            ("PGPASSWORD", "database.password"),
            ("POSTGRES_PASSWORD", "database.password"),
            ("PGDATABASE", "database.database"),
            ("POSTGRES_DB", "database.database"),
            // Database - BLOKLI
            ("BLOKLI_DATABASE_TYPE", "database.type"),
            ("BLOKLI_DATABASE_URL", "database.url"),
            ("BLOKLI_DATABASE_HOST", "database.host"),
            ("BLOKLI_DATABASE_PORT", "database.port"),
            ("BLOKLI_DATABASE_USERNAME", "database.username"),
            ("BLOKLI_DATABASE_PASSWORD", "database.password"),
            ("BLOKLI_DATABASE_DATABASE", "database.database"),
            ("BLOKLI_DATABASE_MAX_CONNECTIONS", "database.max_connections"),
            ("BLOKLI_DATABASE_INDEX_PATH", "database.index_path"),
            ("BLOKLI_DATABASE_LOGS_PATH", "database.logs_path"),
            // Indexer
            ("BLOKLI_INDEXER_FAST_SYNC", "indexer.fast_sync"),
            ("BLOKLI_INDEXER_ENABLE_LOGS_SNAPSHOT", "indexer.enable_logs_snapshot"),
            ("BLOKLI_INDEXER_LOGS_SNAPSHOT_URL", "indexer.logs_snapshot_url"),
            // Indexer Subscription
            (
                "BLOKLI_INDEXER_SUBSCRIPTION_EVENT_BUS_CAPACITY",
                "indexer.subscription.event_bus_capacity",
            ),
            (
                "BLOKLI_INDEXER_SUBSCRIPTION_SHUTDOWN_SIGNAL_CAPACITY",
                "indexer.subscription.shutdown_signal_capacity",
            ),
            (
                "BLOKLI_INDEXER_SUBSCRIPTION_BATCH_SIZE",
                "indexer.subscription.batch_size",
            ),
            // API
            ("BLOKLI_API_ENABLED", "api.enabled"),
            ("BLOKLI_API_BIND_ADDRESS", "api.bind_address"),
            ("BLOKLI_API_PLAYGROUND_ENABLED", "api.playground_enabled"),
            // API Health
            ("BLOKLI_API_HEALTH_MAX_INDEXER_LAG", "api.health.max_indexer_lag"),
            ("BLOKLI_API_HEALTH_TIMEOUT", "api.health.timeout"),
            (
                "BLOKLI_API_HEALTH_READINESS_CHECK_INTERVAL",
                "api.health.readiness_check_interval",
            ),
        ];

        // Keys that should be parsed as boolean values
        let boolean_keys = [
            "indexer.fast_sync",
            "indexer.enable_logs_snapshot",
            "api.enabled",
            "api.playground_enabled",
        ];

        for (env_var, config_key) in env_mappings {
            if let Ok(val) = std::env::var(env_var) {
                // Try to parse the value as a more specific type if needed.
                // For boolean config keys, attempt bool parsing first (before u64)
                // so that "1"/"0" are interpreted as booleans, not integers.
                // For other keys, try u64 first, then bool, then fall back to string.
                let override_val: config_rs::Value = if boolean_keys.contains(&config_key) {
                    // For boolean keys, prioritize bool parsing
                    if let Ok(flag) = val.parse::<bool>() {
                        flag.into()
                    } else if let Ok(num) = val.parse::<u64>() {
                        // Support "1"/"0" as booleans by converting to bool
                        (num != 0).into()
                    } else {
                        val.into()
                    }
                } else {
                    // For non-boolean keys, try numeric first, then bool, then string
                    if let Ok(num) = val.parse::<u64>() {
                        num.into()
                    } else if let Ok(flag) = val.parse::<bool>() {
                        flag.into()
                    } else {
                        val.into()
                    }
                };
                builder = builder
                    .set_override(config_key, override_val)
                    .map_err(|e| ConfigError::Parse(e.to_string()))?;
            }
        }

        let config_rs_config = builder.build().map_err(|e| ConfigError::Parse(e.to_string()))?;
        let mut config: Config = config_rs_config
            .try_deserialize()
            .map_err(|e| ConfigError::Parse(e.to_string()))?;

        // Validate that database configuration is provided either in config file or environment variables
        if config.database.is_none() {
            return Err(ConfigError::NoDatabaseConfiguration.into());
        }

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
            node_safe_registry: chain_network_config.node_safe_registry,
            ticket_price_oracle: chain_network_config.ticket_price_oracle,
            winning_probability_oracle: chain_network_config.winning_probability_oracle,
            node_stake_v2_factory: chain_network_config.node_stake_v2_factory,
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

    // Log the final configuration with redacted secrets
    {
        let cfg = config
            .read()
            .map_err(|_| BloklidError::NonSpecific("failed to lock config for logging".into()))?;
        info!("{}", cfg.display_redacted());
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
                cfg.database
                    .as_ref()
                    .ok_or_else(|| {
                        BloklidError::DatabaseNotConfigured(
                            "Failed to read database configuration during connection pool initialization".to_string(),
                        )
                    })?
                    .max_connections()
            },
            log_slow_queries: Duration::from_secs(1),
        };
        let db = BlokliDb::new(&database_path, logs_database_path.as_deref(), db_config).await?;

        // Clone notification manager for API before db is moved to BlokliChain
        let sqlite_notification_manager = db.sqlite_notification_manager().cloned();

        info!("Connecting to RPC endpoint: {}", rpc_url);

        // Extract chain_id and network before chain_network is moved
        let chain_id = chain_network.chain.chain_id as u64;
        let network = chain_network.id.clone();

        // Create BlokliChain instance
        let blokli_chain = BlokliChain::new(db, chain_network, contracts, indexer_config, rpc_url)?;

        // Get IndexerState for API subscriptions
        let indexer_state = blokli_chain.indexer_state();

        // Start API server if enabled (before starting indexer processes)
        // This ensures the API is available immediately even if indexer initialization takes time
        let api_handle = if api_config.enabled {
            info!("Starting blokli-api server on {}", api_config.bind_address);

            // Connect to database for API server
            let api_db = Database::connect(&database_path)
                .await
                .map_err(|e| BloklidError::NonSpecific(format!("Failed to connect API database: {}", e)))?;

            // Construct blokli-api ApiConfig from bloklid config
            // We need to get rpc_url and contracts from the original config
            let (rpc_url_for_api, _contracts_for_api) = {
                let cfg = config
                    .read()
                    .map_err(|_| BloklidError::NonSpecific("failed to lock config".into()))?;
                (cfg.rpc_url.clone(), cfg.contracts)
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
                network,
                blokli_api_config,
                indexer_state,
                blokli_chain.transaction_executor(),
                blokli_chain.transaction_store(),
                rpc_operations,
                sqlite_notification_manager,
            )
            .await
            .map_err(|e| BloklidError::NonSpecific(format!("Failed to build API app: {}", e)))?;

            // Bind the listener first to ensure the port is available before spawning the server
            let listener = tokio::net::TcpListener::bind(api_config.bind_address)
                .await
                .map_err(|e| BloklidError::NonSpecific(format!("Failed to bind API server: {}", e)))?;

            info!("API server listening on {}", api_config.bind_address);
            if api_config.playground_enabled {
                info!(
                    "GraphQL Playground available at http://{}/graphql",
                    api_config.bind_address
                );
            }

            // Spawn API server as a background task
            let handle = tokio::spawn(async move {
                axum::serve(listener, api_app).await.expect("API server failed");
            });

            info!("API server started successfully");
            Some(handle)
        } else {
            info!("API server disabled in configuration");
            None
        };

        info!("Starting BlokliChain processes");

        // Start all chain processes
        let process_handles = blokli_chain.start().await?;

        info!("BlokliChain started successfully");

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

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    #[test]
    fn test_env_var_override() {
        // Create a temp config file
        let mut file = tempfile::Builder::new().suffix(".toml").tempfile().unwrap();
        writeln!(
            file,
            r#"
            host = "127.0.0.1:3000"
            network = "dufour"
            rpc_url = "http://localhost:8545"
            [database]
            type = "postgresql"
            url = "postgres://file:5432/db"
        "#
        )
        .unwrap();
        let path = file.path().to_path_buf();

        // Set env vars that should override config file
        temp_env::with_vars(
            [
                ("BLOKLI_HOST", Some("127.0.0.1:4000")),
                ("BLOKLI_DATABASE_URL", Some("postgres://env:5432/db")),
            ],
            || {
                let args = Args {
                    verbose: 0,
                    config: Some(path),
                    command: None,
                };

                let config = args.load_config(false).expect("Failed to load config");

                // Environment variables should override config file
                assert_eq!(config.host.to_string(), "127.0.0.1:4000");
                match config.database {
                    Some(crate::config::DatabaseConfig::PostgreSql(c)) => {
                        assert_eq!(c.url.as_ref().map(|s| s.as_str()), Some("postgres://env:5432/db"));
                    }
                    _ => panic!("Expected PostgreSQL database config"),
                }
            },
        );
    }

    #[test]
    fn test_canonical_env_var_override() {
        // Create a temp config file
        let mut file = tempfile::Builder::new().suffix(".toml").tempfile().unwrap();
        writeln!(
            file,
            r#"
            host = "127.0.0.1:3000"
            network = "dufour"
            rpc_url = "http://localhost:8545"
            [database]
            type = "postgresql"
            url = "postgres://file:5432/db"
        "#
        )
        .unwrap();
        let path = file.path().to_path_buf();

        // Set canonical DATABASE_URL env var
        temp_env::with_var("DATABASE_URL", Some("postgres://canonical:5432/db"), || {
            let args = Args {
                verbose: 0,
                config: Some(path),
                command: None,
            };

            let config = args.load_config(false).expect("Failed to load config");

            // Canonical env var should override config file
            match config.database {
                Some(crate::config::DatabaseConfig::PostgreSql(c)) => {
                    assert_eq!(c.url.as_ref().map(|s| s.as_str()), Some("postgres://canonical:5432/db"));
                }
                _ => panic!("Expected PostgreSQL database config"),
            }
        });
    }

    #[test]
    fn test_env_only_database_config() {
        // Create config file without [database] section
        let mut file = tempfile::Builder::new().suffix(".toml").tempfile().unwrap();
        writeln!(
            file,
            r#"
            host = "0.0.0.0:3064"
            network = "dufour"
            rpc_url = "http://localhost:8545"
        "#
        )
        .unwrap();
        let path = file.path().to_path_buf();

        // Database config must come entirely from environment variables
        temp_env::with_vars(
            [
                ("BLOKLI_DATABASE_TYPE", Some("postgresql")),
                ("BLOKLI_DATABASE_URL", Some("postgresql://user:pass@localhost/db")),
            ],
            || {
                let args = Args {
                    verbose: 0,
                    config: Some(path),
                    command: None,
                };

                let config = args
                    .load_config(false)
                    .expect("Should load database config from environment");

                // Should successfully load database config from environment variables
                match config.database {
                    Some(crate::config::DatabaseConfig::PostgreSql(c)) => {
                        assert_eq!(
                            c.url.as_ref().map(|s| s.as_str()),
                            Some("postgresql://user:pass@localhost/db")
                        );
                        assert_eq!(c.max_connections, 10); // Default value
                    }
                    _ => panic!("Expected PostgreSQL database config"),
                }
            },
        );
    }

    #[test]
    fn test_missing_database_config_fails() {
        // Create config file without [database] section
        let mut file = tempfile::Builder::new().suffix(".toml").tempfile().unwrap();
        writeln!(
            file,
            r#"
            host = "0.0.0.0:3064"
            network = "dufour"
            rpc_url = "http://localhost:8545"
        "#
        )
        .unwrap();
        let path = file.path().to_path_buf();

        // Don't set any database environment variables
        temp_env::with_var("BLOKLI_DATABASE_TYPE", None::<&str>, || {
            let args = Args {
                verbose: 0,
                config: Some(path),
                command: None,
            };

            // Should fail when database config is missing from both file and environment
            let result = args.load_config(false);
            assert!(result.is_err(), "Should fail when database config is missing");

            let err_msg = result.unwrap_err().to_string();
            assert!(
                err_msg.contains("database configuration"),
                "Error message should mention database configuration"
            );
        });
    }

    #[test]
    fn test_env_var_string_values_are_parsed_by_config_rs() {
        let mut file = tempfile::Builder::new().suffix(".toml").tempfile().unwrap();
        writeln!(
            file,
            r#"
            host = "0.0.0.0:3064"
            network = "dufour"
            rpc_url = "http://localhost:8545"
            [database]
            type = "postgresql"
            url = "postgres://file:5432/db"
        "#
        )
        .unwrap();
        let path = file.path().to_path_buf();

        temp_env::with_var("BLOKLI_NETWORK", Some("rotsee"), || {
            let args = Args {
                verbose: 0,
                config: Some(path),
                command: None,
            };

            let config = args.load_config(false).expect("Failed to load config");

            // String environment variables should be properly parsed to their target types
            assert_eq!(config.network, "rotsee", "String env var should override config");
        });
    }

    #[test]
    fn test_database_config_defaults_max_connections_to_10() {
        // Create a temp config file with PostgreSQL configuration
        let mut file = tempfile::Builder::new().suffix(".toml").tempfile().unwrap();
        writeln!(
            file,
            r#"
            host = "0.0.0.0:3064"
            network = "dufour"
            rpc_url = "http://localhost:8545"
            [database]
            type = "postgresql"
            url = "postgres://file:5432/db"
        "#
        )
        .unwrap();
        let path = file.path().to_path_buf();

        temp_env::with_var("BLOKLI_DATABASE_MAX_CONNECTIONS", None::<&str>, || {
            let args = Args {
                verbose: 0,
                config: Some(path),
                command: None,
            };

            let config = args.load_config(false).expect("Failed to load config");

            // Verify max_connections defaults to 10 (u32) when not specified
            match config.database {
                Some(crate::config::DatabaseConfig::PostgreSql(c)) => {
                    assert_eq!(c.max_connections, 10, "PostgreSQL max_connections should default to 10");
                }
                _ => panic!("Expected PostgreSQL database config"),
            }
        });
    }

    #[test]
    fn test_database_config_max_connections_from_config_file() {
        // Create a temp config file with explicit max_connections
        let mut file = tempfile::Builder::new().suffix(".toml").tempfile().unwrap();
        writeln!(
            file,
            r#"
            host = "0.0.0.0:3064"
            network = "dufour"
            rpc_url = "http://localhost:8545"
            [database]
            type = "postgresql"
            url = "postgres://file:5432/db"
            max_connections = 50
        "#
        )
        .unwrap();
        let path = file.path().to_path_buf();

        temp_env::with_var("BLOKLI_DATABASE_MAX_CONNECTIONS", None::<&str>, || {
            let args = Args {
                verbose: 0,
                config: Some(path),
                command: None,
            };

            let config = args.load_config(false).expect("Failed to load config");

            // Verify max_connections is correctly read as u32 from config file
            match config.database {
                Some(crate::config::DatabaseConfig::PostgreSql(c)) => {
                    assert_eq!(
                        c.max_connections, 50,
                        "max_connections should be parsed as u32 from config file"
                    );
                }
                _ => panic!("Expected PostgreSQL database config"),
            }
        });
    }

    #[test]
    fn test_sqlite_database_config_max_connections_defaults() {
        // Create a temp config file with SQLite database configuration
        let mut file = tempfile::Builder::new().suffix(".toml").tempfile().unwrap();
        writeln!(
            file,
            r#"
            host = "0.0.0.0:3064"
            network = "dufour"
            rpc_url = "http://localhost:8545"
            [database]
            type = "sqlite"
            index_path = "data/index.db"
            logs_path = "data/logs.db"
        "#
        )
        .unwrap();
        let path = file.path().to_path_buf();

        temp_env::with_var("BLOKLI_DATABASE_MAX_CONNECTIONS", None::<&str>, || {
            let args = Args {
                verbose: 0,
                config: Some(path),
                command: None,
            };

            let config = args.load_config(false).expect("Failed to load config");

            // Verify SQLite max_connections defaults to 10 (u32)
            match config.database {
                Some(crate::config::DatabaseConfig::Sqlite(c)) => {
                    assert_eq!(c.max_connections, 10, "SQLite max_connections should default to 10");
                }
                _ => panic!("Expected SQLite database config"),
            }
        });
    }

    #[test]
    fn test_env_var_blokli_database_max_connections_integer_casting() {
        // Create a temp config file with PostgreSQL configuration
        let mut file = tempfile::Builder::new().suffix(".toml").tempfile().unwrap();
        writeln!(
            file,
            r#"
            host = "0.0.0.0:3064"
            network = "dufour"
            rpc_url = "http://localhost:8545"
            [database]
            type = "postgresql"
            url = "postgres://file:5432/db"
        "#
        )
        .unwrap();
        let path = file.path().to_path_buf();

        // Override max_connections with environment variable (string in env, should be parsed to u32)
        temp_env::with_var("BLOKLI_DATABASE_MAX_CONNECTIONS", Some("75"), || {
            let args = Args {
                verbose: 0,
                config: Some(path),
                command: None,
            };

            let config = args.load_config(false).expect("Failed to load config");

            // Verify max_connections was correctly parsed from string to u64 then to u32
            match config.database {
                Some(crate::config::DatabaseConfig::PostgreSql(c)) => {
                    assert_eq!(
                        c.max_connections, 75,
                        "BLOKLI_DATABASE_MAX_CONNECTIONS should be parsed as u32 from env var"
                    );
                }
                _ => panic!("Expected PostgreSQL database config"),
            }
        });
    }

    #[test]
    fn test_env_var_blokli_database_max_connections_for_sqlite() {
        // Create a temp config file with SQLite configuration
        let mut file = tempfile::Builder::new().suffix(".toml").tempfile().unwrap();
        writeln!(
            file,
            r#"
            host = "0.0.0.0:3064"
            network = "dufour"
            rpc_url = "http://localhost:8545"
            [database]
            type = "sqlite"
            index_path = "data/index.db"
            logs_path = "data/logs.db"
        "#
        )
        .unwrap();
        let path = file.path().to_path_buf();

        // Override max_connections with environment variable for SQLite
        temp_env::with_var("BLOKLI_DATABASE_MAX_CONNECTIONS", Some("40"), || {
            let args = Args {
                verbose: 0,
                config: Some(path),
                command: None,
            };

            let config = args.load_config(false).expect("Failed to load config");

            // Verify max_connections was correctly parsed from string to u64 then to u32 for SQLite
            match config.database {
                Some(crate::config::DatabaseConfig::Sqlite(c)) => {
                    assert_eq!(
                        c.max_connections, 40,
                        "BLOKLI_DATABASE_MAX_CONNECTIONS should be parsed as u32 from env var for SQLite"
                    );
                }
                _ => panic!("Expected SQLite database config"),
            }
        });
    }

    #[test]
    fn test_boolean_env_var_with_true_string() {
        let mut file = tempfile::Builder::new().suffix(".toml").tempfile().unwrap();
        writeln!(
            file,
            r#"
            host = "0.0.0.0:3064"
            network = "dufour"
            rpc_url = "http://localhost:8545"
            [database]
            type = "postgresql"
            url = "postgres://file:5432/db"
            [indexer]
            fast_sync = false
        "#
        )
        .unwrap();
        let path = file.path().to_path_buf();

        temp_env::with_var("BLOKLI_INDEXER_FAST_SYNC", Some("true"), || {
            let args = Args {
                verbose: 0,
                config: Some(path),
                command: None,
            };

            let config = args.load_config(false).expect("Failed to load config");
            assert_eq!(
                config.indexer.fast_sync, true,
                "BLOKLI_INDEXER_FAST_SYNC should be parsed as bool"
            );
        });
    }

    #[test]
    fn test_boolean_env_var_with_digit_one() {
        let mut file = tempfile::Builder::new().suffix(".toml").tempfile().unwrap();
        writeln!(
            file,
            r#"
            host = "0.0.0.0:3064"
            network = "dufour"
            rpc_url = "http://localhost:8545"
            [database]
            type = "postgresql"
            url = "postgres://file:5432/db"
            [indexer]
            fast_sync = false
        "#
        )
        .unwrap();
        let path = file.path().to_path_buf();

        // "1" should be parsed as boolean true, not integer 1
        temp_env::with_var("BLOKLI_INDEXER_FAST_SYNC", Some("1"), || {
            let args = Args {
                verbose: 0,
                config: Some(path),
                command: None,
            };

            let config = args.load_config(false).expect("Failed to load config");
            assert_eq!(
                config.indexer.fast_sync, true,
                "BLOKLI_INDEXER_FAST_SYNC='1' should be parsed as boolean true"
            );
        });
    }

    #[test]
    fn test_boolean_env_var_with_digit_zero() {
        let mut file = tempfile::Builder::new().suffix(".toml").tempfile().unwrap();
        writeln!(
            file,
            r#"
            host = "0.0.0.0:3064"
            network = "dufour"
            rpc_url = "http://localhost:8545"
            [database]
            type = "postgresql"
            url = "postgres://file:5432/db"
            [indexer]
            fast_sync = true
        "#
        )
        .unwrap();
        let path = file.path().to_path_buf();

        // "0" should be parsed as boolean false, not integer 0
        temp_env::with_var("BLOKLI_INDEXER_FAST_SYNC", Some("0"), || {
            let args = Args {
                verbose: 0,
                config: Some(path),
                command: None,
            };

            let config = args.load_config(false).expect("Failed to load config");
            assert_eq!(
                config.indexer.fast_sync, false,
                "BLOKLI_INDEXER_FAST_SYNC='0' should be parsed as boolean false"
            );
        });
    }

    #[test]
    fn test_boolean_env_var_false_string() {
        let mut file = tempfile::Builder::new().suffix(".toml").tempfile().unwrap();
        writeln!(
            file,
            r#"
            host = "0.0.0.0:3064"
            network = "dufour"
            rpc_url = "http://localhost:8545"
            [database]
            type = "postgresql"
            url = "postgres://file:5432/db"
            [indexer]
            fast_sync = true
        "#
        )
        .unwrap();
        let path = file.path().to_path_buf();

        temp_env::with_var("BLOKLI_INDEXER_FAST_SYNC", Some("false"), || {
            let args = Args {
                verbose: 0,
                config: Some(path),
                command: None,
            };

            let config = args.load_config(false).expect("Failed to load config");
            assert_eq!(
                config.indexer.fast_sync, false,
                "BLOKLI_INDEXER_FAST_SYNC should be parsed as bool"
            );
        });
    }

    #[test]
    fn test_numeric_env_var_still_parsed_as_integer() {
        let mut file = tempfile::Builder::new().suffix(".toml").tempfile().unwrap();
        writeln!(
            file,
            r#"
            host = "0.0.0.0:3064"
            network = "dufour"
            rpc_url = "http://localhost:8545"
            [database]
            type = "postgresql"
            url = "postgres://file:5432/db"
            [api.health]
            max_indexer_lag = 5
        "#
        )
        .unwrap();
        let path = file.path().to_path_buf();

        // Non-boolean numeric config should still parse as u64
        temp_env::with_var("BLOKLI_API_HEALTH_MAX_INDEXER_LAG", Some("20"), || {
            let args = Args {
                verbose: 0,
                config: Some(path),
                command: None,
            };

            let config = args.load_config(false).expect("Failed to load config");
            assert_eq!(
                config.api.health.max_indexer_lag, 20,
                "Non-boolean numeric config should parse as u64"
            );
        });
    }
}
