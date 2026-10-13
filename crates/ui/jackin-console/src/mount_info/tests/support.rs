// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn github_mount(owner_repo: &str, branch: GitBranch) -> MountKind {
    let base = format!("https://github.com/{owner_repo}");
    let web_url = match &branch {
        GitBranch::Named(b) => format!("{base}/tree/{b}"),
        GitBranch::Detached { short_sha } => format!("{base}/commit/{short_sha}"),
        GitBranch::Unknown => base.clone(),
    };
    MountKind::Git {
        branch,
        origin: Some(GitOrigin::Github {
            remote_url: format!("{base}.git"),
            web_url,
        }),
    }
}

pub(super) fn other_mount(remote_url: &str, branch: GitBranch) -> MountKind {
    MountKind::Git {
        branch,
        origin: Some(GitOrigin::Other {
            remote_url: remote_url.into(),
        }),
    }
}

pub(super) fn no_origin_mount(branch: GitBranch) -> MountKind {
    MountKind::Git {
        branch,
        origin: None,
    }
}
