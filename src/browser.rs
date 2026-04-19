use std::process::Command;

use anyhow::{Context, Result, bail};

pub fn open_target(url: &str, custom_command: Option<&str>) -> Result<()> {
    if let Some(command) = custom_command.and_then(normalize_command) {
        let shell_command = if command.contains("{url}") {
            command.replace("{url}", url)
        } else {
            format!("{command} {url}")
        };
        let status = Command::new("sh")
            .arg("-c")
            .arg(shell_command)
            .status()
            .context("failed to spawn configured browser command")?;
        if !status.success() {
            bail!("configured browser command exited with {status}");
        }
        return Ok(());
    }

    open::that_detached(url).context("failed to open URL in browser")
}

fn normalize_command(command: &str) -> Option<&str> {
    let trimmed = command.trim();
    (!trimmed.is_empty()).then_some(trimmed)
}
