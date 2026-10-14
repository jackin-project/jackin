// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

use crate::schema::MountConfig;

const SENSITIVE_SUFFIXES: &[(&str, &str)] = &[
    (".ssh", "SSH keys and configuration"),
    (".aws", "AWS credentials and configuration"),
    (".gnupg", "GPG keys and trust database"),
    (".config/gcloud", "Google Cloud credentials"),
    (".kube", "Kubernetes credentials and configuration"),
    (".docker", "Docker credentials and configuration"),
    (".config/gh", "GitHub CLI credentials"),
    (".netrc", "Network client credentials"),
    (".npmrc", "npm registry credentials"),
    (".git-credentials", "Git credential-store secrets"),
    (".config/op", "1Password CLI configuration"),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SensitiveMount {
    pub src: String,

    pub reason: String,
}

#[must_use]
pub fn normalize_sensitive_mount_sources<F>(
    mounts: &[MountConfig],
    mut canonicalize: F,
) -> Vec<MountConfig>
where
    F: FnMut(&Path) -> Option<PathBuf>,
{
    mounts
        .iter()
        .map(|mount| {
            let normalized = PathBuf::from(crate::paths::resolve_path(&mount.src));
            let resolved = canonicalize(&normalized).unwrap_or(normalized);
            MountConfig {
                src: resolved.display().to_string(),
                ..mount.clone()
            }
        })
        .collect()
}

pub fn find_sensitive_mounts(mounts: &[MountConfig]) -> Vec<SensitiveMount> {
    let mut hits = Vec::new();
    for mount in mounts {
        let components = normal_components(Path::new(&mount.src));
        for &(suffix, reason) in SENSITIVE_SUFFIXES {
            let sensitive = normal_components(Path::new(suffix));
            if !sensitive.is_empty()
                && components
                    .windows(sensitive.len())
                    .any(|window| window == sensitive)
            {
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

fn normal_components(path: &Path) -> Vec<OsString> {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_os_string()),
            _ => None,
        })
        .collect()
}
