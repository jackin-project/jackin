//! Parse `src[:dst][:ro]` mount specs from CLI arguments into `MountConfig`.
//!
//! Not responsible for global mount config deserialization (`config::mounts`)
//! or isolation type selection — only the CLI `src[:dst][:ro]` grammar and
//! path expansion.

use std::path::Path;

use crate::workspace::MountConfig;
use crate::workspace::paths::{expand_tilde, resolve_path};

pub fn parse_mount_spec(spec: &str) -> anyhow::Result<MountConfig> {
    Ok(parse_mount_spec_inner(spec, false))
}

/// Like [`parse_mount_spec`] but also resolves relative paths to absolute.
/// Use this for CLI arguments where the user may pass relative paths.
pub fn parse_mount_spec_resolved(spec: &str) -> anyhow::Result<MountConfig> {
    Ok(parse_mount_spec_inner(spec, true))
}

fn parse_mount_spec_inner(spec: &str, resolve: bool) -> MountConfig {
    let (raw, readonly) = spec
        .strip_suffix(":ro")
        .map_or((spec, false), |value| (value, true));
    let (src, dst) = raw
        .split_once(':')
        .map_or_else(|| (raw, raw), |(s, d)| (s, d));
    let expand = if resolve { resolve_path } else { expand_tilde };
    let expanded_src = expand(src);
    let dst = if src == dst {
        expanded_src.clone()
    } else {
        dst.to_string()
    };

    MountConfig {
        src: expanded_src,
        dst,
        readonly,
        isolation: crate::isolation::MountIsolation::Shared,
    }
}

/// Structural validation: absolute paths, no duplicate destinations.
/// Safe to call at config-save time — does not touch the filesystem.
pub fn validate_mount_specs(mounts: &[MountConfig]) -> anyhow::Result<()> {
    let mut seen_dst = std::collections::HashSet::new();

    for mount in mounts {
        if !Path::new(&mount.src).is_absolute() {
            anyhow::bail!("mount source must be absolute: {}", mount.src);
        }
        if !mount.dst.starts_with('/') {
            anyhow::bail!("mount destination must be an absolute path: {}", mount.dst);
        }
        if !seen_dst.insert(mount.dst.clone()) {
            anyhow::bail!("duplicate mount destination: {}", mount.dst);
        }
    }

    Ok(())
}

/// Filesystem validation: checks that mount sources exist on disk.
/// Call at load/resolve time, not at config-save time.
pub fn validate_mount_paths(mounts: &[MountConfig]) -> anyhow::Result<()> {
    for mount in mounts {
        if !Path::new(&mount.src).exists() {
            anyhow::bail!("mount source does not exist: {}", mount.src);
        }
    }

    Ok(())
}

/// Full validation: structural + filesystem checks combined.
pub fn validate_mounts(mounts: &[MountConfig]) -> anyhow::Result<()> {
    validate_mount_specs(mounts)?;
    validate_mount_paths(mounts)
}

// ── Rule-C covering predicate ───────────────────────────────────────────

/// Returns true iff `parent` strictly covers `child` under rule C:
/// `parent.src` is a proper ancestor of `child.src`, AND the path suffix
/// `child.src - parent.src` equals the path suffix `child.dst - parent.dst`.
///
/// Equivalently: `child` projects the same host subtree to the same container
/// location that `parent` would already expose it at.
///
/// Identity (equal src and equal dst) returns false — that case is handled by
/// upsert-by-dst in `edit_workspace`.
///
/// The `readonly` flag is ignored here. Readonly mismatches are caught at
/// `plan_collapse` level, not in the predicate.
pub(crate) fn covers(parent: &MountConfig, child: &MountConfig) -> bool {
    let parent_src = parent.src.trim_end_matches('/');
    let parent_dst = parent.dst.trim_end_matches('/');
    let child_src = child.src.trim_end_matches('/');
    let child_dst = child.dst.trim_end_matches('/');

    // Identity is not covering.
    if parent_src == child_src && parent_dst == child_dst {
        return false;
    }

    // child.src must be strictly under parent.src.
    let Some(src_suffix) = child_src.strip_prefix(parent_src) else {
        return false;
    };
    if !src_suffix.starts_with('/') {
        return false;
    }

    // child.dst must be strictly under parent.dst with the same suffix.
    let Some(dst_suffix) = child_dst.strip_prefix(parent_dst) else {
        return false;
    };
    src_suffix == dst_suffix
}

#[cfg(test)]
mod tests;
