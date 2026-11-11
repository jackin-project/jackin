// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use jackin_console::backend::{DiffLine, DiffStatus, FolderProbe, UnreadFile};
use jackin_console::domain::account::DetectedKind;
use jackin_core::{EnvSnapshot, FileSystem, HostPath, ProcessRunner, SandboxName};
use jackin_session::{OnePasswordCli, SourceDesk};
use jackin_testkit::{Rig, TestResult, guard};
use serde_json::json;
use tokio_util::sync::CancellationToken;

fn desk(rig: &Rig) -> SourceDesk {
    let fs: Arc<dyn FileSystem> = Arc::clone(&rig.world.fs);
    let runner: Arc<dyn ProcessRunner> =
        Arc::<jackin_testkit::TestRunner>::clone(&rig.world.runner);
    SourceDesk::new(
        fs,
        EnvSnapshot::from_pairs(Vec::new()),
        OnePasswordCli::new(runner),
    )
}

async fn folder(
    rig: &Rig,
    name: &str,
    auth: Option<&str>,
) -> Result<HostPath, Box<dyn std::error::Error>> {
    let path = rig.home.as_path().join(name);
    tokio::fs::create_dir_all(&path).await?;
    if let Some(text) = auth {
        tokio::fs::write(path.join("auth.json"), text).await?;
    }
    Ok(HostPath::parse(&path)?)
}

#[tokio::test]
async fn a_folder_probe_names_the_credential_the_folder_holds() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let desk = desk(&rig);
        let login = tokio::fs::read_to_string(rig.home.as_path().join("auth.json")).await?;
        let cases = [
            (
                folder(&rig, "login", Some(&login)).await?,
                FolderProbe::Found(DetectedKind::CodexAuthJson),
            ),
            (
                folder(
                    &rig,
                    "apikey",
                    Some(&json!({"auth_mode": "apikey"}).to_string()),
                )
                .await?,
                FolderProbe::Found(DetectedKind::Unknown),
            ),
            (
                folder(&rig, "broken", Some("{")).await?,
                FolderProbe::Malformed,
            ),
            (
                folder(&rig, "empty", None).await?,
                FolderProbe::NoCredential,
            ),
            (
                HostPath::parse(&rig.home.as_path().join("auth.json"))?,
                FolderProbe::Unreadable,
            ),
            (
                HostPath::parse(&rig.home.as_path().join("gone"))?,
                FolderProbe::Missing,
            ),
        ];
        for (path, expected) in cases {
            assert_eq!(
                jackin_tui::folder_probe(&desk, &path).await,
                expected,
                "{path}"
            );
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn read_changes_maps_every_file_status_and_line_kind_and_keeps_the_unpushed_count()
-> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let project = handle.root().join("project");
        tokio::fs::write(project.join("README.md"), "readme\nsecond line\n").await?;
        tokio::fs::remove_file(project.join("src/main.rs")).await?;
        tokio::fs::write(project.join("notes.txt"), "a\nb\n").await?;
        let found = jackin_tui::read_changes(
            &rig.world.session,
            &handle.name(),
            3,
            &CancellationToken::new(),
        )
        .await?;
        assert_eq!(found.unpushed, 3);
        let by_path = |path: &str| found.files.iter().find(|f| f.path == path);
        let readme = by_path("README.md").ok_or("README.md")?;
        assert_eq!(readme.status, DiffStatus::Modified);
        assert_eq!(
            readme.hunks[0].lines,
            [DiffLine::context("readme"), DiffLine::add("second line")]
        );
        let main = by_path("src/main.rs").ok_or("src/main.rs")?;
        assert_eq!(main.status, DiffStatus::Deleted);
        assert_eq!(main.hunks[0].lines, [DiffLine::remove("fn main() {}")]);
        let notes = by_path("notes.txt").ok_or("notes.txt")?;
        assert_eq!(notes.status, DiffStatus::Added);
        assert_eq!(
            notes.hunks[0].lines,
            [DiffLine::add("a"), DiffLine::add("b")]
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn read_changes_of_a_sandbox_that_is_gone_is_an_error_text() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let gone = SandboxName::parse("jackin-gone")?;
        let refused =
            jackin_tui::read_changes(&rig.world.session, &gone, 0, &CancellationToken::new()).await;
        assert!(refused.is_err());
        let text = refused.err().unwrap_or_default();
        assert!(text.contains("jackin-gone"), "{text}");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn read_changes_carries_the_omitted_count_and_lists_an_unread_file_apart() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let project = handle.root().join("project");
        tokio::fs::write(project.join("big.bin"), vec![b'b'; 300 * 1024]).await?;
        for index in 0..501 {
            tokio::fs::write(project.join(format!("f{index:03}.txt")), "x\n").await?;
        }
        let found = jackin_tui::read_changes(
            &rig.world.session,
            &handle.name(),
            0,
            &CancellationToken::new(),
        )
        .await?;
        assert_eq!(found.omitted, 2);
        assert_eq!(
            found.unread,
            [UnreadFile {
                path: "big.bin".to_owned(),
                status: DiffStatus::Added,
            }]
        );
        assert_eq!(found.files.len(), 499);
        assert!(found.files.iter().all(|file| file.path != "big.bin"));
        assert!(!found.is_empty());
        Ok(())
    })
    .await
}
