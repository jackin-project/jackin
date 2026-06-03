//! Detect sensitive host paths (`~/.ssh`, `~/.aws`, etc.) in mount sources; prompt confirmation.
//!
//! Pure classification against a static suffix table — no filesystem access
//! or operator I/O. Callers own the prompt and the decision to abort or
//! proceed.

use crate::workspace::MountConfig;

/// Path suffixes that indicate sensitive host directories. A mount source is
/// considered sensitive when its resolved path ends with one of these suffixes
/// (after tilde expansion).
const SENSITIVE_SUFFIXES: &[(&str, &str)] = &[
    ("/.ssh", "SSH keys and configuration"),
    ("/.aws", "AWS credentials and configuration"),
    ("/.gnupg", "GPG keys and trust database"),
    ("/.config/gcloud", "Google Cloud credentials"),
    ("/.kube", "Kubernetes credentials and configuration"),
    ("/.docker", "Docker credentials and configuration"),
];

/// A mount source that matched a sensitive path pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SensitiveMount {
    pub src: String,
    pub reason: String,
}

/// Return any mounts whose source path matches a known sensitive pattern.
pub fn find_sensitive_mounts(mounts: &[MountConfig]) -> Vec<SensitiveMount> {
    let mut hits = Vec::new();
    for mount in mounts {
        let normalized = mount.src.trim_end_matches('/');
        for &(suffix, reason) in SENSITIVE_SUFFIXES {
            if normalized.ends_with(suffix) || normalized == suffix.trim_start_matches('/') {
                hits.push(SensitiveMount {
                    src: mount.src.clone(),
                    reason: reason.to_string(),
                });
                break;
            }
        }
    }
    hits
}

/// Display a warning for sensitive mounts and ask the operator to confirm.
/// Returns `Ok(true)` when the operator confirms, `Ok(false)` when they
/// decline, and `Err` on I/O errors.
pub fn confirm_sensitive_mounts(sensitive: &[SensitiveMount]) -> anyhow::Result<bool> {
    use owo_colors::OwoColorize;

    if sensitive.is_empty() {
        return Ok(true);
    }

    crate::tui::require_interactive_stdin(
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
