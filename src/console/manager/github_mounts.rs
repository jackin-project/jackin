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

use crate::console::widgets::github_picker::GithubChoice;
use crate::workspace::WorkspaceConfig;

use super::mount_info::{GitBranch, GitHost, MountKind, inspect};

/// Project `ws`'s mounts down to the list of GitHub-hosted sources that
/// expose a resolvable web URL. Mounts with non-GitHub remotes, no
/// remote at all, plain folders, or missing sources are omitted.
pub(super) fn resolve_for_workspace(ws: &WorkspaceConfig) -> Vec<GithubChoice> {
    ws.mounts
        .iter()
        .filter_map(|m| {
            let MountKind::Git {
                branch,
                host: GitHost::Github,
                web_url: Some(url),
                ..
            } = inspect(&m.src)
            else {
                return None;
            };
            let branch_label = match branch {
                GitBranch::Named(b) => b,
                GitBranch::Detached { short_sha } => format!("detached {short_sha}"),
                GitBranch::Unknown => "unknown".to_string(),
            };
            Some(GithubChoice {
                src: m.src.clone(),
                branch: branch_label,
                url,
            })
        })
        .collect()
}
