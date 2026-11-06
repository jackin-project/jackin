// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::{Duration, UNIX_EPOCH};

use jackin_core::{Phase, SandboxName, Sha256, TerminalText};
use jackin_openshell::PowerAction;
use jackin_session::{
    ChangeHunk, ChangeLine, ChangeSet, ChangeStatus, ChangedFile, ChangesError, Unread,
};
use jackin_testkit::{RepoFixture, Rig, SandboxHandle, TestResult, guard};
use tokio_util::sync::CancellationToken;

use super::support::state;

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
async fn changes_lists_the_work_of_a_linked_worktree_under_its_name() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        let path = handle.root().join("side").display().to_string();
        handle
            .git(&["worktree", "add", "-q", "--detach", &path])
            .await?;
        write(&handle, "main-only.txt", b"main\n")?;
        std::fs::write(handle.root().join("side/side-only.txt"), b"side\n")?;

        let found = changes(&rig, &name).await?;
        let paths: Vec<String> = found
            .files
            .iter()
            .map(|changed| changed.path.to_string())
            .collect();
        assert_eq!(paths, ["main-only.txt", "side/side-only.txt"]);
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

        rig.world
            .session
            .power(&name, PowerAction::Stop, &CancellationToken::new())
            .await?;
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

#[tokio::test]
async fn a_staged_file_in_a_repository_with_an_unborn_head_is_listed_before_the_untracked_ones()
-> TestResult {
    guard(async {
        let repo = RepoFixture::new().await?;
        repo.file("staged.txt", b"s\n").await?;
        repo.file("loose.txt", b"l\n").await?;
        let rig = Rig::builder().repo(repo).build().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        handle.git(&["add", "staged.txt"]).await?;
        handle
            .git(&["rev-parse", "-q", "--verify", "HEAD"])
            .await
            .unwrap_err();

        let found = changes(&rig, &handle.name()).await?;

        assert_eq!(
            found.files,
            [
                file(
                    "staged.txt",
                    ChangeStatus::Added,
                    vec![hunk(0, 1, vec![added("s")])]
                ),
                file(
                    "loose.txt",
                    ChangeStatus::Added,
                    vec![hunk(0, 1, vec![added("l")])]
                ),
            ]
        );
        assert_eq!(found.omitted, 0);
        Ok(())
    })
    .await
}

const LIMIT: usize = 256 * 1024;
const LINE: &str = "0123456789abcde";

#[tokio::test]
async fn an_untracked_file_over_the_read_limit_is_listed_unread_even_when_it_is_larger_than_the_output_limit()
-> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();

        let at_limit = format!("{LINE}\n").repeat(LIMIT / (LINE.len() + 1));
        assert_eq!(at_limit.len(), LIMIT);
        write(&handle, "a-at-limit.txt", at_limit.as_bytes())?;
        write(
            &handle,
            "b-over-by-one.txt",
            format!("{at_limit}x").as_bytes(),
        )?;
        write(&handle, "c-300-kib.txt", &vec![b'c'; 300 * 1024])?;
        write(&handle, "d-17-mib.bin", &vec![b'd'; 17 * 1024 * 1024])?;
        let outside = tempfile::tempdir()?;
        let target = outside.path().join("target.txt");
        std::fs::write(&target, b"SENTINEL-BEHIND-THE-LINK\n")?;
        std::os::unix::fs::symlink(&target, handle.root().join("project/e-link"))?;

        let found = changes(&rig, &name).await?;
        let summary: Vec<(&str, usize, Option<Unread>)> = found
            .files
            .iter()
            .map(|changed| {
                (
                    changed.path.as_str(),
                    changed.hunks.iter().map(|hunk| hunk.lines.len()).sum(),
                    changed.unread,
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                ("a-at-limit.txt", LIMIT / (LINE.len() + 1), None),
                ("b-over-by-one.txt", 0, Some(Unread::TooLarge)),
                ("c-300-kib.txt", 0, Some(Unread::TooLarge)),
                ("d-17-mib.bin", 0, Some(Unread::TooLarge)),
                ("e-link", 0, Some(Unread::NotRegular)),
            ]
        );
        assert_eq!(found.omitted, 0);
        let kept = found.files.first().ok_or("no files")?;
        assert!(
            kept.hunks
                .iter()
                .flat_map(|hunk| &hunk.lines)
                .all(|line| *line == added(LINE))
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn untracked_files_beyond_the_first_five_hundred_are_counted_not_listed() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        for index in 0..501 {
            write(&handle, &format!("f{index:03}.txt"), b"x\n")?;
        }
        let more = changes(&rig, &name).await?;
        let listed: Vec<String> = more
            .files
            .iter()
            .map(|found| found.path.as_str().to_owned())
            .collect();
        let first: Vec<String> = (0..500).map(|index| format!("f{index:03}.txt")).collect();
        assert_eq!(
            (
                listed.len(),
                listed.last().map(String::as_str),
                more.omitted
            ),
            (500, Some("f499.txt"), 1)
        );
        assert!(listed == first, "the listed paths are not the first 500");
        Ok(())
    })
    .await
}

async fn touch_readme(handle: &SandboxHandle) -> TestResult {
    tokio::fs::OpenOptions::new()
        .write(true)
        .open(handle.root().join("project/README.md"))
        .await?
        .into_std()
        .await
        .set_modified(UNIX_EPOCH + Duration::from_secs(1_700_000_000))?;
    Ok(())
}

fn index_digest(handle: &SandboxHandle) -> Result<Sha256, std::io::Error> {
    Ok(Sha256::of(&std::fs::read(
        handle.root().join("project/.git/index"),
    )?))
}

#[tokio::test]
async fn asking_for_the_work_state_never_rewrites_the_sandbox_index() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        touch_readme(&handle).await?;
        let before = index_digest(&handle)?;

        state(&rig, &name).await;
        assert_eq!(index_digest(&handle)?, before);

        handle.git(&["status", "--porcelain"]).await?;
        assert_ne!(index_digest(&handle)?, before);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn asking_for_the_changes_never_rewrites_the_sandbox_index() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        touch_readme(&handle).await?;
        let before = index_digest(&handle)?;

        assert_eq!(
            changes(&rig, &name).await?,
            ChangeSet {
                files: Vec::new(),
                omitted: 0
            }
        );
        assert_eq!(index_digest(&handle)?, before);

        handle.git(&["status", "--porcelain"]).await?;
        assert_ne!(index_digest(&handle)?, before);
        Ok(())
    })
    .await
}
