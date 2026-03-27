use std::{path::PathBuf, time::Duration};

use anyhow::{Context, Result};
use clap::{Parser, builder::TypedValueParser};
use url::Url;

const DEFAULT_INTEGRATION_CONFIG: &str = "config-integration-anvil.toml";
const DEFAULT_TEST_IMAGE: &str = "bloklid:integration-test";

/// Base ports for integration test stacks. Each stack offsets from these
/// using a deterministic value derived from the process ID.
const BASE_BLOKLID_PORT: u16 = 18081;
const BASE_ANVIL_PORT: u16 = 18546;
const BASE_REGISTRY_PORT: u16 = 15001;

/// Generates a short stack identifier from the process ID.
/// Each test binary runs as a separate process, so the PID gives natural uniqueness.
fn default_stack_id() -> String {
    format!("{:04x}", std::process::id() % 0xFFFF)
}

/// Computes a deterministic port offset (0..255) from a stack ID string.
fn port_offset(stack_id: &str) -> u16 {
    let hash: u16 = stack_id.bytes().fold(0u16, |acc, b| acc.wrapping_add(b as u16));
    hash % 256
}

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

    #[arg(long, env = "BLOKLI_TEST_REMOTE_IMAGE")]
    pub remote_image: Option<String>,

    #[arg(long, env = "BLOKLI_TEST_BLOKLID_URL")]
    pub bloklid_url: Option<Url>,

    #[arg(long, env = "BLOKLI_TEST_RPC_URL")]
    pub rpc_url: Option<Url>,

    #[arg(
        long,
        env = "BLOKLI_TEST_HTTP_TIMEOUT_SECS",
        default_value = "30",
        value_parser = clap::value_parser!(u64).map(Duration::from_secs)
    )]
    pub http_timeout: Duration,

    #[arg(long, env = "BLOKLI_TEST_CONFIRMATIONS", default_value_t = 1)]
    pub tx_confirmations: usize,

    #[arg(long, env = "BLOKLI_TEST_REGISTRY_PORT")]
    pub registry_port: Option<u16>,

    #[arg(long, env = "BLOKLI_TEST_STACK_ID", default_value_t = default_stack_id())]
    pub stack_id: String,
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

        let offset = port_offset(&self.stack_id);

        if self.bloklid_url.is_none() {
            self.bloklid_url =
                Some(Url::parse(&format!("http://localhost:{}", self.bloklid_port(offset))).unwrap());
        }
        if self.rpc_url.is_none() {
            self.rpc_url = Some(Url::parse(&format!("http://localhost:{}", self.anvil_port(offset))).unwrap());
        }
        if self.registry_port.is_none() {
            self.registry_port = Some(BASE_REGISTRY_PORT + offset);
        }

        Ok(())
    }

    pub fn bloklid_url(&self) -> &Url {
        self.bloklid_url.as_ref().expect("bloklid_url not initialized")
    }

    pub fn rpc_url(&self) -> &Url {
        self.rpc_url.as_ref().expect("rpc_url not initialized")
    }

    pub fn registry_port(&self) -> u16 {
        self.registry_port.expect("registry_port not initialized")
    }

    pub fn bloklid_port(&self, offset: u16) -> u16 {
        BASE_BLOKLID_PORT + offset
    }

    pub fn anvil_port(&self, offset: u16) -> u16 {
        BASE_ANVIL_PORT + offset
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
