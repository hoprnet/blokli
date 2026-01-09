use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
};

use anyhow::{Result, bail};
use chrono::{DateTime, Utc};
use tracing::{info, warn};

use crate::{
    anvil::AnvilAccount,
    config::TestConfig,
    util::{build_command, capture_command, run_command},
};

pub struct DockerEnvironment {
    config: Arc<TestConfig>,
    running: bool,
}

impl DockerEnvironment {
    pub fn new(config: Arc<TestConfig>) -> Self {
        Self { config, running: false }
    }

    pub fn ensure_image_available(&self) -> Result<()> {
        if self.try_load_local_image()? {
            return Ok(());
        }
        if let Some(remote) = &self.config.remote_image {
            return self.pull_remote_image(remote);
        }
        bail!(
            "No local bloklid Docker image found at {} and BLOKLI_TEST_REMOTE_IMAGE is not set. Please run `nix build \
             .#bloklid-docker-${{arch}}` before running integration tests.",
            self.config.project_root.join("result").display()
        );
    }

    fn try_load_local_image(&self) -> Result<bool> {
        let result_path = self.config.project_root.join("result");
        if !result_path.exists() {
            return Ok(false);
        }

        info!(
            path = %result_path.display(),
            "loading bloklid Docker image from local nix build result"
        );
        match self.load_image_from_path(&result_path) {
            Ok(source_image) => {
                self.tag_image(&source_image)?;
                info!(
                    image = %source_image,
                    target = %self.config.bloklid_image,
                    "loaded bloklid Docker image from local build"
                );
                Ok(true)
            }
            Err(err) => {
                warn!(
                    error = ?err,
                    path = %result_path.display(),
                    "failed to load local Docker image, falling back to remote image"
                );
                Ok(false)
            }
        }
    }

    fn load_image_from_path(&self, archive_path: &Path) -> Result<String> {
        let mut cmd = Command::new("docker");
        cmd.arg("load").arg("--input").arg(archive_path);
        let output = capture_command(cmd, "docker load bloklid image from local result")?;

        if let Some(image) = output.lines().rev().find_map(|line| {
            line.trim()
                .strip_prefix("Loaded image: ")
                .map(|value| value.to_string())
        }) {
            Ok(image)
        } else {
            bail!(
                "Unable to determine loaded Docker image from docker load output: {}",
                output
            );
        }
    }

    fn pull_remote_image(&self, remote: &str) -> Result<()> {
        info!(remote = %remote, "pulling bloklid Docker image from registry");

        let cmd = build_command("docker", &["pull", "--platform", "linux/amd64", remote]);
        run_command(cmd, true, "docker pull bloklid image")?;
        self.tag_image(remote)
    }

    fn tag_image(&self, source_image: &str) -> Result<()> {
        let mut cmd = Command::new("docker");
        cmd.arg("tag").arg(source_image).arg(&self.config.bloklid_image);
        run_command(cmd, true, "docker tag bloklid image")?;
        Ok(())
    }

    pub fn compose_up(&mut self) -> Result<()> {
        info!("starting docker-compose stack for blokli integration tests");
        let mut cmd = self.compose_command();
        cmd.arg("up").arg("-d");

        run_command(cmd, true, "docker compose up")?;
        self.running = true;
        Ok(())
    }

    pub fn compose_down(&mut self) -> Result<()> {
        if !self.running {
            return Ok(());
        }

        info!("stopping docker-compose stack");
        let mut cmd = self.compose_command();
        cmd.arg("down").arg("-v").arg("--remove-orphans");

        run_command(cmd, true, "docker compose down")?;
        self.running = false;
        Ok(())
    }

    pub fn collect_logs(&self, name: &str, timestamp: DateTime<Utc>) -> Result<PathBuf> {
        if !self.running {
            bail!("Docker stack not running");
        }
        info!(name, "collecting container logs");

        let command = build_command("docker", &["logs", &format!("blokli-integration-{}", name)]);
        let logs = capture_command(command, &format!("docker logs blokli-integration-{}", name))?;
        let timestamp = timestamp.format("%Y%m%d_%H%M%S");
        let filename = format!("blokli-integration/{}/{}.log", timestamp, name);
        let log_path = PathBuf::from("/tmp").join(filename);

        fs::create_dir_all(log_path.parent().unwrap())?;
        fs::write(&log_path, logs)?;
        info!(path = %log_path.display(), name, "saved logs");
        Ok(log_path)
    }

    fn compose_command(&self) -> Command {
        let mut cmd = build_command("docker", &["compose", "-f", "docker-compose.yml"]);

        cmd.current_dir(&self.config.integration_dir);
        cmd.env("BLOKLID_IMAGE", &self.config.bloklid_image);
        cmd.env("INTEGRATION_CONFIG", &self.config.integration_config);
        cmd.env("REGISTRY_PORT", self.config.registry_port.to_string());
        cmd
    }

    pub fn fetch_anvil_accounts(&self) -> Result<Vec<AnvilAccount>> {
        let cmd = build_command("docker", &["logs", "blokli-integration-anvil"]);
        let logs = capture_command(cmd, "docker logs blokli-integration-anvil")?;
        parse_anvil_accounts(&logs)
    }
}

impl Drop for DockerEnvironment {
    fn drop(&mut self) {
        let timestamp = Utc::now();
        if self.running {
            if let Err(err) = self.collect_logs("bloklid", timestamp) {
                warn!(error = ?err, "failed to collect bloklid logs");
            }
            if let Err(err) = self.collect_logs("anvil", timestamp) {
                warn!(error = ?err, "failed to collect anvil logs");
            }
            if let Err(err) = self.collect_logs("registry", timestamp) {
                warn!(error = ?err, "failed to collect registry logs");
            }
            if let Err(err) = self.compose_down() {
                warn!(error = ?err, "failed to stop docker-compose stack");
            }
        }
    }
}

fn parse_anvil_accounts(logs: &str) -> Result<Vec<AnvilAccount>> {
    let addresses = extract_section_values(logs, "Available Accounts");
    let keys = extract_section_values(logs, "Private Keys");

    if addresses.is_empty() {
        bail!("Failed to parse Anvil addresses from logs");
    }
    if addresses.len() != keys.len() {
        bail!("Mismatch between addresses and private keys in Anvil logs");
    }

    let accounts: Vec<AnvilAccount> = addresses
        .into_iter()
        .zip(keys)
        .map(|(address, private_key)| AnvilAccount::new(private_key, address))
        .collect();

    if accounts.is_empty() {
        bail!("Failed to parse Anvil private keys from logs");
    }

    Ok(accounts)
}

fn extract_section_values(logs: &str, marker: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut in_section = false;

    for line in logs.lines() {
        let clean_line = strip_ansi_codes(line).trim().to_string();
        if clean_line.is_empty() {
            if in_section && !result.is_empty() {
                break;
            }
            continue;
        }

        if clean_line.contains(marker) {
            in_section = true;
            continue;
        }

        if in_section && clean_line.starts_with('(') {
            if let Some(pos) = clean_line.find("0x") {
                let value = clean_line[pos..]
                    .split_whitespace()
                    .next()
                    .unwrap_or_default()
                    .to_string();
                result.push(value);
            }
        } else if in_section && clean_line.starts_with("===") {
            continue;
        }
    }

    result
}

fn strip_ansi_codes(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            for next in chars.by_ref() {
                if next == 'm' {
                    break;
                }
            }
        } else {
            output.push(ch);
        }
    }

    output
}
