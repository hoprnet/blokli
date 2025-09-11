mod config;
mod errors;

use async_signal::{Signal, Signals};
use clap::Parser;
use futures::TryStreamExt;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tracing::{info, warn};
use tracing_subscriber::{EnvFilter, fmt};
use validator::Validate;

use crate::config::Config;
use crate::errors::{BloklidError, ConfigError};

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
        .map_err(|e| ConfigError::ParseError(e.to_string()))?;

        config
            .validate()
            .map_err(|e| ConfigError::ValidationError(e))?;
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
        config = args
            .config
            .as_ref()
            .map(|p| p.display().to_string())
            .as_deref(),
        "bloklid starting"
    );

    // Initial config load
    let config = Arc::new(RwLock::new(args.load_config(true)?));

    info!("daemon running; send SIGHUP to reload config, SIGINT/SIGTERM to stop");

    let mut signals = Signals::new([Signal::Hup, Signal::Int, Signal::Term])?;
    while let Some(signal) = signals.try_next().await? {
        match signal {
            Signal::Hup => {
                info!("received SIGHUP; reloading configuration");
                let new_cfg = args.load_config(false)?;
                {
                    let mut cfg_guard = config.write().map_err(|_| {
                        BloklidError::NonSpecificError("failed to lock config".into())
                    })?;
                    *cfg_guard = new_cfg;
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

    info!("bloklid stopped gracefully");
    Ok(())
}
