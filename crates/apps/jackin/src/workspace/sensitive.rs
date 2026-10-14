// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

pub use jackin_config::{
    MountConfig, SensitiveMount, find_sensitive_mounts, normalize_sensitive_mount_sources,
};

#[must_use]
pub fn resolved_sensitive_mounts(mounts: &[MountConfig]) -> Vec<SensitiveMount> {
    let resolved =
        normalize_sensitive_mount_sources(mounts, |path| std::fs::canonicalize(path).ok());
    find_sensitive_mounts(&resolved)
}

pub fn confirm_sensitive_mounts(sensitive: &[SensitiveMount]) -> anyhow::Result<bool> {
    use owo_colors::OwoColorize;

    if sensitive.is_empty() {
        return Ok(true);
    }

    crate::prompt::require_interactive_stdin(
        "sensitive mount paths detected but stdin is not a terminal — cannot prompt for confirmation",
    )?;

    eprintln!(
        "\n{}",
        "⚠  Sensitive host paths detected in mounts:"
            .yellow()
            .bold()
    );
    for hit in sensitive {
        eprintln!("     {} — {}", hit.src.bold(), hit.reason);
    }
    eprintln!(
        "   {}",
        "These paths may expose credentials to the role container.".dimmed()
    );
    eprintln!();

    Ok(dialoguer::Confirm::new()
        .with_prompt("Continue with these mounts?")
        .default(false)
        .interact()?)
}

#[cfg(test)]
mod tests;
