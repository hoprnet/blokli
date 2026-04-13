use std::{ffi::OsString, path::PathBuf};

use ::config as config_rs;
use blokli_chain_types::{AlloyAddressExt, ChainConfig};
use clap::{Parser, Subcommand};
use validator::Validate;

use crate::{
    config::Config,
    errors::{self, BloklidError, ConfigError},
    network::Network,
};

#[derive(Debug, Parser)]
#[command(
    name = "bloklid",
    about = "Daemon for indexing HOPR on-chain events and executing HOPR-related on-chain transactions",
    version = crate::constants::APP_VERSION_COERCED
)]
pub(crate) struct Args {
    #[arg(short = 'v', long = "verbose", action = clap::ArgAction::Count, global = true)]
    pub(crate) verbose: u8,

    #[arg(short = 'c', long = "config", value_name = "FILE", global = true)]
    pub(crate) config: Option<PathBuf>,

    #[command(subcommand)]
    pub(crate) command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    GenerateConfig {
        #[arg(value_name = "FILE")]
        output: PathBuf,
    },
}

pub(crate) fn generate_config_template() -> String {
    include_str!("../example-config.toml").to_string()
}

pub(crate) fn peek_verbosity_from_env_args() -> u8 {
    fn verbosity_from_arg(arg: &OsString) -> u8 {
        let Some(arg) = arg.to_str() else {
            return 0;
        };

        if arg == "--verbose" || arg == "-v" {
            return 1;
        }

        if arg.starts_with('-') && !arg.starts_with("--") && arg[1..].chars().all(|value| value == 'v') {
            return (arg.len() - 1) as u8;
        }

        0
    }

    std::env::args_os()
        .skip(1)
        .map(|arg| verbosity_from_arg(&arg))
        .fold(0u8, u8::saturating_add)
}

