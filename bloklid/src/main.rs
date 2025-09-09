use clap::Parser;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::RwLock;
use tokio::time::Duration;
use tracing::{debug, info, warn};
use tracing_subscriber::{fmt, EnvFilter};

/// Bloklid: HOPR on-chain indexer and executor
///
/// This binary is prepared to accept CLI arguments via `clap` and
/// uses the Tokio async runtime by default.
#[derive(Debug, Parser)]
#[command(name = "bloklid", about = "Daemon for indexing HOPR on-chain events and executing HOPR-related on-chain transactions")] 
struct Args {
    /// Increase output verbosity (-v, -vv)
    #[arg(short = 'v', long = "verbose", action = clap::ArgAction::Count)]
    verbose: u8,

    /// Optional path to a configuration file
    #[arg(short = 'c', long = "config", value_name = "FILE")]
    config: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    // Example configurable fields parsed from YAML
    pub http_port: Option<u16>,
    pub log_level: Option<String>,

    // Runtime-managed fields (not from YAML)
    #[serde(skip)]
    version: u64,
    #[serde(skip)]
    source: Option<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            http_port: None,
            log_level: None,
            version: 0,
            source: None,
        }
    }
}

async fn load_config(path: &Option<PathBuf>, previous_version: u64) -> Config {
    match path {
        Some(p) => {
            match tokio::fs::read_to_string(p).await {
                Ok(contents) => match serde_yaml::from_str::<Config>(&contents) {
                    Ok(mut cfg) => {
                        cfg.version = previous_version + 1;
                        cfg.source = Some(p.clone());
                        cfg
                    }
                    Err(e) => {
                        warn!(error = %e, path = %p.display(), "failed parsing YAML config; keeping previous version incremented");
                        let mut cfg = Config::default();
                        cfg.version = previous_version + 1;
                        cfg.source = Some(p.clone());
                        cfg
                    }
                },
                Err(e) => {
                    warn!(error = %e, path = %p.display(), "failed reading config; keeping previous version incremented");
                    let mut cfg = Config::default();
                    cfg.version = previous_version + 1;
                    cfg.source = Some(p.clone());
                    cfg
                }
            }
        }
        None => {
            let mut cfg = Config::default();
            cfg.version = previous_version + 1;
            cfg.source = None;
            cfg
        }
    }
}

#[tokio::main]
async fn main() {
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
    let initial_cfg = load_config(&args.config, 0).await;
    info!(version = initial_cfg.version, source = ?initial_cfg.source, "configuration loaded");
    let config = Arc::new(RwLock::new(initial_cfg));

    // Set up signal handlers
    let mut sigint = signal(SignalKind::interrupt()).expect("failed to register SIGINT handler");
    let mut sigterm = signal(SignalKind::terminate()).expect("failed to register SIGTERM handler");
    let mut sighup = signal(SignalKind::hangup()).expect("failed to register SIGHUP handler");

    // Example background loop to keep the daemon alive
    // In a real implementation, start tasks and pass config handle
    let mut ticker = tokio::time::interval(Duration::from_secs(10));

    info!("daemon running; send SIGHUP to reload config, SIGINT/SIGTERM to stop");

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                let cfg = config.read().await;
                debug!(version = cfg.version, source = ?cfg.source, "heartbeat");
            }
            _ = sigint.recv() => {
                info!("received SIGINT; shutting down");
                break;
            }
            _ = sigterm.recv() => {
                info!("received SIGTERM; shutting down");
                break;
            }
            _ = sighup.recv() => {
                info!("received SIGHUP; reloading configuration");
                let prev_version = { config.read().await.version };
                let new_cfg = load_config(&args.config, prev_version).await;
                {
                    let mut cfg_guard = config.write().await;
                    *cfg_guard = new_cfg;
                }
                info!(version = config.read().await.version, source = ?config.read().await.source, "configuration reloaded");
            }
        }
    }

    info!("bloklid stopped");
}
