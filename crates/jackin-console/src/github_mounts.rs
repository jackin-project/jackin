// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! GitHub mount resolution for workspace rows.
//!
//! Given a `WorkspaceConfig`, return the subset of mounts whose sources
//! are GitHub-hosted Git repositories with a resolvable web URL. The
//! result drives two independent features:
//!
//! - `handle_list_open_in_github` in `input.rs` — the `o` key on the
//!   workspace list opens or lets the operator pick a GitHub URL.
//! - The footer hint in `render.rs` — the "O open in GitHub" segment
//!   is shown only when the selected workspace has at least one
//!   GitHub-backed mount.
//!
//! Originally this helper lived in `input.rs` because the `o`-key
//! handler was its first caller. The footer render path needs the same
//! query and a render -> input dependency was the wrong shape — this
//! is a workspace query, not input logic. Moving the helper into a
//! dedicated module under `launch/manager` lets both render and input
//! call a neutral helper.

use crate::mount_info::{GitBranch, GitOrigin, MountKind, inspect};
use crate::mount_info_cache::MountInfoCache;

/// GitHub-hosted mount row that can be shown in the TUI picker or opened
/// directly when it is the only candidate.
#[derive(Debug, Clone)]
pub struct GithubChoice {
    pub src: String,
    pub branch: String,
    pub url: String,
}

pub trait WorkspaceMounts {
    fn mount_sources(&self) -> impl Iterator<Item = &str>;
}

/// Project `ws`'s mounts down to the list of GitHub-hosted sources that
/// expose a resolvable web URL. Mounts with non-GitHub remotes, no
/// remote at all, plain folders, or missing sources are omitted.
pub fn resolve_for_workspace(ws: &impl WorkspaceMounts) -> Vec<GithubChoice> {
    ws.mount_sources()
        .filter_map(|src| github_choice_from_kind(src, inspect(src)))
        .collect()
}

/// Same projection as [`resolve_for_workspace`], but uses mount metadata
/// already collected by the TUI's typed mount-info refresh effect.
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

#[cfg(test)]
mod tests;

/// `WorkspaceMounts` impl for `jackin_config::WorkspaceConfig`.
impl WorkspaceMounts for jackin_config::WorkspaceConfig {
    fn mount_sources(&self) -> impl Iterator<Item = &str> {
        self.mounts.iter().map(|m| m.src.as_str())
    }
}
