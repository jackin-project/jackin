//! Non-TUI instance discovery services.

use anyhow::Context;

/// Return running role container names from the local Docker CLI.
pub fn running_role_containers() -> anyhow::Result<Vec<String>> {
    let output = std::process::Command::new("docker")
        .args([
            "ps",
            "--filter",
            "label=jackin.kind=role",
            "--format",
            "{{.Names}}",
        ])
        .output()
        .map_err(anyhow::Error::new)
        .context("starting docker ps for live instance reconciliation")?;
    anyhow::ensure!(
        output.status.success(),
        "docker ps exited with status {:?}: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect())
}
