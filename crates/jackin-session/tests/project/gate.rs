// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::path::Path;

use jackin_core::{FileMode, RepoPath, sandbox_layout};
use jackin_testkit::{Fault, HostEdit, RepoFixture, UploadKind};

use super::oracle::{arrived, git};
use super::support::{Rig, TestResult, global_config, guard, launch_id, mismatch, ready, tree_of};

async fn base() -> Result<RepoFixture, Box<dyn std::error::Error>> {
    let repo = RepoFixture::new().await?;
    repo.file("a.txt", b"alpha\n").await?;
    repo.file("tool.sh", b"#!/bin/sh\n").await?;
    repo.commit().await?;
    repo.file("a.txt", b"alpha edited\n").await?;
    repo.file("u.txt", b"untracked\n").await?;
    Ok(repo)
}

fn marker_present(handle: &jackin_testkit::SandboxHandle) -> bool {
    handle
        .file(&sandbox_layout::uploaded_marker(&launch_id(1)))
        .is_some()
}

#[tokio::test]
async fn a_host_edit_after_the_tree_upload_fails_with_a_mismatch_for_content_and_mode() -> TestResult
{
    guard(async {
        for (label, edit) in [
            ("a modified file", "a.txt"),
            ("an untracked file", "u.txt"),
            ("an execute bit", "tool.sh"),
        ] {
            let repo = base().await?;
            let target = repo.root().as_path().join(edit);
            let host_edit = if edit == "tool.sh" {
                HostEdit::Chmod {
                    path: jackin_core::HostPath::parse(&target)?,
                    mode: FileMode(0o755),
                }
            } else {
                HostEdit::Write {
                    path: jackin_core::HostPath::parse(&target)?,
                    bytes: b"changed during the upload\n".to_vec(),
                }
            };
            let rig = Rig::new(repo).await?;
            rig.world.gateway.inject(Fault::EditHostAfter {
                upload: UploadKind::Tree,
                edit: host_edit,
            });
            let found = mismatch(rig.launch(1).await?)?;
            assert_ne!(found.host, found.sandbox, "{label}");
            assert_eq!(
                found
                    .differing
                    .iter()
                    .map(RepoPath::as_str)
                    .collect::<Vec<_>>(),
                [edit],
                "{label}"
            );
            assert!(found.host_only.is_empty() && found.sandbox_only.is_empty());
            let handle = rig.handle(1).await?;
            assert!(!marker_present(&handle), "{label}");
            assert_eq!(
                tree_of(&handle.root().join("project"))?,
                Vec::<String>::new(),
                "{label}"
            );
        }

        let repo = base().await?;
        repo.file("a.txt", b"changed before the launch\n").await?;
        repo.file("u.txt", b"changed before the launch\n").await?;
        repo.chmod("tool.sh", 0o755).await?;
        let rig = Rig::new(repo).await?;
        ready(rig.launch(1).await?)?;
        let handle = rig.handle(1).await?;
        arrived(
            rig.repo.root().as_path(),
            &handle.root(),
            &[" M a.txt", " M tool.sh", "?? u.txt"],
        )
    })
    .await
}

const FILTER: &str = "[filter \"demo\"]\n\tclean = tr a-z A-Z\n";

