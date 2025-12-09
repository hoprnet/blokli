use std::{fs, path::PathBuf, process::Command, sync::Arc};

use anyhow::{Result, bail};
use chrono::Utc;
use tracing::{info, warn};

use crate::{
    TestConfig,
    util::{capture_command, run_command},
};

pub struct AnvilAccounts {
    pub sender_private_key: String,
    pub sender_address: String,
    pub recipient_address: String,
}

pub struct DockerEnvironment {
    config: Arc<TestConfig>,
    running: bool,
}

impl DockerEnvironment {
    pub fn new(config: Arc<TestConfig>) -> Self {
        Self { config, running: false }
    }

    pub fn ensure_image_available(&self) -> Result<()> {
        let remote = &self.config.remote_image;
        info!(remote = %remote, "pulling bloklid Docker image from registry");

        let mut cmd = Command::new("docker");
        cmd.arg("pull").arg("--platform").arg("linux/amd64").arg(remote);
        run_command(cmd, "docker pull bloklid image")?;

        let mut cmd = Command::new("docker");
        cmd.arg("tag").arg(remote).arg(&self.config.bloklid_image);
        run_command(cmd, "docker tag bloklid remote image")?;

        Ok(())
    }

    pub fn compose_up(&mut self) -> Result<()> {
        info!("starting docker-compose stack for blokli integration tests");
        let mut cmd = self.compose_command();
        cmd.arg("up").arg("-d");
        run_command(cmd, "docker compose up")?;
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
        run_command(cmd, "docker compose down")?;
        self.running = false;
        Ok(())
    }

    pub fn collect_logs(&self) -> Result<PathBuf> {
        if !self.running {
            anyhow::bail!("Docker stack not running");
        }
        info!("collecting bloklid container logs");

        let mut command = Command::new("docker");
        command.arg("logs").arg("blokli-integration-bloklid");

        let logs = capture_command(command, "docker logs blokli-integration-bloklid")?;
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        let filename = format!(
            "blokli-integration-{}-{}.log",
            self.config.integration_config_name, timestamp
        );
        let log_path = PathBuf::from("/tmp").join(filename);
        fs::write(&log_path, logs)?;
        info!(path = %log_path.display(), "saved bloklid logs");
        Ok(log_path)
    }

    fn compose_command(&self) -> Command {
        let mut cmd = Command::new("docker");
        cmd.current_dir(&self.config.integration_dir)
            .arg("compose")
            .arg("-f")
            .arg("docker-compose.yml");
        cmd.env("BLOKLID_IMAGE", &self.config.bloklid_image);
        cmd.env("INTEGRATION_CONFIG", &self.config.integration_config);
        cmd.env("REGISTRY_PORT", self.config.registry_port.to_string());
        cmd
    }

    pub fn fetch_anvil_accounts(&self) -> Result<AnvilAccounts> {
        let mut cmd = Command::new("docker");
        cmd.arg("logs").arg("blokli-integration-anvil");
        let logs = capture_command(cmd, "docker logs blokli-integration-anvil")?;
        parse_anvil_accounts(&logs)
    }
}

impl Drop for DockerEnvironment {
    fn drop(&mut self) {
        if self.running {
            if let Err(err) = self.collect_logs() {
                warn!(error = ?err, "failed to collect bloklid logs");
            }
            if let Err(err) = self.compose_down() {
                warn!(error = ?err, "failed to stop docker-compose stack");
            }
        }
    }
}

fn parse_anvil_accounts(logs: &str) -> Result<AnvilAccounts> {
    let addresses = extract_section_values(logs, "Available Accounts");
    let keys = extract_section_values(logs, "Private Keys");

    if addresses.len() < 2 {
        bail!("Failed to parse Anvil addresses from logs");
    }
    if keys.is_empty() {
        bail!("Failed to parse Anvil private keys from logs");
    }

    Ok(AnvilAccounts {
        sender_private_key: keys[0].clone(),
        sender_address: addresses[0].clone(),
        recipient_address: addresses[1].clone(),
    })
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
