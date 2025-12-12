use std::{path::PathBuf, time::Duration};

use anyhow::{Context, Result};
use clap::{Parser, builder::TypedValueParser};
use url::Url;

const DEFAULT_INTEGRATION_CONFIG: &str = "config-integration-anvil.toml";
const DEFAULT_BLOKLID_URL: &str = "http://localhost:8080";
const DEFAULT_REMOTE_IMAGE: &str = "europe-west3-docker.pkg.dev/hoprassociation/docker-images/bloklid:latest";
const DEFAULT_RPC_URL: &str = "http://localhost:8545";
const DEFAULT_TEST_IMAGE: &str = "bloklid:integration-test";

#[derive(Parser, Debug, Clone)]
pub struct TestConfig {
    #[arg(skip = PathBuf::new())]
    pub project_root: PathBuf,

    #[arg(skip = PathBuf::new())]
    pub integration_dir: PathBuf,

    #[arg(long, env = "BLOKLI_TEST_IMAGE", default_value = DEFAULT_TEST_IMAGE)]
    pub bloklid_image: String,

    #[arg(long, env = "BLOKLI_TEST_CONFIG", default_value = DEFAULT_INTEGRATION_CONFIG)]
    pub integration_config: String,

    #[arg(long, env = "BLOKLI_TEST_REMOTE_IMAGE", default_value = DEFAULT_REMOTE_IMAGE)]
    pub remote_image: String,

    #[arg(
        long,
        env = "BLOKLI_TEST_BLOKLID_URL",
        default_value = DEFAULT_BLOKLID_URL,
        value_parser = clap::value_parser!(Url)
    )]
    pub bloklid_url: Url,

    #[arg(
        long,
        env = "BLOKLI_TEST_RPC_URL",
        default_value = DEFAULT_RPC_URL,
        value_parser = clap::value_parser!(Url)
    )]
    pub rpc_url: Url,

    #[arg(
        long,
        env = "BLOKLI_TEST_HTTP_TIMEOUT_SECS",
        default_value = "30",
        value_parser = clap::value_parser!(u64).map(Duration::from_secs)
    )]
    pub http_timeout: Duration,

    #[arg(long, env = "BLOKLI_TEST_MAX_FEE_PER_GAS", default_value_t = 2_000_000_000u128)]
    pub max_fee_per_gas: u128,

    #[arg(
        long,
        env = "BLOKLI_TEST_MAX_PRIORITY_FEE_PER_GAS",
        default_value_t = 1_000_000_000u128
    )]
    pub max_priority_fee_per_gas: u128,

    #[arg(long, env = "BLOKLI_TEST_GAS_LIMIT", default_value_t = 21_000)]
    pub gas_limit: u64,

    #[arg(long, env = "BLOKLI_TEST_CONFIRMATIONS", default_value_t = 1)]
    pub tx_confirmations: u32,

    #[arg(long, env = "BLOKLI_TEST_REGISTRY_PORT", default_value_t = 5001)]
    pub registry_port: u16,
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