impl Args {
    pub(crate) fn load_config(&self, use_default: bool) -> errors::Result<Config> {
        let mut builder = config_rs::Config::builder();

        if let Some(config_path) = &self.config {
            builder = builder.add_source(config_rs::File::from(config_path.clone()));
        } else if use_default {
            tracing::warn!("no configuration file specified; using defaults and environment variables");
        } else {
            return Err(ConfigError::NoConfiguration.into());
        }

        let env_mappings = [
            ("BLOKLI_DATA_DIRECTORY", "data_directory"),
            ("BLOKLI_NETWORK", "network"),
            ("BLOKLI_RPC_URL", "rpc_url"),
            ("BLOKLI_MAX_RPC_REQUESTS_PER_SEC", "max_rpc_requests_per_sec"),
            ("BLOKLI_MAX_BLOCK_RANGE", "max_block_range"),
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
            ("BLOKLI_INDEXER_FAST_SYNC", "indexer.fast_sync"),
            ("BLOKLI_INDEXER_ENABLE_LOGS_SNAPSHOT", "indexer.enable_logs_snapshot"),
            ("BLOKLI_INDEXER_LOGS_SNAPSHOT_URL", "indexer.logs_snapshot_url"),
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
            ("BLOKLI_API_ENABLED", "api.enabled"),
            ("BLOKLI_API_BIND_ADDRESS", "api.bind_address"),
            ("BLOKLI_API_PLAYGROUND_ENABLED", "api.playground_enabled"),
            ("BLOKLI_API_GAS_MULTIPLIER", "api.gas_multiplier"),
            ("BLOKLI_API_SSE_KEEPALIVE_ENABLED", "api.sse_keepalive.enabled"),
            ("BLOKLI_API_SSE_KEEPALIVE_INTERVAL", "api.sse_keepalive.interval"),
            ("BLOKLI_API_SSE_KEEPALIVE_TEXT", "api.sse_keepalive.text"),
            ("BLOKLI_API_HEALTH_MAX_INDEXER_LAG", "api.health.max_indexer_lag"),
            ("BLOKLI_API_HEALTH_TIMEOUT", "api.health.timeout"),
            (
                "BLOKLI_API_HEALTH_READINESS_CHECK_INTERVAL",
                "api.health.readiness_check_interval",
            ),
            ("BLOKLI_METRIC_EXPORT_INTERVAL", "telemetry.metric_export_interval"),
            ("BLOKLI_OTLP_ENDPOINT", "telemetry.otlp_endpoint"),
            ("BLOKLI_OTLP_SIGNALS", "telemetry.otlp_signals"),
        ];

        let boolean_keys = [
            "indexer.fast_sync",
            "indexer.enable_logs_snapshot",
            "api.enabled",
            "api.playground_enabled",
            "api.sse_keepalive.enabled",
        ];

        for (env_var, config_key) in env_mappings {
            if let Ok(val) = std::env::var(env_var) {
                let override_val: config_rs::Value = if boolean_keys.contains(&config_key) {
                    if let Ok(flag) = val.parse::<bool>() {
                        flag.into()
                    } else if let Ok(num) = val.parse::<u64>() {
                        (num != 0).into()
                    } else {
                        val.into()
                    }
                } else if let Ok(num) = val.parse::<u64>() {
                    num.into()
                } else if let Ok(num) = val.parse::<f64>() {
                    num.into()
                } else if let Ok(flag) = val.parse::<bool>() {
                    flag.into()
                } else {
                    val.into()
                };

                builder = builder
                    .set_override(config_key, override_val)
                    .map_err(|error| ConfigError::Parse(error.to_string()))?;
            }
        }

        let config_rs_config = builder.build().map_err(|error| ConfigError::Parse(error.to_string()))?;
        let mut config: Config = config_rs_config
            .try_deserialize()
            .map_err(|error| ConfigError::Parse(error.to_string()))?;

        if !config.api.gas_multiplier.is_finite() || config.api.gas_multiplier < 1.0 {
            return Err(ConfigError::Parse(
                "api.gas_multiplier must be a finite number greater than or equal to 1".to_string(),
            )
            .into());
        }

        if config
            .telemetry
            .otlp_endpoint
            .as_deref()
            .map(str::trim)
            .is_some_and(|value| !value.is_empty())
            && config.telemetry.metric_export_interval.is_zero()
        {
            return Err(
                ConfigError::Parse("telemetry.metric_export_interval must be greater than 0".to_string()).into(),
            );
        }

        if config.database.is_none() {
            return Err(ConfigError::NoDatabaseConfiguration.into());
        }

        config.validate().map_err(ConfigError::Validation)?;

        let network_config = config.network.resolve().ok_or_else(|| {
            BloklidError::NonSpecific(format!(
                "Network '{}' is not defined in hopr-bindings.\n\nSupported networks: {}",
                config.network,
                Network::all_names().join(", ")
            ))
        })?;

        let chain_config = ChainConfig {
            chain_id: network_config.chain_id,
            tx_polling_interval: config.network.tx_polling_interval(),
            confirmations: config.network.confirmations(),
            max_block_range: config.max_block_range,
            channel_contract_deploy_block: network_config.indexer_start_block_number,
            max_requests_per_sec: config.max_rpc_requests_per_sec,
            expected_block_time: config.network.expected_block_time(),
        };

        config.chain_network = Some(chain_config.clone());
        let mut contracts = blokli_chain_types::ContractAddresses {
            token: network_config.addresses.token.to_hopr_address(),
            channels: network_config.addresses.channels.to_hopr_address(),
            announcements: network_config.addresses.announcements.to_hopr_address(),
            module_implementation: network_config.addresses.module_implementation.to_hopr_address(),
            node_safe_migration: network_config.addresses.node_safe_migration.to_hopr_address(),
            node_safe_registry: network_config.addresses.node_safe_registry.to_hopr_address(),
            ticket_price_oracle: network_config.addresses.ticket_price_oracle.to_hopr_address(),
            winning_probability_oracle: network_config.addresses.winning_probability_oracle.to_hopr_address(),
            node_stake_factory: network_config.addresses.node_stake_factory.to_hopr_address(),
        };

        if let Some(override_contracts) = config.contracts_override {
            contracts = override_contracts;
        }

        config.contracts = contracts;

        tracing::info!(
            contract_addresses = ?config.contracts,
            "Resolved contract addresses",
        );

        config.validate().map_err(ConfigError::Validation)?;
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use std::{io::Write, time::Duration};

    use super::*;

    #[test]
    fn test_env_var_override() {
        let mut file = tempfile::Builder::new().suffix(".toml").tempfile().unwrap();
        writeln!(
            file,
            r#"
            network = "rotsee"
            rpc_url = "http://localhost:8545"
            [database]
            type = "postgresql"
            url = "postgres://file:5432/db"
        "#
        )
        .unwrap();
        let path = file.path().to_path_buf();

        temp_env::with_vars([("BLOKLI_DATABASE_URL", Some("postgres://env:5432/db"))], || {
            let args = Args {
                verbose: 0,
                config: Some(path),
                command: None,
            };

            let config = args.load_config(false).expect("Failed to load config");

            match config.database {
                Some(crate::config::DatabaseConfig::PostgreSql(c)) => {
                    assert_eq!(c.url.as_ref().map(|s| s.as_str()), Some("postgres://env:5432/db"));
                }
                _ => panic!("Expected PostgreSQL database config"),
            }
        });
    }

    #[test]
    fn test_canonical_env_var_override() {
        let mut file = tempfile::Builder::new().suffix(".toml").tempfile().unwrap();
        writeln!(
            file,
            r#"
            network = "rotsee"
            rpc_url = "http://localhost:8545"
            [database]
            type = "postgresql"
            url = "postgres://file:5432/db"
        "#
        )
        .unwrap();
        let path = file.path().to_path_buf();

        temp_env::with_var("DATABASE_URL", Some("postgres://canonical:5432/db"), || {
            let args = Args {
                verbose: 0,
                config: Some(path),
                command: None,
            };

            let config = args.load_config(false).expect("Failed to load config");

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
        let mut file = tempfile::Builder::new().suffix(".toml").tempfile().unwrap();
        writeln!(
            file,
            r#"
            network = "rotsee"
            rpc_url = "http://localhost:8545"
        "#
        )
        .unwrap();
        let path = file.path().to_path_buf();

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

                match config.database {
                    Some(crate::config::DatabaseConfig::PostgreSql(c)) => {
                        assert_eq!(
                            c.url.as_ref().map(|s| s.as_str()),
                            Some("postgresql://user:pass@localhost/db")
                        );
                        assert_eq!(c.max_connections, 10);
                    }
                    _ => panic!("Expected PostgreSQL database config"),
                }
            },
        );
    }

    #[test]
    fn test_missing_database_config_fails() {
        let mut file = tempfile::Builder::new().suffix(".toml").tempfile().unwrap();
        writeln!(
            file,
            r#"
            network = "rotsee"
            rpc_url = "http://localhost:8545"
        "#
        )
        .unwrap();
        let path = file.path().to_path_buf();

        temp_env::with_var("BLOKLI_DATABASE_TYPE", None::<&str>, || {
            let args = Args {
                verbose: 0,
                config: Some(path),
                command: None,
            };

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
            network = "rotsee"
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
            assert_eq!(config.network, Network::Rotsee, "String env var should override config");
        });
    }

    #[test]
    fn test_database_config_defaults_max_connections_to_10() {
        let mut file = tempfile::Builder::new().suffix(".toml").tempfile().unwrap();
        writeln!(
            file,
            r#"
            network = "rotsee"
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
        let mut file = tempfile::Builder::new().suffix(".toml").tempfile().unwrap();
        writeln!(
            file,
            r#"
            network = "rotsee"
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
        let mut file = tempfile::Builder::new().suffix(".toml").tempfile().unwrap();
        writeln!(
            file,
            r#"
            network = "rotsee"
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
        let mut file = tempfile::Builder::new().suffix(".toml").tempfile().unwrap();
        writeln!(
            file,
            r#"
            network = "rotsee"
            rpc_url = "http://localhost:8545"
            [database]
            type = "postgresql"
            url = "postgres://file:5432/db"
        "#
        )
        .unwrap();
        let path = file.path().to_path_buf();

        temp_env::with_var("BLOKLI_DATABASE_MAX_CONNECTIONS", Some("75"), || {
            let args = Args {
                verbose: 0,
                config: Some(path),
                command: None,
            };

            let config = args.load_config(false).expect("Failed to load config");

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
        let mut file = tempfile::Builder::new().suffix(".toml").tempfile().unwrap();
        writeln!(
            file,
            r#"
            network = "rotsee"
            rpc_url = "http://localhost:8545"
            [database]
            type = "sqlite"
            index_path = "data/index.db"
            logs_path = "data/logs.db"
        "#
        )
        .unwrap();
        let path = file.path().to_path_buf();

        temp_env::with_var("BLOKLI_DATABASE_MAX_CONNECTIONS", Some("40"), || {
            let args = Args {
                verbose: 0,
                config: Some(path),
                command: None,
            };

            let config = args.load_config(false).expect("Failed to load config");

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
            network = "rotsee"
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
            network = "rotsee"
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
            network = "rotsee"
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
            network = "rotsee"
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
            network = "rotsee"
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

    #[test]
    fn test_float_env_var_parsed_as_f64() {
        let mut file = tempfile::Builder::new().suffix(".toml").tempfile().unwrap();
        writeln!(
            file,
            r#"
            network = "rotsee"
            rpc_url = "http://localhost:8545"
            [database]
            type = "postgresql"
            url = "postgres://file:5432/db"
            [api]
            gas_multiplier = 1.0
        "#
        )
        .unwrap();
        let path = file.path().to_path_buf();

        temp_env::with_var("BLOKLI_API_GAS_MULTIPLIER", Some("1.5"), || {
            let args = Args {
                verbose: 0,
                config: Some(path),
                command: None,
            };

            let config = args.load_config(false).expect("Failed to load config");
            assert_eq!(
                config.api.gas_multiplier, 1.5,
                "BLOKLI_API_GAS_MULTIPLIER should be parsed as f64"
            );
        });
    }

    #[test]
    fn test_invalid_gas_multiplier_rejected() {
        let mut file = tempfile::Builder::new().suffix(".toml").tempfile().unwrap();
        writeln!(
            file,
            r#"
            network = "rotsee"
            rpc_url = "http://localhost:8545"
            [database]
            type = "postgresql"
            url = "postgres://file:5432/db"
            [api]
            gas_multiplier = 0.5
        "#
        )
        .unwrap();
        let path = file.path().to_path_buf();

        temp_env::with_var("BLOKLI_API_GAS_MULTIPLIER", None::<&str>, || {
            let args = Args {
                verbose: 0,
                config: Some(path),
                command: None,
            };
            let error = args
                .load_config(false)
                .expect_err("gas multiplier 0.0 should be invalid");
            assert!(
                error
                    .to_string()
                    .contains("api.gas_multiplier must be a finite number greater than or equal to 1"),
                "unexpected error: {error}"
            );
        });
    }

    #[test]
    fn test_max_block_range_default() {
        let mut file = tempfile::Builder::new().suffix(".toml").tempfile().unwrap();
        writeln!(
            file,
            r#"
            network = "rotsee"
            rpc_url = "http://localhost:8545"
            [database]
            type = "postgresql"
            url = "postgres://file:5432/db"
        "#
        )
        .unwrap();
        let path = file.path().to_path_buf();

        temp_env::with_var("BLOKLI_MAX_BLOCK_RANGE", None::<&str>, || {
            let args = Args {
                verbose: 0,
                config: Some(path),
                command: None,
            };

            let config = args.load_config(false).expect("Failed to load config");
            assert_eq!(config.max_block_range, 10000, "max_block_range should default to 10000");
        });
    }

    #[test]
    fn test_max_block_range_from_config_file() {
        let mut file = tempfile::Builder::new().suffix(".toml").tempfile().unwrap();
        writeln!(
            file,
            r#"
            network = "rotsee"
            rpc_url = "http://localhost:8545"
            max_block_range = 5000
            [database]
            type = "postgresql"
            url = "postgres://file:5432/db"
        "#
        )
        .unwrap();
        let path = file.path().to_path_buf();

        temp_env::with_var("BLOKLI_MAX_BLOCK_RANGE", None::<&str>, || {
            let args = Args {
                verbose: 0,
                config: Some(path),
                command: None,
            };

            let config = args.load_config(false).expect("Failed to load config");
            assert_eq!(
                config.max_block_range, 5000,
                "max_block_range should be parsed from config file"
            );
        });
    }

    #[test]
    fn test_max_block_range_env_override() {
        let mut file = tempfile::Builder::new().suffix(".toml").tempfile().unwrap();
        writeln!(
            file,
            r#"
            network = "rotsee"
            rpc_url = "http://localhost:8545"
            max_block_range = 5000
            [database]
            type = "postgresql"
            url = "postgres://file:5432/db"
        "#
        )
        .unwrap();
        let path = file.path().to_path_buf();

        temp_env::with_var("BLOKLI_MAX_BLOCK_RANGE", Some("2500"), || {
            let args = Args {
                verbose: 0,
                config: Some(path),
                command: None,
            };

            let config = args.load_config(false).expect("Failed to load config");
            assert_eq!(
                config.max_block_range, 2500,
                "BLOKLI_MAX_BLOCK_RANGE env var should override config file"
            );
        });
    }

    #[test]
    fn test_max_rpc_requests_per_sec_zero_from_config() {
        let mut file = tempfile::Builder::new().suffix(".toml").tempfile().unwrap();
        writeln!(
            file,
            r#"
            network = "rotsee"
            rpc_url = "http://localhost:8545"
            max_rpc_requests_per_sec = 0
            [database]
            type = "postgresql"
            url = "postgres://file:5432/db"
        "#
        )
        .unwrap();
        let path = file.path().to_path_buf();

        temp_env::with_var("BLOKLI_MAX_RPC_REQUESTS_PER_SEC", None::<&str>, || {
            let args = Args {
                verbose: 0,
                config: Some(path),
                command: None,
            };

            let config = args.load_config(false).expect("Failed to load config");
            assert_eq!(
                config.max_rpc_requests_per_sec,
                Some(0),
                "max_rpc_requests_per_sec should preserve explicit 0 from config"
            );
        });
    }

    #[test]
    fn test_max_rpc_requests_per_sec_env_override_zero() {
        let mut file = tempfile::Builder::new().suffix(".toml").tempfile().unwrap();
        writeln!(
            file,
            r#"
            network = "rotsee"
            rpc_url = "http://localhost:8545"
            max_rpc_requests_per_sec = 250
            [database]
            type = "postgresql"
            url = "postgres://file:5432/db"
        "#
        )
        .unwrap();
        let path = file.path().to_path_buf();

        temp_env::with_var("BLOKLI_MAX_RPC_REQUESTS_PER_SEC", Some("0"), || {
            let args = Args {
                verbose: 0,
                config: Some(path),
                command: None,
            };

            let config = args.load_config(false).expect("Failed to load config");
            assert_eq!(
                config.max_rpc_requests_per_sec,
                Some(0),
                "BLOKLI_MAX_RPC_REQUESTS_PER_SEC should override config file value"
            );
        });
    }

    #[test]
    fn test_telemetry_env_overrides() {
        let mut file = tempfile::Builder::new().suffix(".toml").tempfile().unwrap();
        writeln!(
            file,
            r#"
            network = "rotsee"
            rpc_url = "http://localhost:8545"
            [database]
            type = "postgresql"
            url = "postgres://file:5432/db"
        "#
        )
        .unwrap();
        let path = file.path().to_path_buf();

        temp_env::with_vars(
            [
                ("BLOKLI_OTLP_ENDPOINT", Some("http://localhost:4317")),
                ("BLOKLI_OTLP_SIGNALS", Some("metrics,traces,logs")),
                ("BLOKLI_METRIC_EXPORT_INTERVAL", Some("15s")),
            ],
            || {
                let args = Args {
                    verbose: 0,
                    config: Some(path),
                    command: None,
                };

                let config = args.load_config(false).expect("Failed to load config");
                assert_eq!(config.telemetry.otlp_endpoint.as_deref(), Some("http://localhost:4317"));
                assert_eq!(config.telemetry.otlp_signals, "metrics,traces,logs");
                assert_eq!(config.telemetry.metric_export_interval, Duration::from_secs(15));
            },
        );
    }
}
