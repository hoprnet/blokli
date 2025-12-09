use std::{path::PathBuf, time::Duration};

use anyhow::{Context, Result};
use clap::{Parser, builder::TypedValueParser};

const DEFAULT_TEST_IMAGE: &str = "blokli-integration:local";
const DEFAULT_INTEGRATION_CONFIG: &str = "config-integration-anvil.toml";
const DEFAULT_BLOKLID_URL: &str = "http://127.0.0.1:8080";
const DEFAULT_RPC_URL: &str = "http://127.0.0.1:8545";
const DEFAULT_REMOTE_IMAGE: &str = "europe-west3-docker.pkg.dev/hoprassociation/docker-images/bloklid:latest";

#[derive(Parser, Debug, Clone)]
#[command(author = "BLOKLI", version = None, disable_help_flag = true)]
pub struct TestConfig {
    #[arg(skip = PathBuf::new())]
    pub project_root: PathBuf,

    #[arg(skip = PathBuf::new())]
    pub integration_dir: PathBuf,

    #[arg(long, env = "BLOKLI_TEST_IMAGE", default_value = DEFAULT_TEST_IMAGE)]
    pub bloklid_image: String,

    #[arg(long, env = "BLOKLI_TEST_CONFIG", default_value = DEFAULT_INTEGRATION_CONFIG)]
    pub integration_config: String,

    #[arg(skip = String::new())]
    pub integration_config_name: String,

    #[arg(long, env = "BLOKLI_TEST_REMOTE_IMAGE", default_value = DEFAULT_REMOTE_IMAGE)]
    pub remote_image: String,

    #[arg(long, env = "BLOKLI_TEST_BLOKLID_URL", default_value = DEFAULT_BLOKLID_URL)]
    pub bloklid_url: String,

    #[arg(long, env = "BLOKLI_TEST_RPC_URL", default_value = DEFAULT_RPC_URL)]
    pub rpc_url: String,

    #[arg(
        long,
        env = "BLOKLI_TEST_HTTP_TIMEOUT_SECS",
        default_value = "30",
        value_parser = clap::value_parser!(u64).map(Duration::from_secs)
    )]
    pub http_timeout: Duration,

    #[arg(
        long,
        env = "BLOKLI_TEST_READY_TIMEOUT_SECS",
        default_value = "180",
        value_parser = clap::value_parser!(u64).map(Duration::from_secs)
    )]
    pub ready_timeout: Duration,

    #[arg(
        long,
        env = "BLOKLI_TEST_READY_POLL_SECS",
        default_value = "5",
        value_parser = clap::value_parser!(u64).map(Duration::from_secs)
    )]
    pub ready_poll_interval: Duration,

    #[arg(
        long,
        env = "BLOKLI_TEST_RECEIPT_TIMEOUT_SECS",
        default_value = "120",
        value_parser = clap::value_parser!(u64).map(Duration::from_secs)
    )]
    pub receipt_timeout: Duration,

    #[arg(
        long,
        env = "BLOKLI_TEST_RECEIPT_POLL_SECS",
        default_value = "3",
        value_parser = clap::value_parser!(u64).map(Duration::from_secs)
    )]
    pub receipt_poll_interval: Duration,

    #[arg(long, env = "BLOKLI_TEST_TX_VALUE_WEI", default_value_t = 1_000_000_000_000_000u128)]
    pub tx_value_wei: u128,

    #[arg(long, env = "BLOKLI_TEST_GAS_PRICE", default_value_t = 1_000_000_000u128)]
    pub gas_price: u128,

    #[arg(long, env = "BLOKLI_TEST_GAS_LIMIT", default_value_t = 21_000)]
    pub gas_limit: u64,

    #[arg(long, env = "BLOKLI_TEST_CONFIRMATIONS", default_value_t = 1)]
    pub tx_confirmations: u32,

    #[arg(long, env = "BLOKLI_TEST_REGISTRY_PORT", default_value_t = 5001)]
    pub registry_port: u16,
}

impl Default for TestConfig {
    fn default() -> Self {
        let mut cfg = TestConfig {
            project_root: PathBuf::new(),
            integration_dir: PathBuf::new(),
            bloklid_image: DEFAULT_TEST_IMAGE.to_string(),
            integration_config: DEFAULT_INTEGRATION_CONFIG.to_string(),
            integration_config_name: String::new(),
            remote_image: DEFAULT_REMOTE_IMAGE.to_string(),
            bloklid_url: DEFAULT_BLOKLID_URL.to_string(),
            rpc_url: DEFAULT_RPC_URL.to_string(),
            http_timeout: Duration::from_secs(30),
            ready_timeout: Duration::from_secs(180),
            ready_poll_interval: Duration::from_secs(5),
            receipt_timeout: Duration::from_secs(120),
            receipt_poll_interval: Duration::from_secs(3),
            tx_value_wei: 1_000_000_000_000_000,
            gas_price: 1_000_000_000,
            gas_limit: 21_000,
            tx_confirmations: 1,
            registry_port: 5001,
        };
        cfg.finalize().expect("configuration defaults valid");
        cfg
    }
}

impl TestConfig {
    pub fn load() -> Result<Self> {
        let mut cfg = TestConfig::parse_from(["blokli-integration-config"]);
        cfg.finalize()?;
        Ok(cfg)
    }

    fn finalize(&mut self) -> Result<()> {
        let (project_root, integration_dir) = resolve_paths()?;
        self.project_root = project_root;
        self.integration_dir = integration_dir;
        self.integration_config_name = PathBuf::from(&self.integration_config)
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_else(|| "config-integration".to_string());
        Ok(())
    }
}

fn resolve_paths() -> Result<(PathBuf, PathBuf)> {
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let tests_dir = crate_dir
        .parent()
        .context("Failed to resolve tests directory")?
        .to_path_buf();
    let project_root = tests_dir
        .parent()
        .context("Failed to resolve workspace root")?
        .to_path_buf();
    Ok((project_root.clone(), project_root.join("tests/integration")))
}
