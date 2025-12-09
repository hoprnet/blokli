use std::process::Command;

use anyhow::{Context, Result};

pub fn run_command(mut command: Command, description: &str) -> Result<()> {
    let status = command
        .status()
        .with_context(|| format!("Failed to run {description}"))?;

    if !status.success() {
        anyhow::bail!("`{description}` exited with status {status}");
    }
    Ok(())
}

pub fn capture_command(mut command: Command, description: &str) -> Result<String> {
    let output = command
        .output()
        .with_context(|| format!("Failed to run {description}"))?;

    if !output.status.success() {
        anyhow::bail!(
            "{description} exited with status {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
