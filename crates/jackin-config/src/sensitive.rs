//! Detect sensitive host paths (`~/.ssh`, `~/.aws`, etc.) in mount sources.
//!
//! Pure classification against a static suffix table — no filesystem access
//! or operator I/O. Callers own the prompt and the decision to abort or
//! proceed.

use crate::schema::MountConfig;

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
    /// Host mount source that matched a sensitive suffix.
    pub src: String,
    /// Human-readable reason (e.g. "SSH keys and configuration").
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
                    reason: reason.to_owned(),
                });
                break;
            }
        }
    }
    hits
}
