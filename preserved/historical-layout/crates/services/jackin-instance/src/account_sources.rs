// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Selected-account source validation and capture.

use super::{InstanceAuthBinding, auth, xdg_root_agent};
use anyhow::Context;
use jackin_config::AuthForwardMode;

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

pub(crate) fn validate_selected_account_sources(
    bindings: &[InstanceAuthBinding],
    _host_home: &Path,
) -> anyhow::Result<()> {
    let mut configured_cache_roots = BTreeMap::<PathBuf, String>::new();
    for binding in bindings {
        if xdg_root_agent(binding.agent)
            && let Some(roots) = &binding.xdg_roots
        {
            let cache_root = canonical_xdg_cache_root(&roots.cache)?;
            if let Some((previous_root, previous_key)) =
                configured_cache_roots.iter().find(|(previous_root, _)| {
                    cache_root == **previous_root
                        || cache_root.starts_with(previous_root)
                        || previous_root.starts_with(&cache_root)
                })
            {
                anyhow::bail!(
                    "configured XDG cache roots for instances {:?} and {:?} overlap at {}",
                    previous_key,
                    binding.key,
                    previous_root.display()
                );
            }
            configured_cache_roots.insert(cache_root, binding.key.clone());
        }
    }
    Ok(())
}

/// Re-admit the latest selected source as one descriptor/content revision.
/// The retained private snapshot crosses worker boundaries; original paths
/// remain descriptor identity only and are never reopened by a worker.
pub(crate) fn capture_selected_account_sources(
    bindings: &[InstanceAuthBinding],
    host_home: &Path,
    role_root: &Path,
) -> anyhow::Result<Vec<InstanceAuthBinding>> {
    validate_selected_account_sources(bindings, host_home)?;
    let snapshot_parent = role_root.join("provider-config/source-snapshots");
    bindings
        .iter()
        .map(|binding| {
            let mut admitted = binding.clone();
            if binding.mode == AuthForwardMode::Sync
                && let Some(source_dir) = binding.effective_selected_source_dir()
            {
                let descriptor = auth::AuthSourceDescriptor {
                    agent: binding.agent,
                    provider: binding.source_provider,
                    selector: binding.source_selector.clone(),
                    source_dir: source_dir.clone(),
                };
                if let Some(snapshot) = &binding.selected_source {
                    anyhow::ensure!(
                        snapshot.descriptor() == &descriptor,
                        "selected source descriptor changed after credential capture for {:?}",
                        binding.key
                    );
                } else {
                    admitted.selected_source = Some(
                        auth::capture_selected_source(
                            binding.agent,
                            binding.source_provider,
                            binding.source_selector.as_ref(),
                            &source_dir,
                            host_home,
                            &snapshot_parent,
                        )?
                        .with_context(|| {
                            format!(
                                "selected {} account credentials disappeared before capture",
                                binding.agent
                            )
                        })?,
                    );
                }
            }
            Ok(admitted)
        })
        .collect()
}

/// Compare XDG cache roots by their filesystem identity, not their spelling.
/// Reject parent traversal before canonicalization so a missing path cannot
/// smuggle an unresolved `..` through the fallback normalization.
pub(crate) fn canonical_xdg_cache_root(path: &Path) -> anyhow::Result<PathBuf> {
    anyhow::ensure!(
        !path
            .components()
            .any(|component| matches!(component, Component::ParentDir)),
        "XDG cache root contains parent traversal: {}",
        path.display()
    );
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::RootDir => normalized.push(Path::new("/")),
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::Normal(component) => normalized.push(component),
            Component::ParentDir => unreachable!("parent traversal rejected above"),
        }
    }
    if let Ok(canonical) = std::fs::canonicalize(&normalized) {
        return Ok(canonical);
    }

    let mut ancestor = normalized.clone();
    let mut missing = Vec::<OsString>::new();
    while !ancestor.exists() {
        let Some(name) = ancestor.file_name().map(OsString::from) else {
            return Ok(normalized);
        };
        missing.push(name);
        if !ancestor.pop() {
            return Ok(normalized);
        }
    }

    let mut resolved = std::fs::canonicalize(&ancestor).unwrap_or(ancestor);
    for name in missing.iter().rev() {
        resolved.push(name);
    }
    Ok(resolved)
}
