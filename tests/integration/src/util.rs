use std::process::{Command, Stdio};

use anyhow::{Context, Result};

pub fn run_command(mut command: Command, silenced: bool, description: &str) -> Result<()> {
    if silenced {
        command.stdout(Stdio::null()).stderr(Stdio::null());
    }

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

pub fn build_command(cmd: &str, args: &[&str]) -> Command {
    let mut command = Command::new(cmd);
    for arg in args {
        command.arg(arg);
    }
    command
}
