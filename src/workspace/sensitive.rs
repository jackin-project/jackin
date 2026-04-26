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
        "These paths may expose credentials to the agent container.".dimmed()
    );
    eprintln!();

    Ok(dialoguer::Confirm::new()
        .with_prompt("Continue with these mounts?")
        .default(false)
        .interact()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mount(src: &str) -> MountConfig {
        MountConfig {
            src: src.to_string(),
            dst: "/container/path".to_string(),
            readonly: false,
            isolation: crate::isolation::MountIsolation::Shared,
        }
    }

    #[test]
    fn detects_ssh_mount() {
        let mounts = vec![mount("/home/user/.ssh")];
        let hits = find_sensitive_mounts(&mounts);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].src, "/home/user/.ssh");
        assert!(hits[0].reason.contains("SSH"));
    }

    #[test]
    fn detects_aws_mount() {
        let hits = find_sensitive_mounts(&[mount("/home/user/.aws")]);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].reason.contains("AWS"));
    }

    #[test]
    fn detects_gnupg_mount() {
        let hits = find_sensitive_mounts(&[mount("/home/user/.gnupg")]);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].reason.contains("GPG"));
    }

    #[test]
    fn detects_gcloud_mount() {
        let hits = find_sensitive_mounts(&[mount("/home/user/.config/gcloud")]);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].reason.contains("Google Cloud"));
    }

    #[test]
    fn detects_kube_mount() {
        let hits = find_sensitive_mounts(&[mount("/home/user/.kube")]);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].reason.contains("Kubernetes"));
    }

    #[test]
    fn detects_docker_mount() {
        let hits = find_sensitive_mounts(&[mount("/home/user/.docker")]);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].reason.contains("Docker"));
    }

    #[test]
    fn ignores_safe_mounts() {
        let mounts = vec![
            mount("/home/user/projects"),
            mount("/tmp/workspace"),
            mount("/var/data"),
        ];
        assert!(find_sensitive_mounts(&mounts).is_empty());
    }

    #[test]
    fn detects_multiple_sensitive_mounts() {
        let mounts = vec![
            mount("/home/user/.ssh"),
            mount("/home/user/projects"),
            mount("/home/user/.aws"),
        ];
        let hits = find_sensitive_mounts(&mounts);
        assert_eq!(hits.len(), 2);
    }

    #[test]
    fn handles_trailing_slash_on_sensitive_mount() {
        let hits = find_sensitive_mounts(&[mount("/home/user/.ssh/")]);
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn does_not_match_partial_name() {
        // ".sshd" should NOT match ".ssh"
        let hits = find_sensitive_mounts(&[mount("/home/user/.sshd")]);
        assert!(hits.is_empty());
    }
}
