// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::path::Path;
use std::time::{Duration, UNIX_EPOCH};

use jackin_core::{Sha256, TerminalText};
use jackin_openshell::GatewayError;
use jackin_session::{
    ChangeHunk, ChangeLine, ChangeSet, ChangeStatus, ChangedFile, ChangesError, Unread,
};
use jackin_testkit::{Rig, SandboxHandle, TestResult, guard};

use super::tap::Tap;
use tokio_util::sync::CancellationToken;

const LIMIT: usize = 256 * 1024;
const FILES: usize = 500;

fn t(text: &str) -> TerminalText {
    TerminalText::sanitize(text.as_bytes())
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

fn untracked(path: &str, line: &str) -> ChangedFile {
    ChangedFile {
        path: t(path),
        status: ChangeStatus::Added,
        hunks: vec![hunk(0, 1, vec![added(line)])],
        unread: None,
    }
}

fn edited(path: &str, line: &str) -> ChangedFile {
    ChangedFile {
        path: t(path),
        status: ChangeStatus::Modified,
        hunks: vec![hunk(1, 1, vec![kept("readme"), added(line)])],
        unread: None,
    }
}

async fn changes(rig: &Rig, handle: &SandboxHandle) -> Result<ChangeSet, ChangesError> {
    rig.world
        .session
        .changes(&handle.name(), &CancellationToken::new())
        .await
}

async fn add_agent(
    handle: &SandboxHandle,
    relative: &str,
    branch: &str,
) -> Result<String, Box<dyn Error>> {
    let path = handle.root().join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let shown = path.display().to_string();
    handle
        .git(&["worktree", "add", "-q", "-b", branch, &shown])
        .await?;
    Ok(std::fs::canonicalize(path)?.display().to_string())
}

fn write(root: &str, path: &str, bytes: &[u8]) -> TestResult {
    let target = Path::new(root).join(path);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(target, bytes)?;
    Ok(())
}

fn lead(handle: &SandboxHandle) -> Result<String, Box<dyn Error>> {
    Ok(std::fs::canonicalize(handle.root().join("project"))?
        .display()
        .to_string())
}

fn paths(found: &ChangeSet) -> Vec<&str> {
    found.files.iter().map(|file| file.path.as_str()).collect()
}

fn body(file: &ChangedFile) -> usize {
    file.hunks.iter().map(|hunk| hunk.lines.len()).sum()
}

#[tokio::test]
async fn two_checkouts_editing_the_same_relative_path_are_both_listed_under_their_own_identity()
-> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let omp = add_agent(&handle, "work/omp/project", "agent/omp").await?;
        let codex = add_agent(&handle, "work/codex/project", "agent/codex").await?;
        let lead = lead(&handle)?;
        write(&lead, "README.md", b"readme\nlead\n")?;
        write(&omp, "README.md", b"readme\nomp\n")?;
        write(&codex, "README.md", b"readme\ncodex\n")?;
        write(&lead, "notes.txt", b"zero\n")?;
        write(&omp, "notes.txt", b"one\n")?;
        write(&codex, "notes.txt", b"two\n")?;

        let found = changes(&rig, &handle).await?;

        assert_eq!(
            found.files,
            vec![
                edited("README.md", "lead"),
                untracked("notes.txt", "zero"),
                edited(&format!("{codex}/README.md"), "codex"),
                untracked(&format!("{codex}/notes.txt"), "two"),
                edited(&format!("{omp}/README.md"), "omp"),
                untracked(&format!("{omp}/notes.txt"), "one"),
            ]
        );
        assert_eq!(found.omitted, 0);
        assert_eq!((found.additions(), found.deletions()), (6, 0));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_rename_in_each_checkout_is_qualified_on_both_sides() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let agent = add_agent(&handle, "work/omp/project", "agent/omp").await?;
        handle.git(&["mv", "src/main.rs", "src/app.rs"]).await?;
        handle
            .git(&["-C", &agent, "mv", "src/main.rs", "src/app.rs"])
            .await?;

        let found = changes(&rig, &handle).await?;

        let renamed = |path: &str, from: &str| ChangedFile {
            path: t(path),
            status: ChangeStatus::Renamed { from: t(from) },
            hunks: Vec::new(),
            unread: None,
        };
        assert_eq!(
            found.files,
            vec![
                renamed("src/app.rs", "src/main.rs"),
                renamed(
                    &format!("{agent}/src/app.rs"),
                    &format!("{agent}/src/main.rs")
                ),
            ]
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_detached_checkout_is_listed_under_its_own_path() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let path = handle.root().join("detached").display().to_string();
        handle
            .git(&["worktree", "add", "-q", "--detach", &path])
            .await?;
        let detached = std::fs::canonicalize(&path)?.display().to_string();
        write(&detached, "README.md", b"readme\nloose\n")?;

        let found = changes(&rig, &handle).await?;

        assert_eq!(
            found.files,
            vec![edited(&format!("{detached}/README.md"), "loose")]
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_checkout_whose_directory_is_gone_is_skipped_not_an_error() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let gone = add_agent(&handle, "work/gone/project", "agent/gone").await?;
        let kept = add_agent(&handle, "work/kept/project", "agent/kept").await?;
        write(&kept, "README.md", b"readme\nstays\n")?;
        std::fs::remove_dir_all(&gone)?;

        let found = changes(&rig, &handle).await?;

        assert_eq!(
            found.files,
            vec![edited(&format!("{kept}/README.md"), "stays")]
        );
        assert_eq!(found.omitted, 0);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_unborn_checkout_lists_its_staged_and_untracked_files() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let path = handle.root().join("orphan").display().to_string();
        handle
            .git(&[
                "worktree",
                "add",
                "-q",
                "--orphan",
                "-b",
                "agent/orphan",
                &path,
            ])
            .await?;
        let orphan = std::fs::canonicalize(&path)?.display().to_string();
        write(&orphan, "fresh.txt", b"fresh\n")?;
        handle.git(&["-C", &orphan, "add", "fresh.txt"]).await?;
        write(&orphan, "loose.txt", b"loose\n")?;

        let found = changes(&rig, &handle).await?;

        assert_eq!(
            found.files,
            vec![
                untracked(&format!("{orphan}/fresh.txt"), "fresh"),
                untracked(&format!("{orphan}/loose.txt"), "loose"),
            ]
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_symlink_and_an_oversized_file_in_a_linked_checkout_are_read_from_that_checkout()
-> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let agent = add_agent(&handle, "work/omp/project", "agent/omp").await?;
        write(&agent, "a-small.txt", b"small\n")?;
        write(&agent, "b-big.bin", &vec![b'b'; LIMIT + 1])?;
        std::os::unix::fs::symlink("README.md", Path::new(&agent).join("c-link"))?;

        let found = changes(&rig, &handle).await?;

        let summary: Vec<(&str, usize, Option<Unread>)> = found
            .files
            .iter()
            .map(|file| (file.path.as_str(), body(file), file.unread))
            .collect();
        assert_eq!(
            summary,
            [
                (format!("{agent}/a-small.txt").as_str(), 1, None),
                (
                    format!("{agent}/b-big.bin").as_str(),
                    0,
                    Some(Unread::TooLarge)
                ),
                (
                    format!("{agent}/c-link").as_str(),
                    0,
                    Some(Unread::NotRegular)
                ),
            ]
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_file_cap_is_shared_by_every_checkout_and_the_rest_is_counted() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let agent = add_agent(&handle, "work/omp/project", "agent/omp").await?;
        let lead = lead(&handle)?;
        for index in 0..300 {
            write(&lead, &format!("a{index:03}.txt"), b"x\n")?;
            write(&agent, &format!("b{index:03}.txt"), b"x\n")?;
        }

        let found = changes(&rig, &handle).await?;

        let expected: Vec<String> = (0..300)
            .map(|index| format!("a{index:03}.txt"))
            .chain((0..FILES - 300).map(|index| format!("{agent}/b{index:03}.txt")))
            .collect();
        assert_eq!(paths(&found), expected);
        assert_eq!(found.omitted, 600 - FILES);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn tracked_edits_are_not_held_to_the_untracked_file_cap() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let agent = add_agent(&handle, "work/omp/project", "agent/omp").await?;
        let lead = lead(&handle)?;
        for index in 0..FILES - 2 {
            write(&lead, &format!("a{index:03}.txt"), b"x\n")?;
        }
        write(&agent, "README.md", b"readme\nedited\n")?;
        for index in 0..3 {
            write(&agent, &format!("b{index}.txt"), b"x\n")?;
        }

        let found = changes(&rig, &handle).await?;

        let listed = paths(&found);
        assert_eq!(listed.len(), FILES + 1);
        assert_eq!(
            &listed[FILES - 2..],
            [
                format!("{agent}/README.md").as_str(),
                format!("{agent}/b0.txt").as_str(),
                format!("{agent}/b1.txt").as_str()
            ]
        );
        assert_eq!(found.omitted, 1);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn more_tracked_edits_than_the_untracked_cap_are_all_listed() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let lead = lead(&handle)?;
        for index in 0..FILES + 2 {
            write(&lead, &format!("t{index:03}.txt"), b"old\n")?;
        }
        handle.git(&["add", "-A"]).await?;
        handle.git(&["commit", "-q", "-m", "many"]).await?;
        for index in 0..FILES + 2 {
            write(&lead, &format!("t{index:03}.txt"), b"new\n")?;
        }

        let found = changes(&rig, &handle).await?;

        assert_eq!(found.files.len(), FILES + 2);
        assert_eq!(found.omitted, 0);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_worktree_directory_with_a_newline_in_its_name_is_still_listed() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let agent = add_agent(&handle, "work/two\nlines/project", "agent/lines").await?;
        write(&agent, "README.md", b"readme\nedited\n")?;

        let found = changes(&rig, &handle).await?;

        assert_eq!(
            found.files,
            vec![edited(&format!("{agent}/README.md"), "edited")]
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn calls_that_each_fit_the_limit_but_together_exceed_it_end_in_a_timeout() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let agent = add_agent(&handle, "work/omp/project", "agent/omp").await?;
        write(&agent, "loose.txt", b"x\n")?;
        let mut limits = jackin_testkit::default_limits();
        limits.upload = Duration::from_secs(2);
        let tap = Tap::new(&rig);
        let session = tap.session(&rig, limits);
        let name = handle.name();

        assert_eq!(
            session
                .changes(&name, &CancellationToken::new())
                .await?
                .files
                .len(),
            1
        );

        tap.slow_programs(Duration::from_millis(400));
        let outcome = session.changes(&name, &CancellationToken::new()).await;

        assert!(
            matches!(
                outcome,
                Err(ChangesError::Gateway(GatewayError::Timeout { .. }))
            ),
            "{outcome:?}"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_tracked_patch_byte_budget_is_spent_across_checkouts() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let lead = lead(&handle)?;
        let big = |tag: &str| format!("{tag}-0123456789\n").repeat(300_000);
        write(&lead, "big.txt", b"old\n")?;
        handle.git(&["add", "-A"]).await?;
        handle.git(&["commit", "-q", "-m", "big"]).await?;
        let agent = add_agent(&handle, "work/omp/project", "agent/omp").await?;
        write(&lead, "big.txt", big("lead").as_bytes())?;
        write(&agent, "big.txt", big("omp").as_bytes())?;

        let found = changes(&rig, &handle).await?;

        assert_eq!(paths(&found), ["big.txt"]);
        assert_eq!(found.omitted, 1);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_patch_over_the_gateway_output_limit_is_counted_not_fatal() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let lead = lead(&handle)?;
        write(&lead, "big.txt", b"old\n")?;
        handle.git(&["add", "-A"]).await?;
        handle.git(&["commit", "-q", "-m", "big"]).await?;
        let agent = add_agent(&handle, "work/omp/project", "agent/omp").await?;
        write(
            &agent,
            "big.txt",
            "0123456789abcdef\n".repeat(1_100_000).as_bytes(),
        )?;
        write(&agent, "small.txt", b"x\n")?;

        let found = changes(&rig, &handle).await?;

        assert_eq!(paths(&found), [format!("{agent}/small.txt")]);
        assert_eq!(found.omitted, 1);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_untracked_listing_over_the_gateway_output_limit_is_counted_not_fatal() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let agent = add_agent(
            &handle,
            "work/$(touch pwned) \"q\" -x/project",
            "agent/hostile",
        )
        .await?;
        let deep = format!("{}/", vec!["d".repeat(100); 6].join("/"));
        let total = 22_500;
        for index in 0..total {
            write(
                &agent,
                &format!("{deep}{index:05}{}", "x".repeat(140)),
                b"x",
            )?;
        }

        let found = changes(&rig, &handle).await?;

        assert_eq!(found.files.len(), FILES);
        assert_eq!(found.omitted, total - FILES);
        assert!(!Path::new("pwned").exists());
        assert!(!handle.root().join("pwned").exists());
        assert!(
            found
                .files
                .iter()
                .all(|file| file.path.as_str().starts_with(&agent))
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn names_with_shell_syntax_are_read_as_data() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let agent = add_agent(
            &handle,
            "work/$(touch pwned) \"q\" -x/project",
            "agent/hostile",
        )
        .await?;
        write(
            &agent,
            "$(touch pwned2) `touch pwned3` \"q\" -f.txt",
            b"x\n",
        )?;

        let found = changes(&rig, &handle).await?;

        assert_eq!(
            found.files,
            vec![untracked(
                &format!("{agent}/$(touch pwned2) `touch pwned3` \"q\" -f.txt"),
                "x"
            )]
        );
        for name in ["pwned", "pwned2", "pwned3"] {
            assert!(!handle.root().join(name).exists());
            assert!(!Path::new(&agent).join(name).exists());
            assert!(!handle.root().join("project").join(name).exists());
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_removed_worktree_directory_with_a_newline_in_its_name_is_skipped() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let gone = add_agent(&handle, "work/two\nlines/project", "agent/lines").await?;
        let kept = add_agent(&handle, "work/kept/project", "agent/kept").await?;
        write(&kept, "README.md", b"readme\nstays\n")?;
        std::fs::remove_dir_all(&gone)?;

        let found = changes(&rig, &handle).await?;

        assert_eq!(
            found.files,
            vec![edited(&format!("{kept}/README.md"), "stays")]
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_checkout_git_cannot_read_fails_the_listing_instead_of_dropping_it() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let agent = add_agent(&handle, "work/omp/project", "agent/omp").await?;
        let lead = lead(&handle)?;
        write(&lead, "notes.txt", b"zero\n")?;
        for entry in std::fs::read_dir(Path::new(&lead).join(".git/worktrees"))? {
            std::fs::write(entry?.path().join("index"), b"not an index")?;
        }
        write(&agent, "notes.txt", b"one\n")?;

        let refused = changes(&rig, &handle).await;

        assert!(
            matches!(refused, Err(ChangesError::Git { .. })),
            "{refused:?}"
        );
        Ok(())
    })
    .await
}

fn worktree_index_digest(lead: &str) -> Result<Vec<Sha256>, Box<dyn Error>> {
    let mut digests = Vec::new();
    for entry in std::fs::read_dir(Path::new(lead).join(".git/worktrees"))? {
        digests.push(Sha256::of(&std::fs::read(entry?.path().join("index"))?));
    }
    Ok(digests)
}

#[tokio::test]
async fn asking_for_the_changes_never_rewrites_a_linked_checkout_index() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let agent = add_agent(&handle, "work/omp/project", "agent/omp").await?;
        let lead = lead(&handle)?;
        tokio::fs::OpenOptions::new()
            .write(true)
            .open(Path::new(&agent).join("README.md"))
            .await?
            .into_std()
            .await
            .set_modified(UNIX_EPOCH + Duration::from_secs(1_700_000_000))?;
        let before = worktree_index_digest(&lead)?;

        assert_eq!(
            changes(&rig, &handle).await?,
            ChangeSet {
                files: Vec::new(),
                omitted: 0
            }
        );
        assert_eq!(worktree_index_digest(&lead)?, before);

        handle.git(&["-C", &agent, "status", "--porcelain"]).await?;
        assert_ne!(worktree_index_digest(&lead)?, before);
        Ok(())
    })
    .await
}
