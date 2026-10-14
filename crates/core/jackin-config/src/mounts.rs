// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::MountIsolation;

use crate::paths::{expand_tilde, resolve_path};
use crate::schema::MountConfig;

pub fn parse_mount_spec(spec: &str) -> crate::ConfigResult<MountConfig> {
    Ok(parse_mount_spec_inner(spec, false))
}

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

pub fn covers(parent: &MountConfig, child: &MountConfig) -> bool {
    let parent_src = parent.src.trim_end_matches('/');
    let parent_dst = parent.dst.trim_end_matches('/');
    let child_src = child.src.trim_end_matches('/');
    let child_dst = child.dst.trim_end_matches('/');

    if parent_src == child_src && parent_dst == child_dst {
        return false;
    }

    let Some(src_suffix) = child_src.strip_prefix(parent_src) else {
        return false;
    };
    if !src_suffix.starts_with('/') {
        return false;
    }

    let Some(dst_suffix) = child_dst.strip_prefix(parent_dst) else {
        return false;
    };
    src_suffix == dst_suffix
}

#[cfg(test)]
mod tests;
