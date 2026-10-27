// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::mount_info::{GitBranch, GitOrigin, MountKind, inspect};
use crate::mount_info_cache::MountInfoCache;

#[derive(Debug, Clone)]
pub struct GithubChoice {
    pub src: String,
    pub branch: String,
    pub url: String,
}

pub trait WorkspaceMounts {
    fn mount_sources(&self) -> impl Iterator<Item = &str>;
}

pub fn resolve_for_workspace(ws: &impl WorkspaceMounts) -> Vec<GithubChoice> {
    ws.mount_sources()
        .filter_map(|src| github_choice_from_kind(src, inspect(src)))
        .collect()
}

pub fn resolve_for_workspace_from_cache(
    ws: &impl WorkspaceMounts,
    cache: &MountInfoCache,
) -> Vec<GithubChoice> {
    ws.mount_sources()
        .filter_map(|src| {
            cache
                .inspect_cached(src)
                .and_then(|kind| github_choice_from_kind(src, kind))
        })
        .collect()
}

fn github_choice_from_kind(src: &str, kind: MountKind) -> Option<GithubChoice> {
    let MountKind::Git {
        branch,
        origin: Some(GitOrigin::Github { web_url, .. }),
    } = kind
    else {
        return None;
    };
    let branch_label = match branch {
        GitBranch::Named(b) => b,
        GitBranch::Detached { short_sha } => format!("detached {short_sha}"),
        GitBranch::Unknown => "unknown".to_owned(),
    };
    Some(GithubChoice {
        src: src.to_owned(),
        branch: branch_label,
        url: web_url,
    })
}

impl WorkspaceMounts for jackin_config::WorkspaceConfig {
    fn mount_sources(&self) -> impl Iterator<Item = &str> {
        self.mounts.iter().map(|m| m.src.as_str())
    }
}
