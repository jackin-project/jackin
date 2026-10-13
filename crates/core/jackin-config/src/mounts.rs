// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Parse `src[:dst][:ro]` mount specs from CLI arguments into `MountConfig`.
//!
//! Not responsible for global mount config deserialization (`config::mounts`)
//! or isolation type selection — only the CLI `src[:dst][:ro]` grammar and
//! path expansion.

use jackin_core::MountIsolation;

use crate::paths::{expand_tilde, resolve_path};
use crate::schema::MountConfig;

/// Parse a CLI `src[:dst][:ro]` mount string with tilde expansion only.
pub fn parse_mount_spec(spec: &str) -> crate::ConfigResult<MountConfig> {
    Ok(parse_mount_spec_inner(spec, false))
}

/// Like [`parse_mount_spec`] but also resolves relative paths to absolute.
/// Use this for CLI arguments where the user may pass relative paths.
pub fn parse_mount_spec_resolved(spec: &str) -> crate::ConfigResult<MountConfig> {
    Ok(parse_mount_spec_inner(spec, true))
}

fn parse_mount_spec_inner(spec: &str, resolve: bool) -> MountConfig {
    let (raw, readonly) = spec
        .strip_suffix(":ro")
        .map_or((spec, false), |value| (value, true));
    let (src, dst) = raw
        .split_once(':')
        .map_or_else(|| (raw, raw), |(s, d)| (s, d));
    let expand: fn(&str) -> String = if resolve { resolve_path } else { expand_tilde };
    let expanded_src = expand(src);
    let dst = if src == dst {
        expanded_src.clone()
    } else {
        dst.to_owned()
    };

    MountConfig {
        src: expanded_src,
        dst,
        readonly,
        isolation: MountIsolation::Shared,
    }
}

// ── Rule-C covering predicate ───────────────────────────────────────────

/// Returns true iff `parent` strictly covers `child` under rule C.
///
/// Rule C: `parent.src` is a proper ancestor of `child.src`, AND the path
/// suffix `child.src - parent.src` equals `child.dst - parent.dst`.
///
/// Equivalently: `child` projects the same host subtree to the same container
/// location that `parent` would already expose it at.
///
/// Identity (equal src and equal dst) returns false — that case is handled by
/// upsert-by-dst in `edit_workspace`.
///
/// The `readonly` flag is ignored here. Readonly mismatches are caught at
/// `plan_collapse` level, not in the predicate.
pub fn covers(parent: &MountConfig, child: &MountConfig) -> bool {
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
