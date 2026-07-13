// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Sensitive mount detection and operator confirmation prompt.
//!
//! Pure detection helpers (`SensitiveMount`, `find_sensitive_mounts`) are
//! implemented in `jackin-config` and re-exported here. `confirm_sensitive_mounts`
//! lives here because it depends on `crate::tui` and `dialoguer`.

pub use jackin_config::{SensitiveMount, find_sensitive_mounts};

#[cfg(test)]
use crate::workspace::MountConfig;

/// Display a warning for sensitive mounts and ask the operator to confirm.
/// Returns `Ok(true)` when the operator confirms, `Ok(false)` when they
/// decline, and `Err` on I/O errors.
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
