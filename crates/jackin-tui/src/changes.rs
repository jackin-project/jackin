// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_console::backend::{
    ChangeSet, ChangedFile as DiffFile, DiffLine, DiffStatus, Hunk as DiffHunk, UnreadFile,
};
use jackin_core::SandboxName;
use jackin_session::{ChangeLine, ChangeStatus, ChangedFile, Session, error_chain};
use tokio_util::sync::CancellationToken;

fn status(status: &ChangeStatus) -> DiffStatus {
    match status {
        ChangeStatus::Added => DiffStatus::Added,
        ChangeStatus::Modified => DiffStatus::Modified,
        ChangeStatus::Deleted => DiffStatus::Deleted,
        ChangeStatus::Renamed { from } => DiffStatus::Renamed {
            from: from.as_str().to_owned(),
        },
    }
}

fn changed_file(file: &ChangedFile) -> DiffFile {
    DiffFile {
        path: file.path.as_str().to_owned(),
        status: status(&file.status),
        hunks: file
            .hunks
            .iter()
            .map(|hunk| DiffHunk {
                old_start: hunk.old_start,
                new_start: hunk.new_start,
                lines: hunk
                    .lines
                    .iter()
                    .map(|line| match line {
                        ChangeLine::Context(text) => DiffLine::context(text.as_str()),
                        ChangeLine::Added(text) => DiffLine::add(text.as_str()),
                        ChangeLine::Removed(text) => DiffLine::remove(text.as_str()),
                    })
                    .collect(),
            })
            .collect(),
    }
}

pub async fn read_changes(
    session: &Session,
    sandbox: &SandboxName,
    unpushed: usize,
    cancel: &CancellationToken,
) -> Result<ChangeSet, String> {
    let found = session
        .changes(sandbox, cancel)
        .await
        .map_err(|error| error_chain(&error))?;
    let (unread, read): (Vec<_>, Vec<_>) =
        found.files.iter().partition(|file| file.unread.is_some());
    Ok(ChangeSet {
        files: read.into_iter().map(changed_file).collect(),
        unpushed,
        omitted: found.omitted,
        unread: unread
            .into_iter()
            .map(|file| UnreadFile {
                path: file.path.as_str().to_owned(),
                status: status(&file.status),
            })
            .collect(),
    })
}
