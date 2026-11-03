// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{Phase, SandboxName, TerminalText};
use jackin_openshell::PowerAction;
use jackin_session::{ChangeHunk, ChangeLine, ChangeSet, ChangeStatus, ChangedFile, ChangesError};
use jackin_testkit::{Rig, SandboxHandle, TestResult, guard};
use tokio_util::sync::CancellationToken;

fn t(text: &str) -> TerminalText {
    TerminalText::sanitize(text.as_bytes())
}

fn file(path: &str, status: ChangeStatus, hunks: Vec<ChangeHunk>) -> ChangedFile {
    ChangedFile {
        path: t(path),
        status,
        hunks,
        unread: None,
    }
}

fn hunk(old_start: usize, new_start: usize, lines: Vec<ChangeLine>) -> ChangeHunk {
    ChangeHunk {
        old_start,
        new_start,
        lines,
    }
}

fn kept(line: &str) -> ChangeLine {
    ChangeLine::Context(t(line))
}

fn added(line: &str) -> ChangeLine {
    ChangeLine::Added(t(line))
}

fn removed(line: &str) -> ChangeLine {
    ChangeLine::Removed(t(line))
}

async fn changes(rig: &Rig, name: &SandboxName) -> Result<ChangeSet, ChangesError> {
    rig.world
        .session
        .changes(name, &CancellationToken::new())
        .await
}

fn write(handle: &SandboxHandle, path: &str, bytes: &[u8]) -> TestResult {
    let target = handle.root().join("project").join(path);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(target, bytes)?;
    Ok(())
}

#[tokio::test]
async fn changes_lists_tracked_edits_deletions_and_untracked_files_with_their_hunks() -> TestResult
{
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        assert_eq!(
            changes(&rig, &name).await?,
            ChangeSet {
                files: Vec::new(),
                omitted: 0
            }
        );

        write(&handle, "README.md", b"readme\nsecond line\n")?;
        std::fs::remove_file(handle.root().join("project/src/main.rs"))?;
        write(&handle, "notes.txt", b"a\nb\n")?;
        write(&handle, "blob.bin", b"\0\x01\x02")?;
        write(&handle, "target/ignored.o", b"built")?;

        let found = changes(&rig, &name).await?;
        assert_eq!(
            found.files,
            vec![
                file(
                    "README.md",
                    ChangeStatus::Modified,
                    vec![hunk(1, 1, vec![kept("readme"), added("second line")])]
                ),
                file(
                    "src/main.rs",
                    ChangeStatus::Deleted,
                    vec![hunk(1, 0, vec![removed("fn main() {}")])]
                ),
                file("blob.bin", ChangeStatus::Added, Vec::new()),
                file(
                    "notes.txt",
                    ChangeStatus::Added,
                    vec![hunk(0, 1, vec![added("a"), added("b")])]
                ),
            ]
        );
        assert_eq!((found.additions(), found.deletions()), (3, 1));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_staged_rename_is_one_renamed_file_and_a_new_staged_file_is_added() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        handle.git(&["mv", "src/main.rs", "src/app.rs"]).await?;
        write(&handle, "staged.txt", b"x\n")?;
        handle.git(&["add", "staged.txt"]).await?;

        let found = changes(&rig, &handle.name()).await?;
        assert_eq!(
            found.files,
            vec![
                file(
                    "src/app.rs",
                    ChangeStatus::Renamed {
                        from: t("src/main.rs")
                    },
                    Vec::new()
                ),
                file(
                    "staged.txt",
                    ChangeStatus::Added,
                    vec![hunk(0, 1, vec![added("x")])]
                ),
            ]
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn changes_refuses_a_stopped_or_absent_sandbox_and_a_project_that_is_not_a_repository()
-> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();

        let absent = SandboxName::parse("jackin-ghost-0000")?;
        assert!(matches!(
            changes(&rig, &absent).await,
            Err(ChangesError::NotFound(found)) if found == absent
        ));

        std::fs::remove_dir_all(handle.root().join("project/.git"))?;
        assert!(matches!(
            changes(&rig, &name).await,
            Err(ChangesError::NoRepository { .. })
        ));

        rig.world.session.power(&name, PowerAction::Stop).await?;
        assert!(matches!(
            changes(&rig, &name).await,
            Err(ChangesError::NotRunning {
                phase: Phase::Stopped,
                ..
            })
        ));
        Ok(())
    })
    .await
}