#[tokio::test]
async fn a_filter_in_the_global_settings_is_seen_and_a_global_exclude_is_honoured() -> TestResult {
    guard(async {
        let filtered = RepoFixture::new().await?;
        filtered
            .file(".gitattributes", b"*.dat filter=demo\n")
            .await?;
        filtered.file("data.dat", b"lower case\n").await?;
        filtered.commit().await?;
        let rig = Rig::new(filtered).await?;
        global_config(&rig, FILTER)?;
        let found = mismatch(rig.launch(1).await?)?;
        assert_eq!(
            found
                .differing
                .iter()
                .map(RepoPath::as_str)
                .collect::<Vec<_>>(),
            ["data.dat"]
        );

        let plain = RepoFixture::new().await?;
        plain.file("data.dat", b"lower case\n").await?;
        plain.commit().await?;
        let rig = Rig::new(plain).await?;
        global_config(&rig, FILTER)?;
        ready(rig.launch(1).await?)?;

        let junk = RepoFixture::new().await?;
        junk.file("a.txt", b"alpha\n").await?;
        junk.commit().await?;
        junk.file(".DS_Store", b"finder junk\n").await?;
        let rig = Rig::new(junk).await?;
        let ignore = rig.world.home().as_path().join("ignore");
        std::fs::write(&ignore, ".DS_Store\n")?;
        global_config(
            &rig,
            &format!("[core]\n\texcludesFile = {}\n", ignore.display()),
        )?;
        ready(rig.launch(1).await?)?;
        let handle = rig.handle(1).await?;
        let project = handle.root().join("project");
        assert!(!project.join(".DS_Store").exists());
        assert!(project.join("a.txt").is_file());
        assert_eq!(git(&project, &["status", "--porcelain"])?, "");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn global_autocrlf_with_crlf_files_mismatches_and_repo_attributes_travel_with_the_tree()
-> TestResult {
    guard(async {
        let crlf = RepoFixture::new().await?;
        crlf.file("text.txt", b"one\ntwo\n").await?;
        crlf.commit().await?;
        crlf.file("text.txt", b"one\r\ntwo\r\nthree\r\n").await?;
        let rig = Rig::new(crlf).await?;
        global_config(&rig, "[core]\n\tautocrlf = true\n")?;
        let found = mismatch(rig.launch(1).await?)?;
        assert_eq!(found.autocrlf.as_deref(), Some("true"));
        assert_eq!(
            found
                .differing
                .iter()
                .map(RepoPath::as_str)
                .collect::<Vec<_>>(),
            ["text.txt"]
        );

        let attributed = RepoFixture::new().await?;
        attributed
            .file(".gitattributes", b"* text eol=crlf\n")
            .await?;
        attributed.file("text.txt", b"one\ntwo\n").await?;
        attributed.commit().await?;
        attributed
            .file("text.txt", b"one\r\ntwo\r\nthree\r\n")
            .await?;
        let rig = Rig::new(attributed).await?;
        global_config(&rig, "[core]\n\tautocrlf = true\n")?;
        ready(rig.launch(1).await?)
    })
    .await
}

fn snapshot(roots: &[&Path]) -> Result<BTreeMap<String, Vec<u8>>, Box<dyn std::error::Error>> {
    let mut found = BTreeMap::new();
    let mut pending: Vec<_> = roots.iter().map(|root| root.to_path_buf()).collect();
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                pending.push(entry.path());
            } else {
                let name = entry.path().display().to_string();
                if entry.file_name() == "index" {
                    found.insert(
                        format!("mtime {name}"),
                        format!("{:?}", entry.metadata()?.modified()?).into_bytes(),
                    );
                }
                found.insert(name, std::fs::read(entry.path())?);
            }
        }
    }
    Ok(found)
}

fn assert_unchanged(after: &BTreeMap<String, Vec<u8>>, before: &BTreeMap<String, Vec<u8>>) {
    let changed: Vec<&String> = after
        .keys()
        .chain(before.keys())
        .filter(|name| after.get(*name) != before.get(*name))
        .collect();
    assert!(changed.is_empty(), "the host changed: {changed:?}");
}

#[tokio::test]
async fn neither_a_success_nor_a_mismatch_changes_a_byte_of_the_host_repository() -> TestResult {
    guard(async {
        let main = RepoFixture::new().await?;
        main.file("a.txt", b"alpha\n").await?;
        main.file("data.dat", b"lower case\n").await?;
        main.file(".gitattributes", b"*.dat filter=demo\n").await?;
        main.commit().await?;
        let tree = main.linked_worktree("topic").await?;
        tree.file("a.txt", b"edited\n").await?;
        tree.file("u.txt", b"untracked\n").await?;
        tree.git(&["add", "a.txt"]).await?;
        let roots = [main.root().as_path(), tree.root().as_path()];
        let before = snapshot(&roots)?;

        let rig = Rig::new(tree.clone()).await?;
        global_config(&rig, FILTER)?;
        mismatch(rig.launch(1).await?)?;
        assert_unchanged(&snapshot(&roots)?, &before);
        let sandbox = rig.handle(1).await?.root().join("project");

        std::fs::remove_file(rig.world.home().as_path().join(".gitconfig"))?;
        ready(rig.launch(1).await?)?;
        assert_unchanged(&snapshot(&roots)?, &before);
        git(&sandbox, &["diff", "--cached", "--quiet"])?;
        Ok(())
    })
    .await
}
